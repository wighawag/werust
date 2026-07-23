---
title: "Enable the real Web Inspector (WebKit/Chrome devtools: console REPL + network) on every platform — desktop in-window, mobile via tethered inspection"
slug: enable-web-inspector-devtools-all-platforms
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

Give werust a REAL web devtools experience — the full browser inspector (a console with a JS REPL you can TYPE into, a network tab, DOM/sources) — on every platform, by enabling each platform WebView's OWN inspector rather than building a custom window. HUMAN REQUEST (v0.2.2): "I want a window that lets me see console + network at least, and ideally type in the console like a desktop browser." Correction from the earlier scoping: the shift+F12 that works today shows the GTK INTERACTIVE DEBUGGER (GTK widget tree / CSS), NOT web content; and werust does NOT need a custom debug window because every platform's WebView already ships a full inspector.

Each platform, enable its native inspector (same devtools capability, reached per-platform):
- **Desktop (WebKitGTK)**: the WebView does not enable developer extras today. Set `WebKitSettings::enable-developer-extras = true` and wire a shortcut to SHOW the inspector in-window (`WebInspector::show`), so a real WebKit Web Inspector (console REPL + network + DOM) opens over the page. Pick a shortcut that does NOT collide with the GTK interactive debugger (e.g. F12 / Ctrl+Shift+I for the web inspector; leave GTK's own to its default) — or intercept and route to the web inspector.
- **iOS (WKWebView)**: set `webView.isInspectable = true` (iOS 16.4+), so the page is inspectable via Safari's Web Inspector (macOS) over USB — the same WebKit devtools. (On the simulator it is always enabled.)
- **Android (System WebView)**: call `WebView.setWebContentsDebuggingEnabled(true)`, so the page is inspectable via `chrome://inspect` (Chrome DevTools: console REPL + network) over USB.

Gate the mobile inspectability behind a debug/dev build (or a setting) so a release build is not silently inspectable if that is a concern — decide and record. The point is: the same full devtools (console you can type in + network) are available on all three, using each platform's real inspector, not a hand-built one.

## Acceptance criteria

- [ ] Desktop: a shortcut opens the WebKitGTK Web Inspector in/over the werust window — a real console with a JS REPL (type + evaluate) and a network tab — for the current page. Developer extras are enabled. The shortcut does not conflict with the GTK interactive debugger.
- [ ] iOS: the WKWebView is `isInspectable` (appropriately gated), so the page can be inspected via Safari Web Inspector — console + network — over USB.
- [ ] Android: `setWebContentsDebuggingEnabled(true)` (appropriately gated), so the page can be inspected via chrome://inspect — console + network — over USB.
- [ ] The devtools give at least a typeable console and a network view on every platform (using the platform's real inspector).
- [ ] Inspectability on mobile (and developer-extras on desktop) is gated as decided (debug build / setting) and recorded; the capability is registered in the platform-capability matrix (all three implemented, per its reach).
- [ ] Tests/build-config cover that the settings are enabled (developer-extras on desktop; the mobile flags set in the intended build), documented with the manual "open the inspector" steps per platform.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: enable the REAL web inspector (WebKit/Chrome devtools — typeable console + network) on all three platforms, using each platform's OWN inspector rather than a custom window. The human wants a desktop-browser-like console + network everywhere. Note: today's shift+F12 is the GTK interactive debugger (widgets), not web content.
>
> Where to look: desktop `crates/webview-renderer/src/backend.rs` (the `WebView::builder()...build()` sets no `WebKitSettings`; add a Settings object with `enable-developer-extras = true`, and wire a shortcut in the shell `crates/werust/src/main.rs` to `WebInspector::show()` on the view — WebKitGTK `WebKit2.WebInspector`, `open-window` signal if you want your own window, else it opens its own). iOS `crates/werust-ios` (`webView.isInspectable = true`, iOS 16.4+; Safari Web Inspector over USB). Android `crates/werust-android` (`WebView.setWebContentsDebuggingEnabled(true)`; chrome://inspect over USB). Gate mobile inspectability + desktop developer-extras behind a debug build / setting and record the decision. Register the capability in `docs/platform-capability-matrix.toml`.
>
> Done = a real inspector (console REPL + network) is reachable on desktop (in-window shortcut), iOS (Safari over USB), and Android (chrome://inspect over USB), gated as decided, capability in the parity matrix, with per-platform "how to open it" documented. FIRST re-check the desktop WebView has no developer-extras today and pick a shortcut that avoids the GTK debugger. RECORD the gating decision durably.
