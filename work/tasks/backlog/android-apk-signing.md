---
title: "Sign the Android APK in CI (release keystore → signed APK attached to the GitHub Release)"
slug: android-apk-signing
spec: signed-multi-platform-builds
blockedBy: []
covers: []
---

## What to build

The existing CI (`android-apk` job in `.github/workflows/release.yml`) builds an **unsigned** `app-debug.apk`. A signed APK can be sideloaded without developer-mode warnings and is necessary for any distribution channel.

**This is NOT a humanOnly task** (an agent can implement the CI + Gradle + secrets wiring), but it IS blocked on the **human** generating the keystore and uploading it as GitHub secrets. The task prescription below covers both the human step and the code/CI step.

### Human step (do before dispatching)

Generate a release keystore and upload the secrets to the GitHub repository. Run:

```sh
keytool -genkey -v -keystore werust-release.jks \
  -alias werust -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass <storepass> -keypass <keypass> \
  -dname "CN=wighawag, OU=werust, O=wighawag, L=, ST=, C="
```

Then base64-encode the keystore and create four repository secrets:

```sh
base64 -w0 werust-release.jks   # → ANDROID_KEYSTORE_B64
```
Secrets to create on github.com/wighawag/werust → Settings → Secrets and variables → Actions:
- `ANDROID_KEYSTORE_B64` — base64 of the .jks above
- `ANDROID_KEYSTORE_PASSWORD` — the storepass used above
- `ANDROID_KEY_ALIAS` = `werust`
- `ANDROID_KEY_PASSWORD` — the keypass used above

Keep `werust-release.jks` somewhere safe outside the repo (it cannot be regenerated from secrets alone — the keytool command generates a NEW key each time).

### CI + Gradle step

- `.github/workflows/release.yml` `android-apk` job: add a step after the build that decodes the keystore from the secret, signs the APK with `jarsigner`, and zipaligns. Gated behind `${{ secrets.ANDROID_KEYSTORE_B64 != '' }}` — when the secret is absent (forks, dry-runs), skip signing and output the unsigned debug APK as before but rename it to `app-debug-unsigned.apk` to be honest.

- `crates/werust-android/app/build.gradle.kts`: add a `signingConfigs.release` block referencing the env vars `ANDROID_KEYSTORE_PATH` / `ANDROID_KEYSTORE_PASSWORD` / `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD`, gated behind `System.getenv("ANDROID_KEYSTORE_PATH") != null`. The CI step writes the decoded keystore to a temp file and sets `ANDROID_KEYSTORE_PATH` to it.

- Attach the signed APK as `app-release.apk` to the Release alongside the existing debug APK (both attached, distinguishable by name).

- The existing `release_plumbing_shape.rs` test already covers the job shape; extend it to assert that a signed APK is attached when the secrets are present, or the debug APK is honestly named `unsigned` when not.

## Acceptance criteria

- [ ] A signed `app-release.apk` is attached to every tagged release alongside the existing debug APK.
- [ ] The debug APK is named `app-debug-unsigned.apk` when secrets are absent (forks, dry-runs) — honest naming.
- [ ] The signing step is a no-op (gracefully skipped) when the secrets are not configured.
- [ ] The Gradle `signingConfigs.release` block does not affect local dev builds (gated behind env presence).
- [ ] The `release_plumbing_shape.rs` test covers both signed and unsigned paths.
- [ ] Network-isolated tests (the signing step does not touch the network).

## Human gate

The task prescription names the keytool command and the four secrets to create. The developer running this task cannot sign the APK without those secrets existing — the prescription must state this up front and stop if the env vars are absent (graceful no-op, not a hard error).

## Prompt

> Add APK signing to the android-apk CI job. Prescribed: keygen command and four GitHub secrets → Gradle signingConfigs.release + CI signing step gated behind secret presence → signed app-release.apk attached alongside the debug APK. The human must create the secrets first; the task implements the CI + Gradle wiring.
