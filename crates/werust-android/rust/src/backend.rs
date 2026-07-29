//! [`AndroidBackend`]: the [`Renderer`] seam backend for the Android OS edge.
//!
//! On Android the forced OS edge is Kotlin plus the platform
//! `android.webkit.WebView` (Android's system webview) — there is no GTK. So the
//! browsing LOGIC (URL bar, session history, load lifecycle, chrome) stays in the
//! Rust core behind the seam, exactly as the desktop
//! [`BrowserShell`](werust_core::BrowserShell) sits over WebKitGTK, and this
//! backend is the seam implementation the core drives.
//!
//! Unlike the WebKitGTK backend, `AndroidBackend` does not OWN a native view: the
//! Kotlin `Activity` owns the platform `WebView`. So this backend is *edge-driven*
//! from both sides across the JNI boundary, and it shares its state behind an
//! [`Arc<Mutex>`](std::sync::Mutex) — the THREAD-SAFE analogue of the
//! `Rc<RefCell>` interior-mutability shape `webview-renderer` uses to share a
//! `LoadLifecycle` with the webview's signal closures — so the core owns a
//! `Box<dyn Renderer>` while the session keeps an [`AndroidHandle`] to the same
//! state for the platform-`WebView` protocol:
//!
//! Why not the desktop `Rc<RefCell>` shape: Android is the one edge where the
//! shared state is touched from TWO threads at once (the WebView WORKER thread's
//! `shouldInterceptRequest` and the UI thread's page-signal callbacks), and the
//! UI-thread callbacks deliberately go through a CLONED handle OFF the session
//! lock (task `mobile-page-signal-callbacks-off-session-lock` — a worker-held
//! session lock during a multi-second CAR retrieval must never delay them, the
//! ANR shape). Only a thread-safe shared cell makes that clone-boundary sound;
//! every lock hold here is a microsecond field access (no call holds the lock
//! across I/O — see [`AndroidHandle::resolve_scheme`]), so the two paths contend
//! for nanoseconds, never for the length of a retrieval.
//!
//! * The core drives navigation INTO the backend
//!   ([`navigate`](Renderer::navigate)/[`go_back`](Renderer::go_back)/…); the
//!   backend records the intent, updates its session history + load lifecycle, and
//!   surfaces the URL Kotlin must load onto the platform `WebView`
//!   ([`take_pending_load`](AndroidHandle::take_pending_load)).
//! * Kotlin reports the platform `WebView`'s REAL load-lifecycle signals back
//!   through the handle ([`on_page_committed`](AndroidHandle::on_page_committed) /
//!   [`on_page_finished`](AndroidHandle::on_page_finished) /
//!   [`on_page_failed`](AndroidHandle::on_page_failed)), which advance the
//!   lifecycle and emit the matching [`LoadEvent`]s the core's chrome reflects.
//!
//! The session history (back/forward availability, the effective URL after a
//! history move) is the BACKEND's truth — Kotlin never keeps a URL stack of its
//! own, so "Kotlin confined to the OS edge" holds: history logic lives here, in
//! Rust, behind the seam.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use renderer::{
    KeyEvent, LoadEvent, LoadState, PointerEvent, Renderer, RendererError, SchemeHandler,
    SchemeRequest, SchemeResponse, ScriptMessage, ScriptMessageHandler, ScrollDelta, TrustPosture,
    ViewHandle,
};

use crate::origin_map::{from_webview_url, to_webview_url};

/// Lock the shared inner, recovering a poisoned lock into the guard rather than
/// propagating: the edge must stay responsive, and the inner is plain data whose
/// mutations are individually consistent (a panic mid-call is a bug we would
/// rather surface as a degraded-but-live session than a crash on every later
/// call). The same posture `SyncSession::with` takes for the session lock.
///
/// Every hold of this lock is a microsecond field access or map/queue operation
/// — NEVER held across a scheme/script handler call or any I/O (see
/// [`AndroidHandle::resolve_scheme`]) — so the UI thread's lock-free page-signal
/// path can never queue behind a worker thread's retrieval.
fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|p| p.into_inner())
}

/// Validate a URL for [`Renderer::navigate`], rejecting unusable ones.
///
/// The same rule the WebKitGTK backend uses: an absolute URL with a non-empty
/// scheme and target is handed to the platform `WebView`; anything without a
/// scheme is rejected with [`RendererError::InvalidUrl`] and never starts a load
/// (the bad text stays in the URL bar for the user to fix).
fn validate_url(url: &str) -> Result<(), RendererError> {
    match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => Ok(()),
        _ => Err(RendererError::InvalidUrl(url.to_string())),
    }
}

/// The mutable innards shared between the [`AndroidBackend`] (owned by the core's
/// shell as a `dyn Renderer`) and the [`AndroidHandle`] (kept by the session for
/// the platform-`WebView` protocol the cross-backend seam does not carry).
#[derive(Default)]
struct Inner {
    /// The back/forward list; `cursor` indexes the current entry.
    history: Vec<String>,
    cursor: Option<usize>,
    state: LoadState,
    events: VecDeque<LoadEvent>,
    /// The URL the core has committed to but Kotlin has not yet loaded onto the
    /// platform `WebView`. Drained by [`AndroidHandle::take_pending_load`].
    pending_load: Option<String>,
    /// The registered custom-scheme handlers, keyed by scheme (e.g. `ipfs`).
    ///
    /// This is what makes [`register_scheme_handler`](Renderer::register_scheme_handler)
    /// REAL on the Android edge (it was an empty no-op before): the platform
    /// `WebView` cannot itself load an `ipfs://` URL (`ERR_UNKNOWN_URL_SCHEME`),
    /// so Kotlin's `shouldInterceptRequest` intercepts the request and drives it
    /// through [`AndroidHandle::resolve_scheme`], which dispatches to the handler
    /// stored here. The `ipfs` handler is wired by the session's `install_ipfs`
    /// (the twin of the desktop backend's `install_ipfs`), routing each request
    /// through the SAME `werust_core::ipfs::resolve_ipfs_request` path desktop
    /// uses, so the same content resolution + fail-closed trust posture apply.
    scheme_handlers: HashMap<String, SchemeHandler>,
    /// The registered script-message-bridge handlers, keyed by channel name (e.g.
    /// `werustProvider`).
    ///
    /// This is what makes [`register_script_message_handler`](Renderer::register_script_message_handler)
    /// REAL on the Android edge (it was an empty no-op before): the platform
    /// `WebView` cannot post to `window.webkit.messageHandlers.<name>`, so Kotlin
    /// bridges the channel with `addJavascriptInterface` and drives each posted
    /// envelope through [`AndroidHandle::handle_script_message`], which dispatches
    /// to the handler stored here. The `werustProvider` handler is wired by the
    /// session's `install_provider` (the twin of the desktop backend's
    /// `install_provider`), routing each envelope through the SAME
    /// `werust_core::provider` EIP-1193 path desktop uses.
    script_handlers: HashMap<String, ScriptMessageHandler>,
    /// The scripts injected at document start (e.g. the EIP-1193 provider shim),
    /// in injection order. Read by [`AndroidHandle::document_start_scripts`] so
    /// Kotlin can install them onto the platform `WebView` as page-start user
    /// scripts. This is the Android stand-in for WebKitGTK's
    /// `UserContentManager::add_script` (`inject_script` was an empty no-op
    /// before).
    injected_scripts: Vec<String>,
    /// Response JS the browser must evaluate back in the live page (browser ->
    /// page), queued by a script-message handler (the EIP-1193 provider's response
    /// push that settles a page's pending Promise). This is the Android stand-in
    /// for the desktop backend's `evaluate_javascript`: the mobile backend owns no
    /// live view, so the response JS is queued here and drained by
    /// [`AndroidHandle::take_pending_eval`] for Kotlin to run via
    /// `WebView.evaluateJavascript`.
    ///
    /// Held behind its OWN [`Arc<Mutex<_>>`](std::sync::Arc) (not just the
    /// surrounding shared cell) so the provider bridge handler can own a clone
    /// of JUST this queue and push without locking the whole inner: the seam's
    /// [`ScriptMessageHandler`] runs with the inner lock OUT (see
    /// [`AndroidHandle::handle_script_message`]), so the provider closure
    /// captures this eval sink alone rather than the whole handle — the mobile
    /// twin of how the desktop `install_provider` closure captures a cloneable
    /// view handle for its response push.
    pending_eval: Arc<Mutex<Vec<String>>>,
    /// The [`TrustPosture`] of the CURRENT load: the same shared-`LoadLifecycle`
    /// posture the desktop backend surfaces, made real on the Android edge (the
    /// seam default `UnverifiedOrigin` was inherited before). Reset to
    /// `UnverifiedOrigin` on every fresh [`begin`](Inner::begin) and upgraded ONLY
    /// when the `ipfs` scheme handler verifies this load's bytes
    /// ([`mark_content_verified`](Inner::mark_content_verified)) — the same
    /// hash-verified load path the trust indicator must track, never the URL.
    posture: TrustPosture,
    /// Whether the CURRENT load originated from an ENS name resolved over a
    /// trusted RPC (the bare-`.eth` front door), mirroring the desktop
    /// `LoadLifecycle::ens_origin`. Set by the shell via
    /// [`mark_ens_origin`](Renderer::mark_ens_origin); reset on `begin`.
    ens_origin: bool,
    /// Whether the CURRENT load points at a MUTABLE name (an IPNS name, or a later
    /// ENS name), mirroring the desktop `LoadLifecycle::mutable_name`. Set by the
    /// shell via [`mark_mutable_name`](Renderer::mark_mutable_name); reset on
    /// `begin`.
    mutable_name: bool,
}

// A `SchemeHandler` is a boxed `FnMut` and cannot derive `Debug`; hand-write it
// so the surrounding session types can still be `Debug` (the handler map is
// summarised by its registered scheme names).
impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("history", &self.history)
            .field("cursor", &self.cursor)
            .field("state", &self.state)
            .field("events", &self.events)
            .field("pending_load", &self.pending_load)
            .field(
                "scheme_handlers",
                &self.scheme_handlers.keys().collect::<Vec<_>>(),
            )
            .field(
                "script_handlers",
                &self.script_handlers.keys().collect::<Vec<_>>(),
            )
            .field("injected_scripts", &self.injected_scripts.len())
            .field(
                "pending_eval",
                &self.pending_eval.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field("posture", &self.posture)
            .field("ens_origin", &self.ens_origin)
            .field("mutable_name", &self.mutable_name)
            .finish()
    }
}

impl Inner {
    fn current(&self) -> Option<&String> {
        self.cursor.and_then(|c| self.history.get(c))
    }

    /// Begin a load of `url`: record it as the pending load for Kotlin to apply to
    /// the platform `WebView`, move to [`LoadState::Started`], and emit
    /// [`LoadEvent::Started`].
    ///
    /// A fresh load starts UNVERIFIED and un-flagged, exactly like the desktop
    /// `LoadLifecycle::begin`: the trust posture resets to
    /// [`TrustPosture::UnverifiedOrigin`] and both trust-axis flags clear, so a
    /// stale verified/ENS/mutable posture never leaks from a prior load onto this
    /// one. The posture is only upgraded again if THIS load's bytes verify
    /// ([`mark_content_verified`](Inner::mark_content_verified)).
    fn begin(&mut self, url: &str) {
        self.pending_load = Some(url.to_string());
        self.state = LoadState::Started;
        self.posture = TrustPosture::UnverifiedOrigin;
        self.ens_origin = false;
        self.mutable_name = false;
        self.events.push_back(LoadEvent::Started {
            url: url.to_string(),
        });
    }

    /// Mark the CURRENT load content-verified: its bytes came back through the
    /// hash-verified content-addressed path (the `ipfs` scheme handler resolved
    /// them successfully). Surfaces the honest two-axis posture via the ONE shared
    /// rule [`TrustPosture::after_verify`], exactly as the desktop
    /// `LoadLifecycle::mark_content_verified` does: `NameViaTrustedRpc` if
    /// ENS-originated (loudest), else `MutableName` if mutable, else plain
    /// `ContentVerified`.
    fn mark_content_verified(&mut self) {
        self.posture = TrustPosture::after_verify(self.ens_origin, self.mutable_name);
    }
}

/// The [`Renderer`] backend for the Android edge: a session history + load
/// lifecycle over the platform `WebView`, driven from Kotlin across JNI.
///
/// It renders nothing itself (the platform `WebView` does); it owns the browsing
/// LOGIC the core drives through the seam. The core holds it as `Box<dyn
/// Renderer>`; the session keeps an [`AndroidHandle`] (from
/// [`handle`](AndroidBackend::handle)) to the same shared state to run the
/// platform-`WebView` protocol (pending-load + signals).
#[derive(Debug, Default, Clone)]
pub struct AndroidBackend {
    inner: Arc<Mutex<Inner>>,
}

impl AndroidBackend {
    /// A fresh backend with no history, ready for the core to drive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A handle to the same shared state, for the session's platform-`WebView`
    /// protocol (pending-load + WebView load signals).
    #[must_use]
    pub fn handle(&self) -> AndroidHandle {
        AndroidHandle {
            inner: self.inner.clone(),
        }
    }
}

/// A handle to an [`AndroidBackend`]'s shared state, held by the session for the
/// platform-`WebView` protocol: the URL to load onto the `WebView`, and the
/// `WebView`'s real load signals reported back into the core.
///
/// The shared state is an `Arc<Mutex<_>>`, so a CLONE of this handle is the
/// lock-free boundary the `SyncSession` serves the UI thread's page-signal
/// callbacks through (task `mobile-page-signal-callbacks-off-session-lock`):
/// the clone contends only on the inner's own microsecond holds, never on the
/// session lock a worker thread can hold for the length of a CAR retrieval.
#[derive(Debug, Clone)]
pub struct AndroidHandle {
    inner: Arc<Mutex<Inner>>,
}

impl AndroidHandle {
    /// Take the URL the core has committed to but Kotlin has not yet loaded onto
    /// the platform `WebView`, if any. Kotlin calls this after driving the core
    /// (navigate/back/forward/reload) and calls `WebView.loadUrl` with the result.
    ///
    /// An `ipfs://<cid>` pending load is surfaced ON THE INTERNAL `https://`
    /// origin ([`to_webview_url`]): a `shouldInterceptRequest`-served `ipfs://`
    /// document gets an OPAQUE origin in the System WebView (Blink refuses
    /// `fetch(ipfs://…)` before the network stack and throws `SecurityError` on
    /// `pushState`), which is what killed SvelteKit client-side navigation on
    /// Android (task `mobile-ronan-eth-buttons-no-navigation`). The WebView
    /// loads the internal origin; every URL it reports back is mapped to the
    /// real `ipfs://` form by [`from_webview_url`], so the core's history and
    /// the URL bar never see the internal origin.
    pub fn take_pending_load(&self) -> Option<String> {
        lock(&self.inner)
            .pending_load
            .take()
            .map(|url| to_webview_url(&url))
    }

    /// Resolve an intercepted `<scheme>://…` request through the handler the
    /// session registered for that scheme, or `None` if no handler is registered.
    ///
    /// This is the Android edge's stand-in for WebKitGTK's `register_uri_scheme`
    /// callback: the platform `WebView` cannot load an `ipfs://` URL itself, so
    /// Kotlin's `WebViewClient.shouldInterceptRequest` calls this with the
    /// intercepted URI, gets back the verified bytes + MIME type (or a
    /// fail-closed error), and answers the `WebView` with a `WebResourceResponse`.
    /// The handler routes through the SAME core resolve path desktop uses, so the
    /// content resolution + trust posture + fail-closed reasons match desktop.
    ///
    /// `None` means the scheme was never registered (Kotlin then lets the
    /// `WebView` handle the URL normally); `Some(Err(..))` is a real, honest
    /// resolution failure that must FAIL the load, never render unverified bytes.
    pub fn resolve_scheme(&self, uri: &str) -> Option<Result<SchemeResponse, RendererError>> {
        // Map an internal-`https`-origin URL back to its real `ipfs://` form
        // FIRST ([`from_webview_url`]; identity for anything else), so a page
        // living on the internal origin has its subresource + client-router
        // data fetches dispatched to the `ipfs` handler as real `ipfs://`
        // requests — and the `_redirects` 3xx main-frame inference (which
        // compares the intercepted URI against the shell's top-level URL, both
        // `ipfs://`) stays consistent.
        let uri = from_webview_url(uri);
        let scheme = uri.split_once("://").map(|(s, _)| s.to_string())?;
        // Take the handler OUT of the map for the duration of the call so the
        // inner lock is NOT held across it (the same shape
        // [`handle_script_message`](AndroidHandle::handle_script_message) uses):
        // a handler can run a multi-second CAR retrieval on the WebView WORKER
        // thread, and the UI thread's page-signal callbacks borrow this SAME
        // inner OFF the session lock — holding the lock across the call would
        // block the UI thread behind the retrieval, exactly the ANR shape the
        // clone-handle boundary (task
        // `mobile-page-signal-callbacks-off-session-lock`) exists to remove.
        // Concurrent `resolve_scheme` calls for the same scheme are serialised
        // by the session lock above this (`SyncSession::resolve_ipfs` stays on
        // `self.with`), so the remove/call/reinsert cannot observe a missing
        // handler in production.
        let taken = lock(&self.inner).scheme_handlers.remove(&scheme);
        let mut handler = taken?;
        let result = handler(SchemeRequest { uri });
        lock(&self.inner).scheme_handlers.insert(scheme, handler);
        Some(result)
    }

    /// Mark the CURRENT load content-verified from the OS edge: its bytes came
    /// back through the hash-verified `ipfs` resolve path. This is the Android
    /// stand-in for the desktop `install_ipfs` scheme handler calling
    /// `life.borrow_mut().mark_content_verified()` on a verified resolution: the
    /// mobile backend owns no live `LoadLifecycle`, so the session's `resolve_ipfs`
    /// calls this the moment a resolution succeeds, and the trust indicator then
    /// surfaces the honest two-axis posture (`NameViaTrustedRpc` / `MutableName` /
    /// `ContentVerified`) for THIS load instead of the served default.
    pub fn mark_content_verified(&self) {
        lock(&self.inner).mark_content_verified();
    }

    /// The scripts to inject at document start (the EIP-1193 provider shim), in
    /// injection order, so Kotlin can install them onto the platform `WebView` as
    /// page-start user scripts (`addDocumentStartJavaScript` / a page-start
    /// `evaluateJavascript`). This is the read half of the Android `inject_script`
    /// bridge, which used to be an empty no-op.
    #[must_use]
    pub fn document_start_scripts(&self) -> Vec<String> {
        lock(&self.inner).injected_scripts.clone()
    }

    /// Dispatch a page-posted script-message envelope on channel `name` to the
    /// registered handler (the EIP-1193 provider bridge), then drain and return
    /// the response JS (if any) the browser must evaluate back in the page to
    /// settle the page's pending Promise.
    ///
    /// This is the Android edge's stand-in for WebKitGTK's
    /// `connect_script_message_received` + `evaluate_javascript` round-trip: the
    /// platform `WebView` cannot post to `window.webkit.messageHandlers.<name>`,
    /// so Kotlin bridges the channel with `addJavascriptInterface` and calls this
    /// with each posted body; the handler answers it (queuing the response JS via
    /// `evaluate_javascript`) and this returns that JS for Kotlin to run with
    /// `WebView.evaluateJavascript`. `None` (empty vec) means the channel is
    /// unregistered or the message needed no response.
    #[must_use]
    pub fn handle_script_message(&self, name: &str, body: &str) -> Vec<String> {
        // Take the handler OUT of the map for the duration of the call so the
        // inner lock is not held across the handler body: the handler is a
        // `FnMut` capturing its own response sink (`evaluate_javascript`, which
        // locks the same inner to queue into `pending_eval`), so holding the
        // lock here would be a re-entrant deadlock. Re-insert it after.
        let taken = lock(&self.inner).script_handlers.remove(name);
        if let Some(mut handler) = taken {
            handler(ScriptMessage {
                handler: name.to_string(),
                body: body.to_string(),
            });
            lock(&self.inner)
                .script_handlers
                .insert(name.to_string(), handler);
        }
        self.take_pending_eval()
    }

    /// Drain the response JS the browser must evaluate back in the live page
    /// (browser -> page), queued by a script-message handler's response push. The
    /// Android stand-in for the desktop `evaluate_javascript` immediate eval: the
    /// backend owns no live view, so Kotlin runs these with
    /// `WebView.evaluateJavascript`.
    #[must_use]
    pub fn take_pending_eval(&self) -> Vec<String> {
        let queue = lock(&self.inner).pending_eval.clone();
        let mut q = queue.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::take(&mut *q)
    }

    /// A `Send` clone of JUST the response-JS eval queue (browser -> page), for the
    /// session's `install_provider` to hand
    /// [`route_provider_message`](werust_core::provider::route_provider_message)
    /// as its `respond` sink: the provider handler pushes each response-delivery
    /// call here, and [`take_pending_eval`](AndroidHandle::take_pending_eval)
    /// drains it for `WebView.evaluateJavascript`. Cloning JUST this `Arc` (not the
    /// `!Send` backend handle) is what lets the seam's `Send`
    /// [`ScriptMessageHandler`] capture the sink — the mobile twin of the desktop
    /// `install_provider` closure capturing a cloneable view handle.
    #[must_use]
    pub fn eval_sink(&self) -> Arc<Mutex<Vec<String>>> {
        lock(&self.inner).pending_eval.clone()
    }

    /// Report a SAME-DOCUMENT URL change: an SPA `pushState`/`replaceState`
    /// client-side navigation rewrote the address WITHOUT a fresh page load, so no
    /// `onPageStarted`/`onPageFinished` fires. Called from Kotlin's
    /// `WebViewClient.doUpdateVisitedHistory` (which DOES fire on same-document
    /// history changes).
    ///
    /// It emits ONLY a [`LoadEvent::UrlChanged`] and updates the session-history
    /// entry, but leaves the load state, trust posture, and per-load flags
    /// UNTOUCHED — the document (and its already-established verified/ENS posture)
    /// is unchanged; the SPA only rewrote the history URL. This is the mobile twin
    /// of the desktop `LoadLifecycle::url_changed` (WebKitGTK `notify::uri`). A
    /// NO-OP when `url` already matches the current entry, so a
    /// `doUpdateVisitedHistory` that merely echoes the current load's URL (a real
    /// load, not an SPA nav) emits nothing.
    pub fn on_url_changed(&self, url: &str) {
        // The WebView reports the URL IT is on (the internal `https://` origin
        // for a content-addressed page); map it back so the core's history and
        // the URL bar track the real `ipfs://` location.
        let url = from_webview_url(url);
        let mut b = lock(&self.inner);
        if b.current().map(String::as_str) == Some(url.as_str()) {
            return;
        }
        // A same-document history push adds a forward entry from mid-history,
        // dropping any forward entries — just like a navigation, but with NO load
        // lifecycle reset (state/posture/flags keep the current document's values).
        let next = b.cursor.map_or(0, |c| c + 1);
        b.history.truncate(next);
        b.history.push(url.clone());
        b.cursor = Some(b.history.len() - 1);
        b.events.push_back(LoadEvent::UrlChanged { url });
    }

    /// Report that the platform `WebView` committed the load on `url` (the
    /// effective URL after any redirects): advance to [`LoadState::Committed`] and
    /// emit [`LoadEvent::Committed`]. Called from Kotlin's `onPageCommitVisible`.
    pub fn on_page_committed(&self, url: &str) {
        let url = from_webview_url(url);
        let mut b = lock(&self.inner);
        b.state = LoadState::Committed;
        b.events.push_back(LoadEvent::Committed { url });
    }

    /// Report that the platform `WebView` finished loading `url`: advance to
    /// [`LoadState::Finished`] and emit [`LoadEvent::Finished`]. Called from
    /// Kotlin's `onPageFinished`.
    pub fn on_page_finished(&self, url: &str) {
        let url = from_webview_url(url);
        let mut b = lock(&self.inner);
        b.state = LoadState::Finished;
        b.events.push_back(LoadEvent::Finished { url });
    }

    /// Report that the platform `WebView` failed to load `url`: advance to
    /// [`LoadState::Failed`] and emit [`LoadEvent::Failed`]. Called from Kotlin's
    /// `onReceivedError`.
    pub fn on_page_failed(&self, url: &str, reason: &str) {
        let url = from_webview_url(url);
        let mut b = lock(&self.inner);
        b.state = LoadState::Failed;
        b.events.push_back(LoadEvent::Failed {
            url,
            reason: reason.to_string(),
        });
    }
}

impl Renderer for AndroidBackend {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        validate_url(url)?;
        let mut b = lock(&self.inner);
        // A fresh navigation from mid-history drops the forward entries.
        let next = b.cursor.map_or(0, |c| c + 1);
        b.history.truncate(next);
        b.history.push(url.to_string());
        b.cursor = Some(b.history.len() - 1);
        b.begin(url);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        let mut b = lock(&self.inner);
        let url = b
            .current()
            .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?
            .clone();
        b.begin(&url);
        Ok(())
    }

    fn stop(&mut self) {
        let mut b = lock(&self.inner);
        if b.state.is_loading() {
            b.state = LoadState::Idle;
        }
    }

    fn go_back(&mut self) {
        let mut b = lock(&self.inner);
        if let Some(c) = b.cursor {
            if c > 0 {
                b.cursor = Some(c - 1);
                let url = b.history[c - 1].clone();
                b.begin(&url);
            }
        }
    }

    fn go_forward(&mut self) {
        let mut b = lock(&self.inner);
        if let Some(c) = b.cursor {
            if c + 1 < b.history.len() {
                b.cursor = Some(c + 1);
                let url = b.history[c + 1].clone();
                b.begin(&url);
            }
        }
    }

    fn can_go_back(&self) -> bool {
        matches!(lock(&self.inner).cursor, Some(c) if c > 0)
    }

    fn can_go_forward(&self) -> bool {
        let b = lock(&self.inner);
        matches!(b.cursor, Some(c) if c + 1 < b.history.len())
    }

    fn load_state(&self) -> LoadState {
        lock(&self.inner).state
    }

    fn current_url(&self) -> Option<String> {
        lock(&self.inner).current().cloned()
    }

    fn poll_event(&mut self) -> Option<LoadEvent> {
        lock(&self.inner).events.pop_front()
    }

    fn view_handle(&self) -> ViewHandle {
        // The Android edge owns the platform WebView; the core never embeds a view
        // handle here (unlike the GTK edge). The seam still requires the method.
        ViewHandle(std::ptr::null_mut())
    }

    fn send_pointer(&mut self, _event: PointerEvent) {}
    fn send_key(&mut self, _event: KeyEvent) {}
    fn send_scroll(&mut self, _delta: ScrollDelta) {}
    fn set_focus(&mut self, _focused: bool) {}

    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
        // Store the handler so the Android edge can dispatch page-posted envelopes
        // to it from `addJavascriptInterface` via
        // [`AndroidHandle::handle_script_message`]. This is the seam method that
        // used to be a silent no-op — the exact gap the platform-capability parity
        // guard exists to forbid; it is now real. It is the channel the EIP-1193
        // provider is injected over (`install_provider`).
        lock(&self.inner)
            .script_handlers
            .insert(name.to_string(), handler);
    }

    fn inject_script(&mut self, script: &str) {
        // Record the document-start script (the EIP-1193 provider shim) so the
        // Android edge can install it onto the platform `WebView` as a page-start
        // user script via [`AndroidHandle::document_start_scripts`]. The seam
        // method that used to be a silent no-op is now real.
        lock(&self.inner).injected_scripts.push(script.to_string());
    }

    fn evaluate_javascript(&self, script: &str) {
        // Queue the response JS (browser -> page) for the Android edge to run in
        // the live page via `WebView.evaluateJavascript`. The backend owns no live
        // view, so unlike the desktop backend (which evaluates immediately on the
        // GTK loop) the JS is queued and drained by
        // [`AndroidHandle::take_pending_eval`]. This is the RESPONSE half of the
        // provider round-trip that settles a page's pending Promise.
        if let Ok(mut queue) = lock(&self.inner).pending_eval.lock() {
            queue.push(script.to_string());
        }
    }

    fn trust_hooks(&self) -> renderer::TrustHooks {
        // OPT IN to BOTH trust hooks, exactly as the desktop `WebViewRenderer`
        // does: this backend genuinely wires EIP-1193 provider injection
        // (`register_script_message_handler` + `inject_script` + the
        // `evaluate_javascript` response push, driven by the OS edge) AND `ipfs://`
        // custom-scheme resolution (`register_scheme_handler` -> the hash-verified
        // core path). The seam default is FAIL-CLOSED (`TrustHooks::none()`), so a
        // backend must EXPLICITLY declare the hooks it satisfies to qualify; the
        // mobile no-ops these methods USED to be would have disqualified it.
        renderer::TrustHooks::all()
    }

    fn trust_posture(&self) -> TrustPosture {
        // The current load's posture, the SAME source the desktop chrome reads:
        // `ContentVerified` (or the louder ENS/mutable variant) iff this load's
        // bytes came back through the hash-verified `ipfs` path (marked by
        // `mark_content_verified`), else the served-origin default. This is what
        // makes the mobile trust indicator reflect the real load posture rather
        // than the inherited seam default.
        lock(&self.inner).posture
    }

    fn mark_ens_origin(&mut self) {
        // Flag the current load ENS-originated (the front door resolved the name
        // over the trusted RPC), so when the `ipfs` handler later verifies the
        // bytes the posture surfaces `NameViaTrustedRpc`. A fresh `begin` clears
        // the flag. The twin of the desktop backend's `mark_ens_origin`.
        lock(&self.inner).ens_origin = true;
    }

    fn mark_mutable_name(&mut self) {
        // Flag the current load's name MUTABLE (an IPNS resolution), so a verified
        // load surfaces at most `MutableName` (or the louder `NameViaTrustedRpc` if
        // also ENS-originated), never immutable `ContentVerified`. A fresh `begin`
        // clears the flag. The twin of the desktop backend's `mark_mutable_name`.
        lock(&self.inner).mutable_name = true;
    }

    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
        // Store the handler so the Android edge can dispatch to it from
        // `shouldInterceptRequest` via [`AndroidHandle::resolve_scheme`]. This is
        // the seam method that used to be a silent no-op — the exact gap the
        // platform-capability parity guard exists to forbid; it is now real.
        lock(&self.inner)
            .scheme_handlers
            .insert(scheme.to_string(), handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the in-flight load to done the way Kotlin would from the platform
    /// `WebView`'s `onPageCommitVisible` + `onPageFinished` signals.
    fn settle(backend: &mut AndroidBackend, handle: &AndroidHandle) {
        let url = handle
            .take_pending_load()
            .expect("a pending load to apply to the WebView");
        handle.on_page_committed(&url);
        handle.on_page_finished(&url);
        // Drain the events the core would drain via the seam.
        while backend.poll_event().is_some() {}
    }

    #[test]
    fn navigate_surfaces_a_pending_load_and_drives_the_lifecycle() {
        // The core navigates INTO the backend; the handle surfaces the URL Kotlin
        // must load onto the platform WebView and the lifecycle starts.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        assert_eq!(b.load_state(), LoadState::Idle);

        b.navigate("https://example.com/").expect("valid https url");
        assert_eq!(b.load_state(), LoadState::Started);
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::Started {
                url: "https://example.com/".into()
            })
        );
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(h.take_pending_load(), None, "drained once");

        h.on_page_committed("https://example.com/");
        h.on_page_finished("https://example.com/");
        assert_eq!(b.load_state(), LoadState::Finished);
        assert_eq!(b.current_url().as_deref(), Some("https://example.com/"));
    }

    #[test]
    fn navigate_rejects_an_unusable_url_without_starting_a_load() {
        let mut b = AndroidBackend::new();
        let h = b.handle();
        let err = b.navigate("not-a-url").expect_err("unusable url rejected");
        assert_eq!(err, RendererError::InvalidUrl("not-a-url".into()));
        assert_eq!(b.load_state(), LoadState::Idle);
        assert_eq!(h.take_pending_load(), None);
        assert_eq!(b.current_url(), None);
    }

    #[test]
    fn back_and_forward_are_the_backends_session_history() {
        // History availability + the effective URL after a move are the backend's
        // truth (Kotlin keeps no URL stack of its own).
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate("https://a.example/").unwrap();
        settle(&mut b, &h);
        assert!(!b.can_go_back(), "one entry: nowhere back");

        b.navigate("https://b.example/").unwrap();
        settle(&mut b, &h);
        assert!(b.can_go_back());
        assert!(!b.can_go_forward());

        b.go_back();
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some("https://a.example/"),
            "back surfaces the prior URL for the WebView"
        );
        h.on_page_committed("https://a.example/");
        h.on_page_finished("https://a.example/");
        while b.poll_event().is_some() {}
        assert_eq!(b.current_url().as_deref(), Some("https://a.example/"));
        assert!(!b.can_go_back());
        assert!(b.can_go_forward());

        b.go_forward();
        settle(&mut b, &h);
        assert_eq!(b.current_url().as_deref(), Some("https://b.example/"));
    }

    #[test]
    fn a_fresh_navigation_from_mid_history_drops_the_forward_entries() {
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate("https://a.example/").unwrap();
        settle(&mut b, &h);
        b.navigate("https://b.example/").unwrap();
        settle(&mut b, &h);
        b.go_back();
        settle(&mut b, &h);
        assert!(b.can_go_forward());

        b.navigate("https://c.example/").unwrap();
        settle(&mut b, &h);
        assert!(
            !b.can_go_forward(),
            "a new navigation dropped the forward entry"
        );
        assert_eq!(b.current_url().as_deref(), Some("https://c.example/"));
    }

    #[test]
    fn reload_re_surfaces_the_current_url_and_stop_settles() {
        let mut b = AndroidBackend::new();
        let h = b.handle();
        assert!(b.reload().is_err(), "nothing to reload yet");

        b.navigate("https://example.com/").unwrap();
        settle(&mut b, &h);
        b.reload().expect("reload the settled page");
        assert!(b.load_state().is_loading());
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some("https://example.com/")
        );

        b.stop();
        assert_eq!(b.load_state(), LoadState::Idle);
    }

    #[test]
    fn a_registered_scheme_handler_is_reachable_from_the_edge() {
        // The seam method that used to be a silent no-op is now real: a handler
        // registered for `ipfs` is stored and dispatched when the Android edge
        // (`shouldInterceptRequest`) resolves an intercepted `ipfs://` request.
        let mut b = AndroidBackend::new();
        let h = b.handle();

        // A canned handler standing in for the real `install_ipfs` resolver, so
        // this stays network-isolated: it echoes the intercepted URI back as the
        // body and pins a MIME type, exactly as a verified resolution would.
        b.register_scheme_handler(
            "ipfs",
            Box::new(|request: SchemeRequest| {
                Ok(SchemeResponse::ok("text/html", request.uri.into_bytes()))
            }),
        );

        let resolved = h
            .resolve_scheme("ipfs://bafycid/index.html")
            .expect("the ipfs scheme is registered, so it routes to the handler")
            .expect("the canned handler resolves successfully");
        assert_eq!(resolved.mime_type, "text/html");
        assert_eq!(resolved.body, b"ipfs://bafycid/index.html");
    }

    #[test]
    fn the_shared_state_is_send_and_sync_so_the_clone_boundary_is_sound() {
        // The clone-handle boundary's type-level pin (task
        // `mobile-page-signal-callbacks-off-session-lock`): the UI thread serves
        // the page-signal callbacks through a CLONED handle OFF the session
        // lock, which is only sound because the shared inner is an
        // `Arc<Mutex<_>>` — never the desktop `Rc<RefCell>` shape. If the shared
        // cell ever regresses to a single-threaded one, this stops compiling.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AndroidHandle>();
        assert_send_sync::<AndroidBackend>();
    }

    #[test]
    fn a_scheme_handler_runs_without_the_inner_lock_held() {
        // The ANR-guard half of the clone-handle boundary (task
        // `mobile-page-signal-callbacks-off-session-lock`): the UI thread's
        // page-signal callbacks borrow the SAME shared inner OFF the session
        // lock, so `resolve_scheme` must NOT hold the inner lock across the
        // handler call — a handler can run a multi-second CAR retrieval, and
        // holding the lock across it would block the UI thread's lock-free path
        // behind the retrieval (exactly the freeze shape the task removes).
        //
        // A canned handler that RE-ENTERS the inner through a cloned handle can
        // only complete if the lock is not held across the call: held, the
        // re-entry deadlocks (or panics, under the old `RefCell` shape).
        let mut b = AndroidBackend::new();
        let h = b.handle();
        let reentrant = h.clone();
        b.register_scheme_handler(
            "ipfs",
            Box::new(move |_request: SchemeRequest| {
                // Re-enter the shared inner mid-handler, the same access the
                // UI-thread page-signal path performs during a retrieval.
                reentrant.on_page_finished("https://example.com/");
                Ok(SchemeResponse::ok("text/html", b"ok".to_vec()))
            }),
        );
        let resolved = h
            .resolve_scheme("ipfs://bafycid/")
            .expect("registered, so it routes to the handler")
            .expect("the re-entrant handler completes");
        assert_eq!(resolved.body, b"ok");
        assert_eq!(b.load_state(), LoadState::Finished, "the re-entry landed");
    }

    #[test]
    fn an_unregistered_scheme_is_not_intercepted() {
        // A scheme with no registered handler returns `None` so the Android edge
        // lets the platform `WebView` handle the URL normally (e.g. `https://`).
        let b = AndroidBackend::new();
        let h = b.handle();
        assert!(h.resolve_scheme("https://example.com/").is_none());
        assert!(h.resolve_scheme("ipfs://bafycid/").is_none());
    }

    #[test]
    fn a_scheme_handler_error_is_surfaced_fail_closed() {
        // A resolution failure (a hash mismatch, an unverifiable CID, a source
        // error on the shared core path) is surfaced as `Some(Err(..))` so the
        // edge FAILS the load with an honest reason — never renders unverified
        // bytes. This is the fail-closed parity the desktop path has.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.register_scheme_handler(
            "ipfs",
            Box::new(|_request: SchemeRequest| {
                Err(RendererError::Backend(
                    "ipfs:// load failed: hash mismatch".to_string(),
                ))
            }),
        );
        let err = h
            .resolve_scheme("ipfs://tampered/")
            .expect("registered, so it routes to the handler")
            .expect_err("the handler fails the load");
        assert_eq!(
            err,
            RendererError::Backend("ipfs:// load failed: hash mismatch".to_string())
        );
    }

    #[test]
    fn a_same_document_url_change_emits_url_changed_without_a_load_transition() {
        // Acceptance (Android SPA tracking): a same-document URL change (the
        // `doUpdateVisitedHistory` the OS edge reports for an SPA `pushState`)
        // emits a DISTINCT `LoadEvent::UrlChanged`, updates the current entry, and
        // leaves the load state + trust posture UNTOUCHED — the document (and its
        // established verified posture) is unchanged. Not a fresh load.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate("ipfs://bafyroot/").unwrap();
        h.mark_content_verified();
        settle(&mut b, &h);
        assert_eq!(b.load_state(), LoadState::Finished);
        assert_eq!(b.trust_posture(), TrustPosture::ContentVerified);

        h.on_url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::UrlChanged {
                url: "ipfs://bafyroot/portfolio".into()
            })
        );
        assert_eq!(
            b.current_url().as_deref(),
            Some("ipfs://bafyroot/portfolio"),
            "the same-document URL change updates the current entry"
        );
        assert_eq!(
            b.load_state(),
            LoadState::Finished,
            "a same-document URL change is not a load transition"
        );
        assert_eq!(
            b.trust_posture(),
            TrustPosture::ContentVerified,
            "a same-document nav keeps the document's established posture"
        );
        // Back returns to the root entry (the same-document nav pushed history).
        assert!(b.can_go_back());

        // A change that merely echoes the current URL emits nothing.
        h.on_url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(b.poll_event(), None, "an unchanged URL emits no event");
    }

    /// The ronan.eth fixture root's canonical base32 CIDv1 (the form the ENS
    /// contenthash decoder produces and the internal origin carries).
    const CID_V1: &str = "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq";

    /// The internal-origin form of an `ipfs://` fixture URL, as the platform
    /// WebView loads + reports it.
    fn internal(path: &str) -> String {
        format!("https://{CID_V1}.ipfs.werust.invalid{path}")
    }

    #[test]
    fn the_pending_load_is_served_on_the_internal_https_origin() {
        // The opaque-origin fix (task mobile-ronan-eth-buttons-no-navigation):
        // the platform WebView must NOT load `ipfs://` directly (a
        // `shouldInterceptRequest`-served `ipfs://` document gets an OPAQUE
        // origin in the System WebView, which kills SvelteKit client-side nav:
        // Blink refuses `fetch(ipfs://...)` before the network stack and throws
        // `SecurityError` on `pushState`). The pending load the edge hands to
        // `WebView.loadUrl` is therefore the internal `https://` origin, while
        // the core's own truth (history, the URL bar) stays the real `ipfs://`
        // URL.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate(&format!("ipfs://{CID_V1}/")).unwrap();
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some(internal("/").as_str()),
            "the WebView loads the internal https origin"
        );
        assert_eq!(
            b.current_url().as_deref(),
            Some(format!("ipfs://{CID_V1}/").as_str()),
            "the core's truth stays the real ipfs:// URL"
        );
        // A non-ipfs pending load passes through unchanged.
        b.navigate("https://example.com/").unwrap();
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some("https://example.com/")
        );
    }

    #[test]
    fn webview_signals_on_the_internal_origin_report_back_as_ipfs() {
        // The WebView reports its load signals with the URL IT loaded (the
        // internal origin); the edge maps them back so the core's lifecycle
        // events + history never see the internal origin.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate(&format!("ipfs://{CID_V1}/")).unwrap();
        let _ = b.poll_event(); // Started
        let loaded = h.take_pending_load().expect("a pending load");
        assert!(loaded.starts_with("https://"), "loaded: {loaded}");

        h.on_page_committed(&loaded);
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::Committed {
                url: format!("ipfs://{CID_V1}/")
            }),
            "the commit signal reports the real ipfs:// URL"
        );
        h.on_page_finished(&loaded);
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::Finished {
                url: format!("ipfs://{CID_V1}/")
            })
        );
        assert_eq!(
            b.current_url().as_deref(),
            Some(format!("ipfs://{CID_V1}/").as_str())
        );
    }

    #[test]
    fn a_spa_client_side_nav_on_the_internal_origin_completes_end_to_end() {
        // THE acceptance regression guard (the mobile half of field finding D):
        // a SvelteKit `pushState` client nav, reported by the WebView as a
        // same-document history change ON THE INTERNAL ORIGIN, must complete as
        // a real navigation in the core: a `UrlChanged` event carrying the real
        // `ipfs://` URL, the history entry + Back availability updated, the
        // document's verified posture untouched, and Back returning to the
        // site's root (surfaced again on the internal origin for the WebView).
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate(&format!("ipfs://{CID_V1}/")).unwrap();
        h.mark_content_verified();
        settle(&mut b, &h);
        assert_eq!(b.trust_posture(), TrustPosture::ContentVerified);

        // The SvelteKit router's pushState to `/blog/` fires
        // `doUpdateVisitedHistory` with the internal-origin URL.
        h.on_url_changed(&internal("/blog/"));
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::UrlChanged {
                url: format!("ipfs://{CID_V1}/blog/")
            }),
            "the SPA nav reports the real ipfs:// URL, never the internal origin"
        );
        assert_eq!(
            b.current_url().as_deref(),
            Some(format!("ipfs://{CID_V1}/blog/").as_str())
        );
        assert_eq!(
            b.trust_posture(),
            TrustPosture::ContentVerified,
            "a same-document nav keeps the document's established posture"
        );
        assert!(b.can_go_back(), "the SPA nav pushed a history entry");

        // Back returns to the site root, surfaced to the WebView on the
        // internal origin again.
        b.go_back();
        assert_eq!(
            h.take_pending_load().as_deref(),
            Some(internal("/").as_str())
        );
    }

    #[test]
    fn an_internal_origin_request_routes_to_the_ipfs_scheme_handler() {
        // A page on the internal origin requests its subresources (and the
        // client router fetches `__data.json`) as `https://<cid>.ipfs.werust
        // .invalid/...`; `shouldInterceptRequest` routes every request through
        // `resolve_scheme`, which must map the URL back and dispatch it to the
        // `ipfs` handler as the real `ipfs://<cid>/...` request.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.register_scheme_handler(
            "ipfs",
            Box::new(|request: SchemeRequest| {
                Ok(SchemeResponse::ok("text/html", request.uri.into_bytes()))
            }),
        );

        let resolved = h
            .resolve_scheme(&internal("/blog/__data.json?x-sveltekit-invalidated=01"))
            .expect("the internal origin routes to the ipfs handler")
            .expect("the canned handler resolves");
        assert_eq!(
            resolved.body,
            format!("ipfs://{CID_V1}/blog/__data.json?x-sveltekit-invalidated=01").into_bytes(),
            "the handler sees the real ipfs:// request"
        );
        // A plain https request is still not intercepted.
        assert!(h.resolve_scheme("https://example.com/x").is_none());
    }

    #[test]
    fn a_failed_load_is_reported_through_the_handle() {
        let mut b = AndroidBackend::new();
        let h = b.handle();
        b.navigate("https://does-not-resolve.invalid/").unwrap();
        let _ = b.poll_event(); // Started
        let _ = h.take_pending_load();
        h.on_page_failed("https://does-not-resolve.invalid/", "name not resolved");
        assert_eq!(b.load_state(), LoadState::Failed);
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::Failed {
                url: "https://does-not-resolve.invalid/".into(),
                reason: "name not resolved".into(),
            })
        );
    }

    #[test]
    fn the_script_bridge_round_trips_a_page_message_to_a_response_push() {
        // The seam method that used to be a silent no-op is now real: a registered
        // script-message handler receives a page-posted envelope, and its response
        // push (via `evaluate_javascript`) is queued for the OS edge to run in the
        // page. This is the mechanism the EIP-1193 provider round-trips over.
        let mut b = AndroidBackend::new();
        let h = b.handle();

        // A canned handler standing in for the provider bridge: it echoes the body
        // back as a settle-call, exactly as `route_provider_message` would queue
        // the response-delivery JS.
        let sink = h.eval_sink();
        b.register_script_message_handler(
            "werustProvider",
            Box::new(move |message| {
                sink.lock()
                    .unwrap()
                    .push(format!("__settle({});", message.body));
            }),
        );

        let pushed = h.handle_script_message("werustProvider", "42");
        assert_eq!(
            pushed,
            vec!["__settle(42);".to_string()],
            "the page message reaches the handler and its response is queued to eval"
        );
        assert!(
            h.take_pending_eval().is_empty(),
            "handle_script_message drained the queue"
        );
    }

    #[test]
    fn an_unregistered_script_channel_yields_no_response() {
        // A message on a channel with no registered handler produces nothing to
        // evaluate (the provider bridge only answers its own channel).
        let b = AndroidBackend::new();
        let h = b.handle();
        assert!(h.handle_script_message("nope", "{}").is_empty());
    }

    #[test]
    fn inject_script_is_surfaced_for_the_os_edge_to_install() {
        // The `inject_script` seam no-op is gone: injected document-start scripts
        // are recorded so the OS edge can install them as page-start user scripts.
        let mut b = AndroidBackend::new();
        let h = b.handle();
        assert!(h.document_start_scripts().is_empty());
        b.inject_script("window.__shim = 1;");
        assert_eq!(h.document_start_scripts(), vec!["window.__shim = 1;"]);
    }

    #[test]
    fn the_backend_opts_into_both_trust_hooks() {
        // The backend genuinely wires BOTH trust hooks now (provider + ipfs), so
        // it declares them explicitly and passes the qualifying gate — the mobile
        // no-ops would have left it fail-closed disqualified.
        let b = AndroidBackend::new();
        assert!(renderer::qualify(&b).is_ok(), "a real backend qualifies");
    }

    #[test]
    fn trust_posture_tracks_the_verified_load_path_and_the_two_axes() {
        // The trust indicator source, made real on the Android edge (it inherited
        // the seam default before). A fresh load is untrusted; a verified load is
        // content-verified; the two-axis front-door flags surface the louder
        // warning; and a fresh navigation resets it so no posture leaks.
        let mut b = AndroidBackend::new();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);

        // A plain load stays untrusted until its bytes are proven verified.
        b.navigate("ipfs://bafycid/").unwrap();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);

        // The `ipfs` handler verified the bytes: plain content-verified (immutable
        // direct CID, no ENS/mutable flag).
        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::ContentVerified);

        // An ENS-originated verified load surfaces the louder name-via-trusted-RPC.
        b.navigate("ipfs://enscid/").unwrap();
        assert_eq!(
            b.trust_posture(),
            TrustPosture::UnverifiedOrigin,
            "reset on begin"
        );
        b.mark_ens_origin();
        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::NameViaTrustedRpc);

        // A mutable (IPNS) verified load with no RPC trust surfaces mutable-name.
        b.navigate("ipfs://ipnscid/").unwrap();
        b.mark_mutable_name();
        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::MutableName);

        // A fresh plain navigation clears every axis so no warning leaks forward.
        b.navigate("https://example.com/").unwrap();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);
    }
}
