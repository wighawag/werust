---
title: Gate-3 (conductor) verdict — ipfs-scheme-resolution-through-renderer-seam — APPROVE
date: 2026-07-22
kind: observation
reviewOf: ipfs-scheme-resolution-through-renderer-seam
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 62589a8)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review. This is the
second trust hook + the thesis's user-facing payoff, so the verify-gates-the-load
path got extra scrutiny.

### Acceptance criteria — all met

- ✅ `ipfs://<cid>...` navigates and renders the content-addressed page via the
  webview backend (`werust-core/src/ipfs.rs`, wired to the seam's custom-scheme hook).
- ✅ Served through the interception hook, resolved by the hash-verified `Fetcher`
  path; a hash mismatch (or ANY verify failure) FAILS the load, never rendering
  unverified bytes (`verify_error_to_renderer_error` maps every `VerifyError` to a
  `RendererError` the backend surfaces as a failed load).
- ✅ Parity with the served page (raw verified bytes + MIME inferred from the path).
- ✅ Tests cover scheme -> verified-fetch -> render with a pinned fixture CID and a
  tampered-content mismatch case, with NO network access.

### FORWARD-NOTE HONOURED (conductor value confirmed)

My forward-note (planted after the verified-fetch task landed) was followed exactly:
resolution routes through `VerifyingContentFetcher`/`ContentAddressedFetcher`
(`fetch_verified` path), NEVER the raw `ContentSource`; `HashMismatch`/`UnsupportedHash`/
`InvalidCid`/`Source` all map to a FAILED load; and the tests pin single-block
sha2-256 CIDs via `cid_v1_raw_sha256` + `put_tampered_under` for the mismatch case —
precisely the scope the note specified.

### Nit triage — all RATIFY/KEEP

1. Default gateway hardcoded `https://dweb.link` — untrusted-by-design (verification
   hash-gates every load, so a hostile/wrong gateway cannot render unverified bytes);
   reversible via `with_gateway`; durable gateway/peer policy is an open question on
   the exploration spec (out of scope). KEEP.
2. MIME inferred from path extension, default `text/html` — served-page parity (the
   fetcher returns raw bytes with no content-type); reversible. KEEP.
3. `pub use cid::Cid` re-exported from the fetcher — `ContentSource::get` already
   exposes `cid::Cid`; re-export avoids caller version skew. Self-contained. KEEP.

### What this unlocks

This is the SECOND trust hook: with provider-injection (eip1193) + ipfs-scheme both
wired, the webview backend genuinely satisfies BOTH trust hooks the qualification
gate encodes. Landing it unlocks `trust-indicator-verified-vs-served`,
`t0-content-addressed-floor-parity` (its other dep native-renderer-t0 is done), and
(with t1-core-css, done) `t1-content-addressed-floor-ipfs-static-site`.
