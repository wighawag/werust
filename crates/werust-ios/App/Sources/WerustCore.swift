// WerustCore — the Swift edge's thin binding to the werust **Rust core**
// (`libwerust_mobile.a`, built from the `werust-core` / `werust-ios-core`
// crates). The twin of the Android `WerustCore.kt`, over a plain C-ABI instead
// of JNI.
//
// This class holds NO browsing logic: it is a mechanical bridge over the C-ABI
// the Rust core exports (declared in `werust_mobile.h`, the bridging header).
// Every browsing decision (whether Back is available, what the URL bar shows,
// the load state) is the core's truth, read back through `chrome()`. The
// controller drives the core through this class and paints whatever the core
// reports; the platform `WKWebView` is fed the URL the core surfaces via
// `takePendingLoad()` and reports its real load signals back in via
// `onPageCommitted` / `onPageFinished` / `onPageFailed`.
//
// One instance per view controller; the deinit frees the native session.

import Foundation

final class WerustCore {
    // Opaque pointer to the native `CoreSession`, threaded through every call.
    private let handle: OpaquePointer

    init() {
        handle = werust_ios_session_new()
    }

    deinit {
        werust_ios_session_free(handle)
    }

    /// Navigate to `url` (the URL bar's Enter action). Returns false if rejected.
    @discardableResult
    func navigate(_ url: String) -> Bool {
        url.withCString { werust_ios_navigate(handle, $0) }
    }

    /// Go one step back in the core's session history.
    func goBack() { werust_ios_go_back(handle) }

    /// Go one step forward in the core's session history.
    func goForward() { werust_ios_go_forward(handle) }

    /// Reload the current page. Returns false if there is nothing to reload.
    @discardableResult
    func reload() -> Bool { werust_ios_reload(handle) }

    /// Stop the in-flight load.
    func stop() { werust_ios_stop(handle) }

    /// The URL (if any) the core has committed to but the platform `WKWebView`
    /// has not yet loaded. `nil` means "nothing pending".
    func takePendingLoad() -> String? {
        guard let c = werust_ios_take_pending_load(handle) else { return nil }
        defer { werust_ios_string_free(c) }
        return String(cString: c)
    }

    /// Resolve an intercepted `ipfs://<cid>[/path]` request through the SHARED
    /// `werust-core` resolve path (the same hash-verified path desktop uses), for
    /// the `WKURLSchemeHandler`.
    ///
    /// A `WKWebView` loads `ipfs://` only via a registered `WKURLSchemeHandler`,
    /// so the shell's handler calls this and answers the `WKURLSchemeTask` from
    /// the result: verified bytes + MIME type on success, a fail-closed
    /// `.failure(reason)` (a hash mismatch / unverifiable CID / source error,
    /// never rendered) on failure, or `nil` if the URL is not an intercepted
    /// scheme. The native resolution handle is queried and freed here.
    func resolveIpfs(_ uri: String) -> Resolution? {
        guard let res = uri.withCString({ werust_ios_resolve_ipfs(handle, $0) }) else {
            return nil
        }
        defer { werust_ios_resolution_free(res) }
        if werust_ios_resolution_is_ok(res) {
            let mime = werust_ios_resolution_mime(res).map { c -> String in
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            } ?? ""
            let ptr = werust_ios_resolution_body(res)
            let len = werust_ios_resolution_body_len(res)
            let body: Data
            if let ptr = ptr, len > 0 {
                body = Data(bytes: ptr, count: len)
            } else {
                body = Data()
            }
            let status = Int(werust_ios_resolution_status(res))
            return .success(mimeType: mime, body: body, status: status)
        } else {
            let reason = werust_ios_resolution_error(res).map { c -> String in
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            } ?? "ipfs resolution failed"
            return .failure(reason: reason)
        }
    }

    /// Serve (and apply) an intercepted `werust://settings[?backend=...]` request
    /// through the SHARED `werust-core` settings path (the same retrieval-backend
    /// settings page desktop + Android serve), for the `werust` scheme's
    /// `WKURLSchemeHandler`.
    ///
    /// A `WKWebView` loads a custom scheme like `werust://` only via a registered
    /// `WKURLSchemeHandler`, so the shell's handler for `werust` calls this and
    /// answers the `WKURLSchemeTask` from the result: the page HTML + `text/html`
    /// on success, a fail-closed `.failure(reason)` (a non-`settings` host) on
    /// failure, or `nil` if the URL is not the `werust` scheme. A
    /// `?backend=<kind>[&url=...]` selection is validated + persisted by the shared
    /// core (the same isolated settings file the desktop chrome writes). The native
    /// resolution handle is queried and freed here.
    ///
    /// This is the twin of [resolveIpfs]; it exists as a distinct method so the
    /// shell registers a `WKURLSchemeHandler` per scheme and each is honestly named
    /// for what it serves (the requeue's Gate-2 fix: the `werust` scheme was
    /// unreachable on iOS with no Swift handler dispatching it).
    func applySettings(_ uri: String) -> Resolution? {
        guard let res = uri.withCString({ werust_ios_apply_settings(handle, $0) }) else {
            return nil
        }
        defer { werust_ios_resolution_free(res) }
        if werust_ios_resolution_is_ok(res) {
            let mime = werust_ios_resolution_mime(res).map { c -> String in
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            } ?? ""
            let ptr = werust_ios_resolution_body(res)
            let len = werust_ios_resolution_body_len(res)
            let body: Data
            if let ptr = ptr, len > 0 {
                body = Data(bytes: ptr, count: len)
            } else {
                body = Data()
            }
            return .success(mimeType: mime, body: body, status: Int(werust_ios_resolution_status(res)))
        } else {
            let reason = werust_ios_resolution_error(res).map { c -> String in
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            } ?? "settings request failed"
            return .failure(reason: reason)
        }
    }

    /// The outcome of resolving an intercepted `ipfs://` request through the core:
    /// verified bytes + MIME type + status, or a fail-closed reason. Mirrors the
    /// Rust `SchemeResolution`; the `WKURLSchemeHandler` turns `.success` into a
    /// `URLResponse` + data on the task and `.failure` into `didFailWithError`,
    /// so the desktop fail-closed posture holds on iOS. Also serves the internal
    /// `werust://settings` page (via [applySettings]): the `.success` bytes are the
    /// page HTML, the `.failure` reason a fail-closed host error.
    ///
    /// `status` is 200 for an ordinary resource; it is carried so a site's own
    /// error page (named by its `_redirects`, IPIP-0002, for a path that is not in
    /// its DAG) is answered with the HONEST not-found status while still
    /// rendering, exactly as an IPFS gateway serves it.
    enum Resolution {
        case success(mimeType: String, body: Data, status: Int)
        case failure(reason: String)
    }

    /// The document-start script (the EIP-1193 provider shim) to install onto the
    /// platform `WKWebView` as a `WKUserScript` so a page's `window.ethereum` is
    /// the injected native provider. Routed through the SAME `werust-core` provider
    /// path desktop uses. `nil` / empty means nothing to inject.
    func documentStartScript() -> String {
        guard let c = werust_ios_document_start_script(handle) else { return "" }
        defer { werust_ios_string_free(c) }
        return String(cString: c)
    }

    /// Dispatch an EIP-1193 envelope a page posted on the provider channel through
    /// the shared `werust-core` provider path and return the response JS to run in
    /// the live page (via `WKWebView.evaluateJavaScript`) to settle the page's
    /// pending Promise. Empty means nothing to run. This is the page -> native ->
    /// page provider round-trip on iOS, called from the `WKScriptMessageHandler`.
    func handleProviderMessage(_ name: String, _ body: String) -> String {
        name.withCString { n in
            body.withCString { b in
                guard let c = werust_ios_handle_provider_message(handle, n, b) else { return "" }
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            }
        }
    }

    /// Report the platform `WKWebView`'s commit signal into the core.
    func onPageCommitted(_ url: String) {
        url.withCString { werust_ios_on_page_committed(handle, $0) }
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    func onPageFinished(_ url: String) {
        url.withCString { werust_ios_on_page_finished(handle, $0) }
    }

    /// Report a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
    /// client-side navigation) into the core, so the URL bar follows the new
    /// location instead of freezing. Reported from a KVO observer on
    /// `webView.url`, which fires on same-document history changes (no
    /// `didCommit`/`didFinish`).
    func onUrlChanged(_ url: String) {
        url.withCString { werust_ios_on_url_changed(handle, $0) }
    }

    /// Report the platform `WKWebView`'s error signal into the core.
    func onPageFailed(_ url: String, _ reason: String) {
        url.withCString { u in
            reason.withCString { r in werust_ios_on_page_failed(handle, u, r) }
        }
    }

    /// The current chrome the controller paints (URL bar, nav enablement, status).
    func chrome() -> Chrome {
        guard let c = werust_ios_chrome_json(handle) else { return Chrome.idle }
        defer { werust_ios_string_free(c) }
        return Chrome.fromJSON(String(cString: c))
    }

    /// The bounded console + network CAPTURE STORE as its own JSON document, the
    /// wire form the in-app debug view renders (a DEDICATED accessor beside
    /// [chrome], so the chrome JSON — re-encoded on every refresh — stays lean).
    func debugJSON() -> String {
        guard let c = werust_ios_debug_json(handle) else { return "" }
        defer { werust_ios_string_free(c) }
        return String(cString: c)
    }

    /// Empty the capture store: the debug view's Clear action.
    func debugClear() { werust_ios_debug_clear(handle) }

    /// Capture one envelope the injected debug shim posted on the capture channel,
    /// from the `WKScriptMessageHandler` (task
    /// `debug-console-network-capture-per-platform`).
    ///
    /// WKWebView has NO native console callback, so iOS captures the console with
    /// the SAME injected shim desktop uses (one shared string in `werust-core`),
    /// and captures what network it can reach with a best-effort `fetch`/`XHR`
    /// wrapper on the same channel. The body is page-controlled text; the core's
    /// parse is total and fail-quiet, so a hostile or unreadable body is dropped
    /// rather than fabricated into an entry.
    func captureScriptMessage(_ name: String, _ body: String) {
        name.withCString { n in
            body.withCString { b in werust_ios_capture_script_message(handle, n, b) }
        }
    }

    /// Capture one NETWORK request from an iOS point that sees it NATIVELY: the
    /// `WKURLSchemeHandler` custom-scheme tasks and the `WKNavigationDelegate`
    /// main-frame navigations.
    ///
    /// [verified] must say whether THIS request's bytes really came back through
    /// the hash-verified content-addressed path — never whether the URL merely
    /// looks content-addressed — so the Network tab can never imply a request was
    /// trusted that was not (ADR-0006).
    ///
    /// [mainFrame] says only that the CALLER natively knows this is the main
    /// document (the `WKNavigationDelegate`, handed the main frame's own URL). A
    /// `WKURLSchemeTask` carries no such flag and passes `false`: the CORE then
    /// decides with its ONE shared main-frame predicate. Do NOT compare URLs in
    /// Swift — the obvious compare against `chrome().url` is against the DISPLAY
    /// identity, which on an ENS load is the pinned name while the request is
    /// `ipfs://<cid>/…`, so it never fires on the page the reconciliation exists
    /// for. Either way the main-document row takes the LOAD's own posture, so the
    /// tab cannot contradict the trust indicator. A `0` [status]/[size] means
    /// unknown.
    func captureNetwork(
        method: String,
        url: String,
        status: UInt16,
        mime: String,
        size: UInt64,
        verified: Bool,
        mainFrame: Bool
    ) {
        method.withCString { m in
            url.withCString { u in
                mime.withCString { mi in
                    werust_ios_capture_network(
                        handle, m, u, status, mi, size, verified, mainFrame)
                }
            }
        }
    }

    /// werust's version string, from the ONE shared source (`werust_core::version`,
    /// the Rust workspace version) over the C-ABI — never a Swift literal and never
    /// the bundle's `CFBundleShortVersionString`, so the iOS menu can never disagree
    /// with the desktop and Android menus.
    ///
    /// `static` because the export takes NO session handle: the version is a
    /// property of the BUILD, not of a browsing session.
    static func version() -> String {
        guard let c = werust_ios_version() else { return "" }
        defer { werust_ios_string_free(c) }
        return String(cString: c)
    }

    /// The GENERAL browser menu (the ⋮ menu) the controller builds its native
    /// `UIMenu` from: the werust VERSION line + a Debug entry that opens the in-app
    /// debug view, decoded from the core's JSON wire form.
    ///
    /// The item list is the SHARED core's, so the iOS menu shows exactly what the
    /// desktop popover and the Android menu show, and a FUTURE menu item added in
    /// `werust-core` appears here with no Swift change. `static` for the same
    /// reason as ``version()``: the menu is session-free, hence always available.
    static func menu() -> Menu {
        guard let c = werust_ios_menu_json() else { return .empty }
        defer { werust_ios_string_free(c) }
        return Menu.fromJSON(String(cString: c))
    }

    /// The GENERAL browser menu, decoded from the core's JSON wire form: the werust
    /// `version` plus the ordered `items` every platform renders.
    ///
    /// A CONTAINER meant to GROW into the usual browser items (bookmarks, settings,
    /// history, …): the controller maps `items` onto `UIAction`s by each item's
    /// `kind`, so a new item added in `werust-core` needs no Swift change unless it
    /// is an action with new behaviour.
    struct Menu {
        let version: String
        let items: [Item]

        /// One menu entry: the stable cross-platform `id` the controller dispatches
        /// on (never the `label`, which is display text), the `label` to show, and
        /// the `kind` telling the controller whether to render it as a
        /// non-interactive line or a tappable entry.
        struct Item {
            let id: String
            let label: String
            let kind: String

            /// Whether this entry is activatable (vs a non-interactive line).
            func isAction() -> Bool { kind == Menu.kindAction }
        }

        /// The stable id of the non-interactive `werust <version>` line.
        static let itemVersion = "version"
        /// The stable id of the entry that opens the in-app debug view.
        static let itemDebug = "debug"
        /// The `kind` of an activatable entry (vs `"info"`, a plain line).
        static let kindAction = "action"

        /// The fail-soft fallback if the wire form is unreadable.
        static let empty = Menu(version: "", items: [])

        static func fromJSON(_ json: String) -> Menu {
            guard let data = json.data(using: .utf8),
                  let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return .empty }
            let raw = o["items"] as? [[String: Any]] ?? []
            let items = raw.compactMap { entry -> Item? in
                guard let id = entry["id"] as? String, let label = entry["label"] as? String
                else { return nil }
                return Item(id: id, label: label, kind: entry["kind"] as? String ?? "info")
            }
            return Menu(version: o["version"] as? String ?? "", items: items)
        }
    }

    /// The chrome the controller paints, decoded from the core's JSON wire form.
    /// Every field is the core's truth; the controller holds none of this logic.
    ///
    /// TWO HALVES, both decided in `werust-core`:
    ///
    /// * the FACTS (`url`, `loading`, `loadStep`, `trustPosture`, `error`,
    ///   `failureKind`, `retryable`, `invalidEntry`, …): what the chrome IS;
    /// * the DERIVATION (`statusLine`, `trustIndicator`, `trustIndicatorDetail`,
    ///   `errorBannerText`, `invalidEntryBadgeText`, `loadProgressFraction`, …):
    ///   what the chrome SHOWS, each the return value of the core rule of the same
    ///   name (`status_line`, `trust_indicator`, `trust_indicator_detail`,
    ///   `error_banner_text`, `invalid_entry_badge_text`, `load_progress_fraction`,
    ///   …).
    ///
    /// This struct used to RE-DERIVE the second half in Swift (`statusLine()`,
    /// `trustIndicator()`, `errorBanner()`, `invalidEntryBadge()`,
    /// `loadProgress*()`), a hand-written twin of the Rust originals: one rule
    /// set in three languages, which had already drifted. the trust EXPLANATION
    /// (`trustIndicatorDetail`) existed only on desktop. It now reads a field
    /// instead of running a `switch` (task
    /// `mobile-chrome-presentation-from-one-derivation`, `docs/adr/0011`), so a
    /// new trust posture or pipeline phase reaches iOS with no Swift change at
    /// all.
    ///
    /// Adding a display rule here is therefore a REGRESSION, not a feature: the
    /// rule belongs in `werust-core` beside its siblings, where every edge gets it
    /// (a source-shape guard,
    /// `crates/werust-core/tests/mobile_chrome_presentation_shape.rs`, reds the
    /// gate if a twin comes back).
    struct Chrome {
        let url: String
        let loadState: String
        let loading: Bool
        let loadStep: String
        let canGoBack: Bool
        let canGoForward: Bool
        let trustPosture: String
        let error: String?
        let failureKind: String?
        let retryable: Bool
        let invalidEntry: String?

        /// The one-line status shown in the footer: the core's `status_line` (a
        /// failure wins, else a loading indicator NAMING the real pipeline step,
        /// else idle).
        let statusLine: String
        /// The short trust-indicator badge: the core's `trust_indicator`, from the
        /// posture of the ACTUAL load path, neutral while a load is in flight.
        let trustIndicator: String
        /// The longer EXPLANATION of what the current `trustIndicator` means: the
        /// core's `trust_indicator_detail`, the same sentence desktop shows on
        /// hover. iOS has no hover, so the controller surfaces it as the badge's
        /// accessibility label plus a tap affordance (task
        /// `mobile-chrome-presentation-from-one-derivation`). For months this text
        /// reached desktop only, which is exactly the drift the collapse closes.
        let trustIndicatorDetail: String
        /// Whether the PROMINENT in-view error banner shows: the core's
        /// `error_banner_visible` (exactly when the last load failed).
        let errorBannerVisible: Bool
        /// The banner's text: the core's `error_banner_text`, i.e. the accurate,
        /// protocol-named reason, framed as a retryable timeout or a hard failure.
        let errorBannerText: String
        /// Whether the small "invalid URL" badge shows: the core's
        /// `invalid_entry_badge_visible`.
        let invalidEntryBadgeVisible: Bool
        /// The invalid-entry badge's text: the core's `invalid_entry_badge_text`.
        let invalidEntryBadgeText: String
        /// Whether there is a load to indicate at all: the core's
        /// `load_progress_visible` (a backend load in flight OR a pinned
        /// pre-content resolution step).
        let loadProgressVisible: Bool
        /// The load-progress FRACTION, 0.0-1.0: the core's
        /// `load_progress_fraction`, in the core's unit (`UIProgressView` takes a
        /// `Float` on the same 0-1 scale, so only the numeric type is narrowed).
        let loadProgressFraction: Double
        /// The phase NAME behind the current progress, for the progress line's
        /// accessibility label: the core's `load_progress_hint`.
        let loadProgressHint: String

        /// The FAIL-SOFT fallback for an unreadable/absent document (a freed
        /// session, a null C string): the facts read as an idle chrome and every
        /// DERIVED string is EMPTY.
        ///
        /// Empty rather than a Swift copy of the core's wording, deliberately: a
        /// second spelling of "⚠ unverified origin" here would be a new twin of the
        /// very rule this type stopped re-deriving. Showing nothing is also the
        /// honest claim when the chrome could not be read at all: a trust badge
        /// must never be asserted from a fallback (`docs/adr/0006`).
        static let idle = Chrome(
            url: "", loadState: "idle", loading: false, loadStep: "idle",
            canGoBack: false, canGoForward: false,
            trustPosture: "unverified-origin", error: nil,
            failureKind: nil, retryable: false, invalidEntry: nil,
            statusLine: "", trustIndicator: "", trustIndicatorDetail: "",
            errorBannerVisible: false, errorBannerText: "",
            invalidEntryBadgeVisible: false, invalidEntryBadgeText: "",
            loadProgressVisible: false, loadProgressFraction: 0, loadProgressHint: "")

        static func fromJSON(_ json: String) -> Chrome {
            guard let data = json.data(using: .utf8),
                  let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return .idle }
            return Chrome(
                url: o["url"] as? String ?? "",
                loadState: o["loadState"] as? String ?? "idle",
                loading: o["loading"] as? Bool ?? false,
                loadStep: o["loadStep"] as? String ?? "idle",
                canGoBack: o["canGoBack"] as? Bool ?? false,
                canGoForward: o["canGoForward"] as? Bool ?? false,
                trustPosture: o["trustPosture"] as? String ?? "unverified-origin",
                error: o["error"] as? String,
                failureKind: o["failureKind"] as? String,
                retryable: o["retryable"] as? Bool ?? false,
                invalidEntry: o["invalidEntry"] as? String,
                // The DERIVED half. Each default is the EMPTY/absent value, never a
                // Swift copy of the core's wording, for the same reason `.idle` is
                // empty: a document that somehow lacked a derived field must show
                // nothing rather than a second, drifting spelling of it.
                statusLine: o["statusLine"] as? String ?? "",
                trustIndicator: o["trustIndicator"] as? String ?? "",
                trustIndicatorDetail: o["trustIndicatorDetail"] as? String ?? "",
                errorBannerVisible: o["errorBannerVisible"] as? Bool ?? false,
                errorBannerText: o["errorBannerText"] as? String ?? "",
                invalidEntryBadgeVisible: o["invalidEntryBadgeVisible"] as? Bool ?? false,
                invalidEntryBadgeText: o["invalidEntryBadgeText"] as? String ?? "",
                loadProgressVisible: o["loadProgressVisible"] as? Bool ?? false,
                loadProgressFraction: o["loadProgressFraction"] as? Double ?? 0,
                loadProgressHint: o["loadProgressHint"] as? String ?? "")
        }
    }
}
