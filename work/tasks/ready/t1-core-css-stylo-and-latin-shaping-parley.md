---
title: T1 — core CSS engine (stylo) + Latin/LTR shaping (parley) for static layout
slug: t1-core-css-stylo-and-latin-shaping-parley
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-whatwg-parser-html5ever-behind-tokenizer-seam]
covers: [14]
---

## What to build

Add a core CSS engine (stylo cascade) covering the common box-model, colour,
typography, and normal-flow properties a hand-written or lightly-templated static
page uses, plus real Latin/LTR text shaping (parley / cosmic-text), producing
correct static block/inline layout of REAL documents. No floats/flex/grid/tables
(that is T2); no JS (that is T3). Together with the html5ever parser this is the T1
capability.

## Acceptance criteria

- [ ] stylo provides a real cascade over the core CSS property set (box-model, colour, typography, normal-flow) on the html5ever-produced tree.
- [ ] parley/cosmic-text produces correctly shaped Latin/LTR text in the rendered output.
- [ ] Real static documents produce correct block/inline layout via the native path (no floats/flex/grid/tables, no JS).
- [ ] Tests cover the cascade + shaping on representative real fragments; the T1 core-CSS WPT areas can be run (threshold wired in `t1-wpt-subset-regression-meter`).

## Blocked by

- Blocked by `t1-whatwg-parser-html5ever-behind-tokenizer-seam`.

## Prompt

> Goal: core CSS + real shaping — stylo cascade + parley/cosmic-text — for correct T1
> static layout of real documents (see `docs/conformance-tiers.md` T1, `CONTEXT.md`).
>
> Scope is the core box-model/colour/typography/normal-flow set + Latin/LTR shaping —
> explicitly NOT floats/flex/grid/tables (T2) or complex-script/bidi (T2) or JS (T3).
> Consumes the html5ever tree from `t1-whatwg-parser-html5ever-behind-tokenizer-seam`.
> This is the other half of the pure-Rust-stack experiment; the T1 page checklist
> (`t1-server-web-floor-article-and-blog`, `t1-content-addressed-floor-ipfs-static-site`)
> is what proves it, the WPT bar (`t1-wpt-subset-regression-meter`) guards it.
>
> Done = real static documents lay out and shape correctly via the native path at the
> T1 core-CSS scope.
