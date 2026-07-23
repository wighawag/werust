---
title: review-gate non-blocking nits for 'mobile-ipfs-scheme-interception-ios-and-android' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: mobile-ipfs-scheme-interception-ios-and-android
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-ipfs-scheme-interception-ios-and-android' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the SyncSession Mutex RECOVERS a poisoned lock (unwrap_or_else(|p| p.into_inner())) instead of propagating, so after a panic mid-borrow the edge keeps serving a possibly-inconsistent CoreSession rather than crashing. Is degraded-but-live the intended posture here?
  (crates/werust-android/rust/src/lib.rs SyncSession::with; the agent recorded the rationale (keep the edge responsive) in the KDoc and the observation note. Reasonable default; flagged only for human ratification.)
- Ratify residual risk already recorded: the Linux-only gate cannot confirm a TOP-LEVEL ipfs:// navigation reaches Android shouldInterceptRequest on a real device; the internal-https fallback is designed but unbuilt. Needs an on-device/emulator check as a follow-up.
  (work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md RESIDUAL RISK; the Rust-side capability + fail-closed routing is covered by cargo test, so this does not block the landed capability.)
