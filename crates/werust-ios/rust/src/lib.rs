//! The werust **iOS core**: the Rust core cross-compiled as a C-ABI static
//! library the iOS app links and drives from its Swift OS edge.
//!
//! This crate is the iOS realisation of "the browsing logic stays in the Rust
//! core behind the seams" (`CONTEXT.md`, the `mobile-ios-shell-and-static-lib`
//! task) — the twin of the Android core crate, swapping JNI for a plain C-ABI.
//! It builds a `staticlib` (`libwerust_mobile.a`) that the Xcode app links with
//! `-force_load` and drives over a small `extern "C"` surface (declared for Swift
//! in the bridging header `Sources/werust_mobile.h`). The core itself is the
//! SHARED [`werust_core`] crate the desktop GTK shell and the Android app use;
//! the only iOS-specific pieces are:
//!
//! * [`IosBackend`] — the [`Renderer`](renderer::Renderer) seam backend over the
//!   platform `WKWebView` (the iOS system webview), driven from Swift across the
//!   C-ABI. It owns the browsing LOGIC (session history, load lifecycle, chrome),
//!   so Swift stays confined to the OS edge.
//! * [`CoreSession`] — a thin, ObjC-free wrapper binding a
//!   [`BrowserShell`](werust_core::BrowserShell) over an [`IosBackend`] with the
//!   WebView-signal callbacks, so the whole session is testable with ordinary
//!   `cargo test` (no UIKit / no simulator), and the C-ABI layer is a mechanical
//!   marshalling shim.
//! * The `werust_ios_*` C-ABI exports — the mechanical bridge Swift calls.
//!
//! # The Swift ↔ core protocol
//!
//! One [`CoreSession`] per `UIViewController`. On a user action (typed URL, Back,
//! Forward, Reload, Stop) Swift drives the session, then reads back:
//!
//! * [`take_pending_load`](CoreSession::take_pending_load) — the URL (if any) to
//!   apply to the platform `WKWebView` via `WKWebView.load`.
//! * [`chrome_json`](CoreSession::chrome_json) — the [`ChromeState`] as JSON to
//!   paint the URL bar, the Back/Forward/Reload/Stop enablement, and the status
//!   line. Swift holds NO browsing logic; every one of those is the core's truth.
//!
//! And Swift reports the platform `WKWebView`'s real load signals back in
//! ([`on_page_committed`](CoreSession::on_page_committed) /
//! [`on_page_finished`](CoreSession::on_page_finished) /
//! [`on_page_failed`](CoreSession::on_page_failed)), which the core folds into
//! the chrome exactly as the desktop pump folds WebKitGTK's signals.

mod backend;
mod ffi_json;

pub use backend::{IosBackend, IosHandle};

use renderer::Renderer;
use werust_core::{BrowserShell, ChromeState};

/// The wire form of a resolved `ipfs://` request handed back to the Swift edge:
/// the MIME type and the verified bytes, or the fail-closed reason.
///
/// The Swift `WKURLSchemeHandler` turns an [`Ok`] into a `URLResponse` + data on
/// the `WKURLSchemeTask` and a [`Err`] into `didFailWithError`, so the
/// fail-closed posture desktop has (a hash mismatch fails the load, never
/// renders) holds on iOS too.
pub enum SchemeResolution {
    /// A verified resolution: the MIME type, the verified body bytes, and the
    /// HTTP-equivalent status to answer with.
    ///
    /// `status` is 200 for an ordinary resource. It is carried because a site's
    /// IPFS `_redirects` rules (IPIP-0002) can name its OWN error page for a
    /// path that is not in its DAG (`/* /404.html 404`), which a gateway serves
    /// WITH a not-found status; reporting 200 there would lie about a page the
    /// site declared missing.
    Ok {
        mime_type: String,
        body: Vec<u8>,
        status: u16,
    },
    /// A fail-closed resolution failure carrying its legible reason.
    Err { reason: String },
}

/// A single browsing session for one iOS `UIViewController`: a
/// [`BrowserShell`](werust_core::BrowserShell) over an [`IosBackend`], plus the
/// WebView-signal callbacks Swift reports into.
///
/// This is the ObjC-free heart the C-ABI layer wraps: every method here is plain
/// Rust so the Swift↔core protocol is exercised by `cargo test` without UIKit,
/// and the `werust_ios_*` exports below are a mechanical marshalling shim over it.
pub struct CoreSession {
    shell: BrowserShell,
    /// A handle to the shell's [`IosBackend`]'s shared state, for the
    /// platform-`WKWebView` protocol (pending-load + load signals) that the
    /// cross-backend seam does not carry.
    backend: IosHandle,
}

impl Default for CoreSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreSession {
    /// Build a fresh session over an [`IosBackend`], with the native `ipfs://`
    /// scheme handler installed.
    #[must_use]
    pub fn new() -> Self {
        let mut backend = IosBackend::new();
        let handle = backend.handle();
        // Wire the SECOND trust hook exactly as the desktop backend's
        // `install_ipfs` does, BEFORE handing the backend to the shell: register
        // the `ipfs` scheme handler routing each intercepted request through the
        // SAME `werust_core::ipfs::resolve_ipfs_request` path desktop uses, over
        // the default trustless-gateway CAR retriever (per-block hash-verified).
        // A `WKWebView` loads `ipfs://` only via a registered `WKURLSchemeHandler`,
        // so Swift's handler drives the intercepted request through
        // [`resolve_ipfs`](CoreSession::resolve_ipfs) into this handler.
        // `install_ipfs` hands back the `_redirects` 3xx redirect sink: a matched
        // 3xx rule (IPIP-0002) is a NAVIGATION the scheme handler cannot perform,
        // so it queues the `ipfs://<rootcid><to>` target there and the shell drains
        // it on its pump, surfacing the target as an ordinary pending load the
        // platform webview performs (bar + history move, target hash-verified by the
        // fresh retrieval it triggers). Task `ipfs-redirects-3xx-navigation-support`.
        let redirects = install_ipfs(&mut backend);
        // Wire the FIRST trust hook exactly as the desktop backend's
        // `install_provider` does: register the EIP-1193 provider bridge handler
        // and inject the page-side provider shim at document start, both routed
        // through the SAME `werust_core::provider` path desktop uses. The platform
        // `WKWebView` bridges the channel (a `WKScriptMessageHandler`) and runs the
        // shim + the response push, driving
        // [`handle_provider_message`](CoreSession::handle_provider_message).
        install_provider(&mut backend);
        // Wire the iOS CONSOLE + best-effort NETWORK capture points that feed the
        // in-app debug menu (task `debug-console-network-capture-per-platform`).
        // The store is created HERE and shared with the shell below, so the capture
        // handler and the debug view are the SAME store.
        let debug = werust_core::debug::DebugCapture::new();
        install_debug_capture(&mut backend, debug.clone());
        Self {
            shell: BrowserShell::new(Box::new(backend))
                .with_redirect_sink(redirects)
                .with_debug_capture(debug),
            backend: handle,
        }
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam. The core
    /// front door routes the RAW entry (Swift passes the typed text verbatim):
    /// a bare `.eth` -> ENS; a scheme-less valid host -> `https://` prepend; an
    /// explicit scheme -> literal; an INVALID entry -> the distinct invalid-URL
    /// state (a badge + red-underlined bar, the typed text kept, no navigation).
    /// Returns `true` when the front door handled the entry without erroring
    /// (including an invalid entry, which is handled and surfaced, not a load); an
    /// invalid entry queues NO pending load, so nothing is fed to the WKWebView.
    pub fn navigate(&mut self, url: &str) -> bool {
        self.shell.navigate(url).is_ok()
    }

    /// Go one step back in session history, through the seam.
    pub fn go_back(&mut self) {
        self.shell.go_back();
    }

    /// Go one step forward in session history, through the seam.
    pub fn go_forward(&mut self) {
        self.shell.go_forward();
    }

    /// Reload the current page, through the seam. Returns `true` on success.
    pub fn reload(&mut self) -> bool {
        self.shell.reload().is_ok()
    }

    /// Stop the in-flight load, through the seam.
    pub fn stop(&mut self) {
        self.shell.stop();
    }

    /// The URL (if any) the core has committed to but the platform `WKWebView`
    /// has not yet loaded. Swift drains this after driving the session and calls
    /// `WKWebView.load` with it.
    pub fn take_pending_load(&mut self) -> Option<String> {
        self.backend.take_pending_load()
    }

    /// Resolve an intercepted `ipfs://<cid>[/path]` request through the SHARED
    /// core resolve path, for Swift's `WKURLSchemeHandler`.
    ///
    /// A `WKWebView` will only load `ipfs://` if a `WKURLSchemeHandler` is
    /// registered, so Swift's handler calls this: it routes `uri` through the
    /// `ipfs` scheme handler installed at [`new`](CoreSession::new) (the SAME
    /// `resolve_ipfs_request` + trustless-gateway CAR path desktop uses), and
    /// returns the verified bytes + MIME type, or the fail-closed reason. Returns
    /// `None` if `uri` is not a registered scheme.
    pub fn resolve_ipfs(&self, uri: &str) -> Option<SchemeResolution> {
        // Only the `ipfs` scheme's success is a hash-verified content load that
        // earns the content-verified posture. Swift routes `ipfs://` here via its
        // dedicated `ipfs` `WKURLSchemeHandler` (and `werust://settings` through
        // the separate `apply_settings`, which does NOT mark), but scope the mark
        // to the `ipfs` scheme defensively so an internal chrome page can never be
        // mis-marked content-verified.
        let is_ipfs = uri
            .split_once("://")
            .is_some_and(|(scheme, _)| scheme == werust_core::ipfs::IPFS_SCHEME);
        self.backend.resolve_scheme(uri).map(|result| match result {
            Ok(response) => {
                if is_ipfs {
                    // The bytes verified against their CID on the shared core path:
                    // mark the current load content-verified so the chrome's trust
                    // indicator reflects the REAL (hash-verified) load path — the
                    // same thing the desktop `install_ipfs` scheme handler does on
                    // success. A fail-closed error (below) never reaches this, so an
                    // unverified load is never marked verified. The two-axis flags
                    // (`mark_ens_origin`/`mark_mutable_name`) set by the ENS/IPNS
                    // front door then decide the honest posture surfaced.
                    self.backend.mark_content_verified();
                }
                SchemeResolution::Ok {
                    mime_type: response.mime_type,
                    body: response.body,
                    status: response.status,
                }
            }
            Err(e) => SchemeResolution::Err {
                reason: e.to_string(),
            },
        })
    }

    /// The scripts to inject at document start (the EIP-1193 provider shim), so
    /// Swift installs them onto the platform `WKWebView` as `WKUserScript`s at
    /// document start. This is the `inject_script` half of the provider bridge,
    /// made real on the iOS edge.
    #[must_use]
    pub fn document_start_scripts(&self) -> Vec<String> {
        self.backend.document_start_scripts()
    }

    /// Dispatch a page-posted EIP-1193 envelope on script-message channel `name`
    /// through the registered provider handler and return the response JS Swift
    /// must run in the live page (via `WKWebView.evaluateJavaScript`) to settle
    /// the page's pending Promise. Routed through the SAME `werust_core::provider`
    /// path desktop uses; `None`/empty means the channel is unregistered or the
    /// message needed no response.
    #[must_use]
    pub fn handle_provider_message(&self, name: &str, body: &str) -> Vec<String> {
        self.backend.handle_script_message(name, body)
    }

    /// Capture one envelope the injected debug shim posted up the capture channel
    /// (Swift's `WKScriptMessageHandler` for
    /// [`CAPTURE_BRIDGE`](werust_core::debug::CAPTURE_BRIDGE)), for the in-app
    /// debug menu's Console tab and the best-effort half of its Network tab.
    ///
    /// WKWebView has NO native console callback and no per-resource load callback,
    /// so iOS captures the console by injecting the SHARED
    /// [`console_shim`](werust_core::debug::console_shim) (byte-for-byte the one
    /// desktop injects) and its reachable network by injecting the
    /// [`network_shim`](werust_core::debug::network_shim) (`fetch`/`XHR` only — see
    /// the honestly-recorded coverage limits in
    /// `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`).
    ///
    /// The body is PAGE-CONTROLLED text (a hostile page can post on this channel
    /// directly), so it goes through the core's total, fail-quiet
    /// [`route_capture_message`](werust_core::debug::route_capture_message): an
    /// unreadable body is dropped, never fabricated into an entry, and a
    /// shim-reported request never claims verification.
    pub fn capture_script_message(&self, name: &str, body: &str) {
        // Routed through the registered script-message handler so the channel
        // name is respected (an unregistered channel captures nothing), exactly as
        // the provider channel is dispatched.
        let _ = self.backend.handle_script_message(name, body);
    }

    /// Capture one NETWORK request from an iOS point that CAN see it natively: the
    /// `WKURLSchemeHandler` custom-scheme tasks and the main-frame navigations the
    /// `WKNavigationDelegate` reports.
    ///
    /// These are the points where iOS knows the REAL outcome, so they are the only
    /// ones that may report a verified posture: `verified` must say whether THIS
    /// request's bytes actually came back through the hash-verified
    /// content-addressed path (a successful `ipfs://` resolution), never whether
    /// the URL looks content-addressed (ADR-0006).
    ///
    /// # Which row is the main document
    ///
    /// The main-document row additionally takes the LOAD's own two-axis posture,
    /// so the Network tab cannot contradict the chrome trust indicator on the same
    /// screen. `main_frame` says only that the CALLER natively knows this is the
    /// main document (the `WKNavigationDelegate`, which is handed the main frame's
    /// own URL); a `WKURLSchemeHandler` task carries no such flag, so it passes
    /// `false` and the decision is made HERE, by the core's ONE shared main-frame
    /// predicate ([`BrowserShell::is_main_frame`], driven by the top-level URL the
    /// shell reports into the `_redirects` sink on every navigation).
    ///
    /// Swift MUST NOT compare URLs itself. The obvious compare — the scheme-handler
    /// URL against `chrome().url` — is against the DISPLAY identity: on an ENS load
    /// the shell pins the name, so `url_text` is `ronan.eth` while the request is
    /// `ipfs://<cid>/…` and the compare never fires on exactly the page the
    /// reconciliation was mandated for (the tab would show `content-verified`
    /// beside a `name-via-trusted-rpc` indicator). The shared predicate normalizes
    /// through `frame_key`, so it also survives the authority-less `ipfs:///<cid>`
    /// form and a query/fragment.
    ///
    /// A `0` status/size means unknown and stays honestly absent in the store.
    // The flat argument list MIRRORS the C-ABI export Swift calls
    // (`werust_ios_capture_network`), which cannot take a Rust struct: grouping
    // them here would only move the same marshalling one layer down.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_network(
        &self,
        method: &str,
        url: &str,
        status: u16,
        mime: &str,
        size: u64,
        verified: bool,
        main_frame: bool,
    ) {
        let mut entry = werust_core::debug::network_entry(
            method,
            url,
            Some(status),
            mime,
            Some(size),
            verified,
            epoch_millis(),
        );
        if main_frame || self.shell.is_main_frame(url) {
            // Read the LIVE load posture, NOT the cached `chrome().trust_posture`
            // snapshot: this capture runs BEFORE `didCommit`/`didFinish` refresh
            // the chrome, so the cache still holds the stale pre-verify
            // `unverified-origin` here and would DOWNGRADE the main-document row
            // below the honest posture the indicator is about to show. Desktop
            // reads the same fact straight from its load lifecycle.
            entry = entry.with_trust(self.shell.live_trust_posture());
        }
        self.shell.debug_capture().push_network(entry);
    }

    /// Serve (and apply) an intercepted `werust://settings[?backend=…]` request
    /// through the SHARED core settings path, for Swift's `WKURLSchemeHandler`
    /// for the `werust` scheme.
    ///
    /// A `WKWebView` will only load a custom scheme like `werust://` if a
    /// `WKURLSchemeHandler` is registered for it, so Swift's handler calls this:
    /// it routes `uri` through the `werust` scheme handler installed at
    /// [`new`](CoreSession::new) (the SAME
    /// [`apply_settings_request`](werust_core::retrieval::apply_settings_request)
    /// path desktop and Android use), which renders the retrieval-backend settings
    /// page and PERSISTS a `?backend=…` selection. It returns the page HTML +
    /// MIME type, or the fail-closed reason (a non-`settings` host). Returns `None`
    /// if `uri` is not the registered `werust` scheme (Swift then lets the
    /// `WKWebView` handle it normally).
    ///
    /// This is the twin of [`resolve_ipfs`](CoreSession::resolve_ipfs): both go
    /// through the same generic [`IosHandle::resolve_scheme`] dispatch, but they
    /// are kept as DISTINCT edge methods so the Swift shell registers a
    /// `WKURLSchemeHandler` per scheme (`ipfs` and `werust`) and each is honestly
    /// named for what it serves — the requeue's Gate-2 fix (the `werust` scheme
    /// was dead on iOS because no Swift handler dispatched it).
    pub fn apply_settings(&self, uri: &str) -> Option<SchemeResolution> {
        self.backend.resolve_scheme(uri).map(|result| match result {
            Ok(response) => SchemeResolution::Ok {
                mime_type: response.mime_type,
                body: response.body,
                status: response.status,
            },
            Err(e) => SchemeResolution::Err {
                reason: e.to_string(),
            },
        })
    }

    /// Report the platform `WKWebView`'s commit signal into the core, then fold
    /// the resulting lifecycle events into the chrome.
    pub fn on_page_committed(&mut self, url: &str) {
        self.backend.on_page_committed(url);
        self.shell.pump();
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    pub fn on_page_finished(&mut self, url: &str) {
        self.backend.on_page_finished(url);
        self.shell.pump();
    }

    /// Report a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
    /// client-side navigation) into the core, then fold the resulting
    /// `UrlChanged` event into the chrome so the URL bar FOLLOWS the new location
    /// (dropping a pinned `.eth` name / re-deriving an ENS identity) instead of
    /// freezing. Called from Swift's KVO observer on `webView.url`.
    pub fn on_url_changed(&mut self, url: &str) {
        self.backend.on_url_changed(url);
        self.shell.pump();
    }

    /// Report the platform `WKWebView`'s error signal into the core.
    pub fn on_page_failed(&mut self, url: &str, reason: &str) {
        self.backend.on_page_failed(url, reason);
        self.shell.pump();
    }

    /// The current [`ChromeState`] the Swift edge paints its URL bar, nav-control
    /// enablement, and status line from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        self.shell.chrome()
    }

    /// The current [`ChromeState`] as a JSON object, the wire form Swift reads
    /// across the C-ABI (a single string return is the simplest robust FFI
    /// marshalling).
    #[must_use]
    pub fn chrome_json(&self) -> String {
        ffi_json::chrome_to_json(self.shell.chrome())
    }

    /// The shared bounded CONSOLE + NETWORK capture store behind the in-app debug
    /// menu ([`werust_core::debug::DebugCapture`]).
    ///
    /// Both the PUSH surface the iOS capture points feed (an injected `console.*`
    /// user-script plus the reachable network points: the custom-scheme handler,
    /// main-frame navigation, and a best-effort fetch/XHR script; task
    /// `debug-console-network-capture-per-platform`) and the store the debug view
    /// reads/clears. It is the SHELL's store, so the Swift debug view renders
    /// exactly what desktop and Android render.
    #[must_use]
    pub fn debug_capture(&self) -> &werust_core::debug::DebugCapture {
        self.shell.debug_capture()
    }

    /// The capture store as its own JSON document, the wire form Swift's debug
    /// view reads across the C-ABI: a DEDICATED accessor beside
    /// [`chrome_json`](CoreSession::chrome_json) rather than a section of the
    /// chrome JSON, so the chrome (re-encoded on every refresh) stays lean and
    /// every existing chrome reader is unaffected. The twin of the Android core's
    /// `debug_json`.
    #[must_use]
    pub fn debug_json(&self) -> String {
        self.shell.debug_json()
    }
}

/// werust's version string for the Swift edge's browser MENU: the ONE shared
/// source ([`werust_core::version`], resolved once at build time from the release
/// tag / `git describe` / the Cargo version), so the iOS menu shows exactly what
/// the desktop popover and the Android menu show.
///
/// SESSION-FREE on purpose (unlike every accessor above): the version and the
/// menu are properties of the BUILD, not of a browsing session, so the Swift edge
/// can show them without a live native session, and no `CoreSession` is borrowed
/// to read a constant. The twin of the Android core's `version`; the recorded
/// rationale is in
/// `docs/spikes/general-browser-menu-with-version-and-debug-entry/DECISIONS.md`.
#[must_use]
pub fn version() -> &'static str {
    werust_core::version()
}

/// The general browser MENU as the JSON document the Swift edge builds its native
/// `UIMenu` from ([`werust_core::menu::menu_json`]): the version line plus the
/// Debug entry, each with its stable id and kind.
///
/// Session-free for the same reason as [`version`]. The Swift edge renders
/// whatever items this lists, so a FUTURE menu item added in `werust-core`
/// appears on iOS with no Swift change.
#[must_use]
pub fn menu_json() -> String {
    werust_core::menu::menu_json(&werust_core::menu::BrowserMenu::new())
}

/// Install the native `ipfs://` scheme handler on `backend`, the twin of the
/// desktop backend's `install_ipfs`.
///
/// It registers the `ipfs` scheme through the seam's
/// [`register_scheme_handler`](Renderer::register_scheme_handler) and routes each
/// intercepted request through the pure
/// [`resolve_ipfs_request`](werust_core::ipfs::resolve_ipfs_request) resolver,
/// backed by the default
/// [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever) (fetch
/// the DAG blocks as a CAR from a trustless gateway over the bound HTTP fetcher,
/// verify EACH block against its own CID, reassemble/traverse the UnixFS DAG
/// client-side). The gateway is UNTRUSTED; the per-block verify is what makes the
/// load safe, so a hash mismatch fails the load rather than rendering unverified
/// bytes. This is the SAME core path desktop uses — mobile does not fork it.
///
/// Unlike desktop, the mobile backend does not own a native webview, so the
/// handler is dispatched by the OS edge (Swift's `WKURLSchemeHandler`) via
/// [`CoreSession::resolve_ipfs`], not by a webview signal.
fn install_ipfs(backend: &mut IosBackend) -> werust_core::ipfs::RedirectSink {
    use fetcher::{HttpFetcher, TrustlessGatewayCarRetriever};
    use werust_core::ipfs::{resolve_ipfs_request, RedirectSink, IPFS_SCHEME};
    use werust_core::retrieval::{active_gateway_endpoint, apply_settings_request, WERUST_SCHEME};

    // Point the retriever at the USER'S CHOSEN retrieval backend (persisted via
    // `werust://settings`): a custom gateway/local-node URL if picked, else the
    // default public trustless gateway. The same core switch desktop uses (task
    // `retrieval-backend-user-setting`); the per-block verify is unchanged.
    let retriever =
        TrustlessGatewayCarRetriever::with_gateway(HttpFetcher::new(), &active_gateway_endpoint());
    // The `_redirects` 3xx hand-off, shared between the handler (which pushes a
    // redirect target) and the shell (which drains it and navigates). Cloned into
    // the handler, returned to the caller: both clones are the SAME sink.
    let redirects = RedirectSink::new();
    let redirects_for_handler = redirects.clone();
    backend.register_scheme_handler(
        IPFS_SCHEME,
        Box::new(move |request| resolve_ipfs_request(&retriever, &request, &redirects_for_handler)),
    );
    // The internal `werust://settings` page, resolved through the SAME scheme
    // seam so Swift's `WKURLSchemeHandler` for `werust` serves it and a
    // `?backend=…` selection is applied + persisted by the shared core.
    backend.register_scheme_handler(
        WERUST_SCHEME,
        Box::new(|request| apply_settings_request(&request)),
    );
    redirects
}

/// Install the native EIP-1193 provider bridge on `backend`, the twin of the
/// desktop backend's `install_provider`.
///
/// It registers the provider script-message channel through the seam's
/// [`register_script_message_handler`](Renderer::register_script_message_handler)
/// and injects the page-side provider shim at document start through
/// [`inject_script`](Renderer::inject_script), both routed through the SAME
/// `werust_core::provider` path desktop uses (`provider_shim` /
/// `route_provider_message` / the keyless read-only [`ProviderBridge`]). So a
/// page's `window.ethereum` sees the SAME injected EIP-1193 provider on iOS as on
/// desktop.
///
/// Unlike desktop (which pushes the response by evaluating JS on its GTK loop),
/// the mobile backend owns no live view, so the handler QUEUES the response JS
/// via the backend's `Send` eval sink; Swift drains it (through
/// [`CoreSession::handle_provider_message`]) and runs it with
/// `WKWebView.evaluateJavaScript`. The bridge holds NO keys (a read-only stub),
/// the same security posture as desktop.
fn install_provider(backend: &mut IosBackend) {
    use werust_core::provider::{
        provider_shim, route_provider_message, ProviderBridge, PROVIDER_BRIDGE,
    };

    // The response-push sink is JUST the backend's eval queue (a `Send`
    // `Arc<Mutex<_>>` clone — the mobile twin of the desktop `evaluate_javascript`
    // capturing a cloneable view handle), so the seam's `Send`
    // `ScriptMessageHandler` can own it without capturing the `!Send` backend
    // handle. Each page envelope's response-delivery JS is queued for the OS edge
    // to run via `handle_provider_message`.
    let eval_sink = backend.handle().eval_sink();
    let bridge = ProviderBridge::new();
    backend.register_script_message_handler(
        PROVIDER_BRIDGE,
        Box::new(move |message| {
            route_provider_message(&bridge, &message, &mut |script| {
                if let Ok(mut queue) = eval_sink.lock() {
                    queue.push(script);
                }
            });
        }),
    );
    // Make the provider detectable from document start, exactly as desktop does.
    backend.inject_script(&provider_shim());
}

/// Milliseconds since the Unix epoch, for a captured debug entry's timestamp
/// (`0` if the clock is before the epoch).
///
/// The core store takes a caller-supplied timestamp so it binds no clock (the
/// store's DECISIONS.md Decision 6); this is the iOS edge's supply, taken here
/// rather than in Swift so all three edges stamp entries the same way.
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Install the iOS CONSOLE + best-effort NETWORK capture points on `backend`, the
/// twin of the desktop backend's `install_debug_capture` (task
/// `debug-console-network-capture-per-platform`).
///
/// # Why iOS injects a shim (and why its network coverage is honestly partial)
///
/// WKWebView exposes NO console callback and NO per-resource load callback, so
/// the only page-wide reach iOS has is INJECTED JS:
///
/// * CONSOLE: the SHARED [`console_shim`](werust_core::debug::console_shim) — the
///   byte-for-byte same string desktop injects, from ONE place in `werust-core`,
///   so the two shim platforms cannot drift. It chains to the original
///   `console.*`, so the page's console and Safari's remote inspector are
///   unchanged.
/// * NETWORK: the [`network_shim`](werust_core::debug::network_shim), a
///   best-effort `fetch`/`XHR` wrapper. It sees only requests the PAGE makes
///   through those APIs — NOT the browser-internal subresource loads (`<img>`,
///   `<script>`, CSS `url()`, navigation preloads). Those gaps are covered as far
///   as iOS allows by the NATIVE points Swift drives through
///   [`CoreSession::capture_network`] (the `WKURLSchemeHandler` custom-scheme
///   tasks and the `WKNavigationDelegate` main-frame navigations), and what
///   remains uncovered is recorded honestly in
///   `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`
///   rather than silently presented as complete.
///
/// Desktop does NOT inject the network shim: its resource-load signals already
/// see every resource, so it would only double-record a subset.
///
/// Both shims post on the DEDICATED
/// [`CAPTURE_BRIDGE`](werust_core::debug::CAPTURE_BRIDGE) channel (never the
/// EIP-1193 provider's trust channel), and the registered handler routes each
/// body through the core's total, fail-quiet
/// [`route_capture_message`](werust_core::debug::route_capture_message). Capture
/// is READ-ONLY observation: nothing here answers a request, alters a load, or
/// changes a trust posture.
fn install_debug_capture(backend: &mut IosBackend, capture: werust_core::debug::DebugCapture) {
    use werust_core::debug::{console_shim, network_shim, route_capture_message, CAPTURE_BRIDGE};

    backend.register_script_message_handler(
        CAPTURE_BRIDGE,
        Box::new(move |message| route_capture_message(&capture, &message.body)),
    );
    // Document-start user scripts, so a page's very first `console.log` and its
    // earliest `fetch` are captured.
    backend.inject_script(&console_shim());
    backend.inject_script(&network_shim());
}

// ---------------------------------------------------------------------------
// C-ABI export surface. These are the mechanical `werust_ios_*` bridges the
// Swift shell calls (declared in `Sources/werust_mobile.h`, imported as the
// project's bridging header). They only marshal to/from C and delegate to
// `CoreSession`; all logic lives above so it is UIKit-free-testable.
//
// The session is handed to Swift as an opaque `*mut CoreSession` from
// `werust_ios_session_new`, threaded back through every call, and freed by
// `werust_ios_session_free`. Strings cross the boundary as NUL-terminated
// `char *`: inputs are borrowed C strings (Swift owns them); outputs are
// heap-allocated C strings the caller MUST return via `werust_ios_string_free`.
// ---------------------------------------------------------------------------
mod ffi {
    use super::CoreSession;
    use std::ffi::{c_char, CStr, CString};

    /// Reconstruct a `&mut CoreSession` from the opaque handle Swift threads back.
    ///
    /// # Safety
    /// `session` must be a pointer returned by `werust_ios_session_new` and not
    /// yet freed by `werust_ios_session_free`; Swift guarantees this by
    /// construction (one handle per `UIViewController`, threaded through every
    /// call on the main thread).
    unsafe fn session_mut<'a>(session: *mut CoreSession) -> Option<&'a mut CoreSession> {
        if session.is_null() {
            None
        } else {
            Some(&mut *session)
        }
    }

    /// Read a borrowed C string into an owned `String` (empty on null/invalid).
    ///
    /// # Safety
    /// `s`, if non-null, must point to a valid NUL-terminated C string the caller
    /// keeps alive for the duration of the call.
    unsafe fn read(s: *const c_char) -> String {
        if s.is_null() {
            String::new()
        } else {
            CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    /// Move a Rust `String` into a heap C string the caller frees via
    /// [`werust_ios_string_free`]. Returns null only on an interior-NUL string
    /// (never produced by our JSON/URL encoders).
    fn into_c_string(s: String) -> *mut c_char {
        match CString::new(s) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }

    /// Create a fresh browsing session; the returned pointer is threaded back
    /// through every call and freed with [`werust_ios_session_free`].
    #[no_mangle]
    pub extern "C" fn werust_ios_session_new() -> *mut CoreSession {
        Box::into_raw(Box::new(CoreSession::new()))
    }

    /// Free a session created by [`werust_ios_session_new`].
    ///
    /// # Safety
    /// `session` must be a live handle from `werust_ios_session_new`, freed at
    /// most once. A null pointer is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_session_free(session: *mut CoreSession) {
        if !session.is_null() {
            drop(Box::from_raw(session));
        }
    }

    /// Free a C string returned by any `werust_ios_*` function that returns
    /// `char *` (the chrome JSON and the pending-load URL).
    ///
    /// # Safety
    /// `s` must be a pointer returned by one of those functions, freed at most
    /// once. A null pointer is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_string_free(s: *mut c_char) {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }

    /// Navigate to `url` (the URL bar's Enter action). Returns `true` on success.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_navigate(
        session: *mut CoreSession,
        url: *const c_char,
    ) -> bool {
        let url = read(url);
        session_mut(session)
            .map(|s| s.navigate(&url))
            .unwrap_or(false)
    }

    /// Go one step back in the core's session history.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_go_back(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.go_back();
        }
    }

    /// Go one step forward in the core's session history.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_go_forward(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.go_forward();
        }
    }

    /// Reload the current page. Returns `true` on success.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_reload(session: *mut CoreSession) -> bool {
        session_mut(session).map(|s| s.reload()).unwrap_or(false)
    }

    /// Stop the in-flight load.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_stop(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.stop();
        }
    }

    /// The URL the core has committed to but the `WKWebView` has not yet loaded,
    /// as a heap C string, or null if nothing is pending. Free with
    /// [`werust_ios_string_free`].
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_take_pending_load(
        session: *mut CoreSession,
    ) -> *mut c_char {
        match session_mut(session).and_then(|s| s.take_pending_load()) {
            Some(url) => into_c_string(url),
            None => std::ptr::null_mut(),
        }
    }

    /// Resolve an intercepted `ipfs://` request through the shared core path.
    ///
    /// Returns an opaque handle to a boxed [`SchemeResolution`](super::SchemeResolution)
    /// Swift queries via `werust_ios_resolution_is_ok` / `_mime` / `_body` +
    /// `_body_len` / `_error`, then frees with `werust_ios_resolution_free`. A
    /// NULL return means the URI was not an intercepted scheme (Swift lets the
    /// `WKWebView` handle it normally). Bytes stay out of the JSON chrome wire
    /// form and cross as a `const uint8_t *` + length via `_body` / `_body_len`.
    ///
    /// # Safety
    /// `session` is a live handle; `uri` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolve_ipfs(
        session: *mut CoreSession,
        uri: *const c_char,
    ) -> *mut super::SchemeResolution {
        let uri = read(uri);
        match session_mut(session).and_then(|s| s.resolve_ipfs(&uri)) {
            Some(resolution) => Box::into_raw(Box::new(resolution)),
            None => std::ptr::null_mut(),
        }
    }

    /// Serve (and apply) an intercepted `werust://settings[?backend=…]` request
    /// through the shared core settings path, for Swift's `WKURLSchemeHandler` for
    /// the `werust` scheme.
    ///
    /// Returns an opaque handle to a boxed [`SchemeResolution`](super::SchemeResolution)
    /// Swift queries via the SAME `werust_ios_resolution_*` accessors the ipfs
    /// path uses (`_is_ok` / `_mime` / `_body` + `_body_len` / `_error`) and frees
    /// with `werust_ios_resolution_free`. A NULL return means the URI was not the
    /// `werust` scheme (Swift lets the `WKWebView` handle it normally). This is the
    /// requeue's Gate-2 iOS fix: the export the Swift `werust` scheme handler calls
    /// so `werust://settings` is actually reachable on iOS (the `werust` scheme was
    /// registered on the Rust side but had no Swift `WKURLSchemeHandler` to
    /// dispatch it, so the page was dead).
    ///
    /// # Safety
    /// `session` is a live handle; `uri` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_apply_settings(
        session: *mut CoreSession,
        uri: *const c_char,
    ) -> *mut super::SchemeResolution {
        let uri = read(uri);
        match session_mut(session).and_then(|s| s.apply_settings(&uri)) {
            Some(resolution) => Box::into_raw(Box::new(resolution)),
            None => std::ptr::null_mut(),
        }
    }

    /// Whether the resolution succeeded (a verified load) vs a fail-closed error.
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_is_ok(
        resolution: *const super::SchemeResolution,
    ) -> bool {
        matches!(
            resolution.as_ref(),
            Some(super::SchemeResolution::Ok { .. })
        )
    }

    /// The MIME type of a successful resolution as a heap C string (empty on an
    /// error result / null handle). Free with `werust_ios_string_free`.
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_mime(
        resolution: *const super::SchemeResolution,
    ) -> *mut c_char {
        let mime = match resolution.as_ref() {
            Some(super::SchemeResolution::Ok { mime_type, .. }) => mime_type.clone(),
            _ => String::new(),
        };
        into_c_string(mime)
    }

    /// A pointer to the verified body bytes of a successful resolution (null / 0
    /// length on an error result). The bytes are owned by the resolution handle
    /// and valid until `werust_ios_resolution_free`; Swift copies them into `Data`
    /// before freeing. Pair with `werust_ios_resolution_body_len`.
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_body(
        resolution: *const super::SchemeResolution,
    ) -> *const u8 {
        match resolution.as_ref() {
            Some(super::SchemeResolution::Ok { body, .. }) => body.as_ptr(),
            _ => std::ptr::null(),
        }
    }

    /// The length in bytes of the verified body (0 on an error result).
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_body_len(
        resolution: *const super::SchemeResolution,
    ) -> usize {
        match resolution.as_ref() {
            Some(super::SchemeResolution::Ok { body, .. }) => body.len(),
            _ => 0,
        }
    }

    /// The HTTP-equivalent status of a successful resolution (0 on an error
    /// result, which Swift fails the task on instead).
    ///
    /// Almost always 200. It is exposed so the Swift `WKURLSchemeHandler` can
    /// answer with the HONEST status when a site's `_redirects` (IPIP-0002) names
    /// its own error page for a path that is not in its DAG (`/* /404.html 404`)
    /// — the page renders, but as the not-found it is.
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_status(
        resolution: *const super::SchemeResolution,
    ) -> u16 {
        match resolution.as_ref() {
            Some(super::SchemeResolution::Ok { status, .. }) => *status,
            _ => 0,
        }
    }

    /// The fail-closed reason of an error resolution as a heap C string (empty on
    /// success / null handle). Free with `werust_ios_string_free`.
    ///
    /// # Safety
    /// `resolution` is a live handle from `werust_ios_resolve_ipfs`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_error(
        resolution: *const super::SchemeResolution,
    ) -> *mut c_char {
        let reason = match resolution.as_ref() {
            Some(super::SchemeResolution::Err { reason }) => reason.clone(),
            _ => String::new(),
        };
        into_c_string(reason)
    }

    /// Free a resolution handle from `werust_ios_resolve_ipfs`.
    ///
    /// # Safety
    /// `resolution` must be a handle from `werust_ios_resolve_ipfs`, freed at most
    /// once. A null pointer is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_resolution_free(resolution: *mut super::SchemeResolution) {
        if !resolution.is_null() {
            drop(Box::from_raw(resolution));
        }
    }

    /// The document-start scripts (the EIP-1193 provider shim) as a single heap C
    /// string Swift injects onto the platform `WKWebView` as a `WKUserScript` at
    /// document start. Free with [`werust_ios_string_free`]; an empty string means
    /// nothing to inject. They are all document-start scripts run in order, so
    /// concatenation is equivalent.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_document_start_script(
        session: *mut CoreSession,
    ) -> *mut c_char {
        let script = session_mut(session)
            .map(|s| s.document_start_scripts().join("\n"))
            .unwrap_or_default();
        into_c_string(script)
    }

    /// Dispatch an EIP-1193 provider envelope posted from the page on the provider
    /// channel `name` and return the response JS Swift must run in the live page
    /// (via `WKWebView.evaluateJavaScript`) to settle the page's pending Promise,
    /// as a single heap C string (the response-delivery calls joined; empty means
    /// nothing to run). Free with [`werust_ios_string_free`]. This is the page ->
    /// native -> page provider round-trip on iOS.
    ///
    /// # Safety
    /// `session` is a live handle; `name` / `body` are valid NUL-terminated C
    /// strings.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_handle_provider_message(
        session: *mut CoreSession,
        name: *const c_char,
        body: *const c_char,
    ) -> *mut c_char {
        let name = read(name);
        let body = read(body);
        let script = session_mut(session)
            .map(|s| s.handle_provider_message(&name, &body).join("\n"))
            .unwrap_or_default();
        into_c_string(script)
    }

    /// Report the platform `WKWebView`'s commit signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_committed(
        session: *mut CoreSession,
        url: *const c_char,
    ) {
        let url = read(url);
        if let Some(s) = session_mut(session) {
            s.on_page_committed(&url);
        }
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_finished(
        session: *mut CoreSession,
        url: *const c_char,
    ) {
        let url = read(url);
        if let Some(s) = session_mut(session) {
            s.on_page_finished(&url);
        }
    }

    /// Report a same-document URL change (an SPA `pushState`/`replaceState`) into
    /// the core, then fold it into the chrome. Called from Swift's KVO observer on
    /// `webView.url`.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_url_changed(
        session: *mut CoreSession,
        url: *const c_char,
    ) {
        let url = read(url);
        if let Some(s) = session_mut(session) {
            s.on_url_changed(&url);
        }
    }

    /// Report the platform `WKWebView`'s error signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` and `reason` are valid NUL-terminated
    /// C strings.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_failed(
        session: *mut CoreSession,
        url: *const c_char,
        reason: *const c_char,
    ) {
        let url = read(url);
        let reason = read(reason);
        if let Some(s) = session_mut(session) {
            s.on_page_failed(&url, &reason);
        }
    }

    /// The current chrome as a heap C string (JSON), for Swift to paint the URL
    /// bar, nav-control enablement, and status line. Free with
    /// [`werust_ios_string_free`].
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_chrome_json(session: *mut CoreSession) -> *mut c_char {
        match session_mut(session) {
            Some(s) => into_c_string(s.chrome_json()),
            None => std::ptr::null_mut(),
        }
    }

    /// The debug capture store (console + network) as a heap C string (JSON), for
    /// the Swift debug view. Free with [`werust_ios_string_free`]. A DEDICATED
    /// accessor beside [`werust_ios_chrome_json`]: the chrome JSON is polled on
    /// every chrome refresh, this only while the debug view is open.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_debug_json(session: *mut CoreSession) -> *mut c_char {
        match session_mut(session) {
            Some(s) => into_c_string(s.debug_json()),
            None => std::ptr::null_mut(),
        }
    }

    /// Empty the debug capture store: the debug view's Clear action.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_debug_clear(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.debug_capture().clear();
        }
    }

    /// Capture one envelope the injected debug shim posted, from Swift's
    /// `WKScriptMessageHandler` for the capture channel (task
    /// `debug-console-network-capture-per-platform`).
    ///
    /// The body is PAGE-CONTROLLED text; the core parse is total and fail-quiet,
    /// so an unreadable or hostile body is dropped rather than fabricated into an
    /// entry.
    ///
    /// # Safety
    /// `session` is a live handle; `name` and `body` are valid NUL-terminated C
    /// strings.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_capture_script_message(
        session: *mut CoreSession,
        name: *const c_char,
        body: *const c_char,
    ) {
        let name = read(name);
        let body = read(body);
        if let Some(s) = session_mut(session) {
            s.capture_script_message(&name, &body);
        }
    }

    /// Capture one NETWORK request from an iOS point that sees it NATIVELY: the
    /// `WKURLSchemeHandler` custom-scheme tasks and the `WKNavigationDelegate`
    /// main-frame navigations.
    ///
    /// `verified` must reflect what the request ACTUALLY did (a successful
    /// `ipfs://` resolution through the hash-verified path), never what the URL
    /// looks like; `main_frame` says only that the CALLER natively knows this is
    /// the main document (the nav delegate) — a scheme task passes `false` and the
    /// core decides with its shared main-frame predicate. Either way the
    /// main-document row takes the load's own two-axis posture. A `0` status/size
    /// means unknown.
    ///
    /// # Safety
    /// `session` is a live handle; `method`, `url` and `mime` are valid
    /// NUL-terminated C strings.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_capture_network(
        session: *mut CoreSession,
        method: *const c_char,
        url: *const c_char,
        status: u16,
        mime: *const c_char,
        size: u64,
        verified: bool,
        main_frame: bool,
    ) {
        let method = read(method);
        let url = read(url);
        let mime = read(mime);
        if let Some(s) = session_mut(session) {
            s.capture_network(&method, &url, status, &mime, size, verified, main_frame);
        }
    }

    /// werust's version string as a heap C string, for the Swift browser menu's
    /// version line. Free with [`werust_ios_string_free`]. Takes NO session
    /// handle: the version is a property of the BUILD, and it is the ONE shared
    /// source all three menus read ([`werust_core::version`]), so no edge
    /// hardcodes a version of its own.
    #[no_mangle]
    pub extern "C" fn werust_ios_version() -> *mut c_char {
        into_c_string(super::version().to_string())
    }

    /// The general browser MENU as a heap C string (JSON), for the Swift edge to
    /// build its native `UIMenu` from. Free with [`werust_ios_string_free`].
    /// Session-free, like [`werust_ios_version`].
    #[no_mangle]
    pub extern "C" fn werust_ios_menu_json() -> *mut c_char {
        into_c_string(super::menu_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::LoadState;

    /// Drive the in-flight load to done the way the Swift edge would from the
    /// platform `WKWebView`'s `didCommit` + `didFinish` signals.
    fn settle(session: &mut CoreSession) {
        let url = session
            .take_pending_load()
            .expect("a pending load to apply to the WKWebView");
        session.on_page_committed(&url);
        session.on_page_finished(&url);
    }

    #[test]
    fn a_typed_url_navigates_through_the_core_and_moves_the_chrome() {
        // The end-to-end Swift↔core protocol for the URL bar's Enter action: the
        // core navigates through the seam, surfaces the URL for the WKWebView, and
        // the chrome reflects the in-flight then settled load.
        let mut s = CoreSession::new();
        assert_eq!(s.chrome().url_text, "");
        assert_eq!(s.chrome().load_state, LoadState::Idle);

        assert!(s.navigate("https://example.com/"), "valid https url");
        assert_eq!(s.chrome().url_text, "https://example.com/");
        assert!(s.chrome().is_loading());
        assert_eq!(
            s.take_pending_load().as_deref(),
            Some("https://example.com/"),
            "the URL is surfaced for WKWebView.load"
        );
        assert_eq!(s.take_pending_load(), None, "drained once");

        // The WKWebView's real signals (reported by Swift) settle the load.
        s.on_page_committed("https://example.com/");
        s.on_page_finished("https://example.com/");
        assert_eq!(s.chrome().load_state, LoadState::Finished);
        assert!(!s.chrome().is_loading());
    }

    #[test]
    fn an_invalid_entry_surfaces_the_badge_and_keeps_the_typed_text_without_loading() {
        // Field finding D: a scheme-less GARBAGE entry does NOT navigate. The core
        // front door handles it (surfacing the distinct invalid-URL state and
        // keeping the typed text), so no pending load is queued for the Swift edge
        // and the bar is not reset. The edge passes the RAW text (it no longer
        // prepends `https://` itself), so the core's classifier is what decides.
        let mut s = CoreSession::new();
        s.navigate("not-a-url");
        assert_eq!(s.chrome().load_state, LoadState::Idle);
        // The distinct invalid-entry axis (NOT `last_error`) drives the badge.
        assert!(s.chrome().invalid_entry.is_some());
        assert_eq!(s.chrome().last_error, None);
        // The typed text is kept for the user to fix; no load was queued.
        assert_eq!(s.chrome().url_text, "not-a-url");
        assert_eq!(s.take_pending_load(), None);
    }

    #[test]
    fn a_scheme_less_valid_host_navigates_over_https_through_the_core() {
        // Field finding D: a scheme-less plausible host navigates as
        // `https://<host>` — the prepend is the CORE's job (the edge passes raw
        // text), so the Swift edge gets a pending `https://github.com` to load.
        let mut s = CoreSession::new();
        assert!(s.navigate("github.com"), "a valid host navigates");
        assert!(s.chrome().invalid_entry.is_none());
        assert_eq!(s.take_pending_load().as_deref(), Some("https://github.com"));
    }

    #[test]
    fn back_and_forward_reflect_navigation_state_through_the_core() {
        // Back/Forward availability is the CORE's truth, read by Swift from the
        // chrome — Swift keeps no history of its own.
        let mut s = CoreSession::new();
        assert!(!s.chrome().can_go_back);
        assert!(!s.chrome().can_go_forward);

        assert!(s.navigate("https://a.example/"));
        settle(&mut s);
        assert!(!s.chrome().can_go_back, "one entry: nowhere back");

        assert!(s.navigate("https://b.example/"));
        settle(&mut s);
        assert!(s.chrome().can_go_back);
        assert!(!s.chrome().can_go_forward);

        s.go_back();
        settle(&mut s);
        assert_eq!(s.chrome().url_text, "https://a.example/");
        assert!(!s.chrome().can_go_back);
        assert!(s.chrome().can_go_forward);

        s.go_forward();
        settle(&mut s);
        assert_eq!(s.chrome().url_text, "https://b.example/");
        assert!(!s.chrome().can_go_forward, "back at the tip of history");
    }

    #[test]
    fn reload_re_navigates_and_stop_settles_the_load() {
        let mut s = CoreSession::new();
        assert!(!s.reload(), "nothing to reload yet");

        assert!(s.navigate("https://example.com/"));
        settle(&mut s);
        assert!(s.reload(), "reload the settled page");
        assert!(s.chrome().is_loading());

        s.stop();
        assert_eq!(s.chrome().load_state, LoadState::Idle);
    }

    #[test]
    fn a_failed_load_surfaces_the_failure_in_the_chrome() {
        let mut s = CoreSession::new();
        assert!(s.navigate("https://does-not-resolve.invalid/"));
        let _ = s.take_pending_load();
        assert_eq!(s.chrome().last_error, None);

        s.on_page_failed("https://does-not-resolve.invalid/", "name not resolved");
        assert_eq!(s.chrome().load_state, LoadState::Failed);
        assert_eq!(s.chrome().last_error.as_deref(), Some("name not resolved"));

        // A new navigation clears the surfaced failure.
        assert!(s.navigate("https://example.com/"));
        assert_eq!(s.chrome().last_error, None);
    }

    #[test]
    fn ipfs_scheme_reaches_the_shared_core_resolve_path() {
        // The motivating gap: an `ipfs://` URL the WKWebView cannot load without a
        // WKURLSchemeHandler is intercepted by the Swift edge and routed through
        // the SAME core resolve path desktop uses. Here we prove the scheme is
        // intercepted (not `None`) and reaches the core, which fails CLOSED on a
        // malformed CID BEFORE any network fetch (network-isolated):
        // `Cid::try_from` rejects it, so a fail-closed reason is surfaced and
        // nothing unverified is rendered — desktop-parity trust posture.
        let s = CoreSession::new();
        let resolution = s
            .resolve_ipfs("ipfs://not-a-valid-cid/index.html")
            .expect("the ipfs scheme is intercepted and routed to the core");
        match resolution {
            SchemeResolution::Err { reason } => {
                assert!(
                    reason.contains("ipfs://"),
                    "the fail-closed reason names the ipfs load failure: {reason}"
                );
            }
            SchemeResolution::Ok { .. } => {
                panic!("a malformed CID must fail closed, never render bytes")
            }
        }
    }

    #[test]
    fn a_non_ipfs_url_is_not_intercepted() {
        // A plain `https://` URL is NOT a registered scheme, so the edge lets the
        // WKWebView load it normally (no interception).
        let s = CoreSession::new();
        assert!(s.resolve_ipfs("https://example.com/").is_none());
    }

    #[test]
    fn the_eip1193_provider_bridge_reaches_the_shared_core_through_the_session() {
        // The provider bridge is wired on the iOS edge (the seam no-op is gone):
        // the session injects the provider shim at document start, and a page
        // envelope posted on the provider channel round-trips through the SAME
        // `werust_core::provider` path desktop uses to a response push Swift runs
        // in the page. Network-isolated: the read-only stub answers keylessly.
        let s = CoreSession::new();

        let scripts = s.document_start_scripts();
        // The session now injects several document-start scripts (the provider shim
        // plus the debug capture shims, task
        // `debug-console-network-capture-per-platform`), so this asserts the
        // PROVIDER one is among them rather than pinning the count — which would
        // red every time another edge capability adds a user script.
        let provider = scripts
            .iter()
            .find(|script| script.contains("isWerust: true"))
            .expect("the provider shim is injected at document start");
        assert!(provider.contains("ethereum"));

        let pushed = s.handle_provider_message(
            "werustProvider",
            r#"{"id":7,"method":"eth_chainId","params":[]}"#,
        );
        assert_eq!(
            pushed.len(),
            1,
            "the request yields exactly one response push"
        );
        assert!(
            pushed[0].contains("__resolve(7") && pushed[0].contains("0x1"),
            "the response settles Promise 7 with the stub chain id: {}",
            pushed[0]
        );

        assert!(s
            .handle_provider_message("someOtherChannel", r#"{"id":1,"method":"eth_chainId"}"#)
            .is_empty());
    }

    #[test]
    fn the_chrome_trust_posture_reaches_the_swift_edge_from_the_core() {
        // The trust indicator is wired on the iOS edge (the seam-default
        // `UnverifiedOrigin` was inherited before): the chrome JSON Swift paints
        // carries the current load's REAL posture. A fresh load is untrusted; a
        // verified `ipfs` resolution (marked via the session's `resolve_ipfs`
        // success path) surfaces content-verified — matching desktop.
        let mut s = CoreSession::new();
        assert!(s
            .chrome_json()
            .contains("\"trustPosture\":\"unverified-origin\""));

        assert!(s.navigate("ipfs://bafycid/"));
        assert!(
            s.chrome_json()
                .contains("\"trustPosture\":\"unverified-origin\""),
            "untrusted until the bytes verify"
        );
        // The OS edge intercepts the request, the shared resolve verifies the
        // bytes (marking the load), then the WKWebView reports its commit/finish
        // signals which pump the shell and refresh the chrome from the seam's
        // posture — exactly the real iOS signal flow.
        s.backend.mark_content_verified();
        let url = s.take_pending_load().expect("the ipfs load is pending");
        s.on_page_committed(&url);
        s.on_page_finished(&url);
        assert!(
            s.chrome_json()
                .contains("\"trustPosture\":\"content-verified\""),
            "a verified load surfaces content-verified in the chrome: {}",
            s.chrome_json()
        );
    }

    #[test]
    fn an_internal_werust_page_is_not_marked_content_verified_by_resolve_ipfs() {
        // Swift routes `werust://settings` through the separate `apply_settings`
        // (which does NOT mark), but the `resolve_ipfs` mark is scoped to the
        // `ipfs` scheme defensively: even if a `werust://` URI reached
        // `resolve_ipfs`, the internal chrome page must never earn the
        // content-verified posture (it is not hash-verified content).
        let s = CoreSession::new();
        let resolution = s
            .resolve_ipfs("werust://settings")
            .expect("the werust scheme is intercepted and served");
        assert!(
            matches!(resolution, SchemeResolution::Ok { .. }),
            "the settings page is served"
        );
        assert_eq!(
            s.chrome().trust_posture,
            renderer::TrustPosture::UnverifiedOrigin,
            "an internal settings page is never marked content-verified"
        );
    }

    #[test]
    fn the_werust_settings_scheme_reaches_the_shared_core_settings_page() {
        // The requeue's Gate-2 gap, closed on the Rust side: `werust://settings`
        // must be a REGISTERED scheme the Swift `WKURLSchemeHandler` for `werust`
        // routes into the SHARED `apply_settings_request` core path (the same page
        // desktop + Android serve). Here we prove the scheme is intercepted (not
        // `None`) and reaches the core, which renders the retrieval-backend
        // settings page. Network-isolated: the settings page is pure HTML built in
        // `werust-core`, no fetch.
        let s = CoreSession::new();
        let resolution = s
            .apply_settings("werust://settings")
            .expect("the werust scheme is intercepted and routed to the core");
        match resolution {
            SchemeResolution::Ok {
                mime_type, body, ..
            } => {
                assert_eq!(mime_type, "text/html");
                let html = String::from_utf8(body).expect("the page is UTF-8 HTML");
                assert!(
                    html.contains("IPFS retrieval backend"),
                    "the settings page is served: {html}"
                );
            }
            SchemeResolution::Err { reason } => {
                panic!("werust://settings must render the settings page, got: {reason}")
            }
        }
    }

    #[test]
    fn a_non_werust_url_is_not_intercepted_by_apply_settings() {
        // A plain `https://` URL is NOT the `werust` scheme, so `apply_settings`
        // returns `None` and the edge lets the WKWebView handle it (parity with
        // the ipfs handler's non-interception).
        let s = CoreSession::new();
        assert!(s.apply_settings("https://example.com/").is_none());
    }

    #[test]
    fn chrome_json_carries_every_field_the_swift_edge_paints() {
        // The JSON is the wire form Swift reads across the C-ABI; it must carry the
        // URL bar text, nav-control enablement, load state, and any failure so the
        // Swift edge can paint the whole chrome without any logic of its own.
        let mut s = CoreSession::new();
        assert!(s.navigate("https://a.example/"));
        settle(&mut s);
        assert!(s.navigate("https://b.example/"));
        settle(&mut s);

        let json = s.chrome_json();
        assert!(json.contains("\"url\":\"https://b.example/\""), "{json}");
        assert!(json.contains("\"canGoBack\":true"), "{json}");
        assert!(json.contains("\"canGoForward\":false"), "{json}");
        assert!(json.contains("\"loading\":false"), "{json}");
        assert!(json.contains("\"loadState\":\"finished\""), "{json}");
    }

    // --- The debug capture store over the FFI (task ------------------------
    // `debug-capture-store-console-and-network-in-core`)

    #[test]
    fn debug_json_round_trips_console_and_network_entries_including_their_trust() {
        // The Swift debug view reads the capture store as ONE JSON document over
        // the C-ABI, exactly as it reads the chrome. It must carry every field the
        // view paints, including the HONEST per-request trust posture, in the
        // SAME wire vocabulary the chrome's `trustPosture` uses (ADR-0006). The
        // byte-for-byte twin of the Android core's debug document.
        use werust_core::debug::{ConsoleEntry, ConsoleLevel, NetworkEntry};

        let s = CoreSession::new();
        s.debug_capture().push_console(
            ConsoleEntry::new(ConsoleLevel::Error, "boom")
                .with_source("https://x/app.js")
                .with_line(7)
                .with_timestamp(1_700_000_000_001),
        );
        s.debug_capture().push_network(
            NetworkEntry::new("GET", "ipfs://bafy/pic.png")
                .with_status(200)
                .with_mime("image/png")
                .with_size(99)
                .with_trust(renderer::TrustPosture::ContentVerified)
                .with_duration(12)
                .with_timestamp(1_700_000_000_002),
        );
        s.debug_capture()
            .push_network(NetworkEntry::new("GET", "https://cdn.example/a.js"));

        let json = s.debug_json();
        assert!(json.contains("\"level\":\"error\""), "{json}");
        assert!(json.contains("\"message\":\"boom\""), "{json}");
        assert!(json.contains("\"source\":\"https://x/app.js\""), "{json}");
        assert!(json.contains("\"line\":7"), "{json}");
        assert!(json.contains("\"url\":\"ipfs://bafy/pic.png\""), "{json}");
        assert!(json.contains("\"status\":200"), "{json}");
        assert!(json.contains("\"mime\":\"image/png\""), "{json}");
        assert!(json.contains("\"scheme\":\"ipfs\""), "{json}");
        assert!(json.contains("\"trust\":\"content-verified\""), "{json}");
        assert!(json.contains("\"duration\":12"), "{json}");
        // The https subresource is honestly UNVERIFIED: the debug view can never
        // imply a request was trusted that was not.
        assert!(json.contains("\"trust\":\"unverified-origin\""), "{json}");
        assert!(json.contains("\"networkCaptureEnabled\":true"), "{json}");

        // Clear (the debug view's Clear button) empties both lists.
        s.debug_capture().clear();
        let json = s.debug_json();
        assert!(json.contains("\"console\":[]"), "{json}");
        assert!(json.contains("\"network\":[]"), "{json}");
    }

    #[test]
    fn the_debug_document_is_separate_so_existing_chrome_readers_are_unaffected() {
        // The capture is a DEDICATED accessor, not an additive chrome field: the
        // chrome JSON keeps its exact prior shape, so every existing Swift chrome
        // reader is untouched.
        use werust_core::debug::{ConsoleEntry, ConsoleLevel};
        let s = CoreSession::new();
        s.debug_capture()
            .push_console(ConsoleEntry::new(ConsoleLevel::Log, "hello"));
        let chrome = s.chrome_json();
        assert!(!chrome.contains("console"), "{chrome}");
        assert!(!chrome.contains("debug"), "{chrome}");
        assert!(s.debug_json().contains("hello"));
    }

    #[test]
    fn the_c_abi_reads_and_clears_the_debug_capture_and_frees_its_string() {
        // The Swift debug view reaches the store through the raw C-ABI exports
        // exactly as it reaches the chrome: one heap C string it frees, plus a
        // Clear export. Null handles are tolerated.
        use super::ffi::*;
        use std::ffi::CStr;
        use werust_core::debug::NetworkEntry;

        unsafe {
            let s = werust_ios_session_new();
            (*s).debug_capture()
                .push_network(NetworkEntry::new("GET", "ipfs://bafy/x"));

            let json_ptr = werust_ios_debug_json(s);
            assert!(!json_ptr.is_null());
            let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(json_ptr);
            assert!(json.contains("ipfs://bafy/x"), "{json}");

            werust_ios_debug_clear(s);
            let json_ptr = werust_ios_debug_json(s);
            let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(json_ptr);
            assert!(json.contains("\"network\":[]"), "{json}");

            werust_ios_session_free(s);
            assert!(werust_ios_debug_json(std::ptr::null_mut()).is_null());
            werust_ios_debug_clear(std::ptr::null_mut());
        }
    }

    // --- The iOS CAPTURE POINTS (task -------------------------------------
    // `debug-console-network-capture-per-platform`)

    #[test]
    fn the_injected_console_shim_is_installed_at_document_start_alongside_the_provider() {
        // Acceptance: WKWebView has NO native console callback, so iOS captures the
        // console by injecting the SHARED core shim — the byte-for-byte same string
        // desktop injects, from ONE place — as a document-start user script, on its
        // OWN capture channel (never the EIP-1193 provider's trust channel).
        use werust_core::debug::{console_shim, network_shim, CAPTURE_BRIDGE};
        let s = CoreSession::new();
        let scripts = s.document_start_scripts();
        assert!(
            scripts.iter().any(|script| *script == console_shim()),
            "the SHARED core console shim is injected verbatim, not a local copy"
        );
        assert!(
            scripts.iter().any(|script| *script == network_shim()),
            "iOS also injects the best-effort fetch/XHR shim (its only page-wide \
             network reach)"
        );
        assert_ne!(CAPTURE_BRIDGE, werust_core::provider::PROVIDER_BRIDGE);
    }

    #[test]
    fn a_shim_posted_console_message_reaches_the_shared_store_through_the_capture_channel() {
        use werust_core::debug::{ConsoleLevel, CAPTURE_BRIDGE};
        let s = CoreSession::new();
        s.capture_script_message(
            CAPTURE_BRIDGE,
            r#"{"kind":"console","level":"error","message":"boom",
               "source":"https://x/app.js","line":9,"ts":5}"#,
        );
        let entries = s.debug_capture().console();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, ConsoleLevel::Error);
        assert_eq!(entries[0].message, "boom");
        assert_eq!(entries[0].line, Some(9));
    }

    #[test]
    fn a_shim_posted_fetch_reaches_the_store_but_can_never_claim_verification() {
        // Page-side JS proves NOTHING about the load path, so the best-effort
        // network shim's rows are always honestly unverified.
        use werust_core::debug::CAPTURE_BRIDGE;
        let s = CoreSession::new();
        s.capture_script_message(
            CAPTURE_BRIDGE,
            r#"{"kind":"network","method":"GET","url":"https://api.example/x",
               "status":200,"mime":"application/json","size":4,"ts":1,"duration":9}"#,
        );
        let entries = s.debug_capture().network();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://api.example/x");
        assert_eq!(entries[0].status, Some(200));
        assert_eq!(entries[0].duration, Some(9));
        assert_eq!(entries[0].trust, renderer::TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn a_hostile_page_posting_on_the_capture_channel_cannot_fabricate_an_entry() {
        // The capture channel is reachable from page JS directly, so the parse must
        // be total and fail-quiet: junk is dropped, never turned into an entry and
        // never a panic that would take the browser down.
        use werust_core::debug::CAPTURE_BRIDGE;
        let s = CoreSession::new();
        for body in ["", "not json", "[]", r#"{"kind":"nope"}"#] {
            s.capture_script_message(CAPTURE_BRIDGE, body);
        }
        assert!(s.debug_capture().console().is_empty());
        assert!(s.debug_capture().network().is_empty());
    }

    #[test]
    fn an_unregistered_channel_captures_nothing() {
        // Capture is dispatched by channel name, exactly as the provider is: a
        // message on a channel nobody registered reaches no store.
        let s = CoreSession::new();
        s.capture_script_message("werustNotAChannel", r#"{"kind":"console","message":"x"}"#);
        assert!(s.debug_capture().console().is_empty());
    }

    #[test]
    fn the_native_ios_capture_points_report_the_honest_per_request_posture() {
        // The scheme handler and the nav delegate are the iOS points that KNOW the
        // outcome, so they are the only ones that may report a verified posture —
        // and only when the bytes really came back hash-verified (ADR-0006).
        let s = CoreSession::new();
        s.capture_network(
            "GET",
            "ipfs://bafy/pic.png",
            200,
            "image/png",
            9,
            true,
            false,
        );
        s.capture_network(
            "GET",
            "werust://settings",
            200,
            "text/html",
            10,
            false,
            false,
        );
        s.capture_network("GET", "ipfs://bafy/gone", 0, "", 0, false, false);

        let entries = s.debug_capture().network();
        assert_eq!(entries[0].trust, renderer::TrustPosture::ContentVerified);
        assert_eq!(
            entries[1].trust,
            renderer::TrustPosture::UnverifiedOrigin,
            "an internal werust:// page is not content-verified"
        );
        assert_eq!(
            entries[2].trust,
            renderer::TrustPosture::UnverifiedOrigin,
            "a failed ipfs:// request claims nothing"
        );
        assert_eq!(
            entries[2].status, None,
            "an unknown status stays unknown, never a fake 0"
        );
    }

    #[test]
    fn the_ios_main_document_row_takes_the_loads_own_posture() {
        // The store's DECISIONS.md Decision 4: the main-document row must mirror
        // the chrome trust indicator so the two surfaces cannot disagree.
        //
        // THE ORDERING TRAP this pins: the production order is navigate -> the
        // WKWebView asks for the document -> the shared resolve MARKS the backend
        // content-verified -> the capture runs, all BEFORE `didCommit` /
        // `didFinish` pump the shell and `refresh_chrome` re-caches the posture.
        // Reading the CACHED `chrome().trust_posture` here would stamp the stale
        // pre-verify `unverified-origin`, so the row must read the LIVE posture
        // (the seam's `Renderer::trust_posture`), exactly as the desktop capture
        // reads its load lifecycle directly.
        let mut s = CoreSession::new();
        assert!(s.navigate("ipfs://bafy/index.html"));
        s.backend.mark_content_verified();
        // Deliberately NO on_page_committed / on_page_finished: the chrome cache
        // is still the stale pre-verify snapshot, which is the whole point of
        // this test.
        assert_eq!(
            s.chrome().trust_posture,
            renderer::TrustPosture::UnverifiedOrigin,
            "the cached chrome is still stale — the trap this test pins"
        );
        s.capture_network(
            "GET",
            "ipfs://bafy/index.html",
            200,
            "text/html",
            12,
            true,
            true,
        );
        assert_eq!(
            s.debug_capture().network()[0].trust,
            renderer::TrustPosture::ContentVerified,
            "the main-document row carries the LIVE posture, not the stale cache"
        );
    }

    #[test]
    fn the_ios_scheme_handler_row_is_reconciled_by_the_shared_core_main_frame_predicate() {
        // The requeue's Gate-2 fix. A `WKURLSchemeTask` carries NO main-frame flag,
        // so Swift used to compute one by comparing the task URL against
        // `chrome().url` — the DISPLAY identity. That compare is wrong in exactly
        // the cases it was mandated for: on an ENS load the shell pins the name
        // there (`ronan.eth`) while the task URL is `ipfs://<cid>/…`, and WebKit
        // re-reports the same document in the authority-LESS `ipfs:///<cid>` form.
        // Either way the compare never fires, and the Network tab would show a
        // plain `content-verified` page row beside a louder indicator.
        //
        // So Swift passes `main_frame: false` and the CORE decides, with the ONE
        // shared predicate driven by the top-level URL the shell already reports on
        // every navigation (normalized through `frame_key`).
        let mut s = CoreSession::new();
        assert!(s.navigate("ipfs://bafypage/index.html"));
        s.backend.mark_content_verified();
        let url = s.take_pending_load().expect("the ipfs load is pending");
        s.on_page_committed(&url);
        s.on_page_finished(&url);

        assert_ne!(
            s.chrome().url_text,
            "ipfs:///bafypage/index.html",
            "the display identity is NOT the form a scheme task sees, which is \
             exactly why Swift may not compare against it"
        );

        // The AUTHORITY-LESS form of the very same document, as a scheme task sees
        // it — with `main_frame: false`, exactly as Swift now calls it, and with
        // `verified: false` so the per-request posture alone would be the weaker
        // `unverified-origin`: only the reconciliation can lift it.
        s.capture_network(
            "GET",
            "ipfs:///bafypage/index.html",
            200,
            "text/html",
            12,
            false,
            false,
        );
        // …and a genuine sub-resource of it, which must NOT be reconciled.
        s.capture_network(
            "GET",
            "ipfs://bafypage/app.css",
            200,
            "text/css",
            4,
            true,
            false,
        );

        let entries = s.debug_capture().network();
        assert_eq!(
            entries[0].trust,
            s.chrome().trust_posture,
            "the main-document row takes the LOAD's posture even though Swift \
             passed main_frame: false and the URL form differs from the display one"
        );
        assert_eq!(
            entries[1].trust,
            renderer::TrustPosture::ContentVerified,
            "a sub-resource keeps its own honest per-request posture"
        );
    }

    #[test]
    fn the_c_abi_feeds_the_capture_points_and_tolerates_a_null_session() {
        use super::ffi::*;
        use std::ffi::{CStr, CString};
        unsafe {
            let s = werust_ios_session_new();
            let channel = CString::new(werust_core::debug::CAPTURE_BRIDGE).unwrap();
            let body = CString::new(r#"{"kind":"console","level":"warn","message":"hi"}"#).unwrap();
            werust_ios_capture_script_message(s, channel.as_ptr(), body.as_ptr());

            let method = CString::new("GET").unwrap();
            let url = CString::new("ipfs://bafy/pic.png").unwrap();
            let mime = CString::new("image/png").unwrap();
            werust_ios_capture_network(
                s,
                method.as_ptr(),
                url.as_ptr(),
                200,
                mime.as_ptr(),
                9,
                true,
                false,
            );

            let json_ptr = werust_ios_debug_json(s);
            let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(json_ptr);
            assert!(json.contains("\"message\":\"hi\""), "{json}");
            assert!(json.contains("\"level\":\"warn\""), "{json}");
            assert!(json.contains("\"trust\":\"content-verified\""), "{json}");

            werust_ios_session_free(s);
            // A null session is tolerated (no panic across the C boundary).
            werust_ios_capture_script_message(
                std::ptr::null_mut(),
                channel.as_ptr(),
                body.as_ptr(),
            );
            werust_ios_capture_network(
                std::ptr::null_mut(),
                method.as_ptr(),
                url.as_ptr(),
                200,
                mime.as_ptr(),
                9,
                true,
                false,
            );
        }
    }

    // --- The general browser MENU over the FFI (task ----------------------
    // `general-browser-menu-with-version-and-debug-entry`)

    #[test]
    fn the_menu_version_is_the_one_shared_source_not_a_swift_hardcode() {
        // Acceptance: the version the iOS menu shows comes from ONE place
        // (`werust_core::version`) over the FFI, so it can never drift from the
        // desktop popover or the Android menu. The Swift edge reads THIS, never a
        // literal (or an Info.plist `CFBundleShortVersionString`) of its own.
        assert_eq!(super::version(), werust_core::version());
        assert!(!super::version().is_empty());
    }

    #[test]
    fn the_menu_document_carries_the_version_line_and_the_debug_entry_for_swift() {
        // The Swift edge builds its native `UIMenu` from this ONE document, so it
        // must carry the version and every item with its stable id + kind: a
        // non-interactive `werust <version>` line and an activatable Debug entry
        // that opens the debug view. The byte-for-byte twin of the Android core's
        // menu document, which is what makes the three menus agree.
        let json = super::menu_json();
        assert!(
            json.contains(&format!("\"version\":\"{}\"", werust_core::version())),
            "{json}"
        );
        assert!(json.contains("\"id\":\"version\""), "{json}");
        assert!(
            json.contains(&format!("\"label\":\"werust {}\"", werust_core::version())),
            "{json}"
        );
        assert!(json.contains("\"kind\":\"info\""), "{json}");
        assert!(json.contains("\"id\":\"debug\""), "{json}");
        assert!(json.contains("\"label\":\"Debug\""), "{json}");
        assert!(json.contains("\"kind\":\"action\""), "{json}");
    }

    #[test]
    fn the_c_abi_menu_exports_need_no_session_and_hand_back_freeable_strings() {
        // The menu is a USER-FACING, ALWAYS-AVAILABLE surface (never debug-build-
        // gated, never dependent on a live browsing session), so both exports take
        // NO session handle — Swift can build the menu before/without one. They
        // follow the same string-ownership contract as every other export: a heap
        // C string the caller frees with `werust_ios_string_free`.
        use super::ffi::*;
        use std::ffi::CStr;

        unsafe {
            let version_ptr = werust_ios_version();
            assert!(!version_ptr.is_null());
            let version = CStr::from_ptr(version_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(version_ptr);
            assert_eq!(version, werust_core::version());

            let menu_ptr = werust_ios_menu_json();
            assert!(!menu_ptr.is_null());
            let menu = CStr::from_ptr(menu_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(menu_ptr);
            assert!(menu.contains("\"id\":\"debug\""), "{menu}");
            assert!(menu.contains(&format!("werust {version}")), "{menu}");
        }
    }

    /// Drive the whole Swift↔core protocol across the raw C-ABI exports exactly
    /// as the Swift shell does, proving the mechanical marshalling shim threads
    /// the opaque handle, borrows input C strings, and hands back heap C strings
    /// the caller frees. This is the one test that exercises the FFI layer itself
    /// (the exports are `cfg`-free so the host gate covers them too).
    #[test]
    fn the_c_abi_exports_drive_a_full_navigation_and_free_their_strings() {
        use super::ffi::*;
        use std::ffi::{CStr, CString};

        unsafe {
            let s = werust_ios_session_new();
            assert!(!s.is_null());

            let url = CString::new("https://example.com/").unwrap();
            assert!(werust_ios_navigate(s, url.as_ptr()), "valid https url");

            let pending = werust_ios_take_pending_load(s);
            assert!(!pending.is_null(), "a pending load is surfaced");
            assert_eq!(
                CStr::from_ptr(pending).to_str().unwrap(),
                "https://example.com/"
            );
            werust_ios_string_free(pending);
            assert!(
                werust_ios_take_pending_load(s).is_null(),
                "drained once: null now"
            );

            werust_ios_on_page_committed(s, url.as_ptr());
            werust_ios_on_page_finished(s, url.as_ptr());

            let json_ptr = werust_ios_chrome_json(s);
            assert!(!json_ptr.is_null());
            let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(json_ptr);
            assert!(json.contains("\"url\":\"https://example.com/\""), "{json}");
            assert!(json.contains("\"loadState\":\"finished\""), "{json}");
            assert!(json.contains("\"loading\":false"), "{json}");

            werust_ios_session_free(s);
            // Null handles are tolerated (no crash, sane defaults).
            assert!(!werust_ios_navigate(std::ptr::null_mut(), url.as_ptr()));
            assert!(werust_ios_chrome_json(std::ptr::null_mut()).is_null());
            werust_ios_session_free(std::ptr::null_mut());
            werust_ios_string_free(std::ptr::null_mut());
        }
    }

    /// Drive the `ipfs://` resolution across the raw C-ABI exports exactly as the
    /// Swift `WKURLSchemeHandler` does: intercept the scheme, read the
    /// fail-closed result (a malformed CID fails BEFORE any network fetch, so this
    /// is network-isolated), and free the resolution + strings. Also proves a
    /// non-`ipfs` URL is not intercepted (NULL handle) and null handles are
    /// tolerated.
    #[test]
    fn the_c_abi_resolves_ipfs_through_the_core_and_frees_its_handle() {
        use super::ffi::*;
        use std::ffi::{CStr, CString};

        unsafe {
            let s = werust_ios_session_new();

            // A non-ipfs URL is not an intercepted scheme: NULL handle.
            let https = CString::new("https://example.com/").unwrap();
            assert!(werust_ios_resolve_ipfs(s, https.as_ptr()).is_null());

            // An `ipfs://` URL IS intercepted and routed to the shared core path;
            // a malformed CID fails closed with a legible reason, no bytes.
            let ipfs = CString::new("ipfs://not-a-valid-cid/index.html").unwrap();
            let res = werust_ios_resolve_ipfs(s, ipfs.as_ptr());
            assert!(!res.is_null(), "the ipfs scheme is intercepted");
            assert!(
                !werust_ios_resolution_is_ok(res),
                "malformed CID fails closed"
            );
            assert_eq!(
                werust_ios_resolution_body_len(res),
                0,
                "no bytes on failure"
            );

            let reason_ptr = werust_ios_resolution_error(res);
            assert!(!reason_ptr.is_null());
            let reason = CStr::from_ptr(reason_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(reason_ptr);
            assert!(
                reason.contains("ipfs://"),
                "legible fail-closed reason: {reason}"
            );

            werust_ios_resolution_free(res);
            werust_ios_session_free(s);

            // Null handles are tolerated.
            assert!(werust_ios_resolve_ipfs(std::ptr::null_mut(), ipfs.as_ptr()).is_null());
            assert!(!werust_ios_resolution_is_ok(std::ptr::null()));
            assert_eq!(werust_ios_resolution_body_len(std::ptr::null()), 0);
            werust_ios_resolution_free(std::ptr::null_mut());
        }
    }

    /// Drive the `werust://settings` resolution across the raw C-ABI exports
    /// exactly as the Swift `WKURLSchemeHandler` for `werust` does: intercept the
    /// scheme, read the served settings page (verified-free: it is a pure-HTML
    /// internal page, so this is network-isolated), and free the resolution +
    /// strings. Also proves a non-`werust` URL is not intercepted (NULL handle)
    /// and null handles are tolerated. This is the FFI half of the requeue's
    /// Gate-2 iOS fix — the exact export the Swift edge calls.
    #[test]
    fn the_c_abi_applies_settings_through_the_core_and_frees_its_handle() {
        use super::ffi::*;
        use std::ffi::{CStr, CString};

        unsafe {
            let s = werust_ios_session_new();

            // A non-werust URL is not an intercepted scheme: NULL handle.
            let https = CString::new("https://example.com/").unwrap();
            assert!(werust_ios_apply_settings(s, https.as_ptr()).is_null());

            // A `werust://settings` URL IS intercepted and routed to the shared
            // core path; it renders the retrieval-backend settings page.
            let werust = CString::new("werust://settings").unwrap();
            let res = werust_ios_apply_settings(s, werust.as_ptr());
            assert!(!res.is_null(), "the werust scheme is intercepted");
            assert!(
                werust_ios_resolution_is_ok(res),
                "the settings page renders (not a fail-closed error)"
            );

            let mime_ptr = werust_ios_resolution_mime(res);
            assert!(!mime_ptr.is_null());
            let mime = CStr::from_ptr(mime_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(mime_ptr);
            assert_eq!(mime, "text/html");

            let body_ptr = werust_ios_resolution_body(res);
            let body_len = werust_ios_resolution_body_len(res);
            assert!(!body_ptr.is_null() && body_len > 0, "the page has bytes");
            let body = std::slice::from_raw_parts(body_ptr, body_len);
            let html = std::str::from_utf8(body).unwrap();
            assert!(
                html.contains("IPFS retrieval backend"),
                "the settings page is served: {html}"
            );

            werust_ios_resolution_free(res);
            werust_ios_session_free(s);

            // Null handles are tolerated.
            assert!(werust_ios_apply_settings(std::ptr::null_mut(), werust.as_ptr()).is_null());
        }
    }
}
