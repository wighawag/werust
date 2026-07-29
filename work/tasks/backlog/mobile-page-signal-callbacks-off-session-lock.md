---
title: "The UI thread's onUrlChanged / onPageCommitted / onPageFinished go through the SyncSession mutex — they block for seconds behind a CAR retrieval, causing the Android 'kill app / wait' dialog"
slug: mobile-page-signal-callbacks-off-session-lock
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## What to build

FIELD FINDING (v0.2.7, Android, mobile network): navigating around `ronan.eth` periodically freezes the UI thread for seconds at a time, so Android shows its "kill app / wait?" dialog. The user repeatedly hits "wait" and the navigation completes. **The freeze is NOT the page's SvelteKit JS** — the same site over `ronan.eth.limo` (the https gateway) is smooth, and the gateway has the identical SvelteKit client-side nav. The difference is the path: over `ipfs://`, `shouldInterceptRequest` (a WebView WORKER thread) holds the `SyncSession` mutex during a multi-second CAR retrieval, and the UI thread's `onPageCommitted` / `onPageFinished` / `onUrlChanged` callbacks all go through the SAME mutex via `self.with(|s| s.on_xxx(url))`. So while a CAR retrieval is mid-flight, every page-lifecycle callback the WebView fires queues behind it. SPA client-side nav on `ronan.eth` is particularly exposed because `doUpdateVisitedHistory` fires per URL update and the `__data.json` round-trip keeps the worker thread holding the lock.

Fix: take the UI-thread page-signal callbacks off the session lock, the same way the debug capture reads already are (`SyncSession::debug_capture` reads off the session lock because the store is an `Arc<Mutex<_>>` clone of the shell's own data; reading through the session would put a UI-thread poll behind an in-flight retrieval — exactly the ANR shape user story 4 forbids).

Mechanism (prescribed, the codebase already has the precedent):

- `SyncSession` already separates "shell-owned state that must be served under the lock" from "shell-owned state that is reachable through a clone and so can be read without the lock". Promote the page-signal handlers to the second shape.
- `from_webview_url` (the URL map in `crates/werust-android/rust/src/origin_map.rs`) is already a pure function — NO session state, NO locks. The backend's `on_url_changed` / `on_page_committed` / `on_page_finished` are all `&self` and do at most a `from_webview_url` + a `VecDeque` push + an enum assignment on `b.inner` (`Rc<RefCell<_>>`). The `Rc<RefCell<_>>` is the same shape the debug capture already serves; the `RefCell` borrow can happen on the UI thread (it does today, behind the mutex) — the mutex is only the serialiser against the worker thread's `shouldInterceptRequest`.
- Concretely: expose `Inner` (the `Rc<RefCell<_>>` handle inside the Android backend) as a CLONE-OUT handle on `SyncSession` — `pub fn inner(&self) -> android_handle::AndroidInner`, where `AndroidInner` is `Clone` and wraps the `Rc<RefCell<...>>`. The UI-thread callbacks then `let b = inner.clone(); let url = from_webview_url(url); b.borrow_mut().state = Committed; b.borrow_mut().events.push_back(...)` — NO mutex. The worker thread's `shouldInterceptRequest` keeps its `inner.clone()` + `borrow_mut()`.
- Where to look first: `crates/werust-android/rust/src/lib.rs::SyncSession` (the mutex wrappers), `crates/werust-android/rust/src/backend.rs::AndroidInner` (the `Rc<RefCell<...>>` handle), `crates/werust-android/rust/src/origin_map.rs::from_webview_url` (the pure URL mapper, no session).
- **The worker-thread side has TWO lockers, not one.** `SyncSession::resolve_ipfs` AND `SyncSession::handle_provider_message` both run on WebView/JS-interface threads and serialise against the UI thread via `self.with(...)` for the same reason (the doc on `handle_provider_message` is explicit: "the WebView WORKER/JS-interface thread, serialised by the lock against UI thread navigate / load-signal calls exactly as `resolve_ipfs` is"). `document_start_scripts` and `debug_json` are also worker-thread readers but their body is short and not the freeze class. The PRESCRIPTION: every worker-thread caller keeps `self.with(...)`; the UI-thread page-signal callbacks become the only lock-free path. The existing `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` test must be UPDATED to explicitly cover the worker-side pair (`resolve_ipfs` + `handle_provider_message` concurrently), and its doc must say "the UI thread now reads the chrome through the clone-handle boundary and is NOT serialised by this test; the worker side still is" — the test pins the worker invariant only after the change. The new ANR-style regression guard (the one that holds the worker lock and asserts the UI thread returns <10ms) is a SIBLING test in the same file, not a replacement.

Scope + coherence:

- **DESKTOP and iOS do NOT have this exact bug** (WebKitGTK's signal model does not have the same WebView-worker-thread + UI-thread contention; WKWebView runs resource callbacks on the engine's own thread and reports main-frame load events on the main thread). The architectural change lives in the `SyncSession` shape, which is Android-only, so it ships on Android and the desktop gate continues to pin its own serialisation invariant.
- The debug capture's "off the session lock" precedent is the model; same `Arc<Mutex<_>>` clone semantics, same reasoning, same doc comment pattern.
- `from_webview_url` MUST stay a pure function — do NOT bring the URL mapping behind the lock.

What this does NOT fix: the periodic "kill app" dialog on the page's own JS work; an eventual loading banner (already filed as `loading-banner-with-phase-and-cancel`) is the right UX answer for that, while this task fixes the werust-side contention that magnifies the dialog into a real freeze. Both tasks are independent and complementary.

## Acceptance criteria

- [ ] The UI-thread callbacks `onPageCommitted`, `onPageFinished`, `onPageFailed`, and `doUpdateVisitedHistory` (the Kotlin side calling `core.onPageCommitted` / `core.onPageFinished` / `core.onUrlChanged`) no longer acquire `SyncSession`'s mutex.
- [ ] A `shouldInterceptRequest` mid-`resolve_ipfs` (the worker thread holding the session mutex) does NOT delay a same-document `onUrlChanged` on the UI thread; the URL-bar update from a SPA `pushState` completes in milliseconds regardless of any in-flight CAR retrieval.
- [ ] The `Rc<RefCell<...>>` borrow on the UI thread remains sound; the existing `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` test continues to pin the serialisation guarantee on the worker side.
- [ ] The on-device repro (the ronan.eth blog list / portfolio navigation that triggered the "kill app" dialog) no longer produces the dialog; manual verification on the real APK confirms.
- [ ] An ANR-style regression guard: a test that the UI-thread path does not call `self.with(|s| ...)` for the page-signal callbacks, or a Rust test that calls the four callbacks under an artificially-held worker lock and asserts the UI-thread call returns within 10ms.
- [ ] Network-isolated tests where testable + recorded manual device steps.

## Blocked by

- None. The on-device reproduce is already known (the v0.2.7 field test). The same-Rc clone-handle boundary already exists on `SyncSession::debug_capture`.

## Prompt

> Goal: take the Android UI-thread page-signal callbacks (`onPageCommitted` / `onPageFinished` / `onPageFailed` / `onUrlChanged`) off the `SyncSession` mutex, the same way the debug capture reads already are. The freeze is a `shouldInterceptRequest` mid-`resolve_ipfs` (worker thread holds the session mutex for seconds during a CAR retrieval) + the UI thread's same-document URL update from a SPA `pushState` blocking behind it. ronan.eth.limo (the https gateway) does not freeze because there is no worker thread holding the session mutex.
>
> IMPORTANT: there are TWO worker-thread lockers, not one — `resolve_ipfs` AND `handle_provider_message` (and `document_start_scripts` / `debug_json` for shorter-lived reads). The prescription keeps every worker-thread caller on `self.with(...)` and only takes the UI-thread page-signal callbacks off the lock; do NOT take `handle_provider_message` off the lock, that would silently lose serialisation against the UI thread during a long provider call.
>
> Where to look: `crates/werust-android/rust/src/lib.rs::SyncSession` (the mutex wrappers to change), `crates/werust-android/rust/src/backend.rs::AndroidInner` (the `Rc<RefCell<...>>` handle to expose as a clone-out), `crates/werust-android/rust/src/origin_map.rs::from_webview_url` (already a pure function — keep it that way). The debug capture's "off the session lock" precedent is the model. UPDATE the existing `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` test so it pins the worker-side pair (`resolve_ipfs` + `handle_provider_message` concurrently) AND its doc says "the UI thread now reads the chrome through the clone-handle boundary and is NOT serialised by this test; the worker side still is". ADD a sibling test in the same file that holds the worker lock artificially and asserts the UI-thread page-signal callbacks return within 10ms (the ANR regression guard). Network-isolated tests where testable + manual device steps (the on-device reproduce is the ronan.eth blog nav that triggered the kill-app dialog).
