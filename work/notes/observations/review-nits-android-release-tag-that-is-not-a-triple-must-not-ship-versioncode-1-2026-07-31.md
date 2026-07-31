---
title: review-gate non-blocking nits for 'android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The KDoc on versionCodeOf is now stale: it still says a pre-release tag or an operator's named build 'must take the dev placeholder instead of folding', which is exactly what no longer happens when WERUST_VERSION is injected. A reader of that function alone gets the pre-change rule. Worth a one-line amendment pointing at the werustVersionCode binding.
  (crates/werust-android/app/build.gradle.kts:150-158 (versionCodeOf KDoc) vs the new throw at the werustVersionCode binding)
- The dev-path guard lost teeth: a_local_untagged_android_build_keeps_working_on_a_placeholder now asserts only that the versionCode binding contains devPlaceholderVersionCode, but that identifier also appears interpolated inside the new GradleException message. If a future change deleted the tolerant fallback and always threw, this test would still pass, which is the one regression it exists to catch. Consider asserting the else-branch shape (e.g. the trailing fallback expression) rather than the bare identifier.
  (crates/werust-core/tests/release_plumbing_shape.rs, version_code_binding + the placeholder assertion; the message contains $devPlaceholderVersionCode)
- Ratify: the new failure is not confined to CI. Any WERUST_VERSION present in the Gradle daemon's environment now hard-fails a local build, and this same file already documents that a reused daemon can carry the WERUST_VERSION of an earlier shell until ./gradlew --stop. A developer can therefore hit the new error without setting anything in the current shell, and the message only advises tagging a clean triple. README decision 8 records the hand-set case but not the stale-daemon path; adding --stop to the message or the note would close it.
  (resolveWerustVersion KDoc daemon note vs the new GradleException text; docs/spikes/android-apk-signing/README.md decision 8, second consequence bullet)
- Ratify a small message inaccuracy: a triple that folds to 0 (v0.0.0) now fails with text saying only a clean major.minor.patch triple folds, which is misleading since v0.0.0 is one; it is rejected by the takeIf it > 0 guard. Near-zero real impact (nobody tags v0.0.0), noted only so the wording is a deliberate choice.
  (versionCodeOf(werustVersionName)?.takeIf { it > 0 } ?: run { ... } in build.gradle.kts)
