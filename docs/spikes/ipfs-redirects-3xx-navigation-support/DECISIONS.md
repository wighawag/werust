# Decisions: `_redirects` 3xx as a real navigation (`ipfs-redirects-3xx-navigation-support`)

Durable record of the design choices this task made. Linked from the task done-record. Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`. Follow-on to `ipfs-web-redirects-and-404-fallback-support` (its `docs/spikes/ipfs-web-redirects-and-404-fallback-support/DECISIONS.md`, Decision 3, deferred exactly this). Spec being implemented: <https://specs.ipfs.tech/http-gateways/web-redirects-file/> (IPIP-0002).

## Reality re-check (before building)

The prompt's FIRST re-check held, verbatim:

- `crates/werust-core/src/redirects.rs` DID parse `301`/`302`/`303`/`307`/`308` into a `RedirectRule` (they are in `ALLOWED_STATUSES`, and 301 is the omitted-status default), and `resolve_target()` returned `Err(RedirectsError::RedirectNotSupported { status, to })` for any status outside `{200, 404, 410, 451}`.
- `crates/werust-core/src/ipfs.rs::resolve_not_found_fallback` mapped that error onto a `RendererError::Backend`, so a matching 3xx FAILED the load rather than navigating.
- The parent behaviours (200 rewrite, custom 404, default root `404.html`, off-root refusal, opt-in cost) were all present and are unchanged by this task.

No drift; the build proceeded on the conductor's decided mechanism.

## What landed

| IPIP-0002 feature | Status |
| --- | --- |
| `301` / `302` / `303` / `307` / `308` rules NAVIGATE to the rule's `to` (bar + history move) | **landed** |
| Placeholder / `:splat` injection into a 3xx `to` | **landed** (the same `resolve_target` path 200/404 use) |
| Same-root-CID confinement of a 3xx `to` (off-root refused, incl. an escape smuggled through a capture) | **landed** (unchanged code, now also covering the redirect branch) |
| Hash verification of the redirect target | **landed** (the target is fetched by the navigation it triggers, through the ordinary `ipfs://` path) |
| Bounded redirect chain (cycle / over-long fails closed) | **landed** (`MAX_REDIRECT_HOPS = 5`, per chain) |
| ENS site identity across a redirect | **landed** (no new mechanism: the existing root-CID-prefix `ens_pages` association already covers it, because the target is confined to the same root CID) |
| §3.5 query-parameter merging into a `Location` header | **NOT landed** — werust has no `Location` header; a `to`'s query is dropped, as it is for a served target (a query is not part of a content-addressed DAG path) |
| Permanent (301/308) vs temporary (302/303/307) semantics | **deliberately not differentiated** — see Decision 4 |

## Decision 1 — a 3xx travels on a shared `RedirectSink`, NOT on `SchemeResponse`

**Chosen:** `redirects.rs` gained `FallbackAction::Redirect { path, status }` beside `Serve`; `ipfs.rs::resolve_ipfs_request` takes a `&RedirectSink` and, on a matched 3xx, pushes the absolute `ipfs://<rootcid><to>` into it and answers the intercepted request with a fail-closed error (nothing renders under the OLD url). `BrowserShell::pump` drains the sink on its EXISTING cadence and calls the seam's `Renderer::navigate`.

**Why:** `renderer::SchemeResponse::status` already documents itself as "NOT a redirect channel: a 3xx would be a NAVIGATION ... which belongs to the navigation path". Respecting that is the coherence point: a scheme handler ANSWERS a request, it does not move the browser. The sink is the codebase's existing idiom for exactly this shape (a `Send` handler needs to hand something to a `!Send` shell) — the twin of `werust-android`'s `pending_eval: Arc<Mutex<Vec<String>>>` and `pending_load`. Draining on the shell's existing pump means no new loop and no busy poll.

**Alternatives considered:** (a) add a `Location`/3xx status to `SchemeResponse` — rejected: it re-means a field the seam explicitly scoped, and on Android it is not even expressible (`WebResourceResponse` refuses a 300-399 status code outright); (b) have the resolver fetch the target itself and serve it in place — rejected: that is a 200 rewrite wearing a redirect's clothes, leaving the bar showing a URL whose content the site does not serve there (the parent task rejected the same thing for the same reason).

**Touches:** `renderer::SchemeResponse` (unchanged, deliberately), `BrowserShell::with_redirect_sink` + `pump`, and each edge's `install_ipfs` (which now RETURNS the sink). Ignoring the returned sink is safe: a matched 3xx then fails closed without navigating, i.e. the pre-task behaviour.

## Decision 2 — the chain bound lives in the sink, is per-CHAIN, and counts VISITED targets

**Chosen:** `MAX_REDIRECT_HOPS = 5`. The sink refuses a hop that revisits an already-redirected-to target in the current chain (a cycle) or that exceeds the cap, queueing nothing and leaving a legible fail-closed error. `RedirectSink::reset()` starts a fresh chain and is called by every USER-initiated navigation (`navigate` / `go_back` / `go_forward` / `reload`).

**Why:** each hop is a separate navigation that re-enters the scheme handler, so there is no recursion depth to bound — the state has to be the sink's. Counting VISITED targets (not just hops) catches the common `/a -> /b -> /a` ping-pong on hop 3 instead of hop 6. Resetting on user intent is what keeps the bound about ONE site's chain rather than progressively starving a session: without it, a site that legitimately redirects on every visit would eventually stop working.

**Why 5 and not 20 (a USER-VISIBLE default, so it is recorded):** browsers cap around 20, but each werust hop is a full content-addressed retrieval (a CAR fetch + per-block verify), so a long chain is expensive rather than merely slow, and a `_redirects` file that needs more than a handful of hops is indistinguishable from one that loops. Reversible in one constant if a real site is found that needs more.

**Alternatives considered:** (a) a hop counter only, no visited set — rejected: a 2-cycle would burn the whole budget before failing, and the error could not say "cycle"; (b) resetting on every navigation including the redirect's own — rejected: that IS the unbounded loop.

**Touches:** `BrowserShell`'s four user-navigation methods (each now resets). Any future user-initiated navigation entry point should reset too.

## Decision 3 — the ENS identity across a redirect needs NO new mechanism (verified, not assumed)

**Chosen:** nothing was built for this. The 3xx target is confined to the SAME root CID (the unique-origin rule, unchanged), so the shell's existing root-CID-prefix `ens_pages` association recognises the post-redirect URL as the same site and shows `name/<new-in-site-path>`.

**Why it is recorded anyway:** the task asked to "compose with the root-CID-prefix ens_pages association"; the honest answer is that the composition is already free BECAUSE of the same-root confinement, and that link is worth making explicit — if the confinement were ever relaxed, the identity display would silently break with it. Pinned by `a_redirect_inside_an_ens_site_keeps_the_eth_identity_in_the_bar` (`crates/werust-core/src/lib.rs`).

## Decision 4 — permanent vs temporary is NOT differentiated (surfaced honestly instead)

**Chosen:** all five 3xx codes perform the same navigation. The status is carried through the action and named in the reason, but nothing behaves differently.

**Why:** the only real difference between 301/308 and 302/303/307 in a browser is CACHING the redirect (and, for 307/308, method preservation). werust caches no redirects and issues no methods on the `ipfs://` path, so there is nothing to honour; behaving "differently" would mean inventing a cache this task did not build. Surfacing the code honestly (it appears in the legible reason) without acting on it is the "only insofar as werust surfaces them honestly" the task asked for.

**Touches:** a future redirect-cache would be a new decision, not a bug fix here.

## Decision 5 — the redirected-FROM request's own failure is SUPPRESSED, a refusal is NOT

**Chosen:** the "navigating" error carries `werust_core::ipfs::REDIRECT_NAVIGATING_MARKER` in its reason, and `BrowserShell::pump` clears `last_error` for a `Failed` event carrying it. A REFUSED redirect (off-root, cycle, over-long chain) carries no marker and surfaces normally.

**Why (a USER-VISIBLE choice, so it is recorded):** answering the intercepted request fail-closed is correct — nothing may render under the redirected-FROM url — but the backend reports that as a failed load, which would flash the prominent red error banner for a fraction of a second on every redirect before the target loads. That is a lie about a load that is working. Suppressing it is scoped as tightly as possible: only the exact reason the resolver itself produced when it ACCEPTED a hop, never a refusal and never any other failure.

**Alternatives considered:** (a) suppress every `ipfs://` failure while a redirect is pending — rejected: too broad, it would swallow a genuine failure of the redirected load; (b) don't fail the intercepted request at all (serve an empty 200) — rejected: that renders a blank page under the old URL, which is exactly the identity lie the whole design avoids.

**Touches:** `BrowserShell::pump`'s `Failed` arm. Any future consumer of `last_error` inherits the suppression, which is intended: the redirect is not a failure.

## Decision 6 — `RedirectsError::RedirectNotSupported` was REMOVED, not kept

**Chosen:** the variant and its `Display` arm are gone; the parent task's test asserting it was rewritten to assert the new `Redirect` action.

**Why:** it named a capability gap that no longer exists. Keeping a dead "not supported" variant around invites a later caller to reintroduce the refusal, and leaves the module docs asserting something false. The unique-origin refusal (`OffRootTarget`) is untouched and still covers the 3xx path.

**Touches:** nothing outside `werust-core` matched on the variant (the edges only see the `RendererError` string).

## Tests (all network-isolated)

- `crates/werust-core/src/redirects.rs` — a matching 3xx yields `Redirect` (all five codes + the omitted-status 301 default); splat/placeholder injection; off-root 3xx refused (literal and via a capture); a 3xx `to`'s query dropped.
- `crates/werust-core/src/ipfs.rs` — the seam glue over a pinned retriever double: the queued target is absolute + same-root; the target is NOT pre-fetched; every 3xx code navigates; an off-root target is never queued; a cycle is bounded and fails closed; a reset restores the budget; a dead-end target fails closed on the next hop.
- `crates/werust-core/tests/ipfs_redirects_fixture.rs` — over a REAL synthesized multi-block UnixFS/dag-pb DAG through the production `TrustlessGatewayCarRetriever`: a 3xx navigates and FOLLOWING the target resolves it through the same per-block-verified retrieval; splat injection + off-root refusal; a cycle bounded.
- `crates/werust-core/src/lib.rs` — the shell: a queued redirect is performed on the pump (bar + history move, drained once); an ENS site keeps its `.eth` identity + posture across a redirect; a user navigation resets the chain budget; the redirected-FROM request's own failure is suppressed while a refusal still surfaces.
- `crates/werust-core/tests/redirect_navigation_edge_shape.rs` — the per-edge LAST step the pure-Rust gate cannot otherwise see: both mobile edges drain the pending load on the load signals a redirect lands on, Android never maps a 3xx onto a `WebResourceResponse` status (it would throw), and desktop keeps + hands over the sink `install_ipfs` returns. Mutation-checked (reverting one Kotlin handler to a bare `refreshChrome` reds it), with the brace-matching extractor pinned so the guard cannot go vacuous.
- `crates/werust-core/tests/platform_capability_parity.rs` — the new `ipfs-redirects-3xx-navigation` matrix row, implemented on all three contexts.
