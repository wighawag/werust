---
title: "Gate-3 conductor review: ens-namehash-registry-resolver-contenthash-resolution (APPROVE)"
date: 2026-07-22
status: open
reviewOf: ens-namehash-registry-resolver-contenthash-resolution
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 052b0a9
---

## Verdict: APPROVE ✅ — merged to origin/main as 052b0a9 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

Conductor's own diff-vs-acceptance pass over the landed diff on origin/main. The resolution core composes the two blocking tasks: the `EthereumProvider` seam (eth_call) + the ENSIP-7 `decode_contenthash` decoder. Pure resolution logic behind the seam; no URL-bar / rendering (correctly deferred to the front-door task).

## Acceptance criteria — all met

- `namehash` computes the correct ENSIP-1 node, verified against the canonical known-answer vector (`namehash_matches_the_canonical_ensip1_known_answers`, incl. the well-known `eth` node `93cdeb70...`); also normalizes case before hashing.
- Resolution issues `registry.resolver(node)` then `resolver.contenthash(node)` as `eth_call`s through the `EthereumProvider` seam, then hands the returned bytes to task 2's `decode_contenthash` (consumes the typed output, does not re-decode). Function selectors are re-derived from their signatures via the bound keccak in a test so a typo cannot slip in.
- A known fixture name resolves end to end (pinned resolver-address + contenthash responses) to a decoded `ipfs://<cid>` reference.
- Fail-closed, distinct typed failures for every step: `UnnormalizableName`, `NoResolver` (zero resolver address), `NoContenthash` (empty), `MalformedReturn` (short/ill-shaped ABI return), `Provider(ProviderError)` (reverting resolver / RPC error), plus an unsupported-protocol named refusal from the decoder. Each has its own test; nothing is guessed or partial.
- Tests run against pinned fixture RPC responses with NO live-network dependency (a scripted fixture provider double returning canned results + capturing calldata, plus an end-to-end run over the bound RPC transport off the network).

## Vetted-crate discipline honoured

keccak256 is bound (`sha3::Keccak256`) and ENSIP-1 normalization uses `ens-normalize` — no hand-rolled crypto or Unicode normalization, matching the repo's ADR-0001 "bind vetted implementations" discipline. The mainnet ENS `REGISTRY_ADDRESS` is the canonical well-known constant.

## Drift / forward-notes honoured

- Task's READ-FIRST premise ("re-check the blocking tasks landed as assumed — the seam method shape and the decoder's output enum") honoured: the module imports `EthCall`/`EthereumProvider`/`ProviderError` and `decode_contenthash`/`DecodedContenthash`/`ContenthashError`/`ProtoCode` directly and builds on them. Conductor's own pre-dispatch freshness check independently confirmed both surfaces before dispatch.

## Gate-2 nits (non-blocking, already recorded)

Three non-blocking nits in `review-nits-ens-namehash-registry-resolver-contenthash-resolution-2026-07-22.md`, left open for human triage. None block integration; none require a re-task.
