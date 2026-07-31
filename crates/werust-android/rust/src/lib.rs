//! The werust **Android core**: the Rust core cross-compiled as a JNI shared
//! library the Android app links and drives from its Kotlin OS edge.
//!
//! This crate is the Android realisation of "the browsing logic stays in the Rust
//! core behind the seams" (`CONTEXT.md`, the `mobile-android-shell-and-static-lib`
//! task). It builds a `cdylib` (`libwerust_mobile.so`) that the Kotlin `Activity`
//! loads with `System.loadLibrary("werust_mobile")` and drives over a small JNI
//! surface. The core itself is the SHARED [`werust_core`] crate the desktop GTK
//! shell uses; the only Android-specific pieces are:
//!
//! * [`AndroidBackend`] — the [`Renderer`](renderer::Renderer) seam backend over
//!   the platform `android.webkit.WebView` (the Android system webview), driven
//!   from Kotlin across JNI. It owns the browsing LOGIC (session history, load
//!   lifecycle, chrome), so Kotlin stays confined to the OS edge.
//! * [`CoreSession`] — a thin, JVM-free wrapper binding a
//!   [`BrowserShell`](werust_core::BrowserShell) over an [`AndroidBackend`] with
//!   the WebView-signal callbacks, so the whole session is testable with ordinary
//!   `cargo test` (no JVM), and the JNI layer is a mechanical marshalling shim.
//! * The `Java_..._nativeXxx` JNI exports — the mechanical bridge Kotlin calls.
//!
//! # The Kotlin ↔ core protocol
//!
//! One [`CoreSession`] per `Activity`. On a user action (typed URL, Back, Forward,
//! Reload, Stop) Kotlin drives the session, then reads back:
//!
//! * [`take_pending_load`](CoreSession::take_pending_load) — the URL (if any) to
//!   apply to the platform `WebView` via `WebView.loadUrl`.
//! * [`chrome_json`](CoreSession::chrome_json) — the [`ChromeState`] as JSON to
//!   paint the URL bar, the Back/Forward/Reload/Stop enablement, and the status
//!   line. Kotlin holds NO browsing logic; every one of those is the core's truth.
//!
//! And Kotlin reports the platform `WebView`'s real load signals back in
//! ([`on_page_committed`](CoreSession::on_page_committed) /
//! [`on_page_finished`](CoreSession::on_page_finished) /
//! [`on_page_failed`](CoreSession::on_page_failed)), which the core folds into the
//! chrome exactly as the desktop pump folds WebKitGTK's signals.

mod backend;
mod origin_map;

pub use backend::{AndroidBackend, AndroidHandle};

use std::sync::Mutex;

use renderer::Renderer;
use werust_core::{BrowserShell, ChromeState};

/// The wire form of a resolved `ipfs://` request handed back to the Kotlin edge:
/// the MIME type and the verified bytes, or the fail-closed reason.
///
/// The Kotlin `shouldInterceptRequest` turns an [`Ok`] into a
/// `WebResourceResponse` (bytes + MIME) and a [`Err`] into a failed load
/// (`WebResourceResponse` with an error status / null stream), so the fail-closed
/// posture desktop has (a hash mismatch fails the load, never renders) holds on
/// Android too.
#[derive(Debug)]
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

/// A single browsing session for one Android `Activity`: a
/// [`BrowserShell`](werust_core::BrowserShell) over an [`AndroidBackend`], plus the
/// WebView-signal callbacks Kotlin reports into.
///
/// This is the JVM-free heart the JNI layer wraps: every method here is plain Rust
/// so the Kotlin↔core protocol is exercised by `cargo test` without a JVM, and the
/// `Java_...` exports below are a mechanical marshalling shim over it.
pub struct CoreSession {
    shell: BrowserShell,
    /// A handle to the shell's [`AndroidBackend`]'s shared state, for the
    /// platform-`WebView` protocol (pending-load + load signals) that the
    /// cross-backend seam does not carry.
    backend: AndroidHandle,
}

impl Default for CoreSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreSession {
    /// Build a fresh session over an [`AndroidBackend`], with the native `ipfs://`
    /// scheme handler installed.
    #[must_use]
    pub fn new() -> Self {
        let mut backend = AndroidBackend::new();
        let handle = backend.handle();
        // Wire the SECOND trust hook exactly as the desktop backend's
        // `install_ipfs` does, BEFORE handing the backend to the shell: register
        // the `ipfs` scheme handler routing each intercepted request through the
        // SAME `werust_core::ipfs::resolve_ipfs_request` path desktop uses, over
        // the default trustless-gateway CAR retriever (per-block hash-verified).
        // The platform `WebView` cannot load `ipfs://` itself, so Kotlin's
        // `shouldInterceptRequest` drives the intercepted request through
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
        // `WebView` bridges the channel (`addJavascriptInterface`) and runs the
        // shim + the response push, driving
        // [`handle_provider_message`](CoreSession::handle_provider_message).
        install_provider(&mut backend);
        Self {
            shell: BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects),
            backend: handle,
        }
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam. The core
    /// front door routes the RAW entry (Kotlin passes the typed text verbatim):
    /// a bare `.eth` -> ENS; a scheme-less valid host -> `https://` prepend; an
    /// explicit scheme -> literal; an INVALID entry -> the distinct invalid-URL
    /// state (a badge + red-underlined bar, the typed text kept, no navigation).
    /// Returns `true` when the front door handled the entry without erroring
    /// (including an invalid entry, which is handled and surfaced, not a load); an
    /// invalid entry queues NO pending load, so nothing is fed to the WebView.
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

    /// Drain every pending load event off the backend and fold it into the
    /// chrome ([`BrowserShell::pump`](werust_core::BrowserShell::pump)).
    ///
    /// The single-threaded signal callbacks below
    /// ([`on_page_committed`](CoreSession::on_page_committed) & co.) run this
    /// INLINE. The [`SyncSession`] page-signal path instead RECORDS the signals
    /// off the session lock and runs the pump on the next locked read (the
    /// deferred pump — see [`SyncSession::on_page_committed`]).
    pub fn pump(&mut self) {
        self.shell.pump();
    }

    /// A CLONE of the session's backend handle: the same shared inner the
    /// shell's backend drives, reachable WITHOUT borrowing the session.
    ///
    /// This is the clone-boundary the [`SyncSession`] page-signal path is built
    /// on (task `mobile-page-signal-callbacks-off-session-lock`): the handle's
    /// inner is an `Arc<Mutex<_>>` whose holds are microsecond field accesses,
    /// so the UI thread's page-signal callbacks can RECORD through it while a
    /// worker thread holds the session lock for a retrieval — the debug-capture
    /// clone-out precedent ([`debug_capture`](CoreSession::debug_capture))
    /// applied to the WebView load signals.
    #[must_use]
    pub fn backend_handle(&self) -> AndroidHandle {
        self.backend.clone()
    }

    /// The URL (if any) the core has committed to but the platform `WebView` has
    /// not yet loaded. Kotlin drains this after driving the session and calls
    /// `WebView.loadUrl` with it.
    pub fn take_pending_load(&mut self) -> Option<String> {
        self.backend.take_pending_load()
    }

    /// Resolve an intercepted `ipfs://<cid>[/path]` request through the SHARED
    /// core resolve path, for Kotlin's `WebViewClient.shouldInterceptRequest`.
    ///
    /// The platform `WebView` dies on an `ipfs://` URL with
    /// `net::ERR_UNKNOWN_URL_SCHEME`, so Kotlin intercepts the request and calls
    /// this: it routes `uri` through the `ipfs` scheme handler installed at
    /// [`new`](CoreSession::new) (the SAME `resolve_ipfs_request` +
    /// trustless-gateway CAR path desktop uses), and returns the verified bytes +
    /// MIME type, or the fail-closed reason. Returns `None` if `uri` is not a
    /// registered scheme (Kotlin then lets the `WebView` handle it normally).
    pub fn resolve_ipfs(&self, uri: &str) -> Option<SchemeResolution> {
        // Only the `ipfs` scheme's success is a hash-verified content load that
        // earns the content-verified posture. Kotlin's `shouldInterceptRequest`
        // routes EVERY intercepted scheme through this one method (the generic
        // `resolve_scheme` dispatches `ipfs` AND `werust`), so a `werust://settings`
        // page must NOT be marked content-verified — it is an internal chrome page,
        // not hash-verified content. Scope the mark to the `ipfs` scheme.
        //
        // The check runs on the MAPPED-BACK form
        // ([`origin_map::from_webview_url`], identity for anything off the
        // internal origin): a page served on the internal `https://<cid>.ipfs
        // .werust.invalid` origin has its requests intercepted as `https://…`
        // URLs, but they ARE `ipfs://` requests and their verified resolution
        // must earn the content-verified mark exactly as a direct `ipfs://`
        // request does.
        let uri = origin_map::from_webview_url(uri);
        let is_ipfs = uri
            .split_once("://")
            .is_some_and(|(scheme, _)| scheme == werust_core::ipfs::IPFS_SCHEME);
        self.backend
            .resolve_scheme(&uri)
            .map(|result| match result {
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
    /// Kotlin installs them onto the platform `WebView` as page-start user
    /// scripts. This is the `inject_script` half of the provider bridge, made real
    /// on the Android edge.
    #[must_use]
    pub fn document_start_scripts(&self) -> Vec<String> {
        self.backend.document_start_scripts()
    }

    /// Dispatch a page-posted EIP-1193 envelope on script-message channel `name`
    /// through the registered provider handler and return the response JS Kotlin
    /// must run in the live page (via `WebView.evaluateJavascript`) to settle the
    /// page's pending Promise. Routed through the SAME `werust_core::provider` path
    /// desktop uses; `None`/empty means the channel is unregistered or the message
    /// needed no response.
    #[must_use]
    pub fn handle_provider_message(&self, name: &str, body: &str) -> Vec<String> {
        self.backend.handle_script_message(name, body)
    }

    /// Report the platform `WebView`'s commit signal into the core, then fold the
    /// resulting lifecycle events into the chrome.
    pub fn on_page_committed(&mut self, url: &str) {
        self.backend.on_page_committed(url);
        self.shell.pump();
    }

    /// Report the platform `WebView`'s finished signal into the core.
    pub fn on_page_finished(&mut self, url: &str) {
        self.backend.on_page_finished(url);
        self.shell.pump();
    }

    /// Report the platform `WebView`'s error signal into the core.
    pub fn on_page_failed(&mut self, url: &str, reason: &str) {
        self.backend.on_page_failed(url, reason);
        self.shell.pump();
    }

    /// Report a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
    /// client-side navigation) into the core, then fold the resulting
    /// `UrlChanged` event into the chrome so the URL bar FOLLOWS the new location
    /// (dropping a pinned `.eth` name / re-deriving an ENS identity) instead of
    /// freezing. Called from Kotlin's `WebViewClient.doUpdateVisitedHistory`.
    pub fn on_url_changed(&mut self, url: &str) {
        self.backend.on_url_changed(url);
        self.shell.pump();
    }

    /// The current [`ChromeState`] the Kotlin edge paints its URL bar, nav-control
    /// enablement, and status line from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        self.shell.chrome()
    }

    /// The current [`ChromeState`] as a JSON object, the wire form Kotlin reads
    /// across JNI (a single string return is the simplest robust JNI marshalling).
    ///
    /// The document is encoded by the SHARED core
    /// ([`werust_core::chrome_json`]), not here: it carries the chrome FACTS plus
    /// every string the core's presentation rules DERIVE from them (the status
    /// line, the trust badge + its explanation, the banner text, the badge text,
    /// the progress fraction + hint), so the Kotlin edge reads a field instead of
    /// re-deriving the rule (task
    /// `mobile-chrome-presentation-from-one-derivation`). It used to be encoded by
    /// a local `ffi_json` module that was a byte-for-byte twin of the iOS one:
    /// the same duplication one level down.
    #[must_use]
    pub fn chrome_json(&self) -> String {
        werust_core::chrome_json(self.shell.chrome())
    }

    /// The shared bounded CONSOLE + NETWORK capture store behind the in-app debug
    /// menu ([`werust_core::debug::DebugCapture`]).
    ///
    /// Both the PUSH surface the Android capture points feed (Kotlin's
    /// `WebChromeClient.onConsoleMessage` and `shouldInterceptRequest`, task
    /// `debug-console-network-capture-per-platform`) and the store the debug view
    /// reads/clears. It is the SHELL's store, so the Kotlin debug view renders
    /// exactly what desktop and iOS render.
    #[must_use]
    pub fn debug_capture(&self) -> &werust_core::debug::DebugCapture {
        self.shell.debug_capture()
    }

    /// The capture store as its own JSON document, the wire form Kotlin's debug
    /// view reads across JNI: a DEDICATED accessor beside
    /// [`chrome_json`](CoreSession::chrome_json) rather than a section of the
    /// chrome JSON, so the chrome (re-encoded on every refresh) stays lean and
    /// every existing chrome reader is unaffected.
    #[must_use]
    pub fn debug_json(&self) -> String {
        self.shell.debug_json()
    }
}

/// Map a core URL to the URL the platform `WebView` should load
/// ([`origin_map::to_webview_url`]): `ipfs://<cid>[/path]` -> the internal
/// `https://<cid>.ipfs.werust.invalid[/path]` origin, anything else unchanged.
///
/// SESSION-FREE (a pure function), for the ONE Kotlin call site that loads a
/// URL the core did not surface as a pending load: the `_blank`/`window.open`
/// transport in `BrowserActivity.onCreateWindow`, which hands its target to
/// `WebView.loadUrl` directly. Mapping there too keeps an `ipfs://` new-window
/// target on the SAME internal-origin path as every other load (an unmapped
/// `ipfs://` main-frame load would land the page on the opaque origin again —
/// the root cause of the mobile no-navigation, task
/// `mobile-ronan-eth-buttons-no-navigation`).
#[must_use]
pub fn to_webview_url(url: &str) -> String {
    origin_map::to_webview_url(url)
}

/// werust's version string for the Kotlin edge's browser MENU: the ONE shared
/// source ([`werust_core::version`], resolved once at build time from the release
/// tag / `git describe` / the Cargo version), so the Android menu shows exactly
/// what the desktop popover and the iOS menu show.
///
/// SESSION-FREE on purpose (unlike every accessor above): the version and the
/// menu are properties of the BUILD, not of a browsing session, so the Kotlin
/// edge can show them without a live native session, and no `CoreSession` is
/// borrowed to read a constant. The recorded rationale is in
/// `docs/spikes/general-browser-menu-with-version-and-debug-entry/DECISIONS.md`.
#[must_use]
pub fn version() -> &'static str {
    werust_core::version()
}

/// The general browser MENU as the JSON document the Kotlin edge builds its
/// native `PopupMenu` from ([`werust_core::menu::menu_json`]): the version line
/// plus the Debug entry, each with its stable id and kind.
///
/// Session-free for the same reason as [`version`]. The Kotlin edge renders
/// whatever items this lists, so a FUTURE menu item added in `werust-core`
/// appears on Android with no Kotlin change.
#[must_use]
pub fn menu_json() -> String {
    werust_core::menu::menu_json(&werust_core::menu::BrowserMenu::new())
}

/// The thread-safety boundary between the Kotlin edge's TWO threads and the
/// single-threaded [`CoreSession`].
///
/// # Why this exists (the Android-only data race)
///
/// The [`CoreSession`] is single-threaded by construction: its shell owns the
/// backend as a `Box<dyn Renderer>` and every shell method assumes it is the
/// ONLY thing touching the session. Desktop and iOS honour that: their scheme
/// handlers dispatch on the single main/GTK thread, so the whole session is
/// only ever driven from one thread.
///
/// Android is the exception. The platform `WebView` runs
/// `WebViewClient.shouldInterceptRequest` on a WebView WORKER thread, while the
/// UI thread independently drives the SAME session during an in-flight load
/// (`navigate`/`onPageStarted`/`onPageFinished` + sub-resource interception).
/// Without a boundary, the worker thread's [`resolve_ipfs`](CoreSession::resolve_ipfs)
/// races the UI thread's navigate / load-signal calls on the backend's shared
/// state: two live accesses across threads = a data race / UB.
///
/// `SyncSession` closes that gap: it wraps the session in a [`Mutex`] and every
/// SESSION-DRIVING or SHELL-READING call goes through it, so the worker-thread
/// resolve and the UI-thread drive are serialized. The lock is the SAME
/// single-thread invariant desktop/iOS get for free from their single-threaded
/// dispatch, made explicit on the one edge that needs it.
///
/// # The TWO lock-free paths (clone-handle boundaries)
///
/// Not every call goes through the mutex. Two kinds of state are reachable
/// through a CLONE of thread-safe shared state held BESIDE the mutex, so their
/// callers never queue behind a lock a worker thread can hold for SECONDS (a
/// CAR retrieval inside `resolve_ipfs` — the ANR shape of the v0.2.7 field
/// finding, where a same-document URL update from a SvelteKit `pushState`
/// froze the UI thread long enough to raise Android's "kill app / wait?"
/// dialog):
///
/// * The DEBUG CAPTURE store (`Arc<Mutex<_>>` clone — see
///   [`debug_capture`](SyncSession::debug_capture)), the precedent.
/// * The BACKEND's shared inner (`Arc<Mutex<_>>` via the cloned
///   [`AndroidHandle`]) for the four UI-thread PAGE-SIGNAL callbacks
///   ([`on_page_committed`](SyncSession::on_page_committed) /
///   [`on_page_finished`](SyncSession::on_page_finished) /
///   [`on_page_failed`](SyncSession::on_page_failed) /
///   [`on_url_changed`](SyncSession::on_url_changed), task
///   `mobile-page-signal-callbacks-off-session-lock`). No inner lock is ever
///   held across I/O (see [`AndroidHandle::resolve_scheme`]), so these contend
///   for microseconds at worst.
///
/// Every WORKER-thread caller stays ON the mutex: `resolve_ipfs` AND
/// `handle_provider_message` (the two long-lived lockers), plus the
/// shorter-lived `document_start_scripts`. They serialize against the
/// UI/executor thread's session drive exactly as before; only the page-signal
/// callbacks moved off, and taking a worker caller off would silently lose
/// that serialisation during a long provider call.
///
/// # The deferred pump
///
/// The lock-free page-signal callbacks RECORD the signal (load state + event +
/// history) but do NOT pump the shell — the pump needs the session, which is
/// exactly what is locked. The fold into the chrome happens in the pump-first
/// locked reads ([`take_pending_load`](SyncSession::take_pending_load) and
/// [`chrome_json`](SyncSession::chrome_json)), which the Kotlin edge calls
/// immediately after every signal (`afterCoreAction`), so the chrome — and the
/// `_redirects` 3xx pending-load hand-off — behave exactly as when the
/// callbacks pumped inline.
///
/// RESIDUAL (recorded, per the task's "only lock-free path" prescription): the
/// two pump-first READS still take the mutex, so the UI thread can still wait
/// out the REMAINDER of one in-flight retrieval on a read (bounded, and
/// unchanged from before — the callbacks, the cumulative multi-retrieval
/// freeze, are what moved off). If the on-device verification still reproduces
/// the dialog, a chrome snapshot / clone-handle read path is the documented
/// follow-up (`docs/spikes/mobile-page-signal-callbacks-off-session-lock/MANUAL-VERIFICATION.md`).
///
/// # Soundness of a `Mutex` over a `!Send` session
///
/// [`CoreSession`] is `!Send` (its shell owns a `Box<dyn Renderer>`), so
/// `SyncSession` is itself `!Send`/`!Sync` at the type level and the JNI layer
/// crosses only a raw pointer (never a typed `Send` reference). The `Mutex`
/// provides the actual mutual exclusion + happens-before for the session: one
/// thread drives the shell at a time, with a release/acquire edge between
/// them — the same guarantee a single dispatch thread would give. The cloned
/// [`AndroidHandle`] needs no such discipline: its `Arc<Mutex<_>>` inner is
/// `Send + Sync` in its own right, so the lock-free paths above are sound
/// without the session lock.
pub struct SyncSession {
    inner: Mutex<CoreSession>,
    /// A CLONE of the session's [`DebugCapture`](werust_core::debug::DebugCapture),
    /// held BESIDE the mutex so the debug capture points never touch the session
    /// lock (see [`debug_capture`](SyncSession::debug_capture)).
    debug: werust_core::debug::DebugCapture,
    /// A CLONE of the session's [`AndroidHandle`], held BESIDE the mutex so the
    /// UI thread's page-signal callbacks record through the backend's
    /// `Arc<Mutex<_>>` inner WITHOUT the session lock (see the struct doc's
    /// "lock-free paths" section). Both clones are the same shared inner the
    /// shell's backend drives.
    backend: AndroidHandle,
}

impl Default for SyncSession {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncSession {
    /// Build a fresh synchronized session over a new [`CoreSession`].
    #[must_use]
    pub fn new() -> Self {
        let session = CoreSession::new();
        // Clone the shared capture handle OUT once, at construction, so every
        // later capture push reaches the store WITHOUT taking the session lock
        // (see `debug_capture`). Both clones are the same store.
        let debug = session.debug_capture().clone();
        // Clone the backend handle OUT once too, at construction, so the
        // page-signal callbacks record through the shared inner WITHOUT taking
        // the session lock (see the struct doc's "lock-free paths" section).
        let backend = session.backend_handle();
        Self {
            inner: Mutex::new(session),
            debug,
            backend,
        }
    }

    /// Run `f` against the guarded [`CoreSession`] while holding the lock, so no
    /// other thread can borrow the session's `RefCell` for the duration.
    ///
    /// A poisoned lock (a prior panic while holding it) is recovered into the
    /// guard rather than propagated: the edge must stay responsive, and the
    /// session's own methods are internally consistent (a panic mid-borrow is a
    /// bug we would rather surface as a degraded-but-live session than a crash on
    /// every subsequent call).
    fn with<R>(&self, f: impl FnOnce(&mut CoreSession) -> R) -> R {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut guard)
    }

    /// Navigate to `url`, under the lock. See [`CoreSession::navigate`].
    pub fn navigate(&self, url: &str) -> bool {
        self.with(|s| s.navigate(url))
    }

    /// Go one step back, under the lock. See [`CoreSession::go_back`].
    pub fn go_back(&self) {
        self.with(CoreSession::go_back);
    }

    /// Go one step forward, under the lock. See [`CoreSession::go_forward`].
    pub fn go_forward(&self) {
        self.with(CoreSession::go_forward);
    }

    /// Reload, under the lock. See [`CoreSession::reload`].
    pub fn reload(&self) -> bool {
        self.with(CoreSession::reload)
    }

    /// Stop the in-flight load, under the lock. See [`CoreSession::stop`].
    pub fn stop(&self) {
        self.with(CoreSession::stop);
    }

    /// Drain the pending load, under the lock, PUMPING first. See
    /// [`CoreSession::take_pending_load`].
    ///
    /// The pump-first fold is where a page signal RECORDED off the session lock
    /// (the lock-free path — see [`on_page_committed`](SyncSession::on_page_committed))
    /// lands in the chrome, and it must happen in the SAME locked call as the
    /// drain: the pump is what turns a `_redirects` 3xx into the queued pending
    /// load (task `ipfs-redirects-3xx-navigation-support`), so draining before
    /// the fold would miss the redirect.
    pub fn take_pending_load(&self) -> Option<String> {
        self.with(|s| {
            s.pump();
            s.take_pending_load()
        })
    }

    /// Resolve an intercepted `ipfs://` request, under the lock. This is the
    /// method the WebView WORKER thread calls from `shouldInterceptRequest`; the
    /// lock serializes it against the UI thread's navigate / load-signal calls so
    /// the shared `RefCell` is never borrowed by two threads at once. See
    /// [`CoreSession::resolve_ipfs`].
    pub fn resolve_ipfs(&self, uri: &str) -> Option<SchemeResolution> {
        self.with(|s| s.resolve_ipfs(uri))
    }

    /// The document-start provider scripts, under the lock. See
    /// [`CoreSession::document_start_scripts`].
    #[must_use]
    pub fn document_start_scripts(&self) -> Vec<String> {
        self.with(|s| s.document_start_scripts())
    }

    /// Dispatch an EIP-1193 provider envelope, under the lock. This is called from
    /// the WebView WORKER/JS-interface thread, so the lock serializes it against
    /// the UI thread's navigate / load-signal calls exactly as `resolve_ipfs` is.
    /// See [`CoreSession::handle_provider_message`].
    #[must_use]
    pub fn handle_provider_message(&self, name: &str, body: &str) -> Vec<String> {
        self.with(|s| s.handle_provider_message(name, body))
    }

    /// Report the commit signal, OFF the session lock. The semantics are
    /// [`CoreSession::on_page_committed`]'s; the difference at this layer is
    /// the PATH and the PUMP:
    ///
    /// * PATH: the signal is recorded through the cloned [`AndroidHandle`] (the
    ///   backend's `Arc<Mutex<_>>` inner), never `self.with(...)`. Called from
    ///   the Android UI thread, so it must NEVER queue behind the session lock
    ///   a WebView worker thread holds for a multi-second CAR retrieval inside
    ///   `resolve_ipfs` — the ANR shape of the v0.2.7 field finding (the
    ///   ronan.eth "kill app / wait?" dialog, task
    ///   `mobile-page-signal-callbacks-off-session-lock`).
    /// * PUMP: DEFERRED, unlike [`CoreSession::on_page_committed`]'s inline
    ///   pump — the pump needs the session, which is exactly what is locked.
    ///   The fold into the chrome happens in the pump-first locked reads
    ///   ([`take_pending_load`](SyncSession::take_pending_load) /
    ///   [`chrome_json`](SyncSession::chrome_json)), which the Kotlin edge
    ///   calls immediately after every signal (`afterCoreAction`), so the
    ///   chrome behaves exactly as when the callbacks pumped inline.
    pub fn on_page_committed(&self, url: &str) {
        self.backend.on_page_committed(url);
    }

    /// Report the finished signal, OFF the session lock. See
    /// [`CoreSession::on_page_finished`] for the semantics and
    /// [`on_page_committed`](SyncSession::on_page_committed) for why this
    /// records through the clone-handle boundary with the pump deferred.
    pub fn on_page_finished(&self, url: &str) {
        self.backend.on_page_finished(url);
    }

    /// Report the error signal, OFF the session lock. See
    /// [`CoreSession::on_page_failed`] for the semantics and
    /// [`on_page_committed`](SyncSession::on_page_committed) for why this
    /// records through the clone-handle boundary with the pump deferred.
    pub fn on_page_failed(&self, url: &str, reason: &str) {
        self.backend.on_page_failed(url, reason);
    }

    /// Report a same-document URL change, OFF the session lock. See
    /// [`CoreSession::on_url_changed`] for the semantics and
    /// [`on_page_committed`](SyncSession::on_page_committed) for the path.
    ///
    /// This is THE callback the v0.2.7 freeze pinned: an SPA `pushState` fires
    /// `doUpdateVisitedHistory` on the UI thread WHILE the client router's
    /// `__data.json` round-trip keeps the worker thread holding the session
    /// lock — the URL-bar update now records in microseconds and the fold
    /// follows on the next chrome read, instead of the UI thread freezing
    /// behind the retrieval.
    pub fn on_url_changed(&self, url: &str) {
        self.backend.on_url_changed(url);
    }

    /// The current chrome as a JSON object, under the lock, PUMPING first so a
    /// page signal RECORDED off the session lock is folded into the chrome
    /// before the encode (the deferred pump — see
    /// [`on_page_committed`](SyncSession::on_page_committed)). See
    /// [`CoreSession::chrome_json`].
    #[must_use]
    pub fn chrome_json(&self) -> String {
        self.with(|s| {
            s.pump();
            s.chrome_json()
        })
    }

    /// The debug capture store as a JSON document, read OFF the session lock.
    ///
    /// The debug VIEW polls this on its refresh cadence from the UI thread, and
    /// the store is the same `Arc`-shared handle the capture points push into (see
    /// [`debug_capture`](SyncSession::debug_capture)), so encoding it needs no
    /// session at all. Reading it through the session lock would put a UI-thread
    /// poll behind an in-flight `resolve_ipfs` retrieval — the ANR shape user
    /// story 4 forbids.
    #[must_use]
    pub fn debug_json(&self) -> String {
        werust_core::debug::debug_json(&self.debug)
    }

    /// The shared capture store, reachable WITHOUT the session lock.
    ///
    /// # Why this is not `self.with(|s| s.debug_capture())` (the ANR guard)
    ///
    /// Every other method here funnels through the session lock, which is exactly
    /// right for a call that DRIVES the session. A debug capture push is not that
    /// call. `WebChromeClient.onConsoleMessage` runs on the Android **UI thread**,
    /// while `resolve_ipfs` can hold this same lock for SECONDS on a WebView
    /// worker thread during a CAR retrieval (`docs/adr/0008`). Pushing a console
    /// entry through the session boundary would therefore block the UI thread
    /// behind a content retrieval — precisely the ANR shape the spec's user story
    /// 4 exists to prevent, and precisely what the off-main-thread work fixed.
    ///
    /// [`DebugCapture`](werust_core::debug::DebugCapture) is an `Arc<Mutex<_>>`
    /// handle for this reason: a capture point needs no `&mut` session at all. So
    /// a clone is taken ONCE at construction and held beside the mutex; a push
    /// contends only on the store's own short-lived lock (a bounded ring-buffer
    /// insert, no I/O), never on the session.
    #[must_use]
    pub fn debug_capture(&self) -> &werust_core::debug::DebugCapture {
        &self.debug
    }

    /// Capture one CONSOLE entry, OFF the session lock. Called from the WebView UI
    /// thread (`WebChromeClient.onConsoleMessage`); see
    /// [`debug_capture`](SyncSession::debug_capture) for why it must not go
    /// through the session boundary.
    pub fn push_console_entry(&self, entry: werust_core::debug::ConsoleEntry) {
        self.debug.push_console(entry);
    }

    /// Capture one NETWORK entry, OFF the session lock. Called from the WebView
    /// WORKER thread (`shouldInterceptRequest`) for BOTH the intercepted and the
    /// passed-through requests, so it must never serialize behind an in-flight
    /// retrieval.
    pub fn push_network_entry(&self, entry: werust_core::debug::NetworkEntry) {
        self.debug.push_network(entry);
    }

    /// Empty the capture store (the debug view's Clear action), off the session
    /// lock like the pushes.
    pub fn clear_debug_capture(&self) {
        self.debug.clear();
    }

    /// Capture one console message Kotlin's `WebChromeClient.onConsoleMessage`
    /// reported, mapping the platform's `ConsoleMessage` fields onto a core
    /// [`ConsoleEntry`](werust_core::debug::ConsoleEntry).
    ///
    /// Android is the ONE platform with a REAL native console callback, so it does
    /// NOT use the injected shim desktop and iOS share: `onConsoleMessage` hands
    /// over message / `messageLevel` / `sourceId` / `lineNumber` directly, which is
    /// strictly better (it sees engine-emitted messages a page-side wrapper never
    /// could, and cannot be un-wrapped by the page). The level name is mapped
    /// through the ONE shared
    /// [`ConsoleLevel::from_platform`](werust_core::debug::ConsoleLevel::from_platform),
    /// so Android's `WARNING`/`TIP` land in the same vocabulary the shim's
    /// `warn`/`info` do. Recorded in
    /// `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`.
    ///
    /// Runs on the UI thread, so it goes through the lock-free
    /// [`debug_capture`](SyncSession::debug_capture) path.
    pub fn capture_console(&self, level: &str, message: &str, source: &str, line: u32) {
        use werust_core::debug::{console_entry, ConsoleLevel};
        self.push_console_entry(console_entry(
            ConsoleLevel::from_platform(level),
            message,
            source,
            line,
            epoch_millis(),
        ));
    }

    /// Capture one request Kotlin's `WebViewClient.shouldInterceptRequest`
    /// observed, for BOTH the intercepted (`ipfs://`, answered from the core) and
    /// the passed-through (`return null`) requests — which is why Android has the
    /// widest network reach of the three platforms: that hook sees EVERY request.
    ///
    /// `verified` must say whether THIS request's bytes really came back through
    /// the hash-verified content-addressed path (i.e. the core's resolution
    /// succeeded for an `ipfs://` request), never whether the URL looks
    /// content-addressed: the shared
    /// [`network_entry`](werust_core::debug::network_entry) derives the honest
    /// posture from it (ADR-0006).
    ///
    /// `main_frame` marks the MAIN-DOCUMENT row, which additionally takes the
    /// LOAD's own two-axis posture so the Network tab cannot contradict the chrome
    /// trust indicator on the same screen. That is the ONE case that reads the
    /// session (under the lock), and it runs on the WebView WORKER thread that
    /// already locks for `resolve_ipfs` — never on the UI thread. Every other row
    /// (every sub-resource, the whole hot path) stays entirely off the lock.
    ///
    /// Capture is READ-ONLY: it does not decide, alter or delay what
    /// `shouldInterceptRequest` returns.
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
        // Record the MAPPED-BACK URL ([`origin_map::from_webview_url`],
        // identity off the internal origin): a page served on the internal
        // `https://<cid>.ipfs.werust.invalid` origin makes its requests as
        // `https://…` URLs, but the Network tab is the USER's diagnosis
        // surface and must speak the real `ipfs://` URLs the core everywhere
        // else reports — the internal origin is an edge detail, never a fact
        // the user should have to decode.
        let url = origin_map::from_webview_url(url);
        let mut entry = werust_core::debug::network_entry(
            method,
            &url,
            Some(status),
            mime,
            Some(size),
            verified,
            epoch_millis(),
        );
        if main_frame {
            // The two-axis reconciliation (the store's DECISIONS.md Decision 4):
            // on an ENS-named page the indicator shows `name-via-trusted-rpc`, so
            // the main-document row must show that too rather than the plainer
            // per-request `content-verified`. Read the LIVE posture (the seam's
            // `Renderer::trust_posture`, the same fact the desktop capture reads
            // from its load lifecycle), NOT the cached `chrome().trust_posture`
            // snapshot: this capture runs BEFORE `onPageCommitted`/
            // `onPageFinished` pump the shell and `refresh_chrome` re-caches, so
            // the cache still holds the stale pre-verify `unverified-origin`
            // here and would DOWNGRADE the row below the honest posture.
            entry = entry.with_trust(self.with(|s| s.shell.live_trust_posture()));
        }
        self.push_network_entry(entry);
    }
}

/// Milliseconds since the Unix epoch, for a captured debug entry's timestamp
/// (`0` if the clock is before the epoch).
///
/// The core store takes a caller-supplied timestamp so it binds no clock (the
/// store's DECISIONS.md Decision 6); this is the Android edge's supply, taken
/// here rather than in Kotlin so all three edges stamp entries the same way.
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
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
/// handler is dispatched by the OS edge (Kotlin `shouldInterceptRequest`) via
/// [`CoreSession::resolve_ipfs`], not by a webview signal.
fn install_ipfs(backend: &mut AndroidBackend) -> werust_core::ipfs::RedirectSink {
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
    // seam so Kotlin's `shouldInterceptRequest` for `werust` serves it and a
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
/// page's `window.ethereum` sees the SAME injected EIP-1193 provider on Android
/// as on desktop.
///
/// Unlike desktop (which pushes the response by evaluating JS on its GTK loop),
/// the mobile backend owns no live view, so the handler QUEUES the response JS
/// via the backend handle's `queue_eval`; Kotlin drains it (through
/// [`CoreSession::handle_provider_message`]) and runs it with
/// `WebView.evaluateJavascript`. The bridge holds NO keys (a read-only stub), the
/// same security posture as desktop.
fn install_provider(backend: &mut AndroidBackend) {
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
// JNI export surface. These are the mechanical `Java_<pkg>_<Class>_<method>`
// bridges the Kotlin `WerustCore` class (package com.github.wighawag.werust)
// declares as `external`. They only marshal to/from the JVM and delegate to
// `CoreSession`; all logic lives above so it is JVM-free-testable.
//
// The session is handed to Kotlin as an opaque `jlong` pointer from
// `nativeNew`, threaded back through every call, and freed by `nativeFree`.
// ---------------------------------------------------------------------------
#[cfg(target_os = "android")]
mod jni_exports {
    use super::SyncSession;
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
    use jni::JNIEnv;

    /// Reconstruct a `&SyncSession` from the opaque handle Kotlin threads back.
    ///
    /// The handle points at a [`SyncSession`], not a bare `CoreSession`, so a
    /// shared `&` is enough: every session-DRIVING method locks the inner
    /// `Mutex` before touching the `CoreSession`, and the two lock-free paths
    /// (the page-signal callbacks and the debug capture) go through their own
    /// thread-safe clone handles. This is what makes the two Kotlin threads
    /// safe — the UI thread's navigate / load-signal calls and the WebView
    /// worker thread's `shouldInterceptRequest` -> `nativeResolveIpfs` can hold
    /// this `&` at the same time, with the `Mutex` serializing the actual
    /// session access and the clone handles serializing their own shared state.
    ///
    /// # Safety
    /// `handle` must be a pointer returned by `nativeNew` and not yet freed by
    /// `nativeFree`; Kotlin guarantees this by construction (one handle per
    /// `Activity`, threaded through every call, freed once in `onDestroy`).
    unsafe fn session<'a>(handle: jlong) -> &'a SyncSession {
        &*(handle as *const SyncSession)
    }

    fn read(env: &mut JNIEnv, s: &JString) -> String {
        env.get_string(s).map(|js| js.into()).unwrap_or_default()
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeNew(
        _env: JNIEnv,
        _class: JClass,
    ) -> jlong {
        Box::into_raw(Box::new(SyncSession::new())) as jlong
    }

    /// # Safety
    /// `handle` must be a live handle from `nativeNew`, freed at most once.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeFree(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        if handle != 0 {
            drop(Box::from_raw(handle as *mut SyncSession));
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeNavigate(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) -> jboolean {
        let url = read(&mut env, &url);
        let ok = unsafe { session(handle) }.navigate(&url);
        if ok {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeGoBack(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.go_back();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeGoForward(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.go_forward();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeReload(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jboolean {
        if unsafe { session(handle) }.reload() {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeStop(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.stop();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeTakePendingLoad(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let pending = unsafe { session(handle) }.take_pending_load();
        // An empty string means "no pending load" on the Kotlin side.
        let s = pending.unwrap_or_default();
        env.new_string(s)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Resolve an intercepted `ipfs://` request through the shared core path.
    ///
    /// Returns an opaque handle to a boxed [`SchemeResolution`] Kotlin queries via
    /// `nativeResolutionIsOk` / `nativeResolutionMime` / `nativeResolutionBody` /
    /// `nativeResolutionError` and then frees with `nativeResolutionFree`. A `0`
    /// return means the URI was not an intercepted scheme (Kotlin lets the
    /// `WebView` handle it normally). Bytes cross the boundary as a `jbyteArray`
    /// via `nativeResolutionBody`, kept out of the JSON chrome wire form.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolveIpfs(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        uri: JString,
    ) -> jlong {
        let uri = read(&mut env, &uri);
        match unsafe { session(handle) }.resolve_ipfs(&uri) {
            Some(resolution) => Box::into_raw(Box::new(resolution)) as jlong,
            None => 0,
        }
    }

    /// Reconstruct a `&SchemeResolution` from the opaque handle Kotlin threads back.
    ///
    /// # Safety
    /// `handle` must be a non-zero pointer from `nativeResolveIpfs` not yet freed.
    unsafe fn resolution<'a>(handle: jlong) -> &'a super::SchemeResolution {
        &*(handle as *const super::SchemeResolution)
    }

    /// Whether the resolution succeeded (a verified load) vs a fail-closed error.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionIsOk(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jboolean {
        match unsafe { resolution(handle) } {
            super::SchemeResolution::Ok { .. } => JNI_TRUE,
            super::SchemeResolution::Err { .. } => JNI_FALSE,
        }
    }

    /// The MIME type of a successful resolution (empty string on an error result).
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionMime(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let mime = match unsafe { resolution(handle) } {
            super::SchemeResolution::Ok { mime_type, .. } => mime_type.as_str(),
            super::SchemeResolution::Err { .. } => "",
        };
        env.new_string(mime)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// The verified body bytes of a successful resolution (empty array on error).
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionBody(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jni::sys::jbyteArray {
        let empty = Vec::new();
        let body: &[u8] = match unsafe { resolution(handle) } {
            super::SchemeResolution::Ok { body, .. } => body,
            super::SchemeResolution::Err { .. } => &empty,
        };
        env.byte_array_from_slice(body)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// The HTTP-equivalent status of a successful resolution (0 on an error
    /// result, which Kotlin answers with its own fail-closed status instead).
    ///
    /// Almost always 200. It is exposed so the Kotlin edge can answer a
    /// `WebResourceResponse` with the HONEST status when a site's `_redirects`
    /// (IPIP-0002) names its own error page for a path that is not in its DAG
    /// (`/* /404.html 404`) — the page renders, but as the not-found it is.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionStatus(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jni::sys::jint {
        match unsafe { resolution(handle) } {
            super::SchemeResolution::Ok { status, .. } => jni::sys::jint::from(*status),
            super::SchemeResolution::Err { .. } => 0,
        }
    }

    /// The fail-closed reason of an error resolution (empty string on success).
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionError(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let reason = match unsafe { resolution(handle) } {
            super::SchemeResolution::Ok { .. } => "",
            super::SchemeResolution::Err { reason } => reason.as_str(),
        };
        env.new_string(reason)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Free a resolution handle from `nativeResolveIpfs`.
    ///
    /// # Safety
    /// `handle` must be a non-zero pointer from `nativeResolveIpfs`, freed once.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeResolutionFree(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        if handle != 0 {
            drop(Box::from_raw(handle as *mut super::SchemeResolution));
        }
    }

    /// The document-start scripts (the EIP-1193 provider shim) as a single string
    /// Kotlin injects onto the platform `WebView` at page start (they are all
    /// document-start user scripts run in order, so concatenation is equivalent).
    /// Empty string means nothing to inject.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeDocumentStartScript(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let script = unsafe { session(handle) }
            .document_start_scripts()
            .join("\n");
        env.new_string(script)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Dispatch an EIP-1193 provider envelope posted from the page on the provider
    /// channel `name` and return the response JS Kotlin must run in the live page
    /// (via `WebView.evaluateJavascript`) to settle the page's pending Promise, as
    /// a single string (the response-delivery calls joined; empty means nothing to
    /// run). This is the page -> native -> page provider round-trip on Android; it
    /// runs on the JS-interface thread, serialized by the `SyncSession` mutex
    /// against the UI thread exactly like `nativeResolveIpfs`.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeHandleProviderMessage(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        name: JString,
        body: JString,
    ) -> jstring {
        let name = read(&mut env, &name);
        let body = read(&mut env, &body);
        let script = unsafe { session(handle) }
            .handle_provider_message(&name, &body)
            .join("\n");
        env.new_string(script)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageCommitted(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) {
        let url = read(&mut env, &url);
        unsafe { session(handle) }.on_page_committed(&url);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageFinished(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) {
        let url = read(&mut env, &url);
        unsafe { session(handle) }.on_page_finished(&url);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageFailed(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
        reason: JString,
    ) {
        let url = read(&mut env, &url);
        let reason = read(&mut env, &reason);
        unsafe { session(handle) }.on_page_failed(&url, &reason);
    }

    /// The session-free core->WebView URL map ([`super::to_webview_url`]),
    /// for the `_blank`/`window.open` transport's direct `WebView.loadUrl`.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeToWebViewUrl(
        mut env: JNIEnv,
        _class: JClass,
        url: JString,
    ) -> jstring {
        let url = read(&mut env, &url);
        let mapped = super::to_webview_url(&url);
        env.new_string(mapped)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnUrlChanged(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) {
        let url = read(&mut env, &url);
        unsafe { session(handle) }.on_url_changed(&url);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeChromeJson(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let json = unsafe { session(handle) }.chrome_json();
        env.new_string(json)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// The debug capture store (console + network) as a JSON document, for the
    /// Kotlin debug view. A DEDICATED accessor beside `nativeChromeJson`: the
    /// chrome JSON is polled on every chrome refresh, this only while the debug
    /// view is open.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeDebugJson(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let json = unsafe { session(handle) }.debug_json();
        env.new_string(json)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Empty the debug capture store: the debug view's Clear action.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeDebugClear(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.clear_debug_capture();
    }

    /// Capture one console message from Kotlin's REAL native
    /// `WebChromeClient.onConsoleMessage` callback (task
    /// `debug-console-network-capture-per-platform`).
    ///
    /// Android needs no injected console shim: this callback reports
    /// message/level/source/line directly. `line` is 1-based; `0` means the
    /// platform reported none. Runs on the UI THREAD and pushes OFF the session
    /// lock (`SyncSession::debug_capture`), so it can never block behind an
    /// in-flight `resolve_ipfs` retrieval (the ANR guard).
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeCaptureConsole(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        level: JString,
        message: JString,
        source: JString,
        line: jint,
    ) {
        let level = read(&mut env, &level);
        let message = read(&mut env, &message);
        let source = read(&mut env, &source);
        unsafe { session(handle) }.capture_console(
            &level,
            &message,
            &source,
            u32::try_from(line).unwrap_or(0),
        );
    }

    /// Capture one request from Kotlin's `WebViewClient.shouldInterceptRequest`,
    /// for BOTH the intercepted (`ipfs://`) and the passed-through requests.
    ///
    /// `verified` must reflect what the request ACTUALLY did (the core resolution
    /// succeeded for an `ipfs://` request), never what the URL looks like;
    /// `main_frame` marks the main-document row, which takes the load's own
    /// two-axis posture. A `0` status/size means unknown and stays honestly absent
    /// in the store. Runs on the WebView WORKER thread and (except for the
    /// main-frame posture read) pushes off the session lock.
    #[no_mangle]
    #[allow(clippy::too_many_arguments)]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeCaptureNetwork(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        method: JString,
        url: JString,
        status: jint,
        mime: JString,
        size: jlong,
        verified: jboolean,
        main_frame: jboolean,
    ) {
        let method = read(&mut env, &method);
        let url = read(&mut env, &url);
        let mime = read(&mut env, &mime);
        unsafe { session(handle) }.capture_network(
            &method,
            &url,
            u16::try_from(status).unwrap_or(0),
            &mime,
            u64::try_from(size).unwrap_or(0),
            verified != JNI_FALSE,
            main_frame != JNI_FALSE,
        );
    }

    /// werust's version string, for the Kotlin browser menu's version line. Takes
    /// NO session handle: the version is a property of the BUILD, and it is the
    /// ONE shared source all three menus read (`werust_core::version`), so no
    /// edge hardcodes a version of its own. (Declared on the Kotlin side as an
    /// ordinary instance external — like every other export here — so its symbol
    /// is this plain `Java_..._WerustCore_nativeVersion`; it simply threads no
    /// handle.)
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeVersion(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        env.new_string(super::version())
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// The general browser MENU as a JSON document, for the Kotlin edge to build
    /// its native `PopupMenu` from. Session-free, like `nativeVersion`.
    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeMenuJson(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        env.new_string(super::menu_json())
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::LoadState;

    /// Drive the in-flight load to done the way the Kotlin edge would from the
    /// platform `WebView`'s commit + finished signals.
    fn settle(session: &mut CoreSession) {
        let url = session
            .take_pending_load()
            .expect("a pending load to apply to the WebView");
        session.on_page_committed(&url);
        session.on_page_finished(&url);
    }

    #[test]
    fn a_typed_url_navigates_through_the_core_and_moves_the_chrome() {
        // The end-to-end Kotlin↔core protocol for the URL bar's Enter action: the
        // core navigates through the seam, surfaces the URL for the WebView, and
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
            "the URL is surfaced for WebView.loadUrl"
        );
        assert_eq!(s.take_pending_load(), None, "drained once");

        // The WebView's real signals (reported by Kotlin) settle the load.
        s.on_page_committed("https://example.com/");
        s.on_page_finished("https://example.com/");
        assert_eq!(s.chrome().load_state, LoadState::Finished);
        assert!(!s.chrome().is_loading());
    }

    #[test]
    fn an_invalid_entry_surfaces_the_badge_and_keeps_the_typed_text_without_loading() {
        // Field finding D: a scheme-less GARBAGE entry does NOT navigate. The core
        // front door handles it (surfacing the distinct invalid-URL state and
        // keeping the typed text), so no pending load is queued for the Kotlin
        // edge and the bar is not reset. The edge already passes the RAW typed
        // text, so the core's classifier is what decides.
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
        // `https://<host>` — the prepend is the CORE's job, so the Kotlin edge
        // gets a pending `https://github.com` to load.
        let mut s = CoreSession::new();
        assert!(s.navigate("github.com"), "a valid host navigates");
        assert!(s.chrome().invalid_entry.is_none());
        assert_eq!(s.take_pending_load().as_deref(), Some("https://github.com"));
    }

    #[test]
    fn back_and_forward_reflect_navigation_state_through_the_core() {
        // Back/Forward availability is the CORE's truth, read by Kotlin from the
        // chrome — Kotlin keeps no history of its own.
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
        // The motivating gap: an `ipfs://` URL the platform WebView cannot load
        // (ERR_UNKNOWN_URL_SCHEME) is intercepted by the Kotlin edge and routed
        // through the SAME core resolve path desktop uses. Here we prove the
        // scheme is intercepted (not `None`) and reaches the core, which fails
        // CLOSED on a malformed CID BEFORE any network fetch (network-isolated):
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
        // platform WebView load it normally (no interception).
        let s = CoreSession::new();
        assert!(s.resolve_ipfs("https://example.com/").is_none());
    }

    #[test]
    fn the_eip1193_provider_bridge_reaches_the_shared_core_through_the_session() {
        // The provider bridge is wired on the Android edge (the seam no-op is
        // gone): the session injects the provider shim at document start, and a
        // page envelope posted on the provider channel round-trips through the SAME
        // `werust_core::provider` path desktop uses to a response push Kotlin runs
        // in the page. Network-isolated: the read-only stub answers keylessly.
        let s = CoreSession::new();

        // The shim is injected so a page's `window.ethereum` is detectable.
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

        // A page `eth_chainId` request round-trips to the keyless stub and back:
        // the response push settles the page's pending Promise with the chain id.
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

        // A message on any other channel is ignored (no response to run).
        assert!(s
            .handle_provider_message("someOtherChannel", r#"{"id":1,"method":"eth_chainId"}"#)
            .is_empty());
    }

    #[test]
    fn the_chrome_trust_posture_reaches_the_kotlin_edge_from_the_core() {
        // The trust indicator is wired on the Android edge (the seam-default
        // `UnverifiedOrigin` was inherited before): the chrome JSON Kotlin paints
        // carries the current load's REAL posture. A fresh load is untrusted; a
        // verified `ipfs` resolution (marked via the session's `resolve_ipfs`
        // success path) surfaces content-verified — matching desktop.
        let mut s = CoreSession::new();
        assert!(s
            .chrome_json()
            .contains("\"trustPosture\":\"unverified-origin\""));

        // Drive a direct `ipfs://<cid>` load and mark it verified the way the OS
        // edge does when the shared `resolve_ipfs` path returns Ok. (We mark via
        // the handle directly to stay network-isolated — a real resolve needs the
        // gateway; the posture PLUMBING is what this asserts.)
        assert!(s.navigate("ipfs://bafycid/"));
        assert!(
            s.chrome_json()
                .contains("\"trustPosture\":\"unverified-origin\""),
            "untrusted until the bytes verify"
        );
        // The OS edge intercepts the request, the shared resolve verifies the
        // bytes (marking the load), then the WebView reports its commit/finish
        // signals which pump the shell and refresh the chrome from the seam's
        // posture — exactly the real Android signal flow.
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
    fn an_internal_werust_settings_page_is_not_marked_content_verified() {
        // Kotlin's `shouldInterceptRequest` routes EVERY intercepted scheme through
        // `resolve_ipfs` (the generic dispatch serves `ipfs` AND `werust`), so the
        // internal `werust://settings` chrome page must NOT earn the
        // content-verified posture — it is not hash-verified content. Only the
        // `ipfs` scheme's success marks the load verified.
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
    fn chrome_json_carries_every_field_the_kotlin_edge_paints() {
        // The JSON is the wire form Kotlin reads across JNI; it must carry the URL
        // bar text, nav-control enablement, load state, and any failure so the
        // Kotlin edge can paint the whole chrome without any logic of its own.
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

    // --- The Android-only thread-safety boundary (the requeue's Gate-2 fix). ---

    /// A `Send` shim carrying the raw `*mut SyncSession` across the thread
    /// boundary, exactly as the JNI layer does: the pointer crosses threads but
    /// the `Mutex` inside `SyncSession` is what serializes the actual access, so
    /// this is sound for the same reason the JNI `jlong` handle is.
    struct SessionPtr(*mut SyncSession);
    // SAFETY: the pointer is only ever dereferenced through `SyncSession`'s
    // locking methods (`&self` + inner `Mutex`), so the `!Send` `CoreSession` is
    // only ever touched by one thread at a time under the lock — the same
    // invariant the Kotlin UI-thread + WebView-worker-thread edge relies on.
    unsafe impl Send for SessionPtr {}

    #[test]
    fn the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread() {
        // The requeue's Gate-2 data race, reproduced and closed on the WORKER
        // side. The WebView worker side has TWO session-lock callers, not one:
        // `shouldInterceptRequest` -> `resolve_ipfs` AND the provider
        // JS-interface -> `handle_provider_message` (and `document_start_scripts`
        // / `debug_json` for shorter-lived reads), each serialized by
        // `self.with(...)` against every other session access. This drives BOTH
        // long-lived worker callers concurrently many times over: if the
        // boundary were missing, their accesses on the backend's shared inner
        // and the shell would collide mid-call.
        //
        // The UI thread is deliberately NOT part of this concurrent pair any
        // more: since the page-signal callbacks moved onto the clone-handle
        // boundary (the backend's `Arc<Mutex>` inner cloned out of the session,
        // the debug-capture precedent — task
        // `mobile-page-signal-callbacks-off-session-lock`), the UI thread now
        // reads the chrome through the clone-handle boundary and is NOT
        // serialised by this test; the worker side still is. The UI side's
        // guarantee is pinned by the sibling ANR guard
        // (`the_page_signal_callbacks_never_wait_on_the_session_lock_so_a_spa_nav_cannot_anr`).
        //
        // It stays network-isolated: the `ipfs://` CID is malformed, so
        // `resolve_ipfs` fails closed in `Cid::try_from` BEFORE any fetch, and
        // the provider bridge's read-only stub answers `eth_chainId` keylessly.
        use std::thread;

        // Own the session in a `Box` on the main thread and cross ONLY the raw
        // `*mut SyncSession` to each worker — exactly as the JNI edge does (a
        // `jlong` handle, never an `Arc`): `SyncSession` is `!Send` (it guards a
        // `!Send` `CoreSession`), so the runtime `Mutex`, not the type system, is
        // what makes the two-thread access sound. The main thread joins both
        // threads before the box is dropped, so the pointer stays valid.
        let session: Box<SyncSession> = Box::default();
        let raw: *mut SyncSession = Box::into_raw(session);
        let iterations = 500;

        // WebView worker thread A: `shouldInterceptRequest` resolving `ipfs://`
        // through the shared core path, under the session lock.
        let resolver = {
            let ptr = SessionPtr(raw);
            thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                for _ in 0..iterations {
                    // Intercepted + routed to the core; a malformed CID fails
                    // closed (network-isolated) but STILL exercises the same
                    // locked session access that races the provider worker.
                    match s.resolve_ipfs("ipfs://not-a-valid-cid/index.html") {
                        Some(SchemeResolution::Err { .. }) => {}
                        other => panic!("expected a fail-closed resolution, got {other:?}"),
                    }
                }
            })
        };

        // WebView worker thread B: the provider JS-interface dispatching
        // EIP-1193 envelopes through the shared core path, under the SAME
        // session lock — the second long-lived locker, serialised against the
        // first exactly as both are serialised against the UI-thread drive.
        let provider = {
            let ptr = SessionPtr(raw);
            thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                for i in 0..iterations {
                    let pushed = s.handle_provider_message(
                        "werustProvider",
                        &format!(r#"{{"id":{i},"method":"eth_chainId","params":[]}}"#),
                    );
                    assert_eq!(
                        pushed.len(),
                        1,
                        "the provider round-trips under the contended lock"
                    );
                }
            })
        };

        // If the boundary were missing these joins would surface a panic from a
        // mid-call collision; with it, both worker sides complete serialized.
        resolver
            .join()
            .expect("the WebView-worker resolve must not panic under the lock");
        provider
            .join()
            .expect("the provider worker must not panic under the lock");

        // Reclaim ownership on the main thread now both workers have joined.
        let session: Box<SyncSession> = unsafe { Box::from_raw(raw) };

        // The session survived the concurrent worker drive and is still
        // coherent: a final navigate + settle through the same boundary (the
        // page signals recording OFF the lock, the reads folding them) works.
        session.navigate("https://after.example/");
        if let Some(pending) = session.take_pending_load() {
            session.on_page_committed(&pending);
            session.on_page_finished(&pending);
        }
        let json = session.chrome_json();
        assert!(
            json.contains("\"url\":\"https://after.example/\""),
            "the session is still coherent after the concurrent drive: {json}"
        );
    }

    #[test]
    fn the_page_signal_callbacks_never_wait_on_the_session_lock_so_a_spa_nav_cannot_anr() {
        // THE ANR REGRESSION GUARD (task
        // `mobile-page-signal-callbacks-off-session-lock`, the v0.2.7 field
        // finding): a `shouldInterceptRequest` mid-`resolve_ipfs` holds the
        // session lock for SECONDS on the WebView WORKER thread during a CAR
        // retrieval, and the UI thread's page-signal callbacks used to queue
        // behind it through `self.with(...)` — a same-document `onUrlChanged`
        // from a SvelteKit `pushState` then froze the UI thread long enough to
        // raise Android's "kill app / wait?" dialog.
        //
        // This test HOLDS the session lock (the worker's mid-retrieval stand-in)
        // and then drives the FOUR UI-thread page-signal callbacks from another
        // thread (crossing the raw pointer exactly as the JNI edge does): each
        // must return within 10ms. The receive timeout is the deadlock trip: if
        // a callback ever goes back through the session lock, it blocks behind
        // the held guard and this fails cleanly instead of hanging the gate.
        use std::sync::{mpsc, Arc, Barrier};
        use std::time::{Duration, Instant};

        let session: Box<SyncSession> = Box::default();
        let raw: *mut SyncSession = Box::into_raw(session);

        // Establish a current entry so the four signals have a load to report
        // against (the SPA nav rewrites it below).
        {
            let s: &SyncSession = unsafe { &*raw };
            s.navigate("https://example.com/");
            let _ = s.take_pending_load();
        }

        // The gate: the UI thread starts signalling only once this thread HOLDS
        // the session lock (so thread-spawn latency never pollutes the 10ms).
        let gate = Arc::new(Barrier::new(2));
        let (tx, rx) = mpsc::channel::<(&'static str, Duration)>();
        let ui = {
            let ptr = SessionPtr(raw);
            let gate = gate.clone();
            std::thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                gate.wait();
                let start = Instant::now();
                s.on_page_committed("https://example.com/");
                tx.send(("on_page_committed", start.elapsed())).unwrap();
                let start = Instant::now();
                s.on_page_finished("https://example.com/");
                tx.send(("on_page_finished", start.elapsed())).unwrap();
                let start = Instant::now();
                s.on_page_failed("https://example.com/dead", "boom");
                tx.send(("on_page_failed", start.elapsed())).unwrap();
                let start = Instant::now();
                s.on_url_changed("https://example.com/other");
                tx.send(("on_url_changed", start.elapsed())).unwrap();
            })
        };

        // The WebView WORKER thread: hold the session lock exactly as a
        // multi-second CAR retrieval inside `resolve_ipfs` does, then release
        // the UI thread onto its callbacks.
        let held = unsafe { &*raw }
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        gate.wait();

        for _ in 0..4 {
            let (call, elapsed) = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("a page-signal callback blocked behind the worker-held session lock");
            assert!(
                elapsed < Duration::from_millis(10),
                "{call} took {elapsed:?} while the worker held the session lock \
                 (the ANR shape: the UI thread must return in milliseconds)"
            );
        }
        ui.join().expect("the UI-thread signalling must not panic");
        drop(held);

        // The signals were RECORDED, not dropped: the next locked read folds
        // them into the chrome (the deferred pump), so the URL bar still
        // follows the SPA nav once it is read.
        let session: Box<SyncSession> = unsafe { Box::from_raw(raw) };
        let json = session.chrome_json();
        assert!(
            json.contains("\"url\":\"https://example.com/other\""),
            "the deferred pump folds the recorded signals into the chrome: {json}"
        );
    }

    #[test]
    fn the_sync_session_routes_ipfs_to_the_shared_core_fail_closed() {
        // The sync boundary must not change the resolve SEMANTICS: an `ipfs://`
        // URL is still intercepted and routed to the shared core path (fail-closed
        // on a malformed CID), and a non-`ipfs` URL is still not intercepted —
        // exactly as the bare `CoreSession`, now behind the lock.
        let s = SyncSession::new();
        match s.resolve_ipfs("ipfs://not-a-valid-cid/index.html") {
            Some(SchemeResolution::Err { reason }) => {
                assert!(
                    reason.contains("ipfs://"),
                    "legible fail-closed reason: {reason}"
                );
            }
            other => panic!("a malformed CID must fail closed, got {other:?}"),
        }
        assert!(
            s.resolve_ipfs("https://example.com/").is_none(),
            "a non-ipfs URL is not intercepted through the sync boundary"
        );
    }

    #[test]
    fn the_sync_session_is_safe_to_drive_from_a_background_thread() {
        // The ANR fix's load-bearing property (task
        // `android-anr-main-thread-diagnose-and-unblock`,
        // `docs/spikes/android-anr-main-thread-diagnose-and-unblock/DIAGNOSIS.md`).
        //
        // ROOT CAUSE: `BrowserShell::navigate` resolves an ENS/IPNS name inline
        // with BLOCKING network I/O (two sequential `eth_call`s, +IPNS record
        // fetch), so calling `navigate` on the Android UI thread blocks it for
        // seconds and trips the ANR watchdog REGULARLY. The fix moves the
        // session-DRIVING actions (`navigate`/`goBack`/`goForward`/`reload`) off
        // the UI thread on the Kotlin edge and posts the WebView/widget updates
        // (`takePendingLoad` + `chrome`) back to the UI thread.
        //
        // This guard pins the boundary that dispatch relies on: the long action
        // may run on a thread OTHER than the UI thread (here a dedicated
        // BACKGROUND thread, the Rust twin of the Kotlin executor) WHILE the UI
        // thread only reads the chrome / applies pending loads and the WebView
        // WORKER thread resolves `ipfs://` — and the session stays coherent and
        // never panics. If a later change reintroduced a UI-thread-only
        // assumption into the session, this reds the gate.
        //
        // Network-isolated: the background thread navigates a plain `https://`
        // URL (the in-core `navigate` for a non-`.eth`, explicit-scheme entry does
        // NO network — it just records history + queues the pending load; the real
        // load is the WebView's job), so no `.eth`/RPC round-trip is made; the
        // worker thread's `ipfs://` uses a malformed CID that fails closed BEFORE
        // any fetch. The point is the THREADING boundary, not a real load.
        use std::thread;

        // Own the session on the main thread and cross ONLY the raw pointer to
        // each worker, exactly as the JNI edge does (a `jlong` handle): the
        // runtime `Mutex` inside `SyncSession`, not the type system, is what makes
        // the multi-thread access sound. Both workers are joined before the box is
        // dropped, so the pointer stays valid.
        let session: Box<SyncSession> = Box::default();
        let raw: *mut SyncSession = Box::into_raw(session);
        let iterations = 500;

        // The BACKGROUND executor thread: the session-driving action the fix moves
        // off the UI thread (`navigate`, which is where the blocking ENS/IPNS
        // resolve lives on device). It drives the load to done via the WebView's
        // real signals, the same sequence the edge runs after the action returns.
        let executor = {
            let ptr = SessionPtr(raw);
            thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                for i in 0..iterations {
                    let url = format!("https://example.com/{i}");
                    s.navigate(&url);
                    if let Some(pending) = s.take_pending_load() {
                        s.on_page_committed(&pending);
                        s.on_page_finished(&pending);
                    }
                }
            })
        };

        // The UI thread: after posting the action to the executor, it only READS
        // the chrome to repaint (the cheap, non-blocking half that stays on the
        // UI thread in the fix). It must never see a mid-borrow session.
        let ui = {
            let ptr = SessionPtr(raw);
            thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                for _ in 0..iterations {
                    let _ = s.chrome_json();
                }
            })
        };

        // The WebView worker thread: `shouldInterceptRequest` resolving `ipfs://`
        // (fail-closed on a malformed CID) concurrently with both of the above.
        let worker = {
            let ptr = SessionPtr(raw);
            thread::spawn(move || {
                let ptr = ptr;
                let s: &SyncSession = unsafe { &*ptr.0 };
                for _ in 0..iterations {
                    match s.resolve_ipfs("ipfs://not-a-valid-cid/index.html") {
                        Some(SchemeResolution::Err { .. }) => {}
                        other => panic!("expected a fail-closed resolution, got {other:?}"),
                    }
                }
            })
        };

        executor
            .join()
            .expect("the background-executor drive must not panic under the lock");
        ui.join()
            .expect("the UI-thread chrome read must not panic under the lock");
        worker
            .join()
            .expect("the WebView-worker resolve must not panic under the lock");

        // Reclaim ownership on the main thread now every worker has joined; the
        // session is still coherent after the concurrent off-UI-thread drive.
        let session: Box<SyncSession> = unsafe { Box::from_raw(raw) };
        session.navigate("https://after.example/");
        if let Some(pending) = session.take_pending_load() {
            session.on_page_committed(&pending);
            session.on_page_finished(&pending);
        }
        assert!(
            session
                .chrome_json()
                .contains("\"url\":\"https://after.example/\""),
            "the session is still coherent after the off-UI-thread drive"
        );
    }

    // --- The debug capture store over the FFI (task ------------------------
    // `debug-capture-store-console-and-network-in-core`)

    #[test]
    fn debug_json_round_trips_console_and_network_entries_including_their_trust() {
        // The Kotlin debug view reads the capture store as ONE JSON document over
        // JNI, exactly as it reads the chrome. It must carry every field the view
        // paints, including the HONEST per-request trust posture, in the SAME
        // wire vocabulary the chrome's `trustPosture` uses (ADR-0006).
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
        // chrome JSON keeps its exact prior shape, so every existing Kotlin chrome
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

    // --- The general browser MENU over the FFI (task ----------------------
    // `general-browser-menu-with-version-and-debug-entry`)

    #[test]
    fn the_menu_version_is_the_one_shared_source_not_a_kotlin_hardcode() {
        // Acceptance: the version the Android menu shows comes from ONE place
        // (`werust_core::version`) over the FFI, so it can never drift from the
        // desktop popover or the iOS menu. The Kotlin edge reads THIS, never a
        // literal of its own.
        assert_eq!(super::version(), werust_core::version());
        assert!(!super::version().is_empty());
    }

    #[test]
    fn the_menu_document_carries_the_version_line_and_the_debug_entry_for_kotlin() {
        // The Kotlin edge builds its native `PopupMenu` from this ONE document,
        // so it must carry the version and every item with its stable id + kind:
        // a non-interactive `werust <version>` line and an activatable Debug entry
        // that opens the debug view. The byte-for-byte twin of the iOS core's menu
        // document, which is what makes the three menus agree.
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
    fn the_menu_accessors_need_no_session_so_the_menu_is_always_available() {
        // The menu is a USER-FACING, ALWAYS-AVAILABLE surface (never debug-build-
        // gated, never dependent on a live browsing session): both accessors are
        // session-free, so the Kotlin edge can show the menu whatever the core's
        // state is. Calling them with no `CoreSession` in existence is the test.
        assert!(super::menu_json().contains("\"items\""));
        assert!(!super::version().is_empty());
    }

    // --- The ANDROID CAPTURE POINTS (task ---------------------------------
    // `debug-console-network-capture-per-platform`)

    #[test]
    fn the_native_console_callback_maps_onto_a_core_console_entry() {
        // Acceptance: Android captures console via its REAL native callback
        // (`WebChromeClient.onConsoleMessage`), whose `MessageLevel` names
        // (`WARNING`, `TIP`, …) must land in werust's ONE console vocabulary — the
        // same vocabulary the desktop/iOS shim's `warn`/`info` map onto, so the
        // Console tab reads identically on all three.
        use werust_core::debug::ConsoleLevel;
        let s = SyncSession::new();
        s.capture_console("WARNING", "deprecated API", "https://x/app.js", 42);
        s.capture_console("TIP", "a hint", "", 0);

        let entries = s.debug_capture().console();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, ConsoleLevel::Warn);
        assert_eq!(entries[0].message, "deprecated API");
        assert_eq!(entries[0].source, "https://x/app.js");
        assert_eq!(entries[0].line, Some(42));
        assert_eq!(entries[1].level, ConsoleLevel::Info);
        assert_eq!(
            entries[1].line, None,
            "line 0 means unknown, never a fabricated line zero"
        );
        assert_eq!(entries[1].source, "");
    }

    #[test]
    fn should_intercept_request_records_both_the_intercepted_and_passed_through_requests() {
        // Acceptance: `shouldInterceptRequest` sees EVERY request, so Android
        // records the passed-through (`return null`) ones too — that is what makes
        // the Network tab the whole request stream and not just the
        // content-addressed slice.
        let s = SyncSession::new();
        // The intercepted, hash-verified `ipfs://` sub-resource.
        s.capture_network(
            "GET",
            "ipfs://bafy/pic.png",
            200,
            "image/png",
            99,
            true,
            false,
        );
        // The passed-through `https://` sub-resource: the response never crosses
        // werust, so status/mime/size are unknown here.
        s.capture_network("GET", "https://cdn.example/a.js", 0, "", 0, false, false);

        let entries = s.debug_capture().network();
        assert_eq!(entries.len(), 2, "both branches record");
        assert_eq!(entries[0].url, "ipfs://bafy/pic.png");
        assert_eq!(entries[0].status, Some(200));
        assert_eq!(entries[0].size, Some(99));
        assert_eq!(entries[0].trust, renderer::TrustPosture::ContentVerified);
        assert_eq!(entries[1].url, "https://cdn.example/a.js");
        assert_eq!(
            entries[1].status, None,
            "an unknown status stays unknown, never a fake 200"
        );
        assert_eq!(entries[1].size, None);
        assert_eq!(entries[1].trust, renderer::TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn android_capture_never_labels_a_request_verified_from_its_url_alone() {
        // ADR-0006: the posture tracks the ACTUAL load path. A `werust://settings`
        // page is served by the core but is NOT hash-verified content, and a
        // FAILED `ipfs://` resolution proved nothing.
        let s = SyncSession::new();
        s.capture_network(
            "GET",
            "werust://settings",
            200,
            "text/html",
            10,
            true,
            false,
        );
        s.capture_network(
            "GET",
            "ipfs://bafy/gone",
            502,
            "text/plain",
            0,
            false,
            false,
        );

        let entries = s.debug_capture().network();
        assert_eq!(
            entries[0].trust,
            renderer::TrustPosture::UnverifiedOrigin,
            "an internal werust:// page is not content-verified"
        );
        assert_eq!(
            entries[1].trust,
            renderer::TrustPosture::UnverifiedOrigin,
            "a failed ipfs:// request claims nothing"
        );
    }

    #[test]
    fn the_android_main_document_row_takes_the_loads_own_posture() {
        // The store's DECISIONS.md Decision 4: the main-document row must show the
        // SAME posture the chrome trust indicator shows, so the Network tab and the
        // indicator cannot disagree on the same screen.
        //
        // THE ORDERING TRAP this pins: the production order is navigate -> the
        // WebView asks for the document -> the shared resolve MARKS the backend
        // content-verified -> the capture runs, all BEFORE `onPageCommitted` /
        // `onPageFinished` pump the shell and `refresh_chrome` re-caches the
        // posture. Reading the CACHED `chrome().trust_posture` here would stamp
        // the stale pre-verify `unverified-origin`, so the row must read the
        // LIVE posture (the seam's `Renderer::trust_posture`), exactly as the
        // desktop capture reads its load lifecycle directly.
        let s = SyncSession::new();
        s.with(|c| {
            assert!(c.navigate("ipfs://bafy/index.html"));
            c.backend.mark_content_verified();
            // Deliberately NO on_page_committed / on_page_finished: the chrome
            // cache is still the stale pre-verify snapshot, which is the whole
            // point of this test.
            assert_eq!(
                c.chrome().trust_posture,
                renderer::TrustPosture::UnverifiedOrigin,
                "the cached chrome is still stale — the trap this test pins"
            );
        });
        s.capture_network(
            "GET",
            "ipfs://bafy/index.html",
            200,
            "text/html",
            12,
            true,
            true,
        );
        let entries = s.debug_capture().network();
        assert_eq!(
            entries[0].trust,
            renderer::TrustPosture::ContentVerified,
            "the main-document row carries the LIVE posture, not the stale cache"
        );
    }

    #[test]
    fn a_capture_push_never_waits_on_the_session_lock_so_the_ui_thread_cannot_anr() {
        // THE ANR GUARD (spec user story 4). `onConsoleMessage` runs on the UI
        // THREAD while `resolve_ipfs` can hold the session lock for SECONDS on a
        // worker thread during a CAR retrieval. If a capture push went through the
        // session boundary, the UI thread would block behind that retrieval —
        // exactly the ANR the off-main-thread work fixed.
        //
        // This test HOLDS the session lock and then captures from another thread
        // (crossing the raw pointer exactly as the JNI edge does): it can only
        // complete if the push does not need the session.
        let session: Box<SyncSession> = Box::default();
        let raw: *mut SyncSession = Box::into_raw(session);
        let held = unsafe { &*raw }
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner());

        let ptr = SessionPtr(raw);
        let captured = std::thread::spawn(move || {
            let ptr = ptr;
            let s: &SyncSession = unsafe { &*ptr.0 };
            // Both capture kinds, plus the debug view's read + clear, must be
            // reachable while the session is locked.
            s.capture_console("ERROR", "boom", "", 0);
            s.push_network_entry(werust_core::debug::NetworkEntry::new("GET", "https://x/y"));
            let json = s.debug_json();
            s.clear_debug_capture();
            json
        })
        .join()
        .expect("the capture thread must not block on the session lock");

        assert!(captured.contains("boom"), "{captured}");
        assert!(captured.contains("https://x/y"), "{captured}");
        drop(held);
        let session = unsafe { Box::from_raw(raw) };
        assert!(session.debug_json().contains("\"console\":[]"));
    }

    #[test]
    fn the_sync_session_exposes_the_debug_document_under_the_lock() {
        // Kotlin reads the debug JSON through the SAME `SyncSession` boundary
        // every other call goes through (the capture points run on the WebView
        // worker thread), so the accessor must exist there too.
        use werust_core::debug::NetworkEntry;
        let s = SyncSession::new();
        s.push_network_entry(NetworkEntry::new("GET", "ipfs://bafy/x"));
        assert!(
            s.debug_json().contains("ipfs://bafy/x"),
            "{}",
            s.debug_json()
        );
        s.clear_debug_capture();
        assert!(s.debug_json().contains("\"network\":[]"));
    }
}
