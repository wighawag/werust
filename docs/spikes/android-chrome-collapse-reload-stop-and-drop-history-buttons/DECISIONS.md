# The Android collapse: the decisions this task baked in

Task `android-chrome-collapse-reload-stop-and-drop-history-buttons`, spec `chrome-conventional-controls` (stories 8-12). These are the non-obvious, in-scope judgement calls behind the Android toolbar losing its on-screen `◀`/`▶`, its Reload and Stop buttons becoming one control, and a spinner joining the URL bar's progress line. The task asked for two of them by name (what fills the freed width, and whether the spinner shares a slot with the collapsed control); the rest are here because a reviewer or the sibling iOS task would otherwise have to reverse-engineer them.

The shared, cross-edge decisions this task INHERITS rather than re-makes are in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md` (the control follows `is_loading` while the spinner follows `load_progress_visible`; the mode carries its own painter vocabulary including the ACTION it performs; `description()` rather than `tooltip()`; the spinner sits immediately after the control and never near the trust badge).

## 1. The freed width goes to the URL bar, and nothing else moves in

**Chosen:** the toolbar is now `[reload/stop] [spinner] [URL bar] [invalid badge] [⋮]`. Three widgets left (two history buttons and the second reload/stop button) and one small one arrived; the width they freed goes entirely to the URL bar, which was already the WEIGHTED member of the row (`LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)`) and therefore absorbs it with no code at all.

**Why:** the whole point of story 11 is that on a phone the URL bar is the width-starved element, and this repo has already fixed that exact complaint once (`fix-mobile-chrome-urlbar-crowded-by-buttons`: four fat default buttons squeezing a weighted `EditText`). Putting anything NEW in the reclaimed space would spend the width the task exists to reclaim. The trust badge is not a candidate: it lives in the footer on this edge and moving it would be an unasked chrome change with its own trust-presentation questions.

**Alternatives considered:** (a) promote the trust badge into the toolbar with the freed width, rejected as out of scope and as a change to what the trust surface IS, not just where it sits; (b) widen the remaining buttons toward the 48dp touch target, rejected because the ROW is already 48dp tall (`TOUCH_TARGET_DP`), so the effective touch target is already compliant and extra width would only shrink the bar again; (c) add a forward button back under an overflow menu entry, rejected: the spec accepts losing forward, and the ⋮ menu is a shared core list, so a per-edge entry there would fork it.

**Touches:** nothing outside this edge. The iOS sibling task should reach the same conclusion for the same reason, but its stack has its own hugging/stretch priorities.

## 2. The spinner gets its OWN permanent slot, beside the control, not shared with it

**Chosen:** the spinner is a separate 20dp indeterminate `ProgressBar` in its own permanently allocated slot immediately after the control, `INVISIBLE` (never `GONE`) when idle. It does NOT share a slot with the collapsed control (i.e. the control does not turn into a spinner while loading).

**Why:** sharing the slot would undo the collapse's whole point. While loading, the control's job is to be the STOP button; a control that becomes a spinner while a load is in flight is a control that is unavailable exactly when its cancel action matters, which is the affordance story 5 and story 10 both depend on. Two widgets also say two different things: the control offers an ACTION, the spinner REPORTS a state, and the core already keeps them on deliberately different rules (`is_loading` vs `load_progress_visible`), so during the pre-content name-resolution window the spinner turns while the control is still Reload. One shared widget could not express that at all.

`INVISIBLE` rather than `GONE` is the same geometry rule the GTK edge applies with opacity and the same one `loading-progress-in-the-url-bar-not-a-banner` learned vertically on this very Activity: a slot that is given back re-lays-out the row, so every load start and end would shove the URL bar sideways under the user's finger. 20dp because a status indicator is not a touch target, and a permanently reserved slot should cost the URL bar as little as it can.

**Alternatives considered:** (a) the control becomes the spinner (rejected above); (b) hide the spinner with `GONE` when idle, rejected for the layout jump; (c) put the spinner in the footer beside the status line, rejected because it separates the two loading surfaces the toolbar owns and the footer already carries the phase in WORDS.

## 3. The click activates ONE core entry point (`activate_reload_stop_control`), instead of a Kotlin `when`

**Chosen:** a new session action in the Android edge crate, `CoreSession::activate_reload_stop_control` (JNI `nativeActivateReloadStopControl`, Kotlin `WerustCore.activateReloadStopControl`), which reads `werust_core::reload_stop_control(chrome)` and performs that mode. The Kotlin click handler is `driveCore { core.activateReloadStopControl() }` and contains no `core.reload()` / `core.stop()` at all.

**Why:** the mode's LOOK is carried on the chrome JSON, but its BEHAVIOUR is not carried by anything a Kotlin edge can read, so without this the handler would need `when (chrome.reloadStopControl) { "reload" -> …; "stop" -> … }`. That is a hand-written twin of a core rule sitting one call away from the field that already answers the question, in the exact file whose twins `docs/adr/0011` deleted, and it would drift the moment the two arms stopped agreeing with `ReloadStopControl::action()`. Reading the mode and performing it in the SAME locked call also removes a race the Kotlin branch would have: the mode Kotlin painted a frame ago is not necessarily the mode now.

This is the mobile counterpart of what the GTK edge does with `ReloadStopControl::action()` and its one `perform_chrome_action`; the difference is only that a `ChromeAction` cannot cross the JNI boundary as anything but a string, so the resolution stays on the Rust side of it.

**Coherence note (a new concept, checked against the existing language):** this mints no new vocabulary. "Activate" is already this edge's verb for a menu ENTRY being chosen (`onBrowserMenuItem`, "Dispatch an activated browser-menu entry"), and the concept being performed is the core's existing `ReloadStopControl`. It is an edge-local FFI action beside `reload` / `stop` / `go_back`, not a new seam and not a new chrome concept.

**Sub-decision, it goes through `driveCore` (the background executor), where the old Stop button ran INLINE on the UI thread.** The old Stop was documented as "a cheap non-blocking core call", but that was only true of the shell method: every native call on this edge takes the `SyncSession` mutex first, so an inline Stop already blocked the UI thread whenever a worker held the lock (the ANR guard, `work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md`). Dispatching it like every other session-driving action costs the same wall-clock wait but stops that wait happening on the main thread. Cancel latency is unchanged: both routes wait for the same lock.

**Alternatives considered:** (a) the Kotlin `when` (rejected above); (b) carry the ACTION as a string on the chrome JSON and dispatch on it in Kotlin, rejected because it is the same branch with extra ceremony, and it would put a fifth new field on the mobile carrier for two edges that would each translate it back into a method call; (c) match on `ChromeAction` inside the new Rust method instead of on `ReloadStopControl`, rejected because `ChromeAction` is a wide enum, so it needs a catch-all arm that either silently no-ops (a hidden refusal) or panics across the JNI boundary (an abort); matching the closed two-variant mode makes a future third mode a COMPILE error here instead.

**Touches:** the iOS sibling task, which faces the identical question on its own FFI and should mirror this rather than writing a Swift `switch`. The `reload` / `stop` FFI entry points are KEPT (the binding is documented as a mechanical mirror of the JNI surface, the shell methods are still the ones this new action calls, and the desktop/iOS edges are unaffected).

## 4. The wire-name field `reloadStopControl` is NOT decoded by this edge

**Chosen:** the Kotlin binding decodes three of the four new carrier fields — `reloadStopControlLabel`, `reloadStopControlDescription`, `loadSpinnerVisible` — and not `reloadStopControl` (the mode's stable wire name).

**Why:** a painter that must not branch on the mode has nothing to DO with the wire name: the glyph, the accessible name and the action are all already answered. Decoding it anyway would be a field carried into Kotlin for no consumer, and the only tempting consumer is the `when` decision 3 exists to prevent.

**What this hands the fan-in task, stated plainly:** `register-the-new-chrome-fields-in-the-mobile-presentation-guard` asserts that both bindings DECODE and both painters PAINT every field in `DERIVED_FIELDS`. So it should register the three fields above; registering `reloadStopControl` would demand a paint that deliberately does not exist on either mobile edge. The iOS task is free to decide otherwise, but if it does, the two edges disagree and the fan-in task inherits the argument, so this is the shape to match.

## 5. The spinner announces nothing to a screen reader

**Chosen:** `importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO` on the spinner, and no `contentDescription`.

**Why:** the load is already announced twice on this edge — the progress line carries the core's phase hint as its content description, and the footer status line names the phase in words. A third node saying the same thing on every load is noise for exactly the users who can least skim past it, and this repo has already had one accessibility regression from treating a screen reader as a place to dump chrome text (`mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`). The spinner is the DECORATIVE half of a fact that is already spoken; the control beside it keeps a real accessible name (the core's description), which is the node a user actually acts on.

**Alternatives considered:** (a) give the spinner the core's `loadProgressHint` too, rejected as the duplicate announcement above; (b) leave it default (a `ProgressBar` with no description), rejected because it can still take focus in some navigation modes and announce an unlabelled progress bar, which is worse than being skipped.

## 6. The system-Back guard is re-pointed at the FACT, not weakened

**Chosen:** `crates/werust-android/rust/tests/system_back_wiring_shape.rs` keeps every assertion, but the two that referenced the departed on-screen button now assert against what survives: the enablement line must still read `chrome.canGoBack` in `refreshChrome` (plus a NEW assertion that `backButton` is absent, with instructions to restore the lockstep assertion if a button ever returns), and the off-UI-thread reference path is now the URL bar's `driveCore { core.navigate(entry) }`.

**Why:** those two assertions were written as "the system Back does what the on-screen button does". Deleting the button makes that sentence unfalsifiable, and deleting the assertions with it would quietly remove the ONLY gate-side cover for a field-reported bug at the exact moment it became the sole back affordance. Re-pointing them at the core fact keeps the same teeth. Both were re-checked by mutation (removing the enablement line reds both this guard and the new one).

## 7. This task does NOT register the new fields in the mobile presentation guard

**Chosen:** `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` is untouched, and the new Android guard asserts that it stays untouched (`the_mobile_presentation_guard_field_lists_are_not_registered_here`).

**Why:** the guard demands that BOTH mobile bindings decode and BOTH painters paint every registered derived field. This is the MIGRATE step for ONE edge, so registering the fields here would red the gate until `ios-chrome-collapse-reload-stop-and-drop-history-buttons` lands. The CONTRACT step is `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, blocked on both edges. Asserting the absence (rather than just leaving it alone) is what stops a well-meaning later change from registering it early and then "fixing" the red by weakening the guard, which is the one mechanism keeping the Kotlin/Swift chrome twins from coming back.

**Consequence:** until the fan-in lands, Android consumes three carrier fields that the shared mobile guard does not yet police; they are policed meanwhile by this task's own edge guard, which is strictly narrower (it sees one edge).
