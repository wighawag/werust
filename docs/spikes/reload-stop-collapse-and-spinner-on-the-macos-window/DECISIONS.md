# The AppKit collapse and the spinner: the decisions this task baked in

Task `reload-stop-collapse-and-spinner-on-the-macos-window`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind the macOS edge's half of the collapse: one control where there were two, and a loading spinner. The core-side decisions this edge INHERITS (which predicate drives the control versus the spinner, why the mode carries its own vocabulary, why `description()` rather than `tooltip()`) are recorded once, in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md`, and are not re-argued here. The sibling Win32 edge's decisions are in `docs/spikes/reload-stop-collapse-and-spinner-on-the-windows-chrome/DECISIONS.md`; where this edge differs from it, the difference is stated below.

## 1. The spinner IS AppKit's own progress indicator, in its spinning style

**Chosen:** the spinner is an `NSProgressIndicator` with `setStyle(NSProgressIndicatorStyle::Spinning)`, `setControlSize(NSControlSize::Small)` and `setIndeterminate(true)`, shown and hidden from `ChromePaint::spinner_visible` and animated with `startAnimation:` / `stopAnimation:`.

**Why:** unlike Win32, AppKit ships a real spinner, and it is the platform's own idiom for "indeterminate work in progress" — the same control every Mac app uses, so it matches the user's expectations for free and follows the OS appearance without this edge choosing a colour (`docs/adr/0009`: follow the OS, never force). It also animates ITSELF on the run loop, so this edge adds no frame table, no timer and no drawing code: the whole spinner is four setters and a visibility assignment, which is the discipline this file keeps ("assign values the core derived"). The Win32 sibling drew four glyph frames advanced on the chrome pump precisely because that toolkit has no spinner; its own DECISIONS file says in as many words that AppKit should use its toolkit's own, and this is that.

**Alternatives considered:** (a) the Win32 shape — a one-glyph `NSTextField` cycling `◐ ◓ ◑ ◒` on the 50ms pump — rejected because it re-mints an animation this toolkit already provides, is a second visual idiom in a chrome whose other indicators are AppKit controls, and would make the two desktop edges' spinners differ for no reason but history; (b) a determinate `NSProgressIndicator` in bar style, rejected because werust already paints a determinate bar three pixels away inside the URL bar, and a second bar would say two different things ("how far" and "at all") in one visual language, which is the confusion story 8 exists to remove; (c) `setDisplayedWhenStopped(false)` as the visibility mechanism instead of `setHidden`, rejected because it would make "is the spinner showing?" a property AppKit infers from animation state rather than the derived value this edge assigns — and the smoke reads `isHidden()` back, so the assignment has to be the one that is asserted.

**Touches:** nothing outside this crate. The carrier already had `spinner_visible`; no core change was needed or made.

## 2. Where the spinner sits, and why its slot is permanent

**Chosen:** the toolbar reads back, forward, **[reload/stop] [spinner]**, URL bar, invalid badge, trust badge, ⋮ — the layout the GTK edge landed and the Win32 edge followed. `Chrome::relayout` places the spinner's rectangle whether or not it is showing; only its VISIBILITY follows the derivation.

**Why:** the control that ACTS on a load and the indicator that REPORTS one belong together, and the trust badge is at the far end deliberately: motion beside it would read as a claim about the page's TRUST, the conflation `docs/adr/0012` exists to prevent. Keeping the slot allocated is this window's reading of the GTK sub-decision (there, a permanently visible widget at zero opacity): AppKit lays out in absolute frames computed by hand, so hiding the indicator moves nothing on its own, but reserving the slot means a future relayout cannot start making the URL bar's width depend on whether a load is running — the horizontal twin of the geometry lesson `loading-progress-in-the-url-bar-not-a-banner` learned vertically, and the property the window smoke asserts by comparing `page_frame()` across a whole load.

**Sizing:** `SPINNER_WIDTH = 20.0` for the slot with a `SPINNER_SIZE = 16.0` indicator centred in it, beside the existing `BUTTON_WIDTH = 36.0` nav squares. 16pt is what AppKit's small control size draws; the slot is a little wider so the indicator is not flush against the URL bar. Both are named constants in `window.rs` beside the other design metrics, not literals in the layout arithmetic.

## 3. A click performs the MODE's own action, read off the snapshot

**Chosen:** the two Objective-C actions `reloadPage:` and `stopLoading:` are ONE action, `reloadOrStop:`, whose body is `self.perform_chrome_action(self.reload_stop_action())`, where `reload_stop_action` is `ChromePaint::of(shell.chrome()).reload_stop_control.action()`.

**Why:** two properties, both asserted by `crates/werust-macos/tests/macos_window_shape.rs`. The action comes from the CARRIER, so this edge never re-decides which mode the control is in (`reload_stop_control(` is now on the shape guard's forbidden-call list for the AppKit half, beside `status_line(` and the rest — the carrier is the half the Ubuntu gate compiles and tests). And it is performed by the SAME `perform_chrome_action` the keyboard's Cmd+R and Escape and the mouse's side buttons go through, so the toolbar cancel and the keyboard cancel are one path that cannot drift apart. The pre-collapse handlers had one arm each calling the shell directly; both are gone.

**Alternatives considered:** (a) branching on `paint.is_loading` in the handler, which is the local conditional the one-derivation rule forbids; (b) caching the mode in a `Cell` at paint time and reading it on click, rejected because a click can arrive after a state change and before the next paint, and a stale cached mode would perform the wrong action — asking the snapshot is cheap and always current.

**Touches:** the other toolbar controls (back, forward, URL entry) still call the shell directly, exactly as on the GTK and Win32 edges. Converting them to `ChromeAction` is a separate, unasked change.

## 4. The mode's accessible name rides the TOOLTIP, not `setAccessibilityLabel`

**Chosen:** `ChromePaint::reload_stop_description` ("Reload this page" / "Stop loading this page") is assigned with `setToolTip:`, exactly as the trust badge's explanation is on this edge.

**Why:** hover IS this edge's surface for an explanatory string, and the trust badge — the only other control here that carries one — already uses it, so the collapse introduces no second convention. A VoiceOver-grade accessible name would want `setAccessibilityLabel:`, which is a different, additive change: it needs another `objc2-app-kit` feature, it applies to every control in this chrome (back, forward and ⋮ announce their glyphs today, and would be the odd ones out if only this control were fixed), and nobody here can hear what VoiceOver says, since there is no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`). Making an accessibility claim that no runner and no human can check is worse than not making it.

**Stated plainly as a gap:** this edge's toolbar controls announce glyphs, not names. That is not new to this task — it is what `◀`, `▶` and `⋮` have always done — but the collapse is the moment the gap became worth writing down. The mobile edges, which have no hover at all, DO put this same string in the platform's accessible-name slot (`contentDescription` / `accessibilityLabel`), so the core value is already the right one for whoever picks that work up here.

## 5. The button is built with an EMPTY title and painted before it is ever shown

**Chosen:** `BrowserWindow::open` creates the control as `button(mtm, "")` and lets the `controller.refresh_chrome()` at the end of construction put the mode's glyph on it.

**Why:** the alternative is a literal `"⟳"` in this file, which is exactly the kind of restated string `the_class_names_and_labels_are_never_restated_in_the_window` exists to keep out — and it would be a SECOND place the initial mode is decided, which is the drift the collapse removes. The window is painted before it is ordered on screen (and the smoke opens it far off-screen), so no user or runner ever sees the blank title. It is the same shape the Win32 sibling used, and the same one the trust badge has always used.

## 6. `ChromePaint::is_loading` is left on the carrier, now unread by BOTH desktop painters

**Chosen:** the AppKit painter no longer reads `ChromePaint::is_loading` at all (its only two uses were the enable-one-of-a-pair rule), and the field was NOT removed from `crates/desktop-paint`.

**Why:** with this task landed, neither native-widget painter reads it — the Win32 task left it deliberately, naming this task as the one that would decide. It is still deliberately kept, for a different reason than that one: removing a public field from the SHARED carrier is a carrier change, it is asserted against the core there (`assert_eq!(paint.is_loading, state.is_loading())`), and the loading FACT legitimately remains exported for a future painter that wants it. Removing it would be a change to a crate both desktop legs' CI filters watch, made by a task that was asked to change a painter. The guard here instead asserts that the AppKit `apply` does not mention `paint.is_loading`, which is the property this task actually owes: the loading fact is still carried, it is simply not this edge's to interpret.

## 7. The smoke clicks the control with `performClick:`, not by calling the controller

**Chosen:** `BrowserWindow::activate_reload_stop` sends `performClick:` to the real `NSButton`.

**Why:** it is the closest thing to a user's click that a headless runner can produce — it goes through the control's own target/action wiring, so the smoke proves `wire_actions` really pointed this button at `reloadOrStop:`, not merely that the handler works when called. (The Win32 sibling calls its `WM_COMMAND` handler directly, one step shallower, because that is where its widget layer ends.) It is also the only option that stays inside the crate's own API: `objc2`'s `define_class!` moves the method bodies into the class registration, so the Objective-C actions are not callable as Rust methods from here. The same shape the existing menu check uses (`performActionForItemAtIndex`), which has run green on the `macos-14` leg since the window landed.
