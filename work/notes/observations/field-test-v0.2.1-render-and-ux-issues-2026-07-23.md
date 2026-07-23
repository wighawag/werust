---
title: "Field test v0.2.1: 4 real render/UX issues (main-thread freeze, broken styling/layout, invisible IPNS error, black mobile page)"
date: 2026-07-23
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: field-observation
source: human manual test of the v0.2.1 build on desktop + Android
---

## What the human observed (v0.2.1, real builds)

Four distinct issues, each a different layer. Captured verbatim before triage.

### 1. Desktop: ~10s FREEZE (GNOME "application not responding, wait?") before the page loads
The window froze for ~10s on an ENS/ipfs load; GNOME offered the force-quit-or-wait dialog; waiting let it eventually load. LIKELY ROOT CAUSE (from code): the `ipfs://` scheme handler (`crates/webview-renderer/src/backend.rs`, `register_uri_scheme`) calls `resolve_ipfs_request` SYNCHRONOUSLY inside the GTK scheme-handler closure, which runs the FULL trustless-gateway CAR fetch + per-block verify + DAG reassembly ON THE GTK MAIN THREAD. And it runs PER REQUEST (the main HTML plus every sub-resource), each a blocking network CAR retrieval. Blocking the UI thread on network I/O is the freeze. FIX DIRECTION: move retrieval off the main thread (async/worker + finish the scheme request when bytes are ready), or at minimum stream. NOTE this compounds with the desktop being a single GTK thread (the same thread the Android task had to Mutex-guard for a different reason).

### 2. Desktop: page renders WRONG — text colour off, some layout off
The site loaded but looked wrong: wrong text colour, layout partly off. STRONGLY SUGGESTS sub-resources (CSS, maybe JS/fonts) are not being applied — an unstyled/partially-styled render. Candidates to investigate: (a) sub-resource `ipfs://<dir-cid>/style.css` requests failing or timing out (see #1's per-request blocking), (b) a relative-URL/authority mismatch so the CSS request never reaches the handler, (c) MIME wrong for some asset (the handler derives MIME from the path extension via `mime_type_for_path`; a CSS served as the wrong type would not apply). Needs a real trace of which sub-resource requests fire and what each returns.

### 3. Desktop: `ronan.eth` resolves to IPNS and FAILS, but the error is not easily seen
`ronan.eth`'s contenthash is ipns-ns; the IPNS resolution failed, but the failure was not clearly surfaced. Two sub-issues: (a) WHY did the IPNS resolve fail (a real resolution bug? an endpoint issue? worth capturing the actual error), and (b) the failure is stored in `ChromeState.last_error` but is "not easily seen" — the error surfacing in the desktop chrome is too weak/hidden. A fail-closed load must show its reason PROMINENTLY (the whole honesty point). FIX DIRECTION: make the load-failure reason a visible in-page/chrome error state, not a subtle status line.

### 4. Mobile (Android): the page loaded but is FULLY BLACK
On Android the ENS/ipfs page loaded but rendered fully black. Could be: the same missing-sub-resource/CSS issue as #2 but worse (a dark default with no styling), a MIME/charset issue, a WebView background/theme default, or the mobile scheme interception returning the wrong bytes/type for the root. Needs a mobile trace (what the WebView requested and what `resolve_scheme` returned per request).

## Triage note

#1 (main-thread blocking retrieval) is the highest-value: it is almost certainly the freeze AND a contributor to #2/#4 (blocking per-sub-resource fetch -> timeouts -> partial/failed styling). #2 and #4 may share a root cause (sub-resource resolution) and should be investigated together with a real request trace. #3 splits into a resolution-correctness question and an error-visibility UX fix. NONE of these are hand-fixed here; each becomes a scoped task. The verified-render + parity work is sound at the unit level (all offline tests pass); these are integration/real-browser behaviours the offline fixtures did not exercise — the same class of gap that hid the original mandalas.eth bug, now at the rendering layer.

### 5. Chrome does not reset to a "loading" state on navigation — stale trust indicator persists while a new (differently-trusted) page loads
On navigating to a new page, the stop button (cross) appears (loading IS detected), but the trust indicator and other chrome keep showing the PREVIOUS page's state. `refresh_chrome` unconditionally paints `trust_indicator(state)` from `state.trust_posture` even while `is_loading()` is true, so the user sees the OLD page's trust badge (e.g. a stale "content-verified") while a NEW, potentially unverified/differently-trusted site is loading. This is a trust-HONESTY issue, not just cosmetic: the indicator should become a neutral loading/spinner state on navigation and only show the new posture once the new load's real posture is known. FIX DIRECTION: while `ChromeState::is_loading()`, show a loading/spinner trust state (neutral, "loading…") instead of the carried-over posture, on desktop AND mobile; reveal the real posture only when the load settles. This is the human's explicit request ("the cross appears to stop, but everything else like the trust indicator stays; they should become a spinner to show a new site with potentially different trust is loading").

## UPDATE 2026-07-23 (human re-test): #2 and #4 are TWO DIFFERENT bugs

The human re-tested and found the styling is wrong even on `https://mandalas.eth.limo` — a PLAIN https load, NOT werust's ipfs:// verified path. This splits the render issues:
- **#2 (wrong styling) is a BASE renderer/WebView-config bug**, not ipfs sub-resources: EVERY page renders wrong (plain https too), so it is werust's WebView configuration (no `WebKitSettings` / default fonts attached — confirmed the desktop `WebView::builder()...build()` sets none). Human's hypothesis: "maybe WebView needs base styling." Re-scoped as `webview-base-styling-wrong-on-all-pages` (diagnose against a plain https page).
- **#4 (mobile black page) is ipfs-SPECIFIC**: on mobile the SAME site renders fine via `https://mandalas.eth.limo` but BLACK via werust's ipfs:// path. So the mobile WebView is fine; werust's mobile ipfs interception returns something wrong. Re-scoped as `ipfs-site-mobile-black-page`.
The original combined `ipfs-site-renders-correctly-subresources-and-mime` task was dropped as mis-framed (it assumed an ipfs sub-resource cause; the plain-https evidence disproves that for the styling half).

## UPDATE 2 (2026-07-23, two screenshots werust vs Firefox of mandalas.eth.limo): the styling bug is PRECISELY a UA color-scheme issue

Side-by-side screenshots pinned it exactly. The page mostly renders CORRECTLY in werust (mandalas art, orange headings, rainbow bars, layout all right). The ONLY breakage: UA-styled `<button>` controls (nav: MANDALAS/Wallet/About) render as SOLID WHITE boxes with INVISIBLE white-on-white text; "Mint It" labels/icons are barely-visible. In Firefox the same buttons are dark/transparent with readable white text. So it is NOT missing base styling and NOT ipfs sub-resources (reproduces on plain https): it is the USER-AGENT COLOR SCHEME for form controls. WebKitGTK ties the page color scheme + UA control theming to the GTK theme (`gtk-application-prefer-dark-theme`; WebKit bugs 196685/197947). werust sets no color scheme, so controls get the LIGHT UA default -> white buttons on a dark page -> invisible text. Re-scoped `webview-base-styling-wrong-on-all-pages` -> `webview-ua-color-scheme-controls-wrong` (set the WebView to a dark color scheme / honour prefers-color-scheme). This is a much smaller, precise fix than "base styling".

## UPDATE 3 (2026-07-23, human): FOLLOW the OS color scheme, do not force dark
The mandalas button breakage is partly the SITE's own bug (relies on UA defaults without declaring `color-scheme`). The werust-side fix is to RESPECT the OS light/dark setting, NOT to force always-dark (forcing dark would break light-mode users and misrepresent the user's preference). The phone was in dark mode; werust ignored it and defaulted to light UA controls -> white buttons -> invisible text. Re-scoped `webview-ua-color-scheme-controls-wrong` -> `webview-follow-os-color-scheme`: report the OS preference so `prefers-color-scheme` + UA controls match, on desktop (GTK dark pref) and mobile (Android UI_MODE_NIGHT / iOS userInterfaceStyle), without overriding a page's own declared color-scheme.
