---
title: The `windows` cell of `collapsed-reload-stop-and-loading-spinner` still reads `stubbed` after its task landed
date: 2026-08-04
status: closed
---

Spotted while flipping the `macos` cell of `collapsed-reload-stop-and-loading-spinner` in `docs/platform-capability-matrix.toml` (task `reload-stop-collapse-and-spinner-on-the-macos-window`): the `windows` cell of that same row still reads `windows = { state = "stubbed", task = "reload-stop-collapse-and-spinner-on-the-windows-chrome" }`, although that task is DONE (commit `a68e7b7`, body at `work/tasks/done/reload-stop-collapse-and-spinner-on-the-windows-chrome.md`, Gate-3 APPROVE, verified on a real Windows runner). The parity guard stays green because a `stubbed` cell only has to name a task that exists in `backlog`/`ready`/`done`, so a landed-but-unflipped cell is invisible to it — the row now under-reports Windows.

Not fixed here: it is another edge's residue and outside this task's scope.

## Closed 2026-08-04

Fixed by the drive-tasks conductor: the cell now reads `windows = { state = "implemented" }`. The claim was verified rather than assumed — `reload-stop-collapse-and-spinner-on-the-windows-chrome` is in `work/tasks/done/` with a Gate-3 APPROVE, and its collapsed-control and spinner checks pass on the real `windows-latest` runner (`the ONE reload/stop control sits on the seam's toolbar row`, `the one control becomes STOP while a load is in flight`, `the spinner shows while a load is in flight`), with `windows-renderer` green on `main`. A one-line registry correction, so it was made directly rather than tasked.
