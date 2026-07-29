---
title: review-gate non-blocking nits for 'mobile-page-signal-callbacks-off-session-lock' (Gate 2 approve)
date: 2026-07-29
status: open
reviewOf: mobile-page-signal-callbacks-off-session-lock
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-page-signal-callbacks-off-session-lock' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY: the backend's shared Inner was upgraded from Rc<RefCell> to Arc<Mutex> (AndroidBackend/AndroidHandle), deviating from the task's literal prescription to clone out the Rc<RefCell> handle to the UI thread. The prescribed shape was unsound (Rc is !Send; two threads borrowing it is UB), so the deviation is forced and correct; pinned by the_shared_state_is_send_and_sync_so_the_clone_boundary_is_sound.
  (crates/werust-android/rust/src/backend.rs: AndroidBackend/AndroidHandle inner: Arc<Mutex<Inner>>; task text prescribed 'AndroidInner is Clone and wraps the Rc<RefCell<...>>')
- RATIFY: acceptance criterion 2 is only half-mechanised. The four callbacks now return off-lock, but BrowserActivity calls takePendingLoad + chrome (afterCoreAction/refreshChrome) on the UI thread immediately after every signal, and those reads still take the session mutex, so the user-visible URL-bar update can still wait out the remainder of one in-flight retrieval. The task's own prescription ('callbacks become the only lock-free path') sanctioned this; the residual is honestly recorded (SyncSession struct doc RESIDUAL, work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md, MANUAL-VERIFICATION.md) with the chrome-snapshot follow-up named if on-device verification still reproduces the dialog.
  (BrowserActivity.kt afterCoreAction -> SyncSession::take_pending_load/chrome_json both self.with(...); doUpdateVisitedHistory calls onUrlChanged then afterCoreAction)
- RATIFY: resolve_scheme now removes the scheme handler from the map for the duration of the call (remove/call/reinsert) so the inner lock is never held across a multi-second CAR retrieval. Without it the inner mutex would itself become the ANR channel. A concurrent same-scheme resolve inside the window returns None, documented as impossible in production because resolve_ipfs stays on the session lock.
  (backend.rs AndroidHandle::resolve_scheme; test a_scheme_handler_runs_without_the_inner_lock_held pins the re-entrancy)
- RATIFY: the deferred pump. Signals record (state + event + history) off-lock; the shell pump that folds them into the chrome and turns a _redirects 3xx into a pending load moved into pump-first take_pending_load/chrome_json, which Kotlin calls right after every signal. New named concept, documented consistently in Rust and Kotlin.
  (lib.rs SyncSession::take_pending_load/chrome_json now pump-first; CoreSession::pump exposed)
- No '## Decisions' block in the PR description (the commit body is title-only). The four decisions above are recorded in repo docs instead, but the PR surface should carry them for the ratifying human.
  (git log -1 fb422bb --format=%B is the title line only)
- Cosmetic stale wording the agent itself flagged but did not fix: test name the_sync_session_exposes_the_debug_document_under_the_lock says 'under the lock' while debug_json reads off the lock, and the eval_sink doc still calls AndroidHandle 'the !Send backend handle' though it is now Send+Sync.
  (lib.rs:2342; backend.rs eval_sink doc; both noted in the observation note)
