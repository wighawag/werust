---
title: Gate-3 (conductor) verdict — fetcher-hash-verified-content-addressed-path — APPROVE
date: 2026-07-21
kind: observation
reviewOf: fetcher-hash-verified-content-addressed-path
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit a786f81)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review. This is
the technical CORE of the thesis (verify, don't trust), so the mismatch path got
extra scrutiny.

### Acceptance criteria — all met

- ✅ `Fetcher` seam exposes `fetch_verified(cid)` returning content ONLY after hash
  verification succeeds (via vetted `cid`/`multihash` crates, not hand-rolled).
- ✅ A hash mismatch fails LOUDLY: `VerifyError::HashMismatch` and NOTHING is
  returned — test `a_hash_mismatch_fails_loudly_and_never_returns_the_bytes`
  (uses `put_tampered_under`).
- ✅ Both matching and mismatching fixtures tested, plus malformed-CID rejection
  and unsupported-hash refusal.
- ✅ Content store isolated to a `TempDirContentStore` (tempfile), repo style.

### Nit triage

1. Split into UNTRUSTED `ContentSource` + `VerifyingContentFetcher` (verify layers
   over any source) — RATIFY/KEEP. Clean, un-bypassable-by-design. ACTIONED: the
   hand-off it names (the ipfs task MUST route its gateway through `fetch_verified`,
   never the raw source) is planted as a FORWARD-NOTE on
   `ipfs-scheme-resolution-through-renderer-seam`.
2. Scope = sha2-256 (0x12), single/raw-leaf-block; other hashes refused
   (UnsupportedHash), DAG-PB/UnixFS multi-block traversal OUT OF SCOPE — RATIFY.
   Honestly flagged. Folded into the same forward-note: the ipfs task must pin a
   single-block sha2-256 raw CID fixture and treat unsupported/multi-block as a
   failed load.
3. New public helper `cid_v1_raw_sha256(bytes) -> Result<String, VerifyError>`
   (the inverse of verify, used to store content under its CID) exported `pub` —
   KEEP. Pure helper, no seam leak, low risk. A "keep or make test-only" question
   for the human, but harmless as a public helper; not blocking.

### Forward-note planted (conductor step 2)

`ipfs-scheme-resolution-through-renderer-seam`: route the production gateway source
through `VerifyingContentFetcher::fetch_verified` (never the raw `ContentSource`) so
verification cannot be bypassed; map `HashMismatch`/unsupported/multi-block to a
FAILED load (never render); pin a single-block sha2-256 raw CID fixture (multi-block
DAG traversal is out of scope in the fetcher).

### What this unlocks

Landing this is one of the three deps of `ipfs-scheme-resolution-through-renderer-seam`
(the others: browser-shell [done] and eip1193 [pending]).
