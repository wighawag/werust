---
title: review-gate non-blocking nits for 'ens-history-name-rederive-async-and-normalized' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: ens-history-name-rederive-async-and-normalized
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ens-history-name-rederive-async-and-normalized' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: normalize_ens_page_key reduces ipfs://<cid> to a BARE <cid> key (scheme + authority dropped). A user who navigates DIRECTLY to ipfs://<cid> (not via the ENS front door) for a CID previously ENS-resolved now normalizes to the same key and would surface the .eth name from ens_pages. This direct-CID collision pre-existed (the old raw-string key collided too) and is only WIDENED here (ipfs:/// variants now also collide); ens_pages is still insert-only via the ENS front door. Confirm this widening is acceptable or scope a follow-up.
  (crates/werust-core/src/ipfs.rs normalize_ens_page_key returns cid/path with no scheme; lookup in refresh_chrome/reload/load_resolved_content. Prior task flagged direct-CID collision for the human.)
- Ratify: the key normalization also collapses a bare trailing slash (ipfs://cid == ipfs://cid/) and preserves deeper sub-paths as identity. This trailing-slash equivalence is a small user-visible key policy not spelled out in the task (task said 'ideally reduce to canonical <cid>[/path]'); low risk but worth recording as a deliberate choice.
  (normalize_ens_page_key trims a bare trailing slash via path.trim_end_matches('/'); tests normalize_ens_page_key_ignores_a_bare_trailing_slash / keeps_a_real_sub_resource_path.)
- No '## Decisions' block was recorded in the PR/commit body; the two in-scope normalization choices above were surfaced by review rather than by the agent. Recommend the human capture them so the collision semantics are pinned for the next author.
  (git log -1 body empty; task file has no Decisions section.)
