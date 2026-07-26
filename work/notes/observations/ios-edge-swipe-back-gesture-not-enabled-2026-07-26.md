# iOS: the WKWebView edge-swipe back/forward gesture is not enabled (2026-07-26)

Noticed while wiring the Android system-Back handler (task `android-hardware-back-button-navigates-history`): `crates/werust-ios/App/Sources/WKWebViewShellController.swift` never sets `webView.allowsBackForwardNavigationGestures`, which defaults to `false` — so on iOS the platform's usual edge-swipe back/forward gesture does nothing, and history navigation is only reachable through the on-screen `◀`/`▶` buttons.

Not fixed here (out of this task's Android-only scope, and it is a distinct affordance from Android's system Back, with its own enablement + trust/lifecycle questions). Recorded because the new `system-back-navigates-history` capability row in `docs/platform-capability-matrix.toml` marks iOS `n-a` and points at this note as the place the iOS analogue lives.
