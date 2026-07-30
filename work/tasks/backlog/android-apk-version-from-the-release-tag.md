---
title: "Derive the APK versionCode/versionName from the release tag so a signed release can actually be sequenced as an update"
slug: android-apk-version-from-the-release-tag
blockedBy: []
covers: []
---

## What to build

Origin: the `android-apk-signing` Gate-2 nit + the observation `work/notes/observations/android-release-apk-versioncode-and-signature-identity-2026-07-30.md`, ratified by the human on 2026-07-30.

`crates/werust-android/app/build.gradle.kts` hardcodes `versionCode = 1` and `versionName = "0.0.0"`. The release tag reaches only the RUST core, through the `WERUST_VERSION` env the `android-apk` job sets, so the APK MANIFEST still says 0.0.0 forever. Now that `android-apk-signing` has landed, that is the remaining half of the problem signing was meant to solve: Android sequences updates by a strictly increasing INTEGER `versionCode`, so every signed release would look like the same version to the device and could not be offered as an update. The user-visible `versionName` would also disagree with the version the ⋮ menu reports from the Rust core, which is exactly the kind of two-version-sources drift `versioned-gtk-app-id-and-stale-process-detection` just removed on desktop.

**Where to look:** `crates/werust-android/app/build.gradle.kts` (the `defaultConfig` block), and `.github/workflows/release.yml`'s `android-apk` job, which already computes `WERUST_VERSION` as the tag name on a tag and empty on a dry run. `crates/werust-core/build.rs` already resolves the same version for the Rust side (tag, else `git describe`), so the mapping should read the SAME source rather than minting a second one.

**Prescribed mapping (record it as a decision, with the alternative):** parse the semver triple from the tag and fold it into one monotonic integer, `major * 10000 + minor * 100 + patch` (so `v0.2.9` is `209`, `v0.3.0` is `300`, `v1.0.0` is `10000`). It is monotonic across every release this project will plausibly cut, and it is readable back by eye. The alternative worth naming and rejecting is a CI run number or a timestamp: monotonic too, but it destroys the correspondence between the APK's version and the release it came from. Whatever is chosen, `versionName` must be the SAME string the Rust core reports, so the menu and the Android system settings agree.

**The dev-build path matters:** a local `./gradlew :app:assembleDebug` with no tag must keep working. Fall back to the current `versionCode = 1` / a `git describe`-shaped `versionName` when the version is absent or is not a clean triple, and do not fail the build. A dev APK that will not install is a worse outcome than a dev APK with a placeholder version.

**Also record (from the same observation, no code needed):** the release APK keeps the debug `applicationId` but is signed with a different key, so a device holding a previously installed debug APK must uninstall before the signed one will install. That is a one-time transition, not a defect, but it belongs in `crates/werust-android/README.md` so the first signed release does not surprise anyone.

## Acceptance criteria

- [ ] A tagged release produces an APK whose `versionCode` is derived from the tag and INCREASES with every subsequent release, and whose `versionName` is the SAME version string the Rust core (and therefore the ⋮ menu) reports.
- [ ] The mapping reads the existing version source (`WERUST_VERSION` / the same resolution `build.rs` performs); no second version source is introduced.
- [ ] A local build with no tag still succeeds with a placeholder version and installs.
- [ ] The mapping is pinned by a test in the existing `crates/werust-core/tests/release_plumbing_shape.rs` style (parse the Gradle file and/or the workflow inside the pure-Rust `verify` gate, no SDK, no network).
- [ ] The decision (chosen mapping + the rejected alternative) is recorded in `docs/spikes/android-apk-signing/README.md` next to the signing decisions, since the two together are what make an updatable release.
- [ ] `crates/werust-android/README.md` notes the debug-to-signed transition requires an uninstall.

## Prompt

> Goal: make the Android APK's `versionCode`/`versionName` come from the release tag, so the signed APK that `android-apk-signing` now produces can actually be offered as an in-place UPDATE (Android sequences updates on a strictly increasing integer `versionCode`; the module currently hardcodes 1 / "0.0.0"). Read the version from the SAME source the Rust core already uses (`WERUST_VERSION` in the `android-apk` job, `build.rs`'s tag-else-`git describe`), never a second one, so the APK manifest and the ⋮ menu cannot disagree. Fold the semver triple into one monotonic integer (`major*10000 + minor*100 + patch`) and record that decision plus the rejected CI-run-number alternative. Keep local untagged dev builds working with a placeholder. Pin it with a shape test in the `release_plumbing_shape.rs` style (no SDK, no network).
