---
title: review-gate non-blocking nits for 'fetcher-hash-verified-content-addressed-path' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: fetcher-hash-verified-content-addressed-path
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fetcher-hash-verified-content-addressed-path' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the content-addressed path is split into an UNTRUSTED ContentSource trait plus a VerifyingContentFetcher that layers verify over any source (recorded in the Decisions block). The consuming task ipfs-scheme-resolution-through-renderer-seam MUST route its production gateway source through fetch_verified so the verify is never bypassed. Reasonable and well-recorded; ratify.
  (crates/fetcher/src/lib.rs: ContentSource + VerifyingContentFetcher; PR Decisions block entry 1)
- Ratify: verification scope is sha2-256 (multihash code 0x12) for any CID version/codec addressing raw/leaf block bytes; other hash functions are refused as UnsupportedHash and DAG-PB/UnixFS multi-block traversal is explicitly OUT OF SCOPE (deferred to render/resolution tasks). Any fixture-pinning task should pin a single-block sha2-256 CID until DAG lands. Honestly flagged as a real limit; ratify.
  (PR Decisions block entry 2; verify_bytes_against_cid refuses non-0x12 codes)
- Ratify a new public API surface: cid_v1_raw_sha256(bytes)->Result<String,VerifyError> is exported pub as the inverse of verify (used to store content under its CID). It is used in tests; exporting it publicly is an in-scope choice not named by the task. Low risk (pure helper, no seam leak), but flag for the human to keep or make test-only.
  (crates/fetcher/src/lib.rs: pub fn cid_v1_raw_sha256)
