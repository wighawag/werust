---
title: review-gate non-blocking nits for 'prominent-load-failure-and-ipns-resolution-diagnosis' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: prominent-load-failure-and-ipns-resolution-diagnosis
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'prominent-load-failure-and-ipns-resolution-diagnosis' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the V2-only fix is a wire-normalization SHIM (copy CBOR-signed mirror fields into the deprecated protobuf fields) rather than re-implementing the V2 verifier or forking rust-ipns 0.9.0. All crypto stays in rust-ipns/libp2p-identity per ADR-0001; the shim copies only V2-signature-covered values and the wrong-key test proves it does not weaken verification.
  (crates/werust-core/src/ipns.rs normalize_ipns_record_bytes; finding Decisions block. Recorded and sound; noted for human ratification only.)
- Ratify: prominent failure is an ADDITIVE red banner ON TOP of the retained footer status, registered as a NEW cross-platform capability prominent-load-failure (not folded into trust-indicator), with wording 'This page failed to load: <reason>'. Deliberately kept orthogonal to the trust posture since a failure is not a trust state.
  (docs/platform-capability-matrix.toml + error_banner_* (desktop) / errorBanner* (iOS/Android). Layering is correct; noted for ratification.)
