---
title: Fetcher seam binding a vetted HTTP+TLS stack (TLS never hand-written)
slug: fetcher-seam-bound-http-tls-stack
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [bootstrap-cargo-workspace-and-verify-gate]
covers: [8]
---

## What to build

Define the `Fetcher` seam and implement it over a vetted, bound HTTP+TLS stack
(rustls or bound libcurl). TLS is NEVER hand-written — the dangerous part is
delegated to a vetted implementation. This is the ordinary server-web fetch path
(the content-addressed verified path is a sibling task); it gives the browser a
seam-level fetch that returns response bytes + metadata for a URL over HTTP(S).

## Acceptance criteria

- [ ] A `Fetcher` trait exists; an implementation fetches over HTTP(S) via a bound HTTP+TLS stack (rustls or libcurl), with no hand-written TLS.
- [ ] Callers fetch only through the seam (no direct HTTP client calls leak past it).
- [ ] TLS errors / failures surface as seam errors rather than panicking.
- [ ] Tests cover the seam contract against a controlled local HTTP(S) endpoint (isolated, no real network dependency in the test).

## Blocked by

- Blocked by `bootstrap-cargo-workspace-and-verify-gate`.

## Prompt

> Goal: the `Fetcher` seam over a BOUND HTTP+TLS stack — never write TLS (see
> `CONTEXT.md`). Choose rustls or a bound libcurl and justify briefly; the durable
> TLS trust-store / pinning POLICY is deferred (it's an open question on the
> exploration spec `rust-successor-native-renderer-architecture-benchmark`) — do NOT
> finalize pinning here; just bind a working, safe default HTTP(S) fetch.
>
> This is the plain server fetch path; `fetcher-hash-verified-content-addressed-path`
> adds the verified content-addressed path on top. Test at the seam against a local
> controlled endpoint so tests don't depend on the live network.
>
> Done = the browser can fetch bytes for an HTTP(S) URL through the `Fetcher` seam
> on a vetted TLS stack, with failures surfaced as seam errors.
