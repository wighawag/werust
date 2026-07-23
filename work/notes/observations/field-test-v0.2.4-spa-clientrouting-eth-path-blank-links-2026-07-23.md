---
title: "Field test v0.2.4 (desktop + mobile): URL bar still frozen on SPA client-side nav, .eth/<path> not ENS-routed, target=_blank dead, ronan.eth blog 500 (SvelteKit __data.json over ipfs://)"
date: 2026-07-23
status: open
kind: field-finding
release: v0.2.4
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

## Context

Human field test of v0.2.4 (the 4 v0.2.3 field-fix tasks) on desktop + mobile, on `ronan.eth` (a SvelteKit `adapter-static` prerendered site, source at `../ronan-eth/web`). The v0.2.4 fixes for the FULL-PAGE-LOAD cases landed, but the real site exercises SPA client-side routing, which reveals a deeper class the fixes (and their tests) did not cover. Root cause of the cluster: `ronan.eth` is a SvelteKit SPA - link clicks are CLIENT-SIDE `pushState` navigations that do NOT trigger a WebKitGTK/WKWebView/Android full page load, so no `load-changed`/`LoadEvent` fires. Confirmed against `../ronan-eth/web` (`@sveltejs/adapter-static`, `+layout.ts`: `prerender = true`, `trailingSlash = 'always'`, `ssr = true`; `build/blog/index.html` AND `build/blog/__data.json` both present).

## Findings

### A. (STILL BROKEN vs v0.2.4 Gate-3) URL bar does not track navigation, even for EXTERNAL links - ALL PLATFORMS

Reported: "on both we still do not have the url match the navigation state, even clicking on a external link it will keep the original name." This looks like it contradicts `urlbar-tracks-in-page-navigation-not-just-pinned-name`, but that fix keys off `pump()` receiving a `LoadEvent` for the new URL and then `drop_pin_on_in_page_nav` dropping the pin. ROOT CAUSE (confirmed): SvelteKit's client router intercepts link clicks (internal AND, via its link handling, often external too) and navigates CLIENT-SIDE without a real page load, so WebKitGTK's `connect_load_changed` NEVER fires (`crates/webview-renderer/src/backend.rs` only feeds `LoadEvent`s from `load-changed`/`load-failed`). The 50ms desktop pump (`crates/werust/src/main.rs`) runs, but there are NO events to drain, so `drop_pin_on_in_page_nav` never runs and `url_override` stays pinned -> bar frozen on `ronan.eth`. The v0.2.4 fix + its FakeBackend `navigate_in_page` test model a backend-delivered LoadEvent, which a SPA pushState nav does NOT produce - so the fake (and a real full-page-load) both pass while the real SPA case fails.

Fix direction (a task): track the webview's ACTUAL current URL even when it changes via client-side history (SPA `pushState`/`replaceState`), not only via `load-changed`. On WebKitGTK: observe `WebView::uri` via its `notify::uri` property signal (fires on same-document history changes too), and/or the `load-changed` we already have, and feed a URL-changed event into the seam so the pump sees it and drops the pin / follows the URL. On WKWebView: KVO on `webView.url` (fires on SPA nav). On Android: a `WebViewClient.doUpdateVisitedHistory` / `onPageCommitVisible` or a `WebView.url` poll on the discrete signals. The seam likely needs a "current URL changed" signal distinct from the load lifecycle (a same-document nav is not a fresh load). Then finding A's drop-pin/follow logic works for SPA nav too.

### B. `.eth/<path>` (e.g. `ronan.eth/blog/`) is NOT detected as ENS - ALL PLATFORMS

Reported: "entering `ronan.eth/blog/` in the url werust do not detect that as .eth and try to do `https://ronan.eth/blog/` which fails with `Error resolving "ronan.eth": Name or service not known`." ROOT CAUSE (confirmed): `eth_name_from_entry` (`crates/werust-core/src/lib.rs`) REJECTS any entry containing `/` ("a `ronan.eth/page` entry is not a bare name in Phase 1"), so `ronan.eth/blog/` falls to the scheme-less classifier -> `HttpsCandidate` (a plausible dotted host) -> `https://ronan.eth/blog/` -> DNS failure.

Fix direction (a task): recognise a `.eth` name WITH a path (`<label>.eth/<path>`) as the ENS front door for `<label>.eth`, then feed the `<path>` into the resolved `ipfs://<cid>/<path>` load (the ipfs path resolution already supports sub-paths). So `ronan.eth/blog/` resolves `ronan.eth` -> `ipfs://<cid>` and loads `<cid>/blog/` (its `index.html`), keeping `ronan.eth/blog/` in the bar. Coherence: this touches `eth_name_from_entry`'s "no `/`" rule and the front door's name-vs-path split; keep an explicit-scheme entry literal, and keep a bare host classification for non-`.eth`. It also composes with finding A (the bar should show `ronan.eth/blog/`).

### C. `target="_blank"` links do nothing - ALL PLATFORMS

Reported: "target=_blank links do nothing and since we currently do not have tabs or windows, we should make them act like non-blank one." ROOT CAUSE (confirmed): the desktop backend (`crates/webview-renderer/src/backend.rs`) has NO `connect_create` / new-window handler, so WebKitGTK's `create` signal (a `_blank`/`window.open` request) is unhandled and the navigation is dropped. Same on mobile (no `onCreateWindow` / `WebChromeClient` / `setSupportMultipleWindows`).

Fix direction (a task): since werust has no tab/window model yet, make a `_blank`/`window.open`/`create`-signal request navigate IN THE SAME view (load the requested URL in the current webview) instead of dropping it. WebKitGTK: handle the `create` signal (return the same view or load-uri in place). WKWebView: `WKUIDelegate.webView(_:createWebViewWith:...)` returning nil after loading the request in the main view. Android: `WebChromeClient.onCreateWindow` routing the target URL back into the same WebView. Applied all platforms; record the decision (in-place until tabs exist).

### D. ronan.eth BLOG shows a "500" with no posts on desktop; blog/portfolio buttons DO NOTHING on mobile - the SvelteKit-over-ipfs:// test case

Reported: desktop - clicking "blog" reaches the blog page but shows NO blog posts and prints a "500 error" (NOT a real server 500 - it is SvelteKit's client-side error output when its data load fails); "portfolio" works fine. Mobile - clicking blog OR portfolio results in NO navigation (desktop portfolio works). ROOT CAUSE (partly confirmed, needs a diagnosis pass): on a SvelteKit SPA client nav to `/blog`, the client router fetches `/blog/__data.json` (present in `../ronan-eth/web/build/blog/__data.json`) to hydrate. That fetch goes through werust's `ipfs://` scheme handler. The blog's `__data.json` load failing (a path-resolution / trailing-slash / CAR-scope / MIME issue for the NESTED `blog/__data.json`, vs portfolio which may have a simpler shape) makes SvelteKit render its error boundary = the "500". Portfolio working but blog not suggests a per-route data-fetch difference (blog is a list route with its own `__data.json`; confirm what differs). Mobile no-navigation is a SEPARATE symptom (the SPA nav itself is not proceeding on Android - possibly the ANR-fix executor serialisation, or the WebView not routing the client nav), needs device diagnosis.

Fix direction: this is a DIAGNOSE task (`diagnosing-bugs`), and `../ronan-eth` is the ideal fixture. Diagnose (a) why `blog/__data.json` (nested route data) fails over `ipfs://` while portfolio works, and (b) why mobile does not navigate at all on the blog/portfolio buttons. Then fix the `ipfs://` handler path/data resolution so a SvelteKit `adapter-static` site (prerendered pages + `__data.json` client-nav data) works end to end. STRONGLY consider landing a ronan-eth-derived (or a minimal SvelteKit-adapter-static) fixture as a werust regression test (the human suggested "the code for ronan-eth is in ../ronan-eth and this could be a nice test to check for werust").

## What works (v0.2.4 confirmed)

- Desktop portfolio button navigates and renders fine.
- The v0.2.4 full-page-load fixes are in; this cluster is the SPA-client-routing layer beneath them.

## Cross-cutting note

Findings A + B + D share the SPA/SvelteKit-over-ipfs:// reality: werust's model assumed navigation = a backend page load, but a modern static site does most navigation client-side. The url-tracking (A), the .eth/path entry (B), and the data-fetch-over-ipfs (D) all need werust to handle same-document/client-driven navigation and nested static-site data. A ronan-eth regression fixture would guard the whole cluster.
