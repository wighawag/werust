---
title: review-gate non-blocking nits for 'debug-capture-store-console-and-network-in-core' (Gate 2 approve)
date: 2026-07-27
status: open
reviewOf: debug-capture-store-console-and-network-in-core
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'debug-capture-store-console-and-network-in-core' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Android capture pushes are routed through the WHOLE session lock (SyncSession::push_console_entry / push_network_entry / clear_debug_capture call self.with(...)), which defeats the reason DebugCapture is a shared Arc-Mutex handle and re-couples capture to the same lock a multi-second resolve_ipfs holds on the worker thread. onConsoleMessage runs on the Android UI thread, so a page logging in a loop now takes the global session lock on the UI thread while an ipfs CAR retrieval holds it (the ANR shape spec user story 4 protects). There is no SyncSession accessor returning a CLONED DebugCapture handle, so the capture-points task has no lock-free path today. Should the follow-on task add a clone-out handle accessor (e.g. SyncSession::debug_capture_handle) and push off the session lock, or is going through the session boundary ratified?
  (crates/werust-android/rust/src/lib.rs SyncSession::push_console_entry/push_network_entry/clear_debug_capture; contrast crates/werust-core/src/debug.rs module docs which justify the Arc-Mutex precisely so a capture point needs no &mut shell. Contention is pre-existing in shape (chrome_json etc. already lock), so this amplifies rather than introduces it.)
- Un-recorded in-scope decision: the task says core-only and names the FFI surface as the READ path, but the diff also ships PUSH plumbing on the Android edge (SyncSession::push_console_entry / push_network_entry) that pre-empts the capture-points task debug-console-network-capture-per-platform. DECISIONS.md records the debug_json/clear exports but not these push wrappers or their locking choice. Ratify or move the push surface to the capture-points task?
  (crates/werust-android/rust/src/lib.rs (new SyncSession methods); docs/spikes/debug-capture-store-console-and-network-in-core/DECISIONS.md Decision 1 lists only debug_json + clear on the FFI surface.)
- The per-entry text bound is enforced only by the constructors/with_ setters, while every ConsoleEntry / NetworkEntry field is pub, so a capture point can assign entry.message = huge directly and bypass MAX_TEXT_CHARS, breaking the boundedness claim in exactly the pathological case Decision 3 exists for. Worth a private field + accessor, or a note in the capture-points task prompt?
  (crates/werust-core/src/debug.rs: pub struct ConsoleEntry / NetworkEntry with all-pub fields; truncate() applied only in new()/with_*().)
- Coherence ratification on the per-request posture: request_trust_posture returns plain ContentVerified for a verified ipfs request and never applies ADR-0006 two-axis loudest-warning rule (TrustPosture::after_verify), so on an ENS-named page the Network tab can show requests as content-verified while the chrome indicator shows name-via-trusted-rpc on the same screen. Decision 4 says the main-document entry gets its posture set explicitly by the caller, which makes this the capture-points task's responsibility. Is the per-request vs per-page split ratified, and should the follow-on task carry that obligation explicitly?
  (crates/werust-core/src/debug.rs request_trust_posture; renderer TrustPosture::after_verify (per-page rule); DECISIONS.md Decision 4.)
- Poisoned-lock behaviour is inconsistent between the gate and its report: push_network drops the entry on a poisoned lock while network_capture_enabled() reports the DEFAULT true, so the debug document would claim capture is on while nothing is captured. Low real-world likelihood (no panic-prone code in the critical sections) but the honesty framing elsewhere is strict.
  (crates/werust-core/src/debug.rs push_network (early return on lock error) vs network_capture_enabled (Err(_) => true).)
