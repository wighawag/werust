# Turning the `macos-renderer` leg green: the page-focused Escape check, re-cut against the LOAD

Task `macos-smoke-blur-url-bar-does-not-end-the-field-editor`, spec `chrome-conventional-controls` (story 5 + 6). The one failing check on `main` was:

```
FAIL Escape with the PAGE focused stops the load instead of reverting the bar
window_smoke: FAIL (1 checks)
```

## The confirmed diagnosis

The task's re-scoped diagnosis was re-verified against the code before anything was touched, and it holds in full:

- `press_key -> sendEvent -> claim -> claim_key -> perform_chrome_action` is fully SYNCHRONOUS (`crates/werust-macos/src/window.rs`), and the smoke asserted on the widget immediately after the press with no pump between.
- `ChromeAction::Stop` calls `shell.stop()` and then `refresh_chrome()`, and `Chrome::apply` overwrites the URL field with `paint.url_text` whenever it differs. `ChromePaint::url_text` is verbatim `ChromeState::url_text` (`crates/desktop-paint/src/lib.rs`), i.e. the BELIEVED url.
- `ChromeAction::RevertUrlBar` writes that same believed url into the field.

So both focus branches leave the bar showing the believed url: the assertion was unreachable in BOTH states and discriminated nothing, from the day it landed. The CI failure is fully explained by the Stop repaint alone, and it is NOT evidence that `blur_url_bar` failed to blur. On the same run, `blur_url_bar()` followed by `reported_focus() == Focus::Page` PASSED twice, including once after Cmd+L had really focused the bar and installed its field editor.

`WindowController::shortcut_focus` was examined for a real user and left ALONE: it asks the CONTROL for its field editor (`currentEditor`) and falls back to the first responder, which is exactly right, because a real click into the page moves the first responder and ends the editing session.

The record of the unguarded window is `work/notes/observations/the-macos-page-focused-escape-check-was-never-discriminating-2026-08-04.md`, and the false "What CI proved" claim it created is corrected in `docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md` (item 8).

## What changed

1. **`BrowserWindow::blur_url_bar` (`crates/werust-macos/src/window.rs`)** now ends the field-editor session explicitly (`NSWindow::endEditingFor(None)`) as well as moving the first responder, and RETURNS the `BOOL` from `makeFirstResponder` instead of dropping it. Hygiene, not a fix for the observed failure.
2. **The smoke (`crates/werust-macos/examples/window_smoke.rs`)** replaces the symptom assertion with the EFFECT pair the Windows smoke already uses, plus the focus report that the repaint cannot touch:
   - the BAR half: with a load in flight, Escape reverts the edit, does NOT cancel the load, and the load goes on to settle;
   - the PAGE half: with a load in flight, the blur is asserted to succeed, `reported_focus()` is asserted to be `Focus::Page`, and the identical key press CANCELS the load.
3. **One consequence of (2), handled rather than left to chance:** the two extra in-flight loads leave real history behind them, so the side-button check further down now really starts a back navigation where before `can_go_back` was false and the gated action did nothing. The smoke therefore waits for that navigation to settle before the debug view's store-clearing check reads the capture store, so a page still running its own `console.log` cannot race it. (Making the side-button check ASSERT the navigation is deliberately NOT done here: that is the Windows sibling's open observation, `work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md`, and it is out of this task's scope.)
4. **A source-shape guard** (`crates/werust-macos/tests/macos_shortcut_shape.rs`) keeps both properties: `blur_url_bar` must end the editor and honour the `BOOL`, the pair must be told apart by `is_loading()` in both directions, and the retired `url_text() == typed` check must never come back. This is the only half of the change an Ubuntu gate can judge, so it is where the failing test was written first.

## Decisions

- **The witness is the LOAD, not the widget** (authorised by the task's re-scope). The two branches converge on identical bar text, so the widget could never separate them; an in-flight load that one branch cancels and the other leaves running is strictly stronger and genuinely discriminating. *Touches:* this smoke only. The alternative, making a bar edit survive a chrome repaint, is a cross-edge, user-visible product decision (the GTK edge behaves identically, `crates/werust/src/main.rs`) and is explicitly out of scope; production repaint behaviour is UNCHANGED.
- **Neither half pumps between `navigate` and the key press.** The macOS backend reports the load optimistically at `navigate` (`life.begin`, `crates/macos-renderer/src/backend.rs`), so the seam says "in flight" with zero run-loop turns and the press cannot race a completion. *Alternative considered:* the Windows smoke's `watch_a_load_and_cancel_it` pump loop, which exists there because that edge needs a turn to reach the loading state. Copying it here would add a race nobody on this project can watch fail. Both halves still bound their waits, and each half first ASSERTS the load is in flight, so a future change that makes `navigate` asynchronous fails loudly instead of passing vacuously.
- **`blur_url_bar` returns `bool` and is `#[must_use]`.** *Touches:* a `pub` helper on `BrowserWindow` used only by this smoke (three call sites, all now asserting it). A blur a responder silently REFUSED would leave the page half asserting against a window still in the bar half, which is precisely the failure mode this task exists to close.
- **`shortcut_focus` and the shared resolution are untouched.** The bug was in what the TEST could observe, not in what the edge reports.

## What proves it

- **On the ordinary Ubuntu `verify` gate:** the source-shape guards above (the AppKit half is `#[cfg(target_os = "macos")]` and is never compiled there), plus the whole translation table and the Cmd branch of the shared resolution, as before.
- **Locally, from Linux, without an SDK:** `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` type-checks the changed AppKit code and the changed smoke against `aarch64-apple-darwin` (including the `endEditingFor:` call), clean under `clippy -D warnings`.
- **On the `macos-14` leg, which is the only real evidence:** `cargo run -p werust-macos --example window_smoke`. The leg's push filter includes `crates/werust-macos/**`, so it re-runs on this change.

## What still awaits a Mac with a human in front of it

Unchanged by this task, and still true: that the `sendEvent:` interception really beats a page that binds Escape, and that it really beats AppKit's own `cancelOperation:` in the field editor (`docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md`). The pair added here proves the two focus branches DIFFER in effect on a real window; it does not prove werust wins a fight for the key against a page that wants it.

One RESIDUAL RISK is worth naming, because it is new: `blur_url_bar`'s returned `BOOL` is now ASSERTED, and no Linux check can predict what AppKit returns for `makeFirstResponder(nil)` on an off-screen, non-key window whose field editor has just been taken back. If AppKit answers `NO` there while the window is nevertheless in the page-focused state, the leg will fail the named check `blurring the URL bar ends its field-editor session` while `…so the window reports PAGE focus for the identical key press` passes. That combination is the signature to look for, and the answer would be to keep the `endEditingFor` hardening and report the responder move instead of asserting it. Asserting it is still the right default: a silently refused blur is precisely how this edge came to have an unguarded focus claim in the first place.

