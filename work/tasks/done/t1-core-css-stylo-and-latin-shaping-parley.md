---
title: T1 — core CSS engine (stylo) + Latin/LTR shaping (parley) for static layout
slug: t1-core-css-stylo-and-latin-shaping-parley
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-whatwg-parser-html5ever-behind-tokenizer-seam]
covers: [14]
---

> **FORWARD-POINTER (planted by drive-tasks after `t1-whatwg-parser-html5ever-behind-tokenizer-seam` landed).** The T1 parser task landed a `Parser` seam producing a `ParsedDocument { dom, author_css }` (in `native-renderer/src/parser.rs`): a real WHATWG parse cannot expose a clean Tokenizer->TreeBuilder pipe, so the seam sits at the whole-front-end `Parser` boundary, and BOTH parsers (T0 `SubsetParser`, T1 `Html5everParser`) recover author CSS onto `ParsedDocument.author_css` (T1 by walking the `<style>` html5ever keeps in the tree). This task was NAMED as the future owner of that CSS extraction/cascade — so: consume the cascade INPUT from `ParsedDocument.author_css` + the `Dom` (do not re-tokenize or re-extract `<style>` yourself). There is ALSO already a small hand-rolled cascade in `native-renderer/src/css.rs` over the T0 property allowlist (`SUPPORTED_PROPERTIES`, `is_supported_property/selector`) with a subset-drift golden guard (`tests/t0_server_floor_goldens.rs`): EXTEND/replace that cascade with the real stylo cascade over the core CSS set rather than starting a parallel one, and keep the T0 server-floor goldens green (or consciously re-baseline them with recorded rationale if stylo legitimately changes T0 output). NOTE (from the T0-server-floor Gate-3): the golden transcript does NOT yet assert COLOUR and `paint::transcribe()` overclaims that it does — if you touch paint/cascade for colour, consider closing that gap so colour-cascade regressions actually turn a golden red. Also: `markup5ever_rcdom` is an unofficial/unsupported test DOM used ONLY inside `html5ever_parser.rs` and converted immediately to werust's owned `Dom` — do not reach for it; work against the owned `Dom`.

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
