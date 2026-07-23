---
title: "Real ipfs:// sites render CORRECTLY (styling/layout intact, not black): trace + fix sub-resource resolution across desktop and mobile"
slug: ipfs-site-renders-correctly-subresources-and-mime
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Make a real multi-resource `.eth`/`ipfs://` site actually render RIGHT, not broken. On the v0.2.1 build a real site loaded but rendered wrong: on desktop the text colour was off and some layout was off (an unstyled / partially-styled render — CSS and/or JS/fonts not applied); on Android the page was FULLY BLACK. The unit-level verified-render path passes all offline CAR fixtures, so the gap is in how a REAL browser (WebKitGTK, Android System WebView, iOS WKWebView) requests and receives the sub-resources of a directory site, which the offline fixtures did not exercise.

First TRACE, then fix. Capture what each real WebView actually requests when loading a directory site's `index.html` and what the handler returns per request: the main document, then each relative sub-resource (`style.css`, `app.js`, images, fonts). Determine the failure(s) among the likely candidates and fix them:
- **Sub-resource requests not reaching the handler / wrong URI** — a relative URL in the HTML (`href="/style.css"` or `href="style.css"`) resolves against the `ipfs://<cid>` base to some `ipfs://<cid>/style.css` (or `ipfs://style.css` if the base/authority is mishandled). Confirm the resolved URI actually hits `resolve_ipfs_request` with the right cid+path, on all three WebViews.
- **Wrong MIME/charset** — the handler derives MIME from the path extension (`mime_type_for_path`); a stylesheet served with the wrong type (or missing charset) will not be applied; an HTML document without a charset can mis-render. Verify CSS -> `text/css`, JS -> a script type the engine accepts, and that the HTML document carries a usable content type/charset.
- **Directory-root / index resolution** — the root request (`ipfs://<cid>` or `ipfs://<cid>/`) must resolve to `index.html` and be served as `text/html`; confirm it does on each edge.
- **Mobile-specific black page** — determine whether the Android black render is the same missing-CSS cause, a WebView background/theme default, a charset/MIME issue, or the mobile `resolve_scheme` returning wrong bytes/type for the root; fix accordingly.

## Acceptance criteria

- [ ] A real multi-resource UnixFS directory site renders CORRECTLY on desktop (WebKitGTK): stylesheet(s) applied (correct text colour + layout), scripts run, images/fonts load — visually at parity with the same site served over http(s).
- [ ] The same site renders correctly on Android (System WebView) — NOT a black/blank page — and on iOS (WKWebView).
- [ ] Sub-resource requests resolve to the correct cid+path through the shared core path and are served with the correct MIME/charset on every edge; the directory root resolves to `index.html` as `text/html`.
- [ ] A trace/finding records what each real WebView requested and what was returned (so the fix is grounded in observed behaviour, not guessed), referenced from the done record.
- [ ] Tests/fixtures cover the multi-resource render (a directory with index.html + a stylesheet + a script + an image, each resolved and correctly typed), closing the gap that offline single-blob fixtures left. Where a real-WebView visual assertion is impractical in CI, assert the per-sub-resource resolution + MIME at the seam, and document the manual visual check.

## Blocked by

- None — can start immediately. (Related to `ipfs-retrieval-off-main-thread-no-ui-freeze`: a blocking/timing-out sub-resource fetch could ALSO cause partial styling, so if that task lands first, re-trace on top of it.)

## Prompt

> Goal: make a real `.eth`/`ipfs://` directory site render correctly (styled, laid out, not black) on desktop + Android + iOS. v0.2.1 loaded a real site but rendered it unstyled/wrong (desktop) and fully black (Android). The verified-render unit path is green; the gap is real-WebView sub-resource behaviour the offline fixtures never exercised. TRACE FIRST (what each WebView requests for a directory site's index + relative assets, and what the handler returns), THEN fix.
>
> Where to look: the resolve path `crates/werust-core/src/ipfs.rs` (`parse_ipfs_uri` + `resolve_ipfs_request` + `mime_type_for_path`), the desktop handler `crates/webview-renderer/src/backend.rs`, the mobile edges (`crates/werust-android`, `crates/werust-ios`). Likely causes: relative-URL/authority resolution so a sub-resource URI is malformed; wrong MIME/charset so CSS/JS is not applied; the directory-root -> index.html mapping; a mobile WebView default that renders black without styling. Confirm on the REAL WebViews, not just the offline fixture.
>
> Done = a real multi-resource directory site renders correctly and identically to its http(s) serving on all three platforms (no black page, styling + scripts + images intact), grounded in an actual request trace, with fixtures covering the multi-resource case. FIRST re-check the resolve/MIME code still matches this description. RECORD the trace + the root cause(s) + fixes durably (a finding + the done record).
