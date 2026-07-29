# Manual on-device verification: UI-thread page-signal callbacks off the session lock

Durable verification record for task `mobile-page-signal-callbacks-off-session-lock` (the v0.2.7 field finding: navigating `ronan.eth` on Android periodically froze the UI thread for seconds, raising the "kill app / wait?" dialog; `ronan.eth.limo` over https did not).

## What changed (the shape under test)

- The Android backend's shared state is now an `Arc<Mutex<Inner>>` (was `Rc<RefCell<Inner>>`), and `SyncSession` holds a CLONE of the `AndroidHandle` beside the session mutex (the debug-capture clone-out precedent).
- The four UI-thread page-signal callbacks (`SyncSession::on_page_committed` / `on_page_finished` / `on_page_failed` / `on_url_changed`, called from Kotlin's `onPageStarted` / `onPageFinished` / `onReceivedError` / `doUpdateVisitedHistory`) RECORD through that clone handle and never touch the session mutex.
- Every WebView WORKER-thread caller (`resolve_ipfs`, `handle_provider_message`, and the shorter `document_start_scripts`) still goes through `self.with(...)`: the two long-lived lockers keep their serialisation against the UI/executor-thread session drive.
- The chrome fold (the shell `pump`) is DEFERRED from the callbacks to the pump-first locked reads (`take_pending_load` / `chrome_json`), which Kotlin calls immediately after every signal (`afterCoreAction`), so chrome behaviour — including the `_redirects` 3xx pending-load hand-off — is unchanged.
- `resolve_scheme` no longer holds the inner lock across the scheme-handler call (remove/call/reinsert, the `handle_script_message` precedent), so no inner lock is ever held across a CAR retrieval.

## Automated guards (network-isolated, in the verify gate)

- `the_page_signal_callbacks_never_wait_on_the_session_lock_so_a_spa_nav_cannot_anr` (`crates/werust-android/rust/src/lib.rs`): holds the session lock (the worker's mid-CAR-retrieval stand-in) and asserts each of the four UI-thread callbacks returns within 10ms, then that the deferred pump still folds the recorded signals into the chrome.
- `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` (same file): now pins the WORKER-side pair (`resolve_ipfs` + `handle_provider_message` concurrently) — the UI thread is no longer serialised by it; the worker side still is.
- `a_scheme_handler_runs_without_the_inner_lock_held` and `the_shared_state_is_send_and_sync_so_the_clone_boundary_is_sound` (`crates/werust-android/rust/src/backend.rs`): pin that no inner lock is held across a retrieval and that the clone boundary is `Send + Sync`.

## Manual device steps (acceptance: the kill-app dialog no longer appears)

The on-device reproduce is the ronan.eth blog nav that triggered the dialog on v0.2.7. On a real device (mobile network, NOT wifi — the freeze needed multi-second CAR retrievals):

1. Install the release APK built from this change (`./gradlew assembleRelease` under `crates/werust-android`, or the CI release artifact).
2. Open werust, type `ronan.eth` in the URL bar, let the blog index load (watch the trust indicator settle on "name via trusted RPC").
3. Navigate the SvelteKit client-side routes repeatedly and quickly: blog list -> a post -> back -> portfolio -> a portfolio entry -> back. These are SPA `pushState` navs (`doUpdateVisitedHistory`) with `__data.json` round-trips — the exact freeze shape.
4. While navigating, also scroll to trigger SvelteKit's viewport preloading (more in-flight retrievals holding the worker-side lock), then tap links DURING the preload.
5. PASS: no "werust isn't responding / kill app / wait?" dialog appears across several minutes of this; the URL bar follows each client-side nav; Back walks the SPA history.
6. FAIL (regression): the dialog appears, or the URL bar stops following the nav, or a Back step is lost. If the dialog STILL appears, check the residual below first.

## Recorded residual (a decision, ratified by the task's prescription)

The task prescribes that the page-signal callbacks become the ONLY lock-free path; the UI thread's pump-first READS (`takePendingLoad`, `chrome`) still take the session mutex and can therefore still wait out the remainder of an in-flight retrieval (bounded by one retrieval, and unchanged from before this task — the callbacks, which were the cumulative multi-retrieval freeze, are the part that moved off). If step 6 still reproduces the dialog on device, the next candidate is a chrome snapshot / clone-handle treatment for the reads; that is a NEW design decision (it changes what a chrome read can show: a briefly stale snapshot), out of this task's scope. Recorded in the `SyncSession` struct doc ("The deferred pump") and in `work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md`.
