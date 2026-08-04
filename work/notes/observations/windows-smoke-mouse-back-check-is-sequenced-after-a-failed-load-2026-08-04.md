---
title: "The Windows smoke's new mouse-back check is sequenced after the TAMPERED load, so its own precondition is false and windows-renderer is RED on main"
date: 2026-08-04
status: open
kind: observation
severity: red-ci-on-main
introducedBy: shortcuts-and-mouse-history-buttons-on-the-windows-edge
affects: .github/workflows/windows-renderer.yml (job windows-crates, step "Build and drive the REAL Win32 window")
---

Noticed by the drive-tasks conductor at Gate 3, checking the `windows-renderer` CI leg after `shortcuts-and-mouse-history-buttons-on-the-windows-edge` merged. The Ubuntu acceptance gate is pure-Rust and Linux-only, so it never builds the Win32 edge; this leg is the only thing that executes it, and it went from SUCCESS to FAILURE on this commit.

## The failure

```
the mouse's back side button (XBUTTON1), through the real window proc:
  FAIL there is history to go back to after two loads
  FAIL mouse button 4 navigates history back through the shell
window_smoke: FAIL (2 checks)
```

Every other check passed, including the three new KEYBOARD ones (`F5` posted through the message-loop filter, a page-focused `F5` claimed from WebView2's `AcceleratorKeyPressed`, and an unclaimed key left to the page).

## The cause: a test-sequencing bug, NOT a product bug

In `crates/werust-windows/examples/window_smoke.rs` (~:519-540) the new mouse-button section sits AFTER the negative-control load, and its comment states the assumption it gets wrong:

> There are two history entries by now (the verified page, then the control), so this one really navigates.

The "control" is the TAMPERED-CID load, which is asserted to FAIL two checks earlier (`the tampered load FAILS`, `block hash mismatch`). A failed navigation creates no session-history entry, so there is exactly ONE entry, `can_go_back` is `false`, and the precondition check fails. The second failure is a consequence: `post_x_button` is sent, then `wait_until(..., 30, ...)` spins the full 30-second timeout waiting for a back navigation that correctly never happens (note the 30s gap between the two FAIL lines in the CI log).

The product is behaving CORRECTLY. `perform_chrome_action` in `crates/werust-windows/src/window.rs:593-601` gates `ChromeAction::GoBack` on `can_go_back` and returns early when it is false, deliberately, so "a shortcut must not be able to drive a history move the on-screen control refuses". With no history, XBUTTON1 doing nothing is the specified behaviour. The smoke asserts against a precondition it never established.

## The fix

Give the check two SUCCESSFUL loads before it runs. Either move the mouse-button section to before the tampered-CID negative control (while two good loads are in history), or perform a second successful load inside the section. Do not weaken the check to tolerate `can_go_back == false`: that would delete the only evidence this project ever gets that mouse button 4 really navigates on Windows, which is exactly the story the task was written to prove.

While fixing it, also cap or shorten the 30-second `wait_until` on the failure path, so a future regression here costs seconds rather than half a minute of CI.

## Status

NOT fixed by the conductor: the task had already merged to `main` under `--merge`, so its per-item lock was released and it is not a `requeue`-able stuck item, and the conductor does not implement (that is the build agent's job). It also falls outside the ten-task scope this drive was given. Recorded here as a needs-attention item so it is not lost: `main` currently has a RED `windows-renderer` leg from this one mis-sequenced check.
