---
title: review-gate non-blocking nits for 'fix-release-setup-zig-pin-version' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-release-setup-zig-pin-version
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-release-setup-zig-pin-version' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the in-scope decision: agent pinned Zig to 0.14.1 specifically (task offered it only as an e.g.) and deliberately left cargo-zigbuild UNPINNED (installs latest). 0.14.1 is a released stable referenced by current cargo-zigbuild docs, so the pairing is sound today. Residual risk: a future 'latest' cargo-zigbuild could require a Zig range excluding 0.14.x, silently re-breaking the leg. Human may want to also pin cargo-zigbuild as a follow-up.
  (.github/workflows/release.yml:98-105; cargo-zigbuild README references zig 0.14.1)
- Coherence note (pre-existing, not introduced here): the header calls this the 'deliberately Zig-less build path' yet the goreleaser job installs Zig for cargo-zigbuild's cross-linker. The phrasing predates this diff and refers to the app not depending on Zig; no action needed, flagged only so the term is not later re-forked.
  (.github/workflows/release.yml:3-4 vs 98)
