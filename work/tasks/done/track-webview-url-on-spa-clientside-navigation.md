---
title: "URL bar must track SPA client-side navigation (pushState/replaceState / same-document history), not only backend page loads - fixes the bar frozen on ronan.eth"
slug: track-webview-url-on-spa-clientside-navigation
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
needsAnswers: true
---

## What to build

FIELD FINDING (v0.2.4, human): "on both we still do not have the url match the navigation state" (the URL bar stays frozen). Plus, the `ipfs://` CID STILL leaks back into the bar on back/forward/reload despite the v0.2.4 `ens-history-name-rederive-async-and-normalized` fix. Root-cause source: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` (finding A + the ADDENDUM).

THIS TASK HAS TWO COUPLED PARTS (same identity-tracking machinery):

**Part 1 - track SPA client-side URL changes** (the frozen-bar-on-internal-nav). (The EXTERNAL-link frozen-bar symptom is a DIFFERENT bug - `target=_blank` links are dropped; that is fixed by `blank-and-window-open-links-navigate-in-place`, which makes them fire a real load event so the existing follow logic works. This task covers the INTERNAL SvelteKit SPA case only, which fires no event.)

**Part 2 - fix the `ipfs://`-reappears leak by making the ENS association ROOT-CID-PREFIX, not root-entry-only** (the ADDENDUM finding). This is REQUIRED here because it shares the mechanism: `ens_pages` today is populated only in `load_resolved_content` keyed on the exact resolved ROOT normalized key (the bare `<cid>`). After loading `ronan.eth` the user navigates in-page (SPA, or a real sub-path load) to `<cid>/blog/...`; a later back/forward/reload can land `current_url` on that SUB-PATH CID (`<cid>/blog`), whose normalized key DIFFERS from the stored root `<cid>` (the normalizer preserves sub-paths), so `refresh_chrome`'s `ens_pages.get(normalize(current_url))` MISSES and the final `else if let Some(url) = current_url` branch leaks `ipfs://<cid>/blog/` into the bar. FIX: recognise the current backend URL as being UNDER a known ENS site's ROOT CID (a prefix match on the root CID), and re-derive the name (display `ronan.eth/<in-site-path>`, or at least `ronan.eth`) + re-mark the posture for ANY `<rootcid>/<anypath>` entry - never the raw `ipfs://<rootcid>/<path>`. So the association is with the WHOLE SITE (root CID), not just the exact root entry.

READ-FIRST / drift check: this is the DEEPER layer beneath `urlbar-tracks-in-page-navigation-not-just-pinned-name` (done). That task drops the pin / follows the URL when `pump()` drains a `LoadEvent` for a new URL. But `ronan.eth` is a SvelteKit SPA: link clicks are CLIENT-SIDE `pushState` navigations that do NOT trigger a full page load, so WebKitGTK's `connect_load_changed` NEVER fires (`crates/webview-renderer/src/backend.rs` only feeds `LoadEvent`s from `load-changed`/`load-failed`). The 50ms desktop pump runs but has no events to drain, so `drop_pin_on_in_page_nav` never runs and the bar stays frozen. Confirm the backend still feeds `LoadEvent`s ONLY from the load-lifecycle signals (no same-document/URL-change observation yet).

Fix: observe the webview's ACTUAL current URL even when it changes via CLIENT-SIDE history (SPA `pushState`/`replaceState`, same-document navigation), and feed that URL change into the seam so the shell's pin-drop/follow logic (and the ENS re-derive) works for SPA navigation too. A same-document nav is NOT a fresh load, so model it as a distinct "current-URL changed" signal, not a `Started`/`Committed`/`Finished` lifecycle event (do not fake a load).

Per platform:
- **Desktop (WebKitGTK)**: observe `WebView::uri` via its `notify::uri` property signal - it fires on same-document history changes (pushState/replaceState) as well as real loads. On a URI change with no accompanying load-lifecycle transition, enqueue a seam "URL changed" signal carrying the new URI. (`crates/webview-renderer/src/backend.rs` `connect_load_signals` - add a `connect_uri_notify` / `connect_notify(Some("uri"), ...)`.)
- **iOS (WKWebView)**: KVO-observe `webView.url` (fires on SPA nav); post the new URL into the seam. (`crates/werust-ios`.)
- **Android (System WebView)**: `WebViewClient.doUpdateVisitedHistory(view, url, isReload)` (fires on pushState/replaceState) and/or `onPageCommitVisible`; feed `url` into the seam. (`crates/werust-android/.../BrowserActivity.kt`.)

Seam + core:
- Add a seam way to surface a same-document URL change distinct from `LoadEvent` (e.g. a `LoadEvent::UrlChanged { url }` variant, or a separate `poll_url_change()` - decide + record; a new `LoadEvent` variant is likely cleanest so it flows through the existing `pump()` drain and `drop_pin_on_in_page_nav`). The `LoadEvent::url()` accessor already exists.
- In `pump()` / the shell, a URL-changed signal drives the SAME pin-drop/follow + ENS-re-derive logic as an in-page load event: if the new URL normalizes off the `pinned_root_key`, drop the pin and follow; if it is a known `ens_pages` entry, re-derive the name+posture; the trust posture keeps tracking the actual load path (a same-document nav within a verified `ipfs://` site stays verified; a nav to a different origin updates it). Do NOT re-mean trust.
- Coherence: this composes with `eth-name-with-path-routes-to-ens-and-subpath` (a `.eth/blog/` load pins `ronan.eth/blog/`) - after that lands, an in-SPA nav to `/portfolio` should update the bar to the portfolio path. Keep the pinned-name-for-the-root vs follow-the-path behaviour coherent with the existing `pinned_root_key` decision.

## Acceptance criteria

- [ ] (Part 2, the leak) After loading an ENS site and navigating to a SUB-PATH within it, a back / forward / reload that lands on ANY `<rootcid>/<path>` of that site shows the `.eth` name (as `ronan.eth/<path>` or at least `ronan.eth`) + its ENS posture in the bar - NEVER the raw `ipfs://<rootcid>/<path>`. The `ens_pages` association is matched on the ROOT CID PREFIX of the current entry, not only its exact normalized key. This closes the v0.2.4 `ipfs://`-reappears leak.
- [ ] Clicking a link that navigates CLIENT-SIDE (SvelteKit/SPA pushState, same-document) updates the URL bar to the new location on desktop and mobile, instead of freezing on the pinned `.eth` name.
- [ ] A same-document URL change is modelled as a distinct signal (not a faked load lifecycle event); the decision (new `LoadEvent` variant vs separate poll) is recorded.
- [ ] The pin-drop/follow + `ens_pages` re-derive + posture logic runs for a SPA URL change exactly as for a backend in-page load event: off-root -> follow the URL; back onto a known ENS root -> re-derive the name; posture tracks the actual origin/verification of the current document.
- [ ] A full-page-load navigation (the existing path) is unregressed; a plain non-SPA site still tracks its URL as before.
- [ ] Applied on desktop and mobile via each platform's same-document-URL observation, or tracked per the parity guard.
- [ ] Tests cover a SPA/same-document URL change updating the bar + dropping the pin + re-deriving on return, driven through the seam (the FakeBackend gains a way to emit a same-document URL change WITHOUT a full load, mirroring how it gained `navigate_in_page`). Network-isolated.

## Blocked by

- None. (Composes with the `.eth/<path>` and urlbar-in-page tasks, which have landed; build on their `pinned_root_key` / `ens_pages` machinery.)

## Prompt

> Goal: TWO coupled fixes. (1) Make the URL bar track SPA client-side navigation (SvelteKit pushState/replaceState, same-document history), which fires NO WebKitGTK `load-changed`, so the bar freezes on `ronan.eth` for INTERNAL nav (external-link freeze is the separate `_blank`-dropped bug, fixed elsewhere). (2) FIX the `ipfs://`-STILL-reappears leak: `ens_pages` is populated root-only, so a back/forward/reload landing on a SUB-PATH `<cid>/blog` of a known ENS site MISSES the exact-key lookup and leaks the raw CID. Make the ENS association ROOT-CID-PREFIX: recognise any `<rootcid>/<path>` as under a known ENS site -> show `ronan.eth/<path>` (or `ronan.eth`) + posture, never `ipfs://<rootcid>/<path>`.
>
> Where to look: `crates/webview-renderer/src/backend.rs` `connect_load_signals` (add `notify::uri` observation - WebKitGTK's `WebView::uri` fires on same-document history changes); iOS `crates/werust-ios` (KVO on `webView.url`); Android `crates/werust-android/.../BrowserActivity.kt` (`WebViewClient.doUpdateVisitedHistory`). Add a seam signal for a same-document URL change DISTINCT from a load lifecycle event (likely a `LoadEvent::UrlChanged { url }` variant so it flows through `pump()` -> `drop_pin_on_in_page_nav`; `LoadEvent::url()` already exists). In the shell, a URL change drives the SAME pin-drop/follow + `ens_pages` re-derive + posture-tracks-the-load-path logic as an in-page load event. Do NOT re-mean trust or fake a load lifecycle.
>
> Done = a SPA client-side nav updates the bar (drops the pin/follows, re-derives on return) on desktop + mobile; full-page loads unregressed; the FakeBackend can emit a same-document URL change without a full load (mirroring `navigate_in_page`); network-isolated tests. Composes with the `.eth/<path>` task. FIRST re-check the backend feeds LoadEvents only from load-lifecycle signals.

## Requeue 2026-07-23

Gate-2 verdict was unparseable JSON (a stochastic-reviewer JSON-escaping flake), NOT a real review block. The build is green and the work branch is preserved. Continue from the kept branch tip and re-run the review; no code changes needed unless the fresh review finds a genuine issue.
