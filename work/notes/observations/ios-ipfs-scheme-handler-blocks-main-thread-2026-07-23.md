# iOS `ipfs://` scheme handler still runs the blocking retrieval on the main thread

2026-07-23, noticed while doing `ipfs-retrieval-off-main-thread-no-ui-freeze` (desktop off-thread fix).

The iOS edge has the SAME UI-freeze the desktop fix just removed, but on the Swift side. `IpfsSchemeHandler.webView(_:start:)` in `crates/werust-ios/App/Sources/WKWebViewShellController.swift` calls `core.resolveIpfs(url)` SYNCHRONOUSLY, and `WKURLSchemeHandler` start callbacks are delivered on the main thread by default, so the blocking CAR fetch + verify freezes the iOS UI per request exactly as desktop did.

Not fixed here (out of this task's desktop/Android scope). Unlike Android (whose `shouldInterceptRequest` already runs on a WebView WORKER thread and is guarded by the `SyncSession` Mutex), the iOS FFI documents a "every call on the main thread" invariant (`crates/werust-ios/rust/src/lib.rs`) with NO `SyncSession` analogue, so moving iOS retrieval onto a background `DispatchQueue` also needs an iOS session thread-safety guard (a `SyncSession`-style Mutex). That is its own task touching the iOS edge's threading contract, not the desktop change. Suggest a follow-on task: "ios ipfs retrieval off the main thread + iОС SyncSession guard", mirroring this desktop fix and the Android precedent.
