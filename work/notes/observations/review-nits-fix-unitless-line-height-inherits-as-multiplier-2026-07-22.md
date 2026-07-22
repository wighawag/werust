---
title: review-gate non-blocking nits for 'fix-unitless-line-height-inherits-as-multiplier' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-unitless-line-height-inherits-as-multiplier
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-unitless-line-height-inherits-as-multiplier' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the in-scope shape decisions: line-height is modelled as TWO enums (public css::LineHeight = Normal/Absolute/Multiplier, plus a private LineHeightDecl for the pre-font-size declared form), the WPT meter reports the Normal variant as the string normal (not a px), and Normal keeps mapping to the shaper-side 1.2 approximation. All are reasonable and inside the task guidance, but none were recorded in a Decisions block in the PR description.
  (css.rs LineHeight/LineHeightDecl enums; wpt_meter/core_css.rs line 163 None => normal; shape.rs resolve_line_height None => FontSizeRelative(1.2))
- The PR/commit description has no ## Decisions block at all; future reviewers rely on it to start ratification. Minor process gap, not a defect.
  (git log body for d58f8ce is a single title line)
