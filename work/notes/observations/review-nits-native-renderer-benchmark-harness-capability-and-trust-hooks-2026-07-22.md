---
title: review-gate non-blocking nits for 'native-renderer-benchmark-harness-capability-and-trust-hooks' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: native-renderer-benchmark-harness-capability-and-trust-hooks
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'native-renderer-benchmark-harness-capability-and-trust-hooks' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D2: the vs-wezig arm figures (Rust 14 person-days / 3400 LOC / friction 6 vs wezig 19 / 5200 / 4) are PINNED recorded evidence in tests/fixtures/benchmark/vs-wezig.txt, not derived from either source tree (wezig is a separate repo). The harness structures them; it does not compute them. Confirm these placeholder-ish figures are acceptable as the meter's inputs until refreshed as each arm settles.
  (tests/fixtures/benchmark/vs-wezig.txt + README D2; disclosed as recorded evidence, single fixture source, editable without code change.)
- Ratify the MEASURED-vs-DECLARED modelling: only the Blitz/Stylo-assembly class is MEASURED; own-engine and Servo are DECLARED honest slots (empty capability, both hooks missing, zero arm signals). The task says score 'a candidate' (singular); this delivers one real row plus two comparable placeholders so the exploration spec compares three on the same axes without fabricated zeros.
  (benchmark.rs declared_candidate/CandidateScoring + README D2 + tests output_is_a_structured_comparable_report.)
