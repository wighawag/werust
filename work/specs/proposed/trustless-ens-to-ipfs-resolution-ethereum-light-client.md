---
title: "werust: trustless ENS name -> IPFS resolution via an embedded Ethereum light client"
slug: trustless-ens-to-ipfs-resolution-ethereum-light-client
status: proposed
needsAnswers: true
---

> PROPOSED spec — records intent for human review before tasking. It reuses the seams
> already built (the hash-verified `Fetcher`/ipfs render path, the `Renderer` seam) and
> adds the missing trustless "name -> content-address" half. Not yet tasked; the OPEN
> QUESTIONS below must be answered first.

## Problem Statement

A user should be able to type **`ronan.eth`** (or `ens://ronan.eth`) in werust and get the
site that name points to, **without trusting any server to tell them where it is**. Today
werust can fetch-and-verify IPFS content by CID (the thesis's second half), but it has NO
way to learn a name's CID: the "which CID does `ronan.eth` resolve to?" step does not
exist. Answering that question by asking a centralized RPC "please tell me the contenthash"
would REINTRODUCE exactly the trusted-server the project exists to remove: a malicious RPC
could point `ronan.eth` at a poisoned CID. (The IPFS bytes are still hash-verified against
whatever CID is returned, so an RPC can MISDIRECT to a different valid CID, but not tamper
content in place — still, misdirection defeats the point.)

The missing capability is a **trustless read of Ethereum state**: resolve the ENS name to
its EIP-1577/ENSIP-7 `contenthash` by a chain read that is CRYPTOGRAPHICALLY VERIFIED
against a chain head the client itself validated — no trusted RPC in the trust path.

## Solution (shape, not final)

Compose three pieces, two of which are new, feeding the CID into the render path that
ALREADY works:

```
ens://ronan.eth  (or ronan.eth typed in the URL bar)
   │  1. NAME RESOLUTION  (new: ENS logic)
   │     namehash("ronan.eth") -> ENS registry.resolver(node)
   │                           -> resolver.contenthash(node)  [eth_call]
   ▼
[Ethereum access seam]  (new: the TRUST boundary)
   │  returns a VERIFIED eth_call result (the contenthash bytes)
   ▼  2. DECODE  (new: EIP-1577/ENSIP-7 contenthash -> ipfs://<cid>)
   ▼
[existing hash-verified ipfs path]  fetch_verified(cid) -> render at parity   ✅ built
```

### New seam 1 — `EthereumProvider` (the trust boundary, backend-swappable)

A narrow seam: "given a contract address + calldata (+ block), return the verified return
bytes" (`eth_call`) and the couple of reads ENS needs. Modelled EXACTLY like the
`Renderer`/`Fetcher` seams: the seam is the abstraction; the trust level is a swappable
BACKEND behind it. Two backends, delivered in phases:

- **`RpcProvider` (walking skeleton, TRUSTED).** Plain JSON-RPC `eth_call` to a configured
  endpoint. Gets `ronan.eth -> site` working end to end EARLY so the whole path is real and
  testable. Clearly labelled trusted (the trust indicator must NOT show "verified" for a
  name resolved this way).
- **`LightClientProvider` (the endgame, TRUSTLESS).** Embeds **Helios** (a16z's Rust
  trustless light client: takes an untrusted execution RPC + a weak-subjectivity checkpoint,
  verifies state via `eth_getProof` + sync-committee proofs; compiles to WASM; light enough
  for mobile). `eth_call` results are verified against a chain head the client validated.
  This is what makes `ronan.eth` genuinely trustless.

### New seam 2 — ENS resolution logic (`ens://` scheme + name->CID)

- A new `ens://` scheme (register it on the `Renderer` seam's custom-scheme hook, exactly
  like `ipfs://`), OR treat a bare `name.eth` in the URL bar as `ens://name.eth`.
- `namehash` (ENSIP-1 normalization + hashing), the registry->resolver->`contenthash(node)`
  call sequence (ENSIP-7 / EIP-1577), and CCIP-Read (EIP-3668) awareness (see open Q).
- Decode the returned contenthash (a multicodec-prefixed multihash: `ipfs-ns` /
  `ipns-ns` / `swarm-ns`) into an `ipfs://<cid>` URL, then hand that to the EXISTING
  ipfs verified render path. A name with no/invalid contenthash FAILS the load (never
  renders something unverified).

### Trust indicator integration

Extend the existing verified-vs-served trust indicator with the NAME-resolution trust
state: a page reached via `ens://` through the LIGHT-CLIENT backend is "name-verified +
content-verified"; via the RPC backend it is "content-verified but name via TRUSTED RPC";
a plain served page stays "served". Fail-closed (the established posture): never claim
name-verification the light client did not prove.

## User Stories

1. As a user, I type `ronan.eth` and werust loads the IPFS site that name points to.
2. As a user, I can trust that the name->site mapping was verified against Ethereum, not
   taken on a server's word (via the light-client backend).
3. As a user, if a name has no contenthash (or resolution fails verification), the load
   fails clearly rather than showing me an unverified/guessed page.
4. As a user on mobile, resolution works without running a full node (light-client sync).
5. As a developer, the Ethereum-access trust level is a swappable backend behind one seam,
   so the trusted-RPC skeleton and the trustless light client are the same seam.

## Phased delivery (proposed task shape, for review)

- **Phase 1 — the seam + skeleton (trusted RPC):** `EthereumProvider` seam;
  `RpcProvider` backend; ENS namehash + registry/resolver/`contenthash` call; EIP-1577
  decode -> `ipfs://`; `ens://` wired through the render path; tests against pinned fixture
  responses (no live network). Delivers `ronan.eth -> site` end to end, labelled trusted.
- **Phase 2 — the trustless backend:** embed Helios as `LightClientProvider` behind the
  same seam; checkpoint/bootstrap config; the `eth_call` goes through verified state.
  Trust indicator shows name-verified.
- **Phase 3 — hardening:** CCIP-Read (offchain resolvers), IPNS (mutable) contenthash,
  caching + the sync UX (first-load latency), mobile light-client footprint, wrong-chain /
  stale-checkpoint handling.

## Out of Scope (for this spec)

- Writing/transactions (this is READ-only name resolution; the EIP-1193 provider's
  signing/custody is the separate deferred wallet-broker work).
- Full archival node / general dapp RPC (only the reads ENS resolution needs).
- Non-Ethereum name systems (Unstoppable, Handshake) — future.
- L2/other-chain ENS beyond CCIP-Read awareness in Phase 3.

## OPEN QUESTIONS (must be answered before tasking — needsAnswers: true)

1. **Trust-level endgame + phasing.** Confirm the target is the TRUSTLESS Helios
   light-client backend (recommended), with the trusted-`RpcProvider` as an EARLY skeleton
   behind the same seam (recommended) — NOT trusted-RPC as the permanent answer. Agree?
2. **Weak-subjectivity checkpoint / root of trust.** Helios needs a checkpoint (32 bytes)
   as its trust root. How is it provisioned? (a) ship a recent checkpoint in the build +
   refresh periodically; (b) user-configured; (c) community fallback list (least safe);
   (d) some mix. This is THE security-critical decision of the feature.
3. **Untrusted execution-RPC endpoint.** Helios still needs an untrusted execution RPC
   that supports `eth_getProof` (e.g. Alchemy-class; Infura historically did not). Ship a
   default? user-configured? multiple with fallback? (It is untrusted — Helios verifies it
   — but it must exist and support proofs.)
4. **Helios as a library dependency.** Confirm embedding Helios as a crate (its `core`/
   `ethereum` crates) is acceptable (license, build weight, nightly-toolchain need,
   mobile/WASM build). It compiles to WASM and is mobile-targeted, but it is a large dep —
   ratify pulling it in vs a thinner from-scratch sync-committee verifier (much more work).
5. **CCIP-Read (EIP-3668) scope.** Many ENS names now resolve OFFCHAIN via CCIP-Read
   (OffchainLookup). Phase 1/2 can handle onchain contenthash only; is CCIP-Read a Phase-3
   must, or required earlier? (A CCIP-Read fetch is itself an untrusted gateway call whose
   response is signature-verified — fits the thesis, but is extra work.)
6. **`ens://` scheme vs bare `name.eth`.** Should a bare `ronan.eth` in the URL bar auto-
   resolve as ENS, or only an explicit `ens://ronan.eth`? (Bare-name is nicer UX but risks
   colliding with real DNS `.eth`-like inputs / typos.)
7. **Async vs the current sync fetch path.** The ipfs/fetcher path is synchronous (`ureq`).
   Helios is async (tokio). How does the light-client read integrate with the sync render
   path — a dedicated runtime/thread bridging async->sync at the seam? (An architecture
   decision affecting the `EthereumProvider` seam signature.)

## Why this is the right long-run bet

This is the feature that makes werust's thesis TANGIBLE: `ronan.eth` -> trustless mapping
-> hash-verified content -> rendered page, with NO trusted server anywhere in the path. It
reuses every seam already built (fetcher/ipfs/renderer/trust-indicator) and adds only the
missing trustless "name -> content-address" step. Helios makes the trustless part real and
Rust-native today, on mobile. It is also the natural home for the eventual wallet/provider
trust work (same verified-Ethereum-state foundation).
