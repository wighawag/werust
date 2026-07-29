---
title: "Gate-3 conductor review: mobile-page-signal-callbacks-off-session-lock (APPROVE)"
date: 2026-07-29
status: open
reviewOf: mobile-page-signal-callbacks-off-session-lock
verdict: approve
---

## Verdict: APPROVE

Merged as `246b592`, first dispatch on kimi-k3, no recovery needed. 63 `werust-android-core` tests re-run locally green, including the three that pin the fix.

## Acceptance criteria, ticked against the merged tree

- [x] **UI-thread callbacks no longer acquire the session mutex.** `SyncSession::on_page_committed` / `on_page_finished` / `on_page_failed` / `on_url_changed` now call `self.backend.on_xxx(url)` directly — no `self.with(...)`. The doc on `on_url_changed` names the exact freeze scenario (SPA `pushState` during a `__data.json` round-trip).
- [x] **A worker-held session lock does not delay a same-document URL update.** The callbacks now record in microseconds; the chrome fold is deferred to the next pump-first `take_pending_load`/`chrome_json` read.
- [x] **The `Rc<RefCell>` borrow on the UI thread remains sound — with a forced, correct deviation.** The task prescribed cloning `Rc<RefCell>` to the UI thread, but `Rc` is `!Send` (two threads borrowing it is UB). The agent correctly upgraded `AndroidBackend`/`AndroidHandle` to `Arc<Mutex<Inner>>`, pinned by `the_shared_state_is_send_and_sync_so_the_clone_boundary_is_sound`. This is exactly the kind of prescribed-mechanism-corrected-by-implementation the review protocol exists to catch, and it was caught and ratified.
- [x] **The existing serialisation test was updated, not just re-pinned.** `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` now explicitly covers the worker-side pair (`resolve_ipfs` + `handle_provider_message` concurrently), and its doc says "the UI thread now reads the chrome through the clone-handle boundary and is NOT serialised by this test; the worker side still is". The sibling ANR guard (`the_page_signal_callbacks_never_wait_on_the_session_lock_so_a_spa_nav_cannot_anr`) is a separate test in the same file, not a replacement — exactly as the review-block required.
- [x] **The ANR regression guard exists and passes.** `the_page_signal_callbacks_never_wait_on_the_session_lock_so_a_spa_nav_cannot_anr` asserts the UI-thread callbacks return without the session lock.
- [x] **Network-isolated tests.** The `ipfs://` CID in the serialisation test is malformed so `resolve_ipfs` fails closed before any fetch; the provider bridge stub answers keylessly.

## The residual, honestly recorded (not a block)

The four callbacks are now lock-free, but Kotlin's `afterCoreAction` immediately calls `take_pending_load` + `chrome_json` on the UI thread after every signal, and those reads still go through `self.with(...)` (now pump-first, so the deferred signal fold lands there). So the URL-bar update can still wait for the remainder of ONE in-flight CAR retrieval. This is:

- **Explicitly sanctioned by the task's own prescription** ("the UI-thread page-signal callbacks become the only lock-free path").
- **Honestly recorded** in `work/notes/observations/mobile-chrome-reads-still-take-the-session-lock-2026-07-29.md` and the `SyncSession` struct doc, with a named follow-up (a chrome snapshot or clone-handle read path) if on-device verification still shows the kill-app dialog.
- **A design decision, not a bug** — a chrome read could then show a briefly stale snapshot, which is a trade-off the task correctly deferred.

## Nit triage (6 non-blocking findings)

All ratifications, none blocking:
1. **Rc→Arc upgrade** — forced by `!Send` UB, correct, pinned by test.
2. **Criterion 2 half-mechanised** — the residual above; sanctioned, recorded, follow-up named.
3. **`resolve_scheme` remove/call/reinsert** — the inner lock is never held across a multi-second CAR retrieval, preventing the inner mutex from becoming a new ANR channel. Correct defensive fix.
4. **Deferred pump** — new named concept, documented consistently in Rust and Kotlin.
5. **No PR description decisions block** — the commit body is title-only; the four decisions are in repo docs. Cosmetic.
6. **Stale test name** — `the_sync_session_exposes_the_debug_document_under_the_lock` says "under the lock" while `debug_json` reads off the lock. Cosmetic, flagged by the agent itself.

## For the human

The on-device verification is the one thing left: load `ronan.eth` on a real Android build, navigate the blog list and portfolio, and confirm the "kill app / wait?" dialog no longer appears (or appears far less frequently). The observation note and MANUAL-VERIFICATION.md in the spike dir have the exact steps. If the dialog still appears, the chrome-read residual (finding 2 above) is the next lever — a chrome snapshot read path, which is a small follow-up, not a redesign.
