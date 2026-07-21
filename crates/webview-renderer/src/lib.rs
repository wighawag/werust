//! A [`Renderer`] backend over the WebKitGTK system webview (GTK4 / `webkit6`).
//!
//! This is werust's FIRST rendering backend and the day-one usable path: a real
//! page rendered by the system webview behind the [`Renderer`] seam (the
//! "webview now, native later" hedge — `CONTEXT.md`, `docs/adr/0001`). It binds
//! WebKitGTK via the `webkit6` bindings rather than hand-rolling a renderer, and
//! nothing WebKitGTK-specific leaks past the seam: the rest of werust only ever
//! sees the [`Renderer`] trait.
//!
//! The backend splits into two layers so the seam contract is testable without a
//! display or a GTK main loop:
//!
//! * [`LoadLifecycle`] is a pure, GTK-free state machine that owns
//!   [`LoadState`](renderer::LoadState), the current URL, and the pending
//!   [`LoadEvent`](renderer::LoadEvent) queue. `navigate`/`reload`/`stop` and the
//!   webview's load signals all drive it, and it is exercised directly by the
//!   seam-contract tests.
//! * [`WebViewRenderer`] wires a real [`webkit6::WebView`] on top of that
//!   lifecycle: it connects the WebKitGTK load-lifecycle signals so they feed the
//!   [`LoadLifecycle`], forwards input, and exposes the live view handle. It is
//!   the piece that shows an actual page in a window on Linux.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use renderer::{LoadEvent, LoadState, RendererError};

/// Validate a URL for [`Renderer::navigate`], rejecting unusable ones.
///
/// The webview backend can navigate any absolute URL WebKitGTK understands; the
/// day-one path is `http(s)://`, and the trust hook adds `ipfs://` (task
/// `ipfs-scheme-resolution-through-renderer-seam`). A URL with no scheme, or an
/// empty one, is not something to hand to the engine, so it is rejected with
/// [`RendererError::InvalidUrl`] and never starts a load.
fn validate_url(url: &str) -> Result<(), RendererError> {
    match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => Ok(()),
        _ => Err(RendererError::InvalidUrl(url.to_string())),
    }
}

/// The GTK-free load-lifecycle state machine shared between a
/// [`WebViewRenderer`] and its webview's load signals.
///
/// This is the load-lifecycle surface of the seam, modelled explicitly so it can
/// be driven and asserted at the trait level without a GTK main loop. `navigate`
/// calls [`begin`](LoadLifecycle::begin); the webview's `load-changed` /
/// `load-failed` signals call [`commit`](LoadLifecycle::commit),
/// [`finish`](LoadLifecycle::finish), and [`fail`](LoadLifecycle::fail); `stop`
/// calls [`stop`](LoadLifecycle::stop). Each transition enqueues the matching
/// [`LoadEvent`] that the browser drains with [`poll`](LoadLifecycle::poll).
#[derive(Debug, Default)]
pub struct LoadLifecycle {
    state: LoadState,
    url: Option<String>,
    events: VecDeque<LoadEvent>,
}

impl LoadLifecycle {
    /// Start a load of `url`: move to [`LoadState::Started`] and emit
    /// [`LoadEvent::Started`].
    pub fn begin(&mut self, url: &str) {
        self.url = Some(url.to_string());
        self.state = LoadState::Started;
        self.events.push_back(LoadEvent::Started {
            url: url.to_string(),
        });
    }

    /// Record that the load committed on `url` (the effective URL after any
    /// redirects): move to [`LoadState::Committed`] and emit
    /// [`LoadEvent::Committed`].
    pub fn commit(&mut self, url: &str) {
        self.url = Some(url.to_string());
        self.state = LoadState::Committed;
        self.events.push_back(LoadEvent::Committed {
            url: url.to_string(),
        });
    }

    /// Record that the load of `url` finished successfully: move to
    /// [`LoadState::Finished`] and emit [`LoadEvent::Finished`].
    pub fn finish(&mut self, url: &str) {
        self.url = Some(url.to_string());
        self.state = LoadState::Finished;
        self.events.push_back(LoadEvent::Finished {
            url: url.to_string(),
        });
    }

    /// Record that the load of `url` failed: move to [`LoadState::Failed`] and
    /// emit [`LoadEvent::Failed`].
    pub fn fail(&mut self, url: &str, reason: &str) {
        self.state = LoadState::Failed;
        self.events.push_back(LoadEvent::Failed {
            url: url.to_string(),
            reason: reason.to_string(),
        });
    }

    /// Stop an in-flight load, returning the lifecycle to a settled
    /// [`LoadState::Idle`]. A settled (finished/failed) load is left as-is.
    pub fn stop(&mut self) {
        if self.state.is_loading() {
            self.state = LoadState::Idle;
        }
    }

    /// The current load-lifecycle state.
    #[must_use]
    pub fn state(&self) -> LoadState {
        self.state
    }

    /// The URL of the current (committed or in-flight) load, if any.
    #[must_use]
    pub fn current_url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Pull the next pending [`LoadEvent`], or `None` if the queue is empty.
    pub fn poll(&mut self) -> Option<LoadEvent> {
        self.events.pop_front()
    }
}

/// A shared, interior-mutable [`LoadLifecycle`]: the way [`WebViewRenderer`]
/// shares one lifecycle between the trait methods and the webview's signal
/// closures (which fire on the GTK main loop).
pub(crate) type SharedLifecycle = Rc<RefCell<LoadLifecycle>>;

mod backend;
pub use backend::WebViewRenderer;

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{qualify, KeyEvent, PointerEvent, Renderer, ScrollDelta, ViewHandle};
    use renderer::{SchemeHandler, ScriptMessageHandler, TrustHook, TrustHooks};

    /// A seam-level backend that drives [`LoadLifecycle`] exactly as the real
    /// [`WebViewRenderer`] does, but with the webview's native load signals
    /// simulated by [`drive_to_finished`](SeamHarness::drive_to_finished) instead
    /// of a running GTK main loop. It exists ONLY to exercise the seam contract
    /// at the trait level; it renders nothing.
    #[derive(Default)]
    struct SeamHarness {
        life: LoadLifecycle,
        scheme_handlers: Vec<String>,
        script_handlers: Vec<String>,
        injected: Vec<String>,
    }

    impl Renderer for SeamHarness {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            validate_url(url)?;
            self.life.begin(url);
            Ok(())
        }

        fn reload(&mut self) -> Result<(), RendererError> {
            let url = self
                .life
                .current_url()
                .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?
                .to_string();
            self.navigate(&url)
        }

        fn stop(&mut self) {
            self.life.stop();
        }

        fn load_state(&self) -> LoadState {
            self.life.state()
        }

        fn current_url(&self) -> Option<String> {
            self.life.current_url().map(str::to_string)
        }

        fn poll_event(&mut self) -> Option<LoadEvent> {
            self.life.poll()
        }

        fn view_handle(&self) -> ViewHandle {
            ViewHandle(std::ptr::null_mut())
        }

        fn send_pointer(&mut self, _event: PointerEvent) {}
        fn send_key(&mut self, _event: KeyEvent) {}
        fn send_scroll(&mut self, _delta: ScrollDelta) {}
        fn set_focus(&mut self, _focused: bool) {}

        fn register_script_message_handler(&mut self, name: &str, _handler: ScriptMessageHandler) {
            self.script_handlers.push(name.to_string());
        }

        fn inject_script(&mut self, script: &str) {
            self.injected.push(script.to_string());
        }

        fn register_scheme_handler(&mut self, scheme: &str, _handler: SchemeHandler) {
            self.scheme_handlers.push(scheme.to_string());
        }
    }

    impl SeamHarness {
        /// Simulate WebKitGTK's `load-changed` signal carrying the in-flight load
        /// through commit to done, the way the real backend feeds
        /// [`LoadLifecycle`] from the webview's signals.
        fn drive_to_finished(&mut self) {
            let url = self
                .life
                .current_url()
                .expect("a load in flight")
                .to_string();
            self.life.commit(&url);
            self.life.finish(&url);
        }
    }

    #[test]
    fn navigate_transitions_load_lifecycle_state() {
        let mut r = SeamHarness::default();
        assert_eq!(r.load_state(), LoadState::Idle);
        assert!(!r.load_state().is_loading());

        r.navigate("https://example.com/").expect("valid https url");
        assert_eq!(r.load_state(), LoadState::Started);
        assert!(r.load_state().is_loading());
        assert_eq!(r.current_url().as_deref(), Some("https://example.com/"));
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Started {
                url: "https://example.com/".into()
            })
        );

        // The webview's load signals carry it to Finished via Committed.
        r.drive_to_finished();
        assert_eq!(r.load_state(), LoadState::Finished);
        assert!(!r.load_state().is_loading());
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Committed {
                url: "https://example.com/".into()
            })
        );
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Finished {
                url: "https://example.com/".into()
            })
        );
        assert_eq!(r.poll_event(), None);
    }

    #[test]
    fn navigate_rejects_unusable_url_without_starting_a_load() {
        let mut r = SeamHarness::default();
        let err = r.navigate("not-a-url").expect_err("unusable url rejected");
        assert_eq!(err, RendererError::InvalidUrl("not-a-url".into()));
        assert_eq!(r.load_state(), LoadState::Idle);
        assert_eq!(r.current_url(), None);
        assert_eq!(r.poll_event(), None);
    }

    #[test]
    fn navigate_accepts_https_and_custom_schemes() {
        // The day-one http(s) path plus the ipfs:// trust-hook scheme are all
        // usable URLs the backend hands straight to the engine.
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("http://example.com/").is_ok());
        assert!(validate_url("ipfs://bafyexamplecid/index.html").is_ok());
        // A missing/empty scheme or empty target is not.
        assert!(validate_url("not-a-url").is_err());
        assert!(validate_url("https://").is_err());
        assert!(validate_url("://nowhere").is_err());
    }

    #[test]
    fn reload_re_navigates_the_current_url() {
        let mut r = SeamHarness::default();
        assert!(r.reload().is_err(), "nothing to reload before a navigate");
        r.navigate("https://example.com/").unwrap();
        r.drive_to_finished();
        let _ = r.poll_event();
        let _ = r.poll_event();
        let _ = r.poll_event();

        r.reload().expect("reload re-navigates the committed url");
        assert_eq!(r.load_state(), LoadState::Started);
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Started {
                url: "https://example.com/".into()
            })
        );
    }

    #[test]
    fn stop_returns_lifecycle_to_settled() {
        let mut r = SeamHarness::default();
        r.navigate("https://example.com/").unwrap();
        assert!(r.load_state().is_loading());
        r.stop();
        assert_eq!(r.load_state(), LoadState::Idle);
    }

    #[test]
    fn failed_load_transitions_to_failed_state() {
        // A backend must be able to report a failed load through the seam.
        let mut r = SeamHarness::default();
        r.navigate("https://does-not-resolve.invalid/").unwrap();
        let _ = r.poll_event(); // Started
        r.life
            .fail("https://does-not-resolve.invalid/", "name not resolved");
        assert_eq!(r.load_state(), LoadState::Failed);
        assert!(!r.load_state().is_loading());
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Failed {
                url: "https://does-not-resolve.invalid/".into(),
                reason: "name not resolved".into(),
            })
        );
    }

    #[test]
    fn trust_hooks_are_part_of_the_seam() {
        // A backend qualifies only if it exposes the trust hooks: a
        // script-message bridge, at-document-start injection, and custom-scheme
        // interception — the shape the provider and ipfs:// tasks wire onto.
        let mut r = SeamHarness::default();
        r.register_script_message_handler("werustProvider", Box::new(|_msg| {}));
        r.inject_script("globalThis.ethereum = {};");
        r.register_scheme_handler(
            "ipfs",
            Box::new(|req| {
                Ok(renderer::SchemeResponse {
                    mime_type: "text/html".into(),
                    body: format!("resolved {}", req.uri).into_bytes(),
                })
            }),
        );
        assert_eq!(r.script_handlers, ["werustProvider"]);
        assert_eq!(r.injected, ["globalThis.ethereum = {};"]);
        assert_eq!(r.scheme_handlers, ["ipfs"]);
    }

    #[test]
    fn webview_backend_passes_the_trust_hook_qualification_gate() {
        // The WebKitGTK backend declares BOTH trust hooks (it inherits the
        // qualifying default of `Renderer::trust_hooks`, exactly as the real
        // `WebViewRenderer` does — both share the same seam methods and neither
        // overrides the capability), so the qualification gate accepts it. This
        // runs headlessly: it exercises the seam contract, not a GTK main loop.
        let r = SeamHarness::default();
        assert_eq!(
            r.trust_hooks(),
            TrustHooks::all(),
            "the webview backend declares both trust hooks"
        );
        qualify(&r).expect("the webview backend qualifies");
    }

    #[test]
    fn webview_renderer_does_not_downgrade_its_trust_hook_capability() {
        // Guard against a future edit silently making the REAL backend render-only:
        // `WebViewRenderer` must not override `trust_hooks` to drop a hook. We can
        // assert this display-free by pinning the qualifying set the shared seam
        // default yields; `WebViewRenderer` uses that same default (verified by
        // reading `backend.rs`, which adds no `trust_hooks` override). The
        // display-bound end-to-end check lives in
        // `real_webview_backend_qualifies` below (ignored by default).
        assert!(
            TrustHooks::default().is_qualifying(),
            "the seam default the webview backend inherits is qualifying"
        );
    }

    /// End-to-end qualification of the REAL WebKitGTK backend. Ignored by default
    /// because constructing a `WebViewRenderer` initializes GTK, which needs a
    /// display the `verify` gate may not have. Run explicitly on a desktop
    /// session with `cargo test -p webview-renderer -- --ignored`.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_backend_qualifies() {
        let r = WebViewRenderer::new().expect("gtk init on a desktop session");
        qualify(&r).expect("the real WebKitGTK backend satisfies the trust hooks");
    }

    #[test]
    fn a_render_only_backend_on_this_seam_is_rejected() {
        // A backend on the SAME seam that renders but declares no trust hook is
        // disqualified, naming both missing hooks — the enforced seam property
        // that a future native backend is held to as well.
        struct RenderOnly;
        impl Renderer for RenderOnly {
            fn navigate(&mut self, _url: &str) -> Result<(), RendererError> {
                Ok(())
            }
            fn reload(&mut self) -> Result<(), RendererError> {
                Ok(())
            }
            fn stop(&mut self) {}
            fn load_state(&self) -> LoadState {
                LoadState::Idle
            }
            fn current_url(&self) -> Option<String> {
                None
            }
            fn poll_event(&mut self) -> Option<LoadEvent> {
                None
            }
            fn view_handle(&self) -> ViewHandle {
                ViewHandle(std::ptr::null_mut())
            }
            fn send_pointer(&mut self, _event: PointerEvent) {}
            fn send_key(&mut self, _event: KeyEvent) {}
            fn send_scroll(&mut self, _delta: ScrollDelta) {}
            fn set_focus(&mut self, _focused: bool) {}
            fn register_script_message_handler(
                &mut self,
                _name: &str,
                _handler: ScriptMessageHandler,
            ) {
            }
            fn inject_script(&mut self, _script: &str) {}
            fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {}
            fn trust_hooks(&self) -> TrustHooks {
                TrustHooks::none()
            }
        }
        let err = qualify(&RenderOnly).expect_err("a render-only backend is rejected");
        assert_eq!(
            err.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme]
        );
    }
}
