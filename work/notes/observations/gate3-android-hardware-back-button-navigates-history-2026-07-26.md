---
title: "Gate-3 conductor review: android-hardware-back-button-navigates-history (APPROVE) + ledger residue cleanup"
date: 2026-07-26
status: approved
reviewOf: android-hardware-back-button-navigates-history
gate: gate-3-conductor
mergedCommit: eb0b6f6
---

## Verdict: APPROVE (after a valid Gate-2 BLOCK was fixed via requeue)

Conductor Gate-3 pass. This task took several recovery cycles (detailed below); the key one was a GENUINE Gate-2 block (a vacuous test) that the re-drive fixed. I scrutinised whether that fix actually landed and re-ran the shape tests locally.

## Done-move + landing

- `work/tasks/backlog/android-hardware-back-button-navigates-history.md` -> `done/` on origin/main (merged, feat commit `eb0b6f6`).
- Files: `BrowserActivity.kt` (+62: `systemBackCallback` + registration), `app/build.gradle.kts` (+19: androidx.activity dep), `rust/tests/system_back_wiring_shape.rs` (+215: the shape guard, now with a brace-matched extractor), capability matrix (+34), spike README, an edge-swipe observation.

## Acceptance criteria (ticked, re-verified locally)

- [x] The Android system/hardware Back navigates BACK one page when there is history, instead of exiting. `systemBackCallback = OnBackPressedCallback(false)` with `handleOnBackPressed { driveCore { core.goBack() } }`, registered via `onBackPressedDispatcher.addCallback`.
- [x] When there is NO back history, the callback is disabled so the default Back exits the Activity. `OnBackPressedCallback(false)` starts disabled; `isEnabled = chrome.canGoBack`.
- [x] The system Back drives the core through the SAME off-UI-thread `driveCore { core.goBack() }` as the on-screen button - the ANR fix is NOT regressed (no UI-thread blocking core call). This is exactly the property the Gate-2 block found was NOT actually guarded; see below.
- [x] The system-Back enabled state is in lockstep with `chrome.canGoBack` (`systemBackCallback.isEnabled = chrome.canGoBack` set wherever `backButton.isEnabled` is, line 390).
- [x] Non-deprecated `OnBackPressedDispatcher`/`OnBackPressedCallback` (test `the_deprecated_on_back_pressed_override_is_not_used` green); predictive-back (Android 13+) opt-in deliberately deferred + recorded.
- [x] Tracked per the parity guard (a new `system-back-navigates-history` capability row). 6 shape tests green locally.

## The Gate-2 BLOCK was real and is FIXED (verified)

Gate-2 initially BLOCKED (correctly) that the criterion-3 shape guard was VACUOUS: the old `kotlin_fun_body` extractor picked its terminator by KIND not POSITION, extracting a ~9700-char body that swallowed all of `onCreate`, so the "handler drives driveCore" assert passed on the ON-SCREEN button line inside onCreate, not the real `handleOnBackPressed` (the reviewer proved it by mutation: emptying the handler kept all asserts green). I requeued with a precise fix instruction quoting the finding; the re-drive REPLACED the extractor with `kotlin_block_body`, a proper BRACE-MATCHED parser (tracks depth, skips comments/strings/`"""`), so it now extracts `handleOnBackPressed`'s OWN body and genuinely fails if the handler is emptied or calls the core inline. A new test `the_block_extractor_stops_at_the_matching_brace` (with a decoy `driveCore` further down) guards the extractor itself. I re-ran all 6 shape tests locally: green. The three artifacts (matrix, README, test doc) that claimed the guard were re-verified true against the fixed extractor. The block was a real defect the gate caught; the recovery honoured it.

## Ledger residue cleaned (this commit)

Nit 5 (a real one): the task landed in `done/` still carrying `needsAnswers: true` and a live `work/questions/task-android-hardware-back-button-navigates-history.md` stuck-sidecar from the earlier bounces - the runner did not clear them on the final merge. Per WORK-CONTRACT a done body must not co-exist with a live needsAnswers/questions sidecar. As the conductor I cleared both in this Gate-3 commit (a protocol-mechanical cleanup, like a claim revert): removed the `needsAnswers: true` line and deleted the resolved sidecar (its questions were the vacuous-test block, now fixed).

## Review-nits triage (Gate-2) - flags for the human

1. androidx.activity (1.9.3) dependency added; base class android.app.Activity -> androidx.activity.ComponentActivity - reverses a prior explicit zero-androidx stance. Justified (no non-deprecated system-back API on a framework Activity below Android 13; minSdk 21; view layer stays framework-only), recorded. RATIFY the durable module dependency.
2. A stale rationale now contradicted: `docs/spikes/mobile-provider-injection-and-trust-indicator/decisions.md` still says WebViewCompat was rejected to avoid an androidx dep on the app module - now false. Worth a one-line amendment (FLAGGED; not done here to keep this commit to the ledger cleanup).
3. Predictive-back (android:enableOnBackInvokedCallback) deliberately NOT opted into (Android 13+ uses legacy dispatch). Confirm you agree it stays deferred vs a named follow-up.
4. The Gradle/Kotlin build is NOT in Gate-1 verify (cargo-only); the base-class swap + dep are only compiled by the tag/dispatch android-apk leg, corroborated by the recorded emulator run. Accepted-risk note, not a defect.
5. (Cleaned above.)

## Recovery history (for the record)

This task was unusually stubborn on INFRA, not code: (a) two runner crashes / wrapper-timeout orphans (the Android/Gradle gate build is slow, and my foreground wrapper timed out; switched to a detached `nohup` dispatch that survives turn boundaries); (b) a Gate-2 review that repeatedly HUNG (a model-API stall: the review `pi` sat at ~0% CPU / near-zero CPU-time for 30+ min, state Sl on I/O) - killed the process tree per the interrupt-footgun rule and re-dispatched from the kept branch each time; (c) the one GENUINE Gate-2 block (the vacuous test), recovered via requeue + precise `-m` fix instruction. No work was ever lost (each recovery continued from the kept `work/` branch). The build itself was deterministic and green throughout.

## Net effect

The Android hardware/system Back button now navigates page history (same off-UI-thread path as the on-screen button, in lockstep with canGoBack), instead of exiting the app - fixing the v0.2.5 finding. The shape guard genuinely pins the off-UI-thread wiring now (the vacuous-test defect the gate caught is fixed). Ledger residue cleared.
