---
title: review-gate non-blocking nits for 'release-notes-lose-the-conventional-commit-changelog-to-a-race' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: release-notes-lose-the-conventional-commit-changelog-to-a-race
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'release-notes-lose-the-conventional-commit-changelog-to-a-race' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- ADR-0002's Update section still documents the legs as running gh release create with --generate-notes, which this change forbids. Should the ADR carry an Update noting the empty-bodied create plus release.mode replace, the way CONTEXT.md was updated?
  (docs/adr/0002-release-via-goreleaser-rust-builder-a-zig-less-build-path.md:91 still reads: idempotently gh release create $TAG --generate-notes ... before gh release upload --clobber. CONTEXT.md:41 was updated; the ADR was not.)
- docs/spikes/windows-release-packaging-leg/README.md line 33 justifies putting the zip's contents facts in README.md rather than the Release notes because every leg creates the Release with --generate-notes and the notes are therefore generated from conventional commits. That premise was wrong then (auto-notes are not conventional-commit derived) and is now doubly stale. Worth a one-line correction?
  (docs/spikes/windows-release-packaging-leg/README.md:33)
- New user-visible failure mode to ratify: with --generate-notes gone, if the goreleaser leg fails on a tag (exactly what happened twice on v0.2.9) the published Release now carries an EMPTY body instead of GitHub's auto-notes. Acceptable, or should a leg write a minimal fallback? It is not in the spike's Known limits list.
  (.github/workflows/release.yml artifact legs now use gh release create --notes '' ; docs/spikes/.../README.md Known limits covers the all-bookkeeping-range empty body but not the goreleaser-failed case.)
- Ratify decision 3: the bookkeeping exclude was widened beyond what the task named (surface task: and task:) to the whole prefix family tasking|task|notes|obs|findings|spec|review plus + compounds. This is a user-visible default deciding what a release page says. Reversing it is one line.
  (.goreleaser.yaml changelog.filters.exclude; guarded against over-reach by no_exclude_filter_can_eat_a_feature_or_a_fix in crates/werust-core/tests/release_plumbing_shape.rs)
- Ratify decision 5 and note the human action outstanding: the v0.3.0 backfill body is prepared but deliberately NOT applied, so v0.3.0's page still shows one PR title until a human runs gh release edit v0.3.0 --notes-file docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/v0.3.0-release-notes.md
  (docs/spikes/.../v0.3.0-release-notes.md, 41 entries; the task said backfilling was worth doing but the agent judged a forge mutation from an autonomous build unreviewable.)
