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
    private let trustLabel = UILabel()
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

        // The nav buttons stay at their intrinsic (compact) width: they hug their
        // content tightly and resist being stretched. The URL field, by contrast,
        // hugs weakly and is the first to stretch, so it takes the MAJORITY of the
        // row while the four buttons keep only the width their glyphs need.
        for button in [backButton, forwardButton, reloadButton, stopButton] {
            button.setContentHuggingPriority(.required, for: .horizontal)
            button.setContentCompressionResistancePriority(.required, for: .horizontal)
        }
        urlField.setContentHuggingPriority(.defaultLow, for: .horizontal)
        urlField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        let toolbar = UIStackView(arrangedSubviews: [
            backButton, forwardButton, reloadButton, stopButton, urlField,
        ])
        toolbar.axis = .horizontal
        toolbar.spacing = 8
        toolbar.alignment = .center
        // .fill so the low-hugging URL field absorbs all the spare width (the
        // buttons, at required hugging, stay intrinsic); no button is stretched.
        toolbar.distribution = .fill
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        // Register the native `ipfs` custom-scheme handler on the configuration
        // BEFORE the WKWebView is created (WKWebView refuses a scheme handler set
        // after init). This is the iOS realisation of the mobile `ipfs://`
        // interception: a `WKURLSchemeHandler` for `ipfs` routes each intercepted
        // request through the SHARED werust-core resolve path (the same
        // hash-verified path desktop uses via WebKitGTK `install_ipfs`), so an
        // ENS-resolved `ipfs://<cid>` site renders instead of failing. The `.eth`
        // name stays in the bar (the core's chrome truth); no https/gateway URL is
        // shown. INTERCEPTION MECHANISM (iOS): the NATIVE custom scheme via
        // WKURLSchemeHandler (main-frame-capable since iOS 11). See the recorded
        // decision at
        // work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md.
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(IpfsSchemeHandler(core: core), forURLScheme: "ipfs")
        // Register the internal `werust` custom-scheme handler on the SAME
        // configuration, the twin of the `ipfs` registration above. WKWebView will
        // NOT hand an unregistered custom scheme to any handler, so without this
        // `werust://settings` is unreachable on iOS even though the Rust core
        // registers the `werust` scheme handler (the requeue's Gate-2 fix). This is
        // the SAME mechanism the mobile `ipfs://` interception uses: a
        // WKURLSchemeHandler routes each intercepted request through the SHARED
        // werust-core path (here `apply_settings_request`), so the
        // retrieval-backend selector page renders and a `?backend=...` selection is
        // persisted — at parity with desktop (WebKitGTK `register_uri_scheme`) and
        // Android (`shouldInterceptRequest`).
        configuration.setURLSchemeHandler(WerustSchemeHandler(core: core), forURLScheme: "werust")
        // Wire the EIP-1193 provider bridge: register the provider script-message
        // channel (a `WKScriptMessageHandler` the shared shim posts to at
        // `window.webkit.messageHandlers.werustProvider`) and inject the
        // `werust-core` provider shim as a document-start `WKUserScript`, so a
        // page's `window.ethereum` is the injected native provider — routed through
        // the SAME provider path desktop uses. The handler answers each envelope
        // keylessly and evaluates the response JS back in the page.
        let providerHandler = ProviderBridgeHandler(core: core, webViewRef: { [weak self] in self?.webView })
        configuration.userContentController.add(providerHandler, name: Self.providerChannel)
        let shim = core.documentStartScript()
        if !shim.isEmpty {
            configuration.userContentController.addUserScript(
                WKUserScript(source: shim, injectionTime: .atDocumentStart, forMainFrameOnly: false))
        }
        webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.translatesAutoresizingMaskIntoConstraints = false

        statusLabel.text = "idle"
        statusLabel.font = .systemFont(ofSize: 13)
        statusLabel.textColor = .secondaryLabel
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        // The trust indicator, at the footer next to the status: painted from the
        // core's posture (the ACTUAL load path), the SAME four states desktop shows.
        trustLabel.text = "⚠ unverified origin"
        trustLabel.font = .systemFont(ofSize: 13)
        trustLabel.textColor = .secondaryLabel
        trustLabel.textAlignment = .right
        trustLabel.setContentHuggingPriority(.required, for: .horizontal)
        trustLabel.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbar)
        view.addSubview(webView)
        view.addSubview(statusLabel)
        view.addSubview(trustLabel)

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
            statusLabel.bottomAnchor.constraint(equalTo: g.bottomAnchor, constant: -4),

            trustLabel.centerYAnchor.constraint(equalTo: statusLabel.centerYAnchor),
            trustLabel.leadingAnchor.constraint(
                greaterThanOrEqualTo: statusLabel.trailingAnchor, constant: 8),
            trustLabel.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),
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
        // The trust indicator tracks the core's posture (the real load path),
        // matching desktop; the seam-default no-op is gone.
        trustLabel.text = chrome.trustIndicator()
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

    /// The EIP-1193 provider script-message channel (matches `werust-core`).
    fileprivate static let providerChannel = "werustProvider"
}

/// The `WKScriptMessageHandler` for the EIP-1193 provider channel: the iOS edge
/// that receives a page-posted envelope (page -> native), hands it to the shared
/// `werust-core` provider path, and evaluates the response JS back in the page
/// (native -> page) to settle the page's pending Promise. The shared provider shim
/// posts to `window.webkit.messageHandlers.werustProvider`, which WKWebView routes
/// here because the channel is registered on the `WKUserContentController`. The
/// bridge holds NO keys (a read-only stub), the same posture as desktop.
final class ProviderBridgeHandler: NSObject, WKScriptMessageHandler {
    private let core: WerustCore
    private let webViewRef: () -> WKWebView?

    init(core: WerustCore, webViewRef: @escaping () -> WKWebView?) {
        self.core = core
        self.webViewRef = webViewRef
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        // The shared shim posts the envelope as a JSON string.
        let body = message.body as? String ?? ""
        let response = core.handleProviderMessage(WKWebViewShellController.providerChannel, body)
        guard !response.isEmpty else { return }
        // Push the response back into the live page to settle the pending Promise.
        webViewRef()?.evaluateJavaScript(response, completionHandler: nil)
    }
}

/// The `WKURLSchemeHandler` for `ipfs://`: the iOS edge that intercepts an
/// `ipfs://<cid>[/path]` request the `WKWebView` cannot load itself and answers
/// it from the SHARED `werust-core` resolve path (the same hash-verified path
/// desktop uses). A verified resolution is served as a `URLResponse` + data on
/// the task; a fail-closed resolution error fails the task with a legible reason
/// (never rendering unverified bytes), matching the desktop trust posture where a
/// hash mismatch fails the load.
final class IpfsSchemeHandler: NSObject, WKURLSchemeHandler {
    private let core: WerustCore

    init(core: WerustCore) {
        self.core = core
    }

    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(
                NSError(domain: "werust.ipfs", code: -1, userInfo: nil))
            return
        }
        switch core.resolveIpfs(url.absoluteString) {
        case .some(.success(let mimeType, let body)):
            let response = URLResponse(
                url: url, mimeType: mimeType,
                expectedContentLength: body.count, textEncodingName: "utf-8")
            urlSchemeTask.didReceive(response)
            urlSchemeTask.didReceive(body)
            urlSchemeTask.didFinish()
        case .some(.failure(let reason)):
            // Fail closed: never render unverified bytes; surface the honest reason.
            urlSchemeTask.didFailWithError(
                NSError(
                    domain: "werust.ipfs", code: -1,
                    userInfo: [NSLocalizedDescriptionKey: reason]))
        case nil:
            // Not an intercepted scheme (should not happen for the ipfs handler).
            urlSchemeTask.didFailWithError(
                NSError(domain: "werust.ipfs", code: -2, userInfo: nil))
        }
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {
        // The core resolution is synchronous; nothing to cancel.
    }
}

/// The `WKURLSchemeHandler` for `werust://`: the iOS edge that serves werust's
/// internal pages (today the `werust://settings` retrieval-backend selector) the
/// `WKWebView` cannot load itself, from the SHARED `werust-core` settings path
/// (the same page desktop's WebKitGTK `register_uri_scheme` and Android's
/// `shouldInterceptRequest` serve). A rendered page is served as a `URLResponse` +
/// data on the task; a non-`settings` host fails the task with a legible reason.
/// A `werust://settings?backend=<kind>[&url=...]` selection is applied + persisted
/// by the shared core, so choosing a retrieval backend on iOS switches the actual
/// `ipfs://` load path (on the next session) exactly as on the other platforms.
///
/// This is the requeue's Gate-2 fix: the `werust` scheme was registered on the
/// Rust side but had NO Swift `WKURLSchemeHandler` to dispatch it, so
/// `werust://settings` was dead on iOS. It mirrors [IpfsSchemeHandler] so the two
/// schemes reach the core the same way.
final class WerustSchemeHandler: NSObject, WKURLSchemeHandler {
    private let core: WerustCore

    init(core: WerustCore) {
        self.core = core
    }

    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(
                NSError(domain: "werust.settings", code: -1, userInfo: nil))
            return
        }
        switch core.applySettings(url.absoluteString) {
        case .some(.success(let mimeType, let body)):
            let response = URLResponse(
                url: url, mimeType: mimeType,
                expectedContentLength: body.count, textEncodingName: "utf-8")
            urlSchemeTask.didReceive(response)
            urlSchemeTask.didReceive(body)
            urlSchemeTask.didFinish()
        case .some(.failure(let reason)):
            // Fail closed on a bad internal URL (a non-`settings` host); surface the
            // honest reason rather than rendering nothing silently.
            urlSchemeTask.didFailWithError(
                NSError(
                    domain: "werust.settings", code: -1,
                    userInfo: [NSLocalizedDescriptionKey: reason]))
        case nil:
            // Not the `werust` scheme (should not happen for this handler).
            urlSchemeTask.didFailWithError(
                NSError(domain: "werust.settings", code: -2, userInfo: nil))
        }
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {
        // The core resolution is synchronous; nothing to cancel.
    }
}
