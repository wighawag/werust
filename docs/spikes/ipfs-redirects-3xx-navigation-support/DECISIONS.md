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
| Bounded redirect chain (cycle / over-long fails closed) | **landed** (`MAX_REDIRECT_HOPS = 5`, per chain — including an in-page link click, see Decision 2) |
| History semantics (Back after a redirect) | **landed as an EMULATION**: werust PUSHES rather than replaces (no replace-current-entry exists), so Back SKIPS the redirecting entry — see Decision 8 |
| Only the MAIN FRAME redirects (a sub-resource never navigates the page away) | **landed** (inferred from the top-level URL; the seam has no is-main-frame flag — see Decision 7) |
| ENS site identity across a redirect | **landed** (no new mechanism: the existing root-CID-prefix `ens_pages` association already covers it, because the target is confined to the same root CID) |
| §3.5 query-parameter merging into a `Location` header | **NOT landed** — werust has no `Location` header; a `to`'s query is dropped, as it is for a served target (a query is not part of a content-addressed DAG path) |
| Permanent (301/308) vs temporary (302/303/307) semantics | **deliberately not differentiated** — see Decision 4 |

## Decision 1 — a 3xx travels on a shared `RedirectSink`, NOT on `SchemeResponse`

**Chosen:** `redirects.rs` gained `FallbackAction::Redirect { path, status }` beside `Serve`; `ipfs.rs::resolve_ipfs_request` takes a `&RedirectSink` and, on a matched 3xx, pushes the absolute `ipfs://<rootcid><to>` into it and answers the intercepted request with a fail-closed error (nothing renders under the OLD url). `BrowserShell::pump` drains the sink on its EXISTING cadence and calls the seam's `Renderer::navigate`.

**Why:** `renderer::SchemeResponse::status` already documents itself as "NOT a redirect channel: a 3xx would be a NAVIGATION ... which belongs to the navigation path". Respecting that is the coherence point: a scheme handler ANSWERS a request, it does not move the browser. The sink is the codebase's existing idiom for exactly this shape (a `Send` handler needs to hand something to a `!Send` shell) — the twin of `werust-android`'s `pending_eval: Arc<Mutex<Vec<String>>>` and `pending_load`. Draining on the shell's existing pump means no new loop and no busy poll.

**Alternatives considered:** (a) add a `Location`/3xx status to `SchemeResponse` — rejected: it re-means a field the seam explicitly scoped, and on Android it is not even expressible (`WebResourceResponse` refuses a 300-399 status code outright); (b) have the resolver fetch the target itself and serve it in place — rejected: that is a 200 rewrite wearing a redirect's clothes, leaving the bar showing a URL whose content the site does not serve there (the parent task rejected the same thing for the same reason).

**Touches:** `renderer::SchemeResponse` (unchanged, deliberately), `BrowserShell::with_redirect_sink` + `pump`, and each edge's `install_ipfs` (which now RETURNS the sink). Ignoring the returned sink is safe: a matched 3xx then fails closed without navigating, i.e. the pre-task behaviour.

## Decision 2 — the chain bound lives in the sink, is per-CHAIN, and counts VISITED targets

**Chosen:** `MAX_REDIRECT_HOPS = 5`. The sink refuses a hop that revisits an already-redirected-to target in the current chain (a cycle) or that exceeds the cap, queueing nothing and leaving a legible fail-closed error.

The chain ENDS on any navigation that is not its own target. `RedirectSink::note_navigation(url)` is called with the TOP-LEVEL document URL from every navigation the core sees — the shell's own `navigate` / `go_back` / `go_forward` / `reload` (which also `reset()`), AND, crucially, every `LoadEvent` the pump drains (`Started` / `Committed` / `Finished` / `Failed` / `UrlChanged`). A reported URL that is neither the target this chain queued nor the document already in flight clears the visited set and the hop budget. Only a load that IS the chain's own drained target continues it.

**Why the load-event path is load-bearing (Gate-2 finding, fixed):** an in-page LINK CLICK never passes through `navigate` / `go_back` / `reload` — the webview loads the link itself and only REPORTS it back (WebKitGTK `load-changed`, Android `onPageFinished`, iOS `didFinish`). With reset wired only to the shell's own entry points, the visited set accumulated for the WHOLE SESSION: the same redirecting nav link worked once and was refused as a cycle on the second click, and five unrelated redirected clicks exhausted the cap. Reporting every load event to the sink is what makes this decision's claim (a user who types a URL, CLICKS A LINK, or goes back gets the full budget again) actually true. Pinned by `an_in_page_link_click_resets_the_redirect_chain_budget_too` and `many_unrelated_redirected_link_clicks_never_exhaust_the_session` (`crates/werust-core/src/lib.rs`), both of which go red if the pump's `note_navigation` is removed; `the_chain_bound_still_holds_across_the_redirect_hops_the_shell_itself_performs` pins the other side (the reset must not reintroduce the unbounded loop).

**Why:** each hop is a separate navigation that re-enters the scheme handler, so there is no recursion depth to bound — the state has to be the sink's. Counting VISITED targets (not just hops) catches the common `/a -> /b -> /a` ping-pong on hop 3 instead of hop 6.

**Why 5 and not 20 (a USER-VISIBLE default, so it is recorded):** browsers cap around 20, but each werust hop is a full content-addressed retrieval (a CAR fetch + per-block verify), so a long chain is expensive rather than merely slow, and a `_redirects` file that needs more than a handful of hops is indistinguishable from one that loops. Reversible in one constant if a real site is found that needs more.

**Alternatives considered:** (a) a hop counter only, no visited set — rejected: a 2-cycle would burn the whole budget before failing, and the error could not say "cycle"; (b) resetting on every navigation including the redirect's own — rejected: that IS the unbounded loop; (c) resetting only on the shell's own entry points — what was built first, and what Gate 2 correctly blocked (session-scoped, see above).

**Touches:** `BrowserShell`'s four user-navigation methods (each resets and reports), `load_resolved_content` (the ENS front door's own navigation), `follow_pending_redirect` (reports the target it is following, which CONTINUES the chain), and `pump`'s event loop (reports every load event). Any future navigation entry point should report through `note_navigation`; forgetting to fails SAFE (the chain simply is not reset) rather than looping.

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

## Decision 7 — only the MAIN FRAME redirects, and the main frame is INFERRED from the top-level URL (a named limitation)

**Chosen:** `resolve_ipfs_request` queues a redirect ONLY for the request it believes is the main-frame document. Everything else — every image, stylesheet, script, fetch — falls through to the honest fail-closed not-found (the site's `404.html`, then the original `PathNotFound`), queueing nothing and spending NO hop budget: exactly the pre-task behaviour. The main frame is identified by comparing the intercepted URI against the TOP-LEVEL document URL the shell last reported through `RedirectSink::note_navigation`, reduced through `frame_key` (the same `normalize_ens_page_key` canonicalization the `ens_pages` association uses, plus query/fragment stripping, so WebKitGTK's authority-less `ipfs:///<cid>` re-report and a `?query` still match the one document).

**Why (Gate-2 finding, fixed):** the scheme handler fires for the main document AND every sub-resource (`crates/webview-renderer/src/backend.rs` `install_ipfs` says so explicitly), but a 3xx is a navigation of the WHOLE page. Without this check, a page with a stale `<img src="/blog/logo.png">` under a `/blog/* /posts/:splat 301` rule would yank the browser off the page the user is reading onto the rewritten image path. A sub-resource must be ANSWERED, never navigated.

**THE LIMITATION, named:** the seam's `renderer::SchemeRequest` carries only `uri` — no `isForMainFrame` flag, no `Sec-Fetch-Dest`. So main-frame-ness is INFERRED, not known. Consequences a future reader should know:

- A sub-resource whose URL is byte-identical to the top-level document URL (a page fetching itself) would be treated as the main frame. Harmless here (it resolves to the same redirect the document already took, and the chain bound covers repeats), but it is a guess.
- Nothing reported yet answers "not the main frame": a sink no shell drives cannot redirect anything, which degrades to the pre-3xx fail-closed behaviour rather than guessing (`a_sink_nobody_reported_a_top_level_url_to_treats_every_request_as_a_sub_resource`).
- The check is made LATE (after both retrievals in the fallback path) precisely so an in-page navigation the shell only OBSERVES has reached the pump before the answer is decided. A misread still fails SAFE: the worst case is a legitimate top-level redirect degrading to a not-found, never a sub-resource moving the browser.

**The proper future fix (follow-up):** add an explicit main-frame flag to `renderer::SchemeRequest` (every platform has it: WebKitGTK's `WebKitURISchemeRequest`, Android's `WebResourceRequest.isForMainFrame`, iOS's `WKURLSchemeTask` request). That is a SEAM change touching all three edges, deliberately out of this task's scope; captured for tasking in `work/notes/observations/scheme-request-carries-no-main-frame-flag-2026-07-27.md`.

**Also noted (no longer a correctness issue):** `RedirectSink.pending` is a SINGLE slot, so a second queued target overwrites an undrained first. With sub-resources excluded, only the one main-frame request per load can queue, so concurrent drops cannot happen; the slot is stated here rather than left silent because it WOULD matter if the main-frame restriction were ever relaxed.

**Alternatives considered:** (a) change the seam now to carry an is-main-frame flag — rejected for scope: it touches desktop + both mobile edges and the parity matrix, and the inference is sufficient and fail-safe today; (b) queue the redirect for any request but only act on it if it matches the current document — rejected: the same inference, just made later and with the hop budget already spent.

**Touches:** `RedirectSink::note_navigation` / `is_main_frame` / `frame_key`, `resolve_ipfs_request`'s fallback branch, and every `BrowserShell` navigation entry point (each must report its top-level URL, or a legitimate redirect on that page silently fails closed).

## Decision 8 — a redirect PUSHES a history entry (no replace exists), so Back SKIPS the redirecting entry

**Chosen:** `BrowserShell::follow_pending_redirect` performs the redirect through the ordinary `Renderer::navigate`, which PUSHES a new session-history entry; the redirected-FROM url therefore stays in history. To keep Back usable, the `RedirectSink` remembers each accepted hop's SOURCE (the url whose `_redirects` rule matched, as a `frame_key`), and a Back that LANDS on a remembered source transparently goes back once more, skipping it. Bounded by the same hop cap as the chain (a chain records at most `MAX_REDIRECT_HOPS` sources, each skip spends one, and landing on anything else ends the skip), so it cannot spin. The remembered sources are cleared exactly when the chain resets (`note_navigation` / `reset`), so a later Back in a different chain never silently jumps an entry the user reached deliberately.

**Why (Gate-2 round-3 finding, fixed):** a real browser REPLACES the current entry when it follows a 3xx, so Back from the target lands on whatever preceded the redirecting url. werust pushed instead, so Back landed ON the redirecting url, its rule matched again, and the user was bounced straight forward — Back was unusable after any redirect. Before this task there was no target to trap on, so the diff introduced the trap and must close it.

**Why not the obvious fix:** the seam has NO replace-current-entry, and WebKitGTK exposes no public API to replace or remove a back-forward-list entry (`WebKitBackForwardList` is read-only; cf. `work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md`, where the same absence shaped the reload decision). A true replace is therefore not available on the desktop backend, and widening the seam for it is out of this task's scope. Skipping on Back is the standard emulation.

**Where the skip runs, and why not in `go_back` itself:** a history move settles ASYNCHRONOUSLY (WebKitGTK keeps reporting the previous entry until its `load-changed` signal, which the `FakeBackend` models faithfully), so right after `Renderer::go_back` the shell cannot yet know where it landed. `go_back` therefore only SNAPSHOTS the sources, and `pump` performs the skip on the load event that first names the landed url. The trailing lifecycle events of the entry stepped off are DROPPED (`back_skip_issued`) so the bar never flashes an entry the user is not staying on, and — load-bearing — so `pump`'s `note_navigation` cannot re-adopt the abandoned url as the top-level document and let a late scheme-handler request for it re-queue the very redirect the skip avoids. The skipped load is also abandoned in the sink (`RedirectSink::abandon_navigation`), which clears the main-frame url for the same reason.

**The edge case, accepted deliberately:** if the redirect source is the FIRST history entry there is nothing further back. The user is left on it and the rule re-fires (they are forward on the target again), rather than Back becoming a silent no-op. No machinery is added for this; a redirect as the very first navigation of a session is the only way to reach it, and the honest re-fire is preferable to a dead button.

**Alternatives considered:** (a) add a `replace_current_entry` to the `Renderer` seam — rejected: WebKitGTK cannot implement it, so it would be a seam method one backend must refuse, which is worse than an emulation that works everywhere; (b) keep a shell-side URL stack and drive Back from it — rejected: the shell deliberately keeps NO URL stack (session history is the backend's truth), and forking that would re-mean the whole history model for one feature; (c) accept the trap and name it a limitation — rejected: it makes Back unusable after a redirect, which is a user-visible regression of an existing browser affordance, not a missing extra.

**Touches:** `RedirectSink::queue` (now takes the hop's SOURCE), `RedirectSink::redirect_sources` / `abandon_navigation`, `BrowserShell::go_back` / `pump` / `skip_back_over_redirect_source`, and every user-initiated navigation entry point (each clears a pending skip, because a user who navigates has overtaken the Back).

## Tests (all network-isolated)

- `crates/werust-core/src/redirects.rs` — a matching 3xx yields `Redirect` (all five codes + the omitted-status 301 default); splat/placeholder injection; off-root 3xx refused (literal and via a capture); a 3xx `to`'s query dropped.
- `crates/werust-core/src/ipfs.rs` — the sink's history bookkeeping: an accepted hop remembers the url it redirected away from (in the WebKit-normalized `frame_key` form), a REFUSED hop remembers nothing, a reset forgets them all, and an abandoned navigation stops counting as the main frame. Plus the seam glue over a pinned retriever double: the queued target is absolute + same-root; the target is NOT pre-fetched; every 3xx code navigates; an off-root target is never queued; a cycle is bounded and fails closed; a reset restores the budget; a dead-end target fails closed on the next hop; a matched 3xx on a SUB-RESOURCE navigates nothing and spends no hop budget. Plus the sink's own bookkeeping: a navigation that is not this chain's target ends the chain; many unrelated redirected clicks never exhaust the budget; a re-report of the in-flight document is idempotent; the main-frame key survives the WebKit authority-less form and a query/fragment; an undriven sink treats everything as a sub-resource.
- `crates/werust-core/tests/ipfs_redirects_fixture.rs` — over a REAL synthesized multi-block UnixFS/dag-pb DAG through the production `TrustlessGatewayCarRetriever`: a 3xx navigates and FOLLOWING the target resolves it through the same per-block-verified retrieval; splat injection + off-root refusal; a cycle bounded; a 3xx matched by a SUB-RESOURCE of the page never navigates the page away while the main-frame request for the same shape still does.
- `crates/werust-core/src/lib.rs` — the shell: a queued redirect is performed on the pump (bar + history move, drained once); Back after a redirect SKIPS the redirecting entry and reaches the page the user came from without re-queueing the redirect (Decision 8), skips EVERY hop of a multi-hop chain, leaves ordinary history untouched, and lands on the source when it is the first entry (the named edge case); an ENS site keeps its `.eth` identity + posture across a redirect; a user navigation resets the chain budget; an IN-PAGE LINK CLICK resets it too (the Gate-2 gap, mutation-checked against removing the pump's `note_navigation`); many unrelated redirected link clicks never exhaust the session; the bound still holds across the hops the shell itself performs; the redirected-FROM request's own failure is suppressed while a refusal still surfaces.
- `crates/werust-core/tests/redirect_navigation_edge_shape.rs` — the per-edge LAST step the pure-Rust gate cannot otherwise see: both mobile edges drain the pending load on the load signals a redirect lands on, Android never maps a 3xx onto a `WebResourceResponse` status (it would throw), and desktop keeps + hands over the sink `install_ipfs` returns. Mutation-checked (reverting one Kotlin handler to a bare `refreshChrome` reds it), with the brace-matching extractor pinned so the guard cannot go vacuous.
- `crates/werust-core/tests/platform_capability_parity.rs` — the new `ipfs-redirects-3xx-navigation` matrix row, implemented on all three contexts.
