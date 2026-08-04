# The iOS collapse: the decisions this task baked in

Task `ios-chrome-collapse-reload-stop-and-drop-history-buttons`, spec `chrome-conventional-controls` (stories 8-11). These are the non-obvious, in-scope judgement calls behind the iOS toolbar losing its `◀`/`▶`, its Reload and Stop buttons becoming one control, and a spinner joining the URL bar's progress line. The task asked for two of them by name (what fills the freed width, and whether the spinner shares a slot with the collapsed control); the rest are here because a reviewer or the fan-in task would otherwise have to reverse-engineer them.

The shared, cross-edge decisions this task INHERITS rather than re-makes are in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md` (the control follows `is_loading` while the spinner follows `load_progress_visible`; the mode carries its own painter vocabulary including the ACTION it performs; `description()` rather than `tooltip()`; the spinner sits immediately after the control and never near the trust badge). The Android sibling answered the same questions one task earlier in `docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md`; where this edge agrees with it, that is a DECISION (see §4), not an accident.

## 0. Drift check: both premises the task rests on were re-verified before anything was built

The task is a launch snapshot, and it rests on two things a sibling task was supposed to have landed. Both were confirmed against the code first:

- **The edge-swipe gesture is really ON.** `webView.allowsBackForwardNavigationGestures = true` in `WKWebViewShellController.layoutChrome`, landed by `enable-the-ios-back-forward-swipe-gesture` and pinned by `crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs`. That task also made a gesture-driven move report through the SAME load-lifecycle path a button-driven one takes (`BrowserShell::note_history_navigated` → the shared `enter_history_entry`), so the chrome does not go stale after a swipe. Without both halves this task would have been the change that left iOS with no history navigation at all.
- **The chrome JSON really carries the new fields.** `reloadStopControl`, `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible` are encoded by `werust_core::chrome_json`, landed by `reload-stop-collapse-and-loading-spinner-core-and-gtk`, and asserted there to agree with the core functions verbatim.

No drift. Nothing in this task re-derives either.

## 1. The freed width goes to the URL field, and nothing else moves in

**Chosen:** the toolbar is now `[reload/stop] [spinner] [URL field] [invalid badge] [⋮]`. Three widgets left (two history buttons and the second reload/stop button) and one small one arrived; the width they freed goes entirely to the URL field, which already hugs at `.defaultLow` while every other member hugs at `.required`, so it absorbs the slack with no code at all.

**Why:** the whole point of story 11 is that on a phone the URL bar is the width-starved element, and this repo has already fixed that exact complaint once (`fix-mobile-chrome-urlbar-crowded-by-buttons`). Putting anything NEW in the reclaimed space would spend the width the task exists to reclaim. The trust badge is not a candidate: it lives in the footer on this edge, and moving it would be an unasked chrome change with its own trust-presentation questions.

**Alternatives considered:** (a) promote the trust badge into the toolbar with the freed width, rejected as out of scope and as a change to what the trust surface IS, not just where it sits; (b) grow the remaining controls toward the 44pt iOS touch target, rejected because a `UIButton(type: .system)` in this row already lays out at the system metrics and extra width would only shrink the field again; (c) restore forward under the ⋮ menu, rejected twice over: the swipe already covers forward on iOS, and the ⋮ menu is a shared core list, so a per-edge entry there would fork it.

**Touches:** nothing outside this edge. Same conclusion the Android sibling reached, for the same reason, through a different layout mechanism (a layout weight there, a hugging priority here).

## 2. The spinner gets its OWN permanent slot, beside the control, not shared with it

**Chosen:** the spinner is a separate `UIActivityIndicatorView(style: .medium)` in its own permanently allocated slot immediately after the control. It does NOT share a slot with the collapsed control (the control does not turn into a spinner while loading), and it is shown/hidden by its ALPHA, never by `isHidden`.

**Why (not sharing):** sharing the slot would undo the collapse's whole point. While loading, the control's job is to be the STOP button; a control that becomes a spinner while a load is in flight is unavailable exactly when its cancel action matters, which is the affordance stories 5 and 10 both depend on. Two widgets also say two different things: the control offers an ACTION, the spinner REPORTS a state, and the core keeps them on deliberately different rules (`is_loading` vs `load_progress_visible`), so during the pre-content name-resolution window the spinner turns while the control is still Reload. One shared widget could not express that at all.

**Why alpha and not `isHidden`:** this is a `UIStackView`, and `isHidden` on an ARRANGED SUBVIEW removes it from the row entirely, so every load start and end would shove the URL field sideways under the user's finger. That is the horizontal twin of the layout jump `loading-progress-in-the-url-bar-not-a-banner` fixed vertically on this very screen, and the progress line's own `alpha = progressVisible ? 1 : 0` is the local precedent this follows. `hidesWhenStopped = false` is what stops `UIActivityIndicatorView` doing the same thing to itself when it stops.

**Sub-decision, the animation is started and stopped rather than left running under a zero alpha.** An indicator with `alpha = 0` is invisible but still animating, i.e. a timer werust would be paying for on every settled page — on the platform where the ANR/battery lesson is already recorded for its twin (`android-anr-main-thread-diagnose-and-unblock`). So both the alpha and the animation state are driven from ONE local (`spinnerVisible = chrome.loadSpinnerVisible`), which is also why the guard asserts the local exists: two reads of the same field could in principle be edited apart, one read cannot.

**Alternatives considered:** (a) the control becomes the spinner (rejected above); (b) hide with `isHidden` when idle (rejected for the layout jump); (c) put the spinner in the footer beside the status line, rejected because it separates the two loading surfaces the toolbar owns and the footer already carries the phase in WORDS.

## 3. The tap activates ONE core entry point (`activate_reload_stop_control`), instead of a Swift `switch`

**Chosen:** a new session action in the iOS edge crate, `CoreSession::activate_reload_stop_control` (C-ABI `werust_ios_activate_reload_stop_control`, Swift `WerustCore.activateReloadStopControl`), which reads `werust_core::reload_stop_control(chrome)` and performs that mode. The Swift handler is `@objc private func onReloadStop() { core.activateReloadStopControl(); afterCoreAction() }` and contains no `core.reload()` / `core.stop()` at all.

**Why:** the mode's LOOK is carried on the chrome JSON, but its BEHAVIOUR is not carried by anything a Swift edge can read, so without this the handler would need `switch chrome.reloadStopControl { case "reload": …; case "stop": … }`. That is a hand-written twin of a core rule sitting one call away from the field that already answers the question, in the exact file whose twins `docs/adr/0011` deleted — and this edge has previous form: its invalid-entry badge text was a Swift literal set at build time and never refreshed. Reading the mode and performing it in the SAME call also removes a race the Swift branch would have: the mode Swift painted a frame ago is not necessarily the mode now.

This is the mobile counterpart of what the GTK edge does with `ReloadStopControl::action()` and its one `perform_chrome_action`; the difference is only that a `ChromeAction` cannot cross a C-ABI as anything but a string, so the resolution stays on the Rust side of it.

**Coherence note (a new concept, checked against the existing language):** this mints no new vocabulary. "Activate" is already the mobile edges' verb for a menu ENTRY being chosen (`onBrowserMenuItem`, "Dispatch an activated browser-menu entry"), the concept being performed is the core's existing `ReloadStopControl`, and the method name is byte-for-byte the Android one. It is an edge-local FFI action beside `reload` / `stop` / `go_back`, not a new seam and not a new chrome concept.

**Alternatives considered:** (a) the Swift `switch` (rejected above); (b) carry the ACTION as a string on the chrome JSON and dispatch on it in Swift, rejected because it is the same branch with extra ceremony and it would put a fifth field on the mobile carrier for two edges that would each translate it straight back into a method call; (c) match on `ChromeAction` inside the new Rust method instead of on `ReloadStopControl`, rejected because `ChromeAction` is a wide enum, so it needs a catch-all arm that either silently no-ops (a hidden refusal) or panics across the C-ABI (an abort); matching the closed two-variant mode makes a future third mode a COMPILE error here instead.

**Sub-decision: the `reload` / `stop` FFI entry points are KEPT.** The C-ABI is documented as a mechanical mirror of `CoreSession`'s methods, the new action calls exactly those two shell methods, and the header is a lock-step declaration of the Rust surface rather than a menu of what the current toolbar happens to use. Same for `go_back` / `go_forward`, which the swipe path and the core still need.

**Touches:** the fan-in task (see §5), and the `mobile-ios` + release legs, which now assert the new symbol is really linked (see §6).

## 4. The wire-name field `reloadStopControl` is NOT decoded by this edge either

**Chosen:** the Swift binding decodes three of the four new carrier fields — `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible` — and not `reloadStopControl` (the mode's stable wire name). This MATCHES the Android binding exactly.

**Why:** a painter that must not branch on the mode has nothing to DO with the wire name: the glyph, the accessible name and the action are all already answered. Decoding it anyway would be a field carried into Swift for no consumer, and the only tempting consumer is the `switch` §3 exists to prevent.

Matching Android is itself the decision here, not just the outcome. `docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md` §4 states plainly that the iOS task was free to decide otherwise, but that if it did, the two edges would disagree and the fan-in task would inherit the argument. There is no iOS-specific reason to want the wire name, so agreeing costs nothing and hands `register-the-new-chrome-fields-in-the-mobile-presentation-guard` one shape to register: those three fields, and not the fourth (registering `reloadStopControl` would demand a paint that deliberately does not exist on either mobile edge).

## 5. This task does NOT register the new fields in the mobile presentation guard

**Chosen:** `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` is untouched, and the new iOS guard asserts that it stays untouched (`the_mobile_presentation_guard_field_lists_are_not_registered_here`).

**Why:** the guard demands that BOTH mobile bindings decode and BOTH painters paint every registered derived field. This task is the SECOND (and last) MIGRATE step, so after it lands the fields ARE consumed everywhere the guard would look — but registering them is still a separate, reviewable change, owned by `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, which is blocked on exactly these two edges. Slipping the registration in here would take the contract step through a review aimed at a painter, and would mean this task's own gate run was the only thing that ever exercised it.

**The assertion is the honest kind, not the vacuous kind.** It reads the LITERAL half of the scan and carries a POSITIVE CONTROL (`loadProgressVisible` and the `func loadProgressVisible()` signature, one near each end of the scanned file). That shape is not a style choice: the Android twin first asserted absence from the literal-STRIPPED code view, where a field name can only ever appear as a string literal, so the assertion could never fail — caught in review, recorded in `docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md` §7, and deliberately not repeated here. Mutation-checked: adding `"loadSpinnerVisible"` to `DERIVED_FIELDS` turns this test red, naming the fan-in task.

## 6. The CI evidence: the existing `mobile-ios` leg gains a symbol assertion

**Chosen:** `docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh` now checks a LIST of required C-ABI symbols rather than one, and the list gains `_werust_ios_activate_reload_stop_control`.

**Why:** there is no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so a visual check of the toolbar is not evidence anybody can produce. The pure-Rust gate can assert the SOURCE SHAPE and the core BEHAVIOUR, and it does; what it cannot do is compile Swift. The `mobile-ios` leg can, and it already builds the Simulator `.app` and inspects its symbols — so the cheapest real assertion available is that the shipped binary carries the entry point the one control (and therefore CANCEL) rides on. A Swift file that stopped calling it, or a link that dropped the core, fails the leg.

**What this touches, stated plainly:** the same script runs in the RELEASE job (`release.yml`, the `ios-simulator-app` artifact), so the release leg now also fails if that symbol is missing. That is the intended reading — an `.app` without the collapsed control's action is not a shippable artifact — but it is a second consumer, so it is recorded rather than left to be discovered.

**Alternatives considered:** (a) add a new CI leg, rejected by the repo's own convention (a CI-measurable criterion needs its leg on `main` FIRST, and inventing one inside the task is exactly what that rule forbids); (b) assert nothing beyond the Rust gate, rejected because the task explicitly asks not to leave the removal resting on a check nobody can perform; (c) a UI test on the simulator, rejected as a new harness this task has no mandate to introduce and no Mac to develop against.

## 7. What was deliberately NOT fixed: `core.stop()` still does not stop the `WKWebView`

**Not chosen, on purpose:** `WKWebViewShellController` never calls `webView.stopLoading()`, so the platform load keeps running after a cancel; the core settles its own load state and the chrome goes idle. That was noticed and filed during the previous task (`work/notes/observations/ios-stop-does-not-stop-the-wkwebview-2026-08-04.md`) and is PRE-EXISTING, not introduced here.

**Why leave it:** the acceptance criterion is that cancelling an in-flight load is STILL possible after the collapse, and it is — through exactly the fact and the shell action the separate Stop button used (asserted headlessly in `the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight`). Fixing the platform half is a different change with its own design question, spelled out in that note: the moment `stopLoading()` is added, WebKit answers with `NSURLErrorCancelled` (-999) on `didFailProvisionalNavigation`, which this edge reports straight into `core.onPageFailed`, so Stop would start flashing a red error banner. That needs the failure filter the macOS backend already has (`navigation_failure` in `crates/macos-renderer/src/pure.rs`), which is a change to the ERROR path, not the painter — out of this task's scope, and it would make the collapse's diff carry an unrelated behaviour change nobody asked for.
