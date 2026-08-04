# Android: one reload/stop control, a loading spinner, and no on-screen history buttons

Task `android-chrome-collapse-reload-stop-and-drop-history-buttons`, spec `chrome-conventional-controls` (stories 8-12). The decisions are in `DECISIONS.md` beside this file; this is what changed, what the gate can prove, and what only a device can.

## What changed

`crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt`:

- The toolbar is `[reload/stop] [spinner] [URL bar] [invalid badge] [⋮]`. The on-screen `◀` and `▶` are GONE, and the separate Reload and Stop buttons are ONE `reloadStopButton`. The URL bar is still the weighted member of the row, so it absorbs all the freed width.
- `refreshChrome` paints the control from the carrier and nothing else: `reloadStopButton.text = chrome.reloadStopControlLabel`, `reloadStopButton.contentDescription = chrome.reloadStopControlDescription`, `loadingSpinner.visibility = if (chrome.loadSpinnerVisible) View.VISIBLE else View.INVISIBLE`. The painter no longer reads the raw `chrome.loading` fact at all — the two lines that did (`stopButton.isEnabled = chrome.loading` / `reloadButton.isEnabled = !chrome.loading`) were the pre-collapse rule, and they are what the new guard forbids coming back.
- The control's CLICK is `driveCore { core.activateReloadStopControl() }`: one core entry point that performs whatever the core's own mode says (see `DECISIONS.md` §3). Cancel is therefore still one tap, on the same control, and still off the UI thread.
- The system Back callback is untouched and is now the ONLY back affordance: `systemBackCallback.isEnabled = chrome.canGoBack` in `refreshChrome`, `handleOnBackPressed` → `driveCore { core.goBack() }`.

`crates/werust-android/rust/src/lib.rs`: `CoreSession::activate_reload_stop_control` (+ the `SyncSession` locked wrapper and the JNI export). `WerustCore.kt`: the matching binding method and the three new decoded chrome fields.

Nothing in `werust-core` changed: `can_go_back` / `can_go_forward`, the history seam and the chrome derivation are exactly as `reload-stop-collapse-and-loading-spinner-core-and-gtk` left them. This edge only reads them.

## What the gate proves

`crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs` (a plain `cargo test`, so it rides `verify` with no CI change):

- **Behaviour, headless and network-isolated.** `the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight` drives a real `CoreSession`: settled → the mode is Reload → activating it starts a load → the mode is now Stop → activating it CANCELS the load (`LoadState::Idle`). That is the acceptance criterion "cancelling an in-flight load is still possible", asserted rather than argued. `the_history_capability_is_untouched_by_the_button_removal` pins that `can_go_back` / `can_go_forward` still track history (forward included, even though this edge now paints no forward affordance).
- **Source shape**, because the painter is Kotlin and neither the Android SDK nor the Gradle build is in this repo's pure-Rust gate: the two history buttons and their glyphs are gone; the toolbar carries the control then the spinner then the URL bar, in that order; every painted value is a whole-statement assignment from the carrier; the painter never touches `chrome.loading` and never spells either mode's glyph or wire name; the click goes through the one core entry point; and the mobile presentation guard's field lists are NOT registered yet (the MIGRATE/CONTRACT sequencing — since the CONTRACT step landed, that assertion reads the other way round and requires the registration).
- **Guards on the guard**: `the_assignment_check_is_not_satisfied_by_a_longer_field_name` (`chrome.reloadStopControl` is a prefix of both painted fields), `the_scanner_reads_literals_and_code_apart` (a glyph named in a KDoc is documentation, not an affordance), and the POSITIVE CONTROL inside `the_mobile_presentation_guard_field_lists_are_not_registered_here` (an already-registered field must be visible to the very check that asserts the new ones are absent, at both ends of the scanned file). That last test was inverted into `the_mobile_presentation_guard_registers_the_fields_this_edge_consumes` when the fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard` landed the registration.

**Mutation-checked**, because a vacuous guard has shipped in this very directory before (the system-Back handler assertion, caught in review — and then the sequencing assertion itself, caught in the review of this task: it scanned the literal-STRIPPED code view for a name that can only ever be a string literal, so it could never fail; `DECISIONS.md` §7). Each of these turns the suite RED: painting the glyph from a Kotlin literal instead of the carrier; the click handler calling `core.reload()` itself; the spinner switched to `if (chrome.loading)`; an on-screen `◀` added back to the toolbar; the `systemBackCallback.isEnabled` line deleted (which also reds `system_back_wiring_shape.rs`); and — the sequencing mutation — registering `"loadSpinnerVisible"` in the mobile presentation guard's `DERIVED_FIELDS` early.

## Device verification (runtime-only, not in any gate)

The widgets themselves — that the glyph really flips mid-load, that the spinner really turns, that the row does not jump — need a device. NOT YET RUN: no emulator/device run was performed for this task, so these steps are outstanding rather than a run of record. Build/install: `cd crates/werust-android && ANDROID_HOME=<sdk> ./gradlew :app:assembleDebug && adb install -r app/build/outputs/apk/debug/app-debug.apk`.

1. **The toolbar has two buttons.** Launch. EXPECT: a reload glyph, a (still) spinner slot, a wide URL bar, and the ⋮ menu. No `◀`, no `▶`.
2. **The control flips and cancels.** Navigate to a slow page. EXPECT: mid-load the control shows the stop glyph and the spinner turns; tapping it CANCELS the load (the status line stops naming a phase, the progress line goes invisible, the spinner stops, the control returns to the reload glyph). Tapping it on a settled page reloads.
3. **The row never jumps.** Watch the URL bar's left edge across a whole load and settle. EXPECT: it does not move — the spinner's slot is reserved whether or not it is turning.
4. **The pre-content window spins.** Navigate to a bare `.eth` name (e.g. `ronan.eth`). EXPECT: the spinner turns during the ENS/IPNS resolve, BEFORE the progress bar has anything to report, and the control is still in its reload mode there (nothing to stop yet — the deliberate split in `reload-stop-collapse-and-loading-spinner-core-and-gtk`'s decision 1).
5. **System Back still navigates history.** Follow a link so there are two entries, press system Back (and repeat with the gesture). EXPECT: it goes back a page and the app does NOT exit; at the start of history it exits. This is the affordance the removed `◀` was traded for, and it was a field-reported bug once — the full step list is in `docs/spikes/android-hardware-back-button-navigates-history/README.md`.
6. **TalkBack.** Focus the control. EXPECT: it announces the core's description ("Reload this page" / "Stop loading this page", following the mode), and the spinner is skipped rather than announcing a second, unlabelled progress bar (`DECISIONS.md` §5).
