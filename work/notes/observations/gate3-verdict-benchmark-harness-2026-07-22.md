---
title: Gate-3 (conductor) verdict — native-renderer-benchmark-harness-capability-and-trust-hooks — APPROVE
date: 2026-07-22
kind: observation
reviewOf: native-renderer-benchmark-harness-capability-and-trust-hooks
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 3e165ae)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review. Extra care
on the scope boundary with the GATED exploration spec (which I do not touch).

### Acceptance criteria — all met

- ✅ Scores a candidate on the pinned page checklists + WPT subsets (`CapabilityScore`)
  and on trust-hook qualification (`TrustHookScore`, PASS/FAIL via the seam's own
  `renderer::qualify` — provider injection + ipfs scheme, not graded).
- ✅ Records a comparable vs-wezig meter (`VsWezigMeter`): capability fraction +
  effort/code-volume/DOM-object-graph-friction signals on the shared ladder.
- ✅ Structured, comparable report (`report.json`) over the three candidates
  (own-engine / Servo / Blitz-Stylo-assembly) suitable for the exploration spec's
  decision.
- ✅ Re-runnable + reproducible: two runs are byte-equal (asserted); tests cover the
  scoring logic.

### The load-bearing boundary — HONOURED

The harness MEASURES; it does NOT decide. No `decide`/`choose`/`recommend`/`winner`
function exists (module docs + code at benchmark.rs:315 "does NOT rank or pick a
winner"). It lays the three candidates side by side on the same axes as EVIDENCE; the
architecture decision stays with the gated exploration spec
`rust-successor-native-renderer-architecture-benchmark` (needsAnswers: true, NOT in my
scope, untouched). This is exactly the task's "it does NOT itself pick the
architecture" requirement.

### Nit triage — both RATIFY (inputs the exploration spec refines)

1. vs-wezig arm figures (Rust 14 person-days/3400 LOC/friction 6 vs wezig 19/5200/4)
   are PINNED recorded evidence in `tests/fixtures/benchmark/vs-wezig.txt`, NOT
   computed from either source tree (wezig is a separate repo). The harness structures
   them; it does not fabricate a computation. Editable single fixture, refreshable as
   each arm settles. KEEP. Human-ratify: acceptable as initial meter inputs?
2. MEASURED-vs-DECLARED modelling: only the Blitz/Stylo-assembly class is MEASURED;
   own-engine + Servo are DECLARED honest empty slots (empty capability, both hooks
   missing, zero arm signals). More honest than inventing scores for un-built
   candidates; the task says score "a candidate" (singular) and this gives one real
   row + two comparable placeholders. KEEP.

Both connect to the exploration spec's evidence inputs (human-owned); neither is a
defect. Surfaced in the end-of-run batch as ratify items.

### What this unlocks

Leaf task. It is the EVIDENCE GENERATOR the gated exploration spec consumes; landing
it means the benchmark spec now HAS its harness (though the spec itself remains gated
on human answers + real benchmark evidence — out of this drive's scope).
