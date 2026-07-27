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

final class WKWebViewShellController: UIViewController, UITextFieldDelegate, WKNavigationDelegate,
    WKUIDelegate
{

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
    /// The small "invalid URL" badge next to the URL bar, shown ONLY when the last
    /// entry was INVALID (a scheme-less garbage entry that did not navigate). Paired
    /// with the URL-bar text rendered invalid (red underline), it surfaces the
    /// distinct invalid-URL state while KEEPING the typed text for the user to fix
    /// (field finding D) — orthogonal to the trust indicator and the error banner.
    private let invalidBadge = UILabel()
    /// The PROMINENT in-view error banner: a high-contrast bar under the toolbar,
    /// shown ONLY on a failed load, carrying the accurate protocol-named reason so
    /// the user cannot miss why nothing rendered (the fail-closed honesty fix —
    /// the footer status was "not easily seen"). Hidden otherwise. The SAME
    /// surfacing desktop/Android show.
    private let errorBanner = UILabel()
    private var webView: WKWebView!
    /// KVO token for observing `webView.url` so a SAME-DOCUMENT URL change (an SPA
    /// `pushState`/`replaceState`) is reported into the core. Held for the
    /// controller's lifetime; released on deinit.
    private var urlObservation: NSKeyValueObservation?

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

        invalidBadge.text = "⛔ invalid URL"
        invalidBadge.font = .systemFont(ofSize: 13)
        invalidBadge.textColor = UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
        invalidBadge.setContentHuggingPriority(.required, for: .horizontal)
        invalidBadge.setContentCompressionResistancePriority(.required, for: .horizontal)
        invalidBadge.isHidden = true

        let toolbar = UIStackView(arrangedSubviews: [
            backButton, forwardButton, reloadButton, stopButton, urlField, invalidBadge,
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
        // SAME-DOCUMENT URL tracking (task
        // track-webview-url-on-spa-clientside-navigation): a SvelteKit SPA link
        // click is a CLIENT-SIDE `pushState`/`replaceState` navigation — the
        // document does NOT reload, so `didCommit`/`didFinish` never fire and the
        // URL bar used to freeze on the pinned `.eth` name. `WKWebView.url` IS
        // KVO-observable and DOES update on such same-document history changes, so
        // observe it and report the new URL as a same-document change (NOT a load):
        // the core follows it (dropping the pin / re-deriving the ENS name) without
        // faking a load lifecycle.
        urlObservation = webView.observe(\.url, options: [.new]) { [weak self] wv, _ in
            guard let self = self else { return }
            self.core.onUrlChanged(wv.url?.absoluteString ?? "")
            self.refreshChrome()
        }
        // Handle NEW-WINDOW requests (a `target="_blank"` link / `window.open`) by
        // navigating IN THE CURRENT view instead of dropping them: werust has no
        // tab/window model yet (task
        // blank-and-window-open-links-navigate-in-place, field finding C,
        // docs/adr/0010). Without a `WKUIDelegate.webView(_:createWebViewWith:...)`
        // WKWebView returns nil for a `_blank` request and the navigation is
        // silently DROPPED. Setting ourselves as the UI delegate lets the hook
        // below load the request into THIS same webView. See
        // webView(_:createWebViewWith:for:windowFeatures:).
        webView.uiDelegate = self
        webView.translatesAutoresizingMaskIntoConstraints = false
        // WEB INSPECTOR (task enable-web-inspector-devtools-all-platforms): make
        // the page inspectable via Safari's Web Inspector (the SAME WebKit devtools
        // — console REPL + network — desktop shows in-window), reached over USB
        // from Safari on a Mac (Develop menu -> the device -> this page). iOS 16.4+
        // exposes `WKWebView.isInspectable`; below that (and on the Simulator,
        // where pages are always inspectable) it is a no-op. GATED on a DEBUG build
        // (#if DEBUG) so a RELEASE build is NOT silently inspectable — the iOS
        // analogue of the desktop `enable-developer-extras` debug gate and
        // Android's `BuildConfig.DEBUG`. See
        // work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md.
        #if DEBUG
            if #available(iOS 16.4, *) {
                webView.isInspectable = true
            }
        #endif
        // COLOR SCHEME follows the OS (task webview-follow-os-color-scheme,
        // docs/adr/0009). WKWebView reports the page's `prefers-color-scheme` from
        // its `UITraitCollection.userInterfaceStyle`, which follows the OS
        // light/dark setting by default. werust does NOT pin it: `Info.plist` sets
        // no `UIUserInterfaceStyle` (which would lock the app to one appearance),
        // and we leave `overrideUserInterfaceStyle == .unspecified` here (do NOT
        // set it to `.dark`/`.light`) so the WebView keeps following the OS and a
        // page's own declared `color-scheme` is respected. Setting it would FORCE a
        // scheme, which this task explicitly rejects. iOS updates the trait
        // collection live on an OS light<->dark toggle, so the follow is automatic.
        webView.overrideUserInterfaceStyle = .unspecified

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

        // The prominent error banner: white-on-red, wrapping (a long protocol-named
        // reason stays legible), hidden until a load fails.
        errorBanner.font = .boldSystemFont(ofSize: 14)
        errorBanner.textColor = .white
        errorBanner.backgroundColor = UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
        errorBanner.numberOfLines = 0
        errorBanner.isHidden = true
        errorBanner.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbar)
        view.addSubview(errorBanner)
        view.addSubview(webView)
        view.addSubview(statusLabel)
        view.addSubview(trustLabel)

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: g.topAnchor, constant: 8),
            toolbar.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            toolbar.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),

            // Directly under the toolbar and ABOVE the web view, so a failed load's
            // reason is unmissable in the content area, not buried in the footer.
            errorBanner.topAnchor.constraint(equalTo: toolbar.bottomAnchor, constant: 8),
            errorBanner.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            errorBanner.trailingAnchor.constraint(equalTo: view.trailingAnchor),

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
        // The INVALID-URL surface (field finding D): when the last entry was
        // invalid (a scheme-less garbage entry that did not navigate) show the small
        // badge and render the URL-bar text as invalid (red underline), keeping the
        // typed text for the user to fix. Toggled from the orthogonal `invalidEntry`
        // fact — distinct from the trust indicator and the load-error banner. The
        // SAME rule desktop/Android apply, from the same chrome fact.
        invalidBadge.isHidden = !chrome.invalidEntryVisible()
        if chrome.invalidEntryVisible() {
            urlField.textColor = UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
            if let text = urlField.text {
                urlField.attributedText = NSAttributedString(
                    string: text,
                    attributes: [
                        .underlineStyle: NSUnderlineStyle.single.rawValue,
                        .underlineColor: UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0),
                        .foregroundColor: UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0),
                    ])
            }
        } else {
            urlField.textColor = .label
            if let text = urlField.text {
                urlField.attributedText = NSAttributedString(string: text)
            }
        }
        backButton.isEnabled = chrome.canGoBack
        forwardButton.isEnabled = chrome.canGoForward
        stopButton.isEnabled = chrome.loading
        reloadButton.isEnabled = !chrome.loading
        statusLabel.text = chrome.statusLine()
        // The trust indicator tracks the core's posture (the real load path),
        // matching desktop; the seam-default no-op is gone.
        trustLabel.text = chrome.trustIndicator()
        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate protocol-named reason across the top of the view so the user
        // cannot miss why nothing rendered (the fail-closed honesty fix). Hidden
        // otherwise. The SAME rule desktop/Android apply, from the same chrome fact.
        errorBanner.isHidden = !chrome.errorBannerVisible()
        errorBanner.text = chrome.errorBanner()
        // A TRANSIENT/timeout failure (retryable) is a softer amber banner; a hard
        // failure is the prominent red one (task
        // `clearer-loading-and-error-indicator`). The SAME distinction desktop
        // shows, from the core's `retryable` fact.
        errorBanner.backgroundColor = chrome.errorIsRetryable()
            ? UIColor(red: 0.71, green: 0.51, blue: 0.04, alpha: 1.0)
            : UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
    }

    // --- user intents -> Rust core (THROUGH the seams) ------------------------
    @objc private func onBack() { core.goBack(); afterCoreAction() }
    @objc private func onForward() { core.goForward(); afterCoreAction() }
    @objc private func onReload() { core.reload(); afterCoreAction() }
    @objc private func onStop() { core.stop(); afterCoreAction() }

    // URL field submit: pass the RAW typed text straight to the core, which is
    // the SINGLE front door that routes it (bare `.eth` -> ENS; a scheme-less
    // valid host -> `https://` prepend; an explicit scheme -> literal; garbage ->
    // the invalid-URL badge, keeping the typed text). The edge must NOT prepend a
    // scheme itself: doing so pre-empted the core's classifier and turned a
    // garbage entry into a doomed `https://garbage` LOAD instead of the honest
    // invalid-URL state (task
    // `scheme-less-entry-https-fallback-and-keep-bar-on-error`). The one shared
    // rule lives in `werust-core`, matching desktop + Android, which also pass the
    // raw text.
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        textField.resignFirstResponder()
        if let raw = textField.text, !raw.isEmpty {
            core.navigate(raw)
            afterCoreAction()
        }
        return true
    }

    // --- WKUIDelegate: new-window (`_blank` / window.open) -> in-place ---------
    // A page asking to open a new window (a `target="_blank"` link or a
    // `window.open(url)` call) has nowhere to go — werust has NO tab/window model
    // yet. The recorded decision (in-place until tabs exist, docs/adr/0010) is to
    // load the requested URL in the CURRENT view instead of dropping it. On a
    // `_blank` request `navigationAction.targetFrame` is nil (there is no existing
    // frame to load into), so we load the request into THIS webView and return nil
    // (create NO new WKWebView, so no second window). WKWebView loads it through
    // its NORMAL path, so an `ipfs://` target still routes to the registered
    // `IpfsSchemeHandler` (hash-verified) and an unsupported scheme is still
    // refused — the hook is a router, not a trust bypass. This mirrors the desktop
    // `create`-signal handler and the Android `onCreateWindow`; the shared
    // in-place rule is `renderer::new_window_action` (pinned by the seam test
    // `a_new_window_request_navigates_the_current_view_in_place`). Manual
    // verification steps: docs/spikes/blank-and-window-open-links-navigate-in-place/README.md.
    func webView(
        _ wv: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        // A nil target frame is the `_blank`/new-window case: load it in place.
        // (A non-nil target frame would be handled by the normal navigation path.)
        if navigationAction.targetFrame == nil {
            wv.load(navigationAction.request)
        }
        // Return nil: create NO new WKWebView, so there is no second window — the
        // navigation happened in the current view above.
        return nil
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
        // `afterCoreAction` (not a bare `refreshChrome`) because driving the core
        // here may produce a PENDING LOAD the WKWebView must perform: a site's
        // `_redirects` 3xx rule (IPIP-0002) is a NAVIGATION the intercepted
        // `WKURLSchemeTask` cannot answer in place, so the core queues the
        // `ipfs://<rootcid><to>` target and its pump turns it into an ordinary
        // pending load. Draining it here is what makes the redirect real — bar +
        // history move, and the target hash-verified by the fresh scheme task it
        // triggers (task `ipfs-redirects-3xx-navigation-support`).
        afterCoreAction()
    }

    func webView(_ wv: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        core.onPageFailed(wv.url?.absoluteString ?? "", error.localizedDescription)
        // A `_redirects` 3xx answers the intercepted task fail-closed (no page
        // renders under the OLD url), so THIS is the signal that follows it: drain
        // the pending load the core's pump queued and perform the redirect.
        afterCoreAction()
    }

    func webView(
        _ wv: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        let failed = (error as NSError).userInfo[NSURLErrorFailingURLStringErrorKey] as? String
        core.onPageFailed(failed ?? wv.url?.absoluteString ?? "", error.localizedDescription)
        afterCoreAction()
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
        case .some(.success(let mimeType, let body, let status)):
            // A NON-OK status WITH a body is the site's own error page, named by
            // its `_redirects` (IPIP-0002) for a path that is not in its DAG: it
            // must RENDER, but as the not-found it honestly is (what a gateway
            // does), so it is answered as an `HTTPURLResponse` carrying that
            // status. A plain `URLResponse` cannot express a status, so the
            // ordinary 200 case keeps using it. The bytes are the same
            // hash-verified bytes either way.
            let response: URLResponse
            if status == 200 {
                response = URLResponse(
                    url: url, mimeType: mimeType,
                    expectedContentLength: body.count, textEncodingName: "utf-8")
            } else {
                response =
                    HTTPURLResponse(
                        url: url, statusCode: status, httpVersion: "HTTP/1.1",
                        headerFields: [
                            "Content-Type": mimeType,
                            "Content-Length": String(body.count),
                        ])
                    ?? URLResponse(
                        url: url, mimeType: mimeType,
                        expectedContentLength: body.count, textEncodingName: "utf-8")
            }
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
        case .some(.success(let mimeType, let body, _)):
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
