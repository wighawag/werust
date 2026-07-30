---
title: review-gate non-blocking nits for 'one-derivation-close-the-aggregate-and-tooltip-gaps' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: one-derivation-close-the-aggregate-and-tooltip-gaps
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'one-derivation-close-the-aggregate-and-tooltip-gaps' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D1: the family aggregate landed as a PUBLIC enum (CssClassFamily, its ALL array and a const classes()) in werust-core, not a third slice const. It is now the surface every future painter binds to (the queued windows-webview2-backend-and-window and mobile task), and its arity changes whenever a family is added. Keep this shape?
  (crates/werust-core/src/lib.rs:1089-1155; rationale + rejected flat-const alternative recorded in docs/spikes/one-derivation-close-the-aggregate-and-tooltip-gaps/DECISIONS.md D1)
- Ratify D2: the stop-affordance label is a parameter plus a shared default const STOP_AFFORDANCE_LABEL living in CORE. Core now carries a UI affordance glyph, which sits slightly across the core-derives / edge-paints line the crate docs draw (core has no notion of colour). Also the GTK Stop is a themed process-stop-symbolic icon, not a literal glyph, so the const doc-comment claim that every edge reads as this glyph is generous for GTK. Behaviour is unchanged (both edges already said the same sentence). Keep the const in core?
  (crates/werust-core/src/lib.rs:769-812; crates/werust/src/main.rs:1091 uses an icon-name button; DECISIONS D2 records the per-edge label survey)
- Un-recorded in-scope decision: the edge-wiring shape test dropped its assertion that the desktop shell consumes CHROME_CSS_CLASS_SETS, and as landed that pub const now has no consumer outside core's own tests (the toggle loops use the individual family consts). Its narrower toggling meaning survives only as documentation. Ratify keeping it exported, or should it be retired or re-asserted somewhere?
  (crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs:73 (list shrunk to two names); grep shows CHROME_CSS_CLASS_SETS only in core lib.rs, its own unit test, and doc comments)
- Residual hole in the new tooth: exhaustiveness binds ENUM VARIANTS, so adding a variant cannot compile without joining ALL and classes(). Nothing forces a NEW exported const (say FOO_CSS_CLASSES) to become a variant at all, so an edge could consume it directly and still join neither gate. Worth a core test asserting every *_CSS_CLASSES const in the crate source appears in classes()?
  (crates/werust-core/src/lib.rs:1127-1155 const check; today the enrolment is complete (only 3 such consts exist: lib.rs:887, 995, debug.rs:1142))
- The build captured an observation that verify runs bare cargo clippy, so lint debt in cfg(test) code never reds the gate (it names a pre-existing copied lint and nine field_reassign_with_default in macOS paint tests). Should verify say --all-targets? That is a gate-policy call for the human, deliberately not made here.
  (work/notes/observations/verify-clippy-does-not-lint-test-targets-2026-07-30.md; dorfl.json verify)
