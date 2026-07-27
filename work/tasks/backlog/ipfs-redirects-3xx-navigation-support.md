---
title: "Support 3xx redirect rules in the IPFS _redirects file (301/302/303/307/308) — navigate to the target, updating the bar"
slug: ipfs-redirects-3xx-navigation-support
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Follow-on from `ipfs-web-redirects-and-404-fallback-support` (its DECISIONS.md Decision 3): the `_redirects` parser already PARSES 3xx rules (301/302/303/307/308) but a MATCHING 3xx rule currently FAILS the load with a `RedirectNotSupported` reason rather than performing the redirect. This task delivers the deferred 3xx NAVIGATION: a matching 3xx rule should NAVIGATE to the rule's `to` target (updating the URL bar), the browser-idiomatic redirect behaviour, instead of erroring.

READ-FIRST / drift check: confirm `crates/werust-core/src/redirects.rs` still parses 3xx into a rule but the apply path (in `crates/werust-core/src/ipfs.rs`) returns `RedirectNotSupported` for a matching 3xx (Decision 3). Confirm the 200-rewrite / 404-custom-page / off-root-reject / same-root-confinement behaviour from the parent task is intact and build on it.

Fix: on a matching 3xx rule, resolve the `to` (with placeholder/`:splat` injection, confined to the SAME root CID as the parent task requires) and NAVIGATE the shell to `ipfs://<rootcid>/<to>` (a real navigation that updates the bar + history), distinguishing 301/308 (permanent) vs 302/303/307 (temporary) only insofar as werust surfaces them honestly. Keep: same-root confinement (an off-root 3xx `to` is still rejected), verification intact (the redirected target is hash-verified through the same retrieval), and the loop guard (a `_redirects` that redirects to a path that itself redirects must not loop unboundedly — cap the chain, fail closed on a cycle). Coherence with the bar/ENS-name machinery: a 3xx within an ENS site keeps the site identity (compose with the root-CID-prefix ens_pages association).

## Acceptance criteria

- [ ] A matching 3xx rule (301/302/303/307/308) NAVIGATES to the rule's `to` target (bar + history updated), instead of failing with RedirectNotSupported.
- [ ] Placeholder/`:splat` injection into the `to` works for 3xx as it does for 200/404; the `to` is confined to the SAME root CID (an off-root 3xx `to` is still rejected).
- [ ] Verification intact: the redirected target is hash-verified through the same retrieval; a 3xx `to` that does not resolve fails closed.
- [ ] A redirect CHAIN is bounded (a cycle / over-long chain fails closed, no unbounded loop).
- [ ] The 200-rewrite / 404-custom-page / no-_redirects / off-root behaviours from the parent task are unregressed.
- [ ] Applied on desktop + mobile (shared core), or tracked per the parity guard. Tests cover a 3xx navigation, placeholder injection, off-root rejection, a cycle bounded, network-isolated.

## Blocked by

- None. (Builds on the landed `ipfs-web-redirects-and-404-fallback-support`.)

## Prompt

> Goal: deliver the deferred 3xx NAVIGATION for the IPFS `_redirects` file. The parser (`crates/werust-core/src/redirects.rs`) already parses 301/302/303/307/308 rules, but the apply path (`crates/werust-core/src/ipfs.rs`) currently fails a matching 3xx with `RedirectNotSupported` (parent task's DECISIONS.md Decision 3). Make a matching 3xx rule NAVIGATE to its `to` target instead (bar + history updated), the browser-idiomatic redirect.
>
> Keep everything the parent task established: placeholder/`:splat` injection, same-root-CID confinement (off-root `to` rejected), verification intact (redirected target hash-verified, missing `to` fails closed). ADD a bounded redirect chain (cycle / over-long chain fails closed, no unbounded loop). Compose with the root-CID-prefix ens_pages association so a 3xx within an ENS site keeps the site identity in the bar. Done = a matching 3xx navigates, placeholder injection + off-root reject + cycle-bounded + parent behaviours unregressed, network-isolated tests. FIRST re-check redirects.rs parses 3xx and ipfs.rs returns RedirectNotSupported for a matching 3xx.

## Requeue 2026-07-27

CONDUCTOR RETRY (2026-07-27): the previous run produced an EMPTY diff NOT because there is nothing to do, but because the agent's final model turn terminated empty after ~45min of reading/web-research without ever editing a file. The task premise is VERIFIED STILL TRUE: crates/werust-core/src/redirects.rs resolve_target() still returns RedirectsError::RedirectNotSupported for any status not in {200,404,410,451}, and crates/werust-core/src/ipfs.rs maps that to a RendererError, so a matching 3xx rule still FAILS the load instead of navigating. BUILD IT: start editing early, do not over-research. Scope guidance: the 3xx NAVIGATION decision belongs in werust-core (redirects.rs resolve_target should yield a Navigate/Redirect action alongside Serve; ipfs.rs turns it into a real navigation of the shell to ipfs://<rootcid>/<to>) — the CORE + desktop path is the deliverable; on Android, note that WebResourceResponse REFUSES a 300-399 status code, so do NOT try to return a 3xx response from shouldInterceptRequest — perform the redirect by resolving the target in core and serving/navigating to it, which is also what keeps verification intact. Keep: same-root-CID confinement (off-root to rejected), placeholder/:splat injection, hash verification of the redirected target, a BOUNDED redirect chain (cycle/over-long fails closed), and all parent 200-rewrite / custom-404 / no-_redirects behaviours unregressed. Network-isolated tests.

## Requeue 2026-07-27

CONDUCTOR ATTEMPT 3 — THE MECHANISM IS DECIDED. DO NOT RE-DERIVE IT. Two prior runs burned their whole output budget deliberating and wrote ZERO code. Start editing within your first few tool calls. Budget your thinking; write code early, iterate.

THE DECISION (conductor's, final): a 3xx is a NAVIGATION, so it does NOT travel on SchemeResponse. crates/renderer/src/lib.rs already says so on SchemeResponse::status ('This is NOT a redirect channel... belongs to the navigation path'). RESPECT that; do not add a 3xx status to SchemeResponse.

BUILD IT LIKE THIS:
1. core/redirects.rs: add FallbackAction::Redirect { path, status } beside Serve, and make resolve_target() return it for 301/302/303/307/308 instead of Err(RedirectNotSupported). Keep the SAME expand-then-within_root_path order so an off-root 3xx target is still OffRootTarget. Keep RedirectsError::RedirectNotSupported only if some path still needs it; otherwise remove it and its tests.
2. core/ipfs.rs: in resolve_not_found_fallback, on FallbackAction::Redirect, push the absolute target 'ipfs://<reference.cid>/<path>' into a REDIRECT SINK and return the honest not-found/failed-load for the intercepted request (nothing is served in place). The sink is the existing codebase idiom: an Arc<Mutex<Option<String>>> (or a small struct) that the Send scheme-handler closure owns a clone of — exactly like werust-android backend.rs pending_eval: Arc<Mutex<Vec<String>>>, which exists precisely because the handler is Send and the shell's Inner is not.
3. The SHELL drains the sink on its EXISTING refresh/pump cadence (no new loop, no busy poll) and calls the seam's Renderer::navigate(url) — that is what updates the URL bar + history and re-enters the ipfs:// handler, so the redirect target is hash-verified by the SAME retrieval. Wire it on desktop (crates/werust/src/main.rs + crates/webview-renderer) and expose it to the mobile edges over the FFI the same way pending_load/take_pending_load already works on Android.
4. CHAIN BOUND: because each hop is a fresh navigation, count hops in the sink (a redirect-depth counter, cap ~5, reset on any user-initiated navigation). Over the cap or a repeat of an already-visited redirect target = fail closed with a legible error, never an unbounded loop.
5. ENS identity: the target keeps the same root CID, so the existing root-CID-prefix ens_pages association already keeps the site identity in the bar. Verify it does; do not build a new mechanism.

UNCHANGED + must stay green: placeholder/:splat injection, same-root confinement, hash verification, 200-rewrite / custom-404 / no-_redirects behaviours. Tests network-isolated: a 3xx yields a Redirect action, placeholder injection into a 3xx to, off-root 3xx rejected, the chain cap fails closed, and the parent behaviours unregressed.
