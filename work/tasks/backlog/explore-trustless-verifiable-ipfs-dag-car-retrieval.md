---
title: "EXPLORATION: trustless verifiable IPFS retrieval — walk + hash-verify a UnixFS dag-pb DAG (CAR / block-by-block) so multi-block sites can be legitimately content-verified"
slug: explore-trustless-verifiable-ipfs-dag-car-retrieval
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
needsAnswers: true
blockedBy: []
covers: [1]
---

<!-- open-questions -->

## Open questions

This is an EXPLORATION task: its first job is to answer these, not to ship a subsystem blind.

1. **Retrieval format + source.** Fetch a CAR (`?format=car` from a trustless gateway) and verify the whole DAG offline, or fetch block-by-block (`?format=raw` per CID) and walk incrementally? Which trustless-gateway/protocol surface do we target, and does it stay behind the existing `Fetcher`/`ContentSource` seam (no async runtime), or does it need the async path Phase-2 introduces?
2. **DAG + UnixFS scope.** Which crates/vetted implementations for `dag-pb` block decode, UnixFS file/directory reassembly, and per-block CID verification (bind, don't hand-roll — `docs/adr/0001`)? Which UnixFS features are in scope (chunked files, directories/`index.html` resolution, HAMT-sharded directories, symlinks)? What is explicitly out?
3. **Trust + failure model.** How does a partial/hostile DAG fail (a block that does not hash to its CID, a missing block, a malicious link) — always fail-closed with a distinct reason? How large a site do we retrieve before refusing (a size/time budget)? Does this fold into the Phase-2 trustless story, or stand alone?

<!-- /open-questions -->

## What to build

The REAL, honest fix for multi-block IPFS sites: retrieve and byte-verify a UnixFS `dag-pb` DAG so a real site (directory/chunked file) can be legitimately `ContentVerified` — verifying EACH block against its own CID and reassembling the content LOCALLY, rather than trusting a gateway's reassembled bytes. This replaces the interim honesty (`ipfs-render-unverified-gateway-fallback-for-multiblock-unixfs`, which renders multi-block sites under the `⚠` unverified posture) with genuine verification, so an ENS-resolved multi-block site can carry the honest `NameViaTrustedRpc` (content-verified, name via trusted RPC) posture instead of the served/unverified one.

As an exploration task, produce: the answered open questions (a short findings/ADR), a prototype or spike proving the DAG-walk + per-block verification works against a real fixture site offline, and either the shipped verifier behind the existing seam OR a precise follow-on task set if the work is larger than one task. Wire it so the desktop and (via `mobile-ipfs-scheme-interception-ios-and-android`) mobile `ipfs://` paths can upgrade a multi-block site from `⚠ unverified` to genuinely `ContentVerified` once this lands.

## Acceptance criteria

- [ ] The open questions are answered and recorded (findings note and/or ADR): retrieval format, DAG/UnixFS scope, failure + budget model.
- [ ] A prototype/spike verifies a real multi-block UnixFS fixture end to end OFFLINE: each block checked against its own CID, the file/directory reassembled locally, the root matching the requested CID.
- [ ] A multi-block UnixFS `.eth` site that today renders `⚠ unverified` can be legitimately `ContentVerified` via this path (or a precise follow-on task set to get there is filed, if it exceeds one task).
- [ ] Verification stays honest and fail-closed: a bad/missing/mis-hashing block fails the load with a distinct reason; nothing unverified is ever labelled verified. A retrieval size/time budget refuses runaway DAGs.
- [ ] Vetted crates bind the `dag-pb`/UnixFS/CID work (no hand-rolled crypto or block layout), per `docs/adr/0001`.
- [ ] Tests are network-isolated (CAR/block fixtures, loopback, no live gateway) and mirror the repo's test style.

## Blocked by

- None to START the exploration, but `needsAnswers: true` (the format/scope/trust-model forks must be settled as its first output). Relates to the Phase-2-3 trustless spec (`trustless-ens-to-ipfs-phase2-3-helios-and-hardening`), which removes RPC trust from the NAME; this removes gateway trust from the CONTENT for multi-block sites — decide during exploration whether they land together.

## Prompt

> Goal: build (or precisely scope) the trustless verifiable retrieval that lets a real multi-block UnixFS IPFS site be LEGITIMATELY content-verified — walk the `dag-pb`/UnixFS DAG, verify each block against its own CID, reassemble locally — replacing the interim `⚠ unverified` gateway fallback with genuine verification. This is an EXPLORATION task: answer the format/scope/trust forks first, prototype the DAG-walk verification against a real fixture offline, then ship the verifier behind the existing seam or file a precise follow-on task set.
>
> Domain vocabulary: a CIDv1 `dag-pb` (0x70) CID is a UnixFS DAG root whose block LINKS to child blocks; only walking the DAG and verifying each block against its CID (then reassembling) proves the content — re-hashing a gateway's reassembled bytes against the root CID does not (that is the `HashMismatch` that motivated the interim fallback). Trustless retrieval fetches a CAR (`?format=car`) or raw blocks (`?format=raw`) so verification happens client-side.
>
> Where to look: the current single-block verifier is `VerifyingContentFetcher` in the `fetcher` crate (SHA-256 re-hash, `raw`/leaf only); the `Fetcher`/`ContentSource` seam and `GatewayContentSource` are the transport model. The interim honesty this replaces is `ipfs-render-unverified-gateway-fallback-for-multiblock-unixfs` (multi-block -> `⚠` served). The related name-trust work is the Phase-2-3 trustless spec. Bind vetted `dag-pb`/UnixFS/CID crates (`docs/adr/0001`); do not hand-roll block layout or crypto.
>
> Done = the forks are answered + recorded, a spike proves offline per-block-verified reassembly of a real UnixFS fixture, and either the verifier ships behind the seam (multi-block sites can be genuinely `ContentVerified`) or a precise follow-on task set is filed. FIRST re-check current reality (the interim fallback's landed shape, the seam) and route to needs-attention on drift. RECORD the retrieval/scope/trust decisions durably (findings note + an ADR for the trust-model choice).
