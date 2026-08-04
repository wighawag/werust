---
title: review-gate non-blocking nits for 'shortcuts-and-mouse-history-buttons-on-the-windows-edge' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: shortcuts-and-mouse-history-buttons-on-the-windows-edge
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'shortcuts-and-mouse-history-buttons-on-the-windows-edge' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the cross-task guard relaxation: this branch edits crates/werust-core/tests/shortcut_edge_wiring_shape.rs (a file owned by the hinge task and also touched by the still-stuck macOS sibling) so the sibling-edge assertion now accepts implemented OR tracked-stub instead of tracked-stub only. It is the right shape for a landing edge, but it weakens a guard for a task that has not landed and is a likely conflict with the macOS branch. Confirm the relaxation, and that the parity guard elsewhere still catches an untracked cell.
  (crates/werust-core/tests/shortcut_edge_wiring_shape.rs the_desktop_cell_is_implemented_and_the_sibling_edges_are_tracked; docs/platform-capability-matrix.toml windows cell flipped to implemented)
- Ratify the two user-visible platform trades this edge chose (both recorded in DECISIONS.md, neither specified by the task): (a) a key werust claims over the page is marked SetHandled(true), so WebView2 stops running its OWN Ctrl+R / F12 / Alt+Arrow; (b) WM_XBUTTONUP is swallowed unconditionally so DefWindowProc cannot synthesise a second WM_APPCOMMAND from one side-button click. Both are sensible and reversible, but they change what the engine and the mouse do outside the shortcut table.
  (crates/windows-renderer/src/backend.rs wire_accelerator_keys (SetHandled); crates/werust-windows/src/window.rs WM_XBUTTONUP arm; DECISIONS.md sections 1 and 3)
- Stale doc left behind: the older spike README for windows-win32-window-and-chrome still says the F12 devtools key goes through the URL bar subclass proc, but this branch deleted that branch and ID_DEV_TOOLS (F12 is now a shared-table row reached by the loop filter or the accelerator hook). Worth a one-line correction or a pointer to the new spike doc so a reader does not chase a path that no longer exists.
  (docs/spikes/windows-win32-window-and-chrome/README.md lines ~90 and ~114 vs window.rs url_edit_proc (Enter only) and DECISIONS.md section 6)
