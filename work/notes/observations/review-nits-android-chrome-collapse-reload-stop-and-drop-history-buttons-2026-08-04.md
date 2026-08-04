---
title: review-gate non-blocking nits for 'android-chrome-collapse-reload-stop-and-drop-history-buttons' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: android-chrome-collapse-reload-stop-and-drop-history-buttons
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-chrome-collapse-reload-stop-and-drop-history-buttons' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the requeue finding is fixed, but confirm the fix is the one you wanted. The sequencing test now asserts against the LITERALS half of scan (plus a positive control on loadProgressVisible at both ends of the file), so registering loadSpinnerVisible in DERIVED_FIELDS really does turn it red; DECISIONS.md section 7 and the README mutation list were corrected to match.
  (crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs the_mobile_presentation_guard_field_lists_are_not_registered_here; docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/{DECISIONS.md section 7, README.md Mutation-checked})
- Ratify a behaviour change the task did not ask for: cancel (the old Stop) ran INLINE on the UI thread and now goes through driveCore, so it waits on the background executor. Rationale given is that every native call takes the session mutex anyway, so inline Stop could already block the main thread; cancel latency is argued unchanged.
  (BrowserActivity.kt driveCore { core.activateReloadStopControl() }; DECISIONS.md section 3 sub-decision)
- Ratify a cross-task constraint: Android deliberately does NOT decode the mode wire-name field reloadStopControl, only the three painted fields. That pre-decides what the fan-in task may register and asks the iOS sibling to match; if iOS decodes it, the two edges disagree and the fan-in inherits the argument.
  (WerustCore.kt Chrome decodes reloadStopControlLabel / reloadStopControlDescription / loadSpinnerVisible only; DECISIONS.md section 4)
- Ratify a new FFI verb: CoreSession::activate_reload_stop_control plus JNI nativeActivateReloadStopControl, matching on the closed ReloadStopControl enum rather than routing a ChromeAction as the desktop edges do. Coherent (activate is already this edge's verb, the concept is the core's existing mode) but it is a new cross-boundary entry point iOS is expected to mirror.
  (crates/werust-android/rust/src/lib.rs activate_reload_stop_control + SyncSession wrapper; DECISIONS.md section 3)
- Ratify a user-visible accessibility default: the spinner is marked IMPORTANT_FOR_ACCESSIBILITY_NO with no contentDescription, so TalkBack skips it entirely; the argument is that the load is already announced by the progress line and the footer status.
  (BrowserActivity.kt loadingSpinner; DECISIONS.md section 5)
- Test-coupling nit: the system-Back guard's off-UI-thread reference assertion was re-pointed from the departed on-screen button to the URL bar's driveCore { core.navigate(entry) }. A future refactor of the URL bar dispatch will now red the BACK guard for an unrelated reason. Consider asserting the handler's own dispatch shape only.
  (crates/werust-android/rust/tests/system_back_wiring_shape.rs the_system_back_drives_the_core_off_the_ui_thread_like_every_other_action)
- Ratify the matrix state: the reload-stop-collapse Android row flips from stubbed to implemented while the spike README states plainly that NO emulator/device run was performed. Precedent exists (the Windows history row is implemented with nothing in CI having pressed Back), and the device steps are recorded, but the runtime evidence gap is real for a widget-level change.
  (docs/platform-capability-matrix.toml android = { state = 'implemented' }; spike README section Device verification, NOT YET RUN)
