---
title: Hash-verified content-addressed fetch path in the Fetcher seam
slug: fetcher-hash-verified-content-addressed-path
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [fetcher-seam-bound-http-tls-stack]
covers: [9]
---

## What to build

Add a hash-verified content-addressed fetch path to the `Fetcher` seam: given a
content identifier (CID), fetch the content and VERIFY it against its hash before
returning it — verification moves to the hash, so the path is verifying, not
trusting. A hash mismatch is a hard failure, never a silent pass. This is the fetch
half of the `ipfs://` story (the scheme wiring through the renderer is a sibling
task).

## Acceptance criteria

- [ ] The `Fetcher` seam exposes a content-addressed fetch that takes a CID and returns content ONLY after hash verification succeeds.
- [ ] A hash mismatch (tampered/incorrect content) fails loudly — the content is rejected, never returned as if valid.
- [ ] The verification is exercised by tests with both a matching and a deliberately mismatching fixture.
- [ ] Tests isolate any fixture/content store to a temp location and mirror the repo's test style.

## Blocked by

- Blocked by `fetcher-seam-bound-http-tls-stack`.

## Prompt

> Goal: the verifying content-addressed fetch path — the technical core of the
> thesis (verifiable over server-authoritative; see `CONTEXT.md`, `docs/adr/0001`).
>
> Given a CID, fetch and verify against the hash before returning. A mismatch MUST
> fail loudly (the whole point is that we don't trust the origin — we verify the
> content). This is consumed by `ipfs-scheme-resolution-through-renderer-seam`, which
> wires `ipfs://` URLs to this path and renders the result. Test both the matching
> and mismatching cases; isolate any content store to a temp dir.
>
> Done = the `Fetcher` seam can fetch-and-verify content by CID, rejecting anything
> whose hash doesn't match.
