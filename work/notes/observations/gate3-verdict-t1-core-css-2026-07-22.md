---
title: Gate-3 (conductor) verdict — t1-core-css-stylo-and-latin-shaping-parley — APPROVE
date: 2026-07-22
kind: observation
reviewOf: t1-core-css-stylo-and-latin-shaping-parley
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 5acc6b3)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Real cascade over the core CSS property set (box-model/colour/typography/
  normal-flow) on the html5ever tree: UA sheet -> author by (specificity, source
  order) -> inline `style`, with inheritance, built on `cssparser` (stylo's parser
  + colour parser).
- ✅ `parley` (fontique/harfrust/skrifa, pure-Rust) produces correctly shaped
  Latin/LTR text with measured advances.
- ✅ Real static documents produce correct block/inline normal-flow layout; no
  floats/flex/grid/tables (drift guard rejects `<table>` etc.), no JS.
- ✅ Tests cover cascade + shaping on real fragments; the T0 server-floor goldens
  are STILL asserted byte-equal and pass (not deleted/ignored).

### FORWARD-NOTE HONOURED (conductor value confirmed)

My forward-note (planted after the html5ever parser landed) was followed: the
cascade consumes `ParsedDocument.author_css` (`Stylesheet::parse(&parsed.author_css)`
in layout.rs + paint.rs), the existing small `css.rs` T0 cascade was EXTENDED (not
forked) into the real stylo-stack cascade, the T0 goldens stayed green, and
`markup5ever_rcdom` was not reached for.

### Nit triage

1. D1: cascade on `cssparser` + a FOCUSED hand-written selector matcher, not stylo's
   full `Stylist`/`selectors::Element` — RATIFY. Justified (werust `Dom` has no
   parent/interior-mutability; full `Stylist` reintroduces object-graph friction the
   thesis parks at T1); `cssparser` is a stylo component so "stylo (cascade)" holds
   in spirit; seam stays a clean later swap. KEEP.
2. D2: shaping bundles ONE deterministic font (`assets/DejaVuSans.ttf`), synthesising
   bold/italic — RATIFY. Pins reproducible advances; freely redistributable. The
   sibling t1-server-floor goldens DEPEND on this determinism -> FORWARD-NOTE planted.
3. D4: `background-color` paints on text RUNS but a BLOCK-CONTAINER background does
   NOT paint a filled box (layout emits runs, not box rects) — genuine scope line.
   APPROVED for T1: the cascade+shaping+normal-flow core is delivered and tested;
   filled block-background boxes are a bounded, honestly-recorded deferral (to the
   server-floor task or T2). Colour-on-runs (the dominant case) works. -> FORWARD-NOTE
   planted on t1-server-floor so its fixtures don't rely on filled block backgrounds.

### Forward-note planted (conductor step 2)

`t1-server-web-floor-article-and-blog`: (a) goldens are stable ONLY against the
bundled DejaVuSans font with synthesised bold/italic; (b) block-container backgrounds
do not yet paint filled boxes (D4) — don't author fixtures depending on that, or
assert the documented behaviour; (c) consume the same `author_css`/`Stylesheet`
cascade API; (d) optionally encode colour in the transcript to catch colour-cascade
regressions.

### What this unlocks

This is the other half of T1. Landing it unlocks: `t1-server-web-floor-article-and-blog`,
`t1-wpt-subset-regression-meter`, `native-renderer-benchmark-harness-capability-and-trust-hooks`
(its other dep, the trust-hook gate, is already done), and (with the ipfs path)
`t1-content-addressed-floor-ipfs-static-site`.
