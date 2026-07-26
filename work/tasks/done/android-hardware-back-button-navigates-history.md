---
title: "The Android hardware/system back button must navigate back in page history (WebView back), not exit the app"
slug: android-hardware-back-button-navigates-history
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
needsAnswers: true
---

## What to build

FIELD FINDING (v0.2.5, human, MOBILE/Android): "the android back button do not navigate back in history like it should." The Android hardware/system Back gesture exits the app (default Activity finish) instead of going back a page, even when there IS page history to go back through.

READ-FIRST / drift check: confirm the mechanism. `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` has an ON-SCREEN `◀` `backButton` that calls `driveCore { core.goBack() }` and is enabled from `chrome.canGoBack`, but there is NO handler for the SYSTEM back button - no `onBackPressed` override, no `OnBackPressedCallback` / `getOnBackInvokedDispatcher` (Android 13+ predictive back), no `KEYCODE_BACK` handling. So the system Back falls through to the default (finish the Activity) and the app exits. Confirm no system-back handler exists yet.

Fix: intercept the system Back so that when there IS back history it drives `core.goBack()` (the SAME path the on-screen `◀` uses), and only when there is NO more back history does it fall through to the default (exit/finish the Activity). Use the modern, non-deprecated API:
- Register an `OnBackPressedCallback` on the Activity's `onBackPressedDispatcher` (AndroidX), whose `isEnabled` tracks `chrome.canGoBack` (enable it when there is back history, disable it so the default back = exit when there is not). In `handleOnBackPressed`, run `driveCore { core.goBack() }`. Keep it coherent with the ANR-fix executor: the core call goes through `driveCore` (off the UI thread) and posts the chrome refresh back, exactly like the on-screen button - do NOT reintroduce a UI-thread blocking call. Update the callback's `isEnabled` whenever the chrome is refreshed (where `backButton.isEnabled = chrome.canGoBack` is already set), so the system Back and the on-screen button stay in lockstep.
- (Android 13+ predictive back is opt-in via the manifest `android:enableOnBackInvokedCallback`; the `OnBackPressedDispatcher` bridges to it, so registering the callback is the one implementation that works across versions. Decide + record whether to opt into predictive-back animation now or later.)

Coherence: the on-screen `◀` and the system Back must do the SAME thing (go back one page when possible). A `_blank`/in-place nav, an SPA client-side nav, and a normal load all push history the backend owns, so `core.goBack()` is the single correct action for all of them. Trust/lifecycle unchanged.

## Acceptance criteria

- [ ] The Android system/hardware Back button navigates BACK one page in history when there is back history (same effect as the on-screen `◀`), instead of exiting the app.
- [ ] When there is NO more back history, the system Back falls through to the default (exit/finish the Activity) as a normal browser/app does.
- [ ] The system Back drives the core through the SAME off-UI-thread path (`driveCore { core.goBack() }`) as the on-screen button, so the ANR fix is not regressed (no UI-thread blocking core call).
- [ ] The system-Back enabled state stays in lockstep with `chrome.canGoBack` (updated wherever the on-screen back button's enabled state is), so the two Back affordances never disagree.
- [ ] Implemented with the non-deprecated `OnBackPressedDispatcher` / `OnBackPressedCallback` API (not the deprecated `onBackPressed()` override); the predictive-back (Android 13+) opt-in decision is recorded.
- [ ] Tracked per the parity guard (system-back is an Android-specific affordance; desktop/iOS have their own back). Where the logic is runtime-only, add the strongest automatable guard (e.g. the canGoBack-driven enablement is assertable) + recorded manual device steps.

## Blocked by

- None. (Android-only; composes with the existing goBack path + the ANR-fix executor.)

## Prompt

> Goal: make the Android hardware/system Back button go BACK one page in WebView history (when there is history) instead of exiting the app. Today only the on-screen `◀` button calls `core.goBack()`; there is no system-back handler, so system Back finishes the Activity.
>
> Where to look: `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` (the `backButton` -> `driveCore { core.goBack() }`, `backButton.isEnabled = chrome.canGoBack`). Register an `OnBackPressedCallback` on `onBackPressedDispatcher`: its `isEnabled` = `chrome.canGoBack` (updated wherever the on-screen button's enabled state is), `handleOnBackPressed` runs `driveCore { core.goBack() }` (SAME off-UI-thread path as the button, so the ANR fix isn't regressed); when there's no back history the callback is disabled so the default Back exits. Use the non-deprecated OnBackPressedDispatcher (not `onBackPressed()` override); record the predictive-back (Android 13+) opt-in decision.
>
> Done = system Back navigates history when possible, exits when at the start; same off-UI-thread path as the on-screen button; enabled state in lockstep with `canGoBack`; tracked per the parity guard with a canGoBack-enablement guard + recorded device steps. FIRST re-check no system-back handler exists yet.

## Requeue 2026-07-26

Gate-2 BLOCK (valid, fix it): the shape-guard test for criterion 3 is VACUOUS. In crates/werust-android/rust/tests/system_back_wiring_shape.rs, kotlin_fun_body picks its terminator by KIND not POSITION: rest.find('\n    private fun ') is tried FIRST and matches the far-later 'private fun driveCore', so the extracted handleOnBackPressed body is ~9700 chars and swallows all of onCreate — the positive assert then passes on the ON-SCREEN button line 'compactNavButton(...) { driveCore { core.goBack() } }' inside onCreate, NOT the real handler (proven by mutation: emptying handleOnBackPressed keeps all 4 asserts green). FIX: bound the extracted body at the MINIMUM index over ALL terminator candidates (nearest 'override fun '/'private fun '/closing brace wins), OR extract the systemBackCallback object literal / handleOnBackPressed block specifically, so the assert genuinely pins that handleOnBackPressed calls driveCore { core.goBack() } and FAILS if the handler is emptied or calls the core inline on the UI thread. Then re-verify the matrix row (system-back-navigates-history 'strongest automatable guard'), the spike README ('handleOnBackPressed drives driveCore...never an inline UI-thread core call'), and the test module doc criterion-3 mapping are TRUE against the fixed extractor. The sibling refreshChrome/lockstep test is fine; only the handler test + those 3 claims need fixing.
