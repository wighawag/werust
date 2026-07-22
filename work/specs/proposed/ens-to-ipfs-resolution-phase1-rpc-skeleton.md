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

Compose three pieces feeding the CID into the render path that ALREADY works:

```
ronan.eth  (typed in the URL bar)
   │  1. NAME RESOLUTION (new: ENS logic)
   │     namehash("ronan.eth") -> ENS registry.resolver(node) -> resolver.contenthash(node)
   ▼        (each an eth_call)
[EthereumProvider seam]  (new: the trust boundary; Phase 1 backend = trusted RpcProvider)
   │  returns the eth_call result (the contenthash bytes)
   ▼  2. DECODE (new: EIP-1577/ENSIP-7 contenthash -> ipfs://<cid>, protocol-typed)
   ▼
[existing hash-verified ipfs path]  fetch_verified(cid) -> render at parity   ✅ built
```

### The `EthereumProvider` seam (backend-swappable — the Phase-2 hook)

A narrow seam: "given a contract address + calldata (+ block), return the eth_call return
bytes", plus the couple of reads ENS needs. Modelled EXACTLY like the `Renderer`/`Fetcher`
seams: the seam is the abstraction; the trust level is a swappable BACKEND. Phase 1 ships ONE
backend:

- **`RpcProvider` (TRUSTED skeleton).** Plain JSON-RPC `eth_call` to a configured endpoint
  (user-configurable with a sensible default, clearly labelled untrusted/trusted). Gets
  `ronan.eth -> site` working end to end. The seam signature must ACCOMMODATE the future
  async light-client backend (Phase 2 embeds Helios, which is async/tokio) — so design the
  seam so a later async backend fits behind it (a dedicated tokio runtime bridged async->sync
  at the seam is the intended Phase-2 shape; Phase 1's RPC call can be sync, but do not shape
  the seam so it BLOCKS that).

### ENS resolution logic (bare `name.eth` front door -> contenthash-typed dispatch)

- **Front door: a bare `ronan.eth` typed in the URL bar.** No scheme required (what Brave/
  Opera do). werust recognises a `.eth` input and resolves it as ENS. (`.eth`-input
  strictness: require an explicit resolve — a trailing `/` or Enter on a `*.eth` token — over
  aggressively auto-resolving anything that merely looks like a name; the exact rule is a
  small in-task judgement, defaulting to "treat a `*.eth` URL-bar entry as an ENS name".)
- **`namehash`** (ENSIP-1 normalization + hashing) + the registry -> `resolver(node)` ->
  `resolver.contenthash(node)` call sequence (ENSIP-7/EIP-1577), each via the seam's eth_call.
- **Dispatch by the contenthash's OWN multicodec type.** Decode the ENSIP-7 contenthash and
  key off its protoCode (see the graceful-error table). For `ipfs-ns` -> `ipfs://<cid>` into
  the existing verified path. Do NOT default to `ipfs://` for other types.
- **The address bar KEEPS `ronan.eth`** (the identity the user cares about); the internal load
  is the resolved CID, the displayed URL is the name. NO `https://` rewrite, NO trusted
  gateway.
- **Fail-closed:** a name with no/invalid contenthash, or a resolution that fails, FAILS the
  load with a legible reason — never renders something unverified/guessed.

### Graceful, protocol-named errors for unsupported contenthash (hard requirement)

The decode returns a small typed enum, each mapped to a DISTINCT user-facing load failure,
never a crash / mis-dispatch / blank fail:
- `0xe3` **ipfs-ns** (immutable CID) -> SUPPORTED (the built path).
- `0xe5` **ipns-ns** (mutable IPNS) -> "this name uses a mutable IPNS pointer, not yet
  supported" (deferred to Phase 2/3).
- `0xe4` **swarm-ns** -> "points to Swarm, not supported".
- **Arweave** / `onion` / `onion3` / `skynet` / `zeronet` / DNSLink / any unknown protoCode
  -> name it if known ("points to Arweave, not supported"), else "unsupported/unknown
  contenthash protocol (0x..)".
Plus `NoContenthash` and `Malformed` as their own distinct messages. Tested with a fixture
per protoCode.

### Trust indicator integration

Extend the existing verified-vs-served indicator with the name-resolution state: an
ENS-resolved page in Phase 1 is "content-verified, name via TRUSTED RPC" (a distinct state
from both "content-verified" and "served"). Fail-closed: never claim name-VERIFICATION in
Phase 1 (there is no light client yet). Phase 2 adds the "name-verified" state.

### Reuse vs new

- REUSE: the hash-verified `Fetcher`/ipfs render path (`fetch_verified`); the `Renderer` seam;
  the trust-indicator surface; the config system; the URL-bar/chrome (`werust-core`).
- NEW: the `EthereumProvider` seam + `RpcProvider` backend; ENS namehash + registry/resolver/
  contenthash resolution; the ENSIP-7 contenthash decoder (typed, graceful); bare-`.eth`
  URL-bar recognition wired to resolution -> the ipfs path; the "name via trusted RPC" trust
  state.

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

## Acceptance shape (what the tasks must deliver)

- `EthereumProvider` seam + `RpcProvider` backend; eth_call works against a controlled/pinned
  fixture endpoint (no live-network dependency in tests).
- ENS namehash + registry->resolver->contenthash resolution, tested against pinned fixture
  responses for a known name.
- ENSIP-7 contenthash decoder with the typed graceful-error enum; a fixture test per protoCode
  (ipfs-ns success; ipns-ns/swarm-ns/arweave/unknown each producing their distinct error).
- A bare `.eth` URL-bar entry resolves and renders an immutable-`ipfs-ns` site end to end via
  the existing verified path; the address bar keeps `ronan.eth`; no `https://` rewrite.
- The trust indicator shows the "content-verified, name via trusted RPC" state for such a page.
- Fail-closed on every failure path (no contenthash / malformed / unsupported / resolution
  error), each with a legible chrome reason.
- Tests are network-isolated (pinned RPC + contenthash fixtures); `cargo fmt/clippy/build/test`
  green.

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

- Trusted `RpcProvider` skeleton behind the `EthereumProvider` seam, honestly labelled; the
  trustless backend is the Phase-2 swap.
- Bare `ronan.eth` front door; contenthash-typed dispatch; keep the name in the bar; no
  `https://`/gateway; `ens://` at most a secondary disambiguator (not required in Phase 1).
- `ipfs-ns`-only supported; every other protoCode (incl. `ipns-ns`) is a graceful, named
  failure.
- Seam designed so a later ASYNC (tokio) light-client backend fits behind it.
- `.eth`-input: treat a `*.eth` URL-bar entry (on Enter / trailing `/`) as an ENS name.

## Why this first

It is the self-contained, visible `ronan.eth` win: it reuses every seam already built
(fetcher/ipfs/renderer/trust-indicator) and adds only the ENS name-resolution + contenthash-
decode step, over a trusted RPC that needs no checkpoint/sync. It de-risks the seam shape and
the resolution/decoding logic before the harder Phase-2 light client, which then drops in as a
pure backend swap.
