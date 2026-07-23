---
title: review-gate non-blocking nits for 'android-anr-main-thread-diagnose-and-unblock' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: android-anr-main-thread-diagnose-and-unblock
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'android-anr-main-thread-diagnose-and-unblock' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Stop runs INLINE on the UI thread (core.stop() acquires the same SyncSession mutex via with()). While a navigate/reload is blocked mid-resolve on the background executor it HOLDS that lock for up to ~30-60s, so a UI-thread Stop will block on inner.lock() for that whole window - itself an ANR window, on the one action a user reaches for during a slow load. Ratify: keep Stop inline, or route it through driveCore / make it non-blocking? (Note the synchronous core cannot actually cancel an in-flight resolve regardless.)
  (BrowserActivity.kt:116 stopButton runs core.stop() inline; SyncSession::stop -> with() -> inner.lock() (lib.rs:344), same mutex the executor holds during navigate's blocking ENS/IPNS resolve.)
- In-scope decisions the agent made but did not record in a ## Decisions block (PR body is empty) - please ratify: (1) single-thread executor => a user's rapid nav actions SERIALISE in submitted order rather than latest-wins; (2) Stop stays inline on the UI thread while all other drivers go off-thread; (3) onDestroy uses shutdown (not shutdownNow) so an in-flight action finishes before the native session closes; (4) launch navigate(START_URL) is dispatched off-thread even though START_URL is https:// (no resolve) - defensive, framed for a hypothetical .eth start URL.
  (BrowserActivity.kt:111-116,125,227,242 (driveCore + single-thread executor), onDestroy shutdown; no ## Decisions block in the commit body.)
