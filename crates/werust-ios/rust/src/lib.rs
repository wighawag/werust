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
    /// A verified resolution: the MIME type and the verified body bytes.
    Ok { mime_type: String, body: Vec<u8> },
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
        install_ipfs(&mut backend);
        // Wire the FIRST trust hook exactly as the desktop backend's
        // `install_provider` does: register the EIP-1193 provider bridge handler
        // and inject the page-side provider shim at document start, both routed
        // through the SAME `werust_core::provider` path desktop uses. The platform
        // `WKWebView` bridges the channel (a `WKScriptMessageHandler`) and runs the
        // shim + the response push, driving
        // [`handle_provider_message`](CoreSession::handle_provider_message).
        install_provider(&mut backend);
        Self {
            shell: BrowserShell::new(Box::new(backend)),
            backend: handle,
        }
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam. Returns
    /// `true` on success; an unusable URL is rejected and leaves the chrome
    /// untouched (Swift keeps the bad text for the user to fix).
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
fn install_ipfs(backend: &mut IosBackend) {
    use fetcher::{HttpFetcher, TrustlessGatewayCarRetriever};
    use werust_core::ipfs::{resolve_ipfs_request, IPFS_SCHEME};
    use werust_core::retrieval::{active_gateway_endpoint, apply_settings_request, WERUST_SCHEME};

    // Point the retriever at the USER'S CHOSEN retrieval backend (persisted via
    // `werust://settings`): a custom gateway/local-node URL if picked, else the
    // default public trustless gateway. The same core switch desktop uses (task
    // `retrieval-backend-user-setting`); the per-block verify is unchanged.
    let retriever =
        TrustlessGatewayCarRetriever::with_gateway(HttpFetcher::new(), &active_gateway_endpoint());
    backend.register_scheme_handler(
        IPFS_SCHEME,
        Box::new(move |request| resolve_ipfs_request(&retriever, &request)),
    );
    // The internal `werust://settings` page, resolved through the SAME scheme
    // seam so Swift's `WKURLSchemeHandler` for `werust` serves it and a
    // `?backend=…` selection is applied + persisted by the shared core.
    backend.register_scheme_handler(
        WERUST_SCHEME,
        Box::new(|request| apply_settings_request(&request)),
    );
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
    fn an_unusable_url_is_rejected_and_leaves_the_chrome_untouched() {
        let mut s = CoreSession::new();
        assert!(!s.navigate("not-a-url"), "unusable url rejected");
        assert_eq!(s.chrome().load_state, LoadState::Idle);
        assert_eq!(s.chrome().url_text, "");
        assert_eq!(s.take_pending_load(), None);
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
        assert_eq!(
            scripts.len(),
            1,
            "the provider shim is injected at document start"
        );
        assert!(
            scripts[0].contains("isWerust: true"),
            "the injected shim is the provider shim"
        );
        assert!(scripts[0].contains("ethereum"));

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
            SchemeResolution::Ok { mime_type, body } => {
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
