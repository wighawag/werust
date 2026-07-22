---
title: ENS name resolution — namehash + registry→resolver→contenthash over the EthereumProvider seam
slug: ens-namehash-registry-resolver-contenthash-resolution
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [ethereum-provider-seam-and-trusted-rpc-backend, ensip7-contenthash-decoder-typed-graceful-errors]
covers: [1, 3]
---

## What to build

The ENS name-resolution logic: given a `name.eth`, produce the decoded contenthash reference (or a typed resolution failure), by composing the `EthereumProvider` seam and the ENSIP-7 decoder.

The path:
1. **`namehash`** the name (ENSIP-1 normalization + the recursive keccak256 namehash algorithm) to get the 32-byte node.
2. **`registry.resolver(node)`** — an `eth_call` through the seam to the ENS registry, returning the resolver address for the node.
3. **`resolver.contenthash(node)`** — an `eth_call` through the seam to that resolver (ENSIP-7 / EIP-1577), returning the contenthash bytes.
4. **Decode** those bytes with the ENSIP-7 decoder task's output into the typed reference/enum.

Fail-closed at every step with a legible, typed reason: an unnormalizable name, a zero/absent resolver, a resolver that reverts or returns no contenthash, or an RPC/seam error each surfaces as a distinct resolution failure — never a guessed or partial result. This task is the resolution CORE; it does NOT do URL-bar recognition or rendering (that is the front-door task). It is tested against PINNED fixture RPC responses for a known name (the seam pointed at a fixture endpoint returning canned resolver-address and contenthash results), with NO live network.

## Acceptance criteria

- [ ] `namehash` computes the correct ENSIP-1 node for a name (verified against a known-answer vector, e.g. the canonical `namehash` of a test name).
- [ ] Resolution issues `registry.resolver(node)` then `resolver.contenthash(node)` as `eth_call`s through the `EthereumProvider` seam and decodes the result via the ENSIP-7 decoder.
- [ ] A known fixture name resolves end to end (pinned resolver-address + contenthash responses) to a decoded `ipfs://<cid>` reference.
- [ ] Fail-closed: an unnormalizable name, a zero/absent resolver, a no-contenthash / reverting resolver, and an RPC/seam error each produce a distinct typed resolution failure (never a guess).
- [ ] Tests run against pinned fixture RPC responses with NO live-network dependency.
- [ ] Tests cover the new behaviour (mirror the repo's existing test style).

## Blocked by

- Blocked by `ethereum-provider-seam-and-trusted-rpc-backend` (the seam this resolution calls through).
- Blocked by `ensip7-contenthash-decoder-typed-graceful-errors` (the decoder this resolution feeds the contenthash bytes into).

## Prompt

> Goal: build the ENS name-resolution core — turn a `name.eth` into a decoded contenthash reference (or a typed failure) by composing the `EthereumProvider` seam (task `ethereum-provider-seam-and-trusted-rpc-backend`) with the ENSIP-7 decoder (task `ensip7-contenthash-decoder-typed-graceful-errors`). This is pure resolution logic behind the seam; it does NOT touch the URL bar or rendering (a separate front-door task wires those).
>
> Domain vocabulary: `namehash` (ENSIP-1) is the recursive keccak256 hash of a normalized dotted name into a 32-byte node — normalize labels, then fold from the rightmost label with `namehash(node) = keccak256(parent_node ++ keccak256(label))`, base case the zero node. The **ENS registry** is the canonical mainnet contract whose `resolver(bytes32 node)` returns the resolver contract address for a node; that resolver's `contenthash(bytes32 node)` (ENSIP-7 / EIP-1577) returns the contenthash bytes. Each of these is an `eth_call` — ABI-encode the function selector + node argument, send through the seam, ABI-decode the return. Use vetted crates for keccak256 and ABI/hash primitives (this repo's discipline is to bind vetted implementations, never hand-roll crypto — see `docs/adr/0001` and the `fetcher` crate's TLS-is-bound note); the canonical mainnet ENS registry address is a well-known constant.
>
> Where to look: the `EthereumProvider` seam and its fixture-endpoint test harness (from the blocking task) are how you issue and pin the two `eth_call`s off the live network. The ENSIP-7 decoder (the other blocking task) is what you hand the returned contenthash bytes to — do not re-decode; consume its typed output. Mirror the offline-fixture test style already used across `fetcher` / `werust-core::ipfs` (canned responses, no network).
>
> Fail-closed is a hard requirement (spec story 3): every failure step — unnormalizable name, zero/absent resolver, reverting/empty contenthash, RPC error — is a DISTINCT typed resolution failure, never a partial or guessed result.
>
> Done = a known fixture name resolves through namehash → resolver → contenthash → decode to an `ipfs://<cid>` reference, and each failure path yields its distinct typed error, all proven offline. FIRST re-check the blocking tasks landed as assumed (the seam method shape and the decoder's output enum) — if either differs, route to needs-attention rather than building on a stale premise (WORK-CONTRACT.md "Drift is a needs-attention signal"). RECORD non-obvious in-scope decisions (the registry address constant, the ABI encode/decode choices, the exact failure taxonomy) durably per the task template.
