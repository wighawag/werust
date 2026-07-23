---
title: review-gate non-blocking nits for 'clearer-loading-and-error-indicator' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: clearer-loading-and-error-indicator
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'clearer-loading-and-error-indicator' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify Decision 1: loading progress is a new LoadStep axis on ChromeState (Idle/ResolvingName/FetchingRecord/FetchingContent/Rendering), NOT a new LoadState or TrustPosture variant. It is driven by the real pipeline and stays orthogonal to trust. Recorded in DECISIONS.md; coherent with the glossary. Human to ratify or reverse.
  (crates/werust-core/src/lib.rs: new LoadStep enum + resolving_step field; refresh_chrome derives content step from backend load_state.)
- Ratify Decision 2: transient-vs-hard is a pure classifier FailureKind::classify over the last_error STRING, not a typed retryable flag threaded through the core. Retry reuses the existing reload (no new command). Recorded in DECISIONS.md. Human to ratify or reverse.
  (crates/werust-core/src/lib.rs: FailureKind::classify keys on hard markers first (did not verify / hash mismatch / expired) then transient markers (timeout / timed out / transport error / connection / io error).)
- Classifier gap: IpnsError::Source also covers a non-2xx gateway status and an empty body (per its doc), whose surfaced reason (IPNS record fetch failed: <detail>) carries NO transient marker, so it classifies as Hard and offers no retry - yet a 500/empty-body is often retryable. DECISIONS.md says a record-fetch source failure is Transient, but only the transport-worded subset actually is. Safe default (no false retry promise), so non-blocking, but the DECISIONS claim slightly overstates coverage.
  (crates/werust-core/src/ipns.rs:170 Source(detail) => 'IPNS record fetch failed: {detail}'; classify only matches transport/connection/timeout/io markers.)
- covers:[2] maps this loading/error-UX task to user story 2 (honest trust labelling). The mapping is loose - loading/error are orthogonal to trust - though the task correctly preserves that orthogonality. No action needed unless the human wants a tighter covers link.
  (work/tasks/done/clearer-loading-and-error-indicator.md covers:[2]; spec story 2 is the trust-label story.)
