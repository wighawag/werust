---
title: "The Windows smoke's mouse-back check runs after the TAMPERED load, so its precondition is false and windows-renderer is RED on main"
slug: windows-smoke-mouse-back-check-runs-after-a-failed-load
spec: chrome-conventional-controls
blockedBy: []
covers: [7]
---

## What to build

`windows-renderer` is RED on `main`. The Win32 smoke fails exactly TWO checks, and they are the same failure twice:

```
FAIL there is history to go back to after two loads
FAIL mouse button 4 navigates history back through the shell
window_smoke: FAIL (2 checks)
```

Everything else passes, including the three keyboard shortcut checks driven as real messages (a posted `WM_KEYDOWN` F5, a page-focused F5 through the engine's `AcceleratorKeyPressed` hook, and an unclaimed key left to the page).

## The diagnosis (already done; confirm it before fixing)

In `crates/werust-windows/examples/window_smoke.rs` (~:596-606) the mouse-back section sits AFTER the tampered-CID negative control, and its comment states the assumption it gets wrong:

> There are two history entries by now (the verified page, then the control), so this one really navigates.

The "control" is the TAMPERED load, which the smoke itself asserts FAILS two checks earlier (`the tampered load FAILS`, block hash mismatch). **A failed navigation creates no session-history entry**, so there is exactly ONE entry, `can_go_back` is `false`, and the precondition check fails. The second failure is a consequence: `post_x_button` is sent and `wait_until(..., 30, ...)` then burns its full 30-second timeout waiting for a navigation that correctly never happens (note the 30s gap between the two FAIL lines in the CI log).

The PRODUCT is correct. `perform_chrome_action` (`crates/werust-windows/src/window.rs:593-601`) gates `ChromeAction::GoBack` on `can_go_back` and returns early when it is false, deliberately, so "a shortcut must not be able to drive a history move the on-screen control refuses". With no history, XBUTTON1 doing nothing is the specified behaviour. The smoke asserts against a precondition it never established.

## Acceptance criteria

- [ ] The mouse-back section runs with TWO SUCCESSFUL loads in history. Either move it before the tampered-CID negative control, or perform a second successful load inside the section.
- [ ] Both checks pass: `can_go_back` is true at that point, and XBUTTON1 really navigates history back through the shell.
- [ ] The check is NOT weakened. Do not relax it to tolerate `can_go_back == false`, and do not delete it: it is the only evidence this project ever gets that mouse button 4 really navigates on Windows.
- [ ] The negative-control (tampered CID) checks still run and still pass, wherever they end up in the order.
- [ ] The 30-second `wait_until` on the failure path is shortened or capped, so a future regression here costs seconds rather than half a minute of CI.
- [ ] `windows-renderer` is GREEN on `main` afterwards.

## Blocked by

- None.

## Prompt

> Goal: turn the `windows-renderer` leg green by giving the mouse-back check the history it needs, without weakening it.
>
> The Ubuntu acceptance gate does NOT compile the Win32 edge, so a green local gate proves nothing here: the `windows-renderer` CI leg on `main` is the only evidence. Expect to be judged on that leg.
>
> Read the diagnosis above and CONFIRM it first. This is a test-SEQUENCING fix; resist the urge to change the product, which is behaving as specified. If you conclude otherwise, say so explicitly and show why.
>
> Full background: `work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md`.

## Gate-3 conductor verdict (drive-tasks)

APPROVE, first attempt.

Authored by the conductor to fix the RED `windows-renderer` leg left by `shortcuts-and-mouse-history-buttons-on-the-windows-edge`, whose new mouse-back section was sequenced after the tampered-CID negative control and so asserted against a precondition it never established (a failed load creates no history entry, so `can_go_back` was false).

- The mouse-back section now runs with two SUCCESSFUL loads in history. MET.
- Both previously-failing checks pass on the real `windows-latest` runner: there IS history to go back to, and XBUTTON1 really navigates back through the shell. MET.
- The check was NOT weakened: it still proves mouse button 4 genuinely navigates, which is the only evidence this project ever gets for that behaviour. MET.
- The negative-control (tampered CID) checks still run and still pass. MET.
- Guard file and `rust-toolchain.toml` NOT touched.

CI VERIFIED: `windows-renderer` SUCCESS. The leg is green again.

Three non-blocking Gate-2 nits: `work/notes/observations/review-nits-windows-smoke-mouse-back-check-runs-after-a-failed-load-2026-08-04.md`.
