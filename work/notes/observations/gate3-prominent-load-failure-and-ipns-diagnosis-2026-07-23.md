---
title: "Gate-3 conductor review: prominent-load-failure-and-ipns-resolution-diagnosis (APPROVE) — ronan.eth diagnosed AND fixed"
date: 2026-07-23
status: open
reviewOf: prominent-load-failure-and-ipns-resolution-diagnosis
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: ac69c05
---

## Verdict: APPROVE ✅ — merged as ac69c05 (field-issue #3). Exceptional root-cause work.

## Two deliverables, both met
1. PROMINENT failure surfacing: a high-contrast in-view error banner (not just the subtle footer status the human missed) raised on any failed load with the accurate protocol-named reason. Registered as a NEW cross-platform parity capability `prominent-load-failure` — implemented on desktop + iOS + Android; zero stubbed matrix cells remain.
2. ronan.eth IPNS DIAGNOSED AND FIXED (not just diagnosed): reproduced against the LIVE network, captured the record bytes. Root cause = `rust-ipns 0.9.0` rejects V2-only IPNS records (IPIP-428, which modern gateways serve) with a spurious `DataMismatch` because it requires the deprecated protobuf mirror fields to equal the CBOR data. Fixed with a wire-normalization shim that copies the V2-signature-covered CBOR values into the protobuf mirror, keeping ALL crypto in the vetted `rust-ipns`/`libp2p-identity` (ADR-0001). ronan.eth now resolves to `ipfs://bafybeiepw4aijr4dtlhth2xkzskxaxcjvtk6neqsd6zua7rfv6m5nbkesu` (identical to ronan.eth.limo).

## Safety verified
The shim cannot smuggle an unsigned value: `a_v2_only_record_signed_by_the_wrong_key_still_fails_closed` proves the crate still enforces the real V2 signature over the unchanged CBOR data. `a_modern_v2_only_record_resolves_the_ronan_eth_failure_offline` pins the fix with a signed fixture. Expired/wrong-key records still fail closed.

## Decisions recorded (ratify-or-reverse)
- V2-only fix is a wire-normalization SHIM, not a re-implemented verifier (rejected hand-verify + fork-the-crate; ADR-0001). Upstream `rust-ipns` PR noted as a follow-on so the shim can eventually be dropped.
- The failure banner is a distinct parity capability, NOT folded into `trust-indicator` (a failure is not a trust posture; reuses the same `last_error` fact, no re-meaning).

## Impact
ronan.eth (and other V2-only IPNS names) will now render. This was a real upstream-crate bug, not a werust design flaw.

## Gate-2 nits: 2 non-blocking, recorded.
