---
title: review-gate non-blocking nits for 'macos-smoke-blur-url-bar-does-not-end-the-field-editor' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: macos-smoke-blur-url-bar-does-not-end-the-field-editor
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-smoke-blur-url-bar-does-not-end-the-field-editor' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the author-declared residual risk: the smoke now ASSERTS the BOOL from blur_url_bar at three call sites, and no Linux check can predict what AppKit returns for makeFirstResponder(nil) on an off-screen, non-key window. If it answers NO while the window is nevertheless page-focused, the leg goes RED on the check named 'blurring the URL bar ends its field-editor session' while the focus-report check passes. Accept, or fall back to reporting the responder move instead of asserting it?
  (crates/werust-macos/examples/window_smoke.rs:330,352,454; risk named in docs/spikes/macos-smoke-blur-url-bar-does-not-end-the-field-editor/README.md)
- Second red-leg risk, not named in the Decisions block: the new BAR half asserts reported_focus() == Focus::UrlBar with NO pump after focus_url_bar()/set_url_text(), whereas the pre-existing story-6 block pumps before the same assertion. A pump is not available here (it would settle the in-flight load), so if AppKit needs a run-loop turn to install the field editor this check fails. Fallback would be to establish bar focus BEFORE starting the load.
  (crates/werust-macos/examples/window_smoke.rs:~409-417 vs the pumped block at ~366-374)
- Ratify the deliberate non-delivery on the side buttons: the two extra loads now leave real history, so the rear button really navigates, but the smoke still only asserts the button is CLAIMED and the new settle check passes vacuously when no navigation started (load_state is already Finished). The deferral points at the WINDOWS observation, which does not own the macOS smoke, so no follow-up owns asserting side-button navigation on this edge.
  (crates/werust-macos/examples/window_smoke.rs:~505-530; work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md is Windows-scoped)
- Ratify: neither half pumps between navigate and the key press, so the pair depends on the macOS backend reporting the load optimistically at navigate (life.begin) and on stop() flipping the lifecycle to Idle synchronously. Both hold today (crates/macos-renderer/src/backend.rs navigate/stop, webview-shared lifecycle stop). Recorded in the Decisions block; confirm the dependency is acceptable rather than copying the Windows pump loop.
  (crates/macos-renderer/src/backend.rs:971-996; crates/webview-shared/src/lifecycle.rs:288-292)
- Housekeeping only: the task landed in work/tasks/done/ still carrying needsAnswers: true and unchecked acceptance boxes, and the stale sidecar work/questions/task-macos-smoke-blur-url-bar-does-not-end-the-field-editor.md remains with five duplicate stuck blocks. Precedent exists for needsAnswers: true in done, so this is a sweep question for the runner, not a defect.
  (work/tasks/done/macos-smoke-blur-url-bar-does-not-end-the-field-editor.md frontmatter; work/questions/ sidecar)
- Minor teeth note: the bar half's 'does NOT cancel the in-flight load' check reads is_loading() with nothing pumped, so it can only be falsified by Stop having run. It is discriminating only in combination with the revert-text check next to it. Worth keeping in mind if either half is ever edited alone.
  (crates/werust-macos/examples/window_smoke.rs:~419-433)
