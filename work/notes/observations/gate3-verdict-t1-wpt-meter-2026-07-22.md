---
title: Gate-3 (conductor) verdict — t1-wpt-subset-regression-meter — APPROVE (modelled core-CSS subset flagged for ratify)
date: 2026-07-22
kind: observation
reviewOf: t1-wpt-subset-regression-meter
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 0e1e132)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ The named WPT subsets run against the native T1 path and produce a pass-rate
  (tree-construction via upstream `.dat` format; core-CSS via a computed-value
  subset — see nit 1). Fixtures VENDORED under `tests/fixtures/t1-wpt/` (offline,
  no network at test time; `SOURCE.md` records provenance).
- ✅ Thresholds enforced: `TREE_CONSTRUCTION_THRESHOLD = 0.90`,
  `CORE_CSS_THRESHOLD = 0.70`, asserted fail-below.
- ✅ Complex-script/bidi excluded, and the meter ASSERTS no such area leaks into the
  bar.
- ✅ Runnable in CI (`cargo test`), reports a comparable-over-time number.

### Nit triage

1. **Core-CSS half is a computed-value subset MODELLED on the five WPT areas, not
   the raw upstream WPT corpus** — APPROVED, flagged for human ratify. The raw
   upstream css WPT tests are testharness.js/reftest-based: they need a JS runtime
   (T3, out of scope) or a reference browser, so they are NON-hermetic under the
   pure-Rust `verify` gate. Modelling the computed-value assertions on those areas
   and driving them through the native cascade is the only hermetic way to get a
   comparable T1 core-CSS number; it is well-argued, recorded with a swap point, and
   the tree-construction half DOES use the exact upstream `.dat` format. The task's
   real goal (a comparable-over-time regression number) is met. RATIFY question for
   the human: is a modelled core-CSS subset acceptable for the T1 bar until a
   JS/reference-render path lands? (Connects to the exploration spec's benchmark
   comparability question.)
2. The orphaned unitless-line-height defect is left IN the core-CSS set and counted
   as a real FAILURE (26/27), exposing it honestly rather than hiding it — CORRECT
   for a regression meter (bar sits intentionally below 100%, above the 70% floor).
   This now MEASURES the defect I flagged on the t1-server-floor review. KEEP.
3. Cases are hand-authored/frozen (2026-07-22), not byte-copies of upstream, so the
   percentages are not directly comparable to a PUBLIC WPT run — documented in
   SOURCE.md; comparable-over-time WITHIN werust holds. KEEP (external comparability
   is a future concern if needed).

### What this unlocks

Leaf task (the objective T1 regression guard). Does not unlock new tasks. Together
with the page floors it completes the T1 "reached" proof + regression net.
