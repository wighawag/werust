---
title: review-gate non-blocking nits for 'windows-release-packaging-leg' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-release-packaging-leg
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-release-packaging-leg' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the manifest is applied to EXAMPLES as well as bins, so the already-green windows-renderer leg's window_smoke now runs under comctl32 v6 instead of 5.82 - a cross-task change to another leg's runtime configuration that has not been re-run. If v6 changes a control's behaviour, the next Windows-touching PR goes red. Deliberate (spike decision 2), but the human should ratify the risk placement.
  (crates/werust-windows/build.rs (rustc-link-arg-examples), .github/workflows/windows-renderer.yml smoke step comment)
- Ratify: a packaging task changed shipped window BEHAVIOUR - SetWindowTheme(progress, empty, empty) opts the URL-bar progress strip out of theming so PBM_SETBARCOLOR keeps working. The reasoning (protecting the shared desktop-paint palette) is sound, but the consequence is that one control now draws unthemed beside themed neighbours, and no one has looked at it.
  (crates/werust-windows/src/window.rs:894-905; spike README decision 3)
- Criteria 7 and 8 asked for a MANUAL re-check on real Windows hardware, and no such check happened: the spike list and the parity cell were updated from documented research plus code inference, clearly labelled as unlooked-at. That matches the macOS precedent and is honest, but the human still owes manual steps 10 and 11, and nothing but the spike list tracks that residue.
  (docs/spikes/windows-release-packaging-leg/README.md - What still awaits Windows; docs/spikes/windows-win32-window-and-chrome/README.md steps 10-11)
- The new leg has never executed, not even the workflow_dispatch dry run, so criteria 1 and 4 are claims about YAML. First real exercise will be a tag or a dispatch; worth dispatching once before relying on it (a /MANIFEST:EMBED or Compress-Archive misstep would only surface there).
  (.github/workflows/release.yml windows-desktop-app; the spike README says so plainly)
- Ratify a user-visible default of the very artifact this task ships: werust-windows.exe has no windows_subsystem attribute, so double-clicking the zipped exe should pop a console window beside the browser. It is recorded only as an observation note - no fix and no task was cut. Fix here (a one-line cfg_attr) or cut the task?
  (work/notes/observations/windows-exe-opens-a-console-window-alongside-the-browser-2026-07-31.md)
- Layering nit: the guard for a WINDOW behaviour (SetWindowTheme plus PBM_SETBARCOLOR in crates/werust-windows/src/window.rs) lives in crates/werust-core/tests/release_plumbing_shape.rs. A future window edit reds a release-plumbing test, which is a surprising home for it; the window crate's own source-shape guard would be the coherent place.
  (crates/werust-core/tests/release_plumbing_shape.rs fn the_windows_progress_bar_keeps_the_shared_palette_under_visual_styles)
