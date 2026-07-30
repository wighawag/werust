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
//! * [`LoadLifecycle`] is a pure, toolkit-free state machine that owns
//!   [`LoadState`](renderer::LoadState), the current URL, and the pending
//!   [`LoadEvent`](renderer::LoadEvent) queue. `navigate`/`reload`/`stop` and the
//!   webview's load signals all drive it, and it is exercised directly by the
//!   seam-contract tests. It lives in the shared [`webview_shared`] crate (with
//!   [`validate_url`] and the off-thread `ipfs://` boundary
//!   [`webview_shared::offthread`]) and is re-exported here, because the macOS
//!   WKWebView backend drives the SAME state machine and this crate's
//!   unconditional gtk4/webkit6 dependency cannot host it.
//! * [`WebViewRenderer`] wires a real [`webkit6::WebView`] on top of that
//!   lifecycle: it connects the WebKitGTK load-lifecycle signals so they feed the
//!   [`LoadLifecycle`], forwards input, and exposes the live view handle. It is
//!   the piece that shows an actual page in a window on Linux.

// MOVED, not copied (task `macos-wkwebview-renderer-backend`): the toolkit-free
// lifecycle + URL rule + off-thread `ipfs://` boundary now live in
// `webview-shared` so `macos-renderer` builds on the very same code. Re-exported
// so this crate's public surface (and its doc links) are unchanged.
pub use webview_shared::{validate_url, LoadLifecycle, SharedLifecycle};

mod backend;
#[cfg(test)]
pub(crate) use backend::os_color_scheme_from_portal;
pub use backend::{developer_extras_enabled, WebViewRenderer};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use renderer::{qualify, KeyEvent, PointerEvent, Renderer, ScrollDelta, ViewHandle};
    use renderer::{LoadEvent, LoadState, RendererError, TrustPosture};
    use renderer::{SchemeHandler, ScriptMessageHandler, TrustHook, TrustHooks};

    /// A pinned, in-memory [`ContentRetriever`](fetcher::ContentRetriever) double
    /// (an untrusted-origin stand-in), off the live network, that verifies a
    /// single raw/leaf block against its CID. It holds bytes for a CID and
    /// RE-VERIFIES them against that CID before returning, so it can be pointed
    /// at honest content (stored under its real CID) or TAMPERED content to
    /// exercise the resolve-verified and mismatch-fails-the-load cases. The full
    /// multi-block CAR/DAG verify is covered in the `fetcher::retriever` tests;
    /// the seam wiring here pins a single raw block.
    #[derive(Default)]
    struct PinnedRawRetriever {
        blobs: std::collections::HashMap<String, Vec<u8>>,
    }

    impl PinnedRawRetriever {
        fn insert(&mut self, cid: &str, bytes: &[u8]) {
            self.blobs.insert(cid.to_string(), bytes.to_vec());
        }
    }

    impl fetcher::ContentRetriever for PinnedRawRetriever {
        fn retrieve(
            &self,
            cid: &str,
            _path: &str,
        ) -> Result<fetcher::RetrievedContent, fetcher::RetrieveError> {
            let bytes = self.blobs.get(cid).cloned().ok_or_else(|| {
                fetcher::RetrieveError::MissingBlock {
                    cid: cid.to_string(),
                }
            })?;
            let expected = fetcher::cid_v1_raw_sha256(&bytes).expect("derive cid for held bytes");
            if expected != cid {
                return Err(fetcher::RetrieveError::BlockHashMismatch {
                    cid: cid.to_string(),
                });
            }
            Ok(fetcher::RetrievedContent { bytes, codec: 0x55 })
        }
    }

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

        fn trust_hooks(&self) -> TrustHooks {
            // Mirror the REAL `WebViewRenderer`, which OPTS INTO both trust hooks
            // explicitly (see `backend.rs`). The seam default is now fail-closed
            // (`TrustHooks::none()`), so a backend that genuinely wires both hooks
            // must declare them; this harness stands in for that backend, so it
            // declares them too rather than silently disqualifying.
            TrustHooks::all()
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
    fn a_same_document_url_change_emits_url_changed_without_a_load_transition() {
        // Acceptance (desktop SPA tracking, headless): a same-document URL change
        // (the WebKitGTK `notify::uri` the backend observes for an SPA `pushState`)
        // updates the current URL and emits a DISTINCT `LoadEvent::UrlChanged`,
        // WITHOUT moving the load state or resetting the trust posture — the
        // document (and its established verified/ENS posture) is unchanged. This
        // pins the pure lifecycle behaviour the `connect_uri_notify` wiring drives,
        // display-free (the GTK signal itself needs a display).
        let mut life = LoadLifecycle::default();
        // A verified ENS load has settled on the root.
        life.begin("ipfs://bafyroot/");
        life.mark_ens_origin();
        life.mark_content_verified();
        life.commit("ipfs://bafyroot/");
        life.finish("ipfs://bafyroot/");
        let _ = life.poll(); // Started
        let _ = life.poll(); // Committed
        let _ = life.poll(); // Finished
        assert_eq!(life.state(), LoadState::Finished);
        assert_eq!(life.posture(), TrustPosture::NameViaTrustedRpc);

        // A SPA client-side nav to a sub-path of the SAME document: only a URL
        // change, no load. It emits `UrlChanged` and updates the current URL.
        life.url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(
            life.poll(),
            Some(LoadEvent::UrlChanged {
                url: "ipfs://bafyroot/portfolio".into()
            })
        );
        assert_eq!(
            life.current_url(),
            Some("ipfs://bafyroot/portfolio"),
            "the same-document URL change updates the reported URL"
        );
        // The load state and the posture are UNCHANGED: not a fresh load.
        assert_eq!(life.state(), LoadState::Finished);
        assert_eq!(
            life.posture(),
            TrustPosture::NameViaTrustedRpc,
            "a same-document nav within a verified site keeps its posture"
        );

        // A `notify::uri` that merely echoes the CURRENT URL emits nothing (a real
        // load's optimistic `begin` already set it), so no spurious event.
        life.url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(life.poll(), None, "an unchanged URI emits no event");
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
                Ok(renderer::SchemeResponse::ok(
                    "text/html",
                    format!("resolved {}", req.uri).into_bytes(),
                ))
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
        use fetcher::cid_v1_raw_sha256;
        use werust_core::ipfs::{resolve_ipfs_request, IPFS_SCHEME};

        let page = b"<!doctype html><title>ipfs</title><h1>verifiable page</h1>";
        let cid = cid_v1_raw_sha256(page).expect("derive pinned fixture cid");
        let mut retriever = PinnedRawRetriever::default();
        retriever.insert(&cid, page);

        // Wire the ipfs scheme handler onto the seam EXACTLY as install_ipfs does:
        // route each intercepted request through resolve_ipfs_request against the
        // verifying retriever.
        let mut r = SeamHarness::default();
        r.register_scheme_handler(
            IPFS_SCHEME,
            Box::new(move |request| {
                resolve_ipfs_request(
                    &retriever,
                    &request,
                    &werust_core::ipfs::RedirectSink::new(),
                )
            }),
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
        use fetcher::cid_v1_raw_sha256;
        use werust_core::ipfs::{resolve_ipfs_request, IPFS_SCHEME};

        let honest = b"the page this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive pinned fixture cid");
        let mut retriever = PinnedRawRetriever::default();
        // Hold TAMPERED bytes under the real CID: the retrieve will fail to verify.
        retriever.insert(&cid, b"tampered bytes that do not match the cid");

        let mut r = SeamHarness::default();
        r.register_scheme_handler(
            IPFS_SCHEME,
            Box::new(move |request| {
                resolve_ipfs_request(
                    &retriever,
                    &request,
                    &werust_core::ipfs::RedirectSink::new(),
                )
            }),
        );

        let result = r.deliver_scheme_request(IPFS_SCHEME, &format!("ipfs://{cid}/index.html"));
        let err = result.expect_err("a hash mismatch must fail the load, not render");
        assert!(
            matches!(&err, RendererError::Backend(msg) if msg.contains("mismatch")),
            "the mismatch fails the load with a verify reason, got: {err:?}"
        );
    }

    #[test]
    fn trust_posture_tracks_the_actual_load_path_not_the_url() {
        // Acceptance (the indicator is driven by the REAL load path, headless): a
        // plain served load reports the untrusted posture, and a load served
        // through the hash-verified `ipfs://` path reports content-verified — but
        // ONLY because the bytes actually verified, not because the URL is
        // `ipfs://`. This wires the posture-marking onto the lifecycle EXACTLY as
        // `WebViewRenderer::install_ipfs` does (the scheme handler marks the shared
        // lifecycle verified on a successful resolution), without a GTK loop.
        use fetcher::cid_v1_raw_sha256;
        use werust_core::ipfs::resolve_ipfs_request;

        let page = b"<!doctype html><title>ipfs</title><h1>verified</h1>";
        let cid = cid_v1_raw_sha256(page).expect("derive pinned fixture cid");
        let mut retriever = PinnedRawRetriever::default();
        retriever.insert(&cid, page);

        // The shared lifecycle the scheme handler marks, mirroring `install_ipfs`.
        let life: SharedLifecycle = Rc::new(RefCell::new(LoadLifecycle::default()));
        let life_for_handler = life.clone();
        // Mirror `install_ipfs`: on a verified resolution the handler marks the
        // shared lifecycle content-verified. A plain closure (not the seam's
        // `Send`-bounded `SchemeHandler`) because this stands in for the webview's
        // own GTK-thread scheme registration.
        let ipfs_handler = move |request: renderer::SchemeRequest| {
            let response = resolve_ipfs_request(
                &retriever,
                &request,
                &werust_core::ipfs::RedirectSink::new(),
            )?;
            life_for_handler.borrow_mut().mark_content_verified();
            Ok::<_, RendererError>(response)
        };

        // A plain served load: begin, no scheme handler ever runs. The posture is
        // the untrusted origin — the default, driven by the load path.
        life.borrow_mut().begin("https://example.com/");
        life.borrow_mut().commit("https://example.com/");
        life.borrow_mut().finish("https://example.com/");
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "a plain served load is not content-verified"
        );

        // A content-addressed load: begin resets the posture to untrusted, THEN the
        // ipfs scheme handler serves the main resource and (only) on a verified
        // success marks it content-verified.
        let uri = format!("ipfs://{cid}/index.html");
        life.borrow_mut().begin(&uri);
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "begin resets the posture: a load is untrusted until proven verified"
        );
        let response = ipfs_handler(renderer::SchemeRequest { uri: uri.clone() })
            .expect("verified content resolves");
        assert_eq!(response.body, page);
        life.borrow_mut().commit(&uri);
        life.borrow_mut().finish(&uri);
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::ContentVerified,
            "the verified content path flips the posture to content-verified"
        );
    }

    #[test]
    fn a_hash_mismatch_load_is_never_reported_content_verified() {
        // The load-bearing guard for the indicator: when the `ipfs://` handler
        // FAILS the load on a hash mismatch, it never marks the lifecycle, so the
        // posture stays untrusted — a page whose URL looks content-addressed but
        // did not actually verify is NEVER reported content-verified.
        use fetcher::cid_v1_raw_sha256;
        use werust_core::ipfs::resolve_ipfs_request;

        let honest = b"the page this cid actually names";
        let cid = cid_v1_raw_sha256(honest).expect("derive pinned fixture cid");
        let mut retriever = PinnedRawRetriever::default();
        // Hold TAMPERED bytes under the real CID: the retrieve will fail to verify.
        retriever.insert(&cid, b"tampered bytes that do not match");

        let life: SharedLifecycle = Rc::new(RefCell::new(LoadLifecycle::default()));
        let life_for_handler = life.clone();
        let ipfs_handler = move |request: renderer::SchemeRequest| {
            let response = resolve_ipfs_request(
                &retriever,
                &request,
                &werust_core::ipfs::RedirectSink::new(),
            )?;
            life_for_handler.borrow_mut().mark_content_verified();
            Ok::<_, RendererError>(response)
        };

        let uri = format!("ipfs://{cid}/index.html");
        life.borrow_mut().begin(&uri);
        let err = ipfs_handler(renderer::SchemeRequest { uri }).expect_err("a mismatch fails");
        assert!(matches!(err, RendererError::Backend(_)));
        // The load failed, so the posture was NEVER upgraded: not content-verified.
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "an unverified (mismatched) load is never reported content-verified"
        );
    }

    #[test]
    fn an_ens_resolved_load_reports_the_name_via_trusted_rpc_posture_and_does_not_leak() {
        // Acceptance: the name-via-trusted-RPC posture tracks the ACTUAL load path
        // (a load whose CID came from an ENS resolution over the trusted RPC), set
        // via `mark_name_via_trusted_rpc` exactly as the front-door path will call
        // it. It is a distinct middle state (never "verified"), and it does NOT
        // leak onto a later plain served load: a fresh `begin` resets it.
        let life: SharedLifecycle = Rc::new(RefCell::new(LoadLifecycle::default()));

        // A bare `.eth` load: begin resets to untrusted, THEN the front door
        // resolves the name over the trusted RPC, feeds the CID into the verified
        // ipfs path, and marks the load name-via-trusted-RPC.
        life.borrow_mut().begin("ronan.eth");
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "begin resets: a `.eth`-looking URL is not trusted until it actually resolves"
        );
        life.borrow_mut().mark_name_via_trusted_rpc();
        life.borrow_mut().commit("ronan.eth");
        life.borrow_mut().finish("ronan.eth");
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::NameViaTrustedRpc,
            "a real ENS trusted-RPC resolution surfaces the name-via-trusted-RPC posture"
        );
        // Honesty: this is NOT content-verified, so it is never labelled verified.
        assert!(!life.borrow().posture().is_content_verified());

        // A later plain served load: begin resets the posture; no hook runs. The
        // name-via-trusted-RPC posture does not leak onto it.
        life.borrow_mut().begin("https://example.com/");
        life.borrow_mut().commit("https://example.com/");
        life.borrow_mut().finish("https://example.com/");
        assert_eq!(
            life.borrow().posture(),
            TrustPosture::UnverifiedOrigin,
            "the name-via-trusted-RPC posture does not leak onto a later plain served load"
        );
    }

    #[test]
    fn the_ens_origin_flag_redirects_the_scheme_handlers_verified_mark_and_does_not_leak() {
        // The load-bearing posture-clash mechanism the front door owns: the
        // `ipfs://` scheme handler calls `mark_content_verified` UNCONDITIONALLY;
        // an ENS-originated load (flagged via `mark_ens_origin`, exactly as the
        // front door does after starting the `ipfs://<cid>` load) must surface
        // `NameViaTrustedRpc` from that SAME unconditional mark, while a plain
        // ipfs load surfaces plain `ContentVerified` — and neither leaks onto a
        // later load (a fresh `begin` clears the flag).
        let mut life = LoadLifecycle::default();

        // An ENS-originated verified load: begin, flag ENS, THEN the scheme
        // handler's unconditional verified mark redirects to the ENS posture.
        life.begin("ipfs://bafyenscid/index.html");
        assert!(!life.is_ens_origin(), "begin resets the flag");
        life.mark_ens_origin();
        assert!(life.is_ens_origin());
        life.mark_content_verified(); // the scheme handler's UNCONDITIONAL mark
        assert_eq!(
            life.posture(),
            TrustPosture::NameViaTrustedRpc,
            "the ENS-origin flag makes the scheme handler's mark surface the ENS posture"
        );
        assert!(
            !life.posture().is_content_verified(),
            "never labelled verified"
        );

        // A later PLAIN ipfs load: begin clears the ENS flag, so the SAME
        // unconditional verified mark surfaces plain content-verified — the ENS
        // posture does not leak.
        life.begin("ipfs://bafyplaincid/index.html");
        assert!(!life.is_ens_origin(), "a fresh begin clears the ENS flag");
        life.mark_content_verified();
        assert_eq!(
            life.posture(),
            TrustPosture::ContentVerified,
            "a plain ipfs load is plain content-verified, not the ENS posture"
        );

        // A later plain served load (no verified mark) is untrusted.
        life.begin("https://example.com/");
        assert_eq!(life.posture(), TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn an_ens_flagged_load_that_fails_verification_is_never_marked() {
        // Fail-closed at the lifecycle: an ENS-flagged load whose bytes never
        // verify (the scheme handler returns an error and never calls
        // `mark_content_verified`) stays untrusted — the ENS flag alone claims
        // NOTHING.
        let mut life = LoadLifecycle::default();
        life.begin("ipfs://bafyenscid/");
        life.mark_ens_origin();
        // No verified mark: the load failed verification.
        life.fail(
            "ipfs://bafyenscid/",
            "ipfs:// content-addressed load failed: hash mismatch",
        );
        assert_eq!(
            life.posture(),
            TrustPosture::UnverifiedOrigin,
            "an ENS-flagged load that never verified is never reported trusted"
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
        // The WebKitGTK backend declares BOTH trust hooks EXPLICITLY (it opts into
        // the qualifying set via `Renderer::trust_hooks`, exactly as the real
        // `WebViewRenderer` does now that the seam default is fail-closed — both
        // wire the same hooks and both declare `TrustHooks::all()`), so the
        // qualification gate accepts it. This runs headlessly: it exercises the
        // seam contract, not a GTK main loop.
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
        // `WebViewRenderer` must OPT INTO both trust hooks and never drop one. The
        // seam default is now FAIL-CLOSED (`TrustHooks::none()`), so the backend
        // qualifies ONLY because it explicitly declares `TrustHooks::all()` (see
        // `backend.rs`). This harness mirrors that explicit declaration, so pinning
        // the qualifying set it yields guards the real backend's capability
        // display-free. The display-bound end-to-end check lives in
        // `real_webview_backend_qualifies` below (ignored by default).
        let r = SeamHarness::default();
        assert_eq!(
            r.trust_hooks(),
            TrustHooks::all(),
            "the webview backend explicitly declares both trust hooks"
        );
        assert!(
            r.trust_hooks().is_qualifying(),
            "the explicitly-declared webview capability is qualifying"
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
    fn desktop_maps_the_xdg_portal_color_scheme_to_the_os_signal() {
        // Acceptance (desktop follows the OS, headless): the XDG desktop portal's
        // `org.freedesktop.appearance color-scheme` value (0 = no preference,
        // 1 = prefer dark, 2 = prefer light) maps to the shared `OsColorScheme`
        // the backend applies via `gtk-application-prefer-dark-theme`. This is the
        // pure decision half of `follow_os_color_scheme`, pinned display-free (the
        // GTK-apply half needs a display and lives in the ignored
        // `real_webview_follows_the_os_color_scheme` below).
        //
        // Reproduction context: on a dark-mode GNOME the portal returns 1 while a
        // plain GTK4 app's `gtk-application-prefer-dark-theme` defaults to FALSE,
        // so WebKitGTK reported LIGHT (the bug). Mapping 1 -> Dark -> prefer_dark()
        // is what makes werust follow the OS. See
        // `docs/spikes/webview-follow-os-color-scheme/DIAGNOSIS.md`.
        use renderer::OsColorScheme;
        assert_eq!(
            os_color_scheme_from_portal(1),
            OsColorScheme::Dark,
            "portal 1 = prefer dark -> follow the OS into dark"
        );
        assert!(os_color_scheme_from_portal(1).prefer_dark());
        assert_eq!(
            os_color_scheme_from_portal(2),
            OsColorScheme::Light,
            "portal 2 = prefer light -> keep light, never force dark"
        );
        assert!(!os_color_scheme_from_portal(2).prefer_dark());
        assert_eq!(
            os_color_scheme_from_portal(0),
            OsColorScheme::NoPreference,
            "portal 0 = no preference -> supply no dark preference (light CSS default)"
        );
        assert!(!os_color_scheme_from_portal(0).prefer_dark());
        // An unknown/future value is treated as "no preference" (never forced
        // dark): a value werust does not understand must not silently flip dark.
        assert_eq!(os_color_scheme_from_portal(99), OsColorScheme::NoPreference);
        assert!(!os_color_scheme_from_portal(99).prefer_dark());
    }

    /// End-to-end follow-the-OS on the REAL WebKitGTK backend. Ignored by default
    /// (constructing a `WebViewRenderer` initializes GTK, which needs a display,
    /// and reading the portal needs a session bus). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. The pure portal->signal
    /// mapping is pinned display-free by
    /// `desktop_maps_the_xdg_portal_color_scheme_to_the_os_signal` above; here we
    /// only pin that following the OS color scheme wires without panicking (it
    /// reads the portal and sets `gtk-application-prefer-dark-theme` to match).
    #[test]
    #[ignore = "needs a display + session bus: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_follows_the_os_color_scheme() {
        let r = WebViewRenderer::new().expect("gtk init on a desktop session");
        r.follow_os_color_scheme();
    }

    #[test]
    fn the_web_inspector_developer_extras_are_gated_on_a_debug_build() {
        // Acceptance (the desktop half of the gating decision, headless): the
        // WebKit Web Inspector's `enable-developer-extras` — which the F12
        // shortcut needs to open a real console REPL + network in-window — is
        // turned ON only in a debug build, so a RELEASE build
        // (`cargo build --release`, the shipped GoReleaser path) is NOT silently
        // inspectable. The gate keys off `debug_assertions`, the desktop analogue
        // of Android's `BuildConfig.DEBUG` / iOS's `#if DEBUG`. This test itself
        // runs under `cargo test` (a debug build), so the gate is ON here; the
        // invariant it pins is that the gate IS `debug_assertions`, not a hardcoded
        // `true` that would leave a release build inspectable.
        assert_eq!(
            super::developer_extras_enabled(),
            cfg!(debug_assertions),
            "developer-extras must follow the debug/release build gate, never be unconditionally on"
        );
        #[cfg(debug_assertions)]
        assert!(
            super::developer_extras_enabled(),
            "a debug build enables the web inspector's developer-extras"
        );
        #[cfg(not(debug_assertions))]
        assert!(
            !super::developer_extras_enabled(),
            "a release build does not silently enable the web inspector"
        );
    }

    /// End-to-end open of the REAL WebKitGTK Web Inspector on the backend. Ignored
    /// by default (constructing a `WebViewRenderer` initializes GTK, needing a
    /// display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. The gate is pinned
    /// display-free by `the_web_inspector_developer_extras_are_gated_on_a_debug_build`
    /// above; here we only pin that showing the inspector on the real backend does
    /// not panic (a no-op in a release build where developer-extras is off).
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_shows_the_web_inspector() {
        let r = WebViewRenderer::new().expect("gtk init on a desktop session");
        r.show_inspector();
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
    fn a_new_window_request_navigates_the_existing_view_in_place_no_second_view() {
        // Acceptance (the in-place decision at the layer it lives, headless,
        // `docs/adr/0010`): a new-window / `create` request (a `_blank` link or
        // `window.open(url)`) is routed into the CURRENT view via the SAME
        // `navigate` path a normal in-view navigation takes — NOT a second view,
        // and NOT dropped (field finding C). This models the desktop `create`
        // handler's body without a GTK loop: the real handler
        // (`WebViewRenderer::install_new_window_in_place`, backend.rs) reads the
        // navigation action's target URI, applies the shared
        // `renderer::new_window_action` rule, and on `NavigateInPlace` calls
        // `self.view.load_uri` (the same load `navigate` drives) and returns NULL
        // so WebKitGTK spawns no new WebView. (Returning the EXISTING view instead
        // is what ABORTED the process in v0.2.5 — task
        // `fix-desktop-create-signal-crash-on-blank-links`; the live guard is the
        // ignored `real_webview_new_window_requests_load_in_place_without_aborting`
        // below.)
        use renderer::{new_window_action, NewWindowAction};

        let mut r = SeamHarness::default();
        // A page is showing; the user clicks a `target="_blank"` link to another
        // page. WebKitGTK's `create` fires with that target URI.
        r.navigate("https://example.com/").unwrap();
        r.drive_to_finished();
        while r.poll_event().is_some() {}

        // The `create` handler's body: resolve the request, then load in place.
        let target = "https://example.com/opened-in-blank";
        match new_window_action(Some(target)) {
            NewWindowAction::NavigateInPlace { url } => {
                // Routed through the NORMAL navigate path (which validates the URL
                // and, for `ipfs://`, would run the hash-verified scheme handler),
                // so trust is preserved — no bypass via the new-window hook.
                r.navigate(&url)
                    .expect("the `_blank` target loads in place");
            }
            NewWindowAction::Ignore => panic!("a real target must navigate in place"),
        }

        // The EXISTING view now shows the `_blank` target — it was NOT dropped, and
        // there is exactly ONE view (the seam owns a single lifecycle; no second
        // view was created).
        assert_eq!(
            r.current_url().as_deref(),
            Some(target),
            "the `_blank` target loaded in the current view, not a new window"
        );
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Started {
                url: target.to_string()
            }),
            "the in-place load starts a normal navigation lifecycle"
        );
    }

    /// End-to-end wiring of the REAL WebKitGTK `create` (new-window) hook on the
    /// backend. Ignored by default (constructing a `WebViewRenderer` initializes
    /// GTK, which needs a display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. The in-place routing logic
    /// is pinned display-free by
    /// `a_new_window_request_navigates_the_existing_view_in_place_no_second_view`
    /// above; here we only pin that installing the new-window hook on the real
    /// backend wires the `create` signal without panicking.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_installs_the_new_window_in_place_hook() {
        let mut r = WebViewRenderer::new().expect("gtk init on a desktop session");
        r.install_new_window_in_place();
    }

    /// Turn the GTK main loop until `settled` holds or `timeout_ms` elapses,
    /// returning whether it settled. The live WebKitGTK tests below need a running
    /// loop (loads, signals, and JS all complete on it) but must never hang the
    /// test binary, so every wait is bounded.
    #[cfg(test)]
    fn pump_until(settled: impl Fn() -> bool, timeout_ms: u64) -> bool {
        let ctx = gtk4::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            while ctx.iteration(false) {}
            if settled() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return settled();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// LIVE reproduction of the v0.2.5 desktop CRASH and its fix (task
    /// `fix-desktop-create-signal-crash-on-blank-links`): a real `target="_blank"`
    /// link click AND a real `window.open(url)` call, driven through a REAL
    /// WebKitGTK view on a running GTK loop, must load IN THE CURRENT view and
    /// must NOT abort the process.
    ///
    /// This is the only automatable guard for the crash: the bug was a WebKitGTK
    /// SIGABRT (`std::optional<WebCore::WindowFeatures>` `_M_is_engaged()` failed)
    /// raised inside the `create` signal emission when the handler returned the
    /// EXISTING view instead of a NEW view or NULL, so it is invisible to any
    /// display-free test — and it kills the test binary rather than failing an
    /// assertion, which is exactly what makes it a usable red/green signal here.
    ///
    /// Ignored by default (needs a display; a `WebViewRenderer` initializes GTK).
    /// Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored --test-threads=1`. Both
    /// triggers share ONE renderer because GTK/WebKit want a single view driven on
    /// one loop. The fixture pages are served off an in-memory custom scheme, so
    /// the test never touches the network.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_new_window_requests_load_in_place_without_aborting() {
        use webkit6::prelude::WebViewExt;

        const SOURCE: &str = "blanktest://source/";
        const VIA_BLANK: &str = "blanktest://opened-in-blank/";
        const VIA_WINDOW_OPEN: &str = "blanktest://opened-via-window-open/";

        let mut r = WebViewRenderer::new().expect("gtk init on a desktop session");
        // Serve the fixture pages from memory: the source page carries BOTH
        // triggers (a `target="_blank"` link and a `window.open(url)` button), and
        // each target is a distinct page so the assertion below can tell which
        // trigger navigated the view.
        r.register_scheme_handler(
            "blanktest",
            Box::new(|req: renderer::SchemeRequest| {
                let body = if req.uri.starts_with(SOURCE) {
                    format!(
                        "<!doctype html><html><body>\
                         <a id=\"blank\" href=\"{VIA_BLANK}\" target=\"_blank\">blank</a>\
                         <button id=\"popup\" onclick=\"window.open('{VIA_WINDOW_OPEN}')\">popup</button>\
                         </body></html>"
                    )
                } else {
                    format!("<!doctype html><html><body><p>{}</p></body></html>", req.uri)
                };
                Ok(renderer::SchemeResponse::ok("text/html", body.into_bytes()))
            }),
        );
        r.install_new_window_in_place();

        let view = r.web_view().clone();

        // Drive each trigger from the source page and assert the SAME view ended up
        // on the target: the new-window request navigated in place (no second
        // window, no dropped link) and, crucially, the process is still alive —
        // before the fix the `create` emission aborted here.
        for (trigger, expected) in [
            ("document.getElementById('blank').click()", VIA_BLANK),
            ("document.getElementById('popup').click()", VIA_WINDOW_OPEN),
        ] {
            r.navigate(SOURCE).expect("the fixture source page loads");
            let on_source = pump_until(
                || view.uri().as_deref() == Some(SOURCE) && !view.is_loading(),
                10_000,
            );
            assert!(
                on_source,
                "the fixture source page settled before {trigger}"
            );

            r.evaluate_javascript(trigger);
            let navigated = pump_until(|| view.uri().as_deref() == Some(expected), 10_000);
            assert!(
                navigated,
                "`{trigger}` navigated the EXISTING view to {expected} \
                 (in place, not dropped and not a second window); saw {:?}",
                view.uri()
            );
            // The shell's URL bar follows the new URL because the hook began the
            // lifecycle on the same URL it loaded.
            assert_eq!(
                r.current_url().as_deref(),
                Some(expected),
                "the seam's current URL follows the in-place `{trigger}` load"
            );
        }
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

    // -- desktop console + network CAPTURE (task
    //    `debug-console-network-capture-per-platform`) --------------------------

    #[test]
    fn a_desktop_resource_load_maps_onto_a_network_entry_with_its_response_facts() {
        // Acceptance (desktop network capture, headless): the resource-load signals
        // report a request's method/url at START and its response at FINISH; the
        // capture point folds them into ONE core `NetworkEntry`. This pins that
        // mapping display-free — the live signal wiring is
        // `WebViewRenderer::install_debug_capture` (backend.rs), whose real-webview
        // half is the ignored `real_webview_installs_the_debug_capture` below plus
        // the recorded manual steps.
        let entry = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "https://cdn.example/app.js",
            status: Some(200),
            mime: "application/javascript",
            size: Some(4096),
            finished_ok: true,
            load_posture: None,
            timestamp: 1_700_000_000_000,
            duration: 17,
        });
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.url, "https://cdn.example/app.js");
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.mime, "application/javascript");
        assert_eq!(entry.size, Some(4096));
        assert_eq!(entry.scheme, "https");
        assert_eq!(entry.duration, Some(17));
        assert_eq!(
            entry.trust,
            TrustPosture::UnverifiedOrigin,
            "an https subresource is never content-verified"
        );
    }

    #[test]
    fn desktop_capture_reports_the_honest_per_request_posture_never_the_url_alone() {
        // ADR-0006 per-request: an `ipfs://` sub-resource that FINISHED came back
        // through the hash-verified scheme handler, so it is content-verified; one
        // that FAILED (a hash mismatch fails the request) proved nothing.
        let verified = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "ipfs://bafy/pic.png",
            status: Some(200),
            mime: "image/png",
            size: Some(9),
            finished_ok: true,
            load_posture: None,
            timestamp: 0,
            duration: 0,
        });
        assert_eq!(verified.trust, TrustPosture::ContentVerified);

        let failed = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "ipfs://bafy/pic.png",
            status: None,
            mime: "",
            size: None,
            finished_ok: false,
            load_posture: None,
            timestamp: 0,
            duration: 0,
        });
        assert_eq!(
            failed.trust,
            TrustPosture::UnverifiedOrigin,
            "a failed ipfs:// request claims nothing"
        );
        assert_eq!(
            failed.status, None,
            "no response means no fabricated status"
        );
        assert_eq!(failed.size, None);
    }

    #[test]
    fn the_desktop_main_document_row_takes_the_loads_own_two_axis_posture() {
        // The store's DECISIONS.md Decision 4, honoured here: on an ENS-named page
        // the chrome trust indicator shows `name-via-trusted-rpc`, so the Network
        // tab's MAIN-DOCUMENT row must show the same thing rather than the plain
        // per-request `content-verified` — the two surfaces cannot disagree on the
        // same screen. Sub-resources keep their own honest per-request posture (the
        // test above).
        let main = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "ipfs://bafy/index.html",
            status: Some(200),
            mime: "text/html",
            size: Some(120),
            finished_ok: true,
            load_posture: Some(TrustPosture::NameViaTrustedRpc),
            timestamp: 0,
            duration: 0,
        });
        assert_eq!(main.trust, TrustPosture::NameViaTrustedRpc);

        let mutable = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "ipfs://bafy/index.html",
            status: Some(200),
            mime: "text/html",
            size: None,
            finished_ok: true,
            load_posture: Some(TrustPosture::MutableName),
            timestamp: 0,
            duration: 0,
        });
        assert_eq!(mutable.trust, TrustPosture::MutableName);
    }

    #[test]
    fn a_failed_desktop_resource_pushes_exactly_one_row_and_it_is_never_verified() {
        // WebKit emits a failed resource's `failed` signal AND THEN its `finished`
        // signal (`webkitWebResourceFailed` ends by calling
        // `webkitWebResourceFinished`). Pushing from both recorded every failed
        // load TWICE, and the `finished` row passed `finished_ok = true` — which
        // stamped a FAILED, possibly hash-MISMATCHED `ipfs://` subresource
        // `content-verified` in the very surface whose job is trust honesty.
        //
        // So `failed` must only FLAG, and `finished` must be the single push that
        // reads the flag. Pinned on the source because the double-push lives in the
        // signal wiring, which needs a display to run.
        let backend = include_str!("backend.rs");
        let failed_handler = backend
            .split_once("resource.connect_failed(")
            .expect("the resource-load capture connects a failed handler")
            .1;
        let failed_body = failed_handler
            .split_once("resource.connect_finished(")
            .expect("the failed handler precedes the single finished push")
            .0;
        assert!(
            !failed_body.contains(".record("),
            "connect_failed must NEVER push a row: WebKit emits finished for a \
             failed resource too, so the request would be recorded twice"
        );
        assert!(
            failed_body.contains("failed.set(true)"),
            "connect_failed FLAGS the failure for the single finished push to read"
        );
        assert_eq!(
            backend.matches(".record(").count(),
            1,
            "exactly ONE push site for a resource: the finished handler"
        );
        // And the honest outcome that single push then reports for a failed
        // resource is UNVERIFIED, whatever its scheme looked like.
        let entry = crate::backend::resource_network_entry(&crate::backend::ResourceLoadFacts {
            method: "GET",
            url: "ipfs://bafy/tampered.png",
            status: None,
            mime: "",
            size: None,
            finished_ok: false,
            load_posture: None,
            timestamp: 0,
            duration: 0,
        });
        assert_eq!(entry.trust, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn the_desktop_main_frame_check_is_the_shared_core_predicate_not_a_local_compare() {
        // ONE main-frame concept in the codebase. The desktop capture reuses the
        // core's `RedirectSink::is_main_frame` (driven by the top-level URL the
        // shell reports via `note_navigation`, normalized through `frame_key`)
        // rather than comparing URL strings itself. The naive compares are all
        // subtly wrong: the lifecycle's current URL misses a redirected main
        // document and the WebKit authority-less `ipfs:///<cid>` form, and the
        // chrome's DISPLAYED url is the pinned ENS name on exactly the page the
        // reconciliation exists for.
        let backend = include_str!("backend.rs");
        assert!(
            backend.contains("redirects\n            .is_main_frame(&self.url)")
                || backend.contains("redirects.is_main_frame(&self.url)"),
            "the desktop capture asks the SHARED core main-frame predicate"
        );
        assert!(
            !backend.contains("life.current_url() == Some(self.url"),
            "no local URL compare stands in for the shared predicate"
        );
        // And the predicate itself survives the forms desktop actually sees.
        let sink = werust_core::ipfs::RedirectSink::new();
        sink.note_navigation("ipfs://bafypage/index.html");
        assert!(
            sink.is_main_frame("ipfs:///bafypage/index.html"),
            "the WebKit authority-less form is the SAME document"
        );
        assert!(!sink.is_main_frame("ipfs://bafypage/app.css"));
    }

    #[test]
    fn desktop_console_capture_uses_the_one_shared_shim_and_its_own_channel() {
        // Desktop and iOS have no native console callback, so both inject the SAME
        // core shim over the SAME dedicated capture channel (never the EIP-1193
        // provider's trust channel). Pinning the source here is what stops the two
        // platforms drifting into two copies.
        let backend = include_str!("backend.rs");
        assert!(
            backend.contains("console_shim()"),
            "desktop injects the SHARED core console shim, not a local copy"
        );
        assert!(
            backend.contains("CAPTURE_BRIDGE"),
            "desktop registers the dedicated capture channel"
        );
        assert!(
            backend.contains("route_capture_message"),
            "desktop routes through the ONE shared parse+push"
        );
        assert!(
            !backend.contains("network_shim()"),
            "desktop does NOT inject the page-side fetch/XHR shim: its resource-load \
             signals already see every resource, so it would double-record a subset"
        );
    }

    /// End-to-end install of the console + network capture points on the REAL
    /// WebKitGTK backend. Ignored by default (constructing a `WebViewRenderer`
    /// initializes GTK, which needs a display). Run on a desktop session with
    /// `cargo test -p webview-renderer -- --ignored`. The MAPPING is pinned
    /// display-free by the tests above; here we only pin that installing the hooks
    /// on the real backend wires the signals without panicking, and that capture
    /// leaves the backend's trust posture untouched (it is READ-ONLY observation).
    /// The live end-to-end capture carries recorded manual steps at
    /// `docs/spikes/debug-console-network-capture-per-platform/README.md`.
    #[test]
    #[ignore = "needs a display: constructs a real WebViewRenderer (GTK init)"]
    fn real_webview_installs_the_debug_capture() {
        let mut r = WebViewRenderer::new().expect("gtk init on a desktop session");
        let capture = werust_core::debug::DebugCapture::new();
        let before = r.trust_posture();
        r.install_debug_capture(capture.clone(), werust_core::ipfs::RedirectSink::new());
        assert_eq!(
            r.trust_posture(),
            before,
            "capture is READ-ONLY observation: it does not touch the trust posture"
        );
        assert!(
            capture.console().is_empty(),
            "nothing captured before a load"
        );
        assert!(capture.network().is_empty());
    }
}
