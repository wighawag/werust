---
title: "Wire console + network capture on every platform to feed the core debug store (always-on for now)"
slug: debug-console-network-capture-per-platform
spec: in-app-debug-menu-console-and-network
blockedBy: [debug-capture-store-console-and-network-in-core]
covers: [2]
---

## What to build

Feed the bounded capture store from `debug-capture-store-console-and-network-in-core` with REAL console + network events on every platform. Design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`. Always-on capture for now (the config toggle is a later task).

READ-FIRST / drift check: confirm the store + `push_console`/`push_network` exist (the blockedBy task). Confirm today: NO console hook is wired anywhere; network requests are only partly visible - desktop sees only registered schemes (`ipfs://`/`werust://`) via `register_uri_scheme`, https/http go direct; Android's `shouldInterceptRequest` sees EVERY request (returns null to pass through); iOS `WKURLSchemeHandler` sees only custom schemes.

### Console capture (all 3 platforms)
- **Desktop (WebKitGTK)**: wire the WebView's console-message signal (webkit6 `WebView` "console-message" / the user-content-manager console) -> build a `ConsoleEntry` -> `push_console` through the core (via the shell). `crates/webview-renderer/src/backend.rs`.
- **Android**: `WebChromeClient.onConsoleMessage(ConsoleMessage)` -> map level/message/source/line -> push through the FFI to the core. `crates/werust-android/.../BrowserActivity.kt` + the rust FFI.
- **iOS (WKWebView)**: WKWebView has NO native console callback, so inject a small `WKUserContentController` user-script that wraps `console.log/info/warn/error/debug` to `postMessage` over a message handler; the handler builds a `ConsoleEntry` -> push through the FFI. `crates/werust-ios/...`.

### Network capture (per platform's reach)
- **Desktop**: wire a per-resource load capture - WebKitGTK `WebView` resource-load signals (`resource-load-started` -> the `WebResource`'s `sent-request` / `finished` / `failed`, or `WebView`'s resource signals) -> a `NetworkEntry` (method/url/status/mime/size/from-cache/scheme) -> `push_network`. This captures https/http too, which the scheme handler does not see. Every `ipfs://` request the scheme handler already sees is captured with its verified trust; https subresources are captured as unverified-origin.
- **Android**: `shouldInterceptRequest` ALREADY sees every request - record a `NetworkEntry` there for BOTH the intercepted (`ipfs://`, with its status/mime + verified trust) and the passed-through (return-null) requests, before returning. `crates/werust-android/.../BrowserActivity.kt`.
- **iOS**: WKWebView does not cheaply expose all subresource requests. Capture what is reachable: the `WKURLSchemeHandler` custom-scheme requests (ipfs://, with verified trust), the main-frame navigations via `WKNavigationDelegate`, AND a best-effort JS hook - a `WKUserContentController` user-script wrapping `fetch`/`XMLHttpRequest` to `postMessage` request+response metadata (method/url/status) - the pragmatic route (dovetails with the console user-script). Record iOS's coverage limits honestly (it may miss some browser-internal subresource loads); this is acceptable and improves later.

Every `NetworkEntry` carries the HONEST per-request trust posture (ADR-0006) the store type defines. Capture is off the UI thread where the platform's event already is (do not reintroduce a UI-thread block - respect the Android ANR fix). Feed the ONE shared store; the debug VIEW (separate task) renders it.

## Acceptance criteria

- [ ] Console messages from the page (`console.log/info/warn/error/debug`) are captured into the core store on desktop, Android, and iOS (iOS via the injected console user-script), with level/message/source/line.
- [ ] Network requests are captured into the core store: desktop via the resource-load signals (incl. https/http, not just ipfs://), Android via `shouldInterceptRequest` (all requests), iOS via the reachable points (custom-scheme + main-frame + a best-effort fetch/XHR user-script), each with method/url/status/mime and the honest per-request trust posture.
- [ ] Capture is always-on for now, feeds the bounded store (oldest-evicted), and does NOT block the UI thread (Android ANR fix respected; capture runs where the platform event already runs).
- [ ] iOS network-capture coverage limits are recorded honestly (what it can/can't see); this is accepted, not a blocker.
- [ ] Verification/trust unchanged: capturing is READ-ONLY observation; it does not alter the load path, the ipfs:// verification, or the trust posture (it REPORTS the posture, per entry).
- [ ] Tracked per the parity guard. Tests cover the mapping from a platform console/network event to a core entry where testable (the FFI push + the entry mapping are unit-testable; the live platform hooks get the strongest automatable guard + recorded manual steps). Network-isolated.

## Blocked by

- `debug-capture-store-console-and-network-in-core` (the store + push_console/push_network + entry types this task feeds).

## Prompt

> Goal: feed the core debug capture store (from the blockedBy task) with REAL console + network events on all 3 platforms, always-on for now. Console: desktop WebKitGTK console-message signal, Android `WebChromeClient.onConsoleMessage`, iOS an injected `WKUserContentController` user-script wrapping console.* over a message handler (WKWebView has no native console callback). Network: desktop the WebView resource-load signals (captures https too, which the scheme handler misses), Android `shouldInterceptRequest` (already sees every request - record there for both intercepted + passed-through), iOS the reachable points (custom-scheme + main-frame nav + a best-effort fetch/XHR user-script; record iOS coverage limits honestly).
>
> Where to look: `crates/webview-renderer/src/backend.rs` (register_uri_scheme is there; add console + resource-load signals), `crates/werust-android/.../BrowserActivity.kt` (onConsoleMessage + shouldInterceptRequest) + its rust FFI, `crates/werust-ios/...` (user-scripts + scheme handler + nav delegate) + its FFI. Each NetworkEntry carries the honest per-request trust posture (ADR-0006). Capture runs where the platform event already runs (off the UI thread; respect the Android ANR fix); it is READ-ONLY (does not alter the load path / verification / posture). Feed the ONE shared store; the debug view renders it (separate task).
>
> Done = console + network captured into the core store on all 3 (iOS limits recorded), always-on, bounded, no UI-thread block, verification/trust unchanged, parity-tracked, unit-tested where testable + recorded manual steps. FIRST re-check the store/push_* exist and no console hook is wired yet.
