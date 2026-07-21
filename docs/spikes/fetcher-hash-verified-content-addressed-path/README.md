# Spike: the hash-verified content-addressed fetch path (the `ipfs://` fetch half)

Durable evidence + decisions for task `fetcher-hash-verified-content-addressed-path` (spec story 9).

## What was built

The content-addressed half of the `Fetcher` seam, in `crates/fetcher/src/lib.rs`, layered ON TOP of the existing HTTP+TLS seam:

- **`ContentSource`** trait: `get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError>`. Where candidate bytes for a CID come from (an IPFS gateway over the HTTP `Fetcher`, a local blockstore, a peer, or a temp-dir store in tests). A source is UNTRUSTED: whatever it returns is verified before it is ever handed back.
- **`ContentAddressedFetcher`** trait: `fetch_verified(&self, cid: &str) -> Result<Vec<u8>, VerifyError>`. The seam surface for content-addressed fetch. The rest of werust obtains content-addressed bytes ONLY through this trait, so the verify can never be skipped by a caller.
- **`VerifyingContentFetcher<S: ContentSource>`**: the concrete implementation that parses the CID, asks the source for bytes, RE-COMPUTES the digest over them, and returns them only on a hash MATCH. A mismatch is `VerifyError::HashMismatch` and nothing is returned.
- **`VerifyError`**: `InvalidCid` (unparseable), `UnsupportedHash { code }` (a hash function this path does not implement yet, refused not trusted), `Source(FetchError)` (source miss/transport failure), `HashMismatch { cid }` (the load-bearing loud failure).
- **`cid_v1_raw_sha256(bytes) -> String`**: derives the canonical CIDv1 (raw codec, sha2-256 multihash) that addresses `bytes`, the inverse of the verify, used to store content under its CID (in the store, and in tests).

Verification recomputes the digest with the hash function the CID's multihash names (via the vetted `cid` / `multihash` crates for parsing, `sha2` for the digest) and compares it to the digest the CID carries. TLS/crypto primitives are bound, never hand-written (`CONTEXT.md`, `docs/adr/0001`); CID PARSING is byte layout, not a cryptographic primitive, so it is delegated to `cid`.

## Reproducing

```sh
cargo test -p fetcher
```

Six content-addressed tests run headless, each isolating its content store to a fresh `tempfile::tempdir()` (no live network): the matching case (content stored under its real CID fetches back verified), the mismatching case (tampered bytes under a real CID fail loudly with `HashMismatch`, never returned), a malformed CID rejected before touching the source, a source miss surfaced (not a silent empty pass), an unsupported hash function refused (not assumed to match), and a store/verify round-trip guard.

## Decisions

- **Split the path into an untrusted `ContentSource` + a `VerifyingContentFetcher` over it.** The task's technical core is the VERIFY, which must be identical wherever the bytes came from. So "get candidate bytes for a CID" is abstracted behind `ContentSource` and verification is layered on top of ANY source. Rationale: it keeps this seam's job (verify) separate from the origin's (produce bytes), lets tests isolate a temp-dir store, and lets the consuming task (`ipfs-scheme-resolution-through-renderer-seam`) plug an IPFS gateway (over the HTTP `Fetcher`) in as the source without re-implementing the verify. Alternative considered (rejected): bake a gateway URL/network fetch directly into the content-addressed fetch, which would couple the verify to one origin kind and make it un-isolatable in tests. **Touches:** the consuming task provides the production `ContentSource` (gateway); it MUST route through `VerifyingContentFetcher` (or `fetch_verified`) so the verify is not bypassed.
- **CID scope: any CID version/codec, `sha2-256` multihash (code `0x12`), the IPFS default; other hash functions are REFUSED, not trusted.** The path parses the full CID and verifies the BLOCK bytes it is handed against the CID's `sha2-256` digest. A CID naming a different multihash function returns `VerifyError::UnsupportedHash`, an explicit refusal, never a silent pass (rejecting-when-unsure is the trust stance, `docs/adr/0001`). **Out of scope (a real limit, recorded so it is not mistaken for done):** DAG-PB / UnixFS traversal, i.e. a CID whose block is an IPLD node linking child blocks rather than the content itself. This seam verifies the block bytes against the CID; assembling a multi-block DAG (chunked files, directories) belongs to the render/resolution tasks that pin real multi-block fixtures (`t1-content-addressed-floor-ipfs-static-site`). A single-block (raw / leaf) CID is the honest slice the thesis needs first. **Touches:** any task pinning a fixture CID should pin a single-block sha2-256 CID until DAG traversal lands.
- **Bind `cid` + `multihash` + `sha2`, do not hand-parse CIDs.** The `cid` crate (which re-exports `multihash` and `multibase`) is the canonical, vetted CID parser; `sha2` is a vetted SHA-2 implementation. Hashing is a cryptographic primitive (bound, never hand-written, per the thesis); CID PARSING is not dangerous (byte layout), but using the canonical crate means the sibling tasks that pin REAL fixture CIDs just work rather than tripping a bespoke parser. Pinned `sha2 = "0.10"` (the stable, widely-used line) rather than the newer 0.11 that is still stabilising. **Touches:** only the `fetcher` crate's dependency set; none of these leak past the seam (the seam surface is `Cid` + `Vec<u8>` + `VerifyError`).
- **A distinct `VerifyError`, separate from `FetchError`.** The content-addressed failure modes (malformed CID, unsupported hash, hash mismatch) are different from the HTTP seam's (`InvalidUrl`, `Tls`, `Transport`, `Io`); a source failure is carried through as `VerifyError::Source(FetchError)`. Rationale: the mismatch is the load-bearing case and deserves to be its own legible variant, not folded into a generic transport error. **Touches:** the consuming task pattern-matches `VerifyError`; a `HashMismatch` must fail the load (not render unverified bytes).

## Notes

- The temp-dir content store in the tests keys blobs by the CID's canonical string, plays the role of an untrusted origin, and is removed on `Drop`, with no live-network or cross-test dependency.
