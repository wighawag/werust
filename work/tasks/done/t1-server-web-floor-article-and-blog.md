---
title: T1 server-web floor — real article/doc page + independent blog post
slug: t1-server-web-floor-article-and-blog
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-core-css-stylo-and-latin-shaping-parley]
covers: [15]
---

> **FORWARD-POINTER (planted by drive-tasks after `t1-core-css-stylo-and-latin-shaping-parley` landed).** Two facts from the T1 core-CSS/shaping task shape your golden fixtures: (1) SHAPING is pinned to ONE bundled deterministic font (`assets/DejaVuSans.ttf`, freely redistributable), shaping against it only and SYNTHESISING bold/italic, so text advances are reproducible — your golden references will be stable ONLY against that bundled font; do not assume system fonts, and expect synthesised (not true-cut) bold/italic metrics. (2) SCOPE LINE (decision D4): `background-color` cascades and paints on text RUNS, but a BLOCK-CONTAINER background (e.g. `article { background: ... }`) does NOT yet paint a filled box (layout emits runs, not box rects) — this is deferred to THIS task or T2. So: do NOT author fixtures whose correctness depends on a filled block-background box (or, if you include one, assert the DOCUMENTED current behaviour and record the limitation), and lean on the T1 core-CSS scope that IS delivered (real cascade by specificity+source-order+inline, normal-flow block/inline layout, shaped Latin/LTR text, colour on runs) — NOT floats/flex/grid/tables (T2) or JS (T3). Consume the cascade via the same `ParsedDocument.author_css` + `Stylesheet::parse`/`cascade`/`ComputedStyle` API the core-CSS task built (in `native-renderer`); the WPT meter (`t1-wpt-subset-regression-meter`) is a sibling, not your job. NOTE: the T0-server-floor golden transcript does not assert COLOUR (a known gap) — if your T1 goldens should catch colour-cascade regressions, encode resolved colour in the transcript so they actually turn red.

## What to build

Establish the T1 server-web floor: pin two independently-authored real static pages
and assert the native T1 path renders them correctly. (1) A content-first
hand-authored article/doc page (an MDN-article / `motherfuckingwebsite.com`-class
minimal semantic-HTML page — headings, paragraphs, lists, links, inline emphasis, a
core-CSS stylesheet, properly shaped text). (2) A second, independently-authored
static blog/news post (e.g. a static-site-generator post), so the tier is not tuned
to one exemplar.

## Acceptance criteria

- [ ] Two pinned real static pages (an article/doc page and an independent blog post) render correctly via the native T1 path, with properly shaped text.
- [ ] Each is pinned to a specific snapshot/commit so the fixture is stable and reproducible.
- [ ] Rendering is asserted against golden references (structure/visual stability).
- [ ] Tests run under the `verify` gate and are isolated from the live network (use captured snapshots).

## Blocked by

- Blocked by `t1-core-css-stylo-and-latin-shaping-parley`.

## Prompt

> Goal: the T1 server-web floor — real hand-authored pages render via the native path
> (see `docs/conformance-tiers.md` T1). Two independent exemplars so the tier isn't
> tuned to one.
>
> Pin the pages to specific snapshots (reproducible fixtures), render via the native
> T1 path (`t1-whatwg-parser-html5ever-behind-tokenizer-seam` +
> `t1-core-css-stylo-and-latin-shaping-parley`), assert against goldens. The page
> checklist DRIVES the tier; the WPT bar (`t1-wpt-subset-regression-meter`) guards it.
> Pair with the content-addressed floor (`t1-content-addressed-floor-ipfs-static-site`)
> — T1 needs BOTH.
>
> Done = two real static server pages render correctly with shaped text via the native
> T1 path, against stable pinned goldens.
