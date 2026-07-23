---
title: Decisions — mobile ipfs:// interception mechanism per platform (iOS + Android)
date: 2026-07-23
kind: observation
reviewOf: mobile-ipfs-scheme-interception-ios-and-android
---

## The per-platform interception mechanism (the settled-decision the task asked to RECORD)

The task `mobile-ipfs-scheme-interception-ios-and-android` requires recording which interception mechanism each mobile platform ended up using (native custom scheme vs internal-`https` mapping). This note is that record, linked from the done record.

### iOS: NATIVE custom scheme via `WKURLSchemeHandler` for `ipfs`

`WKWebViewShellController` registers an `IpfsSchemeHandler: WKURLSchemeHandler` on the `WKWebViewConfiguration` for the `ipfs` scheme BEFORE the `WKWebView` is created (WKWebView refuses a scheme handler set post-init). `WKURLSchemeHandler` has handled main-frame custom-scheme navigations since iOS 11, so this is the first-choice native mechanism with no fallback needed. The handler routes each intercepted request through `WerustCore.resolveIpfs` into the shared core resolve path and answers the `WKURLSchemeTask` with the verified bytes + MIME type (`.success`) or fails the task with the legible fail-closed reason (`.failure`), never rendering unverified bytes.

### Android: NATIVE custom scheme via `WebViewClient.shouldInterceptRequest`

`BrowserActivity.CoreWebViewClient` overrides `shouldInterceptRequest` and, for an `ipfs://` request, calls `WerustCore.resolveIpfs` and returns a `WebResourceResponse` with the verified bytes + MIME (success) or an HTTP `502` with an empty body (fail-closed error, also surfaced via `core.onPageFailed`). A non-`ipfs` request returns `null` so the WebView handles it normally.

### The known Android caveat + the recorded internal-`https` fallback (NOT yet needed on the evidence available)

The task's settled decision flags a KNOWN Android edge case: a TOP-LEVEL (main-frame) `ipfs://` navigation may not fire `shouldInterceptRequest` at all on some System WebView versions (the WebView can raise `ERR_UNKNOWN_URL_SCHEME` before giving the hook a chance), even though `shouldInterceptRequest` reliably fires for sub-resource and intercepted-origin requests. The settled fallback is: keep the `.eth` name in the bar but load an internal `https://appassets.androidplatform.net/...` origin (a `WebViewAssetLoader`-style mapping) that the WebView WILL navigate and that fires `shouldInterceptRequest`, then map it back to `ipfs://` inside the hook. The address bar still shows the `.eth` name (the core's chrome truth, via `url_override`), so no `https`/gateway URL is shown to the user.

I implemented the NATIVE custom scheme on Android (the task's first choice) because it keeps the core/edge protocol unchanged (the core surfaces the `ipfs://<cid>` pending load exactly as desktop) and is the simplest correct path when the hook fires. I did NOT add the internal-`https` fallback in this task because:

1. The settled decision makes it a BUILD-TIME device/emulator verification ("verify which actually works on each platform during the build"), and this drive host is Linux with NO Android emulator run in the `verify` gate (the gate is a pure-Rust `cargo test`; the Gradle/APK legs are release-time, per the release task's forward-note). So I could not run the runtime experiment that decides whether the fallback is actually needed.
2. Adding a speculative internal-`https` remap that the core would have to surface would change the core/edge pending-load protocol and history/parity for a contingency that may never trigger — premature and reversible-only-with-churn.

RESIDUAL RISK / follow-up: a human (or a follow-up task) should run the Android app on a device/emulator and confirm a top-level `ipfs://<cid>` navigation reaches `shouldInterceptRequest`. If it does NOT, switch Android to the internal-`https://appassets` mapping described above (the code site is `BrowserActivity.CoreWebViewClient.shouldInterceptRequest`, and the mapping would live at the Kotlin edge translating the core's pending `ipfs://<cid>` load into the internal origin and back). This is the one piece the Linux-only gate cannot settle; it does not block the Rust-side capability (real `register_scheme_handler` + shared-core routing + fail-closed parity), which the `cargo test` gate DOES cover.

## Android thread-safety boundary (the requeue Gate-2 fix, 2026-07-23)

The first attempt shipped the Android interception but ran `shouldInterceptRequest` on a WebView WORKER thread while the UI thread independently drove the SAME `CoreSession` during an in-flight load. Both paths reach the backend's `Rc<RefCell<Inner>>` via `borrow_mut` (`resolve_ipfs` from the worker; `navigate`/`on_page_*` from the UI thread), so two live `&mut CoreSession` across threads plus a non-atomic `RefCell` borrow = a data race / UB / a `RefCell` already-borrowed panic. Desktop and iOS are sound because their scheme handlers dispatch on the single main/GTK thread; Android alone breaks that single-thread assumption.

FIX (chosen mechanism): a `SyncSession` synchronization boundary at the Android FFI edge — a `std::sync::Mutex<CoreSession>` that every JNI export locks before touching the session. The JNI handle now points at a `SyncSession` (not a bare `CoreSession`); each `Java_...native*` export takes `&SyncSession` and locks, so the UI-thread drive and the worker-thread resolve are serialized and the shared `RefCell` is never borrowed by two threads at once. This is the same single-thread invariant desktop/iOS get for free from their single-threaded dispatch, made explicit on the one edge that needs it.

Soundness of a `Mutex` over a `!Send` session: `CoreSession` is `!Send` (it owns `Rc`s), so `SyncSession` is `!Send`/`!Sync` at the type level and the JNI layer crosses only a raw `jlong` pointer (never a typed `Send` reference). The runtime `Mutex` provides the mutual exclusion + happens-before; the `Rc` refcounts are only ever touched under the lock (the `Rc`s never escape the guarded session), so there is no concurrent refcount mutation. Alternatives considered and rejected: (a) marshalling the resolve onto the UI thread via a handler/`runOnUiThread` — rejected because `shouldInterceptRequest` must return the `WebResourceResponse` synchronously on the worker thread, so hopping threads would deadlock or force an async rework of the whole edge protocol; (b) making the backend `Send + Sync` by swapping `Rc<RefCell>` for `Arc<Mutex>` inside the backend — rejected because that would diverge the Android backend's interior-mutability shape from the shared `webview-renderer`/desktop `Rc<RefCell>` precedent for no gain (a single outer lock at the edge is enough and keeps the backend identical across platforms). This decision touches only the Android FFI edge (`crates/werust-android/rust/src/lib.rs` + the Kotlin `shouldInterceptRequest` KDoc); it does NOT change iOS (left single-threaded, no boundary) or the core resolve path. A host-testable `cargo test` (`the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread`) drives both edges concurrently and is proven to catch a regression: with the lock bypassed it SIGSEGVs; with the boundary it passes.

## What this touches

- `crates/werust-android` (Kotlin `BrowserActivity`, `WerustCore.kt`, Rust `backend.rs` + `lib.rs` + JNI exports).
- `crates/werust-ios` (Swift `WKWebViewShellController`, `WerustCore.swift`, Rust `backend.rs` + `lib.rs` + C-ABI exports + `werust_mobile.h`).
- `docs/platform-capability-matrix.toml` (`ipfs-render` flipped desktop/ios/android -> implemented) and its guard test.
- Does NOT touch the desktop backend, the `werust_core::ipfs` resolver, or the `fetcher` retriever (the SAME core path is reused, not forked).
