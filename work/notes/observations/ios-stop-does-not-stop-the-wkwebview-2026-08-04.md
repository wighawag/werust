# iOS: the Stop button never stops the WKWebView (and a cancelled load would flash a banner) (2026-08-04)

Noticed while wiring the edge-swipe gesture (task `enable-the-ios-back-forward-swipe-gesture`): `WKWebViewShellController.onStop()` drives `core.stop()` (which only settles the core's own load state) and never calls `webView.stopLoading()`, so the platform load keeps running — where the Android edge does call `v.stopLoading()` (`BrowserActivity.kt`).

The two halves are linked, which is why they are one note: the moment `stopLoading()` IS added, WebKit will answer the cancelled navigation with `NSURLErrorCancelled` (-999) on `didFailProvisionalNavigation`, which this edge reports straight into `core.onPageFailed` — so Stop would then flash a red error banner. The macOS backend already has exactly this rule (`navigation_failure` in `crates/macos-renderer/src/pure.rs` drops -999 and `WKErrorFrameLoadInterruptedByPolicyChange` 102); the Swift edge has no equivalent filter.

Unverified: no Mac/simulator on this project, so this is read off the code, not observed.
