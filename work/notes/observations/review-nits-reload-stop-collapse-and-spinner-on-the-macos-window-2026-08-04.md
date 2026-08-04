---
title: review-gate non-blocking nits for 'reload-stop-collapse-and-spinner-on-the-macos-window' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: reload-stop-collapse-and-spinner-on-the-macos-window
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'reload-stop-collapse-and-spinner-on-the-macos-window' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY the spinner's user-visible defaults, none of which the task specified: AppKit's own NSProgressIndicator (Spinning, Small, indeterminate), a 16pt indicator centred in a permanently allocated 20pt slot placed between the collapsed control and the URL bar, with visibility driven by setHidden rather than displayedWhenStopped.
  (docs/spikes/reload-stop-collapse-and-spinner-on-the-macos-window/DECISIONS.md sections 1-2; SPINNER_WIDTH / SPINNER_SIZE and Chrome::relayout in crates/werust-macos/src/window.rs)
- RATIFY: the mode's wording (Reload this page / Stop loading this page) rides setToolTip, so the control's VoiceOver accessible name is still its glyph. Consistent with the trust badge and with the Win32 sibling, and stated plainly as a gap, but it is an accessibility default nobody on this project can verify.
  (DECISIONS.md section 4; self.reload_stop.setToolTip(...) in Chrome::apply; README section What still awaits a Mac, item 5)
- RATIFY cross-task: only the reload/stop control was converted to the shared ChromeAction performer; back, forward and URL-enter still call the shell directly, so this edge now carries two dispatch idioms, and each click builds a full ChromePaint snapshot to read one enum. Same shape the Windows sibling ratified, so the question is whether the remaining controls should follow in a named task.
  (WindowController::reload_stop_action and the reloadOrStop: arm in crates/werust-macos/src/window.rs; DECISIONS.md section 3)
- RATIFY cross-task, second instance: ChromePaint::is_loading is now unread by BOTH desktop painters but deliberately left on the shared carrier. The Windows review deferred the removal question to this task, and this task defers it again, so it is still unowned. Should it become a backlog item against crates/desktop-paint?
  (DECISIONS.md section 6; the shape guard instead asserts Chrome::apply never mentions paint.is_loading)
- Un-recorded in-scope decision: BrowserWindow::refresh_chrome was made public purely as the smoke's sampling point, widening a production API for a test. activate_reload_stop's mechanism is recorded (DECISIONS 7) but this one is not. Ratify or fold it into the decisions file.
  (pub fn refresh_chrome(&self) in crates/werust-macos/src/window.rs, called only from examples/window_smoke.rs)
- Evidence hygiene: the matrix cell was flipped to implemented and the spike README states macos-14 results, yet no run is cited (the sibling macos-appkit README cites run 30572253620). The PR filter does include crates/werust-macos/**, so the leg does run on this PR: confirm it is green before merge, since the last two commits on main were fixes for red platform legs. The new in-flight sample also assumes navigate leaves is_loading true with no pump in between.
  (docs/platform-capability-matrix.toml macos cell; docs/spikes/reload-stop-collapse-and-spinner-on-the-macos-window/README.md section What CI proves; smoke lines around 'a load is in flight when the collapsed control is read')
- Two doc lines still describe the pre-collapse pair on macOS: the shortcuts spike README step 4 says the load stops exactly as the Stop button does, and macos-appkit-window-and-chrome README manual step 1 still lists the four-glyph toolbar and the greying rules. The latter is headed by the new superseded banner; the former is not mentioned anywhere.
  (docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md:78; docs/spikes/macos-appkit-window-and-chrome/README.md:105)
- Chrome::apply calls startAnimation on every repaint while a load runs, not only on the idle-to-loading transition. Assumed idempotent in AppKit and unverifiable here; worth a line in the awaits-a-Mac list if a repeated call can visibly restart the rotation.
  (the unsafe block after self.spinner.setHidden(!paint.spinner_visible) in crates/werust-macos/src/window.rs)
