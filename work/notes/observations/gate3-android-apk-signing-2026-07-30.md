---
title: "Gate-3 conductor review: android-apk-signing (APPROVE, with a human step outstanding)"
date: 2026-07-30
status: open
reviewOf: android-apk-signing
verdict: approve
---

## Verdict: APPROVE

Merged as `a3ef728` on `origin/main` (drive-tasks, `--allow-backlog --review --merge`, `etherplay/opus-5`). Gate-1 (repo `verify`) and Gate-2 (review gate, 5 non-blocking nits) both green; the 5 new shape tests re-run locally green (20 in `release_plumbing_shape`).

The CODE side is done. The task cannot DELIVER a signed APK until the human creates the keystore and the four repository secrets, which is by design (the task said so up front) and is the only thing standing between this and a signed release.

## Acceptance criteria, ticked against the merged tree

- [x] **A signed `app-release.apk` is attached to every tagged release alongside the debug APK.** New `assembleRelease` + attach steps in the `android-apk` job; pinned by `android_leg_builds_and_attaches_a_signed_release_apk_when_the_keystore_secret_is_configured`.
- [x] **The debug APK is named `app-debug-unsigned.apk` when secrets are absent.** Rename step on the no-secrets path; pinned by `android_leg_names_the_debug_apk_unsigned_when_the_signing_secrets_are_absent`.
- [x] **The signing step is a graceful no-op when the secrets are not configured.** Gated on a job-level `ANDROID_SIGNING_CONFIGURED` presence FLAG, not the material, because GitHub does not expose `secrets` to a step `if:`. Pinned by `android_leg_gates_signing_on_an_env_presence_flag_not_the_secrets_context`, which also asserts the flag carries no key material into every step's env.
- [x] **`signingConfigs.release` does not affect local dev builds.** The config is only CREATED when `ANDROID_KEYSTORE_PATH` is present; absent it, the release build type gets a null `signingConfig` and AGP emits `app-release-unsigned.apk`, so an unsigned build cannot masquerade as the signed artifact. Pinned by `android_app_gradle_declares_an_env_gated_release_signing_config`.
- [x] **`release_plumbing_shape.rs` covers both paths.** Five new tests, both branches.
- [x] **Network-isolated.** `the_keystore_handling_steps_touch_no_network` asserts it, and the signature proof uses the SDK's own `apksigner verify` offline.

Two judgement calls I checked and agree with: signing via AGP rather than `jarsigner` (jarsigner only produces the v1 signature, which Android 11+ rejects alone for a targetSdk-30+ app), and the keystore being decoded into `RUNNER_TEMP` rather than the workspace, so it cannot be swept into an artifact. The `.gitignore` addition for `*.jks` / `*.keystore` is off-path but is a 7-line safety net for the very `keytool` run the task asks a human to perform in the working tree; I accept it.

## THE HUMAN STEP (nothing signs until this is done)

Create the release keystore and four repository secrets: `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. The exact `keytool` command is in `docs/spikes/android-apk-signing/README.md`. The key CANNOT be regenerated later without breaking update-in-place for everyone who installed a previous signed build, so back it up outside the repo.

## Nit triage (5 non-blocking findings)

**The one I would act on: `versionCode` / `versionName`.** `build.gradle.kts` hardcodes `versionCode = 1`, `versionName = "0.0.0"`, so the first signed release attaches an APK reporting 0.0.0 that can never be SEQUENCED as an update. That partly defeats the reason signing was wanted (updatable releases). The tag version reaches only the Rust core through `WERUST_VERSION`, not the APK manifest. The agent filed `android-release-apk-versioncode-and-signature-identity-2026-07-30.md`; my recommendation is a named follow-up task, since an observation alone will not get it fixed before the first signed release. Same note records the related consequence: the release APK keeps the debug `applicationId` but is signed with a different key, so a device holding the debug APK must uninstall before the signed one installs.

**Ratify: R8/minification is OFF on the release build** (`isMinifyEnabled = false`). Deliberate (the signed APK is code-identical to the tested debug APK, one fewer variable between what CI tests and what users install) but it means no shrinking or obfuscation on the shipped artifact, and it is documented only in a Gradle comment.

**Ratify: naming asymmetry.** On the signed path the debug-key APK is attached to the public Release as plain `app-debug.apk`, while the same class of artifact is `app-debug-unsigned.apk` on the no-secrets path. The task prescribed exactly this, so not a defect, but one naming rule for both paths would be more coherent.

**Worth a one-line test:** nothing pins the line that WIRES the signing config into the release build type, so deleting `signingConfig = signingConfigs.findByName("release")` would keep the whole gate green and silently revert to unsigned builds, caught only at real release time.

**Know this if the first tagged run misbehaves:** `ANDROID_KEYSTORE_PATH` is exported via `GITHUB_ENV` after the first `gradlew` invocation already started a Gradle daemon. Linux Gradle normally applies the client environment per build, and the failure mode is loud (`test -f app-release.apk`), but `--no-daemon` on the release build is the known fix.
