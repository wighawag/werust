---
title: Trust indicator — content-verified vs unverified-origin loads
slug: trust-indicator-verified-vs-served
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [ipfs-scheme-resolution-through-renderer-seam]
covers: [7]
---

## What to build

Add a visible chrome indicator that distinguishes a content-verified
(content-addressed / hash-verified) load from an unverified served-origin load, so
the trust posture is a product surface, not a silent internal. When a page was
loaded via the hash-verified content-addressed path, the chrome shows it was
verified; when loaded from an ordinary served origin, it shows the unverified state.

## Acceptance criteria

- [ ] The chrome shows a clear indicator state for a content-verified load (via the `ipfs://`/content-addressed path) vs an unverified served-origin load.
- [ ] The indicator is driven by the actual load path (verified fetch vs plain fetch), not guessed from the URL scheme alone.
- [ ] The two states are visually distinct and legible.
- [ ] Tests assert the indicator reflects the verified vs unverified load path.

## Blocked by

- Blocked by `ipfs-scheme-resolution-through-renderer-seam`.

## Prompt

> Goal: surface the trust posture in the chrome — a "this was content-verified" vs
> "served by an unverified origin" indicator (see `docs/adr/0001`: the trust posture
> is a product surface, not a silent internal).
>
> Drive the indicator from the REAL load path (did it come through the hash-verified
> content-addressed `Fetcher` path, or a plain served fetch?), not from the URL
> string. Depends on the `ipfs://` path existing
> (`ipfs-scheme-resolution-through-renderer-seam`). Test that the indicator tracks
> the actual verification path.
>
> Done = the user can see, in the chrome, whether the current page was content-verified
> or merely served.
