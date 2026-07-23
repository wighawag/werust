---
title: review-gate non-blocking nits for 'ipns-name-resolution-and-render' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: ipns-name-resolution-and-render
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipns-name-resolution-and-render' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the display-precedence decision: an ENS ipns-ns load is flagged BOTH ens_origin AND mutable_name, and the posture computation makes NameViaTrustedRpc win over MutableName (ens_origin -> RPC warning, else mutable_name -> MutableName, else ContentVerified). So a Phase-1 ENS IPNS page shows the RPC warning today and only falls back to MutableName once Phase 2 clears ens_origin. This matches ADR 0006 and task decision 4, but it is a user-visible trust-label choice worth a human nod.
  (crates/webview-renderer/src/lib.rs posture fall-through; docs/adr/0006)
- Ratify the crate + feature-gating decision: production werust-core binds libp2p-identity verify-only (default-features=false, only peerid) while ed25519/rand are a dev-dependency for signing test fixtures. rust-ipns 0.9 and cid 0.11 are added on the same lineage the fetcher CAR path uses. Sound and well-documented (ADR 0007), but a new trust-boundary dependency worth explicit ratification.
  (crates/werust-core/Cargo.toml [dependencies] vs [dev-dependencies])
- Ratify the /ipns/ chain refusal: a verified record pointing at another /ipns/ name is refused as UnsupportedTarget rather than followed, and a record value sub-path after /ipfs/<cid> is dropped (only the CID authority is taken). Both are reasonable Phase-1 scoping choices named as follow-ons, but the sub-path-dropping is an in-scope choice the task did not spell out.
  (crates/werust-core/src/ipns.rs ipfs_uri_for_value)
