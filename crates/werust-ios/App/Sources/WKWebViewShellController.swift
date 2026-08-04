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
    /// The GENERAL browser menu affordance: the ⋮ button every browser has, at the
    /// end of the toolbar, presenting a `UIMenu` of the SHARED core's menu items
    /// (task `general-browser-menu-with-version-and-debug-entry`).
    ///
    /// USER-FACING and always available — deliberately NOT debug-build-gated (the
    /// Safari `isInspectable` inspector below is; this menu is not). It is a
    /// CONTAINER meant to GROW: ``browserMenu()`` maps whatever items the core
    /// lists, so a future bookmarks/settings entry is a `werust-core` change plus
    /// (only if it is an action) one branch in ``onBrowserMenuItem(_:)``.
    private let menuButton = UIButton(type: .system)
    private let statusLabel = UILabel()
    private let trustLabel = UILabel()
    /// The trust surface's TRUST-ON-FIRST-USE lines and action, cached from the
    /// last ``refreshChrome()`` so ``showTrustExplanation()`` can build the surface
    /// from ONE chrome snapshot (the badge beside them came from the same one), the
    /// same reason the explanation itself is read off the painted label rather than
    /// re-read across the C-ABI. All three are the core's derivation, carried on
    /// the chrome JSON; this edge only holds onto them between a paint and a tap.
    private var trustPinDetailText: String = ""
    private var trustPinActionOffered: Bool = false
    private var trustPinActionLabelText: String = ""
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
    /// The LOAD-PROGRESS line: a thin determinate bar pinned along the TOP EDGE of
    /// the web view (in the gap under the URL-bar row), whose fraction advances
    /// with the real pipeline phase while a load is in flight and fades out once it
    /// settles. It replaces the loading BANNER, which was a full bar of chrome:
    /// showing/hiding it resized the content area on every navigation, so the page
    /// jumped twice per load. This line owns a fixed 3pt strip that is never given
    /// back, so no load state changes the layout (task
    /// `loading-progress-in-the-url-bar-not-a-banner`). CANCEL is the toolbar Stop
    /// button, which is enabled exactly while a load is in flight; the phase NAME
    /// stays in the footer status line, which already names it. Driven by the
    /// existing chrome-refresh pump (no new timer / poll / tight loop).
    private let loadingProgress = UIProgressView(progressViewStyle: .bar)
    private var webView: WKWebView!
    /// KVO token for observing `webView.url` so a SAME-DOCUMENT URL change (an SPA
    /// `pushState`/`replaceState`) is reported into the core. Held for the
    /// controller's lifetime; released on deinit.
    private var urlObservation: NSKeyValueObservation?
    /// The presented IN-APP DEBUG VIEW, if any (weak: the presentation retains it;
    /// after dismissal it deallocs and this nils). Refreshed from the shell's
    /// EXISTING chrome-refresh points (`refreshChrome`) and from the capture
    /// channel's message event (already on the main thread), so it tracks the
    /// store with NO timer and NO poll.
    private weak var debugViewController: DebugViewController?

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
        // The general browser menu: a ⋮ button presenting the core's menu on tap.
        // `showsMenuAsPrimaryAction` makes a single tap open the menu (no long
        // press), which is what a browser's ⋮ button does.
        menuButton.setTitle("⋮", for: .normal)
        menuButton.menu = browserMenu()
        menuButton.showsMenuAsPrimaryAction = true
        backButton.addTarget(self, action: #selector(onBack), for: .touchUpInside)
        forwardButton.addTarget(self, action: #selector(onForward), for: .touchUpInside)
        reloadButton.addTarget(self, action: #selector(onReload), for: .touchUpInside)
        stopButton.addTarget(self, action: #selector(onStop), for: .touchUpInside)

        // The nav buttons stay at their intrinsic (compact) width: they hug their
        // content tightly and resist being stretched. The URL field, by contrast,
        // hugs weakly and is the first to stretch, so it takes the MAJORITY of the
        // row while the four buttons keep only the width their glyphs need.
        for button in [backButton, forwardButton, reloadButton, stopButton, menuButton] {
            button.setContentHuggingPriority(.required, for: .horizontal)
            button.setContentCompressionResistancePriority(.required, for: .horizontal)
        }
        urlField.setContentHuggingPriority(.defaultLow, for: .horizontal)
        urlField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        // No text here: the badge's wording is the core's `invalid_entry_badge_text`,
        // painted in `refreshChrome()` like every other derived string. It USED to
        // be this Swift literal, set once at build and never refreshed: a
        // hand-written twin of the core rule that happened to agree, which would
        // have silently kept the old wording on iOS alone the first time the badge
        // was reworded (task `mobile-chrome-presentation-from-one-derivation`).
        invalidBadge.font = .systemFont(ofSize: 13)
        invalidBadge.textColor = UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
        invalidBadge.setContentHuggingPriority(.required, for: .horizontal)
        invalidBadge.setContentCompressionResistancePriority(.required, for: .horizontal)
        invalidBadge.isHidden = true

        // The ⋮ menu sits at the END of the toolbar, where every other browser
        // puts it.
        let toolbar = UIStackView(arrangedSubviews: [
            backButton, forwardButton, reloadButton, stopButton, urlField, invalidBadge,
            menuButton,
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
        // Wire the iOS CONSOLE + best-effort NETWORK capture that feeds the in-app
        // debug menu (task `debug-console-network-capture-per-platform`). WKWebView
        // has NO native console callback and NO per-resource load callback, so the
        // page-wide reach is INJECTED JS on a DEDICATED capture channel (never the
        // provider's trust channel): the console shim is the byte-for-byte SAME
        // string desktop injects (one place in `werust-core`), and the network one
        // is a best-effort `fetch`/`XHR` wrapper. What that wrapper cannot see (the
        // browser-internal subresource loads) is covered as far as iOS allows by
        // the NATIVE points below — the `WKURLSchemeHandler` tasks and the
        // main-frame navigations — and the residual gap is recorded honestly in
        // docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md.
        //
        // The scripts come from the core (which registered the channel handler), so
        // Swift adds no shim of its own: `documentStartScript()` returns EVERY
        // registered document-start script in injection order, which is why the
        // provider shim and both capture shims arrive through the one call above.
        let captureHandler = DebugCaptureHandler(core: core)
        // The open debug view refreshes from the capture event itself (the
        // script-message handler is already on the main thread): event-driven,
        // never a timer/poll.
        captureHandler.onCapture = { [weak self] in
            self?.debugViewController?.refresh()
        }
        configuration.userContentController.add(captureHandler, name: Self.captureChannel)
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
        // EDGE-SWIPE HISTORY NAVIGATION (task
        // `enable-the-ios-back-forward-swipe-gesture`, spec
        // `chrome-conventional-controls` story 13): the swipe-from-the-edge every
        // other iOS browser has, and the first thing an iOS user reaches for.
        // `WKWebView` defaults this to FALSE, so leaving it unset (as werust did)
        // means the gesture does nothing at all and history is reachable only
        // through the on-screen ◀/▶ buttons — which the sibling task
        // `ios-chrome-collapse-reload-stop-and-drop-history-buttons` REMOVES on
        // the strength of this line existing. Nobody here has a Mac to notice a
        // regression by using the app
        // (work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md),
        // so the flag is pinned by
        // `crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs` in
        // the pure-Rust gate. WebKit performs the navigation itself; the edge only
        // REPORTS it into the core (see `decidePolicyFor` below).
        webView.allowsBackForwardNavigationGestures = true
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

        statusLabel.font = .systemFont(ofSize: 13)
        statusLabel.textColor = .secondaryLabel
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        // The trust indicator, at the footer next to the status: painted from the
        // core's posture (the ACTUAL load path), the SAME four states desktop shows.
        //
        // The initial text of both labels is the core's OWN derivation for the
        // starting chrome, never a Swift literal that happens to match it: a
        // hard-coded "⚠ unverified origin" here would be one more hand-written twin
        // of `trust_indicator`, which is precisely what task
        // `mobile-chrome-presentation-from-one-derivation` removed from this edge.
        // Every later repaint comes from `refreshChrome()`.
        let initialChrome = core.chrome()
        statusLabel.text = initialChrome.statusLine
        trustLabel.text = initialChrome.trustIndicator
        trustLabel.font = .systemFont(ofSize: 13)
        trustLabel.textColor = .secondaryLabel
        trustLabel.textAlignment = .right
        trustLabel.setContentHuggingPriority(.required, for: .horizontal)
        trustLabel.translatesAutoresizingMaskIntoConstraints = false
        // The trust EXPLANATION on a platform with no hover: the badge's
        // accessibility LABEL is its own short STATE name (VoiceOver announces
        // WHICH posture this is first and always), and the core's
        // `trust_indicator_detail` follows it in the SECONDARY slot,
        // `accessibilityValue`, which VoiceOver reads straight after the label. A
        // TAP shows the same sentence in an alert. Both are set in
        // `refreshChrome()` from the same one derivation; the tap affordance is
        // wired once here.
        trustLabel.isUserInteractionEnabled = true
        trustLabel.isAccessibilityElement = true
        trustLabel.accessibilityLabel = initialChrome.trustIndicator
        trustLabel.accessibilityValue = initialChrome.trustIndicatorDetail
        trustLabel.addGestureRecognizer(
            UITapGestureRecognizer(target: self, action: #selector(showTrustExplanation)))

        // The prominent error banner: white-on-red, wrapping (a long protocol-named
        // reason stays legible), hidden until a load fails.
        errorBanner.font = .boldSystemFont(ofSize: 14)
        errorBanner.textColor = .white
        errorBanner.backgroundColor = UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
        errorBanner.numberOfLines = 0
        errorBanner.isHidden = true
        errorBanner.translatesAutoresizingMaskIntoConstraints = false

        // The LOAD-PROGRESS line: a thin determinate bar in werust's blue, along
        // the top edge of the web view. It starts fully transparent (a settled
        // chrome shows no track at all) and fades in while a load is in flight;
        // its 3pt strip is reserved permanently, so no load state ever resizes the
        // content area (task `loading-progress-in-the-url-bar-not-a-banner`).
        loadingProgress.progressTintColor = UIColor(
            red: 0.10, green: 0.37, blue: 0.71, alpha: 1.0)
        loadingProgress.trackTintColor = .clear
        loadingProgress.alpha = 0
        loadingProgress.isAccessibilityElement = true
        loadingProgress.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbar)
        view.addSubview(webView)
        // The error banner and the progress line are added AFTER the web view, so
        // they draw ON TOP of the page rather than being covered by it: both are
        // pinned into the same region the web view starts at, and z-order here is
        // subview order.
        view.addSubview(errorBanner)
        view.addSubview(loadingProgress)
        view.addSubview(statusLabel)
        view.addSubview(trustLabel)

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: g.topAnchor, constant: 8),
            toolbar.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            toolbar.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),

            // The LOAD-PROGRESS line sits along the web view's TOP EDGE, at a
            // FIXED 3pt height that is reserved whether or not a load is in
            // flight: a load state changes the bar's fraction and opacity, never
            // any geometry, so the page cannot jump on navigation the way the old
            // banner made it (task
            // `loading-progress-in-the-url-bar-not-a-banner`).
            loadingProgress.topAnchor.constraint(equalTo: toolbar.bottomAnchor, constant: 8),
            loadingProgress.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            loadingProgress.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            loadingProgress.heightAnchor.constraint(equalToConstant: 3),

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
        invalidBadge.isHidden = !chrome.invalidEntryBadgeVisible
        invalidBadge.text = chrome.invalidEntryBadgeText
        if chrome.invalidEntryBadgeVisible {
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
        statusLabel.text = chrome.statusLine
        // The trust indicator tracks the core's posture (the real load path),
        // matching desktop; the seam-default no-op is gone. The badge's
        // accessibility LABEL is the badge's own STATE name, so VoiceOver announces
        // which posture this is FIRST and always; its EXPLANATION (the core's
        // `trust_indicator_detail`, which used to reach desktop only) follows in
        // the secondary slot (`accessibilityValue`) and in the tap alert wired in
        // `layoutChrome()`: the platform-appropriate stand-in for desktop's hover
        // tooltip. Both strings are the core's, off this ONE snapshot — the edge
        // neither re-derives nor re-words either.
        trustLabel.text = chrome.trustIndicator
        trustLabel.accessibilityLabel = chrome.trustIndicator
        trustLabel.accessibilityValue = chrome.trustIndicatorDetail
        // The trust surface's TRUST-ON-FIRST-USE section, cached off this ONE
        // painted snapshot so the tap handler needs no second chrome read (which
        // would cross the C-ABI into the session again and could disagree with the
        // badge beside it). Every string is the core's derivation: this edge
        // decides neither the wording nor whether the action is offered.
        trustPinDetailText = chrome.trustPinDetail
        trustPinActionOffered = chrome.trustPinActionVisible
        trustPinActionLabelText = chrome.trustPinActionLabel
        // The LOAD-PROGRESS line: its fraction advances with the real pipeline
        // phase while a load is in flight (including the pre-content name-resolution
        // window, where the backend has not started yet), and it fades out once the
        // load settles. Only opacity and fraction change — the 3pt strip is
        // permanent — so a navigation never resizes the page (task
        // `loading-progress-in-the-url-bar-not-a-banner`). CANCEL is the toolbar Stop
        // button, enabled exactly while a load is in flight; the phase NAME is in
        // the footer status line (and this line's accessibility label). Driven by
        // this existing refresh, so no new timer / poll / tight loop.
        //
        // The core hands over its progress FRACTION on the 0.0-1.0 scale
        // `UIProgressView` already uses, so only the numeric type is narrowed here.
        let progressVisible = chrome.loadProgressVisible
        loadingProgress.setProgress(Float(chrome.loadProgressFraction), animated: progressVisible)
        loadingProgress.alpha = progressVisible ? 1 : 0
        loadingProgress.accessibilityLabel = progressVisible ? chrome.loadProgressHint : nil
        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate protocol-named reason across the top of the view so the user
        // cannot miss why nothing rendered (the fail-closed honesty fix). Hidden
        // otherwise. The SAME rule desktop/Android apply, from the same chrome fact.
        errorBanner.isHidden = !chrome.errorBannerVisible
        errorBanner.text = chrome.errorBannerText
        // A TRANSIENT/timeout failure (retryable) is a softer amber banner; a hard
        // failure is the prominent red one (task
        // `clearer-loading-and-error-indicator`). The SAME distinction desktop
        // shows, from the core's `retryable` fact.
        errorBanner.backgroundColor = chrome.retryable
            ? UIColor(red: 0.71, green: 0.51, blue: 0.04, alpha: 1.0)
            : UIColor(red: 0.75, green: 0.11, blue: 0.16, alpha: 1.0)
        // The open IN-APP DEBUG VIEW refreshes on this SAME existing
        // chrome-refresh point: the mobile cadence is event-driven (after each
        // core action / navigation signal), so the view tracks the store with NO
        // new timer and NO busy poll.
        debugViewController?.refresh()
    }

    // --- the general browser menu ---------------------------------------------

    /// Build the `UIMenu` from the SHARED core's menu items.
    ///
    /// The core owns the item LIST, so this only maps each item's `kind` onto a
    /// platform affordance: an `info` item (the `werust <version>` line) becomes a
    /// DISABLED `UIAction` (shown, not tappable), an `action` item an enabled one
    /// dispatched by its STABLE id (never its label) in ``onBrowserMenuItem(_:)``.
    /// Adding a future menu item therefore needs no change here at all unless it is
    /// an action with new behaviour — the "structured to grow" property.
    private func browserMenu() -> UIMenu {
        let actions = WerustCore.menu().items.map { item -> UIAction in
            let action = UIAction(title: item.label) { [weak self] _ in
                self?.onBrowserMenuItem(item.id)
            }
            // A non-interactive line (the version) is shown but not tappable.
            if !item.isAction() { action.attributes = [.disabled] }
            return action
        }
        return UIMenu(title: "", children: actions)
    }

    /// Dispatch an activated browser-menu entry by its STABLE core id. An id this
    /// build does not know about is ignored (the core is the source of the list; a
    /// newer item simply has no iOS behaviour yet).
    private func onBrowserMenuItem(_ id: String) {
        if id == WerustCore.Menu.itemDebug { openDebugView() }
    }

    /// The OPEN-DEBUG-VIEW hook the browser menu's Debug entry calls: presents
    /// the FULL-SCREEN tabbed Console + Network debug view over the core's shared
    /// capture store (`WerustCore.debugJSON()`). The menu task
    /// (`general-browser-menu-with-version-and-debug-entry`) left this hook an
    /// honest "not built yet" placeholder; THIS is the real view that fills it
    /// (task `debug-view-console-network-tabs-mobile`). The view's Done button
    /// dismisses it (the way back to the page).
    private func openDebugView() {
        let controller = DebugViewController(core: core)
        controller.modalPresentationStyle = .fullScreen
        debugViewController = controller
        present(controller, animated: true)
    }

    /// Show what the current trust posture MEANS: the core's
    /// `trust_indicator_detail`, titled with the badge it explains.
    ///
    /// This is the trust EXPLANATION on a platform with no hover. Desktop shows it
    /// as the badge's tooltip; for months neither mobile edge showed it AT ALL,
    /// because each had hand-written its own `trustIndicator()` twin and simply
    /// never wrote a `trustIndicatorDetail()` to go with it (`docs/adr/0011`,
    /// `docs/adr/0006`: for a browser whose thesis is an honest, legible trust
    /// posture, a badge with no explanation on the two platforms most users are on
    /// is a real gap). Reading the sentence from the chrome means iOS now shows
    /// exactly what desktop shows, and a future rewording lands on both at once.
    ///
    /// TWO affordances, both from the same one field: this TAP (an explicit user
    /// gesture, no hover needed) and the badge's accessibility VALUE set in
    /// ``refreshChrome()`` (VoiceOver reads the meaning after the state, never
    /// instead of it).
    ///
    /// It reads the PAINTED badge rather than calling the core again: a chrome read
    /// crosses the C-ABI into the session, and the label + its accessibility text
    /// were both set by the last ``refreshChrome()`` from ONE chrome snapshot, so
    /// they cannot disagree with each other.
    @objc private func showTrustExplanation() {
        guard let detail = trustLabel.accessibilityValue, !detail.isEmpty else { return }
        // The TRUST-ON-FIRST-USE section of the same surface (task
        // `ipns-tofu-pin-and-warn-on-change`): what this MUTABLE name resolves to
        // now, what the user blessed for it, and (when there is something new to
        // record) the bless action. The bless is reached FROM the trust indicator
        // by an explicit tap, never a first-visit prompt, which is why it lives
        // here rather than in an alert of its own. Every string is the core's, read
        // off the last painted snapshot.
        let message =
            trustPinDetailText.isEmpty ? detail : "\(detail)\n\n\(trustPinDetailText)"
        let alert = UIAlertController(
            title: trustLabel.text, message: message, preferredStyle: .alert)
        if trustPinActionOffered {
            alert.addAction(
                UIAlertAction(title: trustPinActionLabelText, style: .default) { [weak self] _ in
                    self?.core.blessName()
                    self?.afterCoreAction()
                })
        }
        alert.addAction(UIAlertAction(title: "OK", style: .default, handler: nil))
        present(alert, animated: true)
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

    // --- WKNavigationDelegate: a navigation the WEBVIEW drove -----------------
    // The EDGE-SWIPE gesture (enabled in `layoutChrome`) navigates the WKWebView's
    // OWN back-forward list internally: it calls none of the shell's actions, so
    // without this hook the core never learns the user moved, and the URL bar, the
    // trust posture and the Back/Forward flags would all keep describing the page
    // the user just swiped AWAY from.
    //
    // `decidePolicyFor` is the one callback that NAMES a navigation `.backForward`
    // (the swipe, and a page's own `history.back()`/`forward()`), and it is the
    // EARLIEST one: it fires before the target document's bytes are resolved, so
    // reporting here resets the per-load trust axes without clobbering the
    // verification the `ipfs` scheme handler performs for the NEW page moments
    // later. A core-driven load (`WKWebView.load`, what every toolbar action and
    // the pending-load pump produce) arrives as `.other`, so it is untouched here
    // and keeps flowing through `syncPendingLoad` exactly as before.
    //
    // The navigation is always ALLOWED: WebKit is performing it, and the edge only
    // reports it. Cancelling to re-drive `core.goBack()` was the obvious
    // alternative and is wrong twice over — it snaps the interactive gesture back
    // and re-LOADS the page instead of restoring it — and `refreshChrome()` (not
    // `afterCoreAction()`) is used deliberately, so no pending load can be issued
    // on top of the navigation WebKit is already running. See
    // docs/spikes/enable-the-ios-back-forward-swipe-gesture/DECISIONS.md.
    //
    // ONLY THE MAIN FRAME IS THE PAGE THE USER IS ON. WebKit issues a policy
    // decision PER FRAME, so a back navigation onto a page carrying iframes — or an
    // iframe of the current page calling `history.back()` — also delivers
    // `.backForward` decisions whose request url is the SUBFRAME's. Reported into
    // the core, such a url is neither neighbour of the current entry, so the move
    // takes its drift branch: it truncates the forward history and pushes the
    // subresource as the current entry, putting an address the user is not on (and,
    // for a hostile iframe, one an attacker chose) in the URL bar of a browser
    // whose thesis is an honest address. `targetFrame?.isMainFrame == true` is the
    // same `targetFrame` idiom the `_blank` hook above already uses, and it is
    // checked BEFORE the report, pinned in that order by
    // `crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs`.
    func webView(
        _ wv: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        if navigationAction.navigationType == .backForward,
            navigationAction.targetFrame?.isMainFrame == true,
            let target = navigationAction.request.url?.absoluteString, !target.isEmpty
        {
            // A history MOVE, not a new entry: the core lands its cursor on the
            // entry the user swiped to, so Back/Forward availability and the trust
            // posture describe the page now shown. The following didCommit /
            // didFinish (cross-document) or the KVO url observer (a same-document
            // entry) then report the SAME navigation through the SAME lifecycle
            // path a button-driven move takes; the core's move is idempotent, so
            // those repeats cost nothing.
            core.onHistoryNavigated(target)
            refreshChrome()
        }
        decisionHandler(.allow)
    }

    // --- WKNavigationDelegate -> Rust core ------------------------------------
    // Report the platform WKWebView's real load-lifecycle signals straight back
    // into the core, then repaint the chrome from the core.
    func webView(_ wv: WKWebView, didCommit navigation: WKNavigation!) {
        core.onPageCommitted(wv.url?.absoluteString ?? "")
        refreshChrome()
    }

    func webView(_ wv: WKWebView, didFinish navigation: WKNavigation!) {
        // THE MAIN-FRAME NETWORK capture point (task
        // `debug-console-network-capture-per-platform`): WKWebView gives no
        // per-resource callback, so a finished navigation is the one native signal
        // that a MAIN DOCUMENT was loaded — including the `https://` pages neither
        // the scheme handler nor the page-side fetch/XHR shim ever sees.
        //
        // It SKIPS the custom schemes the scheme handlers already recorded (with
        // the real status/MIME and, for `ipfs://`, the real verified posture),
        // exactly as the page-side shim does: one request must produce ONE row, and
        // the handler is the point that actually knows the outcome. Recorded as
        // `verified: false` because `didFinish` proves a page LOADED, not that its
        // bytes were hash-verified — but as the MAIN-FRAME row it then takes the
        // LOAD's own posture, so it reports exactly what the chrome trust indicator
        // shows rather than contradicting it.
        if let url = wv.url?.absoluteString, !url.isEmpty, !Self.isCoreServedScheme(url) {
            core.captureNetwork(
                method: "GET", url: url, status: 0, mime: "", size: 0,
                verified: false, mainFrame: true)
        }
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

    /// Whether `url` uses a scheme werust SERVES itself through a
    /// `WKURLSchemeHandler` (`ipfs://`, `werust://`), and which is therefore
    /// already captured at that handler with its real status/MIME and its real
    /// trust posture.
    ///
    /// The other capture points skip these so one request produces ONE Network-tab
    /// row: a second row from a point that does NOT know the outcome would claim
    /// the weaker unverified posture and contradict the handler's honest one. The
    /// page-side `fetch`/`XHR` shim applies the same rule in JS.
    fileprivate static func isCoreServedScheme(_ url: String) -> Bool {
        let lower = url.lowercased()
        return lower.hasPrefix("ipfs:") || lower.hasPrefix("werust:")
    }

    /// The DEBUG CAPTURE script-message channel the injected console + fetch/XHR
    /// shims post on (matches `werust_core::debug::CAPTURE_BRIDGE`).
    ///
    /// Deliberately its OWN channel, not the provider's: the provider channel is a
    /// trust surface with a request/response contract, while capture is one-way,
    /// READ-ONLY observation. Nothing is ever pushed back down this one.
    fileprivate static let captureChannel = "werustDebug"
}

/// The iOS IN-APP DEBUG VIEW: a full-screen tabbed screen (Console + Network)
/// presented from the browser menu's Debug entry, rendering the ONE shared
/// capture store over the FFI (`WerustCore.debugJSON()`) (task
/// `debug-view-console-network-tabs-mobile`, spec
/// `in-app-debug-menu-console-and-network`). The twin of the Android `DebugView`.
///
/// This is the no-tether debug surface: a phone user with no desktop opens the
/// ⋮ menu -> Debug and sees the page's console log and network requests IN-APP.
/// The native remote inspector (Safari Web Inspector over USB) stays as the deep
/// devtools; this is the standalone console+network subset, and it is READ-ONLY
/// by construction (rows are table cells; no `UITextField`/`UITextView` exists
/// here; a typeable REPL is the remote inspector's job, spec Out of Scope).
///
/// The recorded decisions this bakes in live in
/// `docs/spikes/debug-view-console-network-tabs-mobile/DECISIONS.md`; the short
/// form:
///
/// * TABS AS TWO TOGGLED LISTS: a `UISegmentedControl` switching ONE table
///   between the Console and Network tabs (the "two toggled lists" the task
///   allows), mirroring Android's two toggle buttons over one list; no new UI
///   framework (SwiftUI) is pulled into a UIKit-programmatic shell.
/// * REFRESH IS EVENT-DRIVEN, ON THE EXISTING CADENCE: the shell calls
///   `refresh()` from its own `refreshChrome` (the existing chrome-refresh
///   point) and from the capture channel's message event (already on the main
///   thread). NO new timer, NO poll. Each refresh re-renders from the whole
///   snapshot: the store is bounded (300 entries x 2000 chars) and the cadence
///   is per page event, not per frame, so the incremental sequence-anchor the
///   DESKTOP view needs on its 50ms pump is not needed here (the FFI document
///   carries no sequence).
/// * iOS NETWORK COVERAGE IS PARTIAL, BY PLATFORM: the capture task records
///   exactly what iOS can see (custom-scheme tasks, main-frame navigations,
///   page-issued fetch/XHR via the shim, never the browser-internal
///   subresource loads WKWebView exposes no callback for); this view renders
///   whatever is captured and improves as capture does
///   (`docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`,
///   Decision 3).
/// * THE NETWORK TAB SPEAKS THE TRUST INDICATOR'S EXACT VOCABULARY (ADR-0006):
///   each row's trust is the indicator's glyph for the posture plus the core's
///   wire name the debug JSON already carries, in the same hues the desktop
///   stylesheet gives the `trust-*` classes, never a new label.
final class DebugViewController: UIViewController, UITableViewDataSource {
    /// Which tab is showing: the CONSOLE log (segment 0) or the NETWORK requests
    /// (segment 1).
    private enum Tab: Int {
        case console = 0
        case network = 1
    }

    /// One rendered row: the main line, an optional detail line (the network
    /// URL, unbounded, on its own line so a phone-width screen keeps the
    /// columns legible), its colour (nil = the theme default), and whether the
    /// main line is bold (console errors/warnings).
    private struct Row {
        let text: String
        let detail: String?
        let color: UIColor?
        let bold: Bool
    }

    private let core: WerustCore
    private let tabControl = UISegmentedControl(items: ["Console", "Network"])
    private let tableView = UITableView()
    private let emptyLabel = UILabel()
    private var rows: [Row] = []

    init(core: WerustCore) {
        self.core = core
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used; the shell builds this controller in code")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground

        // The header row: the title, the CLEAR action (empties BOTH buffers of
        // the shared store over the FFI), and Done (the way back to the page).
        let titleLabel = UILabel()
        titleLabel.text = "Console + Network capture"
        titleLabel.font = .boldSystemFont(ofSize: 15)
        titleLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let clearButton = UIButton(type: .system)
        clearButton.setTitle("Clear", for: .normal)
        clearButton.addTarget(self, action: #selector(onClear), for: .touchUpInside)
        let doneButton = UIButton(type: .system)
        doneButton.setTitle("Done", for: .normal)
        doneButton.addTarget(self, action: #selector(onDone), for: .touchUpInside)
        let header = UIStackView(arrangedSubviews: [titleLabel, clearButton, doneButton])
        header.axis = .horizontal
        header.spacing = 8
        header.alignment = .center
        header.translatesAutoresizingMaskIntoConstraints = false

        // The tab strip: a segmented control switching the ONE table between the
        // Console and Network tabs.
        tabControl.selectedSegmentIndex = Tab.console.rawValue
        tabControl.addTarget(self, action: #selector(onTabChanged), for: .valueChanged)
        tabControl.translatesAutoresizingMaskIntoConstraints = false

        emptyLabel.text = "Nothing captured yet"
        emptyLabel.textAlignment = .center
        emptyLabel.textColor = .secondaryLabel
        emptyLabel.isHidden = true
        emptyLabel.translatesAutoresizingMaskIntoConstraints = false

        tableView.dataSource = self
        tableView.allowsSelection = false
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 44
        tableView.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(header)
        view.addSubview(tabControl)
        view.addSubview(tableView)
        view.addSubview(emptyLabel)

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: g.topAnchor, constant: 8),
            header.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            header.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),

            tabControl.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 8),
            tabControl.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 8),
            tabControl.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -8),

            tableView.topAnchor.constraint(equalTo: tabControl.bottomAnchor, constant: 8),
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            tableView.bottomAnchor.constraint(equalTo: g.bottomAnchor),

            emptyLabel.centerXAnchor.constraint(equalTo: tableView.centerXAnchor),
            emptyLabel.centerYAnchor.constraint(equalTo: tableView.centerYAnchor),
        ])

        // Paint the store captured so far on open, so the view never opens
        // visibly empty when there are already entries.
        refresh()
    }

    /// The CLEAR action: empty BOTH buffers of the shared store over the FFI,
    /// then repaint.
    @objc private func onClear() {
        core.debugClear()
        refresh()
    }

    /// Done: the way back to the page.
    @objc private func onDone() {
        dismiss(animated: true)
    }

    @objc private func onTabChanged() {
        refresh()
        // A tab switch starts at the BOTTOM (the newest entries).
        scrollToBottom()
    }

    /// Catch the view up with the store: re-read the FFI debug document and
    /// re-render the active tab. Called on open, on the shell's EXISTING
    /// chrome-refresh points, and from the capture channel's message event
    /// (already on the main thread): event-driven, never a timer/poll. Newest
    /// stays at the bottom; the scroll sticks to the bottom only when the user
    /// is already there (a user scrolled up reading an earlier entry is never
    /// yanked back down).
    func refresh() {
        // Before the view is loaded there is nothing to paint (viewDidLoad
        // paints on open); the shell may call this any time after presentation.
        guard isViewLoaded else { return }
        let wasAtBottom = isAtBottom()
        rows = currentRows()
        emptyLabel.isHidden = !rows.isEmpty
        tableView.reloadData()
        if wasAtBottom {
            scrollToBottom()
        }
    }

    /// Scroll to the newest row (the BOTTOM; rows are oldest-first, the
    /// devtools-console idiom).
    private func scrollToBottom() {
        guard !rows.isEmpty else { return }
        tableView.layoutIfNeeded()
        tableView.scrollToRow(
            at: IndexPath(row: rows.count - 1, section: 0), at: .bottom, animated: false)
    }

    /// Whether the table is showing its newest row.
    private func isAtBottom() -> Bool {
        if rows.isEmpty { return true }
        return tableView.contentOffset.y + tableView.bounds.height
            >= tableView.contentSize.height - 1
    }

    /// The rows of the active tab, parsed from the FFI debug document.
    private func currentRows() -> [Row] {
        guard let data = core.debugJSON().data(using: .utf8),
              let document = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [] }
        if tabControl.selectedSegmentIndex == Tab.network.rawValue {
            let entries = document["network"] as? [[String: Any]] ?? []
            return entries.map(Self.networkRow)
        }
        let entries = document["console"] as? [[String: Any]] ?? []
        return entries.map(Self.consoleRow)
    }

    /// One CONSOLE row of the FFI debug document, coloured by its level. An
    /// unknown line is JSON `null`, kept honestly absent.
    private static func consoleRow(_ entry: [String: Any]) -> Row {
        let level = entry["level"] as? String ?? "log"
        return Row(
            text: consoleRowText(
                level: level,
                message: entry["message"] as? String ?? "",
                source: entry["source"] as? String ?? "",
                line: entry["line"] as? Int),
            detail: nil,
            color: consoleLevelColor(level),
            bold: level == "error" || level == "warn")
    }

    /// One NETWORK row of the FFI debug document, coloured by its trust
    /// posture. Unknown status/size is JSON `null`, kept honestly absent.
    private static func networkRow(_ entry: [String: Any]) -> Row {
        let trust = entry["trust"] as? String ?? "unverified-origin"
        return Row(
            text: networkSummaryText(
                method: entry["method"] as? String ?? "GET",
                status: entry["status"] as? Int,
                mime: entry["mime"] as? String ?? "",
                size: entry["size"] as? Int,
                trust: trust),
            detail: entry["url"] as? String ?? "",
            color: trustColor(trust),
            bold: false)
    }

    // --- the render-from-store mapping (pure; the twin of the Android mapping) -

    /// The full text of one console row: `[<level>] <message>` plus the
    /// `<source>:<line>` tail in parentheses when there is one. The level tag is
    /// the store's OWN wire name, and an absent source/line stays honestly
    /// absent (never a fabricated `:0`). The SAME mapping the desktop
    /// `console_row_text` applies.
    static func consoleRowText(level: String, message: String, source: String, line: Int?) -> String {
        if source.isEmpty { return "[\(level)] \(message)" }
        if let line = line { return "[\(level)] \(message) (\(source):\(line))" }
        return "[\(level)] \(message) (\(source))"
    }

    /// The colour of one console row by its level (the desktop stylesheet's
    /// hues: info blue, warn amber, error red, debug grey), nil = the theme
    /// default for log.
    static func consoleLevelColor(_ level: String) -> UIColor? {
        switch level {
        case "info": return UIColor(red: 0x1A / 255, green: 0x5F / 255, blue: 0xB4 / 255, alpha: 1)
        case "warn": return UIColor(red: 0x9A / 255, green: 0x6A / 255, blue: 0x00 / 255, alpha: 1)
        case "error": return UIColor(red: 0xC0 / 255, green: 0x1C / 255, blue: 0x28 / 255, alpha: 1)
        case "debug": return UIColor(red: 0x5C / 255, green: 0x5C / 255, blue: 0x5C / 255, alpha: 1)
        default: return nil
        }
    }

    /// The per-request trust label of a network row: the mobile trust
    /// indicator's glyph for the posture (`✓` / `◈` / `◇` / `⚠`, the SAME four
    /// `Chrome.trustIndicator` carries) plus the core's wire name the debug
    /// JSON carries, never a new label minted for the debug view (ADR-0006).
    /// TOTAL and fail-closed: an unrecognised posture renders as the unverified
    /// one, never verbatim (a verbatim render could smuggle a minted label into
    /// the one surface whose job is honest trust).
    static func networkTrustLabel(_ trust: String) -> String {
        switch trust {
        case "content-verified": return "✓ content-verified"
        case "name-via-trusted-rpc": return "◈ name-via-trusted-rpc"
        case "mutable-name": return "◇ mutable-name"
        default: return "⚠ unverified-origin"
        }
    }

    /// The colour of a network row by its trust posture (the indicator's hues:
    /// the desktop stylesheet's `trust-*` colours).
    static func trustColor(_ trust: String) -> UIColor {
        switch trust {
        case "content-verified":
            return UIColor(red: 0x0A / 255, green: 0x7D / 255, blue: 0x28 / 255, alpha: 1)
        case "name-via-trusted-rpc":
            return UIColor(red: 0x1A / 255, green: 0x5F / 255, blue: 0xB4 / 255, alpha: 1)
        case "mutable-name":
            return UIColor(red: 0x6C / 255, green: 0x3F / 255, blue: 0xB4 / 255, alpha: 1)
        default:
            return UIColor(red: 0x9A / 255, green: 0x6A / 255, blue: 0x00 / 255, alpha: 1)
        }
    }

    /// The summary line of one network row: method, status, MIME, size and the
    /// honest per-request trust label. An unknown field renders as `?`, never a
    /// fabricated `0` (the store's own honesty rule).
    static func networkSummaryText(
        method: String, status: Int?, mime: String, size: Int?, trust: String
    ) -> String {
        [
            method,
            status.map { String($0) } ?? "?",
            mime.isEmpty ? "?" : mime,
            sizeText(size),
            networkTrustLabel(trust),
        ].joined(separator: "  ")
    }

    /// A human byte count (`512 B`, `1.5 KB`, `2.0 MB`), or `?` when unknown.
    static func sizeText(_ size: Int?) -> String {
        guard let size = size else { return "?" }
        if size < 1024 { return "\(size) B" }
        if size < 1024 * 1024 { return String(format: "%.1f KB", Double(size) / 1024.0) }
        return String(format: "%.1f MB", Double(size) / (1024.0 * 1024.0))
    }

    // --- UITableViewDataSource (READ-ONLY rows) -------------------------------

    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        rows.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let row = rows[indexPath.row]
        // Two reuse pools: a plain one for console rows, a subtitle one for
        // network rows (the URL rides the detail line).
        let identifier = row.detail == nil ? "debugConsoleRow" : "debugNetworkRow"
        let cell = tableView.dequeueReusableCell(withIdentifier: identifier)
            ?? UITableViewCell(
                style: row.detail == nil ? .default : .subtitle, reuseIdentifier: identifier)
        cell.textLabel?.text = row.text
        cell.textLabel?.numberOfLines = 0
        cell.textLabel?.font = row.bold ? .boldSystemFont(ofSize: 13) : .systemFont(ofSize: 13)
        cell.textLabel?.textColor = row.color ?? .label
        cell.detailTextLabel?.text = row.detail
        cell.detailTextLabel?.numberOfLines = 0
        cell.detailTextLabel?.font = .systemFont(ofSize: 12)
        cell.detailTextLabel?.textColor = .secondaryLabel
        return cell
    }
}

/// The `WKScriptMessageHandler` for the DEBUG CAPTURE channel: the iOS edge that
/// receives what the injected console / fetch-XHR shims observed and hands it to
/// the shared `werust-core` capture store the debug view renders (task
/// `debug-console-network-capture-per-platform`).
///
/// One-way by construction: it returns nothing to the page and evaluates no JS
/// back into it (contrast [ProviderBridgeHandler], which must answer). The body
/// is PAGE-CONTROLLED text — a hostile page can post on this channel directly —
/// so it is handed straight to the core's total, fail-quiet parse, which drops an
/// unreadable body rather than fabricating an entry, and never lets a
/// shim-reported request claim to have been verified.
final class DebugCaptureHandler: NSObject, WKScriptMessageHandler {
    private let core: WerustCore

    /// Called after each captured envelope has been handed to the store, so an
    /// OPEN debug view can refresh from the SAME event (this handler is already
    /// on the main thread) instead of polling.
    var onCapture: (() -> Void)?

    init(core: WerustCore) {
        self.core = core
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        // The shims post their envelope as a JSON string.
        core.captureScriptMessage(
            WKWebViewShellController.captureChannel, message.body as? String ?? "")
        onCapture?()
    }
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
        // The iOS NETWORK capture point with the REAL outcome: this handler knows
        // whether the bytes actually came back hash-verified, which the page-side
        // fetch/XHR shim never could (and which is why that shim deliberately
        // SKIPS `ipfs:`/`werust:` — it would otherwise add a second, contradicting
        // row claiming the weaker unverified posture).
        //
        // `mainFrame: false` because a `WKURLSchemeTask` carries NO main-frame
        // flag: the core decides, with the ONE shared main-frame predicate driven
        // by the top-level URL the shell reports on every navigation. Swift must
        // NOT compare here — the obvious compare against `chrome().url` is against
        // the DISPLAY identity, which on an ENS load is the pinned `ronan.eth`
        // while this task's URL is `ipfs://<cid>/…`, so it would never fire on
        // exactly the page the reconciliation exists for.
        // Capture is READ-ONLY: it does not change what is served below.
        let method = urlSchemeTask.request.httpMethod ?? "GET"
        let mainFrame = false
        switch core.resolveIpfs(url.absoluteString) {
        case .some(.success(let mimeType, let body, let status)):
            core.captureNetwork(
                method: method, url: url.absoluteString, status: UInt16(status),
                mime: mimeType, size: UInt64(body.count), verified: true,
                mainFrame: mainFrame)
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
            // A FAILED resolution proved nothing, so it is captured honestly
            // UNVERIFIED. Capturing the failure is the point: the Network tab is
            // where a user diagnoses why a page did not render.
            core.captureNetwork(
                method: method, url: url.absoluteString, status: 0, mime: "", size: 0,
                verified: false, mainFrame: mainFrame)
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
        // Capture the internal-page request too, so the Network tab is the whole
        // reachable stream. It is honestly UNVERIFIED: `werust://settings` is an
        // internal chrome page, NOT hash-verified content (the same distinction the
        // trust indicator makes).
        //
        // `mainFrame: false` for the same reason as [IpfsSchemeHandler]: a scheme
        // task carries no main-frame flag, so the CORE decides with its one shared
        // predicate rather than Swift comparing against the display identity.
        let method = urlSchemeTask.request.httpMethod ?? "GET"
        let mainFrame = false
        switch core.applySettings(url.absoluteString) {
        case .some(.success(let mimeType, let body, _)):
            core.captureNetwork(
                method: method, url: url.absoluteString, status: 200, mime: mimeType,
                size: UInt64(body.count), verified: false, mainFrame: mainFrame)
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
