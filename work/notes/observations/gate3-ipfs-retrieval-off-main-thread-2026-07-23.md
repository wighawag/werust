---
title: "Gate-3 conductor review: ipfs-retrieval-off-main-thread-no-ui-freeze (APPROVE)"
date: 2026-07-23
status: open
reviewOf: ipfs-retrieval-off-main-thread-no-ui-freeze
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 7f57578
---

## Verdict: APPROVE ✅ — merged as 7f57578 (field-issue #1: the ~10s UI freeze)

Fixes the GNOME "not responding" freeze the human hit: the `ipfs://` scheme handler ran the full blocking CAR fetch+verify+reassemble on the GTK main thread, per request.

## Acceptance criteria — all met
- Retrieval moved off the UI thread: `gio::spawn_blocking` runs the blocking `resolve_ipfs_request` on a worker; `MainContext::spawn_local` marshals completion back to the main thread and finishes the request there. The event loop no longer blocks (new `crates/webview-renderer/src/offthread.rs`).
- Trust UNCHANGED: only the verified `SchemeResponse` + a `Send` verified-flag crosses the thread boundary; the `Rc<RefCell<LoadLifecycle>>` posture is touched ONLY on the main thread — no worker/UI race. A failure still fails closed without marking verified (`a_verification_failure_off_thread_still_fails_the_load_and_never_marks_verified`). This is the desktop analogue of the Android Mutex fix; recorded in ADR-0008.
- Sub-resources do not serialize the event loop (`concurrent_off_thread_retrievals_do_not_serialize_and_each_completes_correctly`).

## Note
This likely also improves field-issues #2/#4 (blocking per-sub-resource fetches were a probable contributor to partial styling); the render task should re-trace on top of this.

## Gate-2 nits: 3 non-blocking, recorded, left for human triage.
