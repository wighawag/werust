---
title: review-gate non-blocking nits for 't0-content-addressed-floor-parity' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: t0-content-addressed-floor-parity
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't0-content-addressed-floor-parity' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the in-scope design choice: verified ipfs:// bytes are rendered by re-wrapping them in a data:text/html, URL and re-navigating the native path, rather than a direct ipfs navigate. Is composing the two seams this way (verify via resolve_ipfs_request, then feed verified bytes to the T0 data: entry point) the intended shape?
  (render_bytes_transcript in t0_content_addressed_floor.rs builds a data: URL. Verified against backend.rs: the T0 NativeRenderer deliberately has NO networking / ipfs resolution and only renders self-contained data:text/html documents, so this is the only architecturally-available composition, not a shortcut. Coherent with the system language; flagged only because the PR carried no ## Decisions block recording it.)
- The PR/commit description carries no ## Decisions block. Future work-branch PRs should record non-obvious in-scope choices (here, the data:-URL seam composition) so the human ratifies them without re-deriving.
  (git show ce733d7 body is empty; the one decision above had to be reconstructed from the code.)
