---
title: "werust: ENS name -> IPFS resolution (Phase 1 — the EthereumProvider seam + trusted-RPC skeleton)"
slug: ens-to-ipfs-resolution-phase1-rpc-skeleton
taskedAfter: []
---

> Phase 1 of the trustless-ENS work, SCOPED to be fully answered and taskable NOW. It stands
> up the `EthereumProvider` seam + a trusted-RPC backend + ENS name resolution + contenthash
> decode + bare-`.eth` wiring, so `ronan.eth` renders an immutable IPFS site end to end,
> labelled TRUSTED (not verified). The trustless Helios backend + IPNS + CCIP-Read are the
> follow-on `trustless-ens-to-ipfs-phase2-3-helios-and-hardening` spec (taskedAfter this one).
> All Phase-1 decisions are settled (see below); this spec carries NO open questions.

## Problem Statement

A user should be able to type **`ronan.eth`** in werust and get the IPFS site that name
points to. Today werust can fetch-and-verify IPFS content by CID, but it has NO way to learn
a name's CID: the "which CID does `ronan.eth` resolve to?" step does not exist. Phase 1 builds
that step over a (trusted, clearly-labelled) RPC as a walking skeleton, behind a seam whose
trustless backend lands in Phase 2 — so the whole `name -> CID -> verified render` path is
real, testable, and honestly labelled, without waiting on the light client.

(Trust honesty: Phase 1 resolution goes through a TRUSTED RPC, so the name->CID mapping is
taken on that RPC's word. The IPFS bytes are still hash-verified against the returned CID
[the existing path], but the RPC could MISDIRECT to a different valid CID. Phase 1 therefore
labels an ENS-resolved page "content-verified, name via TRUSTED RPC" — never "verified".
Phase 2's light client removes the trust; the seam makes that a backend swap.)

## Solution

Compose three new pieces that feed a CID into the render path that ALREADY works, so the whole `name -> CID -> verified render` path is real and honestly labelled:

1. **The `EthereumProvider` seam** (the new trust boundary) with ONE Phase-1 backend, a TRUSTED `RpcProvider` (plain JSON-RPC `eth_call`). Modelled exactly like the `Renderer`/`Fetcher` seams: the seam is the abstraction, the trust level is a swappable BACKEND, so Phase 2's trustless (async) light client is a backend swap, not a rewrite.
2. **ENS name resolution:** a bare `ronan.eth` typed in the URL bar (no scheme, like Brave/Opera) is `namehash`ed and resolved registry -> `resolver(node)` -> `contenthash(node)` (each an `eth_call` through the seam), then the ENSIP-7 contenthash is decoded and dispatched by its OWN multicodec type. `ipfs-ns` feeds the existing verified `ipfs://` path; every other type is a graceful, protocol-named load failure. The address bar keeps the name (no `https://`/gateway rewrite), and every failure is fail-closed with a legible reason.
3. **A distinct trust state:** an ENS-resolved page is labelled "content-verified, name via TRUSTED RPC" (never "verified" — Phase 1 has no light client; name-verification is Phase 2).

> The per-piece implementation and testing detail (the seam shape, the namehash/resolution sequence, the ENSIP-7 protoCode table, the `.eth`-input rule, the fixture-per-protoCode tests) now lives in the tasks derived from this spec, not here.

## User Stories

1. As a user, I type `ronan.eth` and werust loads the immutable IPFS site it points to.
2. As a user, an ENS-resolved page is honestly labelled "content-verified, name via trusted
   RPC" (NOT "verified") in the trust indicator, because Phase 1 resolves via a trusted RPC.
3. As a user, if a name has no contenthash or resolution fails, the load fails clearly rather
   than showing an unverified/guessed page.
4. As a user, if a name points to a protocol werust does not support yet (Arweave, Swarm,
   IPNS, ...), I get a clear message naming that protocol, not a blank failure or wrong render.
5. As a developer, the Ethereum-access trust level is a swappable backend behind one
   `EthereumProvider` seam, so Phase 2's trustless light client is a backend swap, not a
   rewrite.

## Out of Scope (Phase 1)

- The TRUSTLESS light client (Helios) + the checkpoint/bootstrap trust root + the
  "name-verified" trust state -> Phase 2 (`trustless-ens-to-ipfs-phase2-3-helios-and-hardening`).
- IPNS (mutable) resolution, CCIP-Read (offchain resolvers), caching / sync UX, mobile
  light-client footprint -> Phase 2/3.
- Writing/transactions; general dapp RPC (only the reads ENS resolution needs).
- Non-Ethereum name systems; L2/other-chain ENS.
- RESOLVING non-IPFS contenthash protocols (Arweave/Swarm/IPNS/...) — but DETECTING them and
  erroring clearly IS in scope (the graceful-error requirement above).

## Settled decisions (this spec carries NO open questions)

- Trusted `RpcProvider` skeleton behind the `EthereumProvider` seam, honestly labelled; the trustless backend is the Phase-2 swap.
- Bare `ronan.eth` front door; contenthash-typed dispatch; keep the name in the bar; no `https://`/gateway; `ens://` at most a secondary disambiguator (not required in Phase 1).
- `ipfs-ns`-only supported; every other protoCode (incl. `ipns-ns`) is a graceful, named failure.
- Seam designed so a later ASYNC (tokio) light-client backend fits behind it.
- `.eth`-input: treat a `*.eth` URL-bar entry (on Enter / trailing `/`) as an ENS name.

## Why this first

It is the self-contained, visible `ronan.eth` win: it reuses every seam already built
(fetcher/ipfs/renderer/trust-indicator) and adds only the ENS name-resolution + contenthash-
decode step, over a trusted RPC that needs no checkpoint/sync. It de-risks the seam shape and
the resolution/decoding logic before the harder Phase-2 light client, which then drops in as a
pure backend swap.
