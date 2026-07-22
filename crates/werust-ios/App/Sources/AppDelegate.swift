// AppDelegate — the werust iOS shell app entry point
// (task mobile-ios-shell-and-static-lib, spec story 18). A minimal
// UIApplicationDelegate whose root view controller is the
// `WKWebViewShellController` (the URL-field + back/forward chrome over the werust
// Rust core). The window + controller are retained here for the app's lifetime,
// so a background->foreground round-trip keeps the same live WKWebView (the
// native webview persists its own page/scroll/history).
//
// Swift is confined to the OS edge: it owns the UIKit window/controller and the
// WKWebView, but every browsing DECISION is the Rust core's (see WerustCore).
//
// Simulator only: no signing, no Apple Developer account.

import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    // Retained for the app's lifetime so the hosted WKWebView survives
    // background/foreground (host-only page-state restoration).
    private var shellController: WKWebViewShellController?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let controller = WKWebViewShellController()
        shellController = controller
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}
