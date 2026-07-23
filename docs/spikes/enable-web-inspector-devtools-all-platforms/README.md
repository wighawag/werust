# Web inspector (devtools) on every platform: how to open it

werust enables each platform's OWN full web inspector (WebKit/Chrome devtools: a typeable console REPL + a network view + DOM/sources), not a custom werust window. This is the "desktop-browser-like console + network everywhere" the human asked for. It is a DEVELOPER surface, so it is gated on a DEBUG build on every platform (a release build is not silently inspectable). The gating + shortcut decisions are recorded at `work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`. The capability is registered as the `web-inspector` row in `docs/platform-capability-matrix.toml`.

Note (correcting an earlier belief): the `shift+F12` that was reachable before this task showed the GTK INTERACTIVE debugger (the GTK widget tree / CSS), NOT web content. GTK4 binds its interactive debugger to Ctrl+Shift+I and Ctrl+Shift+D. This task adds the real WEB inspector, on a shortcut that does not collide with the GTK debugger.

## Desktop (WebKitGTK) — in-window, press F12

1. Run a debug build of werust (`cargo run -p werust`, or any non-`--release` build). Developer-extras is enabled only in a debug build (`developer_extras_enabled()` is `cfg!(debug_assertions)`), so a release binary is not inspectable.
2. Press **F12** in the werust window. The WebKitGTK Web Inspector opens over the page: a real console with a JS REPL you can type into and evaluate, a Network tab, and DOM/Sources.
3. F12 is deliberately NOT the GTK interactive debugger. That separate GTK widget/CSS surface stays on its own keys (Ctrl+Shift+I / Ctrl+Shift+D) and is untouched.

Wiring: `crates/webview-renderer/src/backend.rs` (`WebViewRenderer::new` sets `WebKitSettings.enable-developer-extras`; `WebViewRenderer::show_inspector` calls `WebInspector::show`) and `crates/werust/src/main.rs` (the F12 key controller via `should_open_web_inspector`).

## iOS (WKWebView) — Safari Web Inspector over USB

1. Build/run a DEBUG build (the Simulator build is DEBUG; pages on the Simulator are always inspectable). `webView.isInspectable = true` is set on iOS 16.4+ under `#if DEBUG`.
2. On the Mac, enable Safari's Develop menu: Safari > Settings > Advanced > "Show features for web developers".
3. For a real device, connect it over USB and trust the Mac; on the Simulator no cable is needed.
4. In Safari, open the **Develop** menu, choose the device (or Simulator), and pick the werust page. Safari's Web Inspector opens: the SAME WebKit devtools (console REPL + network) as desktop.

Wiring: `crates/werust-ios/App/Sources/WKWebViewShellController.swift` (`webView.isInspectable = true`, gated `#if DEBUG` + `if #available(iOS 16.4, *)`).

## Android (System WebView) — chrome://inspect over USB

1. Build/run the DEBUG APK (`./gradlew assembleDebug` / install it). `WebView.setWebContentsDebuggingEnabled(true)` is called only when the app is debuggable (`ApplicationInfo.FLAG_DEBUGGABLE`), so a future release APK is not inspectable.
2. On the device, enable Developer options and turn on **USB debugging**, then connect it over USB and accept the debugging prompt.
3. On the desktop, open Chrome and navigate to **`chrome://inspect`**. The werust WebView appears under "Remote Target"; click **inspect**. Chrome DevTools opens: a typeable console REPL and a Network tab.

Wiring: `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` (`WebView.setWebContentsDebuggingEnabled(true)`, gated on `ApplicationInfo.FLAG_DEBUGGABLE`).
