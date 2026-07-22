---
title: "werust: trustless ENS name -> IPFS resolution via an embedded Ethereum light client"
slug: trustless-ens-to-ipfs-resolution-ethereum-light-client
needsAnswers: true
taskedAfter: []
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
> **The Ethereum access is a consent-gated subsystem, CONFIGURED at first use.** Under
> `gated-protocol-subsystems-consent-and-lazy-activation`, the FIRST time the user opens a
> `.eth` name werust asks HOW to resolve it (a provider-mode choice, not a bare yes/no):
> "Resolve ENS via: [your own RPC (enter URL)] / [a public RPC — trusted] / [the embedded
> light client — trustless, syncs on first use]", with each option's trust/cost trade-off
> shown and a sensible default preselected, and the choice REMEMBERED (changeable later in
> the subsystems management screen). So a user who runs their own node/RPC points werust at
> it right there; a user who just wants it to work takes the default. The embedded
> `LightClientProvider` (Helios) is the HEAVY, `consent-gated` mode (sync delay + an
> untrusted execution-RPC dependency to disclose); the `RpcProvider` (own or public RPC) is
> lightweight. Declining/failing the chosen mode fails the name resolution cleanly; werust
> does NOT silently fall back to a more-trusted-less-verified mode without telling the user
> the resolution is then trusted-not-verified (reflected in the trust indicator).

- **`LightClientProvider` (the endgame, TRUSTLESS).** Embeds **Helios** (a16z's Rust
  trustless light client: takes an untrusted execution RPC + a weak-subjectivity checkpoint,
  verifies state via `eth_getProof` + sync-committee proofs; compiles to WASM; light enough
  for mobile). `eth_call` results are verified against a chain head the client validated.
  This is what makes `ronan.eth` genuinely trustless.

### New seam 2 — ENS resolution logic (bare `name.eth` front door -> contenthash-typed scheme)

**URL-scheme model (decided after surveying Brave / Opera / MetaMask / eth.limo — see the
resolved question below).** The user-facing contract is a BARE ENS NAME, not an `ens://`
scheme:

- **Front door: a bare `ronan.eth` typed in the URL bar.** No scheme required (the natural
  UX, and what Brave/Opera do). werust recognises a `.eth` (and future ENS TLDs) input and
  resolves it as ENS.
- **Dispatch by the contenthash's OWN type, not a fixed scheme.** ENS `contenthash`
  (ENSIP-7/EIP-1577) is a multicodec-prefixed value: `ipfs-ns` -> immutable CID,
  `ipns-ns` -> mutable IPNS name, `swarm-ns`/others. werust reads the multicodec prefix and
  dispatches to the RIGHT path: `ipfs://<cid>` (existing verified path) or `ipns://<name>`
  (needs IPNS resolution -> a CID -> the existing verified path). Getting THIS wrong
  (defaulting to `ipfs://` when the name is actually `ipns://`) is the exact bug real
  browsers ship; werust must key off the contenthash type.
- **The address bar KEEPS `ronan.eth`.** The ENS name is the identity the user cares about;
  do NOT rewrite the bar to the underlying `ipfs://<cid>` (the top complaint about existing
  browsers is losing the name). Internally the load is the resolved CID; the displayed URL
  is the name.
- **NO `https://ronan.eth` rewrite, NO trusted gateway.** Rewriting to `https://` (the
  eth.limo / `ronan.eth.limo` model) reintroduces a trusted HTTPS gateway and false
  TLS-trust semantics — the exact trusted-server model this project rejects.
- **`namehash`** (ENSIP-1 normalization + hashing), the registry->resolver->
  `contenthash(node)` call sequence (ENSIP-7/EIP-1577), and CCIP-Read (EIP-3668) awareness
  (see open Q).
- A name with no/invalid contenthash, or one whose resolution fails verification, FAILS the
  load (never renders something unverified/guessed).
- **Unsupported contenthash protocols must fail GRACEFULLY and SPECIFICALLY.** ENS
  `contenthash` is multicodec-tagged (ENSIP-7); werust supports only a subset initially and
  MUST decode the protoCode, recognise it, and reject unsupported ones with a CLEAR,
  protocol-named error — NOT crash, NOT silently mis-dispatch to `ipfs://`, NOT fall through
  to a default. The distinctions the resolver must make, each a DIFFERENT user-facing
  message:
  - `0xe3` **ipfs-ns** (IPFS, immutable CID) — SUPPORTED (the built path).
  - `0xe5` **ipns-ns** (IPNS, mutable) — recognised but DEFERRED (Q8): fail with "this name
    uses a mutable IPNS pointer, not yet supported".
  - `0xe4` **swarm-ns** (Swarm) — recognised, UNSUPPORTED: "points to Swarm, not supported".
  - **Arweave** (and `onion`/`onion3`, `skynet`, `zeronet`, DNSLink, any unknown protoCode)
    — recognised-or-unknown, UNSUPPORTED: name the protocol if known ("points to Arweave,
    not supported"), else "unsupported/unknown contenthash protocol (0x..)". The user asked
    specifically for Arweave to error gracefully rather than fail obscurely.
  So the decode result is a small typed enum: Supported(ipfs cid) | Deferred(ipns) |
  Unsupported(named protocol) | NoContenthash | Malformed — each mapped to a distinct load
  failure with a legible reason surfaced in the chrome, so a user learns WHY (e.g. "ronan.eth
  points to Arweave content, which werust does not support yet") instead of a blank fail.
- **`ens://ronan.eth` is a secondary, explicit disambiguator ONLY** (e.g. to force ENS
  resolution of an ambiguous input) — NOT the primary contract and NOT required. No other
  browser uses `ens://`; the bare name is the convention.

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
6. As a user, if a name points to a protocol werust does not support yet (Arweave, Swarm,
   IPNS, …), I get a clear message naming that protocol — not a blank failure or a wrong
   render. (werust decodes the ENS contenthash protoCode and reports the specific reason.)
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
- **Non-IPFS ENS contenthash protocols** (Arweave, Swarm, IPNS-mutable, onion, skynet,
  zeronet, DNSLink) — NOT supported. BUT this is a hard requirement to handle GRACEFULLY:
  they must be DECODED, RECOGNISED, and rejected with a specific protocol-named error, never
  crash / mis-dispatch / blank-fail. "Out of scope to RESOLVE" is not "out of scope to
  detect": detecting-and-erroring-clearly IS in scope. (Per the human: ENS can point to
  other protocols like Arweave; we don't support them now but must error gracefully.)

## DECISIONS CONFIRMED BY THE HUMAN (2026-07-22)

The human accepted the recommended defaults, so these are settled for tasking:
- **Q1 endgame + phasing:** trustless Helios light-client backend is the target; the
  trusted `RpcProvider` is an EARLY skeleton behind the SAME seam, not the permanent answer.
- **Q2 checkpoint:** ship a recent weak-subjectivity checkpoint in-build + allow user
  override + strict-checkpoint-age WARNING; NOT the community fallback as the default root.
- **Q3 execution RPC:** user-configurable with a sensible default, clearly labelled
  untrusted (Helios verifies it); must support `eth_getProof`.
- **Q4 Helios dependency:** embed Helios (Rust-native, mobile/WASM-proven); do NOT hand-roll
  a sync-committee verifier.
- **Q5 CCIP-Read:** Phase 3.
- **Q6 scheme:** RESOLVED above — bare `ronan.eth` front door, contenthash-typed dispatch,
  keep the name in the bar, no `https://`/gateway, `ens://` secondary only.
- **Q7 async bridge:** a dedicated tokio runtime bridged async->sync at the
  `EthereumProvider` seam.

Remaining genuinely-open items for a human touch before/within tasking: Q2's exact
checkpoint-refresh mechanics, Q6's `.eth`-input strictness sub-question, and Q8 (IPNS
scope). Phase 1 can be tasked now on the confirmed decisions; Q8 defaults to `ipfs-ns`-only
for Phase 1.

## OPEN QUESTIONS (context + the still-open sub-items)

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
6. **RESOLVED — URL scheme.** A bare `ronan.eth` is the front door (matches Brave/Opera and
   user expectation); the underlying scheme is chosen by the contenthash's OWN multicodec
   type (`ipfs://` vs `ipns://` vs …); the address bar keeps `ronan.eth`; NO `https://`
   rewrite / trusted gateway; `ens://` is only a secondary explicit disambiguator. (Decided
   from a survey of Brave, Opera, MetaMask, eth.limo: nobody uses `ens://`; `https://`
   rewrite is the trusted-gateway anti-pattern this project rejects.) The one open sub-
   question: how strict is `.eth`-input detection to avoid mis-firing on typos / real inputs
   that merely LOOK like a name (e.g. require a trailing `/` or an explicit resolve action
   vs. auto-resolving any `*.eth`).
8. **NEW — IPNS (mutable contenthash) resolution.** Because dispatch is by contenthash type,
   a name pointing at `ipns-ns` (very common — e.g. sites that update without re-paying gas)
   needs IPNS -> CID resolution, which the current fetcher does NOT do (it fetches immutable
   CIDs only). IPNS resolution is itself a trust question (an IPNS record is signed by its
   key; resolving it via a gateway is a trust hop unless the signature is verified). Scope:
   is `ipfs-ns`-only acceptable for Phase 1/2 (fail an `ipns-ns` name with a clear "mutable
   names not yet supported"), with verified IPNS resolution as Phase 3? (Recommended:
   yes — immutable `ipfs-ns` first; IPNS is its own verified-resolution sub-project.)
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
