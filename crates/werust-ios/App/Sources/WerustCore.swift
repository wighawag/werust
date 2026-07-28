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

        static let idle = Chrome(
            url: "", loadState: "idle", loading: false, loadStep: "idle",
            canGoBack: false, canGoForward: false,
            trustPosture: "unverified-origin", error: nil,
            failureKind: nil, retryable: false, invalidEntry: nil)

        /// The one-line status the controller shows: a failure wins, else a loading
        /// indicator that NAMES the real pipeline step (resolving name / fetching
        /// record / fetching content / rendering) so a slow load reads as working,
        /// not frozen, else idle. The step hint is the core's `loadStep` (driven by
        /// the actual lifecycle), the SAME fact desktop reads (task
        /// `clearer-loading-and-error-indicator`).
        func statusLine() -> String {
            if let error = error { return "failed: \(error)" }
            guard loading else { return "idle" }
            let hint = loadStepHint()
            return hint.isEmpty ? "loading…" : "loading… — \(hint)"
        }

        /// The short human-readable hint for the current pipeline step, or empty
        /// for no step (idle). Mirrors the core's `LoadStep::hint`, so the mobile
        /// status text matches desktop.
        private func loadStepHint() -> String {
            switch loadStep {
            case "resolving-name": return "resolving name"
            case "fetching-record": return "fetching record"
            case "fetching-content": return "fetching content"
            case "rendering": return "rendering"
            default: return ""
            }
        }

        /// The short trust-indicator badge the controller paints from the core's
        /// posture (the ACTUAL load path, not the URL) — the SAME states the
        /// desktop chrome shows. Never labels a name-resolved or mutable page
        /// "verified" (only a direct `ipfs://<cid>` earns that).
        ///
        /// While a load is IN FLIGHT (`loading`) the indicator is a NEUTRAL loading
        /// state that WINS over the posture, making NO trust claim — the
        /// trust-honesty fix (task `chrome-loading-state-resets-trust-indicator`):
        /// on navigation to a possibly differently-trusted page, the indicator must
        /// not keep asserting the previous page's (or a not-yet-proven) trust while
        /// the new page loads; the real posture appears only once the load settles.
        /// The SAME loading-wins rule the desktop chrome applies, from the SAME
        /// `loading` fact.
        func trustIndicator() -> String {
            if loading { return "⋯ loading…" }
            switch trustPosture {
            case "content-verified": return "✓ verified"
            case "name-via-trusted-rpc": return "◈ name via trusted RPC"
            case "mutable-name": return "◇ content verified, mutable name"
            default: return "⚠ unverified origin"
            }
        }

        /// Whether the PROMINENT in-view error banner should be shown: exactly when
        /// the last load failed (`error` is set). The whole point of fail-closed is
        /// that the user UNDERSTANDS why nothing rendered; the subtle footer status
        /// was "not easily seen" (a real `ronan.eth` IPNS failure was missed), so a
        /// failed load ALSO raises a high-contrast banner the user cannot miss. The
        /// SAME rule desktop/Android apply, from the SAME chrome-JSON fact.
        func errorBannerVisible() -> Bool { error != nil }

        /// The PROMINENT error-banner text for a failed load: the accurate,
        /// protocol-named reason drawn straight from `error` (the resolver/decoder
        /// taxonomy — e.g. "IPNS record did not verify: …"), never a generic
        /// "failed". Empty when there is no failure (the banner is hidden then).
        func errorBanner() -> String {
            guard let error = error else { return "" }
            // A TRANSIENT/timeout failure (retryable) is surfaced DISTINCTLY from a
            // hard failure: a softer "timed out — reload to retry" (the Reload
            // button IS the retry — a failed ENS load re-resolves), while a hard
            // failure keeps the prominent "failed to load" wording + its
            // protocol-named reason. The distinction is the core's `retryable`
            // (task `clearer-loading-and-error-indicator`), the SAME fact desktop
            // reads, so the two never disagree.
            if retryable { return "⏳ This page timed out — reload to retry: \(error)" }
            return "⚠ This page failed to load: \(error)"
        }

        /// Whether the surfaced failure is RETRYABLE (a transient timeout a reload
        /// may fix), so the controller can show a retry affordance. `false` for a
        /// hard failure or when nothing failed. The core's `retryable` fact,
        /// matching desktop.
        func errorIsRetryable() -> Bool { error != nil && retryable }

        /// Whether the small "invalid URL" BADGE should be shown: exactly when the
        /// last URL-bar entry was INVALID (a scheme-less garbage entry that did not
        /// navigate). A pure read of the orthogonal `invalidEntry` fact — distinct
        /// from a load error (`error`) — so the controller paints the badge + the
        /// red-underlined URL bar from the SAME chrome-JSON fact desktop uses (field
        /// finding D, task `scheme-less-entry-https-fallback-and-keep-bar-on-error`).
        func invalidEntryVisible() -> Bool { invalidEntry != nil }

        /// The small "invalid URL" badge text for an invalid entry, empty otherwise
        /// (the badge is hidden then). Matches desktop's badge wording.
        func invalidEntryBadge() -> String { invalidEntry != nil ? "⛔ invalid URL" : "" }

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
                invalidEntry: o["invalidEntry"] as? String)
        }
    }
}
