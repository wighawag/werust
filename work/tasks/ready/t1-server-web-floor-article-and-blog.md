---
title: T1 server-web floor — real article/doc page + independent blog post
slug: t1-server-web-floor-article-and-blog
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-core-css-stylo-and-latin-shaping-parley]
covers: [15]
---

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
