---
title: review-gate non-blocking nits for 't1-server-web-floor-article-and-blog' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: t1-server-web-floor-article-and-blog
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't1-server-web-floor-article-and-blog' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the goldens are pinned to a hard-coded 800px viewport width (FIXTURE_VIEWPORT_WIDTH), an in-scope default the agent chose that governs inline wrapping and thus every transcript line. It is not in the Decisions block. Low impact (test-local constant, regenerable), but a human should ratify that 800px is the intended pin so a later viewport change does not silently churn both goldens.
  (crates/native-renderer/tests/t1_server_floor_goldens.rs:41 (const FIXTURE_VIEWPORT_WIDTH = 800.0); not listed under Decisions in docs/spikes/.../README.md)
- Confirm the captured line-height cascade limitation (unitless line-height inherits as absolute px, not a multiplier) is tracked to fix in the T1 core-CSS cascade, not just noted. D3 correctly leaves it out of THIS task and the fixtures avoid it, but the observation is a real cross-task cascade defect that no task yet owns.
  (work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md; verified real at css.rs:826-830 (l.resolve(style.font_size) stored + inherited as absolute); css.rs:1076 test only covers a single element so it does not catch the inherited-child case)
