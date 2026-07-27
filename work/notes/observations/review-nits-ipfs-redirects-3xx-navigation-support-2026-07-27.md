---
title: review-gate non-blocking nits for 'ipfs-redirects-3xx-navigation-support' (Gate 2 approve)
date: 2026-07-27
status: open
reviewOf: ipfs-redirects-3xx-navigation-support
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipfs-redirects-3xx-navigation-support' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- A matched 3xx on a SUB-RESOURCE now falls through to serve_default_404, i.e. the site's 404.html bytes at status 404. Pre-task, ANY matched 3xx (main frame or sub-resource) fail-closed with the RedirectNotSupported reason. Both DECISIONS.md Decision 7 and the ipfs.rs doc comment claim this is 'exactly the pre-task behaviour', which is not accurate. Ratify the new default (a stale image now receives 404.html bytes rather than a hard error) and correct the claim.
  (crates/werust-core/src/ipfs.rs resolve_not_found_fallback: Some(Ok(FallbackAction::Redirect{..})) if !redirects.is_main_frame(uri) => serve_default_404(...))
- The Back-skip only covers the Back that IMMEDIATELY follows the chain. RedirectSink.sources is cleared by note_navigation on any load that is not the chain's own target, so after the user browses on from the redirect target, a later Back lands ON the redirect source, its rule re-fires and bounces them forward once. It self-heals (the bounce repopulates sources, so the next Back skips), but it costs a wasted press plus a retrieval. Decision 8 names only the first-entry edge case, not this residual. Accept and document, or widen the retention?
  (lib.rs go_back snapshots redirects.redirect_sources(); ipfs.rs note_navigation clears chain.sources whenever the reported url does not continue the chain.)
- follow_pending_redirect's doc says that when renderer.navigate fails it 'leaves the handler's fail-closed error standing', but pump already cleared chrome.last_error for any reason carrying REDIRECT_NAVIGATING_MARKER before follow_pending_redirect runs. So a backend that cannot start the redirected load shows the user no error at all. Rare, but the comment and the code disagree.
  (crates/werust-core/src/lib.rs pump: Failed arm clears last_error on the marker; follow_pending_redirect returns false on navigate error without restoring it.)
- Main-frame inference on the MOBILE edges: is_main_frame depends on the core having already been told the new top-level URL. On Android, shouldInterceptRequest runs on a WebView worker thread and can fire BEFORE onPageStarted drives the core, so an in-page link click onto a 3xx path may resolve before top_level updates and silently degrade to a not-found. Decision 7 reasons only about the desktop pump cadence, yet the capability matrix marks the row implemented on all three contexts. Fails safe, but worth naming.
  (BrowserActivity.kt shouldInterceptRequest (worker thread) vs onPageStarted -> core.onPageCommitted -> pump -> note_navigation; docs/platform-capability-matrix.toml ipfs-redirects-3xx-navigation android = implemented.)
- Ratify the in-scope decisions this build made on its own, all recorded in docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md: MAX_REDIRECT_HOPS = 5 (a user-visible default, tighter than the ~20 browsers use); suppressing the redirected-FROM request's error banner via a marker substring in the failure reason; REMOVING the public RedirectsError::RedirectNotSupported variant rather than deprecating it; and splitting a NEW capability-matrix row ipfs-redirects-3xx-navigation out of ipfs-web-pathing-fallback. Each looks right and reversible; they need a human nod, not a change.
  (DECISIONS.md Decisions 2, 5, 6; docs/platform-capability-matrix.toml new [[capability]] block.)
