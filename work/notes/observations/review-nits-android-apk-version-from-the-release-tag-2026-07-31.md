---
title: review-gate non-blocking nits for 'android-apk-version-from-the-release-tag' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: android-apk-version-from-the-release-tag
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-apk-version-from-the-release-tag' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: a RELEASE tag that is not a clean triple (e.g. v0.3.0-rc1, which the workflow trigger tags: [v*] accepts) resolves versionName=0.3.0-rc1 but silently takes the dev placeholder versionCode=1 — the exact unsequenceable-release bug this task removed. Should the release path fail loudly (as decision 5 does for >99) instead of falling back, or is 'we never cut pre-release tags' the accepted policy?
  (build.gradle.kts versionCodeOf() returns null for a non-triple; werustVersionCode falls back to devPlaceholderVersionCode. Decision 5 in docs/spikes/android-apk-signing/README.md argues a placeholder on a release APK is the worst outcome, yet that is what a non-triple tag gets. Existing tags are all clean triples, so no impact today.)
- Ratify decision 5: a new build-FAILING error path (GradleException when minor or patch > 99) fires at Gradle CONFIGURATION time, so it would break every invocation of the module (even ./gradlew tasks or assembleDebug), not just a release build.
  (build.gradle.kts versionCodeOf(); recorded as decision 5. Unreachable at 0.2.9, and only reachable via a version already unshippable under the fold.)
- Ratify decision 6: the version resolution is MIRRORED in Kotlin rather than read back out of the compiled core (the macOS leg's print_version pattern). Nothing fails if build.rs's precedence later changes — the shape test only asserts the Gradle text mentions WERUST_VERSION and git describe --tags --always, not that the two chains agree.
  (Rationale is recorded (configuration-time host compile on every Gradle invocation); inputs are shared, so no second SOURCE is minted, only a second implementation.)
- Ratify decision 7 (a cross-task interaction with mobile-android-shell-and-static-lib): cargoBuildRustCore now takes the resolved version as an @Input and exports WERUST_VERSION into the cargo environment, so a changed version re-runs the two cross-compiles that were previously UP-TO-DATE.
  (build.gradle.kts:344,364,395. Found on a real build where the manifest said 0.3.0 while the packaged .so still reported 0.2.9-91-g...; CI value is byte-identical so the release path is unchanged.)
- Nit: the_android_leg_mints_no_second_version_source_in_the_workflow forbids the substrings versionCode / versionName / git describe anywhere inside the android-apk job, so a future explanatory COMMENT in that job would fail the gate for a non-defect. Worth narrowing to run/with/env lines?
  (crates/werust-core/tests/release_plumbing_shape.rs, criterion 10.)
- Minor deviation from the acceptance wording: a no-git source tree does NOT take the placeholder — it resolves the workspace Cargo version and folds it, so an untagged tarball build gets versionCode 209, the same code the real v0.2.9 release will carry.
  (cargoWorkspaceVersion() is third in the chain; the local-verification table in docs/spikes/android-apk-signing/README.md records this row. It matches build.rs's last resort and such builds are debug-signed (so an uninstall is needed anyway), so impact is low.)
