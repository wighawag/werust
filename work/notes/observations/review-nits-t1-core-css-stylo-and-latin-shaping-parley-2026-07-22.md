---
title: review-gate non-blocking nits for 't1-core-css-stylo-and-latin-shaping-parley' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: t1-core-css-stylo-and-latin-shaping-parley
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 't1-core-css-stylo-and-latin-shaping-parley' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D1: the cascade is built on cssparser (stylo's own parser) with a focused hand-written selector matcher, NOT stylo's full Stylist/selectors::Element. This is an in-scope design choice affecting the sibling WPT-meter and server-floor tasks which consume Stylesheet::parse+cascade+ComputedStyle. Justified (werust Dom has no parent/interior-mutability; wiring Stylist reintroduces the object-graph friction the thesis parks at T1) and the seam stays a clean later swap. Coherent with CONTEXT.md 'stylo (cascade)' since cssparser is a stylo component.
  (docs/spikes/.../README.md D1; crates/native-renderer/src/css.rs)
- Ratify D2: shaping bundles one deterministic font (DejaVuSans.ttf) and shapes against it only, synthesising bold/italic. This is a user-visible default that pins reproducible advances; the sibling t1-server-web-floor golden task depends on it. Font is freely redistributable (LICENSE-DejaVu.txt present).
  (crates/native-renderer/src/shape.rs; assets/)
- Ratify D4: background-color cascades and paints on text RUNS but a block container background (e.g. article{background:...}) does NOT paint a filled box, since layout emits runs not box rects. Deferred to the T1 server-floor task or T2. Recorded as an observation note too. Confirm this scope line is acceptable for T1.
  (README.md D4; work/notes/observations/t1-block-box-background-not-painted-2026-07-22.md; REAL_DOC test sets article background but does not assert a box fill)
