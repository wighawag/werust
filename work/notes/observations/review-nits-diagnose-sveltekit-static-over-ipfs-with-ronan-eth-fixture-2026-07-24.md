---
title: review-gate non-blocking nits for 'diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture' (Gate 2 approve)
date: 2026-07-24
status: open
reviewOf: diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- In-scope decision to RATIFY: the fix strips the query/fragment ONLY in parse_ipfs_uri (the retrieval seam), NOT in normalize_ens_page_key / ipfs_root_cid_and_path (the ENS-bar-key seam). This is correct for the reported bug (the __data.json subresource fetch flows through resolve_ipfs_request), but means a bar-display or history URL that ever carried a query would key differently. Human to confirm the ENS-key seam never needs the same strip.
  (crates/werust-core/src/ipfs.rs parse_ipfs_uri vs normalize_ens_page_key; lib.rs ens_identity_for_url uses the un-stripped key path.)
- The portfolio-works/blog-fails asymmetry is attributed to symptom-ordering by reasoning from SvelteKit source, NOT confirmed against a live ronan-eth build (absent from this worktree). Recorded as an open observation note; fine to leave for the next on-device pass.
  (work/notes/observations/sveltekit-ipfs-query-strip-portfolio-vs-blog-asymmetry-2026-07-24.md)
