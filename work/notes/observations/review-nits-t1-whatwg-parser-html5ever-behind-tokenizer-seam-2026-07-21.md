---
title: review-gate non-blocking nits for 't1-whatwg-parser-html5ever-behind-tokenizer-seam' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: t1-whatwg-parser-html5ever-behind-tokenizer-seam
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't1-whatwg-parser-html5ever-behind-tokenizer-seam' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: html5ever was bound at a whole-front-end `Parser` seam (source -> ParsedDocument) rather than the literal two-trait Tokenizer+TreeBuilder split, because a real WHATWG parser fuses tokenizing and tree construction and cannot consume a pre-tokenized Token stream. The T0 pair is retained composed behind SubsetParser. This changed render_with's signature to `&dyn Parser` and every caller. Recorded and coherent (the glossary/tiers doc already call this the `Tokenizer | TreeBuilder` seam and assign html5ever to parse); it names the umbrella, does not re-mean a concept. Human to ratify.
  (parser.rs seam rationale; docs/spikes/.../README.md Decisions; docs/conformance-tiers.md:77)
- Ratify: ParsedDocument carries author_css next to the Dom, with each parser recovering it differently (T0 from the token stream since the allowlist drops <style>; T1 by walking the <style> html5ever keeps in the tree). Keeps the pipeline parser-agnostic. Recorded, sensible, and the sibling stylo task is named as the future owner of this extraction. Human to ratify.
  (parser.rs author_css_from_dom / author_css_from_tokens; README Decisions #2)
- Ratify: markup5ever_rcdom (0.39, published as 0.39.x+unofficial, an unsupported test DOM) is used as the intermediate, converted to werust's owned Dom and never leaked past the Parser seam, so it is swappable later. Recorded; scoped to html5ever_parser.rs only. Human to ratify the added dep.
  (crates/native-renderer/Cargo.toml +html5ever/markup5ever_rcdom 0.39; html5ever_parser.rs convert_node; README Decisions #3)
- Minor doc staleness: RenderOutput.dom field doc still says 'The allowlist DOM the tree builder produced', which is only true on the T0 path; under T1 it is the html5ever-produced Dom. Also backend.rs module/struct docs still broadly narrate 'T0' though the parse stage is now html5ever (the backend legitimately stays T0-tier for network/CSS/shaping, so this is accurate for those aspects but the blanket framing reads stale). Non-blocking prose nit.
  (crates/native-renderer/src/pipeline.rs RenderOutput.dom doc; backend.rs:1,52)
