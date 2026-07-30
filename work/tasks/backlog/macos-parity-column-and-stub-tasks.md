---
title: "Add the `macos` column to the platform-capability parity matrix, and cut the stub tasks the guard then forces"
slug: macos-parity-column-and-stub-tasks
blockedBy: [macos-wkwebview-backend-and-window]
covers: []
---

## What to build

Sub-task 3 of the `macos-desktop-build` split prescribed by `docs/adr/0011-webview2-for-windows.md`. It is the step the original combined task hid entirely, and the reason that task was oversized.

`docs/platform-capability-matrix.toml` lists `platforms = ["desktop", "ios", "android"]`, and the guard (`crates/werust-core/tests/platform_capability_parity.rs`, riding the normal `verify` gate) requires an EXPLICIT cell for every capability times every platform. Adding `macos` therefore forces a cell in EVERY capability row (21 today), each of which must be `implemented`, or `stubbed` with a `task = "<slug>"` naming a task that really exists in `work/tasks/{backlog,ready,done}/`, or `n-a` with a `reason`. A `stubbed` cell with no resolvable task REDS the gate, which is the whole point of the mechanism.

So this task is two halves: fill the column honestly against what `macos-wkwebview-backend-and-window` actually shipped, and CUT the follow-on tasks that the `stubbed` cells must point at. Do not paper over a gap with `n-a`: `n-a` means genuinely not applicable on that platform (as "system-back-navigates-history" is on desktop), not "not built yet".

Read `docs/adr/0005-platform-capability-parity-guard.md` first: the guard exists because the mobile `ipfs://` gap once shipped desktop-only behind an empty `{}` backend method and the release still looked green.

ADR sizing: 1 to 2 person-days, plus whatever the forced stub tasks themselves are worth (author them, do not build them here).

## Acceptance criteria

- [ ] `platforms` includes `macos`, and EVERY capability row carries an explicit macOS cell.
- [ ] Each cell is honest against the shipped macOS edge: `implemented` only where it really works, `stubbed` with a real task slug where it does not, `n-a` with a reason only where the capability genuinely cannot apply on macOS.
- [ ] Every `stubbed` cell's `task` resolves to a task file that exists (the guard enforces this; the tasks are authored by THIS task).
- [ ] The parity test passes as part of the normal `verify` gate, with no weakening of the guard itself.
- [ ] The authored stub tasks are real tasks (scoped, with acceptance criteria), landing in `work/tasks/backlog/` for human review, not placeholders.

## Prompt

> Goal: add the `macos` platform column to `docs/platform-capability-matrix.toml` and cut the follow-on tasks the parity guard then forces. Adding a platform forces an explicit cell in every capability row (21 today): `implemented` where the macOS edge really does it, `stubbed` with a `task = "<slug>"` that really exists where it does not, `n-a` with a `reason` only where it genuinely cannot apply. Author the stub tasks (scoped, with acceptance criteria, landing in `work/tasks/backlog/`) rather than pointing at nothing, since an unresolvable stub reds the gate by design. Read `docs/adr/0005-platform-capability-parity-guard.md` first: this guard exists because a whole capability once shipped desktop-only behind an empty backend method with a green release. Do not weaken the guard, and do not use `n-a` to mean "not built yet".
