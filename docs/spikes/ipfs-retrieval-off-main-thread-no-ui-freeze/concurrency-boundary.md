# `ipfs://` retrieval off the UI thread: the concurrency boundary at the seam

Durable reference for the desktop off-thread fix (task `ipfs-retrieval-off-main-thread-no-ui-freeze`). Full rationale + rejected options: `docs/adr/0008`.

## The problem (re-checked before building)

`crates/webview-renderer/src/backend.rs::install_ipfs` registered the `ipfs` scheme with a closure that ran `resolve_ipfs_request` SYNCHRONOUSLY. WebKitGTK invokes that closure on the single GTK main thread, once per request (main document + every sub-resource). The blocking trustless-gateway CAR fetch + per-block verify + DAG reassembly therefore froze the UI thread (GNOME's ~10s "not responding" on a real load). Confirmed still-blocking as described; no drift.

## The boundary

```
GTK main thread                         gio I/O thread pool
---------------                         -------------------
register_uri_scheme(ipfs, |request| {
  uri     = request.uri()               ┌─ retrieve_off_thread(retriever, uri)
  spawn_blocking(move || ...) ──────────┘     (blocking CAR fetch + per-block
                                               verify + DAG reassembly)
  spawn_local(async move {                     returns a Send RetrievalOutcome
    outcome = blocking.await   ◄──────── (Result<SchemeResponse, RendererError>)
    complete_ipfs_request(outcome, sink, &life)
      Ok  -> life.mark_content_verified(); request.finish(bytes)
      Err -> request.finish_error(reason)   // fail-closed, posture untouched
  })
})
```

- Only a `Send` VALUE (`RetrievalOutcome`) crosses the boundary. The `!Send` `WebKitURISchemeRequest` and the `!Send` `Rc<RefCell<LoadLifecycle>>` are captured ONLY by the `!Send` `spawn_local` future, so they never leave the main thread.
- The shared posture is mutated ONLY in `complete_ipfs_request`, on the GTK loop: the worker can never race the UI thread's posture updates (the desktop analogue of Android's `SyncSession` Mutex).
- The retriever stays synchronous (`docs/adr/0004`); it is `Arc`-shared so concurrent sub-resource workers share one connection-pooling agent.

## Where the pieces live

- `crates/webview-renderer/src/offthread.rs` — the GTK-free boundary: `retrieve_off_thread` (worker half), `RequestSink` (the completion sink abstraction), `complete_ipfs_request` (main-thread half). Exercised headlessly by three tests: retrieval-runs-off-thread + posture-marked-on-thread, verification-failure-still-fails-closed-and-never-marks, and concurrent-retrievals-do-not-serialize.
- `crates/webview-renderer/src/backend.rs::install_ipfs` — the production glue (`gio::spawn_blocking` + `MainContext::spawn_local`) and `WebKitRequestSink` (the `RequestSink` over the live request).

## Proven offline at the seam

The off-thread wiring, the verified-bytes delivery, the fail-closed-on-mismatch, the no-off-thread-posture-access, and the concurrent-non-serialization are all pinned by the `offthread` unit tests, network-isolated, with no GTK loop and no display. The end-to-end real-`WebViewRenderer` install remains an `#[ignore]` display-bound smoke test alongside the existing ones.
