# The macOS collapse and the spinner: what is proved, and what a human still has to look at

Task `reload-stop-collapse-and-spinner-on-the-macos-window`, spec `chrome-conventional-controls` (stories 8, 9, 10, 14). The judgement calls are in [`DECISIONS.md`](DECISIONS.md) beside this file; the shared derivation every edge reads is `werust_core::reload_stop_control` / `load_spinner_visible`, whose own decisions are in `docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md`.

**Read this first.** Nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so a "looks right" check is not available for this change and never will be. Everything below is split by what proves it: the ordinary Ubuntu `verify` gate, the local from-Linux type-check harness, and the `macos-14` leg. What nothing has checked is listed under [What still awaits a Mac](#what-still-awaits-a-mac).

## The shape, in one paragraph

The AppKit toolbar had a Reload button and a Stop button, each enabled on the negation of the other's condition. It now has ONE button, and a loading spinner beside it. Neither value is decided here: the control's MODE (the glyph it wears, the accessible name it announces on hover, and the action a click performs) and the spinner's VISIBILITY all come off `ChromePaint`, the shared `desktop-paint` snapshot of the core's derivation. What this edge contributes is what a painter contributes: an `NSButton` whose title is re-assigned, AppKit's own indeterminate `NSProgressIndicator` shown or hidden, and two rectangles in the hand-computed toolbar row. Back and forward are untouched (story 14: desktop keeps its history buttons).

## What CI proves

The Ubuntu `verify` gate (pure Rust, no Xcode, no SDK), through `crates/werust-macos/tests/macos_window_shape.rs`:

* the chrome carries ONE `reload_stop: Retained<NSButton>` and a `spinner: Retained<NSProgressIndicator>`, and the pre-collapse `reload` / `stop` pair is gone;
* the paint is a straight assignment from the snapshot (`paint.reload_stop_label`, `paint.reload_stop_description`, `paint.spinner_visible`) and mentions the raw loading fact NOWHERE — the old `self.stop.setEnabled(paint.is_loading)` / `self.reload.setEnabled(!paint.is_loading)` rule cannot come back beside the derived value;
* `reload_stop_control(` and `load_spinner_visible(` are on the AppKit half's forbidden-call list beside `status_line(` and the rest, so this edge cannot call the core's rules directly instead of reading the carrier the gate compiles and tests;
* a click performs the MODE's own `ChromeAction` through the SAME `perform_chrome_action` the keyboard and the mouse side buttons use, and the handler names neither `.reload()` nor `.stop()` itself;
* the spinner is AppKit's own spinning indicator and adds NO timer (`scheduledTimerWithTimeInterval` still appears exactly once: the one 50ms chrome pump);
* back and forward are untouched and still read the core's capability flags (story 14);
* the smoke really drives the new readbacks against the real window, and compares them against the CORE's mode vocabulary rather than a hardcoded glyph.

The LOCAL from-Linux type-check (`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh`) compiles the AppKit half and the smoke for `aarch64-apple-darwin` under clippy, so every message send, `define_class!` arm and objc2 signature in this change type-checks before CI sees it. It uses a STAND-IN core, so it proves the AppKit wiring, not agreement with the real derivation (that is `desktop-paint`'s job, on the Ubuntu gate).

The `macos-14` leg (`.github/workflows/macos-renderer.yml`), through `examples/window_smoke.rs`, on a REAL off-screen window:

* a settled page really shows the RELOAD mode's glyph and the core's accessible name, read back off the real `NSButton` and its real tooltip, with the spinner hidden;
* the control really becomes STOP and the spinner really shows while a load is in flight;
* activating the control with AppKit's own `performClick:` (so the real target/action wiring is exercised) really CANCELS that in-flight load, and the toolbar returns to RELOAD with the spinner gone — the cancel affordance survives the collapse;
* activating it again on the settled page really RELOADS, watched at the pinned fixture's retrieval count rather than at a load state that cannot tell a reload from "already loaded";
* Escape with the page focused still cancels an in-flight load, so the keyboard route is intact too (story 5, asserted by the pre-existing discriminating Escape pair);
* the page view's frame is unchanged across the whole sequence: neither the collapsed control nor the spinner ever displaces the page.

## What still awaits a Mac

1. **That the spinner reads as a spinner.** CI can see the indicator shown and hidden; it cannot see it turn. AppKit owns the animation, so there is little to get wrong, but nobody here has watched one load.
2. **That one control reads as one control.** The whole point of the collapse is that a user stops hunting for a separate Stop. Nobody can measure that from a runner.
3. **The layout at real sizes.** The spinner's 20pt slot and 16pt indicator are new design metrics. A human should confirm the spinner sits between the control and the URL bar without crowding either, and that the button's glyph is centred in its 36pt square in both modes (`⟳` and `✕` are different widths).
4. **Appearance.** The indicator is expected to follow the OS light/dark appearance for free (`docs/adr/0009`: this window sets no `NSAppearance` at all). Unverified visually.
5. **VoiceOver.** The mode's words are on the TOOLTIP, this edge's hover surface; the control's accessible NAME is still its glyph, as it is for `◀`, `▶` and `⋮`. See [`DECISIONS.md`](DECISIONS.md) §4 — this is a stated gap, not an oversight.

## Manual verification

On a real Mac, with `werust-macos`:

1. **On a settled page**: exactly one control between Forward and the URL bar, showing `⟳`. Hover it: the tooltip reads "Reload this page". Nothing spins.
2. **Start a slow load** (a large page, or an `ipfs://` CID that has to be fetched). The SAME control becomes `✕`, its tooltip becomes "Stop loading this page", and the spinner beside it turns for as long as the load runs — including during the pre-content name-resolution window, where the URL bar's own progress strip has little to say yet.
3. **Click it while the load runs**: the load stops, the control goes back to `⟳`, and the spinner stops and disappears. The page does not move or resize at any point in steps 2–3.
4. **Press Escape over the page** during another slow load: the same cancel, from the keyboard. Then **Cmd+R** on the settled page: it reloads, and the control flips to `✕` for the duration.
5. **The URL bar's progress strip is unchanged** (story 9): it still advances inside the bar and still takes no height from the page.
6. **Back and forward are unchanged** (story 14): still there, still greyed at the ends of history.
7. **Switch the system appearance** between light and dark with a load running: the spinner follows the OS, and nothing in the chrome forces an appearance.
