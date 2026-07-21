---
title: Gate-3 (conductor) verdict — t1-whatwg-parser-html5ever-behind-tokenizer-seam — APPROVE
date: 2026-07-21
kind: observation
reviewOf: t1-whatwg-parser-html5ever-behind-tokenizer-seam
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 4ccd58b)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ html5ever bound behind the parser seam, produces a DOM for REAL documents
  (`html5ever_parser.rs`, `Html5everParser`).
- ✅ The native render path consumes the html5ever tree in place of the subset
  tree builder, behind the seam (`render_with(&dyn Parser)`; test
  `same_pipeline_renders_via_the_t1_parser_swap`).
- ✅ Structured so the T1 HTML-parsing WPT tree-construction subset can be run
  against it (threshold wired separately in `t1-wpt-subset-regression-meter`).
- ✅ Tests cover the parser-seam swap + a sample real-document parse.

### Key design deviation — SOUND, approved (worth human ratify)

The task said "behind the `Tokenizer | TreeBuilder` seam". html5ever was instead
bound at a whole-front-end `Parser` seam (source -> `ParsedDocument`), because a real
WHATWG parser FUSES tokenizing and tree construction (tree state feeds tokenizer
state for `<script>`/`<textarea>`/etc.) and cannot consume a pre-tokenized `Token`
stream. This is the CORRECT architecture, not a shortcut: the T0 pair
(`SubsetTokenizer`+`AllowlistTreeBuilder`) is retained composed behind `SubsetParser`;
the tiers/glossary docs already use "`Tokenizer | TreeBuilder` seam" as the UMBRELLA
and assign html5ever to parse, so `Parser` names the umbrella without re-meaning a
concept. Criteria 1+2 are met at the `Parser` boundary. APPROVED. Flagged for human
ratify as a load-bearing (but well-recorded) seam-shape choice.

### Other nit triage

- `ParsedDocument` carries `author_css` next to the `Dom` (each parser recovers it
  differently) — KEEP; parser-agnostic. ACTIONED as a FORWARD-NOTE on
  `t1-core-css-stylo-and-latin-shaping-parley` (it is the named owner of the CSS
  extraction/cascade; it should consume `ParsedDocument.author_css` and extend the
  existing `css.rs` cascade, not start a parallel one).
- `markup5ever_rcdom` 0.39 (unofficial/unsupported test DOM) as the intermediate —
  KEEP; scoped to `html5ever_parser.rs`, converted immediately to werust's owned
  `Dom`, never leaked past the seam. Flagged for human ratify of the added dep;
  folded into the stylo forward-note (do not reach for it).
- Doc staleness (`RenderOutput.dom` doc, backend "T0" framing) — benign prose nit.

### Forward-note planted (conductor step 2)

`t1-core-css-stylo-and-latin-shaping-parley`: consume cascade input from
`ParsedDocument.author_css` + owned `Dom`; extend/replace the existing small
`css.rs` T0 cascade with stylo (keep T0 server-floor goldens green or re-baseline
with rationale); optionally close the colour-not-in-goldens gap; don't touch
`markup5ever_rcdom`.

### What this unlocks

Landing this unlocks `t1-core-css-stylo-and-latin-shaping-parley` (the other half of
T1), which in turn unlocks the T1 page checklists + the WPT meter.
