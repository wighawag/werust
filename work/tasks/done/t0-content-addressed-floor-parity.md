---
title: T0 content-addressed floor — same subset fragment over ipfs:// at parity
slug: t0-content-addressed-floor-parity
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [native-renderer-t0-subset-path-behind-seam, ipfs-scheme-resolution-through-renderer-seam]
covers: [12]
---

## What to build

Establish the T0 content-addressed floor: the SAME class of authored v0-subset
fragment, fetched over `ipfs://` (or the content-addressed resolution seam as an
authored fixture where networking is not yet involved), rendered by the native T0
path IDENTICALLY to the server-served version — proving the content-addressed path
renders v0 content at parity with the server path. T0 is only "reached" once both
this and the server floor land.

## Acceptance criteria

- [ ] A v0-subset fragment served via the content-addressed (`ipfs://`) path renders through the native T0 path.
- [ ] Its rendered output matches the server-floor rendering of the same fragment (parity — pixel/structure-stable against the shared golden).
- [ ] The content is hash-verified on the way in (reuses the verified `Fetcher` path); a mismatch does not render.
- [ ] Tests use a pinned fixture CID, isolated from the live network.

## Blocked by

- Blocked by `native-renderer-t0-subset-path-behind-seam` and `ipfs-scheme-resolution-through-renderer-seam`.

## Prompt

> Goal: the T0 content-addressed floor — v0 subset over `ipfs://` at parity with the
> server floor (see `docs/conformance-tiers.md`: a tier is not "reached" until BOTH
> floors land).
>
> Render the same subset fragment through the native T0 path when fetched via the
> hash-verified content-addressed path, and assert it matches the server-floor
> golden. Reuse `ipfs-scheme-resolution-through-renderer-seam` +
> `t0-server-web-floor-golden-fixtures`. Pin a fixture CID; keep tests off the live
> network.
>
> Done = the content-addressed path renders v0 content identically to the server path,
> completing T0.
