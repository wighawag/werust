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

use std::collections::HashMap;

use renderer::{LoadEvent, LoadState, Renderer, RendererError, TrustPosture};

use fetcher::{HttpFetcher, DEFAULT_CONNECT_TIMEOUT, DEFAULT_IPNS_RECORD_TIMEOUT};

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

/// How the scheme-less URL-bar front door should route a NON-`.eth` entry: a
/// plausible host/URL werust should TRY, or garbage it should REFUSE without
/// navigating.
///
/// This is the shared, unit-tested classification the field finding
/// (`work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`,
/// finding D) calls for: a scheme-less `github.com` must NAVIGATE (as
/// `https://github.com`, the browser-idiomatic default), while stray garbage must
/// surface a distinct invalid-URL state rather than silently resetting the bar.
/// It lives in the toolkit-free core, a sibling to [`eth_name_from_entry`], so
/// every OS edge (desktop + the two mobile edges) shares ONE rule and it is
/// testable at the seam boundary — mirroring the `.eth`-recognition placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryRoute {
    /// The entry already carries an explicit scheme (`https://…`, `ipfs://…`,
    /// `http://…`, …): take it LITERALLY, never prepend or hijack a scheme.
    ExplicitScheme,
    /// A scheme-less string that parses as a plausible host/authority (`github.com`,
    /// `example.com/path`, `localhost:8080`, an IP): navigate it with `https://`
    /// prepended (the browser-idiomatic default for a bare host).
    HttpsCandidate,
    /// Neither a bare `.eth` name nor a parseable host/URL (a stray token, a
    /// string with whitespace, empty): do NOT navigate — surface the invalid-URL
    /// state and keep the typed text for the user to fix.
    Invalid,
}

/// Classify a NON-`.eth` URL-bar `entry` (the caller has already peeled off the
/// bare-`.eth` name via [`eth_name_from_entry`]) into an [`EntryRoute`].
///
/// The rule is deliberately CONSERVATIVE and HONEST, not a full URL-spec parser
/// (recorded in `docs/spikes/scheme-less-entry-https-fallback-and-keep-bar-on-error/DECISIONS.md`):
///
/// * An entry with an explicit `scheme://` is [`ExplicitScheme`](EntryRoute::ExplicitScheme):
///   taken literally so `ipfs://…`/`http://…`/`https://…` are never re-prefixed
///   or hijacked. (A bare `scheme://` with an empty scheme or empty rest is NOT
///   an explicit scheme — it falls through to the host check, which rejects it.)
/// * A scheme-less entry is a [`HttpsCandidate`](EntryRoute::HttpsCandidate) iff
///   its AUTHORITY (the part before the first `/`, `?`, or `#`) is a plausible
///   host: `localhost` (optionally `:port`), or a dotted host (`a.b`, at least
///   one internal `.` with non-empty labels on both sides — so `github.com` and
///   `example.com` pass, a bare `garbage` does not). No spaces anywhere in the
///   entry, no control characters.
/// * Everything else is [`Invalid`](EntryRoute::Invalid): empty, whitespace, a
///   bare single token with no dot, or a malformed authority.
fn classify_entry(entry: &str) -> EntryRoute {
    // An explicit scheme wins and is taken literally (never re-prefixed): a
    // non-empty scheme followed by `://` and a non-empty rest, matching the
    // backends' `validate_url` shape so an entry that already passes there is
    // routed here as an explicit scheme.
    if let Some((scheme, rest)) = entry.split_once("://") {
        if !scheme.is_empty() && !rest.is_empty() {
            return EntryRoute::ExplicitScheme;
        }
    }
    // A scheme-less entry: it must have no whitespace/control chars (a URL never
    // does; a string with a space is a search/garbage token, not a host).
    if entry.is_empty() || entry.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return EntryRoute::Invalid;
    }
    // The AUTHORITY is the part before the first path/query/fragment separator.
    let authority = entry
        .split(['/', '?', '#'])
        .next()
        .expect("split always yields at least one element");
    if is_plausible_authority(authority) {
        EntryRoute::HttpsCandidate
    } else {
        EntryRoute::Invalid
    }
}

/// Whether `authority` (a scheme-less entry's host[:port], userinfo already
/// disallowed by the no-`@` check) is a PLAUSIBLE host to try over `https://`.
///
/// Conservative and honest (see [`classify_entry`]): `localhost` (bare or with a
/// numeric port), or a DOTTED host (`example.com`, an IPv4 literal) — at least
/// one internal `.` with a non-empty label on each side. A bare single token with
/// no dot (`garbage`) is rejected so a typo does not silently become
/// `https://garbage`. Kept intentionally pragmatic; it is not a hostname-grammar
/// validator (the backend + the network are the final arbiters of a real host).
fn is_plausible_authority(authority: &str) -> bool {
    if authority.is_empty() {
        return false;
    }
    // Split off an optional `:port`; reject userinfo (`user@host`) and any other
    // stray `@`/`:` shapes so only a clean `host[:port]` passes.
    if authority.contains('@') {
        return false;
    }
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    // A port, when present, must be a non-empty run of ASCII digits.
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    if host.is_empty() {
        return false;
    }
    // `localhost` is the one dotless host we accept (the common local dev target).
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // Otherwise require a dotted host: at least one internal `.` with a non-empty
    // label on both sides (so `a.b` passes, `.com`/`example.`/`example` do not).
    let mut labels = host.split('.');
    let has_multiple = host.contains('.');
    has_multiple && labels.all(|label| !label.is_empty())
}

/// Which STEP of the real resolution/fetch pipeline a load is currently in, for
/// the live loading-progress indicator.
///
/// This is a THIRD chrome axis, orthogonal to [`LoadState`] (the backend's
/// lifecycle truth) and [`TrustPosture`] (the load path): it says WHICH stage of
/// werust's own `name -> record -> content -> render` pipeline is running right
/// now, so a slow load reads as "working: fetching content" rather than frozen.
/// It is driven by ACTUAL lifecycle events (the ENS/IPNS resolution steps in
/// [`BrowserShell::navigate_ens_name`], then the backend's
/// `Started`/`Committed`/`Finished`), never faked, and is
/// [`Idle`](LoadStep::Idle) whenever nothing is loading.
///
/// It deliberately does NOT re-mean [`LoadState`]: the shell's multi-stage
/// resolution (resolve a name, fetch+verify a record) happens BEFORE the
/// backend's single `navigate` for the resolved `ipfs://<cid>` even starts, so a
/// name-resolution step is not a backend load-state. Loading/error stay
/// orthogonal to the trust posture (the prior tasks' invariant). The chosen model
/// is recorded in `docs/spikes/clearer-loading-and-error-indicator/DECISIONS.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadStep {
    /// No load is in flight: nothing to show a step for (a settled / idle /
    /// failed chrome). The DEFAULT.
    #[default]
    Idle,
    /// Resolving a name to its content pointer: the ENS step (namehash ->
    /// registry -> resolver -> contenthash over the trusted RPC). The FIRST step
    /// of a bare `.eth` load.
    ResolvingName,
    /// Fetching + client-verifying the signed IPNS record that maps a MUTABLE
    /// name to its current CID (the extra round-trip an `ipns-ns` name adds
    /// before any content). Only in an IPNS-backed load.
    FetchingRecord,
    /// Fetching the content itself: the resolved (or directly typed)
    /// `ipfs://<cid>` / `http(s)://` main resource is loading through the backend
    /// (the hash-verified content-addressed path for `ipfs://`). The backend load
    /// has [`Started`](LoadState::Started) but not yet committed.
    FetchingContent,
    /// Rendering: the main resource has committed
    /// ([`Committed`](LoadState::Committed)) and the page is being laid out /
    /// painted, the final step before the load finishes.
    Rendering,
}

impl LoadStep {
    /// A short, human-readable hint for this step, for the loading indicator's
    /// status text ("resolving name", "fetching content", …). Empty for
    /// [`Idle`](LoadStep::Idle) (no load to describe), so a caller can append it
    /// to a spinner only while there is a step to show.
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            LoadStep::Idle => "",
            LoadStep::ResolvingName => "resolving name",
            LoadStep::FetchingRecord => "fetching record",
            LoadStep::FetchingContent => "fetching content",
            LoadStep::Rendering => "rendering",
        }
    }

    /// The stable, lower-kebab wire name of this step for the FFI chrome JSON, so
    /// the mobile edges paint the SAME step hint from the SAME fact (mirroring the
    /// trust-posture wire names).
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            LoadStep::Idle => "idle",
            LoadStep::ResolvingName => "resolving-name",
            LoadStep::FetchingRecord => "fetching-record",
            LoadStep::FetchingContent => "fetching-content",
            LoadStep::Rendering => "rendering",
        }
    }
}

/// The kind of a surfaced load failure: is it a TRANSIENT/timeout failure the
/// user can simply RETRY, or a HARD failure (unsupported protocol, verification
/// failure, malformed content) that a retry will not fix?
///
/// This is a PURE classification of the honest, protocol-named reason already in
/// [`ChromeState::last_error`] — it does NOT re-mean or replace that reason (a
/// hard failure keeps its exact protocol-named text). It exists so the error
/// surface can offer a RETRY affordance for the transient case (a timeout says
/// "timed out, reload to retry" instead of the same scary hard-fail banner),
/// answering the field finding that a retryable timeout looked identical to a
/// hard fail (`work/notes/observations/field-test-v0.2.2-ipns-slow-partial-and-debug-window-2026-07-23.md`,
/// issue C).
///
/// It is derived from the reason STRING because that is the one denominator every
/// failure crosses on: the shell's typed ENS/IPNS errors AND the async
/// content/render failures the webview reports only as a
/// [`LoadEvent::Failed`] reason string. The classifier + its rationale (why the
/// string, not a typed error field) are recorded in
/// `docs/spikes/clearer-loading-and-error-indicator/DECISIONS.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// A transient failure a RETRY can fix: a timeout or a transport/connection
    /// error (the field finding's `transport error: timeout: global`, the
    /// fetcher/provider `Transport`/`Io` taxonomy, an IPNS record-fetch source
    /// failure). The load simply did not COMPLETE; the content it was after may
    /// well be reachable on a reload. The error surface offers a retry affordance.
    Transient,
    /// A hard failure a retry will NOT fix: the content is unsupported
    /// (a non-IPFS protocol), did not VERIFY (a bad IPNS signature, a block hash
    /// mismatch), is malformed, or names nothing loadable (no resolver, no
    /// contenthash, an invalid CID/target). It keeps its prominent, protocol-named
    /// reason unchanged — retrying is pointless, so no retry affordance is shown.
    Hard,
}

impl FailureKind {
    /// Classify a surfaced failure `reason` as [`Transient`](FailureKind::Transient)
    /// (retryable) or [`Hard`](FailureKind::Hard).
    ///
    /// Keys on the TRANSIENT markers a timeout/transport failure carries in its
    /// reason text — the ONE set of markers shared by BOTH failure paths (the
    /// shell's typed `ProviderError::Transport` / `IpnsError::Source` /
    /// `FetchError::Transport`|`Io`, and the webview's `LoadEvent::Failed` reason
    /// for a page-level `http` timeout). Everything else is [`Hard`](FailureKind::Hard):
    /// an unsupported protocol / verification / malformed / not-found reason is
    /// never retried, so it defaults to hard. Case-insensitive so a marker in any
    /// casing still classifies.
    #[must_use]
    pub fn classify(reason: &str) -> Self {
        let r = reason.to_ascii_lowercase();
        // A verification failure can mention "timed out"-adjacent words but is
        // NEVER transient: an expired/invalid record does not become valid on a
        // retry. So the hard markers are checked FIRST and win. ("did not verify"
        // is the IPNS/verify taxonomy; "expired" its EOL case.)
        const HARD_MARKERS: [&str; 3] = ["did not verify", "hash mismatch", "expired"];
        if HARD_MARKERS.iter().any(|m| r.contains(m)) {
            return FailureKind::Hard;
        }
        // The transient markers: a timeout or a transport/connection failure — the
        // load did not complete, so a reload may well succeed.
        const TRANSIENT_MARKERS: [&str; 5] = [
            "timeout",
            "timed out",
            "transport error",
            "connection",
            "io error",
        ];
        if TRANSIENT_MARKERS.iter().any(|m| r.contains(m)) {
            FailureKind::Transient
        } else {
            FailureKind::Hard
        }
    }

    /// Whether this failure kind is RETRYABLE (a reload may fix it): true for
    /// [`Transient`](FailureKind::Transient), false for [`Hard`](FailureKind::Hard).
    #[must_use]
    pub fn is_retryable(self) -> bool {
        matches!(self, FailureKind::Transient)
    }

    /// The stable, lower-kebab wire name for the FFI chrome JSON, so the mobile
    /// edges distinguish the two failure kinds from the SAME fact (mirroring the
    /// trust-posture wire names).
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            FailureKind::Transient => "transient",
            FailureKind::Hard => "hard",
        }
    }
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
    /// cleared when a new load starts. `None` when nothing has failed. Carries the
    /// honest, protocol-named reason verbatim (the resolver/decoder/fetch
    /// taxonomy); the transient-vs-hard distinction is DERIVED from it via
    /// [`failure_kind`](ChromeState::failure_kind), never by re-meaning this text.
    pub last_error: Option<String>,
    /// Which step of the real `name -> record -> content -> render` pipeline the
    /// current load is in, for the live loading-progress indicator. [`Idle`](LoadStep::Idle)
    /// whenever nothing is loading. Driven by actual lifecycle events, so a slow
    /// load shows genuine progress rather than reading as frozen.
    pub load_step: LoadStep,
    /// The [`TrustPosture`] of the current page, driving the chrome's trust
    /// indicator: content-verified vs served by an unverified origin
    /// (`docs/adr/0001`: the trust posture is a product surface). Read straight
    /// from the seam's [`Renderer::trust_posture`], so it tracks the ACTUAL load
    /// path (a page whose bytes came back through the hash-verified
    /// content-addressed path), not the URL string.
    pub trust_posture: TrustPosture,
    /// The INVALID URL-bar entry (the garbage the user typed) when the last Enter
    /// was neither a bare `.eth` name nor a parseable host/URL, `None` otherwise.
    ///
    /// This is a NEW, ORTHOGONAL chrome axis — distinct from
    /// [`last_error`](ChromeState::last_error) (a LOAD failure of a valid target)
    /// and from the [`trust_posture`](ChromeState::trust_posture) — so a
    /// scheme-less GARBAGE entry can be surfaced as a small "invalid URL" BADGE
    /// with the URL-bar text rendered invalid (red underline), while the typed
    /// text is KEPT for the user to fix and NO navigation happens and the bar is
    /// never silently reset (field finding D,
    /// `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`).
    /// It is set by [`navigate`](BrowserShell::navigate) on an invalid entry and
    /// cleared by any navigation that DOES proceed (a valid host / `.eth` /
    /// explicit scheme / back / forward / reload), so it never lingers onto a
    /// later page. Loading/error/validity stay orthogonal to trust.
    pub invalid_entry: Option<String>,
}

impl ChromeState {
    /// Whether the Stop control should be active (a load is in flight) versus the
    /// Reload control (a settled page). The window swaps/enables the two from this.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.load_state.is_loading()
    }

    /// The current pipeline step, for the loading-progress indicator (only
    /// meaningful while [`is_loading`](ChromeState::is_loading)).
    #[must_use]
    pub fn load_step(&self) -> LoadStep {
        self.load_step
    }

    /// The [`FailureKind`] of the surfaced failure, or `None` when nothing has
    /// failed: the classification of [`last_error`](ChromeState::last_error) as a
    /// TRANSIENT (retryable) timeout vs a HARD failure. A pure derivation of the
    /// reason, so the error surface can distinguish a retryable timeout from a
    /// scary hard fail without any new plumbing.
    #[must_use]
    pub fn failure_kind(&self) -> Option<FailureKind> {
        self.last_error.as_deref().map(FailureKind::classify)
    }

    /// Whether the surfaced failure is RETRYABLE (a transient timeout a reload may
    /// fix). `false` when nothing failed or when the failure is hard. The error
    /// surface shows a retry affordance exactly when this is `true`.
    #[must_use]
    pub fn failure_is_retryable(&self) -> bool {
        matches!(self.failure_kind(), Some(FailureKind::Transient))
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

    /// Whether the last URL-bar entry was INVALID (not a bare `.eth` name, not a
    /// parseable host/URL), so the chrome should show the invalid-URL badge and
    /// render the URL-bar text as invalid (red underline). A pure read of the
    /// orthogonal [`invalid_entry`](ChromeState::invalid_entry) axis — the window
    /// (and each mobile edge) paints the badge + red-underline from THIS one fact,
    /// exactly as it paints the trust indicator from the posture.
    #[must_use]
    pub fn has_invalid_entry(&self) -> bool {
        self.invalid_entry.is_some()
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
    ///
    /// This pins the name during an ACTIVE front-door load (the initial Enter, a
    /// reload re-resolve, or a failed resolution where there is no backend URL to
    /// fall back to). Preserving the name across BACK/FORWARD onto an EXISTING
    /// history entry is the job of [`ens_pages`](BrowserShell::ens_pages) instead
    /// (the shell keeps no URL stack, so it re-derives the name from the entry's
    /// underlying CID).
    url_override: Option<String>,
    /// The NORMALIZED CID key of the resolved-root `ipfs://<cid>` the current
    /// `url_override` name was pinned FOR (via
    /// [`crate::ipfs::normalize_ens_page_key`]), or [`None`] when nothing is
    /// pinned or the pin is not for a resolved-root entry (a failed ENS load / an
    /// invalid entry pins the typed text with no backend URL).
    ///
    /// This is what distinguishes PINNING the `.eth` name for the front-door ROOT
    /// load from FOLLOWING the backend URL as the user navigates WITHIN the page.
    /// The pin holds while the load stays on the resolved root (its lifecycle
    /// events carry the root CID), but an IN-PAGE navigation (a link click) is a
    /// FRESH backend load whose event URL normalizes to a DIFFERENT key, so
    /// [`pump`](BrowserShell::pump) drops the pin and the bar follows the backend
    /// URL. The ROOT entry stays recoverable: it is in
    /// [`ens_pages`](BrowserShell::ens_pages), so a back/forward return to it
    /// re-derives its `.eth` name + posture off the normalized key
    /// ([`ens-history-name-rederive-async-and-normalized`]). The pin-vs-follow
    /// decision is recorded in
    /// `docs/spikes/urlbar-tracks-in-page-navigation-not-just-pinned-name/pin-vs-follow-decision.md`.
    pinned_root_key: Option<String>,
    /// The association from a backend underlying URL (a resolved `ipfs://<cid>`)
    /// to the ENS identity that produced it, so reload / back / forward onto an
    /// ENS-originated entry can RE-DERIVE the `.eth` name + its trust posture
    /// instead of leaking the raw CID.
    ///
    /// The shell keeps NO URL stack of its own (session history is the backend's,
    /// via [`Renderer::go_back`]/[`go_forward`](Renderer::go_forward)), so this is
    /// the minimal state that lets the shell recognise, when the backend's
    /// `current_url` lands on a CID it once resolved from a name, that the entry is
    /// ENS-originated: [`refresh_chrome`](BrowserShell::refresh_chrome) then shows
    /// the `.eth` name in the bar and RE-MARKS the load's ENS posture axes
    /// ([`Renderer::mark_ens_origin`] / [`mark_mutable_name`](Renderer::mark_mutable_name))
    /// so the verified content path surfaces `NameViaTrustedRpc` / `MutableName`,
    /// not the plain `ContentVerified` a bare CID would show. Populated by
    /// [`load_resolved_content`](BrowserShell::load_resolved_content); a non-ENS
    /// entry is never in the map, so a plain page is wholly unaffected.
    ///
    /// See `work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md`
    /// for the reload (re-resolve) + history (re-derive) decision.
    ens_pages: HashMap<String, EnsIdentity>,
    /// The PRE-CONTENT pipeline step the shell is currently in, if any: the
    /// synchronous ENS/IPNS resolution stages ([`LoadStep::ResolvingName`] /
    /// [`LoadStep::FetchingRecord`]) that run BEFORE the backend's `navigate` for
    /// the resolved `ipfs://<cid>`. `None` once resolution hands off to the
    /// backend (or for a plain, non-ENS load), so
    /// [`refresh_chrome`](BrowserShell::refresh_chrome) then derives the CONTENT
    /// step ([`FetchingContent`](LoadStep::FetchingContent) /
    /// [`Rendering`](LoadStep::Rendering)) from the backend's load state. This is
    /// what lets a resolution-phase FAILURE surface the step it failed at.
    resolving_step: Option<LoadStep>,
}

/// The ENS identity behind an underlying `ipfs://<cid>` load: the `.eth` name to
/// show in the bar, and whether the name is MUTABLE (an `ipns-ns` / repointable
/// name), so the right posture axes can be re-marked on a reload / history move.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EnsIdentity {
    /// The `.eth` name the user typed, kept in the URL bar in place of the CID.
    name: String,
    /// Whether the resolved name is MUTABLE (`ipns-ns`), so a history/reload load
    /// re-marks the mutable axis too ([`Renderer::mark_mutable_name`]).
    mutable: bool,
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
        //
        // The record fetch is a SMALL single signed-record GET, a distinct step
        // from the (larger, slower) content fetch it precedes, so it uses the
        // SPLIT-OUT `DEFAULT_IPNS_RECORD_TIMEOUT` (shorter than the content
        // path's `DEFAULT_GLOBAL_TIMEOUT`) with the SAME tight connect bound: a
        // cold-but-progressing record lookup is not killed, a dead gateway still
        // fails fast, and the record step does not eat the content step's budget
        // (`fetch-timeout-raise-and-split-for-ipns-and-content`).
        let ipns_source = Box::new(GatewayIpnsRecordSource::with_gateway(
            HttpFetcher::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_IPNS_RECORD_TIMEOUT),
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
            pinned_root_key: None,
            ens_pages: HashMap::new(),
            resolving_step: None,
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

    /// Navigate to `url` (the URL bar's Enter action), through the seam, routing a
    /// scheme-less entry three ways: ENS name, `https://` candidate, or invalid.
    ///
    /// The single front door applies the shared, unit-tested routing so it is
    /// consistent across all platforms (field finding D,
    /// `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`):
    ///
    /// 1. A bare `.eth` URL-bar entry (no scheme, like Brave/Opera — see
    ///    [`eth_name_from_entry`]) is the ENS FRONT DOOR: it is resolved to an
    ///    immutable `ipfs://<cid>` and loaded through the existing verified
    ///    `ipfs://` path via
    ///    [`navigate_ens_name`](BrowserShell::navigate_ens_name), keeping the
    ///    `.eth` name in the bar and marking the load "content-verified, name via
    ///    trusted RPC". Unchanged.
    /// 2. Else a VALID host/URL ([`classify_entry`]): an explicit-scheme entry is
    ///    navigated LITERALLY (no double scheme, no `ipfs://`/`http://` hijack); a
    ///    scheme-less plausible host (`github.com`, `localhost:8080`) is navigated
    ///    with `https://` PREPENDED (the browser-idiomatic default). If the LOAD
    ///    then fails (DNS/unreachable), that surfaces as a normal in-page browser
    ///    error via [`last_error`](ChromeState::last_error) while the bar KEEPS the
    ///    attempted URL — a load failure of a valid target, handled by
    ///    [`pump`](BrowserShell::pump), not here.
    /// 3. Else INVALID (a stray token, whitespace, garbage): do NOT navigate.
    ///    Surface the distinct invalid-URL state
    ///    ([`invalid_entry`](ChromeState::invalid_entry)) so the chrome shows an
    ///    "invalid URL" badge + the URL-bar text rendered invalid (red underline),
    ///    KEEPING the typed text for the user to fix. The bar is NEVER silently
    ///    reset to the previous page, and `navigate` returns `Ok(())` (the front
    ///    door handled the entry and surfaced the state in the chrome), not an
    ///    `Err`.
    pub fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        if let Some(name) = eth_name_from_entry(url) {
            return self.navigate_ens_name(name);
        }
        // A NON-`.eth` entry: classify it into try-it (explicit scheme or a
        // scheme-less plausible host) vs refuse-it (garbage).
        let target = match classify_entry(url) {
            // Take an explicit scheme literally: no re-prefixing, no hijack of
            // `ipfs://`/`http://`/`https://`.
            EntryRoute::ExplicitScheme => url.to_string(),
            // A scheme-less plausible host: prepend the browser-idiomatic
            // `https://` default (so `github.com` loads `https://github.com`).
            EntryRoute::HttpsCandidate => format!("https://{url}"),
            // Garbage: do NOT navigate. Surface the distinct invalid-URL state and
            // KEEP the typed text in the bar for the user to fix — never reset the
            // bar to the previous page.
            EntryRoute::Invalid => {
                self.fail_invalid_entry(url);
                return Ok(());
            }
        };
        self.renderer.navigate(&target)?;
        // A plain navigation follows the backend's URL: drop any ENS name that was
        // pinned in the bar so it never lingers on a later page. It is a CONTENT
        // load (no ENS/IPNS resolution step), so clear any pinned resolution step;
        // `refresh_chrome` derives the content step from the backend's load state.
        // A proceeding navigation also clears any prior invalid-entry state.
        self.url_override = None;
        self.pinned_root_key = None;
        self.resolving_step = None;
        self.chrome.last_error = None;
        self.chrome.invalid_entry = None;
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
        // The ENS front door proceeds, so any prior invalid-entry state is cleared
        // (a valid route never leaves the badge showing).
        self.chrome.invalid_entry = None;
        // Step 1 of the pipeline: resolving the name (namehash -> registry ->
        // resolver -> contenthash). Pin the step so a resolution FAILURE surfaces
        // "resolving name" as the stage it failed at, and so a caller inspecting
        // mid-resolution sees genuine progress.
        self.resolving_step = Some(LoadStep::ResolvingName);
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
                // Step 2 (IPNS names only): fetch + client-verify the signed
                // record before any content. Pin the step so a record
                // fetch/verify failure surfaces "fetching record".
                self.resolving_step = Some(LoadStep::FetchingRecord);
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
        // Resolution is done; the backend now drives the CONTENT step. Hand the
        // step off to the backend's load state (via `refresh_chrome`).
        self.resolving_step = None;
        if let Err(e) = self.renderer.navigate(uri) {
            // A backend that cannot even start the content load failed at the
            // content step, not resolution.
            self.resolving_step = None;
            self.fail_ens_load(name, &e.to_string());
            return;
        }
        self.renderer.mark_ens_origin();
        if mutable {
            self.renderer.mark_mutable_name();
        }
        // Remember the CID <-> name association so a later reload / back / forward
        // that lands the backend on this same underlying `ipfs://<cid>` can
        // re-derive the `.eth` name + re-mark the posture axes, instead of leaking
        // the raw CID (`refresh_chrome`). Keyed on the NORMALIZED CID form
        // (`crate::ipfs::normalize_ens_page_key`), applied IDENTICALLY here and at
        // every lookup, so the authority form we store now (`ipfs://<cid>`) still
        // matches the authority-less `ipfs:///<cid>` WebKitGTK reports for the SAME
        // entry after a back/forward. Keying on the raw display string was the
        // v0.2.3 regression: the stored and post-back strings differed and the
        // re-derive missed, leaking the CID into the bar.
        // Record the NORMALIZED key of the resolved root this name is pinned FOR,
        // so `pump` can tell the front-door root load (whose lifecycle events carry
        // this same CID) from an IN-PAGE navigation off it (a link click, a fresh
        // load whose event URL normalizes to a DIFFERENT key) and drop the pin only
        // for the latter. `None` when the backend did not start a load.
        self.pinned_root_key = self
            .renderer
            .current_url()
            .map(|current| crate::ipfs::normalize_ens_page_key(&current));
        if let Some(current) = self.renderer.current_url() {
            self.ens_pages.insert(
                crate::ipfs::normalize_ens_page_key(&current),
                EnsIdentity {
                    name: name.to_string(),
                    mutable,
                },
            );
        }
        // Keep the front-door NAME the user typed in the bar (no `https://`
        // rewrite, no gateway redirect). The override PERSISTS across pumps so the
        // name stays put for the whole load — until the user navigates OFF the
        // resolved root (an in-page link click), which `pump` detects by the
        // event URL's normalized key differing from `pinned_root_key`.
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
        // backend anywhere, so there is no underlying URL to fall back to). The
        // load has SETTLED (failed), so no step is in flight: clear the pinned
        // resolution step BEFORE refreshing so the failed chrome shows the `Idle`
        // step, and so it never lingers onto the next load.
        self.resolving_step = None;
        self.url_override = Some(name.to_string());
        // A failed ENS load never navigated the backend, so there is no resolved
        // root to follow off; the pin holds the name until the next navigation.
        self.pinned_root_key = None;
        self.refresh_chrome();
        self.chrome.last_error = Some(reason.to_string());
    }

    /// Refuse an INVALID URL-bar `entry` without navigating: surface the distinct
    /// invalid-URL state and KEEP the typed text in the bar for the user to fix.
    ///
    /// This is the field-finding-D refusal (finding D,
    /// `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`):
    /// a scheme-less GARBAGE entry (neither a bare `.eth` name nor a parseable
    /// host/URL) does NOT navigate the backend to anything. It sets the orthogonal
    /// [`invalid_entry`](ChromeState::invalid_entry) axis so the chrome paints the
    /// "invalid URL" badge + the red-underlined URL-bar text, PINS the typed text
    /// via `url_override` so it stays in the bar (the bar is never silently reset
    /// to the previous page), and leaves [`last_error`](ChromeState::last_error)
    /// UNTOUCHED — an invalid entry is NOT a load failure (the two axes stay
    /// distinct). No backend navigation happened, so the load state / trust
    /// posture keep the previous page's settled values.
    fn fail_invalid_entry(&mut self, entry: &str) {
        // No load: clear any in-flight resolution step. Pin the typed text so the
        // bar keeps it (there is no backend URL, and no reset to the prior page).
        self.resolving_step = None;
        self.url_override = Some(entry.to_string());
        // No backend load, so no resolved root to follow off.
        self.pinned_root_key = None;
        self.refresh_chrome();
        // Set the invalid-entry axis AFTER refresh (like `fail_ens_load` sets
        // `last_error`), so `refresh_chrome`'s URL logic runs with the pinned text
        // and the badge fact is the final word.
        self.chrome.invalid_entry = Some(entry.to_string());
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
        self.pinned_root_key = None;
        self.resolving_step = None;
        self.chrome.last_error = None;
        // A history move proceeds, so any prior invalid-entry badge is cleared.
        self.chrome.invalid_entry = None;
        self.refresh_chrome();
    }

    /// Go one step forward in session history, through the seam.
    pub fn go_forward(&mut self) {
        self.renderer.go_forward();
        self.url_override = None;
        self.pinned_root_key = None;
        self.resolving_step = None;
        self.chrome.last_error = None;
        self.chrome.invalid_entry = None;
        self.refresh_chrome();
    }

    /// Reload the current page, through the seam.
    ///
    /// For an ENS-originated page (a bare `.eth` that resolved to a CID) reload
    /// RE-RESOLVES the name through the front door
    /// ([`navigate_ens_name`](BrowserShell::navigate_ens_name)) rather than
    /// re-loading the cached CID: reload means "get the current version", so for a
    /// MUTABLE name (`ipns-ns`, or a repointable ENS name) it catches a changed
    /// pointer, and for an immutable `ipfs-ns` name it re-derives the same CID.
    /// Either way the `.eth` name stays pinned in the bar and its ENS posture
    /// (`NameViaTrustedRpc` / `MutableName`) is preserved — never the raw
    /// `ipfs://<cid>` or the plain `ContentVerified` a bare CID would show. (The
    /// re-resolve + history re-derive decision, and its history side-effect, are
    /// recorded in
    /// `work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md`.)
    ///
    /// A plain (non-ENS) page reloads the backend's current underlying URL as
    /// before; the reloaded content-addressed page is still hash-verified by the
    /// `ipfs://` path, so it shows honestly as content-verified.
    pub fn reload(&mut self) -> Result<(), RendererError> {
        // If the current entry is an ENS-originated page, re-resolve its `.eth`
        // name (the recorded reload decision) so a mutable name refreshes and the
        // name + posture stay in the bar. The shell keeps no URL stack, so the
        // current entry is recognised by its underlying CID via `ens_pages`.
        let ens_name = self
            .renderer
            .current_url()
            .and_then(|url| {
                self.ens_pages
                    .get(&crate::ipfs::normalize_ens_page_key(&url))
                    .map(|e| e.name.clone())
            })
            // A FAILED ENS load never navigated the backend (no `current_url`), but
            // still pinned the name in the bar; reloading it re-runs the resolution
            // from that pinned name, so a transient failure is retryable.
            .or_else(|| {
                self.url_override
                    .as_deref()
                    .and_then(eth_name_from_entry)
                    .map(str::to_string)
            });
        if let Some(name) = ens_name {
            return self.navigate_ens_name(&name);
        }
        self.renderer.reload()?;
        self.url_override = None;
        self.pinned_root_key = None;
        self.resolving_step = None;
        self.chrome.last_error = None;
        // A reload proceeds, so any prior invalid-entry badge is cleared.
        self.chrome.invalid_entry = None;
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

    /// Drop the pinned `.eth` name if a lifecycle event's `event_url` is an
    /// IN-PAGE navigation OFF the resolved root the name was pinned for.
    ///
    /// The ENS front door pins the `.eth` name for the resolved-ROOT load only
    /// (recorded in
    /// `docs/spikes/urlbar-tracks-in-page-navigation-not-just-pinned-name/pin-vs-follow-decision.md`).
    /// The pinned root's own lifecycle events carry that root's CID, so they keep
    /// the pin; but an IN-PAGE navigation (a link click within the page) is a
    /// FRESH backend load whose `event_url` normalizes to a DIFFERENT key. When it
    /// does, the user has navigated WITHIN/away, so drop the pin and let the bar
    /// FOLLOW the backend URL. The root entry is still recoverable via `ens_pages`
    /// on a history return, so nothing is lost. A no-op when nothing is pinned or
    /// the pin has no resolved root (a failed ENS load / an invalid entry pins the
    /// typed text with no backend URL to follow off).
    fn drop_pin_on_in_page_nav(&mut self, event_url: &str) {
        let Some(root_key) = &self.pinned_root_key else {
            return;
        };
        if crate::ipfs::normalize_ens_page_key(event_url) != *root_key {
            // Navigated off the resolved root: follow the backend URL from here.
            self.url_override = None;
            self.pinned_root_key = None;
        }
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
            // An IN-PAGE navigation off the pinned ENS root (a link click) is a
            // FRESH backend load whose event URL normalizes to a DIFFERENT key than
            // the resolved root the name was pinned FOR. When that happens, the
            // user has navigated WITHIN/away, so drop the pin here and let the bar
            // FOLLOW the backend URL (the pin-vs-follow decision). The ROOT entry
            // stays recoverable via `ens_pages` on a history return. A load whose
            // URL is the pinned root (the front-door root still loading) keeps the
            // pin, so the name holds for the whole root load.
            self.drop_pin_on_in_page_nav(event.url());
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
        // The live pipeline step: a pinned PRE-CONTENT resolution step
        // (`ResolvingName` / `FetchingRecord`) wins while the shell is still
        // resolving a name (before the backend's `navigate`); once resolution has
        // handed off (or for a plain load), the CONTENT step is derived from the
        // backend's load state — `Started` is fetching, `Committed` is rendering,
        // and a settled/failed/idle load has no step. Driven by ACTUAL lifecycle,
        // never faked, so a slow load shows genuine progress.
        self.chrome.load_step = match self.resolving_step {
            Some(step) => step,
            None => match self.renderer.load_state() {
                LoadState::Started => LoadStep::FetchingContent,
                LoadState::Committed => LoadStep::Rendering,
                LoadState::Idle | LoadState::Finished | LoadState::Failed => LoadStep::Idle,
            },
        };
        // If the backend's current entry is an ENS-originated CID we resolved
        // earlier (a reload / back / forward landed on it, so there is no active
        // `url_override` pinning the name), RE-MARK the load's ENS posture axes on
        // the seam. The backend reset the flags on the fresh Started, so without
        // this the verified content path would surface a bare-CID `ContentVerified`
        // instead of the entry's real `NameViaTrustedRpc` / `MutableName`. Marking
        // is idempotent (a plain bool set on the lifecycle), so re-marking on every
        // pump keeps the axes set for the whole (async) reloaded/history load.
        let ens_entry = self.renderer.current_url().and_then(|url| {
            self.ens_pages
                .get(&crate::ipfs::normalize_ens_page_key(&url))
                .cloned()
        });
        if let Some(entry) = &ens_entry {
            self.renderer.mark_ens_origin();
            if entry.mutable {
                self.renderer.mark_mutable_name();
            }
        }
        // The trust posture is the backend's truth about the current load path
        // (content-verified vs served), pulled fresh like the load state so the
        // indicator tracks the page actually shown — including after a scheme
        // handler verifies the bytes mid-load, which flips the posture without a
        // queued LoadEvent. (Read AFTER any re-mark above so a re-decorated ENS
        // history entry surfaces its ENS posture, not the bare-CID one.)
        self.chrome.trust_posture = self.renderer.trust_posture();
        // A pinned ENS name (`url_override`) is the DISPLAY identity for the bar
        // and wins over the backend's underlying `current_url` (the resolved
        // `ipfs://<cid>`): the user keeps seeing `ronan.eth`, never the CID or a
        // gateway URL. Failing that, a reload / back / forward that landed on a
        // known ENS-originated CID re-derives the `.eth` name from `ens_pages`
        // (the shell keeps no URL stack, so the entry's name is re-derived from its
        // CID). Otherwise the bar follows the backend's URL (redirects, history
        // moves onto a plain, non-ENS entry).
        if let Some(name) = &self.url_override {
            self.chrome.url_text = name.clone();
        } else if let Some(entry) = &ens_entry {
            self.chrome.url_text = entry.name.clone();
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
        /// Each entry stores the WebKit-NORMALIZED display URL (see
        /// [`webkit_normalize`]), so a history move reports the same
        /// authority-less `ipfs:///<cid>` form the real WebKitGTK does, DIFFERENT
        /// from the authority `ipfs://<cid>` string stored at forward-load time.
        history: Vec<String>,
        cursor: Option<usize>,
        /// The URL the backend currently REPORTS as `current_url`, modelling
        /// WebKitGTK's shared `LoadLifecycle` that the `load-changed` signals
        /// update ASYNCHRONOUSLY on the GTK loop. A fresh `navigate` reflects its
        /// target here optimistically (the real backend's `begin(url)`), but a
        /// `go_back`/`go_forward` does NOT: it stays on the PREVIOUS entry until a
        /// simulated load signal (`settle_pending_history`) lands it on the target
        /// entry - exactly the async lag that hid the v0.2.3 regression from the
        /// old synchronous fake. `None` before any navigation.
        reported_url: Option<String>,
        /// A history move (`go_back`/`go_forward`) whose async settle has not
        /// landed yet: the `history` index the backend WILL report once a load
        /// signal turns. `reported_url` lags on the old entry until then, so the
        /// shell's synchronous post-`go_back` `refresh_chrome` sees the OLD url and
        /// must re-derive later on the settled pump.
        pending_history: Option<usize>,
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
        /// The URL the backend REPORTS as its current entry - the async-lagging
        /// `reported_url`, NOT the raw `history[cursor]`. Right after a
        /// `go_back`/`go_forward` this still names the PREVIOUS entry (the history
        /// move has not settled), which is exactly the real WebKitGTK behaviour the
        /// old synchronous fake failed to model.
        fn current(&self) -> Option<&String> {
            self.reported_url.as_ref()
        }

        /// Land a pending async history move onto its target entry: the backend now
        /// REPORTS that entry's (WebKit-normalized) URL as `current_url`. Called by
        /// a simulated load signal (`settle`/`drive_to_*`), modelling the
        /// `load-changed` signal that finally settles `current_url` on the GTK
        /// loop. A no-op when there is no pending history move (a fresh navigate
        /// already reflected its URL optimistically).
        fn settle_pending_history(&mut self) {
            if let Some(idx) = self.pending_history.take() {
                if let Some(url) = self.history.get(idx).cloned() {
                    self.reported_url = Some(url);
                }
            }
        }
    }

    /// Mimic WebKitGTK's normalization of an authority-less `ipfs://<cid>` URL:
    /// WebKit treats the CID as a PATH under an empty authority and re-reports the
    /// entry as `ipfs:///<cid>` (triple slash). This is the exact display-string
    /// variance that made the shell's raw-string `ens_pages` lookup miss after a
    /// back/forward in v0.2.3; the fake now reproduces it so the regression can no
    /// longer pass on an identical-string fake. A non-`ipfs://` URL is unchanged.
    fn webkit_normalize(url: &str) -> String {
        match url.strip_prefix("ipfs://") {
            // Already authority-less (`ipfs:///...`): leave it.
            Some(rest) if rest.starts_with('/') => url.to_string(),
            // Authority form `ipfs://<cid>...` -> authority-less `ipfs:///<cid>...`.
            Some(rest) => format!("ipfs:///{rest}"),
            None => url.to_string(),
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
            // A load signal turning on the GTK loop is where an async history move
            // finally settles `current_url` onto the target entry (its normalized
            // form), so drive that first, then report the settled URL.
            b.settle_pending_history();
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
            b.settle_pending_history();
            let url = b.current().expect("a load in flight").clone();
            b.state = LoadState::Failed;
            b.events.push_back(LoadEvent::Failed {
                url,
                reason: reason.to_string(),
            });
        }

        /// Simulate an IN-PAGE navigation the way a real webview delivers it: the
        /// user clicks a link inside the current page, so WebKitGTK begins a FRESH
        /// load and fires its `load-changed` signals for `url` WITHOUT the shell
        /// ever calling [`Renderer::navigate`]. It pushes a new history entry (in
        /// the WebKit-normalized display form), settles `current_url` onto it, and
        /// resets the per-load posture/flags exactly as a fresh `navigate` does, so
        /// an in-page move to a non-ENS resource starts UNVERIFIED and carries no
        /// stale ENS flag. This is the path the old fake could not model (it only
        /// exposed `navigate`, which the shell drives), and it is exactly where the
        /// pinned `.eth` name used to freeze the bar.
        fn navigate_in_page(&self, url: &str) {
            let mut b = self.inner.borrow_mut();
            // A fresh in-page load from mid-history drops the forward entries, just
            // like `navigate`.
            let next = b.cursor.map_or(0, |c| c + 1);
            b.history.truncate(next);
            b.history.push(webkit_normalize(url));
            b.cursor = Some(b.history.len() - 1);
            // The webview reports the new URL as it loads (no history-move async
            // lag: this is a forward load, not a back/forward).
            b.reported_url = Some(url.to_string());
            b.pending_history = None;
            b.state = LoadState::Started;
            b.posture = TrustPosture::UnverifiedOrigin;
            b.ens_origin = false;
            b.mutable_name = false;
            b.events.push_back(LoadEvent::Started {
                url: url.to_string(),
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
            // History stores the WebKit-NORMALIZED display URL (the form the
            // backend later reports for this entry after a back/forward), while the
            // OPTIMISTIC `reported_url` below is the RAW target the caller passed -
            // exactly the real backend's `begin(url)` vs. its later settled
            // `current_url`. For an authority-less `ipfs://<cid>` these two forms
            // DIFFER, which is the normalization variance the fix must survive.
            b.history.push(webkit_normalize(url));
            b.cursor = Some(b.history.len() - 1);
            // A fresh navigate reflects its target immediately (no history-move
            // async lag) and clears any pending history move.
            b.reported_url = Some(url.to_string());
            b.pending_history = None;
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
            b.pending_history = None;
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
                    // Move the session cursor, but DO NOT settle `current_url` onto
                    // the target yet: WebKitGTK reports the previous entry until its
                    // `load-changed` signal turns on the GTK loop. `reported_url`
                    // stays put; `settle_pending_history` (driven by a simulated
                    // load signal) lands it. This is the async lag the old fake
                    // skipped by updating `current_url` synchronously.
                    b.cursor = Some(c - 1);
                    b.pending_history = Some(c - 1);
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
                    // Same async lag as `go_back`: cursor moves now, `current_url`
                    // settles only when a simulated load signal lands the pending
                    // history move.
                    b.cursor = Some(c + 1);
                    b.pending_history = Some(c + 1);
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
    fn classify_entry_routes_explicit_scheme_valid_host_and_garbage() {
        // The shared, conservative scheme-less classifier (field finding D): an
        // explicit scheme is literal; a scheme-less plausible host is an https
        // candidate; everything else is invalid.
        //
        // Explicit scheme -> literal (never re-prefixed / hijacked).
        assert_eq!(
            classify_entry("https://example.com/"),
            EntryRoute::ExplicitScheme
        );
        assert_eq!(
            classify_entry("http://example.com"),
            EntryRoute::ExplicitScheme
        );
        assert_eq!(classify_entry("ipfs://bafycid"), EntryRoute::ExplicitScheme);
        assert_eq!(
            classify_entry("ens://ronan.eth"),
            EntryRoute::ExplicitScheme
        );
        // Scheme-less plausible host -> https candidate.
        assert_eq!(classify_entry("github.com"), EntryRoute::HttpsCandidate);
        assert_eq!(
            classify_entry("example.com/path"),
            EntryRoute::HttpsCandidate
        );
        assert_eq!(
            classify_entry("example.com/a/b?q=1#frag"),
            EntryRoute::HttpsCandidate
        );
        assert_eq!(classify_entry("localhost:8080"), EntryRoute::HttpsCandidate);
        assert_eq!(classify_entry("localhost"), EntryRoute::HttpsCandidate);
        assert_eq!(classify_entry("127.0.0.1:3000"), EntryRoute::HttpsCandidate);
        assert_eq!(
            classify_entry("a.b.c.example.com"),
            EntryRoute::HttpsCandidate
        );
        // Garbage -> invalid (no dot, whitespace, empty, malformed authority).
        assert_eq!(classify_entry("garbage"), EntryRoute::Invalid);
        assert_eq!(classify_entry("not a url"), EntryRoute::Invalid);
        assert_eq!(classify_entry(""), EntryRoute::Invalid);
        assert_eq!(classify_entry("   "), EntryRoute::Invalid);
        assert_eq!(classify_entry(".com"), EntryRoute::Invalid);
        assert_eq!(classify_entry("example."), EntryRoute::Invalid);
        assert_eq!(classify_entry("host:notaport"), EntryRoute::Invalid);
        assert_eq!(classify_entry("user@host.com"), EntryRoute::Invalid);
        // A bare `://` shape with empty scheme/rest is NOT an explicit scheme;
        // it falls to the host check, which rejects the malformed authority.
        assert_eq!(classify_entry("://nowhere"), EntryRoute::Invalid);
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
    fn navigate_refuses_an_invalid_entry_without_navigating_and_keeps_the_typed_text() {
        // Field finding D: a scheme-less GARBAGE entry (not a bare `.eth` name,
        // not a parseable host/URL) does NOT navigate. The front door now handles
        // it (returns `Ok`, not an `Err`), surfacing the distinct invalid-URL
        // state and KEEPING the typed text in the bar for the user to fix — never
        // resetting the bar to the previous page.
        let (mut shell, _handle) = shell_with_backend();
        shell
            .navigate("not-a-url")
            .expect("the front door handles an invalid entry without erroring");
        // No load started and no backend navigation happened.
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        // The distinct invalid-entry axis is set (NOT `last_error`, which is a
        // load failure): the badge fact, orthogonal to a load error.
        assert!(shell.chrome().has_invalid_entry());
        assert_eq!(shell.chrome().invalid_entry.as_deref(), Some("not-a-url"));
        assert_eq!(
            shell.chrome().last_error,
            None,
            "an invalid entry is NOT a load failure"
        );
        // The typed text is KEPT in the bar (never reset to the previous page).
        assert_eq!(shell.chrome().url_text, "not-a-url");
        // The backend was never navigated.
        assert_eq!(shell.current_url_for_test(), None);
    }

    #[test]
    fn a_scheme_less_valid_host_navigates_over_https() {
        // Field finding D (the desired behaviour): a scheme-less plausible host
        // like `github.com` navigates as `https://github.com` (the
        // browser-idiomatic default), keeping the bar on the attempted URL.
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("github.com")
            .expect("a scheme-less valid host navigates");
        // The `https://` was prepended for the backend load.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some("https://github.com")
        );
        assert!(shell.chrome().is_loading(), "the https load is in flight");
        assert!(
            !shell.chrome().has_invalid_entry(),
            "a valid host is not an invalid entry"
        );
        settle(&mut shell, &handle);
        // The bar follows the backend's attempted URL, not a reset to the prior
        // page.
        assert_eq!(shell.chrome().url_text, "https://github.com");
    }

    #[test]
    fn an_explicit_scheme_is_taken_literally_without_a_double_prepend_or_hijack() {
        // Field finding D: an entry that already carries a scheme is navigated
        // LITERALLY — no `https://` double-prepend, no hijack of `ipfs://` /
        // `http://` / `https://`.
        for url in [
            "https://example.com/",
            "http://example.com/",
            "ipfs://bafyexamplecid/index.html",
        ] {
            let (mut shell, _handle) = shell_with_backend();
            shell.navigate(url).expect("an explicit scheme navigates");
            assert_eq!(
                shell.current_url_for_test().as_deref(),
                Some(url),
                "an explicit scheme is taken literally, never re-prefixed"
            );
            assert!(!shell.chrome().has_invalid_entry());
        }
    }

    #[test]
    fn a_valid_hosts_load_failure_keeps_the_url_in_the_bar_with_an_in_page_error() {
        // Field finding D: when a VALID target's LOAD fails (DNS/unreachable), the
        // failure surfaces as a normal in-page browser error (`last_error`) and
        // the bar KEEPS the attempted URL — it does NOT reset to the previous
        // page, and it is NOT the invalid-entry badge (a load failure, not a
        // malformed entry).
        let (mut shell, handle) = shell_with_backend();
        // A first good page so there IS a "previous page" to (not) reset to.
        shell.navigate("https://good.example/").unwrap();
        settle(&mut shell, &handle);
        // Now a scheme-less valid host whose load will fail.
        shell
            .navigate("nope.invalid")
            .expect("a valid-looking host");
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some("https://nope.invalid")
        );
        assert!(shell.pump()); // drain the Started event
        handle.drive_to_failed("name not resolved");
        assert!(shell.pump());
        assert_eq!(shell.chrome().load_state, LoadState::Failed);
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("name not resolved"),
            "a valid target's load failure is an in-page error"
        );
        assert!(
            !shell.chrome().has_invalid_entry(),
            "a load failure is NOT the invalid-entry badge"
        );
        assert_eq!(
            shell.chrome().url_text,
            "https://nope.invalid",
            "the bar keeps the attempted URL, not the previous page"
        );
    }

    #[test]
    fn a_proceeding_navigation_clears_a_prior_invalid_entry_badge() {
        // The invalid-entry axis must never linger onto a later page: a valid
        // navigation after an invalid one clears the badge.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("garbage token").unwrap();
        assert!(shell.chrome().has_invalid_entry());
        shell.navigate("github.com").expect("a valid host");
        assert!(
            !shell.chrome().has_invalid_entry(),
            "a proceeding navigation clears the badge"
        );
        settle(&mut shell, &handle);
        assert!(!shell.chrome().has_invalid_entry());
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
    fn a_fresh_navigation_shows_a_neutral_loading_state_hiding_the_prior_posture_until_settle() {
        // Acceptance (the DONE bar, the trust-honesty fix): navigating from a
        // content-verified page to a DIFFERENTLY-trusted page must NOT keep
        // asserting the previous page's trust while the new one loads. A
        // fake-backend drives load-start -> load-settle across two pages with
        // different postures: while the SECOND load is in flight the chrome is a
        // neutral loading state (`is_loading()` true, the posture is NOT the prior
        // page's), and the NEW page's real posture appears only once it settles.
        let (mut shell, handle) = shell_with_backend();

        // Page one settles content-verified (a plain `ipfs://<cid>`).
        shell.navigate("ipfs://bafypageone/index.html").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert!(shell.chrome().is_content_verified());
        assert!(!shell.chrome().is_loading());

        // Navigate to page two (a plain served origin, a DIFFERENT posture). While
        // its load is in flight the chrome must be a neutral loading state, NOT the
        // prior page's content-verified posture — the indicator never asserts a
        // trust level for a page that is not the one being displayed.
        shell.navigate("https://example.com/").unwrap();
        assert!(shell.chrome().is_loading(), "the second load is in flight");
        assert!(
            !shell.chrome().is_content_verified(),
            "the prior page's content-verified posture must NOT linger during the new load"
        );
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "a fresh navigation clears the stale posture before the new page settles"
        );

        // Only on settle does the NEW page's real posture appear (here: unverified
        // served origin, honestly weaker than the page we came from).
        settle(&mut shell, &handle);
        assert!(!shell.chrome().is_loading());
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn a_fresh_navigation_clears_the_stale_name_and_error_into_the_new_load() {
        // Acceptance: a fresh navigation clears any stale name/posture/error so
        // nothing from the previous page lingers into the new load. Here the
        // previous page is a FAILED ENS load (it pinned the `.eth` name in the bar
        // and surfaced a failure reason); the next navigation must drop BOTH the
        // pinned name and the surfaced error immediately, before the new page
        // settles — so the chrome never shows a stale failure over a fresh load.
        let mut swarm_ch = varint(0xe4); // swarm-ns: an unsupported protocol
        swarm_ch.extend_from_slice(b"some swarm address bytes");
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x44u8; 20])),
            Ok(abi_bytes_return(&swarm_ch)),
        ]);

        // A failed ENS load: the `.eth` name stays in the bar and the failure is
        // surfaced.
        shell.navigate("swarm-site.eth").unwrap();
        assert_eq!(shell.chrome().url_text, "swarm-site.eth");
        assert!(
            shell.chrome().last_error.is_some(),
            "the failure is surfaced"
        );

        // A fresh navigation clears the stale pinned name AND the stale error at
        // once — nothing from the previous page lingers into the new load.
        shell.navigate("https://fresh.example/").unwrap();
        assert_eq!(
            shell.chrome().url_text,
            "https://fresh.example/",
            "the stale `.eth` name does not linger into the new load"
        );
        assert_eq!(
            shell.chrome().last_error,
            None,
            "the stale failure does not linger into the new load"
        );
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "the new load begins with no carried-over posture"
        );
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://fresh.example/");
    }

    // ---- Reload + history keep an ENS page's `.eth` name + posture ------------
    // (task `preserve-ens-name-in-bar-on-reload-and-history`): reload/back/forward
    // of an ENS-resolved page must keep the `.eth` name AND its ENS posture in the
    // bar, never leaking the underlying `ipfs://<cid>`; a non-ENS page is
    // unaffected. The reload decision is RE-RESOLVE for an ENS page (recorded in
    // `work/notes/observations/reload-re-resolves-ens-name-decision-2026-07-23.md`).

    #[test]
    fn reloading_an_ens_page_re_resolves_and_keeps_the_name_and_posture_in_the_bar() {
        // Acceptance: reloading an ENS-resolved page keeps the `.eth` name in the
        // bar (not the `ipfs://<cid>`) and keeps its `NameViaTrustedRpc` posture.
        // The reload RE-RESOLVES the name (the recorded decision), so the provider
        // is fed a SECOND set of resolution answers for the reload.
        let page = b"<!doctype html><title>ronan</title><h1>ronan.eth</h1>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        // TWO resolutions worth of answers: the first Enter, then the reload.
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // First Enter: resolves, loads the CID, pins the name, verifies.
        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );

        // Reload: the name is re-resolved (not the CID reloaded blindly), and the
        // `.eth` name stays pinned in the bar while the re-resolved CID loads.
        shell.reload().expect("reload the settled ENS page");
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth",
            "reload keeps the `.eth` name, never the ipfs://<cid>"
        );
        assert!(shell.chrome().is_loading(), "reload restarts the load");
        // The re-resolution fed the (re-derived) CID into the verified path.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(ipfs_uri.as_str())
        );

        // The scheme handler verifies the reloaded bytes; the ENS posture is back.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth",
            "the `.eth` name survives the whole reload load"
        );
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "reload keeps the ENS trust posture, not plain ContentVerified"
        );
        assert!(shell.chrome().is_name_via_trusted_rpc());
        assert!(!shell.chrome().is_content_verified());
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn reloading_a_plain_ipfs_page_is_unaffected_and_never_grows_the_eth_name() {
        // Acceptance: a plain `ipfs://` page reloads its real URL (the CID), never
        // gaining an `.eth` name, and stays plain ContentVerified — the ENS name
        // never leaks onto a non-ENS page on reload.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ipfs://bafyplaincid/index.html").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ipfs://bafyplaincid/index.html");

        shell.reload().expect("reload the plain ipfs page");
        assert_eq!(
            shell.chrome().url_text,
            "ipfs://bafyplaincid/index.html",
            "a plain ipfs page reloads its CID URL, never an `.eth` name"
        );
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ipfs://bafyplaincid/index.html");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::ContentVerified);
        assert!(!shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn back_and_forward_onto_an_ens_page_show_the_name_and_posture_not_the_cid() {
        // Acceptance: navigating back/forward onto an ENS-originated history entry
        // shows the `.eth` name + its posture (re-derived from the CID<->name
        // association), while a non-ENS entry shows its real URL as today.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // Entry 1: an ENS page (ronan.eth -> ipfs://<cid>), verified.
        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );

        // Entry 2: a plain served page (NOT served via the verified content path,
        // so it stays the untrusted origin — the ENS posture must not leak here).
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://example.com/");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);

        // Back onto the ENS entry. On the REAL (async) backend `current_url` does
        // NOT settle onto the ENS CID synchronously inside `go_back`, so the name
        // cannot appear yet: the bar still follows the OLD entry until the
        // backend's `load-changed` signal settles. This is exactly the async lag
        // the upgraded fake now models (the old synchronous fake hid it).
        shell.go_back();
        assert_eq!(
            shell.chrome().url_text,
            "https://example.com/",
            "async history: the name is NOT re-derived before current_url settles"
        );
        // Once the history move settles via the pump, the backend reports the ENS
        // CID in WebKit's authority-less `ipfs:///<cid>` form; the shell re-derives
        // the `.eth` name off the NORMALIZED key and re-marks the ENS posture axis.
        // The re-derivation is robust to BOTH v0.2.3 regression causes: the async
        // settle AND the URL-normalization variance.
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth",
            "back onto an ENS entry re-derives the `.eth` name on the settled pump, never the ipfs:///<cid>"
        );
        assert!(
            !shell.chrome().url_text.starts_with("ipfs://"),
            "the ipfs:// / ipfs:/// CID never leaks into the bar"
        );
        // The underlying entry is the resolved CID in WebKit's normalized form; the
        // NORMALIZED key still matches the authority form stored at forward load.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(webkit_normalize(&ipfs_uri).as_str())
        );
        // The verified content path serves the settled entry's bytes; the re-marked
        // ENS-origin axis makes the posture `NameViaTrustedRpc`, not a bare-CID
        // `ContentVerified`.
        handle.serve_via_verified_content_path();
        shell.pump();
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "back onto an ENS page keeps its posture, not a bare-CID ContentVerified"
        );

        // Forward onto the plain page: its real URL, no `.eth` name, no ENS posture
        // (again NOT served via the verified content path).
        shell.go_forward();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "https://example.com/",
            "forward onto a non-ENS entry shows its real URL"
        );
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
        assert!(!shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn back_onto_a_mutable_ipns_ens_page_keeps_the_name_and_mutable_axis() {
        // Acceptance: the mutable axis survives history too. Back onto an ENS page
        // that resolved via a MUTABLE ipns-ns name re-derives the `.eth` name AND
        // re-marks the mutable axis, so once the RPC warning is not the loudest the
        // page is still honestly mutable (here ENS keeps NameViaTrustedRpc loudest,
        // and the mutable flag is proven re-applied by a later plain page staying
        // clean).
        let key = IpnsKeyFixture::new();
        let page = b"<!doctype html><title>ipns</title>";
        let target_cid = cid_v1_raw_sha256(page).expect("derive the target cid");
        let record = key.signed_record_for(&target_cid);
        let (mut shell, handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ],
            PinnedIpnsSource::with_record(&key.name, record),
        );

        // Entry 1: a mutable ENS page (ipns-ns), verified.
        shell.navigate("mutable.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "mutable.eth");

        // Entry 2: a plain served page.
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);

        // Back onto the mutable ENS entry. As with the immutable case, the name is
        // re-derived only once the async history move SETTLES via the pump (not
        // synchronously in `go_back`), off the NORMALIZED key.
        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "mutable.eth",
            "back onto a mutable ENS entry re-derives the name on the settled pump"
        );
        assert!(!shell.chrome().url_text.starts_with("ipfs://"));
        handle.serve_via_verified_content_path();
        shell.pump();
        assert_eq!(shell.chrome().url_text, "mutable.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );
        assert!(shell.chrome().is_name_via_trusted_rpc());
    }

    #[test]
    fn a_non_ens_history_stack_is_wholly_unaffected_by_the_ens_association() {
        // Acceptance: with no ENS load in the mix, reload/back/forward behave
        // exactly as before — the ENS association never touches a plain stack.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://a.example/").unwrap();
        settle(&mut shell, &handle);
        shell.navigate("https://b.example/").unwrap();
        settle(&mut shell, &handle);

        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://a.example/");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);

        shell.go_forward();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://b.example/");

        shell.reload().expect("reload a plain page");
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://b.example/");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn in_page_navigation_on_an_ens_page_updates_the_bar_and_back_re_derives_the_name() {
        // Acceptance (field finding v0.2.3, finding C): after loading an ENS page
        // and navigating WITHIN it (a link click that changes the backend URL),
        // the URL bar must UPDATE to reflect where the user now is, instead of
        // staying FROZEN on the pinned `.eth` name. The pin is for the front-door
        // ROOT load only; an in-page move FOLLOWS the backend URL (the recorded
        // pin-vs-follow decision, see
        // `docs/spikes/urlbar-tracks-in-page-navigation-not-just-pinned-name/`).
        let page = b"<!doctype html><title>ronan</title><h1>ronan.eth root</h1>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // Load the ENS root: the name is pinned in the bar while its CID loads.
        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );

        // The user clicks a link WITHIN the ENS page: the webview begins a fresh
        // in-page load to a DIFFERENT ipfs resource, WITHOUT the shell calling
        // `navigate`. This is the exact path that used to be suppressed by the
        // pinned name.
        let in_page = "ipfs://bafyinpagesubresource/some/page.html";
        handle.navigate_in_page(in_page);
        shell.pump();
        // The bar now FOLLOWS the backend URL: it no longer freezes on `ronan.eth`.
        assert_ne!(
            shell.chrome().url_text,
            "ronan.eth",
            "in-page navigation must not stay frozen on the pinned .eth name"
        );
        assert_eq!(
            shell.chrome().url_text,
            in_page,
            "the bar follows the in-page backend URL"
        );
        // The posture tracks the ACTUAL load path: this in-page resource is NOT a
        // known ENS entry and was not served via the verified path, so the ENS /
        // verified posture must not persist.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::ContentVerified,
            "an in-page move to a non-ENS resource does not keep the ENS posture"
        );
        assert!(!shell.chrome().is_name_via_trusted_rpc());

        // Back onto the ENS ROOT entry: its name is re-derived from the normalized
        // `ens_pages` key (the pin was dropped, but the root is recoverable), and
        // its ENS posture is re-marked — the root never loses its identity.
        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth",
            "back onto the ENS root re-derives the name via ens_pages"
        );
        assert!(!shell.chrome().url_text.starts_with("ipfs://"));
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(webkit_normalize(&ipfs_uri).as_str())
        );
        handle.serve_via_verified_content_path();
        shell.pump();
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "the ENS root keeps its posture on history return"
        );
    }

    #[test]
    fn in_page_navigation_on_a_plain_page_tracks_its_url_unregressed() {
        // Acceptance: a plain (non-ENS) page tracks its URL on in-page navigation
        // exactly as a browser does. This was already fine for non-pinned pages;
        // the pin-vs-follow fix must not regress it.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://example.com/");

        // A link click within the plain page: the bar follows the new URL.
        handle.navigate_in_page("https://example.com/deep/link");
        shell.pump();
        assert_eq!(shell.chrome().url_text, "https://example.com/deep/link");
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://example.com/deep/link");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
    }

    // ---- Loading progress + transient-vs-hard error (task
    // `clearer-loading-and-error-indicator`) ----------------------------------

    #[test]
    fn failure_kind_classifies_a_timeout_as_transient_and_a_protocol_reason_as_hard() {
        // Acceptance: a transient/timeout failure is distinguished from a hard
        // failure. The classifier keys on the transient markers the timeout /
        // transport reasons carry (the field finding's `transport error: timeout:
        // global`, the fetcher/provider taxonomy), and treats everything else
        // (unsupported / verification / malformed) as hard.
        for transient in [
            "transport error: timeout: global",
            "rpc transport error: connection refused",
            "content source error: transport error: timeout",
            "IPNS record fetch failed: connection refused",
            "io error: read timed out",
        ] {
            assert_eq!(
                FailureKind::classify(transient),
                FailureKind::Transient,
                "a timeout/transport reason is retryable: {transient}"
            );
            assert!(FailureKind::classify(transient).is_retryable());
        }
        for hard in [
            "points to Swarm, not supported",
            "IPNS record did not verify: dag-cbor data does not match the protobuf fields",
            "block hash mismatch: bytes do not match cid bafy",
            "this name has no ENS resolver set",
            "this name has no contenthash set",
            "invalid content identifier: bafybad",
        ] {
            assert_eq!(
                FailureKind::classify(hard),
                FailureKind::Hard,
                "an unsupported/verification/malformed reason is hard: {hard}"
            );
            assert!(!FailureKind::classify(hard).is_retryable());
        }
        // A verification reason that happens to mention an expiry is STILL hard: a
        // retry will not make an expired/invalid record valid.
        assert_eq!(
            FailureKind::classify("IPNS record did not verify: the record expired"),
            FailureKind::Hard
        );
    }

    #[test]
    fn load_step_hint_and_wire_names_are_stable_and_idle_has_no_hint() {
        // The step hint is empty only for Idle (no load to describe); each active
        // step has a distinct short hint and a stable wire name for the FFI JSON.
        assert_eq!(LoadStep::Idle.hint(), "");
        assert_eq!(LoadStep::default(), LoadStep::Idle);
        for step in [
            LoadStep::ResolvingName,
            LoadStep::FetchingRecord,
            LoadStep::FetchingContent,
            LoadStep::Rendering,
        ] {
            assert!(!step.hint().is_empty(), "{step:?} has a hint");
        }
        assert_eq!(LoadStep::ResolvingName.wire_name(), "resolving-name");
        assert_eq!(LoadStep::FetchingRecord.wire_name(), "fetching-record");
        assert_eq!(LoadStep::FetchingContent.wire_name(), "fetching-content");
        assert_eq!(LoadStep::Rendering.wire_name(), "rendering");
        assert_eq!(LoadStep::Idle.wire_name(), "idle");
    }

    #[test]
    fn a_slow_load_shows_real_pipeline_progress_from_fetching_to_rendering_to_idle() {
        // Acceptance: a slow load shows clear ongoing activity/progress driven by
        // the REAL lifecycle, not faked. A plain content load moves
        // FetchingContent (Started) -> Rendering (Committed) -> Idle (Finished),
        // so the chrome reads as "working" the whole time rather than frozen.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(shell.chrome().load_step(), LoadStep::Idle);

        shell.navigate("ipfs://bafyslowcid/index.html").unwrap();
        // Started: the content is being fetched.
        assert!(shell.chrome().is_loading());
        assert_eq!(shell.chrome().load_step(), LoadStep::FetchingContent);

        // Committed (first bytes arrived): the page is rendering.
        {
            let mut b = handle.inner.borrow_mut();
            let url = b.current().unwrap().clone();
            b.state = LoadState::Committed;
            b.events.push_back(LoadEvent::Committed { url });
        }
        shell.pump();
        assert!(shell.chrome().is_loading());
        assert_eq!(shell.chrome().load_step(), LoadStep::Rendering);

        // Finished: no step (the load settled), so the indicator stops.
        {
            let mut b = handle.inner.borrow_mut();
            let url = b.current().unwrap().clone();
            b.state = LoadState::Finished;
            b.events.push_back(LoadEvent::Finished { url });
        }
        shell.pump();
        assert!(!shell.chrome().is_loading());
        assert_eq!(shell.chrome().load_step(), LoadStep::Idle);
    }

    #[test]
    fn an_ens_load_shows_the_resolving_name_step_then_the_content_step() {
        // Acceptance: the step reflects the REAL resolution/fetch pipeline (name ->
        // content). During ENS resolution the front door is at ResolvingName; once
        // it hands the resolved CID to the backend the step is the content step.
        // Resolution is synchronous, so the observable transition is
        // ResolvingName-on-failure vs FetchingContent-on-success; a successful
        // resolve lands on FetchingContent.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, _uri) = ipfs_contenthash_fixture(page);
        let (mut shell, _handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);
        shell.navigate("ronan.eth").unwrap();
        // After a successful resolve the load handed off to the backend: the
        // content is being fetched.
        assert!(shell.chrome().is_loading());
        assert_eq!(shell.chrome().load_step(), LoadStep::FetchingContent);
    }

    #[test]
    fn an_ens_resolution_failure_reports_the_resolving_name_step_and_no_lingering_step() {
        // A failure DURING name resolution reports ResolvingName as the stage it
        // failed at, but a FAILED load is not "loading", so the settled chrome
        // shows the Idle step (the indicator stops) — the step never lingers.
        let (mut shell, _handle) = shell_with_provider(vec![Err(ProviderError::Transport(
            "connection refused".to_string(),
        ))]);
        shell.navigate("unreachable.eth").unwrap();
        assert!(!shell.chrome().is_loading());
        assert_eq!(
            shell.chrome().load_step(),
            LoadStep::Idle,
            "a settled failure shows no in-flight step"
        );
    }

    #[test]
    fn a_transient_content_timeout_is_retryable_while_a_hard_fail_is_not() {
        // Acceptance: a transient/timeout content failure (the webview reports it
        // as a `LoadEvent::Failed` reason string) is surfaced as RETRYABLE, while a
        // hard fail keeps its protocol-named reason and is NOT retryable. The fake
        // backend drives both failure reasons through the SAME seam path the real
        // webview uses.
        let (mut shell, handle) = shell_with_backend();

        // A slow content load that times out: a transient failure the user can
        // retry (a reload).
        shell.navigate("ipfs://bafyslowcid/index.html").unwrap();
        shell.pump();
        handle.drive_to_failed("transport error: timeout: global");
        shell.pump();
        assert_eq!(shell.chrome().load_state, LoadState::Failed);
        assert_eq!(
            shell.chrome().failure_kind(),
            Some(FailureKind::Transient),
            "a timeout content failure is transient"
        );
        assert!(
            shell.chrome().failure_is_retryable(),
            "a transient timeout offers a retry affordance"
        );
        // The honest reason is kept verbatim (never masked by the classification).
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("transport error: timeout: global")
        );

        // A hard content failure (a hash mismatch): NOT retryable, keeps its
        // protocol-named reason.
        shell.navigate("ipfs://bafybadcid/index.html").unwrap();
        shell.pump();
        handle.drive_to_failed("block hash mismatch: bytes do not match cid bafybadcid");
        shell.pump();
        assert_eq!(
            shell.chrome().failure_kind(),
            Some(FailureKind::Hard),
            "a hash mismatch is a hard failure"
        );
        assert!(!shell.chrome().failure_is_retryable());
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("block hash mismatch: bytes do not match cid bafybadcid")
        );
    }

    #[test]
    fn a_transient_ens_resolution_timeout_is_retryable_and_reload_re_runs_it() {
        // Acceptance: a transient timeout during ENS resolution is retryable, and
        // the retry affordance IS the existing reload (a failed ENS load re-runs
        // the resolution from the pinned name). Here the first resolve times out
        // (transient), and a reload with a now-succeeding provider completes.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            // First attempt: a transport timeout on the very first eth_call.
            Err(ProviderError::Transport("timeout: global".to_string())),
            // The retry (reload): a full, successful resolution.
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        shell.navigate("ronan.eth").unwrap();
        assert_eq!(shell.chrome().url_text, "ronan.eth");
        assert_eq!(
            shell.chrome().failure_kind(),
            Some(FailureKind::Transient),
            "a resolution timeout is a retryable transient failure"
        );
        assert!(shell.chrome().failure_is_retryable());
        assert_eq!(
            shell.current_url_for_test(),
            None,
            "nothing loaded on timeout"
        );

        // Retry via reload: the pinned name is re-resolved, this time succeeding,
        // and the content loads — proving the transient failure was truly
        // retryable.
        shell
            .reload()
            .expect("reload retries the failed ENS resolution");
        assert!(shell.chrome().is_loading());
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(ipfs_uri.as_str())
        );
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().last_error,
            None,
            "the retry cleared the failure"
        );
        assert_eq!(shell.chrome().url_text, "ronan.eth");
    }

    #[test]
    fn a_hard_ens_failure_keeps_its_protocol_named_reason_and_is_not_retryable() {
        // Acceptance: hard failures keep their prominent protocol-named reason and
        // are NOT offered a retry. An unsupported-protocol contenthash is hard.
        let mut swarm_ch = varint(0xe4); // swarm-ns
        swarm_ch.extend_from_slice(b"some swarm address bytes");
        let (mut shell, _handle) = shell_with_provider(vec![
            Ok(address_word(&[0x44u8; 20])),
            Ok(abi_bytes_return(&swarm_ch)),
        ]);
        shell.navigate("swarm-site.eth").unwrap();
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("points to Swarm, not supported"),
            "the protocol-named reason is kept verbatim"
        );
        assert_eq!(shell.chrome().failure_kind(), Some(FailureKind::Hard));
        assert!(
            !shell.chrome().failure_is_retryable(),
            "a hard failure offers no retry"
        );
    }

    #[test]
    fn an_unverifiable_ipns_record_is_a_hard_failure_not_retryable() {
        // Acceptance: a verification failure (a forged IPNS record) is HARD — a
        // retry cannot make an unsigned/misdirecting record verify — and keeps its
        // "did not verify" reason.
        let key = IpnsKeyFixture::new();
        let attacker = IpnsKeyFixture::new();
        let target_cid = cid_v1_raw_sha256(b"content the attacker wants").expect("cid");
        let forged = attacker.signed_record_for(&target_cid);
        let (mut shell, _handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ],
            PinnedIpnsSource::with_record(&key.name, forged),
        );
        shell.navigate("forged.eth").unwrap();
        let reason = shell.chrome().last_error.clone().expect("a reason");
        assert!(reason.contains("did not verify"), "{reason}");
        assert_eq!(
            shell.chrome().failure_kind(),
            Some(FailureKind::Hard),
            "a verification failure is hard, never retryable"
        );
        assert!(!shell.chrome().failure_is_retryable());
    }

    #[test]
    fn an_ipns_record_fetch_timeout_is_a_transient_retryable_failure() {
        // Acceptance: a transient IPNS record-FETCH failure (a dead/slow gateway,
        // an `IpnsError::Source` transport reason) is retryable — distinct from an
        // unverifiable record (hard). This is the `fetching-record` step's
        // transient case.
        let key = IpnsKeyFixture::new();
        let (mut shell, _handle) = shell_with_provider_and_ipns(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ],
            PinnedIpnsSource::failing(IpnsError::Source("connection refused".into())),
        );
        shell.navigate("slow-ipns.eth").unwrap();
        let reason = shell.chrome().last_error.clone().expect("a reason");
        assert!(reason.contains("connection refused"), "{reason}");
        assert_eq!(
            shell.chrome().failure_kind(),
            Some(FailureKind::Transient),
            "a record-fetch transport failure is transient/retryable"
        );
        assert!(shell.chrome().failure_is_retryable());
    }

    #[test]
    fn no_failure_means_no_failure_kind_and_a_fresh_navigation_clears_it() {
        // A settled-ok / idle chrome has no failure kind, and a fresh navigation
        // clears a prior failure kind with the error.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(shell.chrome().failure_kind(), None);
        assert!(!shell.chrome().failure_is_retryable());

        shell.navigate("https://example.com/").unwrap();
        handle.drive_to_failed("transport error: timeout: global");
        shell.pump();
        assert_eq!(shell.chrome().failure_kind(), Some(FailureKind::Transient));

        shell.navigate("https://fresh.example/").unwrap();
        assert_eq!(
            shell.chrome().failure_kind(),
            None,
            "a fresh load clears it"
        );
        assert!(!shell.chrome().failure_is_retryable());
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

/// A race-hardened reader for the throwaway loopback HTTP fixtures the crate's
/// end-to-end tests bind on `127.0.0.1:0` (`ethereum::LocalRpcServer`,
/// `ens::SequencedRpcServer`).
///
/// The fixtures capture and assert on the request body that went over the wire
/// (e.g. the `eth_call` JSON). Reading that request with a SINGLE
/// `stream.read(&mut buf)` assumes the whole HTTP request (request line +
/// headers + body) lands in one TCP segment. Under parallel test load the body
/// can arrive in a LATER segment, leaving the captured body empty and the
/// downstream `serde_json::from_slice` assert panicking with
/// "EOF while parsing a value" — an intermittent harness race that reds the
/// `verify` gate (see `work/notes/observations/flaky-loopback-rpc-server-partial-read.md`
/// and `flaky-ethereum-end-to-end-loopback-test-2026-07-22.md`). This module is
/// the single shared fix the fixtures reuse, so the whole family is hardened in
/// one place rather than three drifting copies.
#[cfg(test)]
pub(crate) mod loopback_test_server {
    use std::io::Read;

    /// Read one complete HTTP request from `stream` and return its body (the
    /// bytes AFTER the `\r\n\r\n` header terminator), draining the full declared
    /// `Content-Length` before returning.
    ///
    /// It loops `read()` until (a) the header terminator has been seen AND (b)
    /// the number of body bytes already buffered reaches the request's
    /// `Content-Length` (0 for a body-less request), so a body split across
    /// several TCP segments is fully drained rather than truncated. A `read()`
    /// returning `0` (peer closed) or erroring ends the loop with whatever was
    /// received, so a malformed/short request degrades to the old best-effort
    /// capture instead of hanging. Any bytes read past the declared body length
    /// (a pipelined next request, which these one-shot fixtures never send) are
    /// left out of the returned body.
    ///
    /// Returns `None` when no header terminator was ever received (the request
    /// head never completed), matching the fixtures' prior "only capture once we
    /// found the header end" behaviour.
    pub(crate) fn read_request_body(stream: &mut impl Read) -> Option<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            // Stop as soon as we have the full head AND the full declared body.
            if let Some(header_end) = find_header_end(&buf) {
                let content_length = parse_content_length(&buf[..header_end]);
                let body_len = buf.len() - header_end;
                if body_len >= content_length {
                    return Some(buf[header_end..header_end + content_length].to_vec());
                }
            }
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        // The stream closed (or errored) before the full request arrived: return
        // whatever body bytes we did receive if the head at least completed, so a
        // short/malformed request degrades gracefully rather than the read looping
        // forever.
        find_header_end(&buf).map(|header_end| buf[header_end..].to_vec())
    }

    /// The byte offset just past the `\r\n\r\n` header terminator (the start of
    /// the body), or `None` if the head is not yet complete.
    fn find_header_end(raw: &[u8]) -> Option<usize> {
        raw.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
    }

    /// The declared `Content-Length` of a request head (case-insensitive header
    /// name), or `0` when the header is absent or unparseable — the fixtures only
    /// ever send a well-formed `Content-Length`, and a body-less request drains
    /// zero body bytes.
    fn parse_content_length(head: &[u8]) -> usize {
        let text = String::from_utf8_lossy(head);
        for line in text.split("\r\n") {
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    return value.trim().parse().unwrap_or(0);
                }
            }
        }
        0
    }

    #[cfg(test)]
    mod tests {
        use super::read_request_body;
        use std::io::Write;
        use std::net::{TcpListener, TcpStream};
        use std::thread;
        use std::time::Duration;

        /// The bug this whole task fixes: when the request body lands in a LATER
        /// TCP segment than the headers, a single `read()` would capture an empty
        /// body. The shared reader must loop until the full `Content-Length` body
        /// is drained, so the returned body is the COMPLETE JSON payload.
        #[test]
        fn drains_a_body_that_arrives_in_a_later_tcp_segment() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let body = br#"{"jsonrpc":"2.0","id":1,"method":"eth_call"}"#;
            let writer = thread::spawn(move || {
                let mut client = TcpStream::connect(addr).expect("connect loopback");
                // Send the request line + headers FIRST, flush, pause, THEN the
                // body — forcing the body into a separate segment the way the
                // real flake does under parallel load. A single-read fixture would
                // capture an empty body here.
                let head = format!(
                    "POST / HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                client.write_all(head.as_bytes()).expect("write head");
                client.flush().expect("flush head");
                thread::sleep(Duration::from_millis(50));
                client.write_all(body).expect("write body");
                client.flush().expect("flush body");
            });

            let (mut stream, _) = listener.accept().expect("accept");
            let captured = read_request_body(&mut stream).expect("the head completed");
            writer.join().expect("writer thread");

            assert_eq!(
                captured, body,
                "the full body must be drained even when it arrives in a later segment"
            );
        }

        /// A body-less request (no `Content-Length`) returns an empty body once
        /// the head completes, without looping forever waiting for a body.
        #[test]
        fn returns_empty_body_for_a_bodyless_request() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let writer = thread::spawn(move || {
                let mut client = TcpStream::connect(addr).expect("connect loopback");
                client
                    .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                    .expect("write head");
                client.flush().expect("flush");
            });
            let (mut stream, _) = listener.accept().expect("accept");
            let captured = read_request_body(&mut stream).expect("the head completed");
            writer.join().expect("writer thread");
            assert!(captured.is_empty(), "a body-less request drains no body");
        }
    }
}
