# Verifiable IPFS content retrieval: a `ContentRetriever` seam + a trustless-gateway CAR backend that byte-verifies a UnixFS DAG

Context: werust could fetch-and-verify a SINGLE IPFS block by re-hashing a gateway's bytes against the root CID (`VerifyingContentFetcher` in the `fetcher` crate), and `resolve_ipfs_request` ignored the path. Every real `.eth`/`ipfs://` site is a multi-block UnixFS DAG (a directory of `index.html` + assets, or a chunked file), so re-hashing the gateway's reassembled bytes against the root CID FAILS `HashMismatch` on every real site: the root CID names a `dag-pb` node that LINKS to child blocks, not the reassembled content. The only honest way to prove a multi-block site is to walk the DAG and verify EACH block against its OWN CID, then reassemble locally. This is a security-relevant trust trade-off, so it is recorded here (the ADR gate: `docs/adr/0001` trust honesty).

## The decisions

1. **Retrieval is a `ContentRetriever` seam, modelled like `EthereumProvider`/`Fetcher`/`Renderer`.** The seam is the abstraction ("given a CID + a path into the DAG, return the verified bytes for that resource, or a typed failure"); the trust/transport is a swappable BACKEND. It lives in the `fetcher` crate next to `ContentAddressedFetcher`. This makes delegated-routing, an embedded-p2p (Phase-2 async) client, and a user-supplied gateway/node URL pure backend swaps, not rewrites. The user-facing selector is a separate task (`retrieval-backend-user-setting`); this ADR lands the seam + the ONE default backend so that setting has something to switch between.

2. **The default backend is a trustless-gateway CAR fetcher: NO IPFS node, NO async runtime.** It GETs `<gateway>/ipfs/{cid}?format=car` (with `dag-scope`/`entity-bytes` so it fetches only the blocks a resource needs) over the EXISTING synchronous `Fetcher` seam, parses the CAR byte stream into its raw blocks, verifies each block against its own CID, decodes the `dag-pb`/UnixFS DAG, resolves the requested path (directory -> `index.html`, `ipfs://<cid>/sub/resource` into the DAG), and reassembles the leaf bytes locally. A CAR fetch is just a GET whose body is a CAR stream, so it fits the sync seam; the new work is CAR parse + per-block verify + UnixFS reassemble + path resolution, not a new transport. The `DEFAULT_IPFS_GATEWAY` const + a `with_*()` override (the existing `GatewayContentSource` pattern) makes the gateway endpoint swappable with no new config subsystem.

3. **Codec-gated verification (trust honesty, `docs/adr/0001`).** Discriminate by the CID's multicodec: a `raw` (0x55) CID's block bytes ARE the content and are verified directly, a mismatch is a HARD fail-closed tamper error that is NEVER served; a `dag-pb` (0x70) CID is a UnixFS DAG root and goes through the walk-and-verify-each-block path. Every leaf and intermediate block is hash-checked against its own CID before its bytes contribute to the reassembled resource. "content-verified" therefore means EVERY byte was hash-checked; no path serves bytes on a hash failure.

4. **Fail-closed + a retrieval budget, each failure distinct.** The Trustless Gateway spec makes CAR completeness the CLIENT's obligation: a block that does not hash to its CID (`BlockHashMismatch`), a link to a block the CAR never delivered (`MissingBlock`), a truncated/incomplete CAR stream (`IncompleteCar`), or a resource path that does not resolve (`PathNotFound`) each fail the load with a distinct, legible reason. A retrieval BUDGET (max total bytes / max block count / wall-clock) refuses a runaway or hostile DAG so a malicious gateway cannot stream forever (`BudgetExceeded`). Nothing unverified is ever rendered or labelled verified.

5. **UnixFS scope.** IN: `raw` leaves, chunked/multi-block files, directories with `index.html` resolution, and HAMT-sharded directories. OUT (named follow-ons): symlinks, and non-UnixFS `dag-cbor`/`dag-json` DAGs (each rejected with a distinct typed error, never guessed).

## Considered options (crate binding)

The task named `rs-car-ipfs` / `rs-car`, but both are ASYNC (`futures`/`async-std`) and `rs-car-ipfs`'s UnixFS decode is `pub(crate)` (not reusable), which conflicts with decision (2)'s "no async runtime, over the sync seam." Resolved to the task's INTENT (bind vetted crates; do not hand-roll CAR layout, dag-pb, or crypto) by binding the SYNC siblings on the SAME `ipld-core`/`cid 0.11` lineage the repo already uses:

- **`rs-car-sync`** (CARv1/v2 block reader with per-block CID verification, sync `std::io::Read`) for CAR parse + the first per-block hash check. Re-exports the repo's `cid 0.11` type via `ipld-core`, so no CID-type impedance at the trust boundary.
- **`ipld-dagpb`** (sync dag-pb decode to `PbNode { data, links }`, on `ipld-core`/`cid 0.11`) for the dag-pb layer.
- **`quick-protobuf`** for a small BOUND decode of the UnixFS `Data` message (Type / filesize / blocksizes), against the canonical `unixfs.proto` field tags. This is a bound protobuf decode, not hand-rolled crypto or block layout.

Rejected: **`unixfs-v1`** (has a public `FlatUnixFs` + `dir::resolve` incl. HAMT, sync) because it drags in a SECOND CID/multihash lineage (`libipld 0.14` / `multihash 0.16`) distinct from the repo's `cid 0.11`, forcing a serialize-and-reparse bridge at every link boundary right where the trust check lives. Keeping ONE CID lineage at the verify boundary is safer and clearer, which outweighs re-implementing the (well-specified) DAG walk / directory / HAMT lookup over the bound primitives above.

## Consequences

- The per-block verify is done twice defensively: once by `rs-car-sync` (`validate_block_hash = true`) as the CAR is read, and again by werust as each block's bytes are consumed into the reassembly, so a future CAR-reader swap cannot silently drop the check.
- HAMT-sharded directory lookup is implemented over the bound decoders rather than delegated, because the delegating crate (`unixfs-v1`) was rejected on the CID-lineage grounds above.
- The seam's async-p2p backend (Phase 2) will need an async variant of the walk; the sync walk here is the reference semantics that backend must match.
