---
title: "Gate-3 conductor review: ipns-name-resolution-and-render (APPROVE)"
date: 2026-07-23
status: open
reviewOf: ipns-name-resolution-and-render
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 0a0aa5a
---

## Verdict: APPROVE ✅ — merged to origin/main as 0a0aa5a (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green; no recovery needed)

## Acceptance criteria — all met

- A libp2p-key IPNS name resolves to its current CID via a CLIENT-VERIFIED record: the record is fetched from an untrusted endpoint (`GET /ipns/{name}?format=ipns-record`) and its signature + validity window are verified client-side against the key's `PeerId` — bound to vetted `rust-ipns` + `libp2p-identity` (ed25519/secp256k1/ecdsa/rsa), never hand-rolled crypto (`docs/adr/0001`, ADR-0007). No IPFS node.
- The resolved CID renders through the verified `ipfs://` path (reuses task 1's `ContentRetriever`), so an IPNS name pointing at a real directory site renders end to end.
- `ipns-ns` (`0xe5`) is no longer a hard refusal: it decodes to `DecodedContenthash::Ipns` and routes to `resolve_ipns_name` via the front door; every OTHER unsupported protoCode stays a named refusal.
- The NEW `TrustPosture::MutableName` ("content-verified, mutable name", `is_mutable_name()`) is added and threaded (renderer -> webview -> core -> chrome). Honesty CONFIRMED: it is never `ContentVerified`/"verified" (grep found no mislabel). The two-axis display precedence is documented: an IPNS load shows `MutableName`; an ENS ipfs-ns load shows the louder `NameViaTrustedRpc` today and falls back to `MutableName` once Phase-2 clears the RPC warning.
- Fail-closed: 11 distinct failure-path tests (invalid name pre-fetch, fetch failure, malformed record, wrong-key signature, expired record, non-ipfs target, broken/invalid target, non-2xx gateway...). DNSLink correctly deferred (not built).
- Network-isolated (35 loopback/fixture markers; `the_gateway_source_requests_the_ipns_record_format_and_resolves` asserts the request shape). ADR-0007 records the record-verification approach + the mutable-name posture.

## Two-axis trust model realised

This lands the `MutableName` posture from the settled two-axis model (`work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`): mutability is now an honest, distinct product surface, and the display-precedence rule is implemented so ENS naturally falls back to `MutableName` in Phase-2 with no rule change. The follow-on `ipns-tofu-pin-and-warn-on-change` (bless-and-warn) remains parked (needsAnswers).

## Gate-2 nits (non-blocking)

Three non-blocking nits in `review-nits-ipns-name-resolution-and-render-2026-07-23.md`, left open for human triage.
