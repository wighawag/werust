# `target="_blank"` / `window.open` navigate IN THE CURRENT view until a tab/window model exists

A page's new-window request (a `target="_blank"` link or a `window.open(url)` call) navigates IN THE CURRENT view instead of opening a second window or being silently dropped, because werust has NO tab/window model yet. Each platform's native new-window hook routes the requested URL into the existing view through its NORMAL navigation/scheme path (so an `ipfs://`/ENS target is still hash-verified and an unsupported one still refused), and returns no new view. This is a deliberate, REVERSIBLE stand-in: once werust grows a real tab/window model, this decision should be revisited so a `_blank` link can open a new tab as a user expects.

## Status

accepted

## Context

Field test v0.2.4 (finding C, `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md`): on `ronan.eth` (and elsewhere) `target="_blank"` links did NOTHING. Root cause confirmed in code: the desktop backend wired `connect_load_changed` / `connect_load_failed` but NO `connect_create` (WebKitGTK's `create` signal, fired for a `_blank` / `window.open` / new-window request), so the request was unhandled and the navigation DROPPED. The mobile edges were the same: no iOS `WKUIDelegate.webView(_:createWebViewWith:...)`, no Android `WebChromeClient.onCreateWindow` + `setSupportMultipleWindows`. Because every external link on `ronan.eth` is `target="_blank"`, they all did nothing (and the frozen-URL-bar symptom the human also reported was a consequence of this drop, not a separate URL-tracking bug).

werust has no tab or window concept: there is a single live view per shell. So a new-window request has nowhere legitimate to go. The two honest options are (a) drop it (the status quo, a dead link) or (b) load it in the current view (act like a non-`_blank` link).

## Decision

Handle the new-window request on every platform and navigate IN THE CURRENT view. The in-place decision is ONE shared, toolkit-free rule, `renderer::new_window_action(target_uri) -> NewWindowAction`, mirroring how `TrustPosture::after_verify` is the single shared trust rule: a non-empty target resolves to `NavigateInPlace { url }`, a missing/empty target to `Ignore` (open nothing). Each platform's hook applies that rule and feeds a `NavigateInPlace` url into its NORMAL load path, then returns no new view:

- **Desktop (WebKitGTK):** `WebViewRenderer::install_new_window_in_place` wires the `create` signal, reads the navigation action's target URI, loads it into the existing `self.view` via `load_uri`, and returns the EXISTING view widget so WebKitGTK creates no second WebView.
- **iOS (WKWebView):** `WKWebViewShellController` conforms to `WKUIDelegate` and implements `webView(_:createWebViewWith:for:windowFeatures:)` — on a nil target frame (a `_blank`) it calls `webView.load(navigationAction.request)` on the main view and returns nil.
- **Android (System WebView):** `BrowserActivity` sets `settings.setSupportMultipleWindows(true)` and a `WebChromeClient.onCreateWindow` that recovers the target URL (via a throwaway transport WebView) and loads it into the SAME WebView, creating no real second window.

Trust is preserved, NOT bypassed: the hook is a router, not a new trust boundary. Because the in-place load goes through each platform's normal load path, an `ipfs://`/ENS `_blank` target still routes through the hash-verified custom-scheme handler / ENS front door, and an unsupported scheme is still refused — exactly as if the user had navigated to it in view.

The capability is registered in `docs/platform-capability-matrix.toml` (`blank-window-open-navigates-in-place`, implemented on all three contexts). The shared rule is pinned display-free by seam tests in `crates/renderer` and `crates/webview-renderer`; the runtime-only mobile hooks carry recorded manual verification steps at `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md`.

## Consequences

- A `_blank` link REPLACES the current page rather than opening a new tab, which is not the desktop-browser norm. This is an accepted temporary trade-off (a working in-place navigation is strictly better than a dead link) and is explicitly flagged as REVISIT-WHEN-TABS-EXIST: when werust gains a tab/window model, `new_window_action` and the three hooks are the single place to change so `_blank` opens a tab.
- `window.open` with no URL opens nothing (resolves to `Ignore`), matching the "no dead second window" intent.
