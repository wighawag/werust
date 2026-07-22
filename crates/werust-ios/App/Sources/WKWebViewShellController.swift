// WKWebViewShellController — the iOS OS edge: a real `UIViewController` with a
// URL bar and Back/Forward/Reload/Stop controls over a live, interactive
// `WKWebView` (task mobile-ios-shell-and-static-lib, spec story 18). The twin of
// the Android `BrowserActivity`.
//
// This is the forced OS edge and NOTHING more: it owns the platform `WKWebView`
// and the UIKit widgets, but every browsing DECISION is the Rust `WerustCore`'s.
// On a user action it drives the core, then (1) applies whatever URL the core
// surfaces to the `WKWebView` (`syncPendingLoad`) and (2) repaints its chrome
// from the core's `WerustCore.Chrome` (`refreshChrome`). The `WKWebView`'s real
// load-lifecycle callbacks are reported straight back into the core, which folds
// them into the chrome exactly as the desktop GTK pump folds WebKitGTK's signals.
// The URL bar text, the Back/Forward enablement, and the load status are all read
// from the core — the edge keeps no history or load state of its own.
//
// Simulator only: no signing, no Apple Developer account.

import UIKit
import WebKit

final class WKWebViewShellController: UIViewController, UITextFieldDelegate, WKNavigationDelegate {

    // The Rust core: all browsing logic (URL bar, history, load lifecycle, chrome)
    // lives behind this. The controller holds no browsing state of its own.
    private let core = WerustCore()

    // Chrome widgets (the UIKit side; painted from the core's Chrome).
    private let urlField = UITextField()
    private let backButton = UIButton(type: .system)
    private let forwardButton = UIButton(type: .system)
    private let reloadButton = UIButton(type: .system)
    private let stopButton = UIButton(type: .system)
    private let statusLabel = UILabel()
    private var webView: WKWebView!

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        // The greeting build-and-run.sh asserts reached the device log: proof the
        // Rust core is linked + callable from the launched app.
        let probe = WerustCore()
        NSLog("werust: linked Rust core (chrome=\(probe.chrome().loadState))")

        layoutChrome()

        // Launch a browsing surface: drive the core to the start URL, then let the
        // core surface it onto the WKWebView.
        core.navigate(Self.startURL)
        afterCoreAction()
    }

    // --- layout: URL field + back/forward toolbar + content webview -----------
    private func layoutChrome() {
        urlField.borderStyle = .roundedRect
        urlField.placeholder = "Enter a URL and press Go"
        urlField.autocapitalizationType = .none
        urlField.autocorrectionType = .no
        urlField.keyboardType = .URL
        urlField.clearButtonMode = .whileEditing
        urlField.returnKeyType = .go
        urlField.delegate = self

        backButton.setTitle("◀︎", for: .normal)
        forwardButton.setTitle("▶︎", for: .normal)
        reloadButton.setTitle("⟳", for: .normal)
        stopButton.setTitle("✕", for: .normal)
        backButton.addTarget(self, action: #selector(onBack), for: .touchUpInside)
        forwardButton.addTarget(self, action: #selector(onForward), for: .touchUpInside)
        reloadButton.addTarget(self, action: #selector(onReload), for: .touchUpInside)
        stopButton.addTarget(self, action: #selector(onStop), for: .touchUpInside)

        let toolbar = UIStackView(arrangedSubviews: [
            backButton, forwardButton, reloadButton, stopButton, urlField,
        ])
        toolbar.axis = .horizontal
        toolbar.spacing = 8
        toolbar.alignment = .center
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        webView = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        webView.navigationDelegate = self
        webView.translatesAutoresizingMaskIntoConstraints = false

        statusLabel.text = "idle"
        statusLabel.font = .systemFont(ofSize: 13)
        statusLabel.textColor = .secondaryLabel
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbar)
        view.addSubview(webView)
        view.addSubview(statusLabel)

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: g.topAnchor, constant: 8),
            toolbar.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            toolbar.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),

            webView.topAnchor.constraint(equalTo: toolbar.bottomAnchor, constant: 8),
            webView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: view.trailingAnchor),

            statusLabel.topAnchor.constraint(equalTo: webView.bottomAnchor, constant: 4),
            statusLabel.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            statusLabel.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),
            statusLabel.bottomAnchor.constraint(equalTo: g.bottomAnchor, constant: -4),
        ])
    }

    // --- after driving the core: apply any pending load + repaint -------------
    private func afterCoreAction() {
        syncPendingLoad()
        refreshChrome()
    }

    /// Apply the URL the core surfaced (if any) to the platform WKWebView.
    private func syncPendingLoad() {
        if let pending = core.takePendingLoad(), let url = URL(string: pending) {
            webView.load(URLRequest(url: url))
        }
    }

    /// Repaint the chrome from the core's truth (never the edge's own state).
    private func refreshChrome() {
        let chrome = core.chrome()
        if !urlField.isEditing, urlField.text != chrome.url { urlField.text = chrome.url }
        backButton.isEnabled = chrome.canGoBack
        forwardButton.isEnabled = chrome.canGoForward
        stopButton.isEnabled = chrome.loading
        reloadButton.isEnabled = !chrome.loading
        statusLabel.text = chrome.statusLine()
    }

    // --- user intents -> Rust core (THROUGH the seams) ------------------------
    @objc private func onBack() { core.goBack(); afterCoreAction() }
    @objc private func onForward() { core.goForward(); afterCoreAction() }
    @objc private func onReload() { core.reload(); afterCoreAction() }
    @objc private func onStop() { core.stop(); afterCoreAction() }

    // URL field submit: normalise a bare host into an https URL, then navigate
    // THROUGH the core (which validates + starts the load behind the seam).
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        textField.resignFirstResponder()
        if let raw = textField.text, !raw.isEmpty {
            core.navigate(Self.normalizeURL(raw))
            afterCoreAction()
        }
        return true
    }

    static func normalizeURL(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.contains("://") { return trimmed }
        return "https://" + trimmed
    }

    // --- WKNavigationDelegate -> Rust core ------------------------------------
    // Report the platform WKWebView's real load-lifecycle signals straight back
    // into the core, then repaint the chrome from the core.
    func webView(_ wv: WKWebView, didCommit navigation: WKNavigation!) {
        core.onPageCommitted(wv.url?.absoluteString ?? "")
        refreshChrome()
    }

    func webView(_ wv: WKWebView, didFinish navigation: WKNavigation!) {
        core.onPageFinished(wv.url?.absoluteString ?? "")
        refreshChrome()
    }

    func webView(_ wv: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        core.onPageFailed(wv.url?.absoluteString ?? "", error.localizedDescription)
        refreshChrome()
    }

    func webView(
        _ wv: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        let failed = (error as NSError).userInfo[NSURLErrorFailingURLStringErrorKey] as? String
        core.onPageFailed(failed ?? wv.url?.absoluteString ?? "", error.localizedDescription)
        refreshChrome()
    }

    /// The URL the app opens on launch, so it shows a browsing surface.
    private static let startURL = "https://example.com/"
}
