---
title: "Gate-3 conductor review: ensip7-contenthash-decoder-typed-graceful-errors (APPROVE)"
date: 2026-07-22
status: open
reviewOf: ensip7-contenthash-decoder-typed-graceful-errors
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 9ba33e4
---

## Verdict: APPROVE ✅ — merged to origin/main as 9ba33e4 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

NOTE (recovery): this task's FIRST isolated build was interrupted by a harness restart (the `do` runner process crashed mid-run). It had pushed NO work branch (crash was pre-push) but left a stranded `refs/dorfl/lock/task-ensip7-...` lock. Conductor recovered it as a fixable crash (not a decision block): confirmed no work branch existed, `dorfl requeue ensip7-... --arbiter origin` released the stranded lock (requeued for a FRESH claim, nothing to continue from), then re-dispatched `dorfl do ... --isolated --review --merge --allow-backlog`, which built clean and merged. No work lost; the crash was before any code landed.

Conductor's own diff-vs-acceptance pass over the landed diff on origin/main.

## Acceptance criteria — all met

- Pure `decode_contenthash(&[u8]) -> Result<DecodedContenthash, ContenthashError>`, dispatched by the leading multicodec protoCode varint. No network, no seam (grep confirms zero tcp/http/ureq in the module).
- `ipfs-ns` (0xe3) decodes to `DecodedContenthash::Ipfs { uri: "ipfs://<cid>" }` via `Cid::try_from(payload).to_string()` — the `cid` crate's canonical multibase form (base32 CIDv1), i.e. exactly what the verified `ipfs://` path's own `Cid::try_from(&str)` consumes. CIDv0 also decodes to its canonical string. Reuses the vetted `cid` crate rather than hand-rolling byte layout.
- `ipns-ns` (0xe5), `swarm-ns` (0xe4), Arweave, and an unknown protoCode each produce a DISTINCT, protocol-named `Unsupported(ProtoCode)` variant/message: IPNS -> "this name uses a mutable IPNS pointer, not yet supported"; unknown -> named by its raw hex code; the rest named ("points to <protocol>, not supported"). Onion/onion3/skynet/zeronet/DNSLink each named too.
- `NoContenthash` (empty) and `Malformed` (truncated/overlong varint) are distinct `ContenthashError` variants with distinct messages; a bonus distinct `InvalidCid` variant covers an `ipfs-ns` payload with un-parseable CID bytes.
- NEVER defaults an unrecognised protoCode to `ipfs://` (explicit test `the_decoder_never_defaults_a_non_ipfs_protocode_to_ipfs`), and never panics on malformed input (`a_truncated_protocode_varint_is_malformed_not_a_panic`).
- A fixture test per protoCode (ipfs-ns success incl. CIDv0; ipns-ns / swarm-ns / arweave / unknown each distinct error; onion/onion3/skynet/zeronet/dnslink named; no-contenthash; malformed; invalid-cid), all offline.

## Drift / forward-notes honoured

- Task's READ-FIRST premise ("the CID crate surface / the `ipfs://` parser shape may have moved") honoured: the agent used `Cid::try_from`/`.to_string()` to match the verified path's consumer, so the emitted `ipfs://<cid>` feeds the existing path without skew.

## Gate-2 nits (non-blocking, already recorded)

Three non-blocking nits in `review-nits-ensip7-contenthash-decoder-typed-graceful-errors-2026-07-22.md`, left open for human triage. None block integration; none require a re-task.
