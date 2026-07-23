---
title: "Gate-3 conductor review: android-anr-main-thread-diagnose-and-unblock (APPROVE, with a residual Stop-during-resolve ANR path flagged)"
date: 2026-07-23
status: approved
reviewOf: android-anr-main-thread-diagnose-and-unblock
gate: gate-3-conductor
mergedCommit: 10e4598
---

## Verdict: APPROVE (one real residual flagged for a follow-on: Stop stays inline)

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge. Driven in place from backlog via `dorfl do ... --allow-backlog --isolated --review --merge`. Re-ran the guard test locally. This was a diagnose-then-fix task, so I scrutinised whether the diagnosis is evidence-based (not a guess-patch) and whether the fix addresses the ACTUAL root cause.

## Done-move + landing

- `work/tasks/backlog/android-anr-main-thread-diagnose-and-unblock.md` -> `done/` on origin/main (squash merge `10e4598`).
- Files: `BrowserActivity.kt` (+73: the off-UI-thread executor + main-thread post), `crates/werust-android/rust/src/lib.rs` (+114: the background-thread-safety guard test), a `DIAGNOSIS.md` (+61), gate-2 nits note.

## The diagnosis is real (evidence-based, not assumed) - the key thing for a diagnose-task

The DIAGNOSIS traces the exact call chain and confirms it IN CODE: the Android edge drives the shared `BrowserShell` on the UI thread (`core.navigate` in the IME_ACTION_GO listener, the nav-button click listeners, and `onCreate`), and `BrowserShell::navigate` resolves an ENS name SYNCHRONOUSLY inline - `ens::resolve` makes TWO sequential blocking `eth_call` HTTP round-trips (registry.resolver, then resolver.contenthash), each on the RpcProvider's ureq transport with a 30s `DEFAULT_GLOBAL_TIMEOUT` (IPNS adds a third blocking call). So one `.eth` navigation blocks the UI thread up to ~30-60s; Android's ANR watchdog fires at ~5s -> the modal; the input queue is still serviced between blocks (bar stays typeable); the next `.eth` load blocks again -> the modal RECURS. That is exactly the reported "regularly, keeps popping, but I can still type" signature.

Critically, it RULES OUT the prime suspects I listed in the task, WITH evidence:
- The too-tight pump/Handler/Choreographer loop: RULED OUT - `BrowserActivity.kt` has no Handler/Choreographer/postDelayed/timer/frame loop; it is event-driven (chrome read once per user action or WebView signal), and there is no LoadStep POLLING on Android (the LoadStep is read in the same once-per-signal chrome refresh).
- The scheme-interception main-thread hop: RULED OUT as the cause - `shouldInterceptRequest` runs on a WebView WORKER thread, not the UI thread (it can only contend on the SyncSession mutex against the UI-thread navigate that holds it during its blocking resolve = the same root cause).
- Why the prior `ipfs-retrieval-off-main-thread-no-ui-freeze` did NOT fix this: that moved the ipfs:// content RETRIEVAL off-thread (already on the worker thread on Android); it did not touch the ENS/IPNS RESOLUTION step inside `navigate` on the UI thread. This is a DIFFERENT main-thread hop - matching the task's read-first note.

## The fix (correctly scoped, threading-only)

`BrowserActivity.kt` gains a single-thread `coreExecutor`; all four blocking session-drivers (`navigate`/`goBack`/`goForward`/`reload`) now run via `driveCore { }` on that executor, then post `afterCoreAction` (syncPendingLoad + refreshChrome, i.e. `WebView.loadUrl` + widget writes) back to the UI thread via `runOnUiThread`. `onDestroy` calls `coreExecutor.shutdown()` (lets an in-flight action finish) and closes the native session. Trust posture, load lifecycle, and ipfs://ENS verification are UNCHANGED: the SAME CoreSession methods run in the SAME order returning the SAME chrome; only the THREAD changes. This is the resolution-side twin of the retrieval-side off-thread fix, keeping the concurrency boundary at the OS edge (the core stays synchronous per ADR-0004). Correct.

## Acceptance criteria (ticked)

- [x] Root cause DIAGNOSED + recorded durably with evidence (`DIAGNOSIS.md`): the UI-thread synchronous ENS/IPNS resolve with blocking eth_calls.
- [x] The recurring ANR is fixed for the reported case: a normal/slow `.eth` load's blocking resolve no longer runs on the UI thread, so the main thread stays responsive (idles between frames) - the every-navigation ANR that recurred is removed. (Device-verified per the manual steps in the DIAGNOSIS; see the residual below for the one path not covered.)
- [x] The offending work is moved off the main thread (the executor); no busy main-thread loop existed to throttle (ruled out).
- [x] Trust/lifecycle/verification UNCHANGED (threading-only); parity preserved.
- [x] Tests cover the fix at its layer: `the_sync_session_is_safe_to_drive_from_a_background_thread` pins the SyncSession property the Kotlin executor relies on (navigate driven from a background thread while the UI thread reads chrome_json and a worker resolves ipfs://; never panics under the lock), network-isolated, rides `cargo test`. Re-ran locally: green. The device-only ANR property is covered by the recorded manual verification (ANR is a runtime property the workspace `cargo test` cannot assert - honestly scoped).

## Review-nits triage (Gate-2)

1. RESIDUAL ANR PATH (real, flagged for a follow-on, NON-blocking). `stop()` runs INLINE on the UI thread and acquires the same `SyncSession` mutex. While a `navigate`/`reload` is blocked mid-resolve on the background executor (holding the lock up to ~30-60s), a UI-thread Stop blocks on `inner.lock()` for that whole window - itself an ANR window, on the ONE action a user reaches for during a slow load. NOT a regression (Stop was inline before too) and NOT the reported recurring finding (which was every-navigation ANR, now fixed), so it does not block this task. But it is a genuine remaining gap: FOLLOW-ON = route Stop through `driveCore` (or make it non-blocking / try_lock). Note the synchronous core cannot actually CANCEL an in-flight resolve regardless, so a full Stop-during-resolve fix likely needs a cancellation story (a bigger design, candidate for the async/Helios phase). Flagged for the human.
2. Four in-scope decisions recorded here (the PR body had no `## Decisions` block): (a) single-thread executor => rapid nav actions SERIALISE in submit order (not latest-wins) - acceptable, matches a queue; (b) Stop stays inline (see nit 1); (c) onDestroy uses `shutdown` not `shutdownNow` so an in-flight action finishes before the native session closes - correct for not tearing down mid-native-call; (d) launch `navigate(START_URL)` dispatched off-thread though START_URL is https:// (no resolve) - defensive/uniform, fine. RATIFIED (a),(c),(d); (b) is the flagged residual.

## Net effect

The recurring "isn't responding" ANR on Android - which fired on every `.eth` navigation because the two blocking eth_call resolves ran on the UI thread - is fixed by moving the session-driving off the UI thread, the resolution-side twin of the earlier retrieval-side fix. One residual remains for a follow-on: Stop-during-slow-resolve still blocks the UI thread on the shared mutex (needs Stop routed off-thread and, ideally, a resolve-cancellation story). Device re-test recommended (manual steps in the DIAGNOSIS).
