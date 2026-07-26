---
title: review-gate non-blocking nits for 'android-hardware-back-button-navigates-history' (Gate 2 approve)
date: 2026-07-26
status: open
reviewOf: android-hardware-back-button-navigates-history
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-hardware-back-button-navigates-history' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify decision 2: the app module now takes its first androidx dependency (androidx.activity:activity 1.9.3) and BrowserActivity changes base class from android.app.Activity to androidx.activity.ComponentActivity. This narrows a previously explicit zero-androidx stance that three artifacts asserted. It is recorded and justified (no non-deprecated system-back API exists on a framework Activity below Android 13, and minSdk is 21), and the view layer stays framework-only, but it is a durable module-level dependency choice a human should ratify.
  (crates/werust-android/app/build.gradle.kts dependencies block; BrowserActivity.kt:27,53; docs/spikes/android-hardware-back-button-navigates-history/README.md decision 2)
- The stance this diff reverses is only forward-noted in the NEW spike README, not corrected at its source: docs/spikes/mobile-provider-injection-and-trust-indicator/decisions.md still says WebViewCompat.addDocumentStartJavaScript was rejected to avoid adding an androidx dependency to the minimal app module. That premise is now false, so a future task reading that doc inherits a stale rationale. Worth a one-line amendment there.
  (docs/spikes/mobile-provider-injection-and-trust-indicator/decisions.md:35 vs docs/spikes/android-hardware-back-button-navigates-history/README.md decision 2)
- Ratify decision 1: predictive back (android:enableOnBackInvokedCallback) is deliberately NOT opted into. The manifest is unchanged, so Android 13+ uses the legacy dispatch path. The reasoning (app-wide user-visible switch, orthogonal to this bug, one-line change later) is sound; confirm the human agrees it stays deferred rather than becoming a named follow-up task.
  (crates/werust-android/app/src/main/AndroidManifest.xml unchanged; spike README decision 1)
- Scope-of-cover note: the Gradle/Kotlin build is NOT in the Gate-1 verify command (cargo fmt/clippy/build/test only), and the release workflow android-apk leg runs only on tag or workflow_dispatch. So the base-class swap and the new dependency are not compiled by anything a merge runs; the only evidence they build is the recorded emulator run of record. That evidence does corroborate (androidx.activity 1.9.3 plus its transitive androidx artifacts are resolved in the local Gradle cache, and the named AVD exists), so this is an accepted-risk note, not a defect.
  (dorfl.json verify; .github/workflows/release.yml android-apk leg; spike README Run of record 2026-07-26)
- Ledger residue for the runner, not the code: the task file lands in work/tasks/done/ still carrying needsAnswers: true, and work/questions/task-android-hardware-back-button-navigates-history.md still holds five unanswered stuck questions from the earlier bounce. Per WORK-CONTRACT a done body and a live needsAnswers/stuck sidecar should not co-exist; the flag and sidecar want clearing as this lands.
  (work/tasks/done/android-hardware-back-button-navigates-history.md frontmatter; work/questions/task-android-hardware-back-button-navigates-history.md)
