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

use renderer::{
    KeyEvent, LoadEvent, LoadState, PointerEvent, Renderer, RendererError, SchemeHandler,
    SchemeRequest, SchemeResponse, ScriptMessageHandler, ScrollDelta, ViewHandle,
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
    fn begin(&mut self, url: &str) {
        self.pending_load = Some(url.to_string());
        self.state = LoadState::Started;
        self.events.push_back(LoadEvent::Started {
            url: url.to_string(),
        });
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

    fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {}
    fn inject_script(&mut self, _script: &str) {}

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
                Ok(SchemeResponse {
                    mime_type: "text/html".to_string(),
                    body: request.uri.into_bytes(),
                })
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
}
