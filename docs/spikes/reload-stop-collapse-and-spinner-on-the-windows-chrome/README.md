# The Windows collapse and the spinner: what is proved, and what a human still has to look at

Task `reload-stop-collapse-and-spinner-on-the-windows-chrome`, spec `chrome-conventional-controls` (stories 8, 9, 10, 14). The judgement calls are in [`DECISIONS.md`](DECISIONS.md) beside this file; the shared derivation every edge reads is `werust_core::reload_stop_control` / `load_spinner_visible`, whose own decisions are in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md`.

## The shape, in one paragraph

The Win32 toolbar had a Reload button and a Stop button, each enabled on the negation of the other's condition. It now has ONE control, and a loading spinner beside it. Neither value is decided here: the control's MODE (and the glyph it wears, the accessible name it announces, and the action a click performs) and the spinner's VISIBILITY all come off `ChromePaint`, the shared `desktop-paint` snapshot of the core's derivation. What this edge contributes is what a painter contributes: a `BUTTON` whose caption is re-assigned, a `STATIC` that shows or hides, a rectangle from the DPI seam, and — because Win32 ships no spinner control — four glyph frames advanced on the chrome pump that already existed.

## What CI proves

The Ubuntu `verify` gate (pure Rust, no Windows SDK), through `crates/werust-windows/tests/windows_window_shape.rs`:

* the chrome carries ONE `reload_stop` control and a `spinner`, and the pre-collapse `reload`/`stop` pair is gone;
* the paint is a straight assignment from the snapshot (`paint.reload_stop_label`, `paint.reload_stop_description`, `paint.spinner_visible`) and mentions the raw loading fact NOWHERE — the old `enable(self.stop, paint.is_loading)` / `enable(self.reload, !paint.is_loading)` rule cannot come back beside the derived value;
* `reload_stop_control(` and `load_spinner_visible(` are on the Win32 half's forbidden list beside `status_line(` and the rest, so this edge cannot call the core's rules directly instead of reading the carrier the gate compiles;
* a click performs the MODE's own `ChromeAction` through the SAME `perform_chrome_action` the keyboard and the mouse side buttons use, and the handler names neither `.reload()` nor `.stop()` itself;
* back and forward are untouched and still read the core's capability flags (story 14);
* the spinner adds no timer (`SetTimer` still appears exactly once) and its slot goes through the DPI seam like every other rectangle.

The `windows-latest` leg (`.github/workflows/windows-renderer.yml`), through `examples/window_smoke.rs`, on a REAL window:

* a settled page really shows the RELOAD mode's glyph and the core's accessible name, read back off the real `BUTTON` and the real tooltip control, with the spinner hidden;
* across a real load, sampled on every pump, the real control and the real spinner NEVER disagree with what the core derives for the state the shell is in at that instant;
* the control really becomes STOP and the spinner really shows while a load is in flight;
* activating the control (the same `WM_COMMAND` a click sends) really CANCELS that in-flight load, and the toolbar returns to RELOAD with the spinner gone;
* a real `WM_KEYDOWN` for Escape, posted into the real message loop, still cancels an in-flight load too — the collapse did not cost the keyboard route (story 5).

## What still awaits real Windows hardware

1. **That the spinner reads as a spinner.** CI can see the `STATIC` shown and hidden; it cannot see it turn, and it certainly cannot judge whether `◐ ◓ ◑ ◒` at 20 frames a second looks like motion or like a flicker. A human has to watch one load.
2. **Glyph coverage and metrics.** The frames are Geometric Shapes, like the toolbar's existing `◀`/`▶`, but only a real desktop shows whether the chosen UI font draws all four at the same width inside a 20px slot (they are centred, `SS_CENTER`, precisely so a width difference does not jitter).
3. **That one control reads as one control.** The whole point of the collapse is that a user stops hunting for a separate Stop. Nobody can measure that from a runner.
4. **The scaled layout.** The spinner's slot is a new design metric; at 150%/200% a human should confirm it still sits between the control and the URL bar without crowding either.

## Manual verification

On a real Windows desktop, with `werust-windows.exe`:

1. **On a settled page**: exactly one control between Forward and the URL bar, showing `⟳`. Hover it: the tooltip reads "Reload this page". Nothing spins.
2. **Start a slow load** (a large page, or an `ipfs://` CID that has to be fetched). The SAME control becomes `✕`, its tooltip becomes "Stop loading this page", and the spinner beside it turns for as long as the load runs — including during the pre-content name-resolution window, where the URL bar's own progress strip has little to say yet.
3. **Click it while the load runs**: the load stops, the control goes back to `⟳`, and the spinner stops and disappears. The page does not move or resize at any point in steps 2–3.
4. **Press Escape over the page** during another slow load: the same cancel, from the keyboard.
5. **The URL bar's progress strip is unchanged** (story 9): it still advances inside the bar and still takes no height from the page.
6. **Back and forward are unchanged** (story 14): still there, still greyed at the ends of history.
7. **At 150% and 200% display scaling**, repeat step 2 and check the spinner's slot and the control's square scale with the rest of the toolbar.
