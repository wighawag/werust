---
title: "Mobile ipfs:// interception on iOS (WKURLSchemeHandler) and Android (shouldInterceptRequest), routed through the werust-core resolve path"
slug: mobile-ipfs-scheme-interception-ios-and-android
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
needsAnswers: true
---

## Settled decisions (from the design discussion — DECIDED, build to them)

1. **Interception mechanism = native custom scheme first, internal-https fallback if the platform refuses it, verified per platform AT BUILD TIME.** Prefer the native custom-scheme hook (`WebViewClient.shouldInterceptRequest` on Android; `WKURLSchemeHandler` for `ipfs` on iOS). Both have known edge cases for a TOP-LEVEL (main-frame) custom-scheme navigation (Android especially may not fire `shouldInterceptRequest` for a top-level `ipfs://`; iOS `WKURLSchemeHandler` works since iOS 11 but has main-frame caveats). So the builder VERIFIES which actually works on each platform during the build and, if the native scheme is refused for a top-level navigation, falls back to an internal `https://appassets…`/`WebViewAssetLoader`-style origin that the core resolves as `ipfs://` while the address bar still shows the `.eth` name. Record which mechanism each platform ended up using. This is a build-time engineering verification, not an open product question.
2. **Both platforms stay consistent** (same mechanism choice where possible) and route through the SAME `werust-core` resolve path desktop uses — no forked mobile resolver. The mobile Rust backend's `register_scheme_handler` no-op must become real (or the OS edge intercepts and calls the core); a silent no-op is not acceptable (this is exactly what the parity guard forbids).

## What to build

Make a resolved `ipfs://<cid>` actually load on BOTH mobile edges instead of dying with `net::ERR_UNKNOWN_URL_SCHEME`. Today `ipfs://` is intercepted only on desktop (WebKitGTK `register_uri_scheme`): the Android Rust backend's `register_scheme_handler` is an empty no-op and the Kotlin `WebViewClient` has no `shouldInterceptRequest`; iOS has no `WKURLSchemeHandler`. So on mobile the ENS front door resolves the name fine, then hands the WebView an `ipfs://` URL it cannot load.

Wire real `ipfs://` interception on iOS (WKWebView via `WKURLSchemeHandler`) and Android (System WebView via `WebViewClient.shouldInterceptRequest`, or the internal-https mapping per the settled decisions), routing the request into the SAME `werust-core` resolve path the desktop `install_ipfs` uses, so the SAME content resolution, trust posture, and fail-closed behaviour apply on all three platforms. The mobile Rust backend's `register_scheme_handler` must stop being a silent no-op — either it becomes real, or the OS-edge (Kotlin/Swift) does the interception and calls back into the core, but EITHER way the capability is implemented, not stubbed.

This task reuses whatever `ipfs://` resolution + trust semantics land on desktop from `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (single-block verified, and multi-block UnixFS DAGs verified via CAR and rendered as legitimately `ContentVerified` / `NameViaTrustedRpc` via ENS — NOT served-unverified): mobile must call that SAME core path, not fork its own. (It is not `blockedBy` that task — mobile interception can be built independently — but it must not reimplement or diverge the resolution/trust logic.)

## Acceptance criteria

- [ ] A resolved `ipfs://<cid>` loads and renders on Android (System WebView) — no `net::ERR_UNKNOWN_URL_SCHEME`.
- [ ] A resolved `ipfs://<cid>` loads and renders on iOS (WKWebView).
- [ ] Both route through the SAME `werust-core` resolve path as desktop (no forked mobile resolver); the resulting trust posture and fail-closed reasons match desktop for the same input.
- [ ] The mobile Rust backend's `register_scheme_handler` is no longer a silent no-op (it is implemented, or the OS edge intercepts and calls the core — not a stub that discards the handler).
- [ ] The `.eth` name stays in the address bar while the underlying `ipfs://<cid>` loads (parity with desktop), with no `https://`/gateway rewrite shown to the user (even if an internal-https mapping is used under the hood).
- [ ] Fail-closed parity: an unsupported/unverifiable/failed load shows the same honest chrome reason on mobile as on desktop.
- [ ] Tests prove `ipfs://` is intercepted and reaches the core on each mobile edge (as far as each platform's harness allows — at minimum a Rust-side test that the mobile backend now routes the scheme to the core, plus whatever edge-level assertion the Android/iOS harness supports), network-isolated. This closes the gap where no test drove `ipfs://` through a real mobile WebView.

## Blocked by

- None to START, the design forks are settled above. Do not autonomously build until the flag is cleared.

## Prompt

> Goal: make `ipfs://<cid>` actually load on iOS (WKWebView) and Android (System WebView), routed into the SAME `werust-core` resolve path desktop uses, so ENS-resolved sites render on mobile instead of `net::ERR_UNKNOWN_URL_SCHEME`. The ENS resolution already works on mobile; only the `ipfs://` render interception is missing.
>
> Domain vocabulary: desktop intercepts `ipfs://` via WebKitGTK `register_uri_scheme` (`install_ipfs`), serving bytes from `werust_core::ipfs::resolve_ipfs_request`. Mobile has no equivalent: Android `WebViewClient` needs `shouldInterceptRequest` (or a `WebViewAssetLoader`/internal-`https://appassets` mapping if a top-level custom scheme is blocked); iOS/WKWebView needs a `WKURLSchemeHandler` for `ipfs`. The Rust core is shared across all edges (`werust-core`), reached from Kotlin (`crates/werust-android`) and Swift (`crates/werust-ios`) over the FFI.
>
> Where to look: `crates/werust-android` (Rust `backend.rs` — `register_scheme_handler` is an empty no-op today; Kotlin `BrowserActivity.kt` — `CoreWebViewClient` has no `shouldInterceptRequest`, just `webView.loadUrl`); `crates/werust-ios` (no scheme handler). The desktop precedent is `install_ipfs` in the webview backend. Reuse the desktop `ipfs://` resolution semantics landing in `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (single-block verified; multi-block UnixFS DAGs verified via CAR -> `ContentVerified`, or `NameViaTrustedRpc` via ENS) — call the SAME core path, do not reimplement resolution or trust logic per platform.
>
> The no-op is the root cause to eliminate: a seam method (`register_scheme_handler`) was silently stubbed on the mobile backend and nothing caught it, so a capability shipped desktop-only. Make it real (or make the OS edge intercept and call the core), and record the mobile interception mechanism you chose per platform.
>
> Done = `ipfs://` loads and renders on both mobile platforms through the shared core path with desktop-parity trust posture and fail-closed reasons, the `.eth` name stays in the bar, the mobile scheme no-op is gone, and tests prove the scheme reaches the core on each edge. FIRST re-check current reality (the desktop `ipfs://` semantics that landed, the mobile edges) and route to needs-attention on drift. RECORD the per-platform interception mechanism (native custom scheme vs internal-https mapping) durably per the task template.
