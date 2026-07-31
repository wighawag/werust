---
title: "Android's WebView defaults are browser-wrong beyond DOM storage: pinch-zoom is OFF and the wide viewport is OFF, awaiting a human's UX call"
date: 2026-07-31
status: open
---

While fixing `window.localStorage` on Android (`android-enable-dom-storage-and-guard-web-platform-parity`) I audited the neighbouring `WebSettings` and MEASURED their shipped defaults off a fresh `WebView` (emulator, API 36, System WebView 142). Two are wrong for a browser and were deliberately left alone, because they are user-visible UX decisions a human owns: **pinch-to-zoom is OFF** (`builtInZoomControls = false`, so a user cannot zoom a page at all, which is an accessibility affordance as much as a convenience), and **`useWideViewPort` / `loadWithOverviewMode` are both false**, so a legacy desktop-oriented page with no `<meta viewport>` is laid out at phone width instead of the ~980px wide viewport every other mobile browser gives it — which bears on werust's hard full-compatibility requirement for the normal server web.

Two audited items came out the OPPOSITE way to what the API docs suggest and need no action: `textZoom` already follows the OS accessibility font scale (measured 130 at system font scale 1.3, so pinning it to 100 would REMOVE a user's setting), and `mediaPlaybackRequiresUserGesture` already defaults to the browser-correct `true`.

The full list with recommendations, and the security-adjacent settings this audit deliberately did NOT cover (`setMixedContentMode`, file access, Safe Browsing, the stock `WebView` user-agent string), is at `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/WEBSETTINGS-AUDIT.md`. Nothing there was changed.
