---
title: Fix unitless line-height to inherit as a multiplier, not an absolute px
slug: fix-unitless-line-height-inherits-as-multiplier
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: [14]
---

## What to build

Fix a real correctness bug in the T1 core-CSS cascade (`crates/native-renderer/src/css.rs`):
a UNITLESS `line-height` (e.g. `body { line-height: 1.5 }`) is currently resolved to an
absolute px against the element's OWN font-size at cascade time and then inherited as
that fixed px. Per CSS, a unitless `line-height` must inherit as the MULTIPLIER, so each
descendant recomputes it against ITS OWN font-size. (A `line-height` given with a unit,
e.g. `24px`, or the keyword `normal`, keeps its current inheriting-as-a-resolved-value
behaviour — only the UNITLESS/number form changes.)

Observed bug (see `work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`):
with `body { font-size: 16px; line-height: 1.5 }`, an `<h1>` at `font-size: 28px` gets
`line_height = 24px` (1.5 × 16) instead of the correct `42px` (1.5 × 28). Every run in a
document with a unitless body line-height gets the SAME absolute line height regardless
of its font-size.

## What to change (guidance, not prescription)

- Today `parse` turns a unitless number into `Declaration::LineHeight(Some(Length::Em(number)))`
  and the cascade does `style.line_height = l.resolve(style.font_size)` (an f32 px),
  which both COLLAPSES the multiplier to px too early AND loses the "this was unitless"
  distinction on inheritance.
- Carry the unitless line-height through inheritance UNRESOLVED (as the multiplier), and
  resolve it to px against each element's own font-size at use. Options: a small
  `ComputedStyle.line_height` type (e.g. an enum `Absolute(px)` vs `Multiplier(f32)`, or
  a `Normal` variant), or store the multiplier alongside the px and re-resolve after
  font-size is known. Pick the shape that fits the existing `ComputedStyle` (currently
  `pub line_height: f32`) with the least churn, and update the layout consumer
  (`crates/native-renderer/src/layout.rs`, which reads `style.line_height` /
  `shaped.line_height`) to use the resolved px.
- Keep it scoped to line-height. Do NOT re-architect the cascade.

## Acceptance criteria

- [ ] A unitless `line-height` inherits as a multiplier: a child with a different `font-size` than the element that set `line-height` gets `multiplier × child_font_size`, not the parent's absolute px.
- [ ] A unit-bearing `line-height` (e.g. `24px`) and unset/`normal` line-height keep correct behaviour (a fixed value does not rescale per child; unset stays font-size-relative `normal`).
- [ ] A regression test with a parent and a child of DIFFERENT font-size proves the multiplier inherits correctly (the existing single-element test at css.rs did not catch the inherited-child case).
- [ ] The `t1-wpt-subset-regression-meter` core-CSS case that currently fails on this defect now PASSES (the meter's pass-rate rises accordingly); its threshold assertion stays green. Update the meter's expected count / `cases.txt` expectation if it pins the pre-fix number.
- [ ] All existing goldens stay green (T0 server floor, T1 server floor, T0/T1 content-addressed) — or, if a golden legitimately changes because a fixture uses a unitless line-height on differently-sized text, regenerate it with recorded rationale. (The current fixtures avoid unitless line-height, so goldens should be unaffected.)
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` all pass.

## Prompt

> Goal: make unitless `line-height` inherit as a MULTIPLIER (CSS-correct), fixing the
> orphaned cascade defect the WPT meter currently counts as a real failure. This is a
> small, surgical fix in `native-renderer`'s cascade + its one layout consumer, NOT a
> cascade rewrite.
>
> The bug: `css.rs` resolves a unitless `line-height` to absolute px against the setting
> element's own font-size and inherits that fixed px, so descendants with a different
> font-size get the wrong line height. Carry the unitless value UNRESOLVED through
> inheritance and resolve per element's own font-size at use. Unit-bearing and `normal`
> line-heights keep their current behaviour. Add a parent/child-different-font-size test.
> The `t1-wpt-subset-regression-meter` case for this defect should flip to PASS; keep all
> goldens green (fixtures currently avoid unitless line-height). See
> `work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`.
>
> Done = unitless line-height inherits as a multiplier, the WPT meter case passes, and
> the gate + all goldens are green.
