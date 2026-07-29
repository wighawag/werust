---
date: 2026-07-29
---

# Android: the UI thread's chrome READS still take the session lock (residual of the page-signal ANR fix)

Task `mobile-page-signal-callbacks-off-session-lock` took the four UI-thread page-signal callbacks off the `SyncSession` mutex, but — per that task's explicit prescription ("the UI-thread page-signal callbacks become the only lock-free path") — `take_pending_load` and `chrome_json` still go through `self.with(...)` (now pump-first, so the deferred signal fold lands there). Kotlin calls both synchronously on the UI thread in `BrowserActivity.afterCoreAction` right after every page signal, so during a multi-second CAR retrieval the UI thread can still block for the REMAINDER of one retrieval on those reads. If the on-device verification (`docs/spikes/mobile-page-signal-callbacks-off-session-lock/MANUAL-VERIFICATION.md`) still shows the kill-app dialog, the fix is a chrome snapshot or clone-handle read path — a new design decision (a chrome read could then show a briefly stale snapshot), not a bug in this task's change.

Also noticed: the existing test name `the_sync_session_exposes_the_debug_document_under_the_lock` (crates/werust-android/rust/src/lib.rs) says "under the lock" while `SyncSession::debug_json` is explicitly read OFF the session lock — a stale name from before the debug-capture clone-out, cosmetic only.
