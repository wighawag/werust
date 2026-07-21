---
title: T1 content-addressed floor — real ipfs:// static site at parity
slug: t1-content-addressed-floor-ipfs-static-site
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-core-css-stylo-and-latin-shaping-parley, ipfs-scheme-resolution-through-renderer-seam]
covers: [16]
---

## What to build

Establish the T1 content-addressed floor: a real content-addressed static site
fetched by CID (a Jekyll/Hugo-class static docs/landing site pinned to a specific
CID) rendered by the native T1 path at parity with the server path. This is where
the thesis lands FIRST — a verifiable, content-addressed static document opened as a
first-class page, not a novelty. T1 is only "reached" once this and the server floor
both land.

## Acceptance criteria

- [ ] A real `ipfs://` static site (pinned CID) renders correctly via the native T1 path.
- [ ] Its rendering is at parity with an equivalent served page (the content-addressed path is not a second-class renderer).
- [ ] The content is hash-verified on load (reuses the verified `Fetcher` path); a mismatch does not render.
- [ ] Tests pin the CID and are isolated from the live network.

## Blocked by

- Blocked by `t1-core-css-stylo-and-latin-shaping-parley` and `ipfs-scheme-resolution-through-renderer-seam`.

## Prompt

> Goal: the T1 content-addressed floor — a real `ipfs://` static site rendered by the
> native path at parity (see `docs/conformance-tiers.md` T1; the thesis lands FIRST
> here, `docs/adr/0001`).
>
> Pin a Jekyll/Hugo-class static site to a CID, fetch via the hash-verified path
> (`ipfs-scheme-resolution-through-renderer-seam`), render via the native T1 path,
> assert parity with the server rendering. A mismatch must not render. Pin the CID;
> keep tests off the live network. Completes T1 alongside
> `t1-server-web-floor-article-and-blog`.
>
> Done = a verifiable content-addressed static site renders as a first-class page at
> parity with the server path, completing T1's checklist.
