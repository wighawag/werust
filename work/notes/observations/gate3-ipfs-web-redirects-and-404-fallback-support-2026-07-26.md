---
title: "Gate-3 conductor review: ipfs-web-redirects-and-404-fallback-support (APPROVE)"
date: 2026-07-26
status: approved
reviewOf: ipfs-web-redirects-and-404-fallback-support
gate: gate-3-conductor
mergedCommit: 8473e1e
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge. Re-ran the redirects + fixture tests locally (35 green).

## Done-move + landing

- `work/tasks/backlog/ipfs-web-redirects-and-404-fallback-support.md` -> `done/` on origin/main (`8473e1e`). No ledger residue (clean merge).
- Files: new `crates/werust-core/src/redirects.rs` (+695: the IPIP-0002 parser + fallback), `crates/werust-core/src/ipfs.rs` (+477: probe + apply on PathNotFound), `crates/werust-core/tests/ipfs_redirects_fixture.rs` (+681: the content-addressed fixture), all-platform edge plumbing (desktop/Android/iOS set a non-200 response status), capability matrix, a DECISIONS.md.

## Acceptance criteria (ticked, re-verified locally)

- [x] A not-found path under a site with a root `_redirects` is resolved per its rules; the jolly-roger case (`/* /404.html/index.html 404`) serves the custom 404 page with a not-found status. Fixture `jolly_roger_shaped_site` + the catch-all 404 test. `first_matching_rule_wins`, `an_existing_path_is_never_intercepted`.
- [x] A `200` rewrite (SPA/PWA) serves the target content at the requested URL. Parser supports 200 (rewrite) / 301-308 / 404 / 410 / 451; placeholders + `:splat`.
- [x] 3xx: PARSED but navigation DEFERRED (recorded Decision 3); a MATCHING 3xx rule fails the load with a legible `RedirectNotSupported` reason rather than silently mis-serving. (See follow-on I cut below.)
- [x] A site with NO `_redirects` (and no `404.html`) keeps werust's honest not-found - opt-in per site. `a_scoped_gateway_site_with_no_redirects_keeps_its_honest_not_found`.
- [x] Verification/fail-closed intact: every fallback target is fetched + hash-verified through the SAME retrieval (`the_fallback_content_is_hash_verified_through_the_same_retrieval`); a missing `to` is itself a not-found (`a_redirects_target_that_does_not_exist_fails_closed`); a broken `_redirects` fails the load not mis-serves (`a_broken_redirects_file_fails_the_load_rather_than_serving_the_wrong_page`). Budgets unchanged.
- [x] The `to` target is confined to the SAME root CID - `an_off_root_target_is_rejected_so_one_site_cannot_impersonate_another` (RedirectsError::OffRootTarget). The unique-origin security rationale is recorded. This is the key trust property; it holds.
- [x] Tests network-isolated, ride cargo test: 21 redirects unit tests + 14 fixture tests, all green locally.

## Review-nits triage (Gate-2)

1. (REAL - actioned) The deferred 3xx-navigation has no follow-on task; the matrix guard can't catch it (feature-wide gap, every platform cell 'implemented'). I CUT a backlog follow-on `ipfs-redirects-3xx-navigation-support` in this commit so the deliberate non-delivery is TRACKED as work, not just doc'd.
2. (REAL - flag for you) The field payoff depends on each webview RENDERING a non-200 body rather than substituting its own error page (desktop `URISchemeResponse::set_status`, Android status-taking `WebResourceResponse`, iOS `HTTPURLResponse` on the scheme task). None is test-coverable in this gate, and - unlike sibling tasks - no manual-verification steps were recorded. Worst case is a failed load (no trust risk), but the user-visible goal could silently not be reached. RECOMMEND: eyeball `jolly-roger.eth/unknown` on a real v0.2.6 build (all three edges) to confirm the custom 404 page actually renders.
3. Ratify Decision 7: `probe_optional` treats ANY transport failure on the `/_redirects` probe as ABSENT, so a transient gateway failure on a site shipping BOTH `_redirects` and a root `404.html` could serve the default 404 (status 404) for a path the rules said to rewrite 200. Content stays verified + same-root, so it is a degradation/coherence issue, not trust. Ratify or tighten (only treat an actual 404 as absent).
4. Ratify the new user-visible refusals (a matching 3xx / unparseable / oversized / off-root `_redirects` now fails the load with a new `ipfs:// _redirects fallback failed: ...` message where a plain not-found used to show). Same failure class (nothing that rendered now fails), IPIP-0002 3.4 backs it.
5. Root-only vs nearest-ancestor `404.html`: `DEFAULT_404_PATH` is fixed at `/404.html`; gateways resolve the closest ancestor. Scoped-out nicety (the task asked for root only), not a miss.

## Net effect

werust now honours a site's `_redirects` (IPIP-0002) + root `404.html` on a not-found path, so `jolly-roger.eth/unknown` serves the site's custom 404 like a gateway - opt-in per site, fallback content hash-verified, `to` confined to the root CID (no cross-site impersonation), 3xx navigation deferred + now tracked as a follow-on. Recommend a real-build eyeball of the non-200-body rendering (nit 2).
