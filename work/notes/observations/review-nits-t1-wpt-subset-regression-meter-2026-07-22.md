---
title: review-gate non-blocking nits for 't1-wpt-subset-regression-meter' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: t1-wpt-subset-regression-meter
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't1-wpt-subset-regression-meter' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D2: the core-CSS half is a pinned computed-value subset MODELLED on the five WPT areas, not the raw upstream WPT testharness/reftest corpus. A literal reading of story 17 (run css/CSS2/normal-flow, css/css-box, etc.) implies the real WPT tests; the agent substituted hand-authored computed-value cases driven through the native cascade. Reasonable and well-argued (raw files need a JS runtime=T3 or a reference browser, so they are non-hermetic under verify), recorded with a swap point, and the tree-construction half DOES use the exact upstream .dat format. Human should ratify that a modelled core-CSS subset is acceptable for the T1 bar until a JS/reference-render path lands.
  (README D2 + core_css.rs + SOURCE.md; contrast tree_construction.rs which uses real .dat format)
- Ratify D3: line-height-unitless-inherits-as-multiplier is left IN the core-CSS set and counted as a real failure (26/27), exposing the known orphaned cascade defect rather than hiding it. Correct for a regression meter; flagged only so the human knows the bar intentionally sits below 100%.
  (README D3; cases.txt line 232; note t1-unitless-line-height-inherits-as-absolute-px.md)
- Minor: the tree-construction .dat cases and core-CSS cases are hand-authored to pinned behaviours (frozen 2026-07-22), not byte copies of upstream, so the reported percentages are not directly comparable to a public WPT run. Documented honestly in SOURCE.md; the comparable-over-time property holds within werust. No action needed unless external comparability is later required.
  (SOURCE.md pinning notes)
