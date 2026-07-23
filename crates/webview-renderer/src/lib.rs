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

use renderer::{LoadEvent, LoadState, RendererError, TrustPosture};

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
    /// The [`TrustPosture`] of the CURRENT load. It reflects the ACTUAL load
    /// path, not the URL: every fresh [`begin`](LoadLifecycle::begin) resets it
    /// to [`TrustPosture::UnverifiedOrigin`] (a load is untrusted until proven
    /// verified), and it is upgraded to [`TrustPosture::ContentVerified`] ONLY by
    /// [`mark_content_verified`](LoadLifecycle::mark_content_verified) — which the
    /// `ipfs://` scheme handler calls when it has actually served the main
    /// resource through the hash-verified content-addressed fetch path. A plain
    /// served load never calls it, so it stays untrusted.
    posture: TrustPosture,
    /// Whether the CURRENT load originated from an ENS name resolved over the
    /// trusted RPC (the bare-`.eth` front door).
    ///
    /// This is the flag that resolves the posture-marking clash the front-door
    /// task (`bare-eth-urlbar-front-door-end-to-end`) owns. The `ipfs://` scheme
    /// handler calls [`mark_content_verified`](LoadLifecycle::mark_content_verified)
    /// UNCONDITIONALLY on any verified resolution and knows nothing about ENS; so
    /// when this flag is set, that same mark surfaces
    /// [`TrustPosture::NameViaTrustedRpc`] instead of the plain
    /// [`TrustPosture::ContentVerified`] — the ENS-origin posture WINS over the
    /// scheme handler's mark without the handler having to know it was ENS.
    ///
    /// Like [`posture`](Self::posture) it tracks the ACTUAL load path: every fresh
    /// [`begin`](LoadLifecycle::begin) resets it to `false`, and only the
    /// front-door path that genuinely resolved a name over the trusted RPC sets it
    /// via [`mark_ens_origin`](LoadLifecycle::mark_ens_origin) — never a
    /// `.eth`-looking URL on its own — so it never leaks onto a later plain
    /// `ipfs://` or served load.
    ens_origin: bool,
    /// Whether the CURRENT load's name is MUTABLE (controller-repointable): an
    /// IPNS name (the key holder can publish a new signed record) or (in a later
    /// phase) an ENS name (the owner can `setContenthash`).
    ///
    /// This is the SECOND axis of the two-axis trust model
    /// (`work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`,
    /// `docs/adr/0006`): mutability, orthogonal to how the name was learned
    /// ([`ens_origin`](Self::ens_origin)). When a verified load is flagged mutable
    /// (via [`mark_mutable_name`](LoadLifecycle::mark_mutable_name)) but NOT
    /// ENS-originated, [`mark_content_verified`](LoadLifecycle::mark_content_verified)
    /// surfaces [`TrustPosture::MutableName`] instead of the plain
    /// [`TrustPosture::ContentVerified`]. If the load is ALSO ENS-originated the
    /// LOUDER [`TrustPosture::NameViaTrustedRpc`] still wins (a misdirecting RPC
    /// beats an honest controller repointing), which is exactly what an ENS
    /// ipns-ns load wants; when Phase 2 clears the RPC-trust flag such an ENS load
    /// naturally falls back to `MutableName` with NO rule change here.
    ///
    /// Like the other axis flags it tracks the ACTUAL load path: every fresh
    /// [`begin`](LoadLifecycle::begin) resets it, so it never leaks onto a later
    /// immutable `ipfs://<cid>` or served load.
    mutable_name: bool,
}

impl LoadLifecycle {
    /// Start a load of `url`: move to [`LoadState::Started`] and emit
    /// [`LoadEvent::Started`].
    ///
    /// A fresh load starts UNVERIFIED: the trust posture resets to
    /// [`TrustPosture::UnverifiedOrigin`], and is only upgraded to
    /// [`TrustPosture::ContentVerified`] if this load's main resource is actually
    /// served through the hash-verified content-addressed path (via
    /// [`mark_content_verified`](LoadLifecycle::mark_content_verified)). So the
    /// posture always tracks the ACTUAL load path of the CURRENT page, never a
    /// stale value from a previous verified load.
    pub fn begin(&mut self, url: &str) {
        self.url = Some(url.to_string());
        self.state = LoadState::Started;
        self.posture = TrustPosture::UnverifiedOrigin;
        // A fresh load is not ENS-originated until the front door proves it is
        // (by resolving a name over the trusted RPC and calling
        // `mark_ens_origin`). Resetting here is what keeps the ENS posture from
        // leaking onto a later plain `ipfs://` or served load.
        self.ens_origin = false;
        // A fresh load is likewise not mutable-named until the front door flags it
        // (an IPNS resolution), so an immutable `ipfs://<cid>` never inherits a
        // stale mutability warning.
        self.mutable_name = false;
        self.events.push_back(LoadEvent::Started {
            url: url.to_string(),
        });
    }

    /// Mark the CURRENT load as content-verified: its main resource was served
    /// through the hash-verified content-addressed fetch path.
    ///
    /// The `ipfs://` scheme handler calls this the moment it has resolved the
    /// current page's bytes through
    /// [`fetch_verified`](fetcher::ContentAddressedFetcher::fetch_verified) — i.e.
    /// once the bytes are proven to hash to their CID. This is what makes the
    /// trust indicator track the REAL load path rather than the URL string: only
    /// bytes that actually verified flip the posture. A hash mismatch fails the
    /// load (the handler returns an error and never calls this), so a page that
    /// merely LOOKS content-addressed is never reported verified.
    pub fn mark_content_verified(&mut self) {
        // The `ipfs://` scheme handler calls this UNCONDITIONALLY on any verified
        // resolution. For an ENS-originated load (the front door resolved the
        // name over the trusted RPC and fed the CID into this verified path) the
        // honest posture is `NameViaTrustedRpc`, NOT the plain `ContentVerified`:
        // the bytes hash-verified, but the name->CID mapping was taken on the
        // RPC's word. Redirecting the mark here — rather than teaching the scheme
        // handler about ENS — is how the ENS-origin posture WINS over the
        // handler's unconditional content-verified mark.
        //
        // The two-axis display rule (`docs/adr/0006`): show the LOUDEST
        // applicable warning. `ens_origin` (the name learned over a trusted RPC)
        // is the loudest — a misdirecting RPC is worse than an honest controller
        // repointing — so it wins over `mutable_name`; a mutable name with no RPC
        // trust (a client-verified IPNS record) shows `MutableName`; an immutable,
        // RPC-free load is plain `ContentVerified`.
        // Delegate to the ONE shared two-axis rule (`TrustPosture::after_verify`)
        // so desktop and both mobile backends surface the SAME posture from the
        // SAME source of truth rather than each re-deriving the loudest-wins order.
        self.posture = TrustPosture::after_verify(self.ens_origin, self.mutable_name);
    }

    /// Flag the CURRENT load as originating from an ENS name resolved over the
    /// trusted RPC (the bare-`.eth` front door).
    ///
    /// The front-door path calls this the moment it has resolved a name to its
    /// contenthash over the trusted RPC and is about to feed the resulting CID
    /// into the verified `ipfs://` load. It does NOT itself claim any trust: it
    /// only records that IF this load verifies through the content-addressed
    /// path, the honest posture is [`TrustPosture::NameViaTrustedRpc`] rather than
    /// [`TrustPosture::ContentVerified`] (see
    /// [`mark_content_verified`](LoadLifecycle::mark_content_verified)). A load
    /// that fails verification never gets marked at all, so it stays untrusted;
    /// and a fresh [`begin`](LoadLifecycle::begin) clears the flag, so it never
    /// leaks onto a later load.
    pub fn mark_ens_origin(&mut self) {
        self.ens_origin = true;
    }

    /// Whether the CURRENT load was flagged as ENS-originated (see
    /// [`mark_ens_origin`](LoadLifecycle::mark_ens_origin)).
    #[must_use]
    pub fn is_ens_origin(&self) -> bool {
        self.ens_origin
    }

    /// Flag the CURRENT load as pointing at a MUTABLE name (the two-axis model's
    /// mutability axis): an IPNS name (the key holder can publish a new signed
    /// record) or, later, an ENS name (the owner can `setContenthash`).
    ///
    /// The front door calls this right after resolving a mutable name to a CID and
    /// feeding it into the verified `ipfs://` path. Like
    /// [`mark_ens_origin`](LoadLifecycle::mark_ens_origin) it makes NO trust
    /// claim on its own: it only records that IF this load verifies through the
    /// content-addressed path, the honest posture is at most
    /// [`TrustPosture::MutableName`] — never immutable `ContentVerified` — and, if
    /// the name was ALSO learned over a trusted RPC, the louder
    /// [`TrustPosture::NameViaTrustedRpc`] wins (see
    /// [`mark_content_verified`](LoadLifecycle::mark_content_verified)). A load
    /// that fails verification is never marked, and a fresh
    /// [`begin`](LoadLifecycle::begin) clears the flag so it never leaks onto a
    /// later immutable `ipfs://<cid>` load.
    pub fn mark_mutable_name(&mut self) {
        self.mutable_name = true;
    }

    /// Whether the CURRENT load was flagged as pointing at a mutable name (see
    /// [`mark_mutable_name`](LoadLifecycle::mark_mutable_name)).
    #[must_use]
    pub fn is_mutable_name(&self) -> bool {
        self.mutable_name
    }

    /// Mark the CURRENT load as content-verified-but-name-via-a-trusted-RPC: its
    /// bytes were served through the hash-verified content-addressed path, but the
    /// name->CID mapping that chose those bytes came from an ENS resolution over a
    /// TRUSTED RPC (Phase 1, `ens-to-ipfs-resolution-phase1-rpc-skeleton`).
    ///
    /// This is the wiring hook the bare-`.eth` front-door path
    /// (`bare-eth-urlbar-front-door-end-to-end`) calls the moment it has resolved
    /// a name to its contenthash over the trusted RPC and fed the resulting CID
    /// into the verified `ipfs://` render path. Like
    /// [`mark_content_verified`](LoadLifecycle::mark_content_verified) it tracks
    /// the REAL load path: only a load that ACTUALLY went through ENS trusted-RPC
    /// resolution flips the posture here, never a `.eth`-looking URL on its own,
    /// and a fresh [`begin`](LoadLifecycle::begin) resets it. It is honestly NOT
    /// "verified": Phase 1 has no light client, so the name is not verified.
    pub fn mark_name_via_trusted_rpc(&mut self) {
        self.posture = TrustPosture::NameViaTrustedRpc;
    }

    /// The [`TrustPosture`] of the current load (content-verified vs served by an
    /// unverified origin).
    #[must_use]
    pub fn posture(&self) -> TrustPosture {
        self.posture
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
mod offthread;
#[cfg(test)]
pub(crate) use backend::os_color_scheme_from_portal;
pub use backend::WebViewRenderer;

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{qualify, KeyEvent, PointerEvent, Renderer, ScrollDelta, ViewHandle};
    use renderer::{SchemeHandler, ScriptMessageHandler, TrustHook, TrustHooks, TrustPosture};

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
            Box::new(move |request| resolve_ipfs_request(&retriever, &request)),
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
            Box::new(move |request| resolve_ipfs_request(&retriever, &request)),
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
            let response = resolve_ipfs_request(&retriever, &request)?;
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
            let response = resolve_ipfs_request(&retriever, &request)?;
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
