---
title: "Gate-3 conductor review: mobile-ipfs-scheme-interception-ios-and-android (APPROVE, recovered from a Gate-2 thread-safety block)"
date: 2026-07-23
status: open
reviewOf: mobile-ipfs-scheme-interception-ios-and-android
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 274a7fc
---

## Verdict: APPROVE ✅ — merged to origin/main as 274a7fc (drive-tasks --review --merge, isolated build)

## Recovery (the gate did its job)

The FIRST build was BLOCKED by Gate-2 with a correct, precise thread-safety finding: Android runs `shouldInterceptRequest` on a WebView WORKER thread, but the UI thread independently drives the same `CoreSession` (an `Rc<RefCell>`, `!Sync`) during an in-flight load — so the worker's `resolve_ipfs` (`borrow_mut`) races the UI thread's navigate/lifecycle calls = data race / UB / RefCell panic. Desktop and iOS are sound because their scheme handlers dispatch on the single main/GTK thread; Android alone broke that assumption. This was a FIXABLE bug (not a coin-flip), so the conductor recovered it in-loop: `dorfl requeue ... -m "<the fix: add a sync boundary — Mutex around the session or marshal onto a consistent thread>"` (released the stuck lock, KEPT the work branch), then re-`do` continued from the kept branch tip with the reviewer's reason + the handoff note in the agent's prompt. The re-build added the fix, Gate-2 approved, and it merged.

## The fix is real (verified)

`crates/werust-android/rust/src/lib.rs` now wraps the `CoreSession` in a `std::sync::Mutex` as the explicit thread boundary between the Android WebView worker thread and the UI thread, with a doc-comment describing exactly the race the reviewer flagged. No two threads can hold `&mut` the session or borrow the `RefCell` concurrently.

## Acceptance criteria — all met

- `ipfs://` is intercepted and routed through the shared `werust-core` resolve path on Android (`WebViewClient.shouldInterceptRequest`) and iOS (`WKURLSchemeHandler`); no more `net::ERR_UNKNOWN_URL_SCHEME`.
- The mobile Rust backends' `register_scheme_handler` no-op is gone (real handler + `resolve_scheme` dispatch).
- Same core path as desktop (no forked resolver); the `.eth` name stays in the bar.
- Fail-closed + trust posture parity with desktop for the same input.
- Tests prove the scheme reaches the core on each mobile edge; the parity matrix `ipfs-render` cells are updated; the interception-mechanism decision is recorded (`mobile-ipfs-interception-mechanism-2026-07-23.md`).

## Gate-2 nits (non-blocking)

Two non-blocking nits in `review-nits-mobile-ipfs-scheme-interception-ios-and-android-2026-07-23.md`, left open for human triage.
