---
title: "The Windows chrome collapses Reload/Stop into one control and shows the spinner, reading the painted snapshot"
slug: reload-stop-collapse-and-spinner-on-the-windows-chrome
spec: chrome-conventional-controls
blockedBy: [reload-stop-collapse-and-loading-spinner-core-and-gtk, shortcuts-and-mouse-history-buttons-on-the-windows-edge]
covers: [8, 9, 10, 14]
---

## What to build

The Win32 painter's half of the collapse: replace the separate Reload and Stop toolbar controls with the ONE control the core now derives, and show the spinner, both read from the `desktop-paint` snapshot rather than computed here.

This edge currently owns distinct control handles for back, forward, reload and stop, and enables or disables them from the painted snapshot (Stop only while loading, Reload only when not). That enable/disable dance is exactly what the collapse replaces: one control whose MODE comes from the derivation.

Purely a painter change. No new chrome fact is invented here; if something needed is missing from the snapshot, it belongs in the core derivation task, shared with the other edges.

## Acceptance criteria

- [ ] The Windows toolbar shows ONE control that reloads when idle and stops while loading, replacing the separate pair.
- [ ] The control's mode and the spinner's visibility are read from the painted snapshot; neither is computed in this edge.
- [ ] Cancelling an in-flight load is still possible from the toolbar and from the keyboard.
- [ ] Back and forward controls are untouched (desktop keeps them, per the spec).
- [ ] Covered by assertions in the style this crate already uses (its shape tests are exercisable from Linux; interactive parts belong in the existing Windows CI leg).
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- `reload-stop-collapse-and-loading-spinner-core-and-gtk` — it adds the derived control mode and spinner visibility to the `desktop-paint` snapshot this task reads.
- `shortcuts-and-mouse-history-buttons-on-the-windows-edge` — no logical dependency, but both edit this crate's window and chrome modules, so they are serialised to avoid a merge conflict.

## Prompt

> Goal: on the Windows edge, replace the separate Reload and Stop toolbar controls with the single mode-switching control the core derives, and show the loading spinner.
>
> Read the done record of `reload-stop-collapse-and-loading-spinner-core-and-gtk` first: it added the control mode and spinner visibility to the core derivation and to `crates/desktop-paint`, the plain-Rust snapshot this edge already consumes. This task READS those values. Inventing a local conditional for either is the drift the one-derivation rule forbids (`CONTEXT.md`, "chrome presentation / painter").
>
> Look at the Windows crate's chrome module, which owns the toolbar control handles and already enables/disables them from the painted snapshot; that enable/disable pair for reload and stop is what collapses into one control here.
>
> Keep the crate's CI reality in mind: it is package-scoped (`cargo build -p werust-windows`), never a bare workspace build, and its source-shape tests are written to be exercisable from Linux. Keep new assertions in that style so the pure-Rust gate covers them.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the snapshot actually carries the new fields before building against them.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular how a Win32 control renders a spinner (this edge has no toolkit-provided one) and where it sits.

## FORWARD-POINTER (planted by the drive-tasks conductor)

> HEADS UP: the `windows-renderer` CI leg is ALREADY RED on main when you start, and it is
> NOT your doing. The preceding task `shortcuts-and-mouse-history-buttons-on-the-windows-edge`
> added a mouse-back section to `crates/werust-windows/examples/window_smoke.rs` (~:519-540)
> and sequenced it AFTER the tampered-CID negative control. That load FAILS by design, and a
> failed navigation creates no history entry, so `can_go_back` is false and BOTH of its checks
> fail:
>
>     FAIL there is history to go back to after two loads
>     FAIL mouse button 4 navigates history back through the shell
>
> Those two, and ONLY those two, are the expected baseline failures. Every other check in that
> smoke passes. Do not treat them as caused by your change, and do not let them mask a real
> regression you introduce: at the end, the leg should fail on AT MOST those two checks.
>
> The product is correct; only the test is mis-sequenced (`window.rs:593-601` deliberately
> refuses a history move the on-screen control would refuse). Full diagnosis:
> `work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md`.
>
> You are editing that smoke file anyway to add the collapsed-control and spinner checks. You
> MAY fix the sequencing as hygiene (give the mouse section two SUCCESSFUL loads, e.g. by
> moving it before the tampered-CID control). If you do, you MUST NOT weaken it: do not relax
> the check to tolerate `can_go_back == false` and do not delete it. That check is the only
> evidence this project ever gets that mouse button 4 really navigates on Windows. If you would
> rather not touch it, leave it exactly as it is and say so in your done record.


---

### Claiming this task

```sh
dorfl claim reload-stop-collapse-and-spinner-on-the-windows-chrome --arbiter origin
git fetch origin && git switch -c work/reload-stop-collapse-and-spinner-on-the-windows-chrome origin/main
git mv work/tasks/ready/reload-stop-collapse-and-spinner-on-the-windows-chrome.md work/tasks/done/reload-stop-collapse-and-spinner-on-the-windows-chrome.md
```
