# Android APK signing — setup + decisions

Task `android-apk-signing`. The `android-apk` release leg now produces a **release-signed `app-release.apk`** alongside the debug APK, using a keystore that never enters the repo. This file is the stable home for (a) the one-off HUMAN setup and (b) the load-bearing decisions the wiring made, so a reviewer can ratify or reverse them.

The wiring itself lives in two places:

- `.github/workflows/release.yml`, the `android-apk` job (decode the keystore → build the signed APK → check it → attach it).
- `crates/werust-android/app/build.gradle.kts`, the `signingConfigs.release` block (env-gated, so local dev builds are untouched).

Pinned by `crates/werust-core/tests/release_plumbing_shape.rs` (criterion 8), which parses both files inside the pure-Rust `verify` gate — no SDK, no network, no secret values read.

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

## What was verified locally

Run against the real SDK/NDK before landing (not part of the pure-Rust gate):

- `ANDROID_KEYSTORE_* set → ./gradlew :app:assembleRelease` produces `app-release.apk`; `apksigner verify --verbose` reports `Verifies` with v1 + v2 true; `check-apk-abis.sh` passes on it (both floor ABIs present).
- **No** signing env → the same command succeeds and emits `app-release-unsigned.apk` instead (AGP's own honest name). This is why the CI check `test -f app-release.apk` is real evidence the signing config applied.
- `./gradlew :app:assembleDebug` is unchanged (debug APK builds, both ABIs present).
