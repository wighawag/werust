---
title: "Gate-3 conductor review: export-the-chrome-css-class-set-from-core (APPROVE, after a Gate-2 block and an in-loop recovery)"
date: 2026-07-30
status: open
reviewOf: export-the-chrome-css-class-set-from-core
verdict: approve
---

## Verdict: APPROVE (second attempt)

Merged as `69d7e82` on `origin/main`. The FIRST dispatch was BLOCKED by Gate 2, recovered in-loop with `dorfl requeue` (keep + continue) plus a precise handoff, and the rebuild continued from the kept branch. Gate 2 then approved it with 5 non-blocking nits. Full gate re-run locally green.

## The block was correct, and it is the best argument for Gate 2 in this repo so far

The task's whole purpose was a test with TEETH: adding a fifth trust posture in core must RED the gate rather than silently leave painters with a stale class list. The first attempt shipped a test that drove postures from a hand-written array literal, so a fifth `TrustPosture` variant would not force that list to grow: an author adds the arm to `trust_indicator_css_class`, forgets the exported set, the suite stays green, and the painter then finds no matching member, removes all five classes and adds none, so the new posture paints UNSTYLED. The tooth would have been decorative, in a task that existed only to grow a tooth. The reviewer also caught that the code's own doc claimed "exhaustive by construction" when it was not.

The fix on the retry is better than what the task asked for. `TrustPosture::ALL`, `LoadState::ALL`, `LoadStep::ALL` and `FailureKind::ALL` now exist, each kept complete by a compile-time check: a total `match` in an anonymous `const` block that refuses to compile until a new variant is both NAMED and PRESENT in `ALL`. The exhaustiveness test then drives the CARTESIAN PRODUCT of every `ChromeState` axis, not just the axes today's rules read. The reviewer independently compiled a fifth variant to confirm the tooth bites (E0004, then a deny-by-default `unconditional_panic`), so this is measured, not asserted.

## Acceptance criteria, ticked against the merged tree

- [x] **The complete set is exported from core**, both families: `TRUST_INDICATOR_CSS_CLASSES`, `ERROR_BANNER_CSS_CLASSES`, plus `CHROME_CSS_CLASS_SETS` over both.
- [x] **The GTK painter iterates the exported set** rather than literals, in both toggle loops; behaviour unchanged (exactly one class active, none left stale).
- [x] **Exhaustiveness tooth, now real:** `every_chrome_css_class_the_derivation_can_return_is_in_the_exported_set` drives every enum axis through its compile-time-complete `ALL`.
- [x] **No-unstyled-class tooth:** `every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css` asserts each exported name has a `.class { … }` rule in `APP_CSS`, with an anti-vacuity check (`trust-not-a-posture` is not styled) so the assertion cannot pass emptily.
- [x] **Layering intact:** `APP_CSS` stays in the edge, core gained no notion of colour.
- [x] **The debug view's `trust-*` reuse still holds** (its Network tab paints per-request posture with the same classes, ADR-0006).
- [x] Gate green.

## Nit triage (5 non-blocking findings)

**Fixed by me (conductor): stale gate bookkeeping.** The runner set `needsAnswers: true` and wrote five stuck questions when Gate 2 bounced the task. The bounce was then recovered in-loop, so a DONE item was left advertising open questions, which can pull a human or the runner back to finished work. Flag cleared; the sidecar now records how it resolved rather than being deleted, so the bounce-and-recovery stays legible.

**For the human: ratify the new public API surface (D4).** Four new public consts, two of them on the deliberately dependency-free seam crate (`TrustPosture::ALL`, `LoadState::ALL`), plus a completeness check per enum. Chosen over `strum` or a macro. Reversible, but every future variant of those four enums must now visit the list, which is precisely the point.

**For the human: ratify the third test (D3).** A source-parsing wiring guard in `crates/werust-core/tests/` that reads `crates/werust/src/main.rs`, beyond the two tests the task named. Justified (the toggle needs a display the verify gate may lack) and it follows the existing `debug_view_desktop_wiring_shape.rs` precedent, but each future painter must extend or clone it.

**For the human: a residual hole, disclosed rather than hidden.** A fifth posture added with NO new class branch still falls through the `if`/`else` chains in `trust_indicator_css_class` / `trust_indicator` / `trust_indicator_detail` and paints `trust-unverified` silently. That is fail-closed and honest (an unproven posture reading as unverified is the safe direction), and it is captured as its own observation, but the Phase-2 name-verified work must remember the branches itself. Ratify leaving it, or task converting those chains to a `match`.

**Minor, unrecorded but benign:** the first paint in `open_window` now derives its classes from the derivation over `ChromeState::default()` instead of the literals; identical behaviour, since the default is Idle + UnverifiedOrigin + no error.
