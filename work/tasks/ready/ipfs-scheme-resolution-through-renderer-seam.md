---
title: ipfs:// scheme resolution through the Renderer seam, rendered on webview
slug: ipfs-scheme-resolution-through-renderer-seam
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view, fetcher-hash-verified-content-addressed-path, eip1193-provider-injection-via-script-bridge]
covers: [6]
---

## What to build

Wire `ipfs://` URLs end-to-end: the `Renderer` seam's custom-scheme /
request-interception hook intercepts `ipfs://` requests, resolves them through the
`Fetcher` seam's hash-verified content-addressed path, and feeds the verified bytes
to the (webview) backend to render — so an `ipfs://` URL typed in the URL bar loads
and displays a content-addressed page at parity with a served page. First-class
scheme, not a novelty.

## Acceptance criteria

- [ ] Typing an `ipfs://<cid>...` URL navigates and renders the content-addressed page via the webview backend.
- [ ] The request is served through the seam's custom-scheme/interception hook, resolved by the hash-verified `Fetcher` path (a hash mismatch fails to load rather than rendering unverified bytes).
- [ ] The rendered result is at parity with the equivalent served page (same content renders the same).
- [ ] Tests cover the scheme→verified-fetch→render path with a pinned fixture CID, isolated from the live network.

## Blocked by

- Blocked by `browser-shell-url-bar-and-live-interactive-view` and `fetcher-hash-verified-content-addressed-path`.
- Blocked by `eip1193-provider-injection-via-script-bridge` (SERIALIZE: both wire hooks into the same webview `Renderer` backend surface — the script-message bridge and the request-interception hook — so they are ordered to avoid a parallel merge conflict, not because of a logical data dependency).

## Prompt

> Goal: make `ipfs://` a first-class scheme rendered through the `Renderer` seam's
> custom-scheme hook, backed by the verified content-addressed fetch (see
> `CONTEXT.md`, `docs/adr/0001`).
>
> Intercept `ipfs://` via the seam hook, resolve via the `Fetcher` hash-verified
> path (`fetcher-hash-verified-content-addressed-path`), render the verified bytes on
> the webview backend. A mismatch must NOT render — verification gates the load. This
> is the trust-hook that (with provider injection) qualifies the backend
> (`renderer-seam-trust-hook-qualification-gate`). Use a pinned fixture CID; keep
> tests off the live network.
>
> Done = an `ipfs://` URL loads and renders a verified content-addressed page at
> parity with a served page.
