---
title: "Android ANR: diagnose what blocks/starves the Android main thread and unblock it (recurring 'isn't responding' modal, UI still typeable)"
slug: android-anr-main-thread-diagnose-and-unblock
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## What to build

FIELD FINDING (v0.2.3, human, MOBILE/Android): the Android "<app> isn't responding" ANR modal fires REGULARLY. Pressing "wait" does not help - the modal keeps popping up. Notably the UI STAYS usable (the user can still type in the URL bar and interact) while the modal recurs. That signature - the WebView surface repaints and input is accepted, yet Android's ANR watchdog keeps tripping - means the Android MAIN (UI) thread is being repeatedly blocked or starved by long/looping main-thread work, not a hard total freeze.

This task is a DIAGNOSE-THEN-FIX, not a blind change. Do the diagnosis first (see `~/.agents/skills/diagnosing-bugs/SKILL.md`), record the ROOT CAUSE, then apply the minimal fix that moves the offending work off the main thread (or removes the tight loop). Do NOT guess-patch.

READ-FIRST / drift check: confirm the ANR premise still holds. `ipfs-retrieval-off-main-thread-no-ui-freeze` (done) moved the ipfs:// RETRIEVAL off the UI thread on the webview side; the ANR is Android-specific and recurs, so something ELSE is on / hammering the Android main thread. Root-cause source of the finding: `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md` (finding B).

Prime suspects to investigate (confirm which, with evidence - a main-thread stack / Perfetto / systrace / strategic logging, not assumption):
- A too-tight `pump()` / chrome-refresh loop driven from the Android UI thread (e.g. a `Handler`/`Choreographer`/timer that calls the Rust FFI `pump` + reads the chrome JSON every frame or faster), so the main thread never idles - the new v0.2.3 `LoadStep`/chrome-JSON polling may have tightened this. Look at `crates/werust-android/app/.../BrowserActivity.kt`, `WerustCore.kt`, and the FFI (`crates/werust-android/rust/src/lib.rs`, `ffi_json.rs`).
- Main-thread work inside the Android ipfs scheme interception path (a synchronous FFI call on the UI thread while a request resolves), even if the heavy retrieval is off-thread - a per-request main-thread hop that blocks.
- A busy re-render / re-layout triggered by the chrome updating on every poll even when nothing changed (no change-guard), so the UI thread churns.

Fix direction (apply only what the diagnosis proves): move the offending work off the main thread (a background executor / coroutine + post results back), and/or throttle the pump/refresh to a sane cadence with a change-guard (only repaint when the chrome actually changed - the core `pump()` already returns `true`-on-change; make the Android side honour it), and/or make the scheme-interception main-thread hop non-blocking. Keep the trust/verification and lifecycle behaviour unchanged; this is a threading/cadence fix, not a semantics change.

## Acceptance criteria

- [ ] The Android main-thread blocker/starver is DIAGNOSED and its root cause recorded durably (a spike/DIAGNOSIS note under `docs/spikes/<slug>/`), with the evidence (what was on/blocking the main thread).
- [ ] The recurring ANR is fixed: on a normal load (including a slow ipfs:// / ENS load) the Android app no longer trips the ANR watchdog; the main thread stays responsive (idles between frames), verified against the diagnosed cause.
- [ ] The offending work is moved off the main thread and/or the pump/refresh cadence is throttled with a change-guard (repaint only on actual chrome change); no busy main-thread loop remains.
- [ ] Trust posture, load lifecycle, and ipfs:///ENS verification behaviour are UNCHANGED (this is a threading/cadence fix). The platform capability parity is preserved (or tracked per the parity guard).
- [ ] Tests cover the fix at the layer it lives (e.g. the change-guard / cadence logic is unit-testable; a main-thread-work assertion where feasible), network-isolated. Where the fix is Android-runtime-only (ANR is a device/emulator property), record the manual verification steps in the DIAGNOSIS note and add the strongest automatable guard (e.g. "the pump loop repaints only on change").

## Blocked by

- None. (Highest-priority mobile issue from the v0.2.3 field test.)
