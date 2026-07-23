---
title: "ipfs:// retrieval must not block the UI thread: move CAR fetch/verify off the main thread so the window does not freeze during a load"
slug: ipfs-retrieval-off-main-thread-no-ui-freeze
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Stop the UI freeze on an `ipfs://` load. Today the desktop `ipfs://` scheme handler (`crates/webview-renderer/src/backend.rs`, `register_uri_scheme(IPFS_SCHEME, ...)`) calls `resolve_ipfs_request` SYNCHRONOUSLY inside the GTK scheme-handler closure, which runs the full trustless-gateway CAR fetch + per-block verify + DAG reassembly ON THE GTK MAIN THREAD — and once PER REQUEST (the main document plus every sub-resource). Blocking the single UI thread on network I/O freezes the window (the human saw GNOME's ~10s "application not responding" dialog on a real load). The same pattern exists on the mobile edges (the Android worker-thread case was Mutex-guarded, but retrieval is still synchronous per request).

Move the retrieval work OFF the UI thread so the handler returns/streams without blocking the event loop: run the CAR fetch + verify on a worker thread (or a bounded pool) and complete the scheme request (`request.finish` / the mobile equivalent) when the verified bytes are ready, keeping the trust guarantees intact (bytes are still fully verified before they are handed to the renderer; a failure still fails the load). Do NOT weaken verification or fail-closed behaviour to gain speed. The retriever itself stays synchronous (no async runtime, per its ADR); the concurrency boundary is at the scheme-handler/OS-edge, marshalling the blocking call off the UI thread and delivering the result back.

Concurrency correctness is load-bearing: the shared load lifecycle / trust posture (`Rc<RefCell>` on desktop, the Mutex-guarded session on Android) must not be touched from the worker thread without the same synchronization the mobile-ipfs task established — marshal posture updates back onto the UI thread, or guard them. This is the desktop analogue of the Android thread-safety fix.

## Acceptance criteria

- [ ] An `ipfs://` load no longer blocks the UI thread: retrieval (CAR fetch + verify + reassemble) runs off the main/UI thread and the scheme request completes when the verified bytes are ready. No GNOME "not responding" freeze on a real multi-resource site.
- [ ] Sub-resource requests (css/js/images) are served without serializing the whole UI on each blocking fetch (they may run concurrently or at least not freeze the event loop).
- [ ] Trust is UNCHANGED: bytes are fully block-verified before render; a verification/retrieval failure still fails the load with its distinct reason; the trust posture is still driven by the real load path (no posture update races — marshal or guard the shared lifecycle exactly as the Android Mutex fix does).
- [ ] Applies to the desktop backend and (as far as each edge needs) the mobile backends, consistent with the shared core path.
- [ ] Tests cover the off-thread path's correctness (verified bytes delivered, failure still fails, no posture race), network-isolated. Where a real UI-thread-non-blocking assertion is impractical in a unit test, prove the retrieval-off-the-handler-thread wiring at the seam.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: kill the ~10s UI freeze on an `ipfs://` load. The scheme handler runs the blocking CAR fetch+verify+reassemble on the GTK main thread, per request; move it off-thread and complete the request when the verified bytes are ready, WITHOUT weakening verification or fail-closed behaviour. Guard/marshal the shared load-lifecycle/posture (the desktop `Rc<RefCell>` and the Android Mutex session) so the worker thread never races the UI thread's posture updates — the desktop analogue of the Android thread-safety fix already merged.
>
> Where to look: `crates/webview-renderer/src/backend.rs` (`register_uri_scheme(IPFS_SCHEME, ...)` calls `resolve_ipfs_request` synchronously; WebKitGTK lets you complete a `WebKitURISchemeRequest` asynchronously — finish it from a worker via the GTK main-context, or use the async scheme-request API). The retriever (`crates/fetcher/src/retriever.rs`) is deliberately synchronous (ADR: no async runtime) — keep it sync; the concurrency boundary is at the handler/edge. The Android Mutex/thread pattern is in `crates/werust-android` (the precedent for guarding the shared session). The posture lives in the shared `LoadLifecycle`.
>
> Done = an `ipfs://` load does not freeze the UI, sub-resources do not serialize the event loop, trust/verification/fail-closed are unchanged, no posture race, and it is proven at the seam offline. FIRST re-check the handler still blocks synchronously as described and route to needs-attention on drift. RECORD the concurrency design (where the boundary is, how posture updates are marshalled/guarded) durably — likely an ADR.
