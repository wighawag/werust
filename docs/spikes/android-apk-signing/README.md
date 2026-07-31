# Android APK signing + versioning — setup + decisions

Tasks `android-apk-signing` and `android-apk-version-from-the-release-tag`. The `android-apk` release leg produces a **release-signed `app-release.apk`** alongside the debug APK, using a keystore that never enters the repo, and the APK's **`versionCode`/`versionName` now come from the release tag**. The two together are what make a release actually installable *as an update*: a signature gives the app a stable identity, and a strictly increasing integer `versionCode` is how Android sequences one build after another. This file is the stable home for (a) the one-off HUMAN setup and (b) the load-bearing decisions both wirings made, so a reviewer can ratify or reverse them.

The wiring itself lives in two places:

- `.github/workflows/release.yml`, the `android-apk` job (decode the keystore → build the signed APK → check it → attach it; and the `WERUST_VERSION` it already exported from the tag for the Rust core is now ALSO what the APK manifest reads).
- `crates/werust-android/app/build.gradle.kts`, the `signingConfigs.release` block (env-gated, so local dev builds are untouched) and the version block above the `android { }` extension.

Pinned by `crates/werust-core/tests/release_plumbing_shape.rs` (criteria 8 and 10), which parses both files inside the pure-Rust `verify` gate — no SDK, no network, no secret values read.

## Human setup (one-off, required before a tagged release can ship a signed APK)

Generate the release keystore **outside the repo** and keep it safe: it cannot be regenerated (a second `keytool -genkey` makes a DIFFERENT key, and Android will refuse to update an app signed with the old one).

```sh
keytool -genkey -v -keystore werust-release.jks \
  -alias werust -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass <storepass> -keypass <keypass> \
  -dname "CN=wighawag, OU=werust, O=wighawag, L=, ST=, C="
```

Base64 the keystore (one line, no wrapping):

```sh
base64 -w0 werust-release.jks
```

Create four repository secrets at github.com/wighawag/werust → Settings → Secrets and variables → Actions:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_B64` | the `base64 -w0` output above |
| `ANDROID_KEYSTORE_PASSWORD` | the `-storepass` used above |
| `ANDROID_KEY_ALIAS` | `werust` |
| `ANDROID_KEY_PASSWORD` | the `-keypass` used above |

Until those exist, the leg is a **graceful no-op**: it skips every signing step and attaches only the debug APK, renamed `app-debug-unsigned.apk`. Nothing fails.

To reproduce the signed build locally (with a throwaway key, never the real one):

```sh
export ANDROID_KEYSTORE_PATH=/tmp/throwaway.jks ANDROID_KEYSTORE_PASSWORD=... \
       ANDROID_KEY_ALIAS=werust ANDROID_KEY_PASSWORD=...
cd crates/werust-android && ./gradlew :app:assembleRelease
```

## Decisions

### 1. AGP's `signingConfigs.release` does the signing, NOT a hand-rolled `jarsigner` + `zipalign` step

**Chosen:** the CI step only materialises the keystore and sets `ANDROID_KEYSTORE_PATH`; the actual signing is AGP's, via `signingConfigs.release` + `:app:assembleRelease`.

**Why:** the task prescription named BOTH mechanisms (a `jarsigner` + `zipalign` CI step *and* a Gradle `signingConfigs.release` block); they do the same job, so only one can own it. AGP is the correct owner: its release pipeline zipaligns and signs with **apksigner** (v1 JAR + v2 APK Signature Scheme — verified locally: `Verified using v1 scheme: true`, `v2 scheme: true`), whereas `jarsigner` can only produce the v1 signature, which Android 11+ rejects on its own for a `targetSdk` 30+ app. Signing inside the build also means a bad password fails the BUILD rather than silently shipping an unsigned artifact.

**Alternatives considered:** *hand-rolled `jarsigner` + `zipalign`* (rejected: v1-only, and re-implements what AGP already does correctly); *`apksigner sign` as a post-build CI step* (rejected: same result as AGP but keeps the signing identity outside the build graph, so `assembleRelease` alone would produce an unsigned APK a human could mistake for the shipped one).

**What it touches:** the `android-apk` job only. No other leg, flag, or task.

### 2. The signing steps gate on a job-level PRESENCE FLAG, not on `secrets` directly

**Chosen:** the job env sets `ANDROID_SIGNING_CONFIGURED: ${{ secrets.ANDROID_KEYSTORE_B64 != '' && '1' || '' }}` and each signing step gates on `env.ANDROID_SIGNING_CONFIGURED`.

**Why:** the task prescribed `if: ${{ secrets.ANDROID_KEYSTORE_B64 != '' }}`, but GitHub does **not** expose the `secrets` context to a step's `if:` (the allowed contexts there are github/needs/strategy/matrix/job/runner/env/vars/steps/inputs), while a job-level `env:` *can* read `secrets`. The flag is the standard workaround. It deliberately carries **presence only**: the base64 keystore and the two passwords stay in the single step that needs them instead of the job-wide env every other step (cargo, Gradle) inherits.

**Coherence check:** `ANDROID_SIGNING_CONFIGURED` is a new name. It does not collide with the repo's existing CI env vocabulary (`WERUST_VERSION`, `WERUST_RPC_URL`, both *values* injected into the compiled code); it is a CI-local control flag, at the same layer as the `startsWith(github.ref, 'refs/tags/')` tag/dry-run branch that already shapes this workflow.

**What it touches:** the `android-apk` job only. The shape test asserts no step `if:` in that job reads `secrets.`, so the mistake cannot creep back.

### 3. The no-secrets fallback keeps the task's `app-debug-unsigned.apk` name — with a caveat worth knowing

**Chosen:** when the secrets are absent, the debug APK is renamed `app-debug-unsigned.apk` before it is attached, exactly as the acceptance criterion asks.

**Caveat (recorded rather than silently "fixed"):** strictly, an Android debug APK is **not** unsigned — AGP signs it with the auto-generated debug keystore, and Android cannot install a truly unsigned APK at all. "Unsigned" here means *not signed with the project's release key*, which is also how the rest of this repo already words it (`crates/werust-android/README.md`, the done task `mobile-android-shell-and-static-lib`, and this module's Gradle header all said "unsigned debug APK"). The name is therefore consistent with the project's existing language, and honest about the thing that matters to someone downloading it (no release identity), but a reader who reads "unsigned" in the strict APK-signing sense will be slightly misled. Reversing it is a one-line change to the rename step plus its shape assertion.

**Alternative considered:** keep the name `app-debug.apk` on both paths (strictly accurate, since it IS debug-signed) and let the presence/absence of `app-release.apk` carry the signal. Rejected here because it drops an explicit acceptance criterion, which is the human's call, not the build's.

### 4. The `versionCode` is the semver triple folded as `major * 10000 + minor * 100 + patch`

**Chosen:** the release tag's triple is folded into ONE integer — `v0.2.9` → `209`, `v0.3.0` → `300`, `v1.0.0` → `10000` — and `versionName` is the SAME resolved string the Rust core (and therefore the ⋮ menu) reports.

**Why:** Android sequences updates on a strictly increasing INTEGER `versionCode`, so the released version has to survive being squashed into one number. This fold is monotonic across every release this project will plausibly cut, and it is readable back by eye (a `versionCode` a user or a store page shows can be mapped to the tag it came from without a lookup table). Keeping `versionName` byte-identical to `werust_core::version()` is the same one-derivation rule the desktop side already follows: the version in Android's system settings and the version in the ⋮ menu are the same string or they are a bug.

**Alternatives considered:** a *CI run number* (`github.run_number`) or a *timestamp* — monotonic too, and immune to the ≤ 99 limit below, but rejected because they destroy the correspondence between the APK's version and the release it came from (a `versionCode` of `1247` tells nobody which tag they are running, and a re-run of the same tag would produce a different code for identical bits). Also rejected: a hand-bumped literal in the Gradle file (the status quo — it is a SECOND version source, and it is exactly what shipped `0.0.0` forever).

**What it touches:** the `android-apk` leg and the APK manifest only. It reads `WERUST_VERSION`, the variable the leg already exported for the Rust core, so it adds no new CI input, no new flag and no new secret. It is HARD to reverse in one direction only: a released `versionCode` can never be lowered for an installed app, so a future change may only ever widen the mapping upward.

### 5. A triple that would COLLIDE under the fold fails the build instead of shipping

**Chosen:** `major * 10000 + minor * 100 + patch` reserves two decimal digits each for minor and patch, so a version with minor or patch above 99 (`0.100.0` folds to the same `10000` as `1.0.0`) throws a `GradleException` naming the version and this file.

**Why:** the alternative failure modes are all silent and all worse. Falling back to the dev placeholder would attach a *release* APK carrying `versionCode = 1`, i.e. exactly the bug this task removed, discovered only when a user's update did not appear. Wrapping around would make a NEWER release look OLDER, which Android refuses to install at all. A loud red release job is recoverable in minutes (widen the multipliers — upward only, per decision 4); a silently wrong `versionCode` on a published APK is not recoverable at all.

**Alternatives considered:** *widen the mapping now* (e.g. `major * 1_000_000 + minor * 1_000 + patch`, room for 999) — rejected as a premature choice that makes today's codes less readable to buy headroom this project (at `v0.2.9`, minor bumped 3 times in its lifetime) will not use; the guard makes the day it IS needed a build error rather than a surprise. *Clamping* to 99 — rejected: it makes two different releases share a `versionCode`.

**What it touches:** the Gradle build of the app module. It is a new ERROR path, but it can only fire on a version that is already unshippable, and never on a dev build (a `git describe` version is not a clean triple, so it takes the placeholder path long before this check).

### 6. The version resolution is MIRRORED in Kotlin, not read back out of the compiled core

**Chosen:** `app/build.gradle.kts` resolves the version itself, from the same sources in the same order as `crates/werust-core/build.rs` — `WERUST_VERSION`, else `git describe --tags --always`, else the `[workspace.package] version` in the workspace `Cargo.toml` — and applies the same leading-`v` strip.

**Why:** the macOS packaging leg reads the version back out of the compiled binary (`cargo run -p werust-core --example print_version`, decision in `docs/spikes/macos-release-packaging-leg/README.md`) precisely to avoid a second source, so this is the divergence worth recording. `versionCode`/`versionName` are set in `defaultConfig`, i.e. at Gradle CONFIGURATION time, and a host `cargo run` there would compile the core on the host — a third compile, on every single Gradle invocation including `./gradlew tasks` — before Android's own cross-compiles start. The mirrored resolution reads the SAME INPUTS (not a version of its own), so the two cannot disagree about a released build: on a tag both read the identical `WERUST_VERSION` the job exports.

**Alternatives considered:** *invoke the `print_version` example from Gradle* (the macOS pattern) — rejected for the configuration-time cost above; *pass the version in as a Gradle property from CI* (`-PwerustVersion=…`) — rejected because it would mint a CI-side version input that a local build does not have, splitting the sources again.

**What it touches:** the app module's Gradle script. The RISK it carries is drift if `build.rs`'s precedence ever changes; the mitigation is that both sides are named in each other's comments and the shape test asserts the Gradle side reads `WERUST_VERSION` and the same `git describe --tags --always`.

### 7. The resolved version is handed to the cargo cross-compile too, not just to the manifest

**Chosen:** `cargoBuildRustCore` takes the resolved version as a declared Gradle `@Input` and exports it to cargo as `WERUST_VERSION`, so the manifest and the Rust core the APK packages are stamped from the SAME evaluation.

**Why:** found while verifying this task against a real build. `defaultConfig` is evaluated on every configuration, but the cross-compile is an UP-TO-DATE-checked task whose inputs did not mention the version, so a local rebuild after the version changed re-stamped the manifest and repackaged the PREVIOUSLY compiled `libwerust_mobile.so`: the APK read `versionCode=300 / versionName=0.3.0` while the core inside it still reported `0.2.9-91-g…`. That is precisely the manifest-vs-⋮-menu disagreement this task exists to make impossible, so the version had to become an input. Exporting it (rather than letting the cargo child process inherit the Gradle DAEMON's environment) also removes the reused-daemon variable: whatever this script resolved is what the core is compiled with.

**Alternatives considered:** *declare the input but let cargo read the env itself* — rejected: it fixes the up-to-date check but leaves the daemon's (possibly stale) environment deciding what the core reports, so the two could still differ. *Mark the task `outputs.upToDateWhen { false }`* — rejected: it would re-run the two cross-compiles on every build, which is the expensive step of this module.

**What it touches:** the app module's Gradle build only. On CI the value is byte-identical to what the job already exported (`WERUST_VERSION` from the tag), so the release path is unchanged; the difference shows only in incremental LOCAL builds.

### 8. A RELEASE tag that folds to no `versionCode` FAILS the build; sequencing pre-release tags is deliberately not designed

**Chosen:** the placeholder fallback is keyed on **dev-versus-release**, not on the shape of the version string. When `WERUST_VERSION` is INJECTED (CI is cutting a release from a tag) and it does not fold to a valid `versionCode`, the build throws a `GradleException` naming the version, why it cannot be sequenced and the accepted shape. When it is absent (a local build), the tolerant placeholder is unchanged.

**Why:** `.github/workflows/release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is an acceptable release tag today. On it, `versionName` resolves correctly to `0.3.0-rc1` but `versionCodeOf()` returns null, and the fallback then attached a **signed release APK carrying `versionCode = 1`**: unsequenceable, un-updatable, indistinguishable from every placeholder dev build — precisely the bug decision 4 exists to remove, reachable through the front door, and the exact opposite of what decision 5 already does for an out-of-range component. Decision 5's own argument applies unchanged: a loud red release job is recoverable in minutes; a published APK with a wrong `versionCode` is not recoverable at all. Keying on the injected variable rather than on the string is what keeps the two cases apart — the other two version sources (`git describe`, the workspace Cargo version) are dev sources by construction, so their non-triple shapes must stay tolerated.

**Alternatives considered:** *fail on any non-triple, dev builds included* — rejected: it breaks every local `./gradlew :app:assembleDebug` in a checkout (a `git describe` version is never a clean triple), and a dev APK that will not build is worse than one with a placeholder version. *Keep the tolerant fallback and catch it in review* — rejected: the artifact is produced by CI on a tag push, so there is no review between the tag and the published APK.

**The pre-release mapping is deliberately NOT designed here.** The alternative to rejecting `v0.3.0-rc1` is a mapping that can SEQUENCE it — reserving a digit for the pre-release, the way many Android projects fold `rc1` into a code just below the final release. That is a **product** decision about whether this project ever cuts pre-release tags at all, not a build detail, so it is abstained from rather than invented. If it is ever wanted, these are the things that would have to be decided:

- **Whether pre-release tags are cut at all**, and whether they publish a *release* APK to users or only a workflow artifact. If the latter, the correct fix is the trigger/publication rule, not the mapping.
- **The ordering rule**, i.e. that `0.3.0-rc1 < 0.3.0`, which means reserving digits *below* each release rather than above (e.g. `patch * 100 + preRelease`, with the final release taking the top of the band). Reserving them above would make an `rc` outrank the release it precedes.
- **How much of the mapping's headroom to spend on it**, which interacts with decision 5's ≤ 99 limit and with decision 4's one-way constraint: a released `versionCode` can never be lowered, so widening for pre-releases can only ever shift codes *upward*, and every already-published code must keep its meaning.
- **The pre-release grammar accepted** (`-rc1` only, or the full semver pre-release field `-alpha.2`, `-beta`), since the fold has to be total over whatever the tag trigger admits — or the same loud failure returns for the shapes it does not cover.

**What it touches:** the app module's Gradle build and the `android-apk` release leg's tag path. It is a new ERROR path, but it can only fire on a tag whose APK would be unshippable anyway, and never on a dev build (no injected version → the placeholder path, exactly as before). It does not change the trigger in `release.yml`: a `v*` pre-release tag still starts the workflow, and the desktop/iOS legs still build — only the Android leg refuses, loudly, at the point where an unsequenceable APK would be minted.

Two consequences worth stating plainly, because both are user-visible and neither is obvious from the rule:

- **An EMPTY `WERUST_VERSION` is the dev path, not a release.** The workflow sets `WERUST_VERSION: ${{ startsWith(github.ref, 'refs/tags/') && github.ref_name || '' }}`, so on the `workflow_dispatch` dry run the variable is present but empty. Presence is therefore tested after trimming to non-empty, and the dry run keeps falling through to `git describe` + the placeholder exactly as before. A dry run that failed because it is not a tag would be absurd.
- **A hand-set `WERUST_VERSION` that is not a triple now fails a LOCAL build too** (`WERUST_VERSION=vendor-build ./gradlew :app:assembleDebug` used to produce the placeholder; it now fails with the same message). That is the rule doing its job rather than a side effect: the variable's meaning is "this is the released version", so an operator who sets it by hand is asking for exactly the artifact the guard protects. The dev path is *not setting it*, which is what a plain `./gradlew :app:assembleDebug` does.

## What was verified locally

Run against the real SDK/NDK before landing (not part of the pure-Rust gate):

- `ANDROID_KEYSTORE_* set → ./gradlew :app:assembleRelease` produces `app-release.apk`; `apksigner verify --verbose` reports `Verifies` with v1 + v2 true; `check-apk-abis.sh` passes on it (both floor ABIs present).
- **No** signing env → the same command succeeds and emits `app-release-unsigned.apk` instead (AGP's own honest name). This is why the CI check `test -f app-release.apk` is real evidence the signing config applied.
- `./gradlew :app:assembleDebug` is unchanged (debug APK builds, both ABIs present).

For the version mapping (task `android-apk-version-from-the-release-tag`), against the real SDK + Gradle 9.4.1, reading the configured `defaultConfig` back out of the evaluated project:

| environment | `versionCode` | `versionName` |
| --- | --- | --- |
| `WERUST_VERSION=v0.3.0` (the tag path) | `300` | `0.3.0` |
| `WERUST_VERSION=v0.2.9` | `209` | `0.2.9` |
| `WERUST_VERSION=1.0.0` | `10000` | `1.0.0` |
| unset, in a checkout (the local dev path) | `1` (placeholder) | `0.2.9-91-ga94c477` |
| unset, with git unavailable (`GIT_DIR=/nonexistent`) | `209` | `0.2.9` (the workspace Cargo version, the same last resort `build.rs` uses) |
| `WERUST_VERSION=0.100.0` | build FAILS loudly (decision 5) | — |

Re-run after decision 8 landed (Gradle 9.4.1, reading `defaultConfig` back out of the evaluated project), which is where the last three rows CHANGED — an injected version that folds to nothing no longer takes the placeholder:

| environment | `versionCode` | `versionName` |
| --- | --- | --- |
| `WERUST_VERSION=v0.3.0` / `v0.2.9` / `1.0.0` | `300` / `209` / `10000` | `0.3.0` / `0.2.9` / `1.0.0` (unchanged) |
| `WERUST_VERSION=v0.3.0-rc1` (a pre-release tag) | build FAILS loudly (decision 8) | — |
| `WERUST_VERSION=vendor-build` (an operator's named build) | build FAILS loudly (decision 8; used to be the placeholder) | — |
| `WERUST_VERSION=` (empty — the CI dry-run path) | `1` (placeholder) | `0.2.9-106-gf15aca7` |
| unset (the local dev path) | `1` (placeholder) | `0.2.9-106-gf15aca7` |

And end-to-end, reading the fields back out of a REAL `./gradlew :app:assembleDebug` APK with `aapt2 dump badging` (plus `strings` on the `libwerust_mobile.so` it packages, to check the core agrees):

| build | APK manifest | packaged core |
| --- | --- | --- |
| `WERUST_VERSION=v0.3.0` | `versionCode='300' versionName='0.3.0'` | `0.3.0` |
| untagged local build | `versionCode='1' versionName='0.2.9-91-ga94c477'` | `0.2.9-91-ga94c477` |
