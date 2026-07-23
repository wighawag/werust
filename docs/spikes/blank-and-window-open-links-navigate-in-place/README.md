# `target="_blank"` / `window.open` navigate in place: what it does + how to verify

werust has NO tab/window model yet, so a page's new-window request (a `target="_blank"` link or a `window.open(url)` call) navigates IN THE CURRENT view instead of being silently dropped. Each platform's native new-window hook routes the requested URL into the existing view through its NORMAL navigation/scheme path (so an `ipfs://`/ENS `_blank` target is still hash-verified and an unsupported one still refused — the hook is a router, not a trust bypass) and returns no new view. The decision (in-place until tabs exist) is recorded at `docs/adr/0010`; the capability is the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml`. Root cause: field finding C, `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md`.

## What is automatable vs runtime-only

The in-place ROUTING DECISION is a pure, toolkit-free rule — `renderer::new_window_action(target_uri) -> NewWindowAction` — pinned display-free by:

- `crates/renderer/src/lib.rs` — `a_new_window_request_navigates_the_current_view_in_place` and `a_new_window_request_with_no_target_opens_no_window_and_loads_nothing` (the shared decision: a real target navigates in place, verbatim so an `ipfs://` target stays on the verified path; an empty/missing target opens nothing).
- `crates/webview-renderer/src/lib.rs` — `a_new_window_request_navigates_the_existing_view_in_place_no_second_view` (the desktop `create`-handler body: resolve the request, then drive the SAME `navigate` path in place, one view only).

The actual WEBVIEW HOOKS (`connect_create` on desktop, `WKUIDelegate` on iOS, `WebChromeClient.onCreateWindow` on Android) are runtime-only — they fire inside a live GTK/UIKit/Android WebView. Their display-bound wiring is pinned by the `#[ignore]`d `real_webview_installs_the_new_window_in_place_hook` (desktop, run with `cargo test -p webview-renderer -- --ignored` on a desktop session) and by the manual steps below (mobile).

## Manual verification (a `_blank` link + a `window.open`)

Test page (any of): a page with `<a href="https://example.com/" target="_blank">blank link</a>` and a button calling `window.open('https://example.com/')`. `ronan.eth` is the real fixture — its external links are all `target="_blank"`.

### Desktop (WebKitGTK)

1. Run werust (`cargo run -p werust`).
2. Click a `target="_blank"` link (or trigger `window.open(url)`). EXPECT: the CURRENT window navigates to the target — it does NOT do nothing, and NO second window opens.
3. Point a `_blank` link at an `ipfs://<cid>` (or an ENS `.eth` that resolves to IPFS). EXPECT: it loads in place AND the trust indicator shows content-verified / name-via-trusted-RPC (verification intact through the same scheme handler); a `_blank` to an unsupported scheme still fails closed with its reason.

Wiring: `crates/webview-renderer/src/backend.rs` (`WebViewRenderer::install_new_window_in_place`, wired in `crates/werust/src/main.rs`).

### iOS (WKWebView)

1. Build/run the Simulator app.
2. Tap a `target="_blank"` link (or trigger `window.open(url)`). EXPECT: the SAME webView navigates to the target; no second view is presented.
3. Point a `_blank` link at an `ipfs://`/`.eth` target. EXPECT: it loads in place through the registered `ipfs` scheme handler (hash-verified), the `.eth` name stays in the bar; an unsupported target fails closed.

Wiring: `crates/werust-ios/App/Sources/WKWebViewShellController.swift` (`WKUIDelegate.webView(_:createWebViewWith:for:windowFeatures:)` — on `navigationAction.targetFrame == nil`, `webView.load(navigationAction.request)`, return nil; `webView.uiDelegate = self`).

### Android (System WebView)

1. Build/run the debug APK (`./gradlew assembleDebug`, install it).
2. Tap a `target="_blank"` link (or trigger `window.open(url)`). EXPECT: the SAME WebView navigates to the target; no real second window is created.
3. Point a `_blank` link at an `ipfs://`/`.eth` target. EXPECT: it loads in place through `shouldInterceptRequest` (hash-verified); an unsupported target fails closed.

Wiring: `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` (`settings.setSupportMultipleWindows(true)` + `WebChromeClient.onCreateWindow` in `CoreWebChromeClient`, recovering the target URL via a throwaway transport WebView and loading it into the main `webView`).
