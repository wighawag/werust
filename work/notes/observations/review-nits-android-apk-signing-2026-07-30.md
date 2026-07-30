---
title: review-gate non-blocking nits for 'android-apk-signing' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: android-apk-signing
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-apk-signing' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the shipped release build turns R8/minification OFF (isMinifyEnabled = false), a user-visible default the task never specified. Intentional (keeps the signed APK byte-identical in code to the tested debug APK) but it means no shrinking/obfuscation on the artifact users install, and it is documented only in a Gradle comment, not in the spike Decisions block.
  (crates/werust-android/app/build.gradle.kts, buildTypes getByName(release) { isMinifyEnabled = false })
- Ratify the naming asymmetry: on the signed path the debug-key APK is attached to the public Release as plain app-debug.apk, while the SAME class of artifact is attached as app-debug-unsigned.apk on the no-secrets path. A release page therefore shows a debug-signed APK without the honest suffix. The task prescribed exactly this, so it is not a defect, but one name for both paths (or renaming on both) would be more coherent.
  (.github/workflows/release.yml, Attach the APKs to the Release step uploads app-release.apk + app-debug.apk on the signed path; the rename step only runs when the flag is unset)
- Should the versionCode/versionName gap get a NAMED follow-up task rather than only an observation note? The first signed release will attach an app-release.apk reporting versionCode 1 / versionName 0.0.0, which cannot be sequenced as an update, and devices holding the debug APK must uninstall first (same applicationId, different key). Pre-existing, out of this task's criteria, but only now user-facing.
  (work/notes/observations/android-release-apk-versioncode-and-signature-identity-2026-07-30.md; build.gradle.kts hardcodes versionCode = 1, versionName = 0.0.0)
- Add one text assertion that the release BUILD TYPE consumes the signing config? Today the Gradle test pins signingConfigs/create(release)/the four env vars, but nothing pins the line that wires them into the release build type, so deleting signingConfig = signingConfigs.findByName(release) keeps the whole gate green and silently reverts to an unsigned build; only the tag-time CI check test -f app-release.apk would catch it, i.e. during a real release.
  (crates/werust-core/tests/release_plumbing_shape.rs, android_app_gradle_declares_an_env_gated_release_signing_config)
- Signing correctness depends on the SECOND gradlew invocation seeing ANDROID_KEYSTORE_PATH, which was exported via GITHUB_ENV after the first invocation already started a Gradle daemon. Gradle normally applies the client environment per build on Linux, and the failure mode here is loud (test -f app-release.apk fails), but if the first real tagged run misbehaves, --no-daemon on the release build is the fix worth knowing about.
  (.github/workflows/release.yml: assembleDebug step, then Decode the release keystore writes GITHUB_ENV, then assembleRelease in a new shell)

## Human triage (2026-07-30, via drive-tasks)

- **versionCode/versionName: TASKED.** Filed as `work/tasks/backlog/android-apk-version-from-the-release-tag.md`, which derives both from the release tag off the SAME version source the Rust core uses, so the signed APK can actually be sequenced as an update. It also carries the debug-to-signed uninstall transition note for the Android README.
- The human confirmed the keystore setup in `docs/spikes/android-apk-signing/README.md` is sufficient to follow as-is.
- Still open, not ruled on: R8/minification off on the release build, the `app-debug.apk` vs `app-debug-unsigned.apk` naming asymmetry across the two paths, the missing pin on the line wiring `signingConfig` into the release build type, and the Gradle-daemon/`GITHUB_ENV` ordering caveat.
