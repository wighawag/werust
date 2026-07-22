---
title: EthereumProvider seam + trusted RpcProvider backend (eth_call over pinned fixture)
slug: ethereum-provider-seam-and-trusted-rpc-backend
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [5]
---

## What to build

Introduce the `EthereumProvider` seam: the narrow trust boundary for Ethereum READS (werust-internal), modelled EXACTLY like the existing `Fetcher` / `Renderer` / `ContentSource` seams (an abstraction whose trust level is a swappable BACKEND). The seam's surface is "given a contract address + calldata (+ block tag), return the `eth_call` return bytes", plus only the couple of reads ENS resolution needs (nothing more; this is not a general dapp-RPC surface).

Note this is DISTINCT from the glossary's `EIP-1193 provider` (`CONTEXT.md`): that is the page-facing Ethereum provider injected into web pages via the `Renderer` script bridge; THIS is werust's own internal trusted-read seam that ENS resolution calls. Do not conflate the two.

Ship ONE Phase-1 backend behind it: `RpcProvider`, a TRUSTED skeleton that performs a plain JSON-RPC `eth_call` against a configured HTTP endpoint. The endpoint is user-configurable with a sensible default, clearly labelled as a trusted origin (there is no config crate in the repo yet, so mirror the `GatewayContentSource::new` / `with_gateway` shape: a `DEFAULT_*` endpoint constant plus a constructor that overrides it; do NOT invent a config subsystem).

TRANSPORT REALITY (resolve this explicitly, do NOT assume a drop-in): a JSON-RPC `eth_call` is an HTTP POST carrying a JSON request body. The existing `Fetcher` seam is GET-ONLY (`fetch(&self, url)` returning bytes; `HttpFetcher` does `agent.get(url).call()`), so it CANNOT send the request body as-is. Pick one path and RECORD which you chose and why: (a) EXTEND the `Fetcher` seam with a body-carrying POST method (a scoped, named seam change; keep it minimal and update `HttpFetcher` to bind ureq's POST), then have `RpcProvider` transport over that; or (b) have `RpcProvider` bind its OWN minimal synchronous JSON-RPC HTTP transport (mirroring how `HttpFetcher` binds `ureq` behind the seam, still OFF any async runtime), keeping the bound-not-hand-rolled discipline. Either way the transport must stay behind the seam and off async; do NOT try to encode an `eth_call` body into a GET URL.

The RPC call itself is synchronous in Phase 1, BUT the seam signature must ACCOMMODATE a future async backend: Phase 2 will embed an async/tokio light client behind this same seam (bridged async→sync at the seam boundary), so do not shape the trait so it structurally BLOCKS that later swap.

Every failure (transport error, non-2xx, a JSON-RPC error object, an unparseable result) surfaces as a typed seam error, never a panic and never a silently-empty result. Tests drive the whole path against a pinned/local fixture endpoint (a loopback HTTP server answering a canned JSON-RPC response, exactly like the fetcher/ipfs seam tests) so there is NO live-network dependency. Because the loopback fixture answers any accepted request, the test MUST also assert the request carried the JSON-RPC `eth_call` body (method + params), so a GET-only transport that silently drops the body cannot pass.

## Acceptance criteria

- [ ] An `EthereumProvider` seam trait exists whose method(s) cover an `eth_call` (address + calldata + block) returning the raw return bytes, plus only the reads ENS needs.
- [ ] A `RpcProvider` backend implements the seam over JSON-RPC `eth_call`, sending the request as an HTTP POST with the JSON-RPC body (via an extended `Fetcher` POST method OR its own bound minimal sync HTTP transport — the chosen path recorded), NOT a GET URL.
- [ ] The RPC endpoint is user-overridable with a sensible labelled default (constant + constructor override, no new config subsystem).
- [ ] The seam signature does not structurally preclude a later async (tokio) backend fitting behind it (documented, and shaped so an async→sync bridge is possible at the seam).
- [ ] Every failure surfaces as a typed seam error (never a panic, never a silent empty result).
- [ ] Tests exercise `eth_call` end to end against a pinned/local fixture endpoint (loopback server or in-memory transport double) with NO live-network dependency, mirroring the existing fetcher/ipfs test harness style, AND assert the outgoing request actually carried the JSON-RPC `eth_call` body.
- [ ] Tests cover the new behaviour (mirror the repo's existing test style).

## Blocked by

- None — can start immediately.

## Prompt

> Goal: stand up the `EthereumProvider` seam and its one Phase-1 backend, `RpcProvider` (a TRUSTED JSON-RPC `eth_call` skeleton). This is the trust-boundary seam that later Phase-2 work swaps a trustless light client behind, so the shape matters more than the feature: model it EXACTLY like the sibling seams already in the tree — `Fetcher` (in the `fetcher` crate), the `ContentSource` trait, and the `Renderer` seam — where the interface is the abstraction and the concrete stack is a swappable backend. This is werust's INTERNAL read seam, NOT the page-injected `EIP-1193 provider` in the glossary (`CONTEXT.md`) — do not conflate them.
>
> Domain vocabulary: an `eth_call` is a read-only Ethereum contract call (address + ABI-encoded calldata + a block tag, returning ABI-encoded return bytes) carried over JSON-RPC as an HTTP POST with a JSON body (`{'jsonrpc':'2.0','method':'eth_call','params':[{to, data}, block],'id':..}`, single-quoted here to stay legible). ENS resolution (a sibling task) needs only a handful of these reads; do NOT build a general dapp-RPC surface (writes/transactions/subscriptions are out of scope, see the spec's Out of Scope).
>
> Where to look: the `fetcher` crate is the model for a seam + a bound backend + loopback-server tests (its `LocalHttpServer` test harness binds `127.0.0.1:0` and answers canned responses off the live network — copy that pattern for the fixture RPC endpoint). The `GatewayContentSource::new` / `with_gateway` pair (in `werust-core`'s ipfs module) is the model for a labelled default endpoint constant + a constructor that overrides it — there is NO config crate in this repo, so do not chase one.
>
> TRANSPORT (the trap): do NOT assume you can just reuse `Fetcher` for transport. The `Fetcher` seam is GET-ONLY — `fetch(&self, url)` with no method/body, and `HttpFetcher` calls `agent.get(url).call()`; `GatewayContentSource` only ever builds a GET URL. A JSON-RPC `eth_call` is a POST with a JSON body, which cannot be encoded into a bare GET URL. Resolve this ONE of two ways and record which: (a) EXTEND the `Fetcher` seam with a minimal body-carrying POST method and bind ureq's POST in `HttpFetcher`, then transport `RpcProvider` over it; or (b) bind a minimal synchronous JSON-RPC HTTP transport inside `RpcProvider` itself, mirroring how `HttpFetcher` binds `ureq` (bound, not hand-rolled), still off any async runtime. Keep transport behind the seam either way.
>
> Async accommodation (load-bearing for Phase 2): Phase 2 will embed Helios (async/tokio) behind this SAME seam, bridged async→sync at the boundary. Phase 1's call can be plain sync, but design the trait so a later async backend fits — do not, for example, hand back a borrowed reference tied to a sync call stack that an async bridge could not satisfy. Document the intent at the seam.
>
> The loopback fixture answers ANY accepted request with a canned body, so a GET-only transport that silently drops the JSON-RPC body would still make a naive test pass — therefore ASSERT the outgoing request actually carried the `eth_call` JSON body (method + params), not just that some bytes came back.
>
> Done = a `dyn EthereumProvider` caller can issue an `eth_call` (as a real POST with the JSON-RPC body) and get back the return bytes (or a typed error), proven by a test against a pinned/local fixture endpoint with no live network that also checks the request body. FIRST re-check this against current reality (the sibling seams and their test harnesses may have evolved; confirm whether `Fetcher` is still GET-only) per WORK-CONTRACT.md 'Drift is a needs-attention signal'. RECORD any non-obvious in-scope decision (the transport path a/b you chose, the exact seam method shape, the block-tag default, how a JSON-RPC error object maps to your error enum) durably per the task template's decision-recording guidance.
