---
title: review-gate non-blocking nits for 'ipfs-web-redirects-and-404-fallback-support' (Gate 2 approve)
date: 2026-07-26
status: open
reviewOf: ipfs-web-redirects-and-404-fallback-support
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipfs-web-redirects-and-404-fallback-support' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The deferred 3xx-navigation work has no follow-on task in work/tasks/backlog/ - should one be cut before this lands? DECISIONS.md Decision 3 says the gap 'Touches: a follow-on task', but the backlog holds only ipfs-site-mobile-black-page, ipns-tofu-pin-and-warn-on-change, retrieval-default-egress-before-final-release. The capability-matrix guard cannot catch it because every platform cell is 'implemented' (the gap is feature-wide, not platform-wide), so the deliberate non-delivery is recorded in docs only, not tracked as work.
  (docs/spikes/ipfs-web-redirects-and-404-fallback-support/DECISIONS.md Decision 3 vs ls work/tasks/backlog)
- Has the field case (jolly-roger.eth/unknown on a real build) actually been eyeballed on any of the three edges? The whole user-visible payoff depends on each webview RENDERING a non-200 body rather than substituting its own error page: desktop webkit6 URISchemeResponse::set_status + finish_with_response, Android's status-taking WebResourceResponse overload, iOS HTTPURLResponse on the WKURLSchemeTask. None of the three is covered by a test (they cannot be in this gate) and, unlike sibling tasks, this one records no manual-verification steps in its spike dir. Worst case is a failed load (no trust risk), but the acceptance goal would silently not be reached.
  (crates/webview-renderer/src/backend.rs:673-693; BrowserActivity.kt:569-592; WKWebViewShellController.swift:475-500; docs/spikes/<slug>/ has DECISIONS.md but no README with manual steps)
- Ratify Decision 7: probe_optional counts RetrieveError::Source(_) (any transport failure, not just an HTTP 404) as ABSENT. A transient gateway failure on the /_redirects probe for a site that ships BOTH _redirects and a root 404.html will serve the default 404 page with a 404 status for a path the site's rules said to rewrite 200 - i.e. a page the author did not name for that path, the exact outcome Decisions 3/4/5 refuse elsewhere. Content stays verified and same-root, so this is a coherence/degradation issue, not a trust one.
  (crates/werust-core/src/ipfs.rs probe_optional; DECISIONS.md Decision 7)
- Ratify the new user-visible refusals: a MATCHING 3xx rule and any unparseable/oversized/off-root _redirects now fail the load with a new 'ipfs:// _redirects fallback failed: ...' message on paths that previously produced the plain not-found. Same failure class as before (nothing that used to render now fails), and IPIP-0002 3.4 backs it, but it is a new error surface a human should sign off on.
  (crates/werust-core/src/ipfs.rs redirects_error_to_renderer_error; redirects.rs resolve_target)
- Should the default custom-error-page lookup be nearest-ancestor rather than root-only? HTTP gateways resolve the closest 404.html up the directory chain; DEFAULT_404_PATH is fixed at /404.html, so a site with per-directory 404.html pages gets less than gateway parity. The task only asked for a root 404.html, so this is a scoped-out nicety, not a miss.
  (crates/werust-core/src/redirects.rs DEFAULT_404_PATH; ipfs.rs serve_default_404)
