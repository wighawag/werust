---
title: review-gate non-blocking nits for 't1-content-addressed-floor-ipfs-static-site' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: t1-content-addressed-floor-ipfs-static-site
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't1-content-addressed-floor-ipfs-static-site' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the parity-reference choice: this T1 test asserts parity by rendering the SAME bytes two ways in-test (served data: path vs verified ipfs:// path) and asserting byte-equality, PLUS a NEW committed golden (site.golden.txt). The T0 sibling (t0_content_addressed_floor.rs) instead reuses the server floors OWN golden as the reference. The divergence is fine (the in-test dual-render equality is a stronger, direct parity proof), but it is a cross-task pattern choice worth a human nod.
  (t1_content_addressed_floor.rs dual assert_eq vs t0_content_addressed_floor.rs golden reuse)
- Ratify the fixture provenance: the pinned site is a hand-authored ORIGINAL page in the spirit/shape of a Jekyll/Hugo static site (frozen 2026-07-22, CID derived from bytes), NOT a captured live-IPFS site. The task said a real ipfs:// static site (pinned CID). SOURCE.md documents this transparently with a re-pin path, and it matches the sibling hand-authored fixture pattern, so this is a documented interpretation, not a gap.
  (fixtures/t1-content-addressed-floor/SOURCE.md provenance + re-pinning section)
- The commit/PR carries NO Decisions block, so the two in-scope choices above (new golden vs reused golden; authored snapshot vs live capture) were not recorded by the agent for ratification. Not a defect in the code; just a missing decision record.
  (git log c7a9388 bare message)
