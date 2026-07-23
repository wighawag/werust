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
            return .success(mimeType: mime, body: body)
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
            return .success(mimeType: mime, body: body)
        } else {
            let reason = werust_ios_resolution_error(res).map { c -> String in
                defer { werust_ios_string_free(c) }
                return String(cString: c)
            } ?? "settings request failed"
            return .failure(reason: reason)
        }
    }

    /// The outcome of resolving an intercepted `ipfs://` request through the core:
    /// verified bytes + MIME type, or a fail-closed reason. Mirrors the Rust
    /// `SchemeResolution`; the `WKURLSchemeHandler` turns `.success` into a
    /// `URLResponse` + data on the task and `.failure` into `didFailWithError`,
    /// so the desktop fail-closed posture holds on iOS. Also serves the internal
    /// `werust://settings` page (via [applySettings]): the `.success` bytes are the
    /// page HTML, the `.failure` reason a fail-closed host error.
    enum Resolution {
        case success(mimeType: String, body: Data)
        case failure(reason: String)
    }

    /// Report the platform `WKWebView`'s commit signal into the core.
    func onPageCommitted(_ url: String) {
        url.withCString { werust_ios_on_page_committed(handle, $0) }
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    func onPageFinished(_ url: String) {
        url.withCString { werust_ios_on_page_finished(handle, $0) }
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

    /// The chrome the controller paints, decoded from the core's JSON wire form.
    /// Every field is the core's truth; the controller holds none of this logic.
    struct Chrome {
        let url: String
        let loadState: String
        let loading: Bool
        let canGoBack: Bool
        let canGoForward: Bool
        let error: String?

        static let idle = Chrome(
            url: "", loadState: "idle", loading: false,
            canGoBack: false, canGoForward: false, error: nil)

        /// The one-line status the controller shows: a failure wins, else
        /// loading/idle.
        func statusLine() -> String {
            if let error = error { return "failed: \(error)" }
            return loading ? "loading…" : "idle"
        }

        static func fromJSON(_ json: String) -> Chrome {
            guard let data = json.data(using: .utf8),
                  let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return .idle }
            return Chrome(
                url: o["url"] as? String ?? "",
                loadState: o["loadState"] as? String ?? "idle",
                loading: o["loading"] as? Bool ?? false,
                canGoBack: o["canGoBack"] as? Bool ?? false,
                canGoForward: o["canGoForward"] as? Bool ?? false,
                error: o["error"] as? String)
        }
    }
}
