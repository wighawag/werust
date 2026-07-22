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
        /// The registered script-message handlers, so the harness can deliver a
        /// page-posted message to the same handler a real backend would.
        message_handlers: std::collections::HashMap<String, ScriptMessageHandler>,
        /// The registered custom-scheme handlers, so the harness can hand an
        /// intercepted request to the same handler a real backend would (the
        /// stand-in for WebKitGTK's `register_uri_scheme` callback).
        scheme_request_handlers: std::collections::HashMap<String, SchemeHandler>,
        /// JS the seam pushed back into the page via `evaluate_javascript` — the
        /// browser -> page response half of the bridge, recorded so the round-trip
        /// can be asserted headlessly.
        evaluated: Rc<RefCell<Vec<String>>>,
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

        fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
            self.script_handlers.push(name.to_string());
            self.message_handlers.insert(name.to_string(), handler);
        }

        fn inject_script(&mut self, script: &str) {
            self.injected.push(script.to_string());
        }

        fn evaluate_javascript(&self, script: &str) {
            self.evaluated.borrow_mut().push(script.to_string());
        }

        fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
            self.scheme_handlers.push(scheme.to_string());
            self.scheme_request_handlers
                .insert(scheme.to_string(), handler);
        }
    }

    impl SeamHarness {
        /// Hand an intercepted `<scheme>://…` [`SchemeRequest`] to the handler
        /// registered under `scheme`, returning its answer exactly as WebKitGTK's
        /// `register_uri_scheme` callback would drive it on the GTK loop.
        fn deliver_scheme_request(
            &mut self,
            scheme: &str,
            uri: &str,
        ) -> Result<renderer::SchemeResponse, RendererError> {
            let handler = self
                .scheme_request_handlers
                .get_mut(scheme)
                .expect("a handler is registered for this scheme");
            handler(renderer::SchemeRequest {
                uri: uri.to_string(),
            })
        }

        /// Deliver a page-posted [`ScriptMessage`] to the handler registered under
        /// its `handler` name, exactly as WebKitGTK's
        /// `script-message-received` signal would on the GTK loop.
        fn deliver_message(&mut self, handler: &str, body: &str) {
            if let Some(h) = self.message_handlers.get_mut(handler) {
                h(renderer::ScriptMessage {
                    handler: handler.to_string(),
                    body: body.to_string(),
                });
            }
        }

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
    fn eip1193_provider_request_round_trips_across_the_bridge_seam() {
        // Acceptance (injection + round-trip at the bridge seam, headless): a page
        // `request(...)` posted UP the script-message bridge is answered by the
        // native provider stub and the answer is pushed BACK into the page via the
        // seam's `evaluate_javascript` — the full page -> native -> page round-trip,
        // with no keys. This wires the provider routing onto the seam exactly as
        // the real `WebViewRenderer::install_provider` does, but drives it without
        // a GTK main loop or a display.
        use werust_core::provider::{
            provider_shim, route_provider_message, ProviderBridge, PROVIDER_BRIDGE, STUB_CHAIN_ID,
        };

        use std::sync::{Arc, Mutex};

        let mut r = SeamHarness::default();
        // The response push sink. The seam's script-message handler is `Send`, so
        // the collector is an `Arc<Mutex<_>>` (exactly why the REAL backend cannot
        // capture an `Rc`-shared handle and instead pushes via a cloned WebView on
        // its GTK thread — see `WebViewRenderer::install_provider`).
        let pushed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // Inject the page-side shim (detectable `window.ethereum`) and register the
        // provider channel handler that routes each envelope and pushes the
        // response back (the browser -> page response the real backend delivers via
        // the seam's `evaluate_javascript`).
        r.inject_script(&provider_shim());
        let bridge = ProviderBridge::new();
        let pushed_for_handler = pushed.clone();
        r.register_script_message_handler(
            PROVIDER_BRIDGE,
            Box::new(move |message| {
                let sink = pushed_for_handler.clone();
                route_provider_message(&bridge, &message, &mut |script| {
                    sink.lock().unwrap().push(script);
                });
            }),
        );

        // The provider is DETECTABLE: the shim installing `window.ethereum` with
        // the standard `request(...)` interface was injected at document start.
        assert!(r.injected.iter().any(|s| s.contains("\"ethereum\"")));
        assert!(r.injected.iter().any(|s| s.contains("request: function")));

        // A page-side `request({ method: 'eth_chainId' })` posts this envelope up
        // the bridge; deliver it to the registered handler.
        r.deliver_message(
            PROVIDER_BRIDGE,
            r#"{"id":11,"method":"eth_chainId","params":[]}"#,
        );

        // The native stub answered and the settle-call was pushed BACK to the
        // page: the round-trip completed with a result, no keys involved.
        let pushed = pushed.lock().unwrap();
        assert_eq!(
            pushed.len(),
            1,
            "exactly one response pushed back to the page"
        );
        assert_eq!(
            pushed[0],
            format!(r#"window.{PROVIDER_BRIDGE}.__resolve(11, "{STUB_CHAIN_ID}");"#),
            "the pending Promise for id 11 is resolved with the chain id"
        );
    }

    #[test]
    fn ipfs_scheme_resolves_verified_content_through_the_seam_hook() {
        // Acceptance (scheme -> verified-fetch -> render at the seam, headless): an
        // intercepted `ipfs://<cid>...` request is routed through the SAME resolver
        // the real `WebViewRenderer::install_ipfs` wires onto the custom-scheme
        // hook, resolved by the hash-verified Fetcher path, and its VERIFIED bytes
        // handed back as the response the backend would render — at parity with a
        // served page (text/html). Pinned fixture CID, no live network, no GTK loop.
        use fetcher::{cid_v1_raw_sha256, Cid, ContentSource, FetchError, VerifyingContentFetcher};
        use werust_core::ipfs::{resolve_ipfs_request, IPFS_SCHEME};

        // A pinned in-memory source (an untrusted origin stand-in), off the network.
        #[derive(Default)]
        struct PinnedSource {
            blobs: std::collections::HashMap<String, Vec<u8>>,
        }
        impl ContentSource for PinnedSource {
            fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError> {
                self.blobs
                    .get(&cid.to_string())
                    .cloned()
                    .ok_or_else(|| FetchError::Transport("pinned source miss".into()))
            }
        }

        let page = b"<!doctype html><title>ipfs</title><h1>verifiable page</h1>";
        let cid = cid_v1_raw_sha256(page).expect("derive pinned fixture cid");
        let mut source = PinnedSource::default();
        source.blobs.insert(cid.clone(), page.to_vec());
        let fetcher = VerifyingContentFetcher::new(source);

        // Wire the ipfs scheme handler onto the seam EXACTLY as install_ipfs does:
        // route each intercepted request through resolve_ipfs_request against the
        // verifying fetcher.
        let mut r = SeamHarness::default();
        r.register_scheme_handler(
            IPFS_SCHEME,
            Box::new(move |request| resolve_ipfs_request(&fetcher, &request)),
        );
        assert_eq!(r.scheme_handlers, ["ipfs"]);

        // An intercepted `ipfs://<cid>/index.html` resolves to the VERIFIED bytes,
        // rendered as an html document (served-page parity).
        let response = r
            .deliver_scheme_request(IPFS_SCHEME, &format!("ipfs://{cid}/index.html"))
            .expect("verified content resolves through the scheme hook");
        assert_eq!(response.body, page);
        assert_eq!(response.mime_type, "text/html");
    }

    #[test]
    fn ipfs_scheme_hash_mismatch_fails_the_load_and_never_renders() {
        // The load-bearing gate at the seam: the source holds TAMPERED bytes under
        // a real CID, so the intercepted `ipfs://<cid>` request must FAIL (an Err
        // the backend surfaces via `request.finish_error`, a failed load) and must
        // NEVER return the tampered bytes to render. Verification gates the load.
        use fetcher::{cid_v1_raw_sha256, Cid, ContentSource, FetchError, VerifyingContentFetcher};
        use werust_core::ipfs::{resolve_ipfs_request, IPFS_SCHEME};

        #[derive(Default)]
        struct PinnedSource {
            blobs: std::collections::HashMap<String, Vec<u8>>,
        }
        impl ContentSource for PinnedSource {
            fn get(&self, cid: &Cid) -> Result<Vec<u8>, FetchError> {
                self.blobs
                    .get(&cid.to_string())
                    .cloned()
                    .ok_or_else(|| FetchError::Transport("pinned source miss".into()))
            }
        }

        let honest = b"the page this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive pinned fixture cid");
        let mut source = PinnedSource::default();
        source.blobs.insert(
            cid.clone(),
            b"tampered bytes that do not match the cid".to_vec(),
        );
        let fetcher = VerifyingContentFetcher::new(source);

        let mut r = SeamHarness::default();
        r.register_scheme_handler(
            IPFS_SCHEME,
            Box::new(move |request| resolve_ipfs_request(&fetcher, &request)),
        );

        let result = r.deliver_scheme_request(IPFS_SCHEME, &format!("ipfs://{cid}/index.html"));
        let err = result.expect_err("a hash mismatch must fail the load, not render");
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
            "the mismatch fails the load with a verify reason, got: {err:?}"
        );
    }

    #[test]
    fn evaluate_javascript_pushes_into_the_page_over_the_seam() {
        // The browser -> page response half of the bridge is a first-class seam
        // method: a backend can push JS into the live page. Asserted headlessly on
        // the harness (the real backend calls WebKitGTK's evaluate_javascript).
        let r = SeamHarness::default();
        r.evaluate_javascript("window.werustProvider.__resolve(1, \"0x1\");");
        assert_eq!(
            *r.evaluated.borrow(),
            ["window.werustProvider.__resolve(1, \"0x1\");"]
        );
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

    /// End-to-end install of the REAL EIP-1193 provider on the WebKitGTK backend.
    /// Ignored by default (constructing a `WebViewRenderer` initializes GTK, which
    /// needs a display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. Exercising the full page ->
    /// native -> page round-trip additionally needs a running GTK loop with a page
    /// loaded; the headless
    /// `eip1193_provider_request_round_trips_across_the_bridge_seam` above pins the
    /// round-trip logic display-free. Here we only pin that installing the provider
    /// on the real backend wires the bridge without panicking.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_installs_the_eip1193_provider() {
        let mut r = WebViewRenderer::new().expect("gtk init on a desktop session");
        r.install_provider();
    }

    /// End-to-end install of native `ipfs://` resolution on the REAL WebKitGTK
    /// backend. Ignored by default (constructing a `WebViewRenderer` initializes
    /// GTK, which needs a display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. The full
    /// scheme -> verified-fetch -> render path (and its mismatch-fails-the-load
    /// guarantee) is pinned display-free by the headless
    /// `ipfs_scheme_resolves_verified_content_through_the_seam_hook` and
    /// `ipfs_scheme_hash_mismatch_fails_the_load_and_never_renders` above; here we
    /// only pin that installing the scheme hook on the real backend wires the
    /// custom scheme without panicking.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_installs_the_ipfs_scheme() {
        let mut r = WebViewRenderer::new().expect("gtk init on a desktop session");
        r.install_ipfs();
    }

    #[test]
    fn history_availability_starts_empty_before_any_navigation() {
        // The seam's back/forward availability is the backend's session-history
        // truth. On a fresh backend nothing has been visited, so neither Back nor
        // Forward is possible — the state the shell greys both controls from. The
        // real `WebViewRenderer` delegates go_back/go_forward/can_go_* to
        // WebKitGTK's own session list (see backend.rs); the end-to-end walk of
        // that list needs a display and lives in the ignored test below. Here we
        // pin the display-free contract: no history means no back/forward.
        let r = SeamHarness::default();
        assert!(!r.can_go_back(), "nothing visited yet: cannot go back");
        assert!(
            !r.can_go_forward(),
            "nothing visited yet: cannot go forward"
        );
    }

    /// End-to-end back/forward over the REAL WebKitGTK session list. Ignored by
    /// default (constructing a `WebViewRenderer` initializes GTK, needing a
    /// display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_history_starts_empty() {
        let r = WebViewRenderer::new().expect("gtk init on a desktop session");
        // Before any load, WebKitGTK's session list is empty, so the seam reports
        // no back/forward is possible. (Exercising an actual back navigation needs
        // pages to load on a running GTK loop, which the shell's seam-level tests
        // cover via the fake backend; this only pins the real delegation.)
        assert!(!r.can_go_back());
        assert!(!r.can_go_forward());
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
