---
title: review-gate non-blocking nits for 'export-the-chrome-css-class-set-from-core' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: export-the-chrome-css-class-set-from-core
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'export-the-chrome-css-class-set-from-core' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D4: four new PUBLIC consts (TrustPosture::ALL, LoadState::ALL in the deliberately dependency-free seam crate; LoadStep::ALL, FailureKind::ALL in core) plus an anonymous const completeness check per enum. This is new permanent API surface on the seam crate, chosen over strum/macro alternatives. Reversible, but every future variant of those four enums must now visit the list.
  (crates/renderer/src/lib.rs:54,194 and crates/werust-core/src/lib.rs:470 plus the _..._IN_SLOT_ORDER const blocks; recorded in docs/spikes/export-the-chrome-css-class-set-from-core/DECISIONS.md D4. I independently compiled the two-layer construction with rustc: adding a 5th variant gives E0004, and the ALL[4] arm then gives error: this operation will panic at runtime (deny-by-default unconditional_panic). The tooth is real, not asserted.)
- Ratify D3: a THIRD test beyond the two the task names, a source-parsing wiring guard living in crates/werust-core/tests/ but reading crates/werust/src/main.rs. Justified (the toggle needs a display the verify gate may lack) and it follows the existing debug_view_desktop_wiring_shape.rs / browser_menu_edge_wiring_shape.rs precedent, but it means each future painter must extend or clone it.
  (crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs; DECISIONS.md D3)
- Residual hole, disclosed rather than closed: a 5th posture added with NO new class branch still falls through the if/else chains in trust_indicator_css_class / trust_indicator / trust_indicator_detail and paints trust-unverified, silently. Fail-closed and honest, and captured as an observation, but the Phase-2 name-verified task must remember the branches itself. Ratify leaving it, or task the chains-to-match conversion.
  (work/notes/observations/new-trust-posture-falls-back-to-unverified-badge-2026-07-30.md; DECISIONS.md D4 what it does NOT catch)
- Stale gate bookkeeping: the finished task still carries needsAnswers: true in its frontmatter, and work/questions/task-export-the-chrome-css-class-set-from-core.md still holds five identical unanswered stuck questions from the earlier bounce. Contract says gate axes are honest; a done item advertising open questions can pull a human or the runner back to it.
  (work/tasks/done/export-the-chrome-css-class-set-from-core.md frontmatter; work/questions/task-export-the-chrome-css-class-set-from-core.md Q1-Q5. Likely runner-owned cleanup, not the author's diff.)
- Un-recorded in-scope decision: the first paint in open_window now derives its classes from the derivation (trust_indicator_css_class / error_banner_css_class over ChromeState::default()) instead of the literals trust-unverified / error-banner. Behaviour is identical (default is Idle plus UnverifiedOrigin plus no error), so this is a ratify-or-note, not a defect.
  (crates/werust/src/main.rs:1246,1262; not mentioned in DECISIONS.md)
