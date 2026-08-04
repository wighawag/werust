---
title: The `windows` cell of `collapsed-reload-stop-and-loading-spinner` still reads `stubbed` after its task landed
date: 2026-08-04
status: open
---

Spotted while flipping the `macos` cell of `collapsed-reload-stop-and-loading-spinner` in `docs/platform-capability-matrix.toml` (task `reload-stop-collapse-and-spinner-on-the-macos-window`): the `windows` cell of that same row still reads `windows = { state = "stubbed", task = "reload-stop-collapse-and-spinner-on-the-windows-chrome" }`, although that task is DONE (commit `a68e7b7`, body at `work/tasks/done/reload-stop-collapse-and-spinner-on-the-windows-chrome.md`, Gate-3 APPROVE, verified on a real Windows runner). The parity guard stays green because a `stubbed` cell only has to name a task that exists in `backlog`/`ready`/`done`, so a landed-but-unflipped cell is invisible to it — the row now under-reports Windows.

Not fixed here: it is another edge's residue and outside this task's scope.
