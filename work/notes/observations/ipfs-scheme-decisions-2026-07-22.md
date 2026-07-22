---
title: Decisions — ipfs-scheme-resolution-through-renderer-seam
date: 2026-07-22
kind: observation
reviewOf: ipfs-scheme-resolution-through-renderer-seam
---

## Decisions taken while wiring `ipfs://` through the Renderer seam

Recorded for reviewer/human ratification (per the durable-decision rule). None are load-bearing/hard-to-reverse; the build proceeded.

1. **Default IPFS gateway `https://dweb.link`** (a USER-VISIBLE default: a network endpoint the browser contacts). Chosen as a public IPFS HTTP gateway to give `install_ipfs` a working production `ContentSource` out of the box. Alternatives considered: a local IPFS node (not assumable day-one), a bundled list. Fully reversible: `GatewayContentSource::with_gateway(fetcher, base)` swaps it, and the gateway is UNTRUSTED by design (the `VerifyingContentFetcher` hash-gates every load regardless of which gateway served the bytes, so a hostile/wrong gateway cannot render unverified content). The durable gateway/peer POLICY (which gateway, or a local node, and whether content-addressed loads relax origin trust) is already an open question on the exploration spec `rust-successor-native-renderer-architecture-benchmark` (spec Out of Scope) — this task only binds a working default. JSDoc at the choice site: `werust_core::ipfs::DEFAULT_IPFS_GATEWAY`. Touches no other task/flag.

2. **MIME type inferred from the `ipfs://<cid>/path` extension, defaulting to `text/html`** for the CID root or an unknown/absent extension. This gives served-page parity (a bare `ipfs://<cid>` opens as a document, not a download) since the fetcher returns raw verified bytes with no content-type. A small extension→MIME table covers common web types. Choice site: `werust_core::ipfs::mime_type_for_path`. Reversible; touches nothing else.

3. **Re-export `cid::Cid` from the `fetcher` crate** (`pub use cid::Cid;`). The public `ContentSource::get(&self, cid: &Cid)` signature already leaks `cid::Cid`, so any out-of-crate implementor (the production `GatewayContentSource`, and the test sources) needs the type. Re-exporting from the seam avoids callers depending on the `cid` crate directly and risking a version skew with the one the seam verifies against. Small, factual, self-contained; recorded only because it touches the `fetcher` public surface.

## Scope notes (matching the task forward-pointer)

- Production `ipfs://` bytes route THROUGH `VerifyingContentFetcher::fetch_verified` (never the raw `ContentSource`), so a `VerifyError` (`HashMismatch`/`UnsupportedHash`/`InvalidCid`/`Source`) maps to a failed load (`RendererError::Backend`) and NEVER renders. Test fixtures pin single-block `sha2-256` raw CIDs via `cid_v1_raw_sha256`, off the live network. Multi-block/DAG-PB CIDs are out of scope in the fetcher and surface as a failed load, not a trusted render.
