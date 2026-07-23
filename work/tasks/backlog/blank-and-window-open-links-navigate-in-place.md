---
title: "target=_blank / window.open links must navigate IN THE SAME view (until tabs exist), not be silently dropped"
slug: blank-and-window-open-links-navigate-in-place
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

FIELD FINDING (v0.2.4, human): "target=_blank links do nothing and since we currently do not have tabs or windows, we should make them act like non-blank one." Root-cause source: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` (finding C).

READ-FIRST / drift check: confirm the mechanism. The desktop backend (`crates/webview-renderer/src/backend.rs`) wires `connect_load_changed` / `connect_load_failed` but has NO `connect_create` (WebKitGTK's `create` signal, fired for a `_blank` / `window.open` / new-window request), so such a request is unhandled and the navigation is DROPPED. Mobile is the same: no `WebChromeClient.onCreateWindow` + `setSupportMultipleWindows` (Android), no `WKUIDelegate.webView(_:createWebViewWith:...)` (iOS). Confirm none of these handlers exist yet.

Fix: werust has NO tab/window model yet, so make a `_blank` / `window.open` / new-window request load IN THE CURRENT view instead of being dropped. Per platform, using each webview's native new-window hook:
- **Desktop (WebKitGTK)**: handle the `create` signal. The `create` handler receives a `NavigationAction` with the target URI; rather than returning a new `WebView`, load that URI into the EXISTING view (`self.view.load_uri(uri)` through the seam) and return the existing view / signal that no new view is created, so the navigation happens in place. (Follow the webkit6 Rust API for `connect_create`; the target URI comes from the navigation action's request.)
- **iOS (WKWebView)**: implement `WKUIDelegate.webView(_:createWebViewWith:for:windowFeatures:)` - when `navigationAction.targetFrame == nil` (a `_blank`), call `webView.load(navigationAction.request)` on the MAIN view and return `nil` (no new view). (`crates/werust-ios`.)
- **Android (System WebView)**: `WebChromeClient.onCreateWindow` with `setSupportMultipleWindows(true)` (or keep it false and rely on `shouldOverrideUrlLoading`): route the target URL back into the SAME WebView (the message-object href, or `shouldOverrideUrlLoading` loading the URL in place) and return false / do not create a real new window. (`crates/werust-android/app/.../BrowserActivity.kt`.)

Record the decision (in-place navigation for `_blank`/`window.open` until a tab/window model exists) durably (`docs/spikes/<slug>/` or an observation), so a future tabs feature can revisit it. Keep trust/verification/lifecycle unchanged: a `_blank` link to an `ipfs://`/ENS/`https://` target routes through the SAME front door / scheme handling as a normal in-view navigation (so an `ipfs://` `_blank` is still hash-verified, an unsupported scheme still refused).

## Acceptance criteria

- [ ] A `target="_blank"` link (and a `window.open(url)`) loads the target URL IN THE CURRENT view on desktop, iOS, and Android - it is no longer silently dropped.
- [ ] The in-place navigation goes through the normal navigation/scheme path, so an `ipfs://`/ENS target is still hash-verified and an unsupported target is still refused (no trust bypass via the new-window hook).
- [ ] No real second window/tab is spawned (werust has no tab model); the decision (in-place until tabs exist) is recorded durably.
- [ ] Applied on all three platforms via each webview's native new-window hook, or tracked per the parity guard; the capability is registered in `docs/platform-capability-matrix.toml` if it is a parity surface.
- [ ] Tests cover the behaviour at the layer it lives (where a `_blank`/create request is routed in-place is assertable - e.g. a seam-level test that a new-window request navigates the existing view; the platform hooks where runtime-only get the strongest automatable guard + recorded manual steps). Network-isolated.

## Blocked by

- None. (Independent; touches the per-platform backends + possibly the shared navigation path.)

## Prompt

> Goal: make `target="_blank"` / `window.open` links navigate IN THE CURRENT view instead of doing nothing (werust has no tabs yet). Today no platform handles the webview's new-window request, so `_blank` links are dropped.
>
> Where to look: desktop `crates/webview-renderer/src/backend.rs` (wire WebKitGTK's `create` signal - load the navigation action's target URI into the existing `self.view` in place, return no new view); iOS `crates/werust-ios` (`WKUIDelegate.webView(_:createWebViewWith:...)` - on a nil target frame, `webView.load(request)` on the main view, return nil); Android `crates/werust-android/.../BrowserActivity.kt` (`WebChromeClient.onCreateWindow` / `shouldOverrideUrlLoading` - route the target URL into the SAME WebView). Route the in-place load through the NORMAL navigation/scheme path so an `ipfs://`/ENS `_blank` target is still hash-verified and an unsupported one refused (no trust bypass).
>
> Record the decision (in-place until a tab/window model exists) durably. Done = a `_blank`/`window.open` link loads in the current view on all 3 platforms, no second window, verification intact, capability registered, tests at the layer it lives (seam-level assertion + recorded manual steps for runtime-only hooks). FIRST re-check no create/onCreateWindow/UIDelegate handler exists yet.
