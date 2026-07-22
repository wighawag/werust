---
title: Gate-3 (conductor) verdict — fix-unitless-line-height-inherits-as-multiplier — APPROVE
date: 2026-07-22
kind: observation
reviewOf: fix-unitless-line-height-inherits-as-multiplier
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 2726347)

Fix task authored at the human's explicit request ("fix it") for the orphaned
line-height defect flagged at the t1-server-floor review. `do` ran Gate-1 + Gate-2,
both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Unitless `line-height` now inherits as a MULTIPLIER: `css::LineHeight` is an enum
  `Normal` / `Absolute(px)` / `Multiplier(n)`, and `Multiplier` resolves as
  `n * font_size` against EACH element's own font-size at use. `ComputedStyle.line_height`
  is this type (was a bare `f32`).
- ✅ Unit-bearing (`24px`) inherits as a fixed `Absolute`; unset stays `Normal`
  (font-size-relative, shaper 1.2 approximation) — regression test
  `unit_bearing_line_height_inherits_as_a_fixed_px_across_font_sizes`.
- ✅ The previously-MISSING parent/child-different-font-size case is now tested
  (`unitless_line_height_inherits_as_a_multiplier_not_a_fixed_px`).
- ✅ The `t1-wpt-subset-regression-meter` core-CSS case flipped to a passing
  expectation (`line-height-unitless-inherits-as-multiplier`, expect 15 = 1.5 × 10 for
  a `small` at 10px inheriting the body multiplier); the ≥70% threshold stays green.
- ✅ All goldens green (fixtures avoid unitless line-height); gate green.

### Nit triage — both benign

1. Two-enum shape (public `css::LineHeight` for the computed value + private
   `LineHeightDecl` for the pre-font-size declared form), meter reports `Normal` as the
   string `normal`, `Normal` keeps the shaper 1.2 approximation — all reasonable and
   inside the task guidance; only lacked a Decisions block. KEEP.
2. No `## Decisions` block in the commit — recurring benign process nit. KEEP.

### Resolution

The orphaned cascade defect (flagged on the t1-server-floor Gate-3, counted as a real
failure by the WPT meter) is now FIXED and its meter case PASSES. Stuck-set item #2
(the line-height defect) is resolved.
