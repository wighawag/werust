//! The toolkit-free load-lifecycle state machine every system-webview
//! [`Renderer`](renderer::Renderer) backend drives.
//!
//! [`LoadLifecycle`] owns [`LoadState`](renderer::LoadState), the current URL, the
//! pending [`LoadEvent`](renderer::LoadEvent) queue and the two-axis
//! [`TrustPosture`](renderer::TrustPosture) of the current load. It contains NO
//! toolkit type at all, which is exactly why it lives here rather than in a
//! backend crate: `navigate`/`reload`/`stop` and the platform webview's own load
//! signals (WebKitGTK's `load-changed`, WKWebView's `didCommit`/`didFinish`) all
//! drive the SAME state machine, so the seam contract is testable with no
//! display, no main loop and no SDK.
//!
//! It was extracted from `webview-renderer` (which depends on gtk4/webkit6
//! UNCONDITIONALLY and therefore cannot compile on macOS) when the macOS
//! WKWebView backend needed the same lifecycle: MOVED, never copied, so the two
//! desktop backends cannot drift in what a load state or a trust posture means.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use renderer::{LoadEvent, LoadState, TrustPosture};

/// The toolkit-free load-lifecycle state machine shared between a system-webview
/// backend and its webview's load signals.
///
/// This is the load-lifecycle surface of the seam, modelled explicitly so it can
/// be driven and asserted at the trait level without a main loop or a display.
/// `navigate` calls [`begin`](LoadLifecycle::begin); the webview's load signals
/// (WebKitGTK's `load-changed` / `load-failed`, WKWebView's `didCommit` /
/// `didFinish` / `didFail`) call [`commit`](LoadLifecycle::commit),
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

    /// Record a SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`
    /// client-side navigation): update the current URL and emit
    /// [`LoadEvent::UrlChanged`], WITHOUT touching the load state or the trust
    /// posture.
    ///
    /// This is DELIBERATELY not a lifecycle transition: a same-document nav is not
    /// a fresh load — the document (and its already-established
    /// content-verified/ENS posture) is unchanged, the SPA only rewrote the
    /// history URL. So unlike [`begin`](LoadLifecycle::begin) this does NOT reset
    /// `posture`/`ens_origin`/`mutable_name`, and unlike
    /// [`commit`](LoadLifecycle::commit)/[`finish`](LoadLifecycle::finish) it does
    /// NOT move the [`LoadState`]. It is a NO-OP when `url` already matches the
    /// current URL, so the webview's `notify::uri` firing for the load-lifecycle
    /// URL (a real load, not an SPA nav) does not emit a spurious `UrlChanged`.
    /// The browser follows the new URL (dropping a pinned name / re-deriving an
    /// ENS identity) from the emitted event.
    pub fn url_changed(&mut self, url: &str) {
        if self.url.as_deref() == Some(url) {
            return;
        }
        self.url = Some(url.to_string());
        self.events.push_back(LoadEvent::UrlChanged {
            url: url.to_string(),
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

/// A shared, interior-mutable [`LoadLifecycle`]: the way a system-webview backend
/// shares ONE lifecycle between the [`Renderer`](renderer::Renderer) trait methods
/// and the webview's signal closures (which fire on the platform's own main loop —
/// the GTK loop on desktop Linux, the AppKit run loop on macOS).
///
/// `Rc<RefCell<_>>` and therefore `!Send` ON PURPOSE: a system webview is a
/// single-main-thread object, so the lifecycle is only ever touched on that
/// thread. The off-thread `ipfs://` boundary in [`offthread`](crate::offthread)
/// is built around exactly that constraint — only a `Send` VALUE crosses to the
/// worker, and the lifecycle is mutated back on the marshalling thread.
pub type SharedLifecycle = Rc<RefCell<LoadLifecycle>>;

#[cfg(test)]
mod tests {
    use super::*;

    // These are the LoadLifecycle STATE-MACHINE tests, and they live here beside
    // the code they cover. They were written in `webview-renderer` (where this
    // state machine started) and stayed there when the machine itself MOVED, so
    // the shared crate shipped its central guarantee untested on any platform
    // that is not Linux: `cargo test -p webview-shared` on the macOS runner ran
    // the off-thread and URL-rule tests and nothing about the lifecycle at all.
    // Moved by task `macos-spike-doc-accuracy-and-harness-guard` so the crate's
    // guarantees travel WITH it, which is the whole point of a shared crate.
    // Nothing about them changed: they drive `LoadLifecycle` directly and never
    // touched GTK.

    #[test]
    fn a_same_document_url_change_emits_url_changed_without_a_load_transition() {
        // Acceptance (desktop SPA tracking, headless): a same-document URL change
        // (the WebKitGTK `notify::uri` / WKWebView `URL` KVO a backend observes for
        // an SPA `pushState`) updates the current URL and emits a DISTINCT
        // `LoadEvent::UrlChanged`, WITHOUT moving the load state or resetting the
        // trust posture — the document (and its established verified/ENS posture)
        // is unchanged. This pins the pure lifecycle behaviour that wiring drives,
        // display-free (the platform signal itself needs a display).
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

        // A URL notification that merely echoes the CURRENT URL emits nothing (a
        // real load's optimistic `begin` already set it), so no spurious event.
        life.url_changed("ipfs://bafyroot/portfolio");
        assert_eq!(life.poll(), None, "an unchanged URI emits no event");
    }

    #[test]
    fn an_ens_resolved_load_reports_the_name_via_trusted_rpc_posture_and_does_not_leak() {
        // Acceptance: the name-via-trusted-RPC posture tracks the ACTUAL load path
        // (a load whose CID came from an ENS resolution over the trusted RPC), set
        // via `mark_name_via_trusted_rpc` exactly as the front-door path calls it.
        // It is a distinct middle state (never "verified"), and it does NOT leak
        // onto a later plain served load: a fresh `begin` resets it.
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
}
