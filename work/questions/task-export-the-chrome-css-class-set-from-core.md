<!-- dorfl-sidecar: item=task:export-the-chrome-css-class-set-from-core type=task slug=export-the-chrome-css-class-set-from-core allAnswered=false -->

## Q1

**'task:export-the-chrome-css-class-set-from-core' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The exhaustiveness tooth does not bite on the scenario the task names. The core test drives postures from a hand-written array literal (for posture in [UnverifiedOrigin, ContentVerified, NameViaTrustedRpc, MutableName]), so adding a FIFTH TrustPosture variant does not force that list to grow: the author fixes the compile errors in trust_indicator_css_class (new arm returning e.g. trust-name-verified), forgets TRUST_INDICATOR_CSS_CLASSES, and the suite stays GREEN. The painter then finds no set member equal to the active class, removes all five and adds none, so the new posture paints with NO trust class at all (unstyled badge) with a green gate. That is acceptance criterion 3 (adding a posture in core without extending the set fails the gate) plus the second failure mode tooth 2 exists for. It is also a false claim in the code: the doc on every_chrome_state_shape says Exhaustive by construction ... a rule that starts branching on an axis it does not read TODAY (the next posture, ...) is still driven, which is not true of a new enum variant. A Phase-2 name-verified posture is explicitly planned in the TrustPosture docs, so this is a likely path. Cheap fix: derive the posture list exhaustively-by-construction (a match over TrustPosture inside the helper, so a new variant is a COMPILE error), same for the DECISIONS.md mutation-check claim, which only mutated an existing branch and so never exercised this path. (crates/werust-core/src/lib.rs, every_chrome_state_shape + every_chrome_css_class_the_derivation_can_return_is_in_the_exported_set; crates/renderer/src/lib.rs:77 enum TrustPosture has no ALL/iteration helper and is not non_exhaustive. Partial mitigation only: the set==produced assertion reds if the author extends the SET but not the drive list; it stays green when neither is touched.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):
