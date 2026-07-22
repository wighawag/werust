---
title: "werust: trustless ENS resolution (Phase 2-3 — embed Helios light client + IPNS + CCIP-Read hardening)"
slug: trustless-ens-to-ipfs-phase2-3-helios-and-hardening
needsAnswers: true
taskedAfter: [ens-to-ipfs-resolution-phase1-rpc-skeleton]
---

> PROPOSED spec, taskedAfter the Phase-1 skeleton (`ens-to-ipfs-resolution-phase1-rpc-skeleton`).
> It swaps a TRUSTLESS Helios light-client backend behind the `EthereumProvider` seam Phase 1
> builds, then hardens with IPNS + CCIP-Read. Stays `needsAnswers` because the security-critical
> checkpoint/bootstrap decisions and the IPNS trust model are genuinely open. Do NOT task until
> those are answered; Phase 1 delivers the working `ronan.eth` meanwhile.

## Problem Statement

Phase 1 resolves `ronan.eth` over a TRUSTED RPC (labelled "name via trusted RPC"): the RPC
could misdirect the name to a different valid CID. Phase 2 removes that trust by resolving the
ENS `contenthash` through an Ethereum read that is CRYPTOGRAPHICALLY VERIFIED against a chain
head the client itself validated, using an embedded light client, so `ronan.eth` becomes
genuinely trustless. Phase 3 extends coverage (IPNS mutable names, CCIP-Read offchain
resolvers) and hardens the sync/mobile/UX edges.

## Solution (shape, not final)

The `EthereumProvider` seam already exists (Phase 1); this adds a second backend behind it and
the resolver coverage the graceful-error path currently rejects:

### Phase 2 — `LightClientProvider` (the trustless backend)

- Embed **Helios** (a16z's Rust trustless light client): takes an untrusted execution RPC + a
  weak-subjectivity checkpoint, verifies state via `eth_getProof` + sync-committee proofs;
  compiles to WASM; light enough for mobile. `eth_call` results are verified against a chain
  head the client validated, so the ENS resolution is trustless.
- It is a HEAVY, `consent-gated` `Subsystem` under
  `gated-protocol-subsystems-consent-and-lazy-activation`, CONFIGURED at first use: the first
  `.eth` navigation asks "Resolve ENS via: [your own RPC] / [a public RPC, trusted] / [the
  embedded light client, trustless, syncs on first use]", each with its trust/cost trade-off
  and a remembered choice (changeable in the subsystems management screen). Declining/failing
  the chosen mode fails resolution cleanly; werust never silently downgrades to a
  more-trusted-less-verified mode without telling the user (reflected in the trust indicator).
- Trust indicator gains the "name-verified + content-verified" state (only the light-client
  backend earns it; the trusted RPC stays "name via trusted RPC").
- Async integration: Helios is async (tokio); bridge async->sync at the `EthereumProvider`
  seam via a dedicated runtime/thread (the seam was shaped for this in Phase 1).

### Phase 3 — coverage + hardening

- **IPNS (mutable `ipns-ns` contenthash):** resolve an IPNS name -> CID, VERIFIED (an IPNS
  record is key-signed; resolving via a gateway is a trust hop unless the signature is
  verified). Its own verified-resolution sub-project; Phase 1 currently fails `ipns-ns` with
  "not yet supported".
- **CCIP-Read (EIP-3668) offchain resolvers:** many ENS names resolve offchain via
  OffchainLookup; the CCIP-Read fetch is an untrusted gateway call whose response is
  signature-verified (fits the thesis). Phase 1/2 handle onchain contenthash only.
- Caching + first-load sync UX (light-client sync latency), mobile light-client footprint,
  wrong-chain / stale-checkpoint handling.

## User Stories

1. As a user, I can trust that `ronan.eth`'s name->site mapping was VERIFIED against Ethereum
   (via the light-client backend), not taken on a server's word.
2. As a user on mobile, trustless resolution works without running a full node.
3. As a user, a name using a mutable IPNS pointer or an offchain (CCIP-Read) resolver loads
   (Phase 3), still fail-closed on verification failure.
4. As a user, the trust indicator distinguishes "name-verified + content-verified" (light
   client) from "content-verified, name via trusted RPC" (Phase-1 RPC).

## Out of Scope

- The Phase-1 seam + trusted skeleton + contenthash decode + bare-`.eth` wiring (that IS
  `ens-to-ipfs-resolution-phase1-rpc-skeleton`).
- Writing/transactions; general dapp RPC.
- Non-Ethereum name systems; L2/other-chain ENS beyond CCIP-Read.

## OPEN QUESTIONS (must be answered before tasking — needsAnswers: true)

1. **Weak-subjectivity checkpoint / root of trust (security-critical).** Helios needs a 32-byte
   checkpoint. Provisioning: (a) ship a recent checkpoint in the build + refresh periodically;
   (b) user-configured; (c) community fallback (least safe); (d) a mix. AND the exact
   refresh/staleness mechanics (strict-checkpoint-age warning vs error). Confirmed direction:
   ship-in-build + user override + strict-age WARNING, NOT community-fallback default — but the
   refresh mechanics are still open.
2. **Untrusted execution-RPC endpoint.** Helios needs an `eth_getProof`-capable RPC (untrusted,
   it verifies it). Ship a default? user-configured? multiple with fallback? Confirmed: user-
   configurable + sensible default, clearly labelled untrusted — the default endpoint choice
   is open.
3. **Helios as a library dependency.** Ratify embedding Helios's crates (license, build weight,
   nightly-toolchain need, mobile/WASM build). Confirmed direction: embed it (do not hand-roll
   a verifier) — but the concrete build/mobile integration is a spike.
4. **Async->sync bridge concretely.** The seam signature accommodates it; the exact runtime/
   thread model + backpressure/cancellation on navigation is open.
5. **IPNS trust model (Phase 3).** How is an `ipns-ns` record resolved AND its signature
   verified without a trusted gateway? (Its own verified-resolution design.)
6. **CCIP-Read scope/trust (Phase 3).** OffchainLookup handling + verifying the gateway
   response signature; how much lands in Phase 3.
7. **Consent-gated integration.** Depends on the subsystem framework
   (`gated-protocol-subsystems-...`) existing; confirm sequencing (this spec effectively also
   waits on that framework for the first-use consent + management-screen config).

## Why this is the right long-run bet

This is what makes werust's thesis TANGIBLE end to end: `ronan.eth` -> TRUSTLESS mapping ->
hash-verified content -> rendered page, with NO trusted server anywhere. Phase 1 proved the
seam + resolution over a trusted RPC; this swaps in the trustless backend (a pure backend swap,
by design) and extends coverage. Helios makes the trustless part Rust-native and mobile-capable
today. It is also the foundation for later wallet/provider trust work (same verified-Ethereum
-state substrate).
