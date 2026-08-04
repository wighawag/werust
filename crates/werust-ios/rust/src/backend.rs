//! [`IosBackend`]: the [`Renderer`] seam backend for the iOS OS edge.
//!
//! On iOS the forced OS edge is Swift plus the platform `WKWebView` (Apple's
//! system webview) — there is no GTK. So the browsing LOGIC (URL bar, session
//! history, load lifecycle, chrome) stays in the Rust core behind the seam,
//! exactly as the desktop [`BrowserShell`](werust_core::BrowserShell) sits over
//! WebKitGTK, and this backend is the seam implementation the core drives. It is
//! the direct twin of the Android core's `AndroidBackend` (the SAME
//! platform-neutral session-history + load-lifecycle logic; only the OS edge that
//! drives it differs — Swift over a C-ABI here, Kotlin over JNI there).
//!
//! Unlike the WebKitGTK backend, `IosBackend` does not OWN a native view: the
//! Swift `UIViewController` owns the platform `WKWebView`. So this backend is
//! *edge-driven* from both sides across the C-ABI boundary, and it shares its
//! state behind an [`Rc<RefCell>`](std::rc::Rc) — the SAME interior-mutability
//! shape `webview-renderer` uses to share a `LoadLifecycle` with the webview's
//! signal closures — so the core owns a `Box<dyn Renderer>` while the session
//! keeps an [`IosHandle`] to the same state for the platform-`WKWebView`
//! protocol:
//!
//! * The core drives navigation INTO the backend
//!   ([`navigate`](Renderer::navigate)/[`go_back`](Renderer::go_back)/…); the
//!   backend records the intent, updates its session history + load lifecycle, and
//!   surfaces the URL Swift must load onto the platform `WKWebView`
//!   ([`take_pending_load`](IosHandle::take_pending_load)).
//! * Swift reports the platform `WKWebView`'s REAL load-lifecycle signals back
//!   through the handle ([`on_page_committed`](IosHandle::on_page_committed) /
//!   [`on_page_finished`](IosHandle::on_page_finished) /
//!   [`on_page_failed`](IosHandle::on_page_failed)), which advance the lifecycle
//!   and emit the matching [`LoadEvent`]s the core's chrome reflects.
//!
//! The session history (back/forward availability, the effective URL after a
//! history move) is the BACKEND's truth — Swift never keeps a URL stack of its
//! own, so "Swift confined to the OS edge" holds: history logic lives here, in
//! Rust, behind the seam.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use renderer::{
    KeyEvent, LoadEvent, LoadState, PointerEvent, Renderer, RendererError, SchemeHandler,
    SchemeRequest, SchemeResponse, ScriptMessage, ScriptMessageHandler, ScrollDelta, TrustPosture,
    ViewHandle,
};

/// Validate a URL for [`Renderer::navigate`], rejecting unusable ones.
///
/// The same rule the WebKitGTK and Android backends use: an absolute URL with a
/// non-empty scheme and target is handed to the platform `WKWebView`; anything
/// without a scheme is rejected with [`RendererError::InvalidUrl`] and never
/// starts a load (the bad text stays in the URL bar for the user to fix).
fn validate_url(url: &str) -> Result<(), RendererError> {
    match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => Ok(()),
        _ => Err(RendererError::InvalidUrl(url.to_string())),
    }
}

/// The mutable innards shared between the [`IosBackend`] (owned by the core's
/// shell as a `dyn Renderer`) and the [`IosHandle`] (kept by the session for the
/// platform-`WKWebView` protocol the cross-backend seam does not carry).
#[derive(Default)]
struct Inner {
    /// The back/forward list; `cursor` indexes the current entry.
    history: Vec<String>,
    cursor: Option<usize>,
    state: LoadState,
    events: VecDeque<LoadEvent>,
    /// The URL the core has committed to but Swift has not yet loaded onto the
    /// platform `WKWebView`. Drained by [`IosHandle::take_pending_load`].
    pending_load: Option<String>,
    /// The registered custom-scheme handlers, keyed by scheme (e.g. `ipfs`).
    ///
    /// This is what makes [`register_scheme_handler`](Renderer::register_scheme_handler)
    /// REAL on the iOS edge (it was an empty no-op before): a `WKWebView` will
    /// only load a custom scheme like `ipfs://` if a `WKURLSchemeHandler` is
    /// registered for it, so Swift registers one that drives each intercepted
    /// request through [`IosHandle::resolve_scheme`], which dispatches to the
    /// handler stored here. The `ipfs` handler is wired by the session's
    /// `install_ipfs` (the twin of the desktop backend's `install_ipfs`), routing
    /// each request through the SAME `werust_core::ipfs::resolve_ipfs_request`
    /// path desktop uses, so the same content resolution + fail-closed trust
    /// posture apply.
    scheme_handlers: HashMap<String, SchemeHandler>,
    /// The registered script-message-bridge handlers, keyed by channel name (e.g.
    /// `werustProvider`).
    ///
    /// This is what makes [`register_script_message_handler`](Renderer::register_script_message_handler)
    /// REAL on the iOS edge (it was an empty no-op before): a `WKWebView` posts
    /// page messages to a `WKScriptMessageHandler` registered on its
    /// `WKUserContentController`, so Swift bridges the channel and drives each
    /// posted envelope through [`IosHandle::handle_script_message`], which
    /// dispatches to the handler stored here. The `werustProvider` handler is
    /// wired by the session's `install_provider` (the twin of the desktop
    /// backend's `install_provider`), routing each envelope through the SAME
    /// `werust_core::provider` EIP-1193 path desktop uses.
    script_handlers: HashMap<String, ScriptMessageHandler>,
    /// The scripts injected at document start (e.g. the EIP-1193 provider shim),
    /// in injection order. Read by [`IosHandle::document_start_scripts`] so Swift
    /// can install them onto the platform `WKWebView` as `WKUserScript`s at
    /// document start. This is the iOS stand-in for WebKitGTK's
    /// `UserContentManager::add_script` (`inject_script` was an empty no-op
    /// before).
    injected_scripts: Vec<String>,
    /// Response JS the browser must evaluate back in the live page (browser ->
    /// page), queued by a script-message handler (the EIP-1193 provider's response
    /// push that settles a page's pending Promise). This is the iOS stand-in for
    /// the desktop backend's `evaluate_javascript`: the mobile backend owns no live
    /// view, so the response JS is queued here and drained by
    /// [`IosHandle::take_pending_eval`] for Swift to run via
    /// `WKWebView.evaluateJavaScript`.
    ///
    /// Held behind an [`Arc<Mutex<_>>`](std::sync::Arc) (not the surrounding
    /// `Rc<RefCell>`) so the provider bridge handler can own a `Send` clone of
    /// JUST this queue: the seam's [`ScriptMessageHandler`] is `Send`, but the
    /// backend's shared `Inner` is `!Send` (it holds `Rc`s), so the provider
    /// closure captures this `Send` eval sink alone rather than the whole handle —
    /// the mobile twin of how the desktop `install_provider` closure captures a
    /// cloneable view handle for its response push.
    pending_eval: Arc<Mutex<Vec<String>>>,
    /// The [`TrustPosture`] of the CURRENT load: the same shared-`LoadLifecycle`
    /// posture the desktop backend surfaces, made real on the iOS edge (the seam
    /// default `UnverifiedOrigin` was inherited before). Reset to
    /// `UnverifiedOrigin` on every fresh [`begin`](Inner::begin) and upgraded ONLY
    /// when the `ipfs` scheme handler verifies this load's bytes
    /// ([`mark_content_verified`](Inner::mark_content_verified)).
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

    /// Begin a load of `url`: record it as the pending load for Swift to apply to
    /// the platform `WKWebView`, move to [`LoadState::Started`], and emit
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

/// The [`Renderer`] backend for the iOS edge: a session history + load lifecycle
/// over the platform `WKWebView`, driven from Swift across the C-ABI.
///
/// It renders nothing itself (the platform `WKWebView` does); it owns the
/// browsing LOGIC the core drives through the seam. The core holds it as `Box<dyn
/// Renderer>`; the session keeps an [`IosHandle`] (from
/// [`handle`](IosBackend::handle)) to the same shared state to run the
/// platform-`WKWebView` protocol (pending-load + signals).
#[derive(Debug, Default, Clone)]
pub struct IosBackend {
    inner: Rc<RefCell<Inner>>,
}

impl IosBackend {
    /// A fresh backend with no history, ready for the core to drive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A handle to the same shared state, for the session's platform-`WKWebView`
    /// protocol (pending-load + WKWebView load signals).
    #[must_use]
    pub fn handle(&self) -> IosHandle {
        IosHandle {
            inner: self.inner.clone(),
        }
    }
}

/// A handle to an [`IosBackend`]'s shared state, held by the session for the
/// platform-`WKWebView` protocol: the URL to load onto the `WKWebView`, and the
/// `WKWebView`'s real load signals reported back into the core.
#[derive(Debug, Clone)]
pub struct IosHandle {
    inner: Rc<RefCell<Inner>>,
}

impl IosHandle {
    /// Take the URL the core has committed to but Swift has not yet loaded onto
    /// the platform `WKWebView`, if any. Swift calls this after driving the core
    /// (navigate/back/forward/reload) and calls `WKWebView.load` with the result.
    pub fn take_pending_load(&self) -> Option<String> {
        self.inner.borrow_mut().pending_load.take()
    }

    /// Resolve an intercepted `<scheme>://…` request through the handler the
    /// session registered for that scheme, or `None` if no handler is registered.
    ///
    /// This is the iOS edge's stand-in for WebKitGTK's `register_uri_scheme`
    /// callback: a `WKWebView` loads `ipfs://` only via a registered
    /// `WKURLSchemeHandler`, so Swift's handler calls this with the intercepted
    /// URI, gets back the verified bytes + MIME type (or a fail-closed error), and
    /// answers the `WKURLSchemeTask`. The handler routes through the SAME core
    /// resolve path desktop uses, so the content resolution + trust posture +
    /// fail-closed reasons match desktop.
    ///
    /// `None` means the scheme was never registered; `Some(Err(..))` is a real,
    /// honest resolution failure that must FAIL the load, never render unverified
    /// bytes.
    pub fn resolve_scheme(&self, uri: &str) -> Option<Result<SchemeResponse, RendererError>> {
        let scheme = uri.split_once("://").map(|(s, _)| s.to_string())?;
        let mut b = self.inner.borrow_mut();
        let handler = b.scheme_handlers.get_mut(&scheme)?;
        Some(handler(SchemeRequest {
            uri: uri.to_string(),
        }))
    }

    /// Mark the CURRENT load content-verified from the OS edge: its bytes came
    /// back through the hash-verified `ipfs` resolve path. This is the iOS
    /// stand-in for the desktop `install_ipfs` scheme handler calling
    /// `life.borrow_mut().mark_content_verified()` on a verified resolution: the
    /// mobile backend owns no live `LoadLifecycle`, so the session's `resolve_ipfs`
    /// calls this the moment a resolution succeeds, and the trust indicator then
    /// surfaces the honest two-axis posture (`NameViaTrustedRpc` / `MutableName` /
    /// `ContentVerified`) for THIS load instead of the served default.
    pub fn mark_content_verified(&self) {
        self.inner.borrow_mut().mark_content_verified();
    }

    /// The scripts to inject at document start (the EIP-1193 provider shim), in
    /// injection order, so Swift can install them onto the platform `WKWebView` as
    /// `WKUserScript`s at document start. This is the read half of the iOS
    /// `inject_script` bridge, which used to be an empty no-op.
    #[must_use]
    pub fn document_start_scripts(&self) -> Vec<String> {
        self.inner.borrow().injected_scripts.clone()
    }

    /// Dispatch a page-posted script-message envelope on channel `name` to the
    /// registered handler (the EIP-1193 provider bridge), then drain and return
    /// the response JS (if any) the browser must evaluate back in the page to
    /// settle the page's pending Promise.
    ///
    /// This is the iOS edge's stand-in for WebKitGTK's
    /// `connect_script_message_received` + `evaluate_javascript` round-trip: the
    /// `WKWebView` posts page messages to a `WKScriptMessageHandler`, so Swift
    /// bridges the channel and calls this with each posted body; the handler
    /// answers it (queuing the response JS via `evaluate_javascript`) and this
    /// returns that JS for Swift to run with `WKWebView.evaluateJavaScript`.
    /// `None` (empty vec) means the channel is unregistered or the message needed
    /// no response.
    #[must_use]
    pub fn handle_script_message(&self, name: &str, body: &str) -> Vec<String> {
        // Take the handler OUT of the map for the duration of the call so the
        // `RefCell` is not borrowed across the handler body: the handler is a
        // `FnMut` capturing its own response sink (`evaluate_javascript`, which
        // borrows the same `Inner` to queue into `pending_eval`), so holding the
        // borrow here would be a re-entrant `borrow_mut` panic. Re-insert it after.
        let taken = self.inner.borrow_mut().script_handlers.remove(name);
        if let Some(mut handler) = taken {
            handler(ScriptMessage {
                handler: name.to_string(),
                body: body.to_string(),
            });
            self.inner
                .borrow_mut()
                .script_handlers
                .insert(name.to_string(), handler);
        }
        self.take_pending_eval()
    }

    /// Drain the response JS the browser must evaluate back in the live page
    /// (browser -> page), queued by a script-message handler's response push. The
    /// iOS stand-in for the desktop `evaluate_javascript` immediate eval: the
    /// backend owns no live view, so Swift runs these with
    /// `WKWebView.evaluateJavaScript`.
    #[must_use]
    pub fn take_pending_eval(&self) -> Vec<String> {
        let queue = self.inner.borrow().pending_eval.clone();
        let mut q = queue.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::take(&mut *q)
    }

    /// A `Send` clone of JUST the response-JS eval queue (browser -> page), for the
    /// session's `install_provider` to hand
    /// [`route_provider_message`](werust_core::provider::route_provider_message)
    /// as its `respond` sink: the provider handler pushes each response-delivery
    /// call here, and [`take_pending_eval`](IosHandle::take_pending_eval) drains it
    /// for `WKWebView.evaluateJavaScript`. Cloning JUST this `Arc` (not the
    /// `!Send` backend handle) is what lets the seam's `Send`
    /// [`ScriptMessageHandler`] capture the sink — the mobile twin of the desktop
    /// `install_provider` closure capturing a cloneable view handle.
    #[must_use]
    pub fn eval_sink(&self) -> Arc<Mutex<Vec<String>>> {
        self.inner.borrow().pending_eval.clone()
    }

    /// Report a SAME-DOCUMENT URL change: an SPA `pushState`/`replaceState`
    /// client-side navigation rewrote the address WITHOUT a fresh page load, so no
    /// `didCommit`/`didFinish` fires. Called from Swift's KVO observer on
    /// `webView.url` (which fires on same-document history changes).
    ///
    /// It emits ONLY a [`LoadEvent::UrlChanged`] and updates the session-history
    /// entry, but leaves the load state, trust posture, and per-load flags
    /// UNTOUCHED — the document (and its already-established verified/ENS posture)
    /// is unchanged; the SPA only rewrote the history URL. This is the mobile twin
    /// of the desktop `LoadLifecycle::url_changed` (WebKitGTK `notify::uri`) and
    /// the Android `AndroidHandle::on_url_changed` (`doUpdateVisitedHistory`). A
    /// NO-OP when `url` already matches the current entry, so a KVO fire that
    /// merely echoes the current load's URL (a real load, not an SPA nav) emits
    /// nothing.
    pub fn on_url_changed(&self, url: &str) {
        let mut b = self.inner.borrow_mut();
        if b.current().map(String::as_str) == Some(url) {
            return;
        }
        // A same-document history push adds a forward entry from mid-history,
        // dropping any forward entries — just like a navigation, but with NO load
        // lifecycle reset (state/posture/flags keep the current document's values).
        let next = b.cursor.map_or(0, |c| c + 1);
        b.history.truncate(next);
        b.history.push(url.to_string());
        b.cursor = Some(b.history.len() - 1);
        b.events.push_back(LoadEvent::UrlChanged {
            url: url.to_string(),
        });
    }

    /// Report that the platform `WKWebView` navigated its OWN back-forward list:
    /// the user's EDGE-SWIPE gesture (`allowsBackForwardNavigationGestures`), or a
    /// page calling `history.back()`/`forward()`. Called from Swift's
    /// `decidePolicyFor` when the navigation type is `.backForward`, with the
    /// target URL — the earliest signal WebKit gives, before the new document's
    /// bytes are resolved.
    ///
    /// This is a history MOVE, not a new entry, and that is the whole difference
    /// from [`on_url_changed`](IosHandle::on_url_changed): a swipe RE-ENTERS an
    /// entry the session already has, so the cursor must move onto it. Reported as
    /// an ordinary URL change (which pushes), a swipe back from `b` to `a` would
    /// leave the history `[a, b, a]`: Forward would read false while the user can
    /// plainly swipe forward, and every swipe would leak another entry.
    ///
    /// The move is resolved by matching `url` against the ADJACENT entries rather
    /// than by a direction the edge reports, because `WKNavigationAction` names a
    /// navigation `.backForward` without saying WHICH way it went. Back is checked
    /// first, so an ambiguous history (`[a, b, a]` standing on `b`, swiping to
    /// `a`) resolves as a step BACK — the far commoner gesture, and either reading
    /// leaves the bar on the same URL.
    ///
    /// A target that is NEITHER neighbour means WebKit's back-forward list and the
    /// session history have DRIFTED (a core-driven history move is performed as a
    /// fresh `WKWebView.load`, which APPENDS to WebKit's list rather than moving
    /// its cursor). The bar then FOLLOWS the page the user is actually looking at,
    /// by pushing, exactly as [`on_url_changed`](IosHandle::on_url_changed) does:
    /// a browser whose thesis is an honest address must never show an address the
    /// user is not on.
    ///
    /// It moves the cursor and RESETS the per-load trust axes — the target is a
    /// DIFFERENT document, so the current one's `ContentVerified` /
    /// `NameViaTrustedRpc` posture must not be carried onto it — but it does NOT
    /// touch the load state and queues NO pending load: WebKit is already
    /// performing this navigation, and re-issuing it would fight the gesture. The
    /// load state is moved by the ordinary `didCommit`/`didFinish` signals that
    /// follow (a cross-document swipe), or by nothing at all (a same-document
    /// entry, which fires neither) — which is why a `Started` is deliberately NOT
    /// emitted here: it would leave a same-document swipe stuck "loading" forever.
    ///
    /// Idempotent: reporting the entry the session is ALREADY on does nothing, so
    /// the KVO `url` observer and the commit signal that follow the same gesture
    /// cannot walk the cursor a second time.
    pub fn on_history_navigated(&self, url: &str) {
        let mut b = self.inner.borrow_mut();
        if b.current().map(String::as_str) == Some(url) {
            return;
        }
        let back = b
            .cursor
            .filter(|c| *c > 0)
            .filter(|c| b.history[c - 1] == url);
        let forward = b
            .cursor
            .filter(|c| c + 1 < b.history.len())
            .filter(|c| b.history[c + 1] == url);
        match (back, forward) {
            (Some(c), _) => b.cursor = Some(c - 1),
            (None, Some(c)) => b.cursor = Some(c + 1),
            (None, None) => {
                // The two stacks have drifted: follow the URL as a new entry, the
                // same shape a webview-initiated navigation takes.
                let next = b.cursor.map_or(0, |c| c + 1);
                b.history.truncate(next);
                b.history.push(url.to_string());
                b.cursor = Some(b.history.len() - 1);
            }
        }
        // A different document is being entered, so the CURRENT one's posture must
        // not be carried onto it. Resetting understates trust when WebKit restores
        // a verified page from its page cache (no scheme task re-runs, so nothing
        // re-marks it) and that is the fail-closed direction: werust may show less
        // trust than a page has, never more (`docs/adr/0006`).
        b.posture = TrustPosture::UnverifiedOrigin;
        b.ens_origin = false;
        b.mutable_name = false;
        b.events.push_back(LoadEvent::UrlChanged {
            url: url.to_string(),
        });
    }

    /// Report that the platform `WKWebView` committed the load on `url` (the
    /// effective URL after any redirects): advance to [`LoadState::Committed`] and
    /// emit [`LoadEvent::Committed`]. Called from Swift's `didCommit`.
    pub fn on_page_committed(&self, url: &str) {
        let mut b = self.inner.borrow_mut();
        b.state = LoadState::Committed;
        b.events.push_back(LoadEvent::Committed {
            url: url.to_string(),
        });
    }

    /// Report that the platform `WKWebView` finished loading `url`: advance to
    /// [`LoadState::Finished`] and emit [`LoadEvent::Finished`]. Called from
    /// Swift's `didFinish`.
    pub fn on_page_finished(&self, url: &str) {
        let mut b = self.inner.borrow_mut();
        b.state = LoadState::Finished;
        b.events.push_back(LoadEvent::Finished {
            url: url.to_string(),
        });
    }

    /// Report that the platform `WKWebView` failed to load `url`: advance to
    /// [`LoadState::Failed`] and emit [`LoadEvent::Failed`]. Called from Swift's
    /// `didFail` / `didFailProvisionalNavigation`.
    pub fn on_page_failed(&self, url: &str, reason: &str) {
        let mut b = self.inner.borrow_mut();
        b.state = LoadState::Failed;
        b.events.push_back(LoadEvent::Failed {
            url: url.to_string(),
            reason: reason.to_string(),
        });
    }
}

impl Renderer for IosBackend {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        validate_url(url)?;
        let mut b = self.inner.borrow_mut();
        // A fresh navigation from mid-history drops the forward entries.
        let next = b.cursor.map_or(0, |c| c + 1);
        b.history.truncate(next);
        b.history.push(url.to_string());
        b.cursor = Some(b.history.len() - 1);
        b.begin(url);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        let mut b = self.inner.borrow_mut();
        let url = b
            .current()
            .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?
            .clone();
        b.begin(&url);
        Ok(())
    }

    fn stop(&mut self) {
        let mut b = self.inner.borrow_mut();
        if b.state.is_loading() {
            b.state = LoadState::Idle;
        }
    }

    fn go_back(&mut self) {
        let mut b = self.inner.borrow_mut();
        if let Some(c) = b.cursor {
            if c > 0 {
                b.cursor = Some(c - 1);
                let url = b.history[c - 1].clone();
                b.begin(&url);
            }
        }
    }

    fn go_forward(&mut self) {
        let mut b = self.inner.borrow_mut();
        if let Some(c) = b.cursor {
            if c + 1 < b.history.len() {
                b.cursor = Some(c + 1);
                let url = b.history[c + 1].clone();
                b.begin(&url);
            }
        }
    }

    fn can_go_back(&self) -> bool {
        matches!(self.inner.borrow().cursor, Some(c) if c > 0)
    }

    fn can_go_forward(&self) -> bool {
        let b = self.inner.borrow();
        matches!(b.cursor, Some(c) if c + 1 < b.history.len())
    }

    fn load_state(&self) -> LoadState {
        self.inner.borrow().state
    }

    fn current_url(&self) -> Option<String> {
        self.inner.borrow().current().cloned()
    }

    fn poll_event(&mut self) -> Option<LoadEvent> {
        self.inner.borrow_mut().events.pop_front()
    }

    fn view_handle(&self) -> ViewHandle {
        // The iOS edge owns the platform WKWebView; the core never embeds a view
        // handle here (unlike the GTK edge). The seam still requires the method.
        ViewHandle(std::ptr::null_mut())
    }

    fn send_pointer(&mut self, _event: PointerEvent) {}
    fn send_key(&mut self, _event: KeyEvent) {}
    fn send_scroll(&mut self, _delta: ScrollDelta) {}
    fn set_focus(&mut self, _focused: bool) {}

    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
        // Store the handler so the iOS edge can dispatch page-posted envelopes to
        // it from its `WKScriptMessageHandler` via
        // [`IosHandle::handle_script_message`]. This is the seam method that used
        // to be a silent no-op — the exact gap the platform-capability parity
        // guard exists to forbid; it is now real. It is the channel the EIP-1193
        // provider is injected over (`install_provider`).
        self.inner
            .borrow_mut()
            .script_handlers
            .insert(name.to_string(), handler);
    }

    fn inject_script(&mut self, script: &str) {
        // Record the document-start script (the EIP-1193 provider shim) so the iOS
        // edge can install it onto the platform `WKWebView` as a `WKUserScript` at
        // document start via [`IosHandle::document_start_scripts`]. The seam method
        // that used to be a silent no-op is now real.
        self.inner
            .borrow_mut()
            .injected_scripts
            .push(script.to_string());
    }

    fn evaluate_javascript(&self, script: &str) {
        // Queue the response JS (browser -> page) for the iOS edge to run in the
        // live page via `WKWebView.evaluateJavaScript`. The backend owns no live
        // view, so unlike the desktop backend (which evaluates immediately on the
        // GTK loop) the JS is queued and drained by
        // [`IosHandle::take_pending_eval`]. This is the RESPONSE half of the
        // provider round-trip that settles a page's pending Promise.
        if let Ok(mut queue) = self.inner.borrow().pending_eval.lock() {
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
        self.inner.borrow().posture
    }

    fn mark_ens_origin(&mut self) {
        // Flag the current load ENS-originated (the front door resolved the name
        // over the trusted RPC), so when the `ipfs` handler later verifies the
        // bytes the posture surfaces `NameViaTrustedRpc`. A fresh `begin` clears
        // the flag. The twin of the desktop backend's `mark_ens_origin`.
        self.inner.borrow_mut().ens_origin = true;
    }

    fn mark_mutable_name(&mut self) {
        // Flag the current load's name MUTABLE (an IPNS resolution), so a verified
        // load surfaces at most `MutableName` (or the louder `NameViaTrustedRpc` if
        // also ENS-originated), never immutable `ContentVerified`. A fresh `begin`
        // clears the flag. The twin of the desktop backend's `mark_mutable_name`.
        self.inner.borrow_mut().mutable_name = true;
    }

    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
        // Store the handler so the iOS edge can dispatch to it from its
        // `WKURLSchemeHandler` via [`IosHandle::resolve_scheme`]. This is the seam
        // method that used to be a silent no-op — the exact gap the
        // platform-capability parity guard exists to forbid; it is now real.
        self.inner
            .borrow_mut()
            .scheme_handlers
            .insert(scheme.to_string(), handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the in-flight load to done the way Swift would from the platform
    /// `WKWebView`'s `didCommit` + `didFinish` signals.
    fn settle(backend: &mut IosBackend, handle: &IosHandle) {
        let url = handle
            .take_pending_load()
            .expect("a pending load to apply to the WKWebView");
        handle.on_page_committed(&url);
        handle.on_page_finished(&url);
        // Drain the events the core would drain via the seam.
        while backend.poll_event().is_some() {}
    }

    #[test]
    fn navigate_surfaces_a_pending_load_and_drives_the_lifecycle() {
        // The core navigates INTO the backend; the handle surfaces the URL Swift
        // must load onto the platform WKWebView and the lifecycle starts.
        let mut b = IosBackend::new();
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
        let mut b = IosBackend::new();
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
        // truth (Swift keeps no URL stack of its own).
        let mut b = IosBackend::new();
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
            "back surfaces the prior URL for the WKWebView"
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
        let mut b = IosBackend::new();
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
        let mut b = IosBackend::new();
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
        // registered for `ipfs` is stored and dispatched when the iOS edge (the
        // `WKURLSchemeHandler`) resolves an intercepted `ipfs://` request.
        let mut b = IosBackend::new();
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
    fn an_unregistered_scheme_is_not_intercepted() {
        // A scheme with no registered handler returns `None` so the iOS edge lets
        // the `WKWebView` handle the URL normally (e.g. `https://`).
        let b = IosBackend::new();
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
        let mut b = IosBackend::new();
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
        // Acceptance (iOS SPA tracking): a same-document URL change (the KVO on
        // `webView.url` the OS edge reports for an SPA `pushState`) emits a
        // DISTINCT `LoadEvent::UrlChanged`, updates the current entry, and leaves
        // the load state + trust posture UNTOUCHED — the document (and its
        // established verified posture) is unchanged. Not a fresh load.
        let mut b = IosBackend::new();
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
        assert!(b.can_go_back());

        h.on_url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(b.poll_event(), None, "an unchanged URL emits no event");
    }

    #[test]
    fn a_gesture_history_move_lands_the_cursor_without_queuing_a_load() {
        // The EDGE-SWIPE gesture (task `enable-the-ios-back-forward-swipe-gesture`):
        // WebKit navigates its OWN back-forward list, so the edge REPORTS the
        // target rather than driving `go_back`. The cursor must land on the entry
        // swiped to (not push a duplicate), the event is a URL change (not a fresh
        // load lifecycle), and NOTHING may be queued for the WKWebView — a pending
        // load here would re-navigate on top of the navigation WebKit is already
        // performing.
        let mut b = IosBackend::new();
        let h = b.handle();
        b.navigate("https://a.example/").unwrap();
        settle(&mut b, &h);
        b.navigate("https://b.example/").unwrap();
        settle(&mut b, &h);

        h.on_history_navigated("https://a.example/");
        assert_eq!(b.current_url().as_deref(), Some("https://a.example/"));
        assert!(!b.can_go_back(), "the cursor MOVED back, it did not push");
        assert!(
            b.can_go_forward(),
            "the entry swiped away from is still ahead"
        );
        assert_eq!(
            b.poll_event(),
            Some(LoadEvent::UrlChanged {
                url: "https://a.example/".into()
            })
        );
        assert_eq!(
            b.load_state(),
            LoadState::Finished,
            "no load transition of its own: a cross-document swipe's didCommit / \
             didFinish move the state, and a same-document one fires neither — \
             emitting a Started here would strand the latter as forever-loading"
        );
        assert_eq!(h.take_pending_load(), None, "WebKit is already navigating");

        // Idempotent: the KVO url observer and the commit signal report the same
        // URL moments later, and must not walk the cursor a second time.
        h.on_history_navigated("https://a.example/");
        assert_eq!(b.poll_event(), None);
        assert!(!b.can_go_back());
        assert!(b.can_go_forward());

        // Forward, the direction Android's system Back has no equivalent for.
        h.on_history_navigated("https://b.example/");
        assert_eq!(b.current_url().as_deref(), Some("https://b.example/"));
        assert!(b.can_go_back());
        assert!(!b.can_go_forward());

        // A target neither neighbour holds means WebKit's list and the session
        // history have DRIFTED; follow it rather than show an address the user is
        // not on.
        h.on_history_navigated("https://c.example/");
        assert_eq!(b.current_url().as_deref(), Some("https://c.example/"));
        assert!(b.can_go_back());
        assert!(!b.can_go_forward());
    }

    #[test]
    fn a_gesture_history_move_never_carries_the_previous_documents_trust_posture() {
        // The subtler half of the same bug: a swipe enters a DIFFERENT document,
        // so the posture of the one being left must not travel with it. Swiping
        // back from a hash-verified `ipfs://` page onto a plain served page would
        // otherwise leave the badge claiming content-verified for bytes nobody
        // verified — the overclaim `docs/adr/0006` exists to forbid.
        let mut b = IosBackend::new();
        let h = b.handle();
        b.navigate("https://plain.example/").unwrap();
        settle(&mut b, &h);
        b.navigate("ipfs://bafycid/").unwrap();
        b.mark_ens_origin();
        h.mark_content_verified();
        settle(&mut b, &h);
        assert_eq!(b.trust_posture(), TrustPosture::NameViaTrustedRpc);

        h.on_history_navigated("https://plain.example/");
        assert_eq!(
            b.trust_posture(),
            TrustPosture::UnverifiedOrigin,
            "the verified page's posture must not follow the user back onto a \
             served one"
        );
        // The per-load AXES are cleared with it, so a later verification on the
        // entered document surfaces ITS own posture, not the previous entry's ENS
        // origin.
        h.mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::ContentVerified);
    }

    #[test]
    fn a_failed_load_is_reported_through_the_handle() {
        let mut b = IosBackend::new();
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
        let mut b = IosBackend::new();
        let h = b.handle();

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
        let b = IosBackend::new();
        let h = b.handle();
        assert!(h.handle_script_message("nope", "{}").is_empty());
    }

    #[test]
    fn inject_script_is_surfaced_for_the_os_edge_to_install() {
        // The `inject_script` seam no-op is gone: injected document-start scripts
        // are recorded so the OS edge can install them as `WKUserScript`s.
        let mut b = IosBackend::new();
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
        let b = IosBackend::new();
        assert!(renderer::qualify(&b).is_ok(), "a real backend qualifies");
    }

    #[test]
    fn trust_posture_tracks_the_verified_load_path_and_the_two_axes() {
        // The trust indicator source, made real on the iOS edge (it inherited the
        // seam default before). A fresh load is untrusted; a verified load is
        // content-verified; the two-axis front-door flags surface the louder
        // warning; and a fresh navigation resets it so no posture leaks.
        let mut b = IosBackend::new();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);

        b.navigate("ipfs://bafycid/").unwrap();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);

        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::ContentVerified);

        b.navigate("ipfs://enscid/").unwrap();
        assert_eq!(
            b.trust_posture(),
            TrustPosture::UnverifiedOrigin,
            "reset on begin"
        );
        b.mark_ens_origin();
        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::NameViaTrustedRpc);

        b.navigate("ipfs://ipnscid/").unwrap();
        b.mark_mutable_name();
        b.handle().mark_content_verified();
        assert_eq!(b.trust_posture(), TrustPosture::MutableName);

        b.navigate("https://example.com/").unwrap();
        assert_eq!(b.trust_posture(), TrustPosture::UnverifiedOrigin);
    }
}
