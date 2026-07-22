---
title: review-gate non-blocking nits for 'ipfs-scheme-resolution-through-renderer-seam' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: ipfs-scheme-resolution-through-renderer-seam
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipfs-scheme-resolution-through-renderer-seam' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: default IPFS gateway hardcoded to https://dweb.link in DEFAULT_IPFS_GATEWAY (a user-visible network default the browser contacts).
  (Recorded decision #1. Reversible via GatewayContentSource::with_gateway; gateway is untrusted-by-design (VerifyingContentFetcher hash-gates every load) so a hostile/wrong gateway cannot render unverified bytes. Durable gateway/peer policy is an open question on the exploration spec, out of scope here.)
- Ratify: response MIME inferred from the ipfs://<cid>/path extension, defaulting to text/html for the root or unknown extension.
  (Recorded decision #2 (mime_type_for_path). Gives served-page parity since the fetcher returns raw verified bytes with no content-type. Reversible, touches nothing else.)
- Ratify: pub use cid::Cid re-exported from the fetcher crate.
  (Recorded decision #3. ContentSource::get already leaks cid::Cid; re-exporting from the seam avoids callers depending on the cid crate directly and risking version skew. Small, self-contained, touches the fetcher public surface.)
