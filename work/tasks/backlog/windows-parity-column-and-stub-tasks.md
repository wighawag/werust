---
title: "Add the `windows` column to the platform-capability parity matrix, and cut the stub tasks the guard then forces"
slug: windows-parity-column-and-stub-tasks
blockedBy: [windows-win32-window-and-chrome]
covers: []
---

## What to build

The Windows twin of `macos-parity-column-and-stub-tasks`, and sub-task 4 of the Windows split in `docs/adr/0011-webview2-for-windows.md`. It runs AFTER the shell lands, deliberately, so every cell describes what really shipped rather than what was planned.

> DRIFT UPDATE (conductor, 2026-07-31): `macos-parity-column-and-stub-tasks` LANDED after this task was written, so the file now reads `platforms = ["desktop", "macos", "ios", "android"]` and the guard's expected-platform list in `crates/werust-core/tests/platform_capability_parity.rs` hardcodes those four. You are adding a FIFTH. Read the macOS column before writing yours: it is the freshest worked example of the honesty standard (18 `implemented`, 2 `stubbed` at real authored tasks, 1 `n-a` whose reason names its platform analogue instead of hiding it), and the two columns will be read side by side. Add `windows` to the guard's expected list too, for the same reason macOS is there: so a later change cannot quietly drop the column and take every gap it tracks with it. Note also `work/notes/observations/desktop-platform-key-now-means-linux-only-2026-07-31.md`: the `desktop` key means the Linux/GTK edge, and your column is the THIRD desktop. Do not rename it as a side-effect; that decision has other owners.

`docs/platform-capability-matrix.toml` lists `platforms = ["desktop", "macos", "ios", "android"]`, and the guard (`crates/werust-core/tests/platform_capability_parity.rs`, riding the normal `verify` gate) requires an EXPLICIT cell for every capability times every platform. Adding `windows` therefore forces a cell in EVERY capability row, each of which must be `implemented`, or `stubbed` with a `task = "<slug>"` naming a task that really exists in `work/tasks/{backlog,ready,done}/`, or `n-a` with a `reason`. A `stubbed` cell with no resolvable task REDS the gate, which is the whole point of the mechanism.

So this task is two halves: fill the column honestly against what `windows-webview2-renderer-backend` + `windows-win32-window-and-chrome` actually shipped, and CUT the follow-on tasks that the `stubbed` cells must point at. Do not paper over a gap with `n-a`: `n-a` means genuinely not applicable on that platform (as "system-back-navigates-history" is on desktop), not "not built yet".

**Read the two Windows spike READMEs before filling a single cell**, because they already state, honestly, what is proven and what is not: [`docs/spikes/windows-webview2-renderer-backend/README.md`](../../docs/spikes/windows-webview2-renderer-backend/README.md) and [`docs/spikes/windows-win32-window-and-chrome/README.md`](../../docs/spikes/windows-win32-window-and-chrome/README.md). Several cells are already decided by them, and getting them wrong would be a claim the spike itself contradicts. In particular:

- `web-inspector` IS implemented (Edge DevTools via `OpenDevToolsWindow`, F12, gated on a debug build) — but say where it is reached from, as the other rows do.
- The debug view's NETWORK capture is shim-only on Windows exactly as it is on macOS and iOS (page `fetch`/`XHR` only, not browser-internal subresource loads). If the matrix's row claims more than that, the honest cell is `stubbed` pointing at a task to wire `AddWebResourceRequestedFilter("*")` or the DevTools protocol — author it.
- Anything about rendering quality, input, focus, HiDPI or window management is UNMEASURED (CI is not hardware). A capability row that depends on one of those must not be marked `implemented` on the strength of a CI run.

Read `docs/adr/0005-platform-capability-parity-guard.md` first: the guard exists because the mobile `ipfs://` gap once shipped desktop-only behind an empty `{}` backend method and the release still looked green.

ADR sizing: 1 to 2 person-days, plus whatever the forced stub tasks themselves are worth (author them, do not build them here).

## Acceptance criteria

- [ ] `platforms` includes `windows`, and EVERY capability row carries an explicit Windows cell.
- [ ] Each cell is honest against the shipped Windows edge: `implemented` only where it really works (and is proven by something, named), `stubbed` with a real task slug where it does not, `n-a` with a reason only where the capability genuinely cannot apply on Windows.
- [ ] No cell claims a capability the spike READMEs list as unmeasured or unwired.
- [ ] Every `stubbed` cell's `task` resolves to a task file that exists (the guard enforces this; the tasks are authored by THIS task).
- [ ] The parity test passes as part of the normal `verify` gate, with no weakening of the guard itself.
- [ ] The authored stub tasks are real tasks (scoped, with acceptance criteria), landing in `work/tasks/backlog/` for human review, not placeholders.

## Prompt

> Goal: add the `windows` platform column to `docs/platform-capability-matrix.toml` and cut the follow-on tasks the parity guard then forces. Adding a platform forces an explicit cell in every capability row: `implemented` where the Windows edge really does it, `stubbed` with a `task = "<slug>"` that really exists where it does not, `n-a` with a `reason` only where it genuinely cannot apply. Fill it from what the two Windows spike READMEs (`windows-webview2-renderer-backend`, `windows-win32-window-and-chrome`) say is PROVEN — they are deliberately explicit about what CI measured versus what awaits real hardware, so do not mark a rendering/input/focus/HiDPI-dependent row `implemented` on the strength of a CI run, and do not claim more network capture than the shim-only reality. Author the stub tasks (scoped, with acceptance criteria, landing in `work/tasks/backlog/`) rather than pointing at nothing, since an unresolvable stub reds the gate by design. Read `docs/adr/0005-platform-capability-parity-guard.md` first: this guard exists because a whole capability once shipped desktop-only behind an empty backend method with a green release. Do not weaken the guard, and do not use `n-a` to mean "not built yet".
