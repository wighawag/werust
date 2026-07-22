---
title: "Verifiable IPFS content retrieval: a ContentRetriever seam + a default trustless-gateway CAR backend that renders real multi-block UnixFS directory sites as legitimately ContentVerified"
slug: verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

The REAL, honest fix that makes a real `.eth`/`ipfs://` site render: retrieve and byte-verify a UnixFS `dag-pb` DAG so a real multi-block site (a directory with an `index.html` and relative assets, or a chunked file) is LEGITIMATELY `ContentVerified` — verify EACH block against its own CID and reassemble LOCALLY, rather than trusting a gateway's reassembled bytes — AND resolve directory + sub-resource PATHS so the multi-file site actually renders. Today the verifier re-hashes the whole gateway response against the root CID and fails `HashMismatch` on every real (multi-block) site; `resolve_ipfs_request` fetches a single block per CID and ignores the path. This task closes both gaps for real.

Shape it as a SEAM, not a one-off, so retrieval backends are swappable and (later) user-choosable — mirroring `EthereumProvider`/`Fetcher`/`Renderer` (the interface is the abstraction, the trust/transport is a swappable BACKEND):
- Introduce a `ContentRetriever` seam whose surface is "given a CID (+ a path into the DAG), return the verified bytes for that resource, or a typed failure".
- Ship ONE default backend now: a **trustless-gateway CAR backend** — fetch the DAG blocks over plain HTTP from a trustless gateway (`GET /ipfs/{cid}?format=car`, `dag-scope`/`entity-bytes` to fetch only the blocks a resource needs), verify each block against its own CID, and reassemble/traverse the UnixFS DAG client-side. NO IPFS node required. Reuse the existing `DEFAULT_IPFS_GATEWAY` const + `with_*()` override pattern (there is no config crate; do not invent one). Bind vetted crates (`rs-car-ipfs`/`rs-car`, the repo's `cid`/`multihash`) — do not hand-roll CAR parse, DAG walk, or crypto.
- Structure the seam so the other backends are pure swaps behind it, NOT rewrites: delegated-routing + trustless gateways (a resilience follow-on), an embedded libp2p/Bitswap client (`iroh`/`rust-ipfs`, which folds into the Phase-2 async story), and a user-supplied gateway/local-node URL. The USER-FACING choice of backend is its own task (`retrieval-backend-user-setting`) — this task lands the seam + default backend so that setting has something to switch between.

Trust honesty (hard requirement, `docs/adr/0001`): "content-verified" MUST mean every byte was checked against the hash. Discriminate by CODEC — a `raw`/leaf (0x55) CID whose bytes do not hash to it stays a HARD fail-closed tamper error (NEVER served), and a `dag-pb` (0x70) CID goes through the DAG-verify path. No path ever serves bytes on a hash failure. A multi-block site that fully verifies is `ContentVerified`; reached via ENS it is `NameViaTrustedRpc` (content-verified, name via trusted RPC).

## Settled decisions (from the design discussion — these are DECIDED, build to them)

- **Retrieval is a seam with swappable backends.** Ship the trustless-gateway CAR backend as the DEFAULT; shape the seam so delegated-routing, embedded p2p (Phase-2 async), and a user-supplied gateway/node URL are backend swaps. The user-facing selector is `retrieval-backend-user-setting` (separate task).
- **Tier for THIS task = trustless-gateway CAR over the existing sync `Fetcher` seam. No node, no async runtime.** (Embedded p2p is deliberately deferred to the Phase-2 async path; do not pull in libp2p here.)
- **UnixFS scope IN:** chunked/multi-block files; directories with `index.html` resolution; HAMT-sharded directories. **OUT (named follow-ons):** symlinks; non-UnixFS `dag-cbor`/`dag-json` DAGs.
- **Fail-closed + budget:** a block that does not hash to its CID, a missing block, an incomplete CAR (the spec's client obligation), or a malicious link each fail closed with a distinct reason; a retrieval budget (max total bytes / max blocks / wall-clock) refuses runaway or hostile DAGs.
- **Codec-gated, path-aware:** discriminate by codec (0x55 verify-direct-or-hard-fail; 0x70 DAG path); extend `resolve_ipfs_request` from single-block/ignore-path to path-aware resolution over the verified DAG (directory -> index.html, each `ipfs://<cid>/sub/resource` resolved into the DAG).
- **Stands alone:** lands on the sync seam now; relates to but is not gated on the Phase-2-3 trustless spec (that removes RPC trust from the NAME; this removes gateway trust from the CONTENT).

## Acceptance criteria

- [ ] A `ContentRetriever` seam exists (CID + path -> verified bytes or typed failure), modelled like the existing seams, with ONE default backend: a trustless-gateway CAR backend fetching + client-verifying DAG blocks (`?format=car`), NO IPFS node.
- [ ] A single-block `raw`/leaf `sha2-256` CID still renders `ContentVerified` (no regression), and a `raw` CID whose bytes do NOT hash to it is a HARD fail-closed tamper error (never served).
- [ ] A real multi-block UnixFS DIRECTORY site (e.g. `mandalas.eth`'s `bafybei...`) renders end to end — directory resolved to `index.html`, relative sub-resources (css/js/images) each path-resolved into the verified DAG — and is legitimately `ContentVerified`, with every block hash-checked. `resolve_ipfs_request` is extended from single-block/ignore-path to path-aware DAG resolution.
- [ ] Reached via an ENS `.eth`, such a site shows `NameViaTrustedRpc` (content-verified, name via trusted RPC); a plain `ipfs://` directory shows `ContentVerified`. Neither is ever mislabelled.
- [ ] Fail-closed + budget: a mis-hashing/missing block, an incomplete CAR, or a malicious link each fails the load with a distinct legible reason; a size/time/block budget refuses runaway DAGs. Nothing unverified is ever rendered or labelled verified.
- [ ] The seam is shaped so a delegated-routing backend, an embedded-p2p backend, and a user-supplied gateway/node URL fit as backend swaps (documented), and the gateway endpoint is overridable via the existing `DEFAULT_*` const + `with_*()` pattern (no new config subsystem).
- [ ] Vetted crates bind CAR parse + `dag-pb`/UnixFS decode + per-block CID verification (`rs-car-ipfs`/`rs-car` + the repo's `cid`/`multihash`); no hand-rolled block layout or crypto (`docs/adr/0001`).
- [ ] Tests are network-isolated (CAR/block fixtures + a loopback gateway serving a canned CAR, no live network) and mirror the repo's `fetcher`/`ipfs` test style, INCLUDING a real multi-block directory fixture (index.html + a sub-resource) so the old single-block-only coverage gap is closed, plus the tamper/incomplete-CAR/budget failure paths.

## Blocked by

- None — can start immediately. (The forks that once gated this are now settled above.) Relates to the Phase-2-3 trustless spec (name trust) and unblocks `ipns-name-resolution-and-render` and `retrieval-backend-user-setting`, which reuse this seam + verified render.

## Prompt

> Goal: make a real multi-block UnixFS IPFS site LEGITIMATELY content-verified and actually RENDER — walk the `dag-pb`/UnixFS DAG, verify each block against its own CID, reassemble locally, resolve directory + sub-resource PATHS — behind a `ContentRetriever` SEAM whose default backend is a trustless-gateway CAR fetcher (NO node). Model the seam like `EthereumProvider`/`Fetcher`/`Renderer` so delegated-routing, embedded-p2p (Phase-2 async), and a user-supplied gateway/node URL are later backend swaps; the user-facing selector is a separate task (`retrieval-backend-user-setting`). This is the ONE fix — there is no interim `⚠`-served fallback (it was reviewed and dropped as under-scoped and unsafe).
>
> Domain vocabulary: a CIDv1 `dag-pb` (0x70) CID is a UnixFS DAG root whose block LINKS to child blocks; only walking the DAG and verifying each block against its CID (then reassembling) proves the content — re-hashing a gateway's reassembled bytes against the root CID does NOT (that is the `HashMismatch` on every real site today). A trustless gateway serves the raw DAG blocks over plain HTTP (`GET /ipfs/{cid}?format=car`, with `dag-scope`/`entity-bytes` to fetch only what a resource needs); the client verifies each block. A real site is a DIRECTORY: resolve it to `index.html`, then each `ipfs://<cid>/sub/resource` into the DAG. Discriminate by CODEC: `raw` (0x55) verifies directly (hard-fail on mismatch = tamper, never served); `dag-pb` (0x70) uses the DAG path.
>
> Primary sources (already gathered, 2026-07): the Trustless Gateway spec (https://specs.ipfs.tech/http-gateways/trustless-gateway/ — `?format=car`, `dag-scope`, `entity-bytes`, client MUST verify CAR completeness / incomplete-stream = failure); the `rs-car-ipfs` crate (https://docs.rs/rs-car-ipfs/) wrapping `rs-car`; `@helia/verified-fetch` (JS reference) as a semantics oracle. Bind these, do not hand-roll (`docs/adr/0001`).
>
> Where to look: the current single-block verifier is `VerifyingContentFetcher` in the `fetcher` crate (SHA-256 re-hash, `raw`/leaf only) over the `Fetcher`/`ContentSource` seam and `GatewayContentSource` (`DEFAULT_IPFS_GATEWAY` const + `with_gateway()` override — reuse that pattern, no config crate). A CAR fetch is a GET whose body is a CAR byte stream, so it fits the existing sync seam — the new work is CAR parse + per-block verify + UnixFS reassemble + path resolution, not a new transport. CRUCIAL: `werust_core::ipfs::resolve_ipfs_request` fetches a SINGLE block per CID and IGNORES the path (see its docstring) — extend it to path-aware DAG resolution. The postures live in `TrustPosture` (renderer crate). The follow-ons that reuse this seam: `ipns-name-resolution-and-render`, `retrieval-backend-user-setting`.
>
> Trust honesty is a hard requirement (`docs/adr/0001`): "content-verified" MUST mean every byte was hash-checked; never serve bytes on a hash failure; a `raw` mismatch is a hard tamper failure. Enforce the spec's client obligation (incomplete CAR = failure) and a retrieval budget (max bytes/blocks/wall-clock) so a hostile gateway cannot stream forever.
>
> Done = a `ContentRetriever` seam + trustless-gateway CAR backend renders a real multi-block directory `.eth` site end to end as legitimately `ContentVerified` (`NameViaTrustedRpc` via ENS), single-block CIDs still verify, every tamper/incomplete/budget failure is distinct and fail-closed, the seam is shaped for the other backends + a user-URL override, and it is all proven offline including a real directory fixture. FIRST re-check current reality (the single-block/ignore-path `resolve_ipfs_request`, the verifier's codec support, the seam, the crate surfaces) and route to needs-attention on drift. RECORD the seam shape, the CAR/DAG decisions, and the codec+budget trust model durably — this meets the ADR gate (a security-relevant trust trade-off), so write it as an ADR in `docs/adr/`.
