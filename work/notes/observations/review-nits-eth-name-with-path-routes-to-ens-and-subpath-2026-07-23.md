---
title: review-gate non-blocking nits for 'eth-name-with-path-routes-to-ens-and-subpath' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: eth-name-with-path-routes-to-ens-and-subpath
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'eth-name-with-path-routes-to-ens-and-subpath' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the split-at-first-slash rule: a .eth entry folds everything from the first / (including any query/fragment) into the sub-path, so ronan.eth/blog?x=1#frag loads ipfs://<cid>/blog?x=1#frag. Is passing the raw query/fragment into the ipfs sub-path target the intended posture, or should they be stripped/handled separately?
  (eth_name_and_path_from_entry splits on entry.find('/'); load_resolved_content does format!('{uri}{path}'). Not covered by an acceptance criterion or a test.)
- Ratify the no-slash-with-query fallthrough: an entry like ronan.eth?x=1 (a .eth label but no /) fails the .eth suffix check and routes to https:// instead of ENS. Acceptable for Phase 1 (bare .eth + path), or a gap?
  (eth_name_and_path_from_entry: with no /, name=='ronan.eth?x=1', eth_name_from_entry rejects it, so navigate falls to classify_entry -> HttpsCandidate.)
- The PR/commit body carries no '## Decisions' block; the split-at-first-slash choice (query/fragment folded into the path) is an in-scope decision the human should ratify.
  (git log body is a single feat line; the decision is implicit in the diff.)
