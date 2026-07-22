# Spike: ENS name resolution (namehash + registry -> resolver -> contenthash over the `EthereumProvider` seam)

Durable evidence + decisions for task `ens-namehash-registry-resolver-contenthash-resolution` (spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`, stories 1 + 3).

## What was built

The pure ENS resolution CORE in `crates/werust-core/src/ens.rs`: `resolve(provider: &dyn EthereumProvider, name: &str) -> Result<DecodedContenthash, ResolutionError>`, plus the public `namehash(name) -> Result<[u8;32], ResolutionError>` and the typed `ResolutionError` taxonomy. It composes the two blocking tasks' surfaces (unchanged): it issues its two reads through the `EthereumProvider` seam (`ethereum::EthCall` / `eth_call`, from `ethereum-provider-seam-and-trusted-rpc-backend`) and hands the returned contenthash bytes to the ENSIP-7 `decode_contenthash` decoder (`contenthash::DecodedContenthash` / `ContenthashError` / `ProtoCode`, from `ensip7-contenthash-decoder-typed-graceful-errors`) rather than re-decoding.

The composed path: `namehash` the name (ENSIP-1) -> `registry.resolver(node)` `eth_call` -> ABI-decode the resolver address (zero = `NoResolver`) -> `resolver.contenthash(node)` `eth_call` -> ABI-decode the dynamic `bytes` -> `decode_contenthash(bytes)` -> a decoded `ipfs://<cid>` success, or a distinct typed failure. It does NOT touch the URL bar or rendering (the front-door task `bare-eth-urlbar-front-door-end-to-end` wires those).

## Reproducing

```sh
cargo test -p werust-core --lib ens
```

15 tests, all off the live network: the ENSIP-1 known-answer namehash vectors; case-normalization; the unnormalizable-name failure; selector re-derivation via the bound keccak; the end-to-end fixture-name resolve to `ipfs://<cid>` (asserting the two calls' target + calldata); each distinct fail-closed path (no resolver, no contenthash, empty return, revert, RPC error on the resolver lookup, unsupported protocol, malformed contenthash return, short resolver return); and a full end-to-end resolve driving the REAL `RpcProvider` over a `127.0.0.1:0` loopback JSON-RPC fixture answering both calls in order.

## Drift re-check (per WORK-CONTRACT.md "Drift is a needs-attention signal")

Both blocking tasks are in `work/tasks/done/` and landed as this task assumed; confirmed against current source:

- `ethereum-provider-seam-and-trusted-rpc-backend`: the seam is `EthereumProvider::eth_call(&self, call: &EthCall) -> Result<Vec<u8>, ProviderError>` with `EthCall { to, data, block }` (`EthCall::new(to, data)` defaults `BlockTag::Latest`) and a typed `ProviderError` (`InvalidRequest` / `Transport` / `Rpc { code, message }` / `Decode`). Consumed as-is; no drift.
- `ensip7-contenthash-decoder-typed-graceful-errors`: the decoder is `decode_contenthash(bytes: &[u8]) -> Result<DecodedContenthash, ContenthashError>` with `DecodedContenthash::Ipfs { uri, cid }` (the supported case) / `Unsupported(ProtoCode)`, and `ContenthashError::{NoContenthash, Malformed, InvalidCid}`. Consumed as-is; no drift.

## Decisions

- **Canonical mainnet ENS registry address as a constant (`REGISTRY_ADDRESS = 0x00000000000C2E074eC69A0dFb2997BA6C7d2e1e`).** The registry is the well-known root contract and has been at this address on mainnet since the ENS redeploy; Phase 1 is mainnet-only (L2/other-chain ENS is spec Out of Scope), so there is no per-network config to chase (mirrors the `DEFAULT_RPC_ENDPOINT` / `DEFAULT_IPFS_GATEWAY` labelled-constant style). Recorded at the choice site. **Touches:** nothing else; a later multi-network task would parameterise it.

- **keccak256 is the bound `sha3::Keccak256` (RustCrypto), never hand-rolled.** ENS `namehash` and the ABI function selectors need Ethereum's LEGACY Keccak-256 (NOT NIST SHA3-256). `sha3::Keccak256` is exactly that legacy variant, from the SAME vetted RustCrypto `digest 0.10` family the `fetcher` crate already binds (`sha2 0.10`), so no version skew and a bound-not-hand-rolled primitive per `CONTEXT.md` / `docs/adr/0001`. **Touches:** adds `sha3 = "0.10"` to `werust-core`.

- **Name normalization is the bound `ens-normalize` crate (adraffy Rust port), returning `Result`.** ENSIP-1 folds the NORMALIZED name; a name that cannot be normalized (empty label, disallowed character) must be a DISTINCT fail-closed error, not a silently-mangled node. `ens-normalize 0.1.1` is a vetted Rust port of adraffy's reference ENS normalizer whose `normalize() -> Result` gives exactly that fail path, with a light dependency footprint (serde/serde_json, already in the tree). Chosen over the heavier `ens-normalize-rs` (~81 packages) and over hand-rolling Unicode normalization (violates the bind-vetted discipline). **Touches:** adds `ens-normalize = "0.1.1"` to `werust-core`; sets the user-visible name-normalization behaviour every ENS input flows through.

- **ABI encode/decode by hand for the fixed ENS shapes, no general ABI codec.** The only ABI shapes ENS resolution needs are trivial and fixed: encode a `fn(bytes32)` call (4-byte selector + one 32-byte word); decode a single `address` return (20 bytes right-aligned in a 32-byte word); decode a single dynamic `bytes` return (offset word -> length word -> payload). These are decoded directly against the well-specified ABI layout rather than pulling a full ABI/`alloy`/`ethabi` codec for two call shapes. This is layout handling, NOT crypto (the crypto is the bound keccak). ABI offset/length words are read as `usize` with an overflow guard, and any structurally-impossible framing is refused as `MalformedReturn` (never guessed). If a future task needs richer ABI, promoting to a bound codec is a clean follow-up. **Touches:** only the `ens` module's private helpers.

- **Function selectors as constants, re-derived in a test.** `RESOLVER_SELECTOR = 0x0178b8bf` (`resolver(bytes32)`) and `CONTENTHASH_SELECTOR = 0xbc1c58d1` (`contenthash(bytes32)`) are constants; a test re-derives both from their signatures via the bound keccak, so a typo cannot slip in silently and the keccak binding is proven to be the legacy-Keccak Ethereum uses.

- **The fail-closed failure taxonomy (`ResolutionError`).** Every failure step is its OWN distinct variant (spec story 3 — never a partial or guessed result):
  - `UnnormalizableName { name, detail }` — the name failed ENSIP-1 normalization (rejected before any `eth_call`).
  - `Provider(ProviderError)` — a read through the seam failed (transport / JSON-RPC error object / non-2xx / unparseable envelope), carrying the seam's own typed error. A reverting resolver lands here (a JSON-RPC error object).
  - `MalformedReturn(String)` — an `eth_call` succeeded but its ABI return bytes were the wrong shape (a resolver return too short for an address word; a `contenthash` return whose dynamic-`bytes` framing is impossible). Refused, never guessed.
  - `NoResolver` — the registry returned the ZERO address (no resolver set / name absent); short-circuits before `contenthash()`.
  - `Contenthash(ContenthashError)` — the returned contenthash bytes could not be decoded to a reference, carrying the decoder's own `NoContenthash` / `Malformed` / `InvalidCid` so "no site set" stays distinct from "broken bytes" from "bad CID". An empty (`0x`) `contenthash` return is passed through empty and surfaces as `NoContenthash` (a resolver with no record), NOT `MalformedReturn`.
  - `UnsupportedContenthash(ProtoCode)` — a WELL-FORMED contenthash for a protocol werust does not support in Phase 1 (Swarm/IPNS/Arweave/…). This is a NAMED refusal, NOT a success: `resolve` returns `Ok` ONLY for `DecodedContenthash::Ipfs`, so a caller cannot accidentally treat a "points to Swarm" name as loadable. Its `Display` reuses the ENSIP-7 decoder's own protocol-named reason so the two wordings cannot drift.

  Recorded at the choice site (the `ResolutionError` doc comments) and here. **Touches:** the front-door task turns each variant into a legible chrome load-failure message.
