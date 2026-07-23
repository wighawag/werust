//! The werust **core**: the browser product shell, driven entirely through the
//! [`Renderer`] seam.
//!
//! This crate is "the Rust core" `CONTEXT.md` names: the browsing *logic* — the
//! URL bar, the back/forward/reload/stop controls, and the chrome that reflects
//! load state — behind the seams, WITHOUT any OS toolkit. Every OS edge is a thin
//! view over this SAME core: the desktop GTK window (`werust` binary) renders
//! [`ChromeState`] into GTK widgets and forwards actions to [`BrowserShell`]; the
//! Android Kotlin `Activity` and the iOS Swift shell drive the exact same shell
//! over an FFI surface. Keeping the logic toolkit-free is what lets the shell↔seam
//! wiring be tested at the seam boundary (a `dyn Renderer`), not against any GUI
//! internals — exactly the boundary `CONTEXT.md` and the mobile tasks call for.
//!
//! All page navigation goes THROUGH the seam: [`navigate`](BrowserShell::navigate),
//! [`go_back`](BrowserShell::go_back), [`go_forward`](BrowserShell::go_forward),
//! [`reload`](BrowserShell::reload), and [`stop`](BrowserShell::stop) call the
//! matching [`Renderer`] methods, and [`pump`](BrowserShell::pump) drains the
//! seam's [`LoadEvent`]s to refresh the chrome. The shell never reaches past the
//! seam into the webview: page *interaction* (scroll/click/focus/type) is served
//! by embedding the live [`ViewHandle`](renderer::ViewHandle) widget and giving
//! it focus (the webview's `send_*` methods are deliberate no-ops — see the
//! forward-pointer in the task), so this module wires navigation + chrome and
//! leaves raw input to the embedded widget.

use renderer::{LoadEvent, LoadState, Renderer, RendererError, TrustPosture};

use fetcher::HttpFetcher;

use crate::contenthash::DecodedContenthash;
use crate::ethereum::{EthereumProvider, RpcProvider};
use crate::ipns::{GatewayIpnsRecordSource, IpnsRecordSource};

pub mod contenthash;
pub mod ens;
pub mod ethereum;
pub mod ipfs;
pub mod ipns;
pub mod provider;
pub mod retrieval;

/// The URL-bar suffix that marks a bare entry as an ENS name to resolve: a
/// `.eth` TLD.
///
/// The front door recognises a `*.eth` URL-bar entry (like Brave/Opera) as an
/// ENS name; see [`eth_name_from_entry`]. Phase 1 supports only `.eth`
/// (`ens-to-ipfs-resolution-phase1-rpc-skeleton`); other ENS TLDs are out of
/// scope.
const ETH_NAME_SUFFIX: &str = ".eth";

/// Recognise a bare `.eth` URL-bar entry as an ENS name, returning the name to
/// resolve (with any single trailing `/` stripped), or [`None`] if the entry is
/// not a bare `.eth` name.
///
/// The settled `.eth`-input rule (spec Settled decisions,
/// `ens-to-ipfs-resolution-phase1-rpc-skeleton`): treat a `*.eth` URL-bar entry
/// on Enter (or a trailing `/`) as an ENS name — do NOT aggressively auto-resolve
/// anything merely name-ish. Concretely a bare `.eth` entry is one that:
///
/// * carries NO URL scheme (no `://`): a `https://…`, `ipfs://…`, or any other
///   explicit scheme is taken literally and never treated as a name (so this is
///   ONLY the scheme-less front door Brave/Opera expose);
/// * ends in `.eth` (case-insensitively), after removing at most one trailing
///   `/` (the "or a trailing `/`" half of the rule);
/// * has a non-empty label before that `.eth` (so a bare `".eth"` or `"/"` is not
///   a name), and no `/` inside the remaining name (a path like `ronan.eth/x` is
///   NOT treated as a bare name here — Phase 1 resolves the name to a CID, it
///   does not select a sub-path via ENS).
///
/// Normalisation/validation of the label itself is left to the resolver
/// ([`ens::namehash`](crate::ens::namehash) via `ens-normalize`), so this is only
/// the cheap URL-bar recognition, not an ENS-name validity check.
fn eth_name_from_entry(entry: &str) -> Option<&str> {
    // An explicit scheme is taken literally: only the scheme-less front door is a
    // name. This is what stops `ipfs://…` or `https://….eth` from being hijacked.
    if entry.contains("://") {
        return None;
    }
    // "On Enter or a trailing `/`": accept one optional trailing slash.
    let name = entry.strip_suffix('/').unwrap_or(entry);
    // A bare name has no path separators left (a `ronan.eth/page` entry is not a
    // bare name in Phase 1) and ends in the `.eth` TLD (case-insensitively).
    if name.contains('/') || !name.to_ascii_lowercase().ends_with(ETH_NAME_SUFFIX) {
        return None;
    }
    // There must be a non-empty label before `.eth` (reject a bare `".eth"`).
    if name.len() <= ETH_NAME_SUFFIX.len() {
        return None;
    }
    Some(name)
}

/// The chrome state the shell reflects: everything the window must draw ABOUT the
/// current page, distinct from the page content itself.
///
/// This is the observable output of driving the seam: after any action plus a
/// [`pump`](BrowserShell::pump), the window paints its URL bar, its
/// back/forward/reload/stop controls, and its load indicator from this struct.
/// It is a plain value so a test can assert the chrome without a display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChromeState {
    /// The text shown in the URL bar. Tracks the committed/in-flight URL as the
    /// load lifecycle progresses (so the bar follows redirects and history
    /// navigations), not just what the user last typed.
    pub url_text: String,
    /// The current load-lifecycle state, driving the loading/idle indicator
    /// (e.g. a spinner while [`LoadState::is_loading`], stop enabled).
    pub load_state: LoadState,
    /// Whether the Back control is enabled (a back navigation is possible).
    pub can_go_back: bool,
    /// Whether the Forward control is enabled.
    pub can_go_forward: bool,
    /// A human-readable failure surfaced to the user when the last load failed,
    /// cleared when a new load starts. `None` when nothing has failed.
    pub last_error: Option<String>,
    /// The [`TrustPosture`] of the current page, driving the chrome's trust
    /// indicator: content-verified vs served by an unverified origin
    /// (`docs/adr/0001`: the trust posture is a product surface). Read straight
    /// from the seam's [`Renderer::trust_posture`], so it tracks the ACTUAL load
    /// path (a page whose bytes came back through the hash-verified
    /// content-addressed path), not the URL string.
    pub trust_posture: TrustPosture,
}

impl ChromeState {
    /// Whether the Stop control should be active (a load is in flight) versus the
    /// Reload control (a settled page). The window swaps/enables the two from this.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.load_state.is_loading()
    }

    /// Whether the current page was content-verified (its bytes hash-checked on
    /// the content-addressed path), as opposed to merely served by an unverified
    /// origin. The window paints its trust indicator from this.
    #[must_use]
    pub fn is_content_verified(&self) -> bool {
        self.trust_posture.is_content_verified()
    }

    /// Whether the current page's bytes were content-verified but its name->CID
    /// mapping came from a TRUSTED RPC (an ENS-resolved Phase-1 page). A distinct
    /// middle state the window paints as its own trust indicator: never "verified"
    /// (Phase 1 has no light client), never merely "served" (the bytes verified).
    #[must_use]
    pub fn is_name_via_trusted_rpc(&self) -> bool {
        self.trust_posture.is_name_via_trusted_rpc()
    }

    /// Whether the current page's bytes were content-verified but its name is
    /// MUTABLE (controller-repointable): a client-verified IPNS load (or, once
    /// Phase 2 clears the RPC-trust warning, an ENS load). A distinct state the
    /// window paints as its own trust indicator: never "verified" (only a direct
    /// `ipfs://<cid>` is immutable), never merely "served" (the bytes verified),
    /// and quieter than [`is_name_via_trusted_rpc`](ChromeState::is_name_via_trusted_rpc)
    /// (a misdirecting RPC is the louder warning).
    #[must_use]
    pub fn is_mutable_name(&self) -> bool {
        self.trust_posture.is_mutable_name()
    }
}

/// The browser shell: the seam-driven logic behind the window.
///
/// Holds the rendering backend as a `dyn Renderer` (the seam) and the derived
/// [`ChromeState`]. Every user action is a method that drives the seam; the
/// window calls [`pump`](BrowserShell::pump) on the main loop to fold the seam's
/// [`LoadEvent`]s into the chrome. It is generic-free (`Box<dyn Renderer>`) so
/// the SAME shell drives the webview today and a native backend later.
pub struct BrowserShell {
    renderer: Box<dyn Renderer>,
    chrome: ChromeState,
    /// The [`EthereumProvider`](crate::ethereum::EthereumProvider) the front door
    /// resolves a bare `.eth` name through (namehash -> registry -> resolver ->
    /// contenthash). It is `Box<dyn>` so the SAME shell drives the Phase-1 trusted
    /// [`RpcProvider`](crate::ethereum::RpcProvider) today and a Phase-2 trustless
    /// light-client backend later, unchanged.
    provider: Box<dyn EthereumProvider>,
    /// The [`IpnsRecordSource`](crate::ipns::IpnsRecordSource) the front door
    /// resolves a MUTABLE `ipns-ns` name through: fetch the signed record from an
    /// untrusted trustless gateway, then VERIFY it client-side against the key
    /// (`crate::ipns::resolve_ipns_name`). `Box<dyn>` so the SAME shell drives the
    /// default trustless-gateway source today and a delegated-routing /
    /// embedded-p2p source later, unchanged — mirroring `provider`.
    ipns_source: Box<dyn IpnsRecordSource>,
    /// The address-bar text to DISPLAY in place of the backend's underlying load
    /// URL, when they differ.
    ///
    /// The ENS front door loads the resolved `ipfs://<cid>` through the seam but
    /// must keep the `.eth` NAME the user typed in the bar (the identity they care
    /// about) — no `https://` rewrite, no gateway redirect. So an ENS load sets
    /// this to the `.eth` name, and [`refresh_chrome`](BrowserShell::refresh_chrome)
    /// / [`pump`](BrowserShell::pump) show it instead of the backend's
    /// `ipfs://<cid>` `current_url`. It is [`None`] for an ordinary load (the bar
    /// then follows the backend's URL, including redirects/history moves), and is
    /// cleared by any navigation that is not the ENS front door (a plain
    /// navigate/back/forward/reload), so the name never lingers on a later page.
    url_override: Option<String>,
}

impl BrowserShell {
    /// Build a shell over the given rendering backend, using the default trusted
    /// [`RpcProvider`](crate::ethereum::RpcProvider) for ENS resolution.
    ///
    /// The initial chrome reflects the backend's starting state (Idle, empty URL
    /// bar, back/forward derived from the backend's session history), so a caller
    /// can paint the window before any navigation. The ENS front door resolves
    /// through the labelled default trusted RPC; use
    /// [`with_provider`](BrowserShell::with_provider) to point it at a specific
    /// endpoint or a Phase-2 backend.
    #[must_use]
    pub fn new(renderer: Box<dyn Renderer>) -> Self {
        Self::with_provider(renderer, Box::new(RpcProvider::new()))
    }

    /// Build a shell over the given rendering backend and
    /// [`EthereumProvider`](crate::ethereum::EthereumProvider).
    ///
    /// This is how a caller points the ENS front door at a specific RPC endpoint
    /// (or, in a test, an in-process fixture provider) rather than the labelled
    /// default — mirroring `RpcProvider::new` / `with_endpoint`, with no config
    /// subsystem to chase.
    #[must_use]
    pub fn with_provider(renderer: Box<dyn Renderer>, provider: Box<dyn EthereumProvider>) -> Self {
        // The default IPNS record source: a trustless-gateway fetch over the bound
        // HTTP `Fetcher`, pointed at the user's chosen retrieval backend (the SAME
        // `active_gateway_endpoint` the content path uses, so the IPNS record and
        // the content it points at come from one chosen gateway — no second
        // config). Verification of the fetched record happens client-side in
        // `ipns::resolve_ipns_name`, so this untrusted source cannot misdirect a
        // name.
        let ipns_source = Box::new(GatewayIpnsRecordSource::with_gateway(
            HttpFetcher::new(),
            &crate::retrieval::active_gateway_endpoint(),
        ));
        Self::with_provider_and_ipns_source(renderer, provider, ipns_source)
    }

    /// Build a shell over the given rendering backend,
    /// [`EthereumProvider`](crate::ethereum::EthereumProvider), AND
    /// [`IpnsRecordSource`](crate::ipns::IpnsRecordSource).
    ///
    /// This is how a caller (or a test) points BOTH the ENS front door and the
    /// IPNS front door at specific backends — an in-process fixture provider + a
    /// pinned record source — rather than the labelled defaults, so the whole
    /// bare-`.eth` → (ipfs-ns | ipns-ns) → verified-load path is exercised off the
    /// live network. Mirrors [`with_provider`](BrowserShell::with_provider), with
    /// no config subsystem to chase.
    #[must_use]
    pub fn with_provider_and_ipns_source(
        renderer: Box<dyn Renderer>,
        provider: Box<dyn EthereumProvider>,
        ipns_source: Box<dyn IpnsRecordSource>,
    ) -> Self {
        let mut shell = Self {
            renderer,
            chrome: ChromeState::default(),
            provider,
            ipns_source,
            url_override: None,
        };
        shell.refresh_chrome();
        shell
    }

    /// The current chrome state to paint the window from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        &self.chrome
    }

    /// The backend's underlying current-load URL, for tests that assert the
    /// front door fed the resolved `ipfs://<cid>` into the seam (distinct from the
    /// `.eth` name displayed in the bar). Test-only: production reads the bar via
    /// [`chrome`](BrowserShell::chrome).
    #[cfg(test)]
    fn current_url_for_test(&self) -> Option<String> {
        self.renderer.current_url()
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam.
    ///
    /// A bare `.eth` URL-bar entry (no scheme, like Brave/Opera — see
    /// [`eth_name_from_entry`]) is the ENS FRONT DOOR: it is resolved to an
    /// immutable `ipfs://<cid>` and loaded through the existing verified `ipfs://`
    /// path via [`navigate_ens_name`](BrowserShell::navigate_ens_name), keeping the
    /// `.eth` name in the address bar and marking the load's trust posture
    /// "content-verified, name via trusted RPC". Any other entry is navigated
    /// literally through the seam.
    ///
    /// On success the URL bar immediately reflects the target and any prior
    /// failure is cleared; an unusable URL is rejected by the backend with
    /// [`RendererError::InvalidUrl`] and leaves the chrome untouched (the bad text
    /// stays for the user to fix). The load lifecycle then advances via
    /// [`pump`](BrowserShell::pump).
    pub fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        if let Some(name) = eth_name_from_entry(url) {
            return self.navigate_ens_name(name);
        }
        self.renderer.navigate(url)?;
        // A plain navigation follows the backend's URL: drop any ENS name that was
        // pinned in the bar so it never lingers on a later page.
        self.url_override = None;
        self.chrome.last_error = None;
        self.refresh_chrome();
        Ok(())
    }

    /// Resolve a bare `.eth` `name` through the ENS front door and load the
    /// content it points to.
    ///
    /// This is the tracer-bullet path: it resolves `name` via the ENS core
    /// ([`ens::resolve`](crate::ens::resolve): namehash -> registry -> resolver ->
    /// contenthash -> ENSIP-7 decode) over the shell's
    /// [`EthereumProvider`](crate::ethereum::EthereumProvider), then dispatches by
    /// the DECODED contenthash's OWN type:
    ///
    /// * an `ipfs-ns` name feeds its `ipfs://<cid>` into the EXISTING verified
    ///   `ipfs://` render path (the seam's scheme handler hash-verifies the
    ///   bytes), and the load is flagged ENS-originated
    ///   ([`Renderer::mark_ens_origin`]) so the resulting posture is
    ///   "content-verified, name via trusted RPC" rather than plain
    ///   `ContentVerified`;
    /// * an `ipns-ns` name is first RESOLVED to its current CID via a
    ///   client-VERIFIED IPNS record ([`crate::ipns::resolve_ipns_name`] over the
    ///   shell's untrusted [`IpnsRecordSource`](crate::ipns::IpnsRecordSource)),
    ///   then that CID feeds the SAME verified `ipfs://` path. It is flagged BOTH
    ///   ENS-originated AND mutable-named, so the loudest applicable posture wins
    ///   (`NameViaTrustedRpc` via ENS today; `MutableName` once Phase 2 clears the
    ///   RPC warning) — NEVER immutable `ContentVerified`;
    /// * every OTHER type (swarm/arweave/unknown) is the decoder's graceful,
    ///   protocol-named failure — NEVER defaulted to `ipfs://`.
    ///
    /// The address bar keeps `name` (the identity the user typed), not the
    /// resolved CID: there is no `https://` rewrite and no gateway redirect.
    ///
    /// Fail-closed: a resolution failure or an unsupported/absent contenthash
    /// FAILS the load with a legible reason surfaced in
    /// [`ChromeState::last_error`], and nothing unverified is ever rendered. A
    /// failed resolution returns `Ok(())` (the front door handled the entry and
    /// surfaced the failure in the chrome), not an `Err`, so the URL bar keeps the
    /// name for the user to see the reason — mirroring how a failed load surfaces
    /// its reason rather than throwing.
    fn navigate_ens_name(&mut self, name: &str) -> Result<(), RendererError> {
        match crate::ens::resolve(self.provider.as_ref(), name) {
            Ok(DecodedContenthash::Ipfs { uri, .. }) => {
                // The immutable `ipfs-ns` case: load the resolved CID directly. It
                // is ENS-originated (trusted RPC) but NOT mutable-flagged, so the
                // posture is `NameViaTrustedRpc`.
                self.load_resolved_content(name, &uri, false);
                Ok(())
            }
            Ok(DecodedContenthash::Ipns { name: ipns_name }) => {
                // The MUTABLE `ipns-ns` case: RESOLVE the IPNS name to its current
                // CID via a client-VERIFIED record (fetched from the untrusted
                // record source, its signature + name-binding + validity checked
                // client-side against the key) BEFORE loading anything. A bad
                // record / bad target fails closed with its distinct reason —
                // nothing unverified is rendered.
                match crate::ipns::resolve_ipns_name(self.ipns_source.as_ref(), &ipns_name) {
                    Ok(resolved) => {
                        // The name is MUTABLE, so flag the load mutable-named too:
                        // its honest posture is at most `MutableName`, NEVER
                        // immutable `ContentVerified`. Via ENS the LOUDER
                        // `NameViaTrustedRpc` still wins today (the two-axis display
                        // rule); it falls back to `MutableName` once Phase 2 clears
                        // the RPC warning — no rule change here.
                        self.load_resolved_content(name, &resolved.uri, true);
                    }
                    // A record/target failure is fail-closed with its distinct,
                    // legible reason — the load renders nothing.
                    Err(e) => self.fail_ens_load(name, &e.to_string()),
                }
                Ok(())
            }
            // A well-formed but unsupported contenthash (swarm/arweave/unknown) is
            // the decoder's named refusal. `resolve` already maps it to
            // `Err(UnsupportedContenthash)`, so it does not surface here as an
            // `Ok`; but should the contract ever change, dispatch is by the
            // DECODED type's OWN kind — only `ipfs-ns`/`ipns-ns` are loadable, so an
            // `Unsupported` is fail-closed with its named reason, NEVER mis-
            // dispatched to `ipfs://`.
            Ok(other @ DecodedContenthash::Unsupported(_)) => {
                let reason = other
                    .reason()
                    .unwrap_or_else(|| "unsupported contenthash protocol".to_string());
                self.fail_ens_load(name, &reason);
                Ok(())
            }
            // Any typed resolution failure (unnormalizable name, no resolver, no/
            // malformed/unsupported contenthash, an RPC/seam error) is fail-closed
            // with its distinct, legible reason — nothing unverified is rendered.
            Err(e) => {
                self.fail_ens_load(name, &e.to_string());
                Ok(())
            }
        }
    }

    /// Feed an already-resolved `ipfs://<cid>` `uri` into the EXISTING verified
    /// `ipfs://` render path, keeping the front-door `name` in the address bar,
    /// and flag the load's trust axes.
    ///
    /// Shared by the `ipfs-ns` (immutable name via trusted RPC) and the resolved
    /// `ipns-ns` (mutable name) branches: both feed a CID into the SAME verified
    /// path, differing only in whether the name is MUTABLE. The load is always
    /// flagged ENS-originated ([`Renderer::mark_ens_origin`]) — the name was
    /// learned over the trusted RPC — and, when `mutable`, ALSO mutable-named
    /// ([`Renderer::mark_mutable_name`]); the backend then surfaces the LOUDEST
    /// applicable posture when the scheme handler verifies the bytes
    /// (`NameViaTrustedRpc` today, `MutableName` once Phase 2 clears the RPC
    /// warning). The flags must come AFTER `navigate` (which resets them on a
    /// fresh `begin`). If the backend cannot even start the load, that is a
    /// fail-closed front-door failure, never a silent success.
    fn load_resolved_content(&mut self, name: &str, uri: &str, mutable: bool) {
        if let Err(e) = self.renderer.navigate(uri) {
            self.fail_ens_load(name, &e.to_string());
            return;
        }
        self.renderer.mark_ens_origin();
        if mutable {
            self.renderer.mark_mutable_name();
        }
        // Keep the front-door NAME the user typed in the bar (no `https://`
        // rewrite, no gateway redirect). The override PERSISTS across pumps so the
        // name stays put for the whole load.
        self.url_override = Some(name.to_string());
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Fail an ENS front-door load closed: surface `reason` in the chrome and keep
    /// the `.eth` `name` in the bar, without navigating the backend to anything.
    ///
    /// This is the fail-closed path (spec story 3): a resolution failure or an
    /// unsupported/absent contenthash renders NOTHING — it only reports the
    /// legible reason the shell surfaces via [`ChromeState::last_error`], with the
    /// load state left settled so the chrome shows the failure rather than a
    /// spinner. The trust posture stays untrusted (no verified load happened).
    fn fail_ens_load(&mut self, name: &str, reason: &str) {
        // Pin the `.eth` name in the bar (the front door did not navigate the
        // backend anywhere, so there is no underlying URL to fall back to).
        self.url_override = Some(name.to_string());
        self.refresh_chrome();
        self.chrome.last_error = Some(reason.to_string());
    }

    /// Go one step back in session history, through the seam.
    ///
    /// A no-op when [`ChromeState::can_go_back`] is `false`. Delegates to the
    /// backend's session history (the shell keeps no URL stack of its own — see
    /// [`Renderer::go_back`]).
    pub fn go_back(&mut self) {
        self.renderer.go_back();
        // History navigation follows the backend's URL, not the pinned ENS name.
        self.url_override = None;
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Go one step forward in session history, through the seam.
    pub fn go_forward(&mut self) {
        self.renderer.go_forward();
        self.url_override = None;
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Reload the current page, through the seam.
    ///
    /// A reload re-loads the backend's CURRENT underlying URL (for an ENS page,
    /// the resolved `ipfs://<cid>`), so it drops any pinned ENS name from the bar
    /// and follows the backend: Phase 1 does not re-resolve the name on reload
    /// (the front-door resolution runs only on a fresh URL-bar Enter). The
    /// reloaded content-addressed page is still hash-verified by the `ipfs://`
    /// path, so it shows honestly as content-verified.
    pub fn reload(&mut self) -> Result<(), RendererError> {
        self.renderer.reload()?;
        self.url_override = None;
        self.chrome.last_error = None;
        self.refresh_chrome();
        Ok(())
    }

    /// Stop the in-flight load, through the seam.
    pub fn stop(&mut self) {
        self.renderer.stop();
        self.refresh_chrome();
    }

    /// Give (`true`) or take (`false`) keyboard focus of the live page view.
    ///
    /// This is how the shell makes the embedded page INTERACTIVE: with the live
    /// view focused, the OS/GTK routes scroll/click/focus/keyboard input to it
    /// natively (the webview's `send_*` forwarders are no-ops — the task's
    /// forward-pointer). The shell calls this through the seam rather than
    /// touching the webview.
    pub fn focus_page(&mut self, focused: bool) {
        self.renderer.set_focus(focused);
    }

    /// Drain every pending [`LoadEvent`] off the seam and fold it into the chrome.
    ///
    /// The window calls this on its main loop (a periodic pump). Each event moves
    /// the URL bar / load indicator: a `Started` clears any error and shows the
    /// target, `Committed`/`Finished` settle the URL bar on the effective URL,
    /// and a `Failed` surfaces the reason. Returns `true` if any event was
    /// processed, so a caller can repaint only on change.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.renderer.poll_event() {
            changed = true;
            // While an ENS name is pinned in the bar (`url_override`), the
            // lifecycle events carry the underlying `ipfs://<cid>` URL, which must
            // NOT overwrite the displayed name — the user keeps seeing `ronan.eth`
            // while the CID loads. `refresh_chrome` below re-applies the override.
            let pinned = self.url_override.is_some();
            match event {
                LoadEvent::Started { url } => {
                    if !pinned {
                        self.chrome.url_text = url;
                    }
                    self.chrome.last_error = None;
                }
                LoadEvent::Committed { url } | LoadEvent::Finished { url } => {
                    if !pinned {
                        self.chrome.url_text = url;
                    }
                }
                LoadEvent::Failed { url, reason } => {
                    if !pinned {
                        self.chrome.url_text = url;
                    }
                    self.chrome.last_error = Some(reason);
                }
            }
        }
        // The lifecycle state and history availability are read straight from the
        // seam (they are the backend's truth), so refresh them whether or not an
        // event fired — a failed/settled load and can_go_* can change without a
        // queued event.
        self.refresh_chrome();
        changed
    }

    /// The opaque live-view handle for the window to embed.
    #[must_use]
    pub fn view_handle(&self) -> renderer::ViewHandle {
        self.renderer.view_handle()
    }

    /// Re-read the seam's authoritative state (load state, history availability,
    /// and current URL) into the chrome. Load state and back/forward availability
    /// are the backend's truth and always pulled fresh; the URL bar tracks the
    /// backend's `current_url` whenever it has one (the effective URL after
    /// redirects/history moves), so an action that changes the current entry
    /// without a queued event (e.g. a synchronous back on a backend with history)
    /// still moves the bar.
    fn refresh_chrome(&mut self) {
        self.chrome.load_state = self.renderer.load_state();
        self.chrome.can_go_back = self.renderer.can_go_back();
        self.chrome.can_go_forward = self.renderer.can_go_forward();
        // The trust posture is the backend's truth about the current load path
        // (content-verified vs served), pulled fresh like the load state so the
        // indicator tracks the page actually shown — including after a scheme
        // handler verifies the bytes mid-load, which flips the posture without a
        // queued LoadEvent.
        self.chrome.trust_posture = self.renderer.trust_posture();
        // A pinned ENS name (`url_override`) is the DISPLAY identity for the bar
        // and wins over the backend's underlying `current_url` (the resolved
        // `ipfs://<cid>`): the user keeps seeing `ronan.eth`, never the CID or a
        // gateway URL. Otherwise the bar follows the backend's URL (redirects,
        // history moves).
        if let Some(name) = &self.url_override {
            self.chrome.url_text = name.clone();
        } else if let Some(url) = self.renderer.current_url() {
            self.chrome.url_text = url;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{
        KeyEvent, PointerEvent, SchemeHandler, ScriptMessageHandler, ScrollDelta, ViewHandle,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// The mutable innards of the [`FakeBackend`], modelling a REAL session
    /// history the way a webview does (a back/forward list) plus the native load
    /// signals a running GTK loop would deliver. It lives behind `Rc<RefCell>` so
    /// a test can hold a `handle` to drive the simulated native signals while the
    /// shell owns the `dyn Renderer` — exactly the interior-mutability shape the
    /// real `WebViewRenderer` uses to share its `LoadLifecycle` with the webview's
    /// signal closures, so the test drives the seam, never reaches past it.
    #[derive(Default)]
    struct BackendInner {
        /// The back/forward list; `cursor` is the index of the current entry.
        history: Vec<String>,
        cursor: Option<usize>,
        state: LoadState,
        events: VecDeque<LoadEvent>,
        /// Records that the shell forwarded focus/input through the seam, so the
        /// test can assert the shell drives interaction via the seam (not by
        /// reaching past it). On the webview these are no-ops; here we only prove
        /// the CALL crosses the seam.
        focus_calls: Vec<bool>,
        pointer_calls: u32,
        key_calls: u32,
        scroll_calls: u32,
        /// The trust posture of the current load, mirroring the real backend's
        /// shared `LoadLifecycle`: reset to the untrusted origin on every fresh
        /// navigation and flipped to content-verified only when the simulated
        /// verified content-addressed path served this load's bytes.
        posture: TrustPosture,
        /// Whether the current load was flagged ENS-originated (the shell called
        /// `mark_ens_origin`), mirroring the real `LoadLifecycle::ens_origin`:
        /// reset on every fresh `navigate`/`reload`/history move, and consulted by
        /// the simulated verified content path so it surfaces `NameViaTrustedRpc`
        /// instead of plain `ContentVerified` — exactly the real backend's
        /// `mark_content_verified` redirect.
        ens_origin: bool,
        /// Whether the current load was flagged as pointing at a MUTABLE name (the
        /// shell called `mark_mutable_name`), mirroring the real
        /// `LoadLifecycle::mutable_name`: reset on every fresh
        /// `navigate`/`reload`/history move, and consulted by the simulated
        /// verified content path so it surfaces `MutableName` (when not also
        /// ENS-originated) — the two-axis display rule the real backend applies.
        mutable_name: bool,
    }

    impl BackendInner {
        fn current(&self) -> Option<&String> {
            self.cursor.and_then(|c| self.history.get(c))
        }
    }

    /// A seam-level fake backend over a shared [`BackendInner`]. It renders
    /// nothing; it exists ONLY to exercise the shell↔seam wiring (navigation
    /// state transitions, chrome, history availability, focus/input forwarding)
    /// at the trait boundary without a GTK main loop or a display.
    #[derive(Default, Clone)]
    struct FakeBackend {
        inner: Rc<RefCell<BackendInner>>,
    }

    impl FakeBackend {
        /// A handle a test keeps to drive the backend's simulated native signals.
        fn handle(&self) -> BackendHandle {
            BackendHandle {
                inner: self.inner.clone(),
            }
        }
    }

    /// A test-side handle to the same [`BackendInner`] the shell drives, used to
    /// simulate the backend's native load signals (the stand-in for a running GTK
    /// loop turning the webview's `load-changed`/`load-failed` signals).
    struct BackendHandle {
        inner: Rc<RefCell<BackendInner>>,
    }

    impl BackendHandle {
        /// Carry the in-flight load to done (commit then finish), as a real
        /// webview's load signals would.
        fn drive_to_finished(&self) {
            let mut b = self.inner.borrow_mut();
            let url = b.current().expect("a load in flight").clone();
            b.state = LoadState::Committed;
            b.events
                .push_back(LoadEvent::Committed { url: url.clone() });
            b.state = LoadState::Finished;
            b.events.push_back(LoadEvent::Finished { url });
        }

        /// Report a failed load.
        fn drive_to_failed(&self, reason: &str) {
            let mut b = self.inner.borrow_mut();
            let url = b.current().expect("a load in flight").clone();
            b.state = LoadState::Failed;
            b.events.push_back(LoadEvent::Failed {
                url,
                reason: reason.to_string(),
            });
        }

        fn focus_calls(&self) -> Vec<bool> {
            self.inner.borrow().focus_calls.clone()
        }

        /// Simulate the `ipfs://` scheme handler serving the current load's main
        /// resource through the hash-verified content-addressed path: it calls the
        /// SAME unconditional `mark_content_verified` the real scheme handler does
        /// (via [`mark_content_verified`](BackendHandle::mark_content_verified)),
        /// which surfaces `NameViaTrustedRpc` when the load was flagged
        /// ENS-originated and plain `ContentVerified` otherwise. Only a load that
        /// actually went through this path flips the posture — a plain served load
        /// never calls it.
        fn serve_via_verified_content_path(&self) {
            self.mark_content_verified();
        }

        /// The scheme handler's UNCONDITIONAL verified mark, mirroring the real
        /// `LoadLifecycle::mark_content_verified`: it redirects to
        /// `NameViaTrustedRpc` when the current load was flagged ENS-originated
        /// (`mark_ens_origin`), else plain `ContentVerified`. This is the exact
        /// mechanism by which the ENS-origin posture WINS over the scheme handler's
        /// mark without the handler knowing about ENS.
        fn mark_content_verified(&self) {
            let mut b = self.inner.borrow_mut();
            // The two-axis display rule: the trusted-RPC warning is the loudest,
            // then the mutable-name warning, else plain content-verified — exactly
            // the real `LoadLifecycle::mark_content_verified`.
            b.posture = if b.ens_origin {
                TrustPosture::NameViaTrustedRpc
            } else if b.mutable_name {
                TrustPosture::MutableName
            } else {
                TrustPosture::ContentVerified
            };
        }

        /// Simulate a bare `.eth` load: flag the current load ENS-originated (as
        /// the front door's `mark_ens_origin` does) THEN serve it through the
        /// verified content path (the unconditional `mark_content_verified`), so
        /// the posture surfaces `NameViaTrustedRpc` — driving the REAL mechanism,
        /// not setting the posture directly. Only a load actually flagged
        /// ENS-originated reaches this posture.
        fn serve_via_ens_trusted_rpc(&self) {
            self.inner.borrow_mut().ens_origin = true;
            self.mark_content_verified();
        }

        /// Simulate a client-verified IPNS load: flag the current load as pointing
        /// at a MUTABLE name (as the front door's `mark_mutable_name` does) THEN
        /// serve it through the verified content path, so the posture surfaces
        /// `MutableName` — driving the REAL two-axis mechanism, not setting the
        /// posture directly. Only a load actually flagged mutable (and NOT
        /// ENS-originated) reaches this posture.
        #[allow(dead_code)]
        fn serve_via_ipns_mutable_name(&self) {
            self.inner.borrow_mut().mutable_name = true;
            self.mark_content_verified();
        }
    }

    impl Renderer for FakeBackend {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            // Accept any `scheme://rest` URL (the day-one http(s) path plus the
            // ipfs:// trust-hook scheme), mirroring the real backend's
            // `validate_url`; a scheme-less string is still rejected.
            match url.split_once("://") {
                Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => {}
                _ => return Err(RendererError::InvalidUrl(url.to_string())),
            }
            let mut b = self.inner.borrow_mut();
            // A fresh navigation from mid-history drops the forward entries.
            let next = b.cursor.map_or(0, |c| c + 1);
            b.history.truncate(next);
            b.history.push(url.to_string());
            b.cursor = Some(b.history.len() - 1);
            b.state = LoadState::Started;
            // A fresh load starts UNVERIFIED and is only marked verified if this
            // load's bytes actually go through the verified content path — exactly
            // the real `LoadLifecycle::begin` reset that keeps the posture tracking
            // the CURRENT page's load path, never a stale value. The ENS-origin
            // flag resets too, so it never leaks onto a later load.
            b.posture = TrustPosture::UnverifiedOrigin;
            b.ens_origin = false;
            b.mutable_name = false;
            b.events.push_back(LoadEvent::Started {
                url: url.to_string(),
            });
            Ok(())
        }

        fn reload(&mut self) -> Result<(), RendererError> {
            let mut b = self.inner.borrow_mut();
            let url = b
                .current()
                .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?
                .clone();
            b.state = LoadState::Started;
            b.posture = TrustPosture::UnverifiedOrigin;
            b.ens_origin = false;
            b.mutable_name = false;
            b.events.push_back(LoadEvent::Started { url });
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
                    b.state = LoadState::Started;
                    b.posture = TrustPosture::UnverifiedOrigin;
                    b.ens_origin = false;
                    b.mutable_name = false;
                    b.events.push_back(LoadEvent::Started { url });
                }
            }
        }

        fn go_forward(&mut self) {
            let mut b = self.inner.borrow_mut();
            if let Some(c) = b.cursor {
                if c + 1 < b.history.len() {
                    b.cursor = Some(c + 1);
                    let url = b.history[c + 1].clone();
                    b.state = LoadState::Started;
                    b.posture = TrustPosture::UnverifiedOrigin;
                    b.ens_origin = false;
                    b.mutable_name = false;
                    b.events.push_back(LoadEvent::Started { url });
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

        fn trust_posture(&self) -> TrustPosture {
            self.inner.borrow().posture
        }

        fn mark_ens_origin(&mut self) {
            // Flag the current load ENS-originated, exactly as the real
            // `WebViewRenderer::mark_ens_origin` forwards to the shared lifecycle.
            self.inner.borrow_mut().ens_origin = true;
        }

        fn mark_mutable_name(&mut self) {
            // Flag the current load as pointing at a MUTABLE name, exactly as the
            // real `WebViewRenderer::mark_mutable_name` forwards to the shared
            // lifecycle's second axis.
            self.inner.borrow_mut().mutable_name = true;
        }

        fn current_url(&self) -> Option<String> {
            self.inner.borrow().current().cloned()
        }

        fn poll_event(&mut self) -> Option<LoadEvent> {
            self.inner.borrow_mut().events.pop_front()
        }

        fn view_handle(&self) -> ViewHandle {
            ViewHandle(std::ptr::null_mut())
        }

        fn send_pointer(&mut self, _event: PointerEvent) {
            self.inner.borrow_mut().pointer_calls += 1;
        }
        fn send_key(&mut self, _event: KeyEvent) {
            self.inner.borrow_mut().key_calls += 1;
        }
        fn send_scroll(&mut self, _delta: ScrollDelta) {
            self.inner.borrow_mut().scroll_calls += 1;
        }
        fn set_focus(&mut self, focused: bool) {
            self.inner.borrow_mut().focus_calls.push(focused);
        }

        fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {
        }
        fn inject_script(&mut self, _script: &str) {}
        fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {}
    }

    /// Build a shell over a fresh fake backend, returning both the shell and a
    /// handle to drive the backend's simulated native load signals. `settle`
    /// drives the in-flight load to done and pumps the shell — the test stand-in
    /// for a GTK loop turning the webview's load signals.
    fn shell_with_backend() -> (BrowserShell, BackendHandle) {
        let backend = FakeBackend::default();
        let handle = backend.handle();
        (BrowserShell::new(Box::new(backend)), handle)
    }

    fn settle(shell: &mut BrowserShell, handle: &BackendHandle) {
        handle.drive_to_finished();
        shell.pump();
    }

    // ---- The ENS front door: bare `.eth` -> resolve -> verified `ipfs://` -----

    use crate::contenthash::ContenthashError;
    use crate::ethereum::{EthCall, EthereumProvider, ProviderError};
    use fetcher::{cid_v1_raw_sha256, Cid};

    /// An in-process [`EthereumProvider`] double answering each `eth_call` in
    /// order from a queue of canned results — the pinned RPC fixture the front
    /// door resolves through, off the live network (mirrors the `ens` module's
    /// own `ScriptedProvider`). It captures the calls so a test could assert the
    /// calldata, but the front-door tests only care that a name resolves.
    struct ScriptedProvider {
        answers: RefCell<VecDeque<Result<Vec<u8>, ProviderError>>>,
    }

    impl ScriptedProvider {
        fn new(answers: Vec<Result<Vec<u8>, ProviderError>>) -> Self {
            Self {
                answers: RefCell::new(answers.into_iter().collect()),
            }
        }
    }

    impl EthereumProvider for ScriptedProvider {
        fn eth_call(&self, _call: &EthCall) -> Result<Vec<u8>, ProviderError> {
            self.answers
                .borrow_mut()
                .pop_front()
                .expect("the scripted provider ran out of canned answers")
        }
    }

    /// A 32-byte ABI word holding a right-aligned 20-byte address (a
    /// `resolver(node)` return).
    fn address_word(addr20: &[u8; 20]) -> Vec<u8> {
        let mut word = vec![0u8; 32];
        word[12..32].copy_from_slice(addr20);
        word
    }

    /// ABI-encode a dynamic `bytes` return (a `contenthash(node)` result): an
    /// offset word (0x20), a length word, then the payload padded to 32 bytes.
    fn abi_bytes_return(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offset = [0u8; 32];
        offset[31] = 0x20;
        out.extend_from_slice(&offset);
        let mut len = [0u8; 32];
        len[24..32].copy_from_slice(&(payload.len() as u64).to_be_bytes());
        out.extend_from_slice(&len);
        out.extend_from_slice(payload);
        let pad = (32 - payload.len() % 32) % 32;
        out.extend(std::iter::repeat_n(0u8, pad));
        out
    }

    /// Encode a multicodec protoCode as an unsigned LEB128 varint (the real
    /// on-the-wire contenthash prefix; 0xe3 etc. are multi-byte).
    fn varint(mut code: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (code & 0x7f) as u8;
            code >>= 7;
            if code != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if code == 0 {
                break;
            }
        }
        out
    }

    /// The raw ENSIP-7 `ipfs-ns` contenthash bytes for a fixture site, plus the
    /// canonical `ipfs://<cid>` URI they decode to — derived with the SAME
    /// `cid_v1_raw_sha256` helper the verified path uses, so the CID that the
    /// front door feeds into the `ipfs://` path is honest.
    fn ipfs_contenthash_fixture(bytes: &[u8]) -> (Vec<u8>, String) {
        let cid_str = cid_v1_raw_sha256(bytes).expect("derive fixture cid");
        let cid_bytes = Cid::try_from(cid_str.as_str())
            .expect("cid parses")
            .to_bytes();
        let mut ch = varint(0xe3); // ipfs-ns protoCode
        ch.extend_from_slice(&cid_bytes);
        (ch, format!("ipfs://{cid_str}"))
    }

    /// Build a shell over a fresh fake backend AND a scripted RPC fixture
    /// provider, so the ENS front door resolves off the live network. Returns the
    /// shell and the backend handle (for driving the simulated load signals).
    fn shell_with_provider(
        answers: Vec<Result<Vec<u8>, ProviderError>>,
    ) -> (BrowserShell, BackendHandle) {
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let provider = ScriptedProvider::new(answers);
        (
            BrowserShell::with_provider(Box::new(backend), Box::new(provider)),
            handle,
        )
    }

    // ---- The IPNS front door: bare `.eth` -> ipns-ns -> resolve record --------

    use crate::ipns::{IpnsError, IpnsRecordSource};
    use libp2p_identity::Keypair;

    /// A minted ed25519 keypair + the canonical base36 IPNS name (the libp2p-key
    /// CID) it corresponds to, PLUS the raw `ipns-ns` contenthash bytes an ENS
    /// resolver would return for it. A REAL key, so the record it signs verifies
    /// against the name the ENS front door decodes — all off the live network.
    struct IpnsKeyFixture {
        keypair: Keypair,
        name: String,
        contenthash: Vec<u8>,
    }

    impl IpnsKeyFixture {
        fn new() -> Self {
            let keypair = Keypair::generate_ed25519();
            let peer_id = keypair.public().to_peer_id();
            const LIBP2P_KEY_CODEC: u64 = 0x72;
            let mh = cid::multihash::Multihash::from_bytes(&peer_id.to_bytes())
                .expect("peer id is a multihash");
            let name_cid = Cid::new_v1(LIBP2P_KEY_CODEC, mh);
            let name = name_cid
                .to_string_of_base(cid::multibase::Base::Base36Lower)
                .expect("base36 name");
            // The ENSIP-7 `ipns-ns` contenthash is the 0xe5 protoCode varint plus
            // the libp2p-key CID bytes — exactly what the decoder consumes.
            let mut contenthash = varint(0xe5);
            contenthash.extend_from_slice(&name_cid.to_bytes());
            Self {
                keypair,
                name,
                contenthash,
            }
        }

        /// Sign a record pointing the name at `/ipfs/<cid>`, valid 24h, seq 1, and
        /// return its encoded bytes (the wire form a gateway serves).
        fn signed_record_for(&self, ipfs_cid: &str) -> Vec<u8> {
            use chrono::{Duration as ChronoDuration, Utc};
            let record = rust_ipns::Record::new(
                &self.keypair,
                format!("/ipfs/{ipfs_cid}").as_bytes(),
                Utc::now() + ChronoDuration::hours(24),
                1,
                std::time::Duration::from_secs(3600),
            )
            .expect("sign an ipns record");
            record.encode().expect("encode the signed record")
        }
    }

    /// A pinned in-memory [`IpnsRecordSource`] for the front-door tests: returns a
    /// canned record for a name, or a chosen source failure, off the network.
    #[derive(Default)]
    struct PinnedIpnsSource {
        records: std::collections::HashMap<String, Vec<u8>>,
        fail: Option<IpnsError>,
    }

    impl PinnedIpnsSource {
        fn with_record(name: &str, record: Vec<u8>) -> Self {
            let mut records = std::collections::HashMap::new();
            records.insert(name.to_string(), record);
            Self {
                records,
                fail: None,
            }
        }

        fn failing(err: IpnsError) -> Self {
            Self {
                records: std::collections::HashMap::new(),
                fail: Some(err),
            }
        }
    }

    impl IpnsRecordSource for PinnedIpnsSource {
        fn fetch_record(&self, name: &str) -> Result<Vec<u8>, IpnsError> {
            if let Some(err) = &self.fail {
                return Err(err.clone());
            }
            self.records
                .get(name)
                .cloned()
                .ok_or_else(|| IpnsError::Source(format!("no record pinned for {name}")))
        }
    }

    /// Build a shell over a fresh fake backend, a scripted RPC provider, AND a
    /// pinned IPNS record source — so BOTH the ENS resolution and the IPNS record
    /// resolution run off the live network.
    fn shell_with_provider_and_ipns(
        answers: Vec<Result<Vec<u8>, ProviderError>>,
        ipns_source: PinnedIpnsSource,
    ) -> (BrowserShell, BackendHandle) {
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let provider = ScriptedProvider::new(answers);
        (
            BrowserShell::with_provider_and_ipns_source(
                Box::new(backend),
                Box::new(provider),
                Box::new(ipns_source),
            ),
            handle,
        )
    }

    #[test]
    fn a_bare_eth_entry_is_recognised_as_an_ens_name() {
        // Acceptance: a `*.eth` URL-bar entry (no scheme), on Enter or with a
        // trailing `/`, is recognised as a bare ENS name; anything with an
        // explicit scheme, a path, or no `.eth` label is NOT.
        assert_eq!(eth_name_from_entry("ronan.eth"), Some("ronan.eth"));
        assert_eq!(eth_name_from_entry("ronan.eth/"), Some("ronan.eth"));
        assert_eq!(eth_name_from_entry("Ronan.ETH"), Some("Ronan.ETH"));
        assert_eq!(eth_name_from_entry("a.b.eth"), Some("a.b.eth"));
        // An explicit scheme is literal, never a name (so `ipfs://`/`https://`
        // are never hijacked, and `ens://` is not required in Phase 1).
        assert_eq!(eth_name_from_entry("https://ronan.eth/"), None);
        assert_eq!(eth_name_from_entry("ipfs://bafycid"), None);
        assert_eq!(eth_name_from_entry("ens://ronan.eth"), None);
        // Not name-ish enough / not `.eth`.
        assert_eq!(eth_name_from_entry("example.com"), None);
        assert_eq!(eth_name_from_entry(".eth"), None);
        assert_eq!(eth_name_from_entry("ronan.eth/page"), None);
    }

    #[test]
    fn a_bare_eth_name_resolves_and_renders_the_ipfs_site_with_the_name_in_the_bar() {
        // Acceptance (the DONE bar, end to end, offline): a bare `ronan.eth` entry
        // is recognised, resolved over the pinned RPC fixture to an `ipfs-ns`
        // contenthash, and loaded through the EXISTING verified `ipfs://` path —
        // and the address bar keeps `ronan.eth` (no https:// rewrite, no gateway
        // redirect), while the internal load is the resolved CID. Then the trust
        // state is "content-verified, name via trusted RPC", distinct from a plain
        // ipfs load's ContentVerified.
        let page = b"<!doctype html><title>ronan</title><h1>ronan.eth's immutable site</h1>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),    // registry.resolver(node)
            Ok(abi_bytes_return(&contenthash)), // resolver.contenthash(node)
        ]);

        shell
            .navigate("ronan.eth")
            .expect("the front door handles a .eth entry");
        // The bar shows the NAME, not the CID, even while the CID loads.
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert!(shell.chrome().is_loading(), "the ipfs load is in flight");
        // The underlying load actually went to the resolved `ipfs://<cid>`.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(ipfs_uri.as_str())
        );

        // The `ipfs://` scheme handler verifies the bytes and marks the load; then
        // it settles. The name stays pinned in the bar across the pump.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth",
            "the .eth name stays in the bar through the whole verified load"
        );
        // The trust state is the DISTINCT name-via-trusted-RPC posture, NEVER the
        // plain content-verified one (Phase 1 makes no name-verification claim).
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );
        assert!(shell.chrome().is_name_via_trusted_rpc());
        assert!(!shell.chrome().is_content_verified());
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn a_plain_ipfs_load_stays_content_verified_and_the_ens_posture_does_not_leak() {
        // Acceptance: a plain (non-ENS) `ipfs://` load still shows ContentVerified,
        // and navigating there AFTER an ENS load does not carry the ENS posture or
        // the ENS name over — a fresh navigation resets both.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, _uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // First: a real ENS load ends in the name-via-trusted-RPC posture.
        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert!(shell.chrome().is_name_via_trusted_rpc());

        // Then: a plain `ipfs://<cid>` load (typed directly, no ENS) is plain
        // ContentVerified — the ENS posture does NOT leak onto it, and the bar
        // shows the ipfs URL, not `ronan.eth`.
        shell
            .navigate("ipfs://bafyplaincid/index.html")
            .expect("a plain ipfs url navigates");
        assert_eq!(shell.chrome().url_text, "ipfs://bafyplaincid/index.html");
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::ContentVerified,
            "a plain ipfs load is plain content-verified, not the ENS posture"
        );
        assert!(shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_name_via_trusted_rpc());

        // And a later plain served load is untrusted (neither posture leaks).
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn an_unsupported_name_fails_closed_with_its_protocol_named_reason() {
        // Acceptance: a name whose contenthash is an unsupported protocol
        // (swarm-ns here) FAILS the load with the decoder's distinct, protocol-
        // named reason in the chrome — NEVER a mis-dispatch to ipfs://, never a
        // rendered page. Fail-closed.
        let mut swarm_ch = varint(0xe4); // swarm-ns
        swarm_ch.extend_from_slice(b"some swarm address bytes");
        let (mut shell, _handle) = shell_with_provider(vec![
            Ok(address_word(&[0x44u8; 20])),
            Ok(abi_bytes_return(&swarm_ch)),
        ]);

        shell
            .navigate("swarm-site.eth")
            .expect("the front door handles the entry (and fails it closed)");
        // Nothing was navigated to / rendered: the backend has no ipfs load.
        assert_eq!(
            shell.current_url_for_test(),
            None,
            "nothing unverified loaded"
        );
        // The chrome surfaces the distinct, protocol-named reason, and the name
        // stays in the bar.
        assert_eq!(shell.chrome().url_text, "swarm-site.eth");
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("points to Swarm, not supported")
        );
        // The trust posture never became verified.
        assert!(!shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn a_name_with_no_contenthash_fails_closed() {
        // Fail-closed: a name whose resolver returns an empty contenthash (no site
        // set) fails the load with the decoder's distinct "no contenthash" reason,
        // never a guessed or unverified render.
        let (mut shell, _handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&[])), // empty contenthash -> NoContenthash
        ]);
        shell.navigate("empty.eth").expect("handled, failed closed");
        assert_eq!(shell.current_url_for_test(), None);
        assert_eq!(
            shell.chrome().last_error,
            Some(ContenthashError::NoContenthash.to_string())
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn a_resolution_rpc_error_fails_closed_with_a_legible_reason() {
        // Fail-closed: an RPC/seam error during resolution fails the load with a
        // legible reason, never renders anything.
        let (mut shell, _handle) = shell_with_provider(vec![Err(ProviderError::Transport(
            "connection refused".to_string(),
        ))]);
        shell
            .navigate("unreachable.eth")
            .expect("handled, failed closed");
        assert_eq!(shell.current_url_for_test(), None);
        let reason = shell.chrome().last_error.clone().expect("a legible reason");
        assert!(
            reason.contains("connection refused"),
            "the chrome surfaces the seam's reason: {reason}"
        );
        assert!(!shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn a_bare_eth_name_with_an_ipns_ns_contenthash_resolves_via_a_verified_record_and_renders() {
        // Acceptance (the DONE bar, end to end, offline): a bare `.eth` whose ENS
        // contenthash is `ipns-ns` (0xe5) is no longer refused — the front door
        // RESOLVES the IPNS name to its current CID via a client-VERIFIED record
        // (fetched from the untrusted pinned source, signature + validity checked
        // against the key), feeds that CID into the EXISTING verified `ipfs://`
        // path, keeps the `.eth` name in the bar, and — because the name is MUTABLE
        // and learned via a trusted RPC — shows the loudest applicable warning
        // (`NameViaTrustedRpc` via ENS), NEVER immutable `ContentVerified`.
        let key = IpnsKeyFixture::new();
        let page = b"<!doctype html><title>ipns</title><h1>the ipns site's current content</h1>";
        let target_cid = cid_v1_raw_sha256(page).expect("derive the target cid");
        let record = key.signed_record_for(&target_cid);
        let (mut shell, handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),        // registry.resolver(node)
                Ok(abi_bytes_return(&key.contenthash)), // resolver.contenthash(node) -> ipns-ns
            ],
            PinnedIpnsSource::with_record(&key.name, record),
        );

        shell
            .navigate("mutable.eth")
            .expect("the front door handles a .eth entry with an ipns-ns contenthash");
        // The bar shows the NAME, not the resolved CID.
        assert_eq!(shell.chrome().url_text, "mutable.eth");
        assert!(
            shell.chrome().is_loading(),
            "the resolved ipfs load is in flight"
        );
        // The underlying load went to the resolved `ipfs://<cid>` (the record's
        // current target), proving the IPNS resolution actually happened.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(format!("ipfs://{target_cid}").as_str())
        );

        // The `ipfs://` scheme handler verifies the bytes and marks the load; it
        // settles with the name still pinned.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(shell.chrome().url_text, "mutable.eth");
        // Via ENS the loudest warning wins: `NameViaTrustedRpc`. It is NEVER the
        // immutable `ContentVerified`.
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );
        assert!(!shell.chrome().is_content_verified());
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn a_client_verified_ipns_load_shows_the_mutable_name_posture_once_rpc_trust_clears() {
        // Acceptance: the NEW `MutableName` posture is the honest floor for a
        // client-verified IPNS load whose name was NOT learned over a trusted RPC
        // (the Phase-2 shape, and a direct `ipns://` follow-on). Driven through the
        // REAL two-axis mechanism: a load flagged mutable-named but NOT
        // ENS-originated surfaces `MutableName` — content-verified, never
        // "verified", and distinct from the trusted-RPC posture. This proves the
        // display precedence is explicit, so ENS falls back to `MutableName` with
        // no rule change once Phase 2 clears the RPC warning.
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("ipns://k51fixturekey/index.html")
            .expect("a direct ipns-resolved ipfs load navigates");
        // The mechanism: flag the load mutable-named (as the front door does for a
        // resolved IPNS name) then serve it through the verified content path.
        handle.serve_via_ipns_mutable_name();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().trust_posture, TrustPosture::MutableName);
        assert!(shell.chrome().is_mutable_name());
        assert!(!shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn an_ipns_name_with_an_unverifiable_record_fails_closed() {
        // Fail-closed: an `ipns-ns` name whose record does NOT verify (here signed
        // by a DIFFERENT key than the name — a misdirecting source) FAILS the load
        // with a legible reason, renders NOTHING, and never becomes verified.
        let key = IpnsKeyFixture::new();
        let attacker = IpnsKeyFixture::new();
        let target_cid = cid_v1_raw_sha256(b"content the attacker wants").expect("cid");
        // The attacker signs a record, but it is served for `key`'s name.
        let forged = attacker.signed_record_for(&target_cid);
        let (mut shell, _handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ],
            PinnedIpnsSource::with_record(&key.name, forged),
        );

        shell
            .navigate("forged.eth")
            .expect("handled, failed closed");
        // Nothing was navigated to / rendered.
        assert_eq!(
            shell.current_url_for_test(),
            None,
            "nothing unverified loaded"
        );
        assert_eq!(shell.chrome().url_text, "forged.eth");
        let reason = shell.chrome().last_error.clone().expect("a legible reason");
        assert!(
            reason.contains("did not verify"),
            "the chrome surfaces the record-verification failure: {reason}"
        );
        assert!(!shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_name_via_trusted_rpc());
        assert!(!shell.chrome().is_mutable_name());
    }

    #[test]
    fn an_ipns_name_whose_record_cannot_be_fetched_fails_closed() {
        // Fail-closed: an `ipns-ns` name whose record cannot be fetched (an
        // unresolvable name / dead endpoint) fails the load with a distinct
        // legible reason, never a guessed render.
        let key = IpnsKeyFixture::new();
        let (mut shell, _handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ],
            PinnedIpnsSource::failing(IpnsError::Source("connection refused".into())),
        );

        shell
            .navigate("unreachable-ipns.eth")
            .expect("handled, failed closed");
        assert_eq!(shell.current_url_for_test(), None);
        let reason = shell.chrome().last_error.clone().expect("a legible reason");
        assert!(
            reason.contains("connection refused"),
            "the chrome surfaces the record-fetch failure: {reason}"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn typed_url_navigates_through_the_seam_and_updates_the_chrome() {
        // Acceptance: a window with a URL bar navigates to a typed URL through the
        // seam, and the chrome reflects the in-flight load.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert_eq!(shell.chrome().url_text, "");

        shell
            .navigate("https://example.com/")
            .expect("valid https url");
        assert_eq!(shell.chrome().url_text, "https://example.com/");
        assert!(shell.chrome().is_loading(), "load is in flight after Enter");

        // Draining the seam's lifecycle events settles the chrome on Finished.
        handle.drive_to_finished();
        assert!(shell.pump());
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert!(!shell.chrome().is_loading());
        assert_eq!(shell.chrome().url_text, "https://example.com/");
    }

    #[test]
    fn navigate_rejects_an_unusable_url_and_leaves_the_chrome_untouched() {
        let (mut shell, _handle) = shell_with_backend();
        let err = shell
            .navigate("not-a-url")
            .expect_err("unusable url rejected");
        assert_eq!(err, RendererError::InvalidUrl("not-a-url".into()));
        // A rejected navigation does not start a load or move the chrome.
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert_eq!(shell.chrome().url_text, "");
    }

    #[test]
    fn back_and_forward_work_and_reflect_navigation_state() {
        // Acceptance: back/forward work and reflect navigation state (the Back /
        // Forward controls enable/disable as history allows), all through the seam.
        let (mut shell, handle) = shell_with_backend();
        assert!(!shell.chrome().can_go_back, "no history at the start");
        assert!(!shell.chrome().can_go_forward);

        shell.navigate("https://a.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(!shell.chrome().can_go_back, "one entry: nowhere back");

        shell.navigate("https://b.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_back, "two entries: can go back");
        assert!(!shell.chrome().can_go_forward);

        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://a.example/");
        assert!(!shell.chrome().can_go_back, "back at the first entry");
        assert!(shell.chrome().can_go_forward, "a forward entry now exists");

        shell.go_forward();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://b.example/");
        assert!(shell.chrome().can_go_back);
        assert!(!shell.chrome().can_go_forward, "back at the tip of history");
    }

    #[test]
    fn a_fresh_navigation_from_mid_history_drops_the_forward_entries() {
        // Navigating after a Back truncates forward history, so Forward greys out
        // again — the navigation-state contract the chrome must reflect.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://a.example/").unwrap();
        settle(&mut shell, &handle);
        shell.navigate("https://b.example/").unwrap();
        settle(&mut shell, &handle);
        shell.go_back();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_forward);

        shell.navigate("https://c.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_back);
        assert!(
            !shell.chrome().can_go_forward,
            "a new navigation dropped the forward entry"
        );
        assert_eq!(shell.chrome().url_text, "https://c.example/");
    }

    #[test]
    fn reload_re_navigates_and_stop_settles_the_load() {
        let (mut shell, handle) = shell_with_backend();
        assert!(shell.reload().is_err(), "nothing to reload yet");

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);

        shell.reload().expect("reload the settled page");
        assert!(shell.chrome().is_loading(), "reload restarts the load");

        // Stop mid-load returns the chrome to a settled (idle) state.
        shell.stop();
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert!(!shell.chrome().is_loading());
    }

    #[test]
    fn a_failed_load_surfaces_the_failure_in_the_chrome() {
        // Acceptance: load-lifecycle failure is surfaced through the seam into the
        // chrome (the shell shows the reason), and clears on the next navigation.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://does-not-resolve.invalid/").unwrap();
        assert!(shell.pump()); // drain the Started event
        assert_eq!(shell.chrome().last_error, None);

        handle.drive_to_failed("name not resolved");
        assert!(shell.pump());
        assert_eq!(shell.chrome().load_state, LoadState::Failed);
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("name not resolved")
        );

        // A new navigation clears the surfaced failure.
        shell.navigate("https://example.com/").unwrap();
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn the_chrome_shows_the_unverified_posture_for_a_plain_served_load() {
        // Acceptance: an ordinary served-origin load surfaces the UNVERIFIED trust
        // posture in the chrome. It is read straight from the seam (the actual
        // load path), and a plain load never went through the verified
        // content-addressed path, so it is content-verified == false.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "nothing loaded yet: the untrusted default"
        );

        shell
            .navigate("https://example.com/")
            .expect("valid https url");
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "a plain served page is not content-verified"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_chrome_shows_the_content_verified_posture_when_served_via_the_verified_path() {
        // Acceptance: a page whose bytes came back through the hash-verified
        // content-addressed path surfaces the CONTENT-VERIFIED posture in the
        // chrome — and it tracks the ACTUAL load path, not the URL: the posture
        // only flips after the verified content path serves this load's main
        // resource (mirroring the real `ipfs://` scheme handler marking the
        // lifecycle on a verified resolution).
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("ipfs://bafyfixturecid/index.html")
            .expect("an ipfs url is navigable through the seam");
        // Before the verified content path serves the bytes, the load is untrusted
        // — the URL looking like `ipfs://` is NOT enough to claim verified.
        shell.pump();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "an ipfs:// URL is not content-verified until its bytes actually verify"
        );

        // The scheme handler resolves the main resource through the hash-verified
        // path and marks the load verified; then the load settles.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::ContentVerified,
            "the verified content path surfaces the content-verified posture"
        );
        assert!(shell.chrome().is_content_verified());
    }

    #[test]
    fn the_verified_posture_does_not_leak_into_a_later_served_load() {
        // The indicator must track the CURRENT page: after a content-verified load,
        // navigating to a plain served origin resets the chrome to the untrusted
        // posture (a fresh navigation begins unverified until proven otherwise).
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ipfs://bafyfixturecid/").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert!(shell.chrome().is_content_verified());

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "the verified posture does not leak onto a later plain served load"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_chrome_shows_the_name_via_trusted_rpc_posture_for_an_ens_resolved_load() {
        // Acceptance: an ENS-resolved Phase-1 page surfaces the DISTINCT
        // name-via-trusted-RPC posture in the chrome — tracking the ACTUAL load
        // path, not the URL: the posture only flips after the load actually goes
        // through ENS trusted-RPC resolution (a `.eth`-looking URL is not enough).
        // It is honestly NOT content-verified (Phase 1 has no light client).
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("ens://ronan.eth")
            .expect("a name load is navigable through the seam");
        // Before the trusted-RPC resolution feeds a CID into the verified path, the
        // load is untrusted — the URL looking like a name is NOT enough.
        shell.pump();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "a name URL is not name-via-trusted-RPC until it actually resolves"
        );

        // The front door resolves the name over the trusted RPC and marks the
        // load; then it settles.
        handle.serve_via_ens_trusted_rpc();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "the ENS trusted-RPC resolution surfaces the name-via-trusted-RPC posture"
        );
        assert!(shell.chrome().is_name_via_trusted_rpc());
        // It is NEVER surfaced as verified: Phase 1 makes no name-verification claim.
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_name_via_trusted_rpc_posture_does_not_leak_into_a_later_served_load() {
        // The indicator must track the CURRENT page: after an ENS-resolved load,
        // navigating to a plain served origin resets the chrome to the untrusted
        // posture (a fresh navigation begins unverified until proven otherwise).
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ens://ronan.eth").unwrap();
        handle.serve_via_ens_trusted_rpc();
        settle(&mut shell, &handle);
        assert!(shell.chrome().is_name_via_trusted_rpc());

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "the name-via-trusted-RPC posture does not leak onto a later plain served load"
        );
        assert!(!shell.chrome().is_name_via_trusted_rpc());
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_shell_forwards_focus_through_the_seam() {
        // Acceptance: the shell makes the page interactive THROUGH the seam. It
        // focuses the live view via the seam (how the embedded webview widget
        // receives real OS scroll/click/focus/keyboard input). We assert the CALL
        // crosses the seam, not that the webview's no-ops move anything (per the
        // task's forward-pointer).
        let (mut shell, handle) = shell_with_backend();
        shell.focus_page(true);
        assert_eq!(
            handle.focus_calls(),
            [true],
            "focus was forwarded via the seam"
        );
    }
}
