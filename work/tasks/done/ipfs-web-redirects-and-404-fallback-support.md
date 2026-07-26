---
title: "Support the IPFS _redirects file + custom 404.html fallback (IPIP-0002) so SPA rewrites and custom 404 pages work like a gateway (jolly-roger.eth/unknown)"
slug: ipfs-web-redirects-and-404-fallback-support
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

FIELD FINDING (v0.2.5, human): sites like `jolly-roger.eth` have a fallback page for unknown paths that IPFS gateways support (`https://<cid>.ipfs.dget.top/unknown`, `https://jolly-roger.eth.limo/unknown` both serve the custom 404 page), but werust does NOT - it returns a hard not-found error. We should support the same convention.

Root cause / confirmed shape: `crates/fetcher/src/retriever.rs::resolve_in_dag` returns `RetrieveError::PathNotFound` when a path does not resolve in the UnixFS DAG, and `crates/werust-core/src/ipfs.rs` surfaces that as a hard failure. IPFS gateways implement the IPIP-0002 web-pathing convention (spec: `https://specs.ipfs.tech/http-gateways/web-redirects-file/`, docs: `https://docs.ipfs.tech/how-to/websites-on-ipfs/redirects-and-custom-404s/`): when the requested path is NOT in the DAG, evaluate a `_redirects` file stored under the root CID. Confirmed with jolly-roger's build: its root `_redirects` is `/* /404.html/index.html 404` and it ships a `404.html/index.html` custom error page; ronan-eth has neither (so a not-found there stays a not-found - the fallback is opt-in per site).

Implement the `_redirects` fallback in the verified retrieval path, so a not-found path is resolved per the site's own rules, KEEPING full verification:

- On a `PathNotFound` for `ipfs://<rootcid>/<path>`, look for a `_redirects` file at the ROOT of the site (`<rootcid>/_redirects`). If absent, the not-found stays a not-found (today's behaviour, unchanged - the feature is opt-in per site).
- If present, parse it per the IPIP-0002 grammar (each line `from  to  [status]`, evaluated top-to-bottom, FIRST match wins, only for a path not in the DAG). Support at least the field-relevant subset and record what is/isn't supported:
  - `from to 200` (REWRITE / SPA + PWA: serve `to`'s content at the requested URL WITHOUT changing the bar - e.g. `/app/* /app/index.html 200`).
  - `from to 404` (custom 404: serve `to`'s content with a not-found status - e.g. `/* /404.html 404`, the jolly-roger case; note jolly-roger's `to` is `/404.html/index.html`, a directory-index path).
  - `from to 301|302` (redirect: NAVIGATE to `to`, updating the bar).
  - The `*` catch-all and the `:splat` / named `:placeholder` capture-and-inject in `to`.
  - Also honour the DEFAULT `404.html` fallback (a root `404.html` served on not-found) if that is simpler to land first / for sites with a `404.html` but no `_redirects`. Decide the scope + record it (start with the jolly-roger catch-all-404 case working end to end, then the SPA-200 rewrite; redirects/placeholders can be a follow-on if scoping tight - but record exactly what landed).
- Every fallback target is fetched through the SAME verified `ipfs://` retrieval (the `to` path is resolved into the DAG + hash-verified exactly like any resource); a `_redirects` pointing at a missing `to` is itself a not-found. NO verification bypass: the fallback content is content-addressed under the same root CID and verified.
- Surface the right status to the webview: a 200-rewrite serves the content as the requested resource; a 404 serves the custom page (the webview shows it); a 301/302 drives a navigation. Keep the bar/trust behaviour coherent (a same-origin rewrite/404 does not change the ENS name in the bar; a redirect updates it).

SECURITY / trust note (record it): the gateway spec limits `_redirects` evaluation to UNIQUE-ORIGIN content roots (subdomain/DNSLink), because a redirect/rewrite is a per-site capability that must not cross content roots. werust serves `ipfs://<cid>` as its own content root, so the `_redirects` of `<rootcid>` only ever governs paths UNDER `<rootcid>` - a `to` path is resolved under the SAME root CID, never another. Make that explicit (a `to` must stay within the root CID; reject/ignore an off-root `to`), so the feature cannot be used to make one site impersonate another. Verification is unchanged (all content under the root CID is hash-verified).

## Acceptance criteria

- [ ] A not-found path under a site with a root `_redirects` is resolved per its rules: the jolly-roger case (`/* /404.html/index.html 404`) serves the site's custom 404 page (content of `404.html/index.html`) with a not-found status, instead of werust's hard error - matching `jolly-roger.eth.limo/unknown`.
- [ ] A `200` rewrite rule (SPA/PWA catch-all, e.g. `/app/* /app/index.html 200`) serves the target content at the requested URL WITHOUT changing the bar.
- [ ] (If in scope this task) a `301`/`302` rule navigates to the target updating the bar; placeholders/`:splat` capture-and-inject work. Whatever subset lands is RECORDED (what is/isn't supported) durably.
- [ ] A site with NO `_redirects` (and no `404.html`) is unchanged: a not-found stays werust's honest not-found (the feature is opt-in per site).
- [ ] Verification/fail-closed intact: every fallback target is fetched + hash-verified through the SAME `ipfs://` retrieval; a `_redirects` whose `to` is missing is itself a not-found; the DAG size/block budgets are unchanged. NO verification bypass.
- [ ] The `to` target is constrained to the SAME root CID (a `_redirects` cannot point off-root / at another site); the unique-origin security rationale is recorded.
- [ ] Tests cover: the jolly-roger catch-all-404 case (a fixture site with `_redirects` + a `404.html` served content-addressed, asserting a not-found path serves the 404 page with the not-found status); a 200-rewrite; a no-`_redirects` site still hard-not-founds; a `to` pointing at a missing resource fails closed; an off-root `to` is rejected. Network-isolated.

## Blocked by

- None. (Builds on the verified retrieval path + the SvelteKit-over-ipfs fixture shape from `diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture`.)

## Prompt

> Goal: support the IPFS `_redirects` file (IPIP-0002) + custom `404.html` fallback so a not-found path is resolved per the SITE'S rules like a gateway does - `jolly-roger.eth/unknown` should serve its custom 404 page (its root `_redirects` is `/* /404.html/index.html 404`), not werust's hard error. Spec: `https://specs.ipfs.tech/http-gateways/web-redirects-file/`.
>
> Where to look: `crates/fetcher/src/retriever.rs` (`resolve_in_dag` returns `PathNotFound`; the directory-index resolution is the model), `crates/werust-core/src/ipfs.rs` (surfaces the failure). On a `PathNotFound`, look for `<rootcid>/_redirects`; if present, parse IPIP-0002 (`from to [status]`, first-match, only-when-path-absent) and handle 200 (rewrite/SPA, no bar change), 404 (custom 404 page + not-found status - the jolly-roger case), 301/302 (navigate), `*`/`:splat`/placeholders. Also honour a default root `404.html`. Start with the catch-all-404 case working end to end, then 200-rewrite; record exactly what subset landed.
>
> Every `to` is fetched + hash-verified through the SAME `ipfs://` retrieval; NO verification bypass; the `to` MUST stay within the root CID (record the unique-origin security rationale). A no-`_redirects` site is unchanged (opt-in). Done = the acceptance list, a network-isolated fixture (a site with `_redirects` + `404.html` served content-addressed) asserting the 404 fallback + a 200-rewrite + fail-closed on a missing `to` + off-root `to` rejected. FIRST re-check `resolve_in_dag` returns `PathNotFound` on a missing path.
