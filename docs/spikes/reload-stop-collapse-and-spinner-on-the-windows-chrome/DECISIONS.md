# The Win32 collapse and the spinner: the decisions this task baked in

Task `reload-stop-collapse-and-spinner-on-the-windows-chrome`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind the Windows edge's half of the collapse: one control where there were two, and a loading spinner on a toolkit that ships no spinner. The core-side decisions this edge INHERITS (which predicate drives the control versus the spinner, why the mode carries its own vocabulary, why `description()` rather than `tooltip()`) are recorded once, in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md`, and are not re-argued here.

## 1. The spinner is a one-glyph `STATIC`, not comctl32's marquee progress bar

**Chosen:** the spinner is a `STATIC` control carrying one character, advanced a quarter turn per chrome pump through the four frames `◐ ◓ ◑ ◒` (`SPINNER_FRAMES`, `crates/werust-windows/src/chrome.rs`).

**Why:** Win32 has no spinner. The only indeterminate-progress affordance the toolkit offers is `msctls_progress32` with `PBS_MARQUEE`, and werust already paints a real, determinate progress bar INSIDE the URL bar a few pixels away. Putting a second BAR beside it would say two different things (how far the load got, and that it is running at all) in one visual language, which is precisely the confusion the spinner was added to remove: the point of story 8 is a signal that is legible when the bar is a bare sliver. A rotating glyph is unmistakably a different statement, and this toolbar is already a row of glyph controls (`◀ ▶ ⋮`, plus the core's own `⟳ ✕`), so it needs no new drawing machinery, no owner-draw and no bitmap resource. The frames come from the same Unicode Geometric Shapes block as the existing `◀`/`▶`, so any font that can draw this chrome at all can draw them.

**Alternatives considered:** (a) `PBS_MARQUEE`, rejected above — it also animates only with visual styles on, which this crate deliberately turns OFF for the neighbouring progress bar so `PBM_SETBARCOLOR` keeps the shared palette's blue, so the two controls would have had to disagree about theming; (b) an owner-drawn arc on a GDI `HDC`, rejected as real drawing code (and a second painting idiom) in a file whose whole discipline is straight-line assignment; (c) an animated GIF/AVI via `SysAnimate32`, rejected because it needs a binary resource in a crate that has none, and because a resource cannot follow the OS colour scheme the way a text control's `WM_CTLCOLORSTATIC` colour does.

**Touches:** the sibling AppKit task (`reload-stop-collapse-and-spinner-on-the-macos-window`) is NOT bound by this: AppKit has a real `NSProgressIndicator` in spinning style, so that edge should use its toolkit's own spinner. This decision is specifically "what to do on a toolkit with none".

## 2. The animation rides the EXISTING pump tick, not a paint and not a new timer

**Chosen:** `Chrome::spin` is called from `Controller::tick` (the window's 50ms `WM_TIMER` pump), never from `Chrome::apply`.

**Why:** two constraints meet here. `Chrome::apply` runs only when `BrowserShell::pump()` reported an event, and the load a spinner exists for is exactly the one that reports nothing for seconds — a spinner advanced by repaints would stand still through the very stall it was added to describe. And a spinner with a timer of its own would be a second cadence in a shell whose one-pump rule is asserted (`the_window_carries_every_chrome_surface_over_the_webview2_backend` pins that `SetTimer` appears once), besides being the shape the Android ANR work warns about. Riding the existing tick satisfies both: 20 frames a second, one full rotation every 200ms, and no new machinery. WHETHER the spinner shows is still not decided here — `apply` sets that from `ChromePaint::spinner_visible`, and `spin` is a no-op on a hidden control.

**Alternatives considered:** (a) advancing the frame inside `apply`, rejected above; (b) a dedicated `SetTimer`, rejected above; (c) making `tick` always repaint the whole chrome so `apply` could own the animation, rejected because it would push a `SetWindowTextW` into every control twenty times a second — the repaint-only-on-change discipline this file keeps for the URL bar's caret exists for that reason.

## 3. Where the spinner sits: after the control, before the URL bar; its slot is permanent

**Chosen:** the toolbar reads back, forward, **[reload/stop] [spinner]**, URL bar, invalid badge, trust badge, ⋮ — the same order the GTK edge landed. The spinner's rectangle is placed by `Chrome::relayout` whether or not it is showing; only its VISIBILITY follows the derivation.

**Why:** the control that ACTS on a load and the indicator that REPORTS one belong together, and the trust badge is at the far end deliberately: motion beside it would read as a claim about the page's trust, the conflation the separate-indicators ADR exists to prevent. Keeping the slot allocated is the Win32 reading of the GTK sub-decision (there, a permanently visible widget at zero opacity): Win32 lays out in absolute rectangles, so hiding the control moves nothing on its own, but reserving the slot means a future relayout cannot start making the URL bar's width depend on whether a load is running.

**Sizing:** the slot is its own design metric, `SPINNER_WIDTH = 20` at 96 DPI (`crates/werust-windows/src/dpi.rs`), narrower than a nav button because it carries one glyph and is not a click target. It goes through the DPI seam like every other rectangle, so it scales on a 150%/200% display; the layout guard would have rejected a raw pixel anyway.

## 4. A click on the collapsed control performs the MODE's own action, read off the snapshot

**Chosen:** `ID_RELOAD_STOP`'s handler calls `perform_chrome_action(controller, controller.reload_stop_action())`, where `reload_stop_action` is `ChromePaint::of(shell.chrome()).reload_stop_control.action()`.

**Why:** two properties, both asserted. The action comes from the CARRIER, so this edge never re-decides which mode the control is in (the `reload_stop_control(` call itself is on the shape guard's forbidden list for the Win32 half, exactly like `status_line(` and the rest — the carrier is the half the Ubuntu gate compiles). And it is performed by the SAME `perform_chrome_action` the keyboard's Ctrl+R and Escape and the mouse's side buttons go through, so the toolbar cancel and the keyboard cancel are one path that cannot drift apart. The pre-collapse handler had one arm per button calling the shell directly; those arms are gone.

**Alternatives considered:** (a) branching on `paint.is_loading` in the handler, which is the local conditional the one-derivation rule forbids; (b) caching the mode in a `Cell` at paint time and reading it on click, rejected because a click can arrive after a state change and before the next paint, and a stale cached mode would perform the wrong action — the snapshot is cheap and always current.

**Touches:** the other toolbar controls (back, forward, URL entry) still call the shell directly, exactly as on the GTK edge. Converting them to `ChromeAction` is a separate, unasked change.

## 5. `ChromePaint::is_loading` is left on the carrier, unused by this edge

**Chosen:** the Windows painter no longer reads `ChromePaint::is_loading` at all (its only two uses were the enable-one-of-a-pair pair), and the field was NOT removed from `crates/desktop-paint`.

**Why:** the carrier is shared with the AppKit painter, whose own collapse task (`reload-stop-collapse-and-spinner-on-the-macos-window`) has not landed yet and still reads it. Removing a field from the shared carrier is that task's business, if it turns out nobody wants it afterwards. The guard here instead asserts that the WINDOWS `apply` does not mention `paint.is_loading`, which is the property this task actually owes: the loading fact is still exported, it is simply not this edge's to interpret.
