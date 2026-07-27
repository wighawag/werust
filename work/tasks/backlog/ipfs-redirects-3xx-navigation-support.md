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

## Requeue 2026-07-27

CONDUCTOR (attempt 3 recovery): the BUILD SUCCEEDED — commit 8630c7d is pushed on work/task-ipfs-redirects-3xx-navigation-support with Gate-1 green (core Redirect action + sink, desktop/iOS wiring, chain cap, DECISIONS.md, tests). Gate 2 did NOT block: its review agent failed to LAUNCH on a transient anthropic overloaded_error. Nothing is wrong with the work. CONTINUE from the kept branch tip: re-verify it still builds, do NOT rewrite or re-litigate the design, and let the review gate run. If everything is already in place, make no further source change beyond any genuine fix.

## Requeue 2026-07-27

CONDUCTOR FIX-UP (Gate-2 BLOCK is CORRECT — fix these two, keep everything else). The branch c0d0f82 is green and preserved; CONTINUE from it. Do NOT redesign the sink or re-litigate the mechanism. Two real defects to fix, both prescribed:

FIX 1 — the redirect chain is SESSION-scoped, must be PER-CHAIN. RedirectSink.visited/hop-count is only cleared by BrowserShell::navigate / go_back / go_forward / reload (crates/werust-core/src/lib.rs:861,1139,1153,1212). An in-page LINK CLICK never passes through those, so visited accumulates all session: the same redirecting link works once and is refused as a cycle on the second click, and 5 unrelated redirected clicks exhaust the cap. THE FIX (as the reviewer suggests): reset the chain whenever a navigation OTHER THAN the pending redirect target COMMITS/finishes — i.e. on a LoadEvent::Started/Committed (and the Android/iOS onPageFinished/onPageFailed report path) whose url is NOT the queued redirect target, clear visited + the hop count. Only a load that IS the queued target continues the current chain. This makes DECISIONS.md Decision 2 (a user who types a URL, CLICKS A LINK, or goes back gets the full budget again) actually TRUE — it is currently false. TEST the link-click path specifically, not just shell.navigate: the existing test a_user_navigation_resets_the_redirect_chain_budget only exercises shell.navigate and misses the whole gap.

FIX 2 — a matched 3xx on a SUB-RESOURCE must not yank the top-level page. resolve_ipfs_request fires for the main document AND every sub-resource (see webview-renderer/src/backend.rs install_ipfs), but queue_redirect always queues a TOP-LEVEL navigation. So a stale image/CSS/JS whose path matches a 3xx rule navigates the whole browser away from the page the user is reading. THE FIX: only queue a redirect for the MAIN-FRAME request. The seam's SchemeRequest carries only uri today, so do it in core WITHOUT a seam change: the shell knows the top-level document URL it is loading (renderer.current_url / the pending load it queued) — treat an intercepted request whose uri matches that top-level URL as main-frame, and ANY OTHER intercepted uri as a sub-resource. On a sub-resource, do NOT queue a navigation and do NOT consume the hop budget: fall through to the honest fail-closed not-found, exactly the pre-task behaviour. RECORD this in DECISIONS.md as a named limitation (main-frame detection is inferred from the top-level URL because the seam carries no is-main-frame flag; a real Sec-Fetch-Dest / isForMainFrame flag on SchemeRequest is the proper future fix) and note it for a follow-up. Also note the single-slot pending: with sub-resources excluded, concurrent drops are no longer a correctness issue, but say so rather than leaving it silent.

Everything else Gate-2 accepted stays as built. Keep tests network-isolated and the parent 200-rewrite / custom-404 / off-root / verification behaviours unregressed.

## Requeue 2026-07-27

CONDUCTOR FIX-UP ROUND 3 (Gate-2 is right again — ONE defect left, the fix is DECIDED). Branch is green and preserved; CONTINUE from its tip. Fixes 1 and 2 from the previous round were accepted — do NOT touch them, do NOT redesign anything else, do NOT re-litigate. Just close this one gap and finish.

THE DEFECT: follow_pending_redirect calls renderer.navigate(target), which PUSHES a history entry, so the redirected-FROM url stays in history. Back then lands on it, its 3xx matches again, and the user is bounced forward — Back is unusable after any redirect.

WHY NOT THE OBVIOUS FIX: a real browser REPLACES the current entry on a 3xx, but the seam has no replace-current-entry and WebKitGTK exposes no public API to replace or remove a back-forward-list entry, so a true replace is not available on the desktop backend. Do NOT invent one and do NOT widen the seam for this task.

THE PRESCRIBED FIX (core-only, the standard emulation): remember the redirect SOURCE url for each hop (the url whose _redirects rule matched), alongside the existing chain state. On go_back, if the entry the shell lands on is a remembered redirect source, transparently go_back ONCE MORE so the user skips over it, instead of re-following the rule forward. Bound it by the same hop cap; clear the remembered sources exactly when the chain resets (note_top_level_navigation). Edge case: if the redirect source is the FIRST history entry there is nothing further back — leave the user there and let the redirect re-fire rather than trapping; record that in DECISIONS.md, do not add machinery for it.

ALSO RECORD IT: add a Decision to docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md (following the Decision 7 precedent for the main-frame flag) covering HISTORY SEMANTICS explicitly — that werust PUSHES rather than replaces because the seam and WebKitGTK offer no replace-current-entry, that Back therefore skips remembered redirect sources as the emulation, the first-entry edge case, and that a proper replace-current-entry seam is the named future fix. DECISIONS.md currently says nothing about history semantics at all; that silence is part of what Gate-2 flagged.

TESTS: update the shell test a_queued_redirect_navigates_the_shell_on_the_pump_and_moves_the_bar_and_history (it currently asserts can_go_back == true after a redirect as a SUCCESS) so it asserts the SKIP behaviour instead, and add a test that Back after a redirect does not bounce forward. Keep everything network-isolated and every parent behaviour unregressed.
