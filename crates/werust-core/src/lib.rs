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

use crate::ethereum::{EthereumProvider, RpcProvider};
use crate::ipns::IpnsRecordSource;

pub mod contenthash;
pub mod debug;
pub mod ens;
pub mod ethereum;
pub mod ipfs;
pub mod ipns;
pub mod menu;
pub mod name_resolution;
pub mod pins;
pub mod provider;
pub mod redirects;
pub mod retrieval;
pub mod shortcuts;

/// The version-RESOLUTION rules the build script runs, compiled in under test so
/// the pure-Rust `verify` gate covers them.
///
/// `build.rs` `include!`s the SAME file to resolve `WERUST_VERSION` at build
/// time; a build script cannot itself be `cargo test`ed, so the precedence and
/// normalisation live in one shared file that is both `include!`d there and
/// unit-tested here. It contributes nothing to a non-test build.
#[cfg(test)]
mod version_resolution;

/// werust's version string: the SINGLE source every surface that shows a version
/// reads.
///
/// It is `WERUST_VERSION`, RESOLVED ONCE at build time by this crate's
/// [`build.rs`](../build.rs): the injected `WERUST_VERSION` env var (CI exports
/// it from the release tag), else `git describe --tags --always` (an informative
/// dev build such as `0.2.6-3-gabc1234`), else `CARGO_PKG_VERSION`. The rules
/// live in `src/version_resolution.rs` and are unit-tested there.
///
/// The desktop startup banner, the browser [`menu`]'s version line, and the
/// mobile menus (which read it over the FFI — `werust_ios_version` /
/// `nativeVersion`) all report the SAME version because they all call THIS.
/// Before this, the version existed only as an `env!` in the desktop binary and
/// a hand-maintained Gradle `versionName`; the menu task made a second, third
/// and fourth reader, so it is centralised here rather than re-`env!`'d per edge
/// (an `env!` in a mobile crate would read THAT crate's version, which is the
/// same today only by workspace inheritance).
///
/// It is deliberately NOT `CARGO_PKG_VERSION` directly: nothing in the release
/// path ever injected the tag into the compiled Rust, so a tagged `v0.2.6` build
/// shipped every menu reading `werust 0.0.0` — a confident lie in a user-facing
/// surface. Do NOT add a second version source; extend the resolution instead.
#[must_use]
pub fn version() -> &'static str {
    env!("WERUST_VERSION")
}

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

/// Recognise a `.eth` URL-bar entry that MAY carry a sub-path, splitting it into
/// the ENS NAME (`<label>.eth`) and the PATH (the remainder from the first `/`,
/// with its leading `/`, or `""` for a bare name), or [`None`] if the entry is
/// not a `.eth` name.
///
/// This is the name+path sibling of [`eth_name_from_entry`] (which stays the
/// strict bare-name recogniser and STILL rejects any `/`). It is what lets a
/// `.eth` name WITH a path route to the ENS front door: `ronan.eth/blog/` splits
/// into the name `ronan.eth` and the path `/blog/`, so the front door resolves
/// the NAME and threads the PATH into the resolved `ipfs://<cid>/<path>` load
/// (the ipfs sub-path + directory-index resolution already exists,
/// `docs/adr/0004` / `resolve_ipfs_request`). Field finding B,
/// `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md`.
///
/// The guards mirror [`eth_name_from_entry`], applied to the NAME part only:
///
/// * an explicit scheme (`https://…`, `ipfs://…`, any `scheme://…`) is taken
///   literally and never treated as a name (so `https://ronan.eth/blog/` is NOT
///   hijacked);
/// * the label before the first `/` must end in `.eth` (case-insensitively) with
///   a non-empty label before it — so a non-`.eth` host with a path
///   (`github.com/foo`) is NOT an ENS name (it classifies as an https candidate),
///   and ONLY a `.eth` TLD label routes to ENS;
/// * a bare `.eth` (no `/`, or a lone trailing `/`) yields an EMPTY path, so it is
///   identical to the [`eth_name_from_entry`] bare-name case.
///
/// A lone trailing slash on the bare name (`ronan.eth/`) is treated as the bare
/// name with no path (empty path), matching [`eth_name_from_entry`] — the
/// directory + trailing-slash handling for a REAL sub-path lives in the ipfs path
/// resolution + [`normalize_ens_page_key`](crate::ipfs::normalize_ens_page_key),
/// so `ronan.eth/blog/` and `ronan.eth/blog` resolve the same entity there.
/// Label normalisation/validation is still the resolver's job.
fn eth_name_and_path_from_entry(entry: &str) -> Option<(&str, &str)> {
    // An explicit scheme is taken literally: only the scheme-less front door is a
    // name (identical to `eth_name_from_entry`).
    if entry.contains("://") {
        return None;
    }
    // Split off the sub-path at the FIRST `/`: the name is before it, the path is
    // from the `/` onward (leading `/` kept). A bare `.eth` (no `/`, or only a
    // lone trailing `/`) has an empty path.
    let (name, path) = match entry.find('/') {
        Some(idx) => (&entry[..idx], &entry[idx..]),
        None => (entry, ""),
    };
    // A lone trailing slash (`ronan.eth/`) is the bare name with no path, matching
    // `eth_name_from_entry`'s "or a trailing `/`" rule.
    let path = if path == "/" { "" } else { path };
    // The NAME part must be a valid bare `.eth` name — delegate to the SAME
    // recogniser the no-path front door uses, so the `.eth` TLD + non-empty-label
    // guard lives in ONE place (and `eth_name_from_entry` stays load-bearing and
    // keeps its "a bare name has no `/`" rule intact for the no-path caller).
    let name = eth_name_from_entry(name)?;
    Some((name, path))
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
    /// Every pipeline step, in the order a full load walks them.
    ///
    /// The single source of truth for "which steps exist", so a caller that must
    /// cover the whole axis (the chrome-rule drives in the tests below) iterates
    /// THIS instead of re-listing the variants in a literal that silently goes
    /// stale. Kept complete by the const check below.
    pub const ALL: [LoadStep; 5] = [
        LoadStep::Idle,
        LoadStep::ResolvingName,
        LoadStep::FetchingRecord,
        LoadStep::FetchingContent,
        LoadStep::Rendering,
    ];

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

/// Keeps [`LoadStep::ALL`] EXHAUSTIVE, at compile time.
///
/// `listed` is a total match — no wildcard arm — whose every arm hands back the
/// step's OWN entry in the list, so a new [`LoadStep`] variant cannot reach a
/// build twice over: the match stops compiling until the variant is named here,
/// and the arm the author then writes (`… => LoadStep::ALL[5]`, the next slot)
/// stops compiling too — `index out of bounds`, the deny-by-default
/// `unconditional_panic` lint — unless the variant is ALSO added to `ALL`. The
/// loop closes the last hole: it proves the list holds each step, once, at the
/// slot its arm claims, so a reordered or duplicated entry is a compile error as
/// well. (The `as u8` casts compare two fieldless-enum values in const context,
/// where `==` is not available.)
const _LOAD_STEP_ALL_IS_EVERY_STEP_IN_SLOT_ORDER: () = {
    const fn listed(step: LoadStep) -> LoadStep {
        match step {
            LoadStep::Idle => LoadStep::ALL[0],
            LoadStep::ResolvingName => LoadStep::ALL[1],
            LoadStep::FetchingRecord => LoadStep::ALL[2],
            LoadStep::FetchingContent => LoadStep::ALL[3],
            LoadStep::Rendering => LoadStep::ALL[4],
        }
    }
    let mut i = 0;
    while i < LoadStep::ALL.len() {
        assert!(
            listed(LoadStep::ALL[i]) as u8 == LoadStep::ALL[i] as u8,
            "LoadStep::ALL must hold every step, once, in slot order"
        );
        i += 1;
    }
};

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
    /// Every failure severity, loudest last.
    ///
    /// The single source of truth for "which severities exist", so a caller that
    /// must cover the whole axis iterates THIS instead of re-listing the variants
    /// in a literal that silently goes stale. Load-bearing for the chrome: the
    /// error banner derives one CSS class per severity and this crate exports the
    /// complete class set for painters to toggle, so the test that proves the set
    /// covers every severity drives it from here (task
    /// `export-the-chrome-css-class-set-from-core`). Kept complete by the const
    /// check below.
    pub const ALL: [FailureKind; 2] = [FailureKind::Transient, FailureKind::Hard];

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

/// Keeps [`FailureKind::ALL`] EXHAUSTIVE at compile time, by exactly the
/// construction [`LoadStep::ALL`]'s check uses (see it for the full reasoning):
/// the total `listed` match refuses to compile until a new severity is named here,
/// and the arm it is named in (`… => FailureKind::ALL[2]`) refuses to compile
/// until the severity is in `ALL` as well — which is what keeps the error
/// banner's exported CSS-class set honest (task
/// `export-the-chrome-css-class-set-from-core`).
const _FAILURE_KIND_ALL_IS_EVERY_KIND_IN_SLOT_ORDER: () = {
    const fn listed(kind: FailureKind) -> FailureKind {
        match kind {
            FailureKind::Transient => FailureKind::ALL[0],
            FailureKind::Hard => FailureKind::ALL[1],
        }
    }
    let mut i = 0;
    while i < FailureKind::ALL.len() {
        assert!(
            listed(FailureKind::ALL[i]) as u8 == FailureKind::ALL[i] as u8,
            "FailureKind::ALL must hold every failure kind, once, in slot order"
        );
        i += 1;
    }
};

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
    /// The current page's MUTABLE-NAME identity and its trust-on-first-use state:
    /// the name, the CID it resolves to on THIS load, and whatever the user has
    /// BLESSED for that name (task `ipns-tofu-pin-and-warn-on-change`,
    /// `docs/adr/0006`'s mutability axis). `None` whenever the current page is
    /// not a name-resolved load at all (a direct `ipfs://<cid>`, an ordinary
    /// `https://` page, a failed resolution): there is nothing to bless and
    /// nothing to warn about.
    ///
    /// A SEPARATE axis from [`trust_posture`](ChromeState::trust_posture) rather
    /// than a fifth posture, for the same reason
    /// [`invalid_entry`](ChromeState::invalid_entry) is separate from
    /// [`last_error`](ChromeState::last_error): the posture is the seam's truth
    /// about how THIS load's bytes and name were learned (a
    /// [`Renderer::trust_posture`] read), while the pin is a DURABLE user
    /// decision about a name, read from the [`pins`](crate::pins) store and
    /// unrelated to any backend. The display rules combine the two (a
    /// blessed-then-CHANGED name is the LOUDEST state, above every posture), but
    /// the facts stay apart, so no seam has to learn what a pin is.
    ///
    /// FAIL-SAFE: this axis can only make the chrome say MORE. An unblessed name
    /// (or an unreadable pin store) leaves every other rule exactly as it was
    /// before, and nothing here participates in deciding what to load or whether
    /// bytes verified.
    pub mutable_name: Option<crate::pins::MutableNameTrust>,
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

    /// The TOFU WARNING condition: the current page's mutable name was BLESSED,
    /// and it now resolves to a DIFFERENT CID than the one the user trusted.
    ///
    /// This is the actionable form of the mutability warning ("this CHANGED
    /// since you trusted it", not "this could change"), and it is the LOUDEST
    /// state the chrome has: it wins over every [`TrustPosture`] (including
    /// [`NameViaTrustedRpc`](TrustPosture::NameViaTrustedRpc)) and raises the
    /// failure-class banner, because it is the only chrome state that says
    /// something the user has personally verified is no longer true. It is never
    /// silently accepted and never a hard block: the user can look and decide.
    ///
    /// `false` for an unblessed name, so an unblessed load behaves exactly as it
    /// did before the pin store existed.
    #[must_use]
    pub fn mutable_name_changed(&self) -> bool {
        self.mutable_name
            .as_ref()
            .is_some_and(crate::pins::MutableNameTrust::is_changed)
    }

    /// Whether the trust surface should offer the BLESS action for this page:
    /// there is a mutable name, and blessing it would record something new
    /// (a never-blessed name, or a blessed one that has since changed).
    ///
    /// The bless is an EXPLICIT user action reached from the trust indicator, not
    /// a first-visit prompt (a prompt on first visit trains people to dismiss it,
    /// and the trust surface is already where the posture is explained), so this
    /// only says whether the affordance EXISTS, never that anything should pop
    /// up.
    #[must_use]
    pub fn can_bless_name(&self) -> bool {
        self.mutable_name
            .as_ref()
            .is_some_and(crate::pins::MutableNameTrust::is_blessable)
    }
}

// -- The chrome PRESENTATION rules -----------------------------------------
//
// The display rules the window paints FROM a [`ChromeState`]: the status line,
// the trust indicator (+ its detail and CSS class), the error banner, the
// invalid-entry badge, and the URL bar's load progress. They are pure functions
// of the chrome facts above, so they live HERE rather than at an edge (task
// `desktop-chrome-presentation-into-core`, `docs/adr/0011`): they were written
// in the GTK binary, and every new window (Win32, AppKit) would otherwise mint
// another copy of one rule set — the mistake the hand-written Kotlin and Swift
// twins already made. An edge is a PAINTER: it calls these and sets widget
// properties.
//
// The `trust-*` / `error-banner*` strings these return are CSS class NAMES only
// by convention — plain stable identifiers for "which of the N states is this",
// which is a presentation fact, not a toolkit call. The stylesheet that gives
// each one a colour stays in the edge that has a stylesheet.
//
// The Kotlin (`WerustCore.kt`) and Swift (`WerustCore.swift`) twins ARE
// collapsed onto these now (task `mobile-chrome-presentation-from-one-derivation`,
// which answered the open question in favour of an extended chrome JSON over
// per-field FFI calls): a non-Rust edge consumes this derivation through
// [`chrome_json`] below, which carries every string these rules return on the
// document both mobile edges already decode each refresh. So this IS the only
// copy (three languages became one), and adding a rule here reaches every edge
// with no per-platform change.
//
// Nothing outside this block may re-implement one of these: the desktop painters
// go through `desktop-paint` (Rust) and the mobile edges through the chrome JSON
// (Kotlin/Swift), and a guard on each
// (`tests/chrome_css_class_set_edge_wiring_shape.rs`,
// `tests/mobile_chrome_presentation_shape.rs`) reds the gate if a twin returns.

/// Whether there is a load to indicate at all: a backend load in flight
/// ([`ChromeState::is_loading`]) **or** a pinned pre-content resolution step (a
/// non-[`Idle`](LoadStep::Idle) step). The second half matters: while the shell
/// resolves an ENS/IPNS name the backend has not started its load yet, so
/// `is_loading()` is false during EXACTLY the long `ronan.eth` freeze window the
/// old banner sat out. `load_step` is `Idle` on every settled/failed chrome, so
/// this can never linger after a load ends.
///
/// A passive view update driven by the existing chrome-refresh pump (NOT a new
/// timer / poll / tight loop), so the Android ANR guard is not regressed. A pure
/// function of [`ChromeState`] so it is testable without a display; the mobile
/// shells apply the SAME rule from the chrome JSON `loading` + `loadStep` facts
/// (task `loading-progress-in-the-url-bar-not-a-banner`).
///
/// This is the IN-FLIGHT counterpart of [`error_banner_visible`]: the error
/// banner appears on a FAILED load, the URL-bar progress appears while a load is
/// STILL RUNNING. The two are mutually exclusive in practice (a load is either in
/// flight or has settled as finished/failed/idle), and unlike the two banners
/// they never compete for a slot at all: progress is painted INSIDE the URL bar.
#[must_use]
pub fn load_progress_visible(state: &ChromeState) -> bool {
    state.is_loading() || state.load_step() != LoadStep::Idle
}

/// The URL bar's progress fraction for the current load: `0.0` on a settled
/// chrome (painting nothing at all), else a value that ADVANCES with the real
/// pipeline phase so a slow load reads as working rather than frozen — the
/// field-test v0.2.7 finding, now answered WITHOUT displacing the page.
///
/// The fractions are deliberately monotonic and never reach `1.0`: the phases are
/// milestones on the actual lifecycle, not a byte-accurate measurement, so the
/// bar must not claim a load is done while it is still running. A load in flight
/// with no phase yet still shows a small sliver, so "something started" is
/// visible immediately. Pure, for the same reason as [`status_line`].
#[must_use]
pub fn load_progress_fraction(state: &ChromeState) -> f64 {
    if !load_progress_visible(state) {
        return 0.0;
    }
    match state.load_step() {
        LoadStep::Idle => 0.1,
        LoadStep::ResolvingName => 0.25,
        LoadStep::FetchingRecord => 0.45,
        LoadStep::FetchingContent => 0.7,
        LoadStep::Rendering => 0.9,
    }
}

/// The phase NAME behind the current progress, for the URL bar's tooltip: the
/// existing [`LoadStep`] hint vocabulary verbatim ("resolving name", "fetching
/// record", …), so the URL bar, the footer status line and the debug Network tab
/// cannot disagree about which phase a slow load is stuck in. A generic
/// "loading" covers a load in flight with no phase known yet, so it never lies
/// about a frozen phase; empty on a settled chrome (there is no phase to name).
/// Pure, for the same reason as [`status_line`].
#[must_use]
pub fn load_progress_hint(state: &ChromeState) -> &'static str {
    if !load_progress_visible(state) {
        return "";
    }
    match state.load_step() {
        LoadStep::Idle => "loading",
        step => step.hint(),
    }
}

/// The label werust's Stop control carries on every edge today: the GTK toolbar
/// button (the themed `process-stop-symbolic` cross), the AppKit toolbar button,
/// and both mobile Stop buttons all read as this glyph.
///
/// It exists so the ONE progress sentence ([`load_progress_tooltip`]) names the
/// SAME affordance everywhere rather than each painter passing its own literal —
/// the parameter is there for an edge whose Stop control really is labelled
/// differently, not as an invitation to fork the wording.
pub const STOP_AFFORDANCE_LABEL: &str = "✕";

/// The URL bar's progress TOOLTIP for the current load: the phase name (the
/// shared [`load_progress_hint`] vocabulary), plus a cancel hint naming the Stop
/// affordance exactly when there is a backend load Stop can cancel. [`None`] on a
/// settled chrome, which CLEARS the tooltip, so a stale phase never lingers on
/// hover.
///
/// The cancel half is deliberately gated on [`ChromeState::is_loading`] rather
/// than on [`load_progress_visible`]: during the PRE-CONTENT resolution window
/// (a name being resolved before the backend load starts) there is no backend
/// load to stop, so promising a cancel there would lie — and Stop is insensitive
/// then anyway, on that same fact.
///
/// `stop_label` is the painter's label for its Stop control ([`STOP_AFFORDANCE_LABEL`]
/// on every werust edge today): the sentence names a UI affordance the EDGE owns,
/// so an edge that labels Stop differently passes its own label instead of
/// forking the sentence. Pure, for the same reason as [`status_line`]; it lives
/// here because both desktop painters had written it out verbatim, which is how
/// the Kotlin and Swift twins began to drift (`docs/adr/0011`).
#[must_use]
pub fn load_progress_tooltip(state: &ChromeState, stop_label: &str) -> Option<String> {
    if !load_progress_visible(state) {
        return None;
    }
    let hint = load_progress_hint(state);
    Some(if state.is_loading() {
        format!("{hint}… — press Stop ({stop_label}) to cancel")
    } else {
        format!("{hint}…")
    })
}

/// The one-line status shown in the chrome: a surfaced failure wins, otherwise a
/// loading indicator that names the REAL pipeline STEP (resolving name / fetching
/// record / fetching content / rendering) so a slow load reads as "working",
/// otherwise idle. Kept pure so it is trivially correct and reusable.
///
/// The step hint is the core's [`ChromeState::load_step`] (driven by the actual
/// lifecycle), so "loading…" gains a live "— <step>" tail while a load is in
/// flight (task `clearer-loading-and-error-indicator`).
#[must_use]
pub fn status_line(state: &ChromeState) -> String {
    if let Some(reason) = &state.last_error {
        format!("failed: {reason}")
    } else if state.is_loading() {
        let hint = state.load_step().hint();
        if hint.is_empty() {
            "loading…".to_string()
        } else {
            format!("loading… — {hint}")
        }
    } else {
        "idle".to_string()
    }
}

/// Whether the PROMINENT in-view error banner should be shown: exactly when the
/// last load failed ([`ChromeState::last_error`] is set).
///
/// The whole point of fail-closed is that the user UNDERSTANDS why nothing
/// rendered (`docs/adr/0001`: the honesty stance). The subtle one-line
/// [`status_line`] footer was "not easily seen" (the human missed a real
/// `ronan.eth` IPNS failure), so a failed load ALSO raises this high-contrast
/// banner across the top of the view — an error state the user cannot miss —
/// while a loading/idle chrome hides it. A pure function of [`ChromeState`] so it
/// is testable without a display; the mobile shells apply the same rule from the
/// chrome JSON.
///
/// # A CHANGED trusted name is failure-class too
///
/// The banner is the repo's one "you cannot miss this" surface, and the settled
/// TOFU decision is that a blessed name resolving to DIFFERENT content gets the
/// SAME prominence a fail-closed failure does (task
/// `ipns-tofu-pin-and-warn-on-change`): it is never silently accepted, and never
/// a hard block. So this rule is "a failure-class state", of which there are now
/// two: a failed load, or a mutable name that changed since the user trusted it
/// ([`ChromeState::mutable_name_changed`]). It is deliberately NOT a second
/// banner surface: a second high-contrast bar would compete with this one for
/// the slot above the page and each edge would have to decide which wins.
///
/// The sibling constraint from `loading-progress-in-the-url-bar-not-a-banner`
/// still holds and is respected: a FAILURE-class banner may displace the page,
/// transient in-flight state may not. A changed pin is failure-class.
#[must_use]
pub fn error_banner_visible(state: &ChromeState) -> bool {
    state.last_error.is_some() || state.mutable_name_changed()
}

/// The PROMINENT error-banner text for a failed load: a protocol-named,
/// accurate reason drawn straight from [`ChromeState::last_error`] (the decoder /
/// resolver taxonomy — e.g. "IPNS record did not verify: …", "points to Swarm,
/// not supported"), never a generic "failed". Empty when there is no failure (the
/// banner is hidden then). Pure, for the same reason as [`status_line`].
///
/// The reason text is the SAME `last_error` the core surfaces, so the banner and
/// the footer never disagree; it is only shown far more prominently.
///
/// A TRANSIENT/timeout failure (retryable) is surfaced DISTINCTLY from a HARD
/// failure (task `clearer-loading-and-error-indicator`): a transient failure
/// reads as a softer "timed out" with an explicit "reload to retry" affordance
/// (the Reload button IS the retry — a failed ENS load re-resolves), while a hard
/// failure keeps the prominent "failed to load" wording with its protocol-named
/// reason. The distinction is the core's [`ChromeState::failure_is_retryable`]
/// (a pure classification of the reason), so the two never disagree with the
/// footer.
///
/// A CHANGED trusted name (the TOFU warning) uses this same banner, with its own
/// legible sentence naming the name, the day the user trusted it, and both CIDs:
/// everything needed to decide, since werust deliberately does not decide for the
/// user. A LOAD failure still wins the banner when both are true: the page in
/// front of the user did not render at all, which is the more immediate fact
/// (and the changed-name warning survives on the trust indicator regardless).
#[must_use]
pub fn error_banner_text(state: &ChromeState) -> String {
    match &state.last_error {
        Some(reason) if state.failure_is_retryable() => {
            format!("⏳ This page timed out — reload to retry: {reason}")
        }
        Some(reason) => format!("⚠ This page failed to load: {reason}"),
        None => match &state.mutable_name {
            Some(name) if name.is_changed() => changed_name_banner_text(name),
            _ => String::new(),
        },
    }
}

/// The changed-trusted-name banner sentence: what changed, when it was trusted,
/// and both CIDs, so the user can look and decide.
///
/// Composed once here (not at an edge) for the same reason every other chrome
/// sentence is, and phrased as the settled decision words it: "this name now
/// points to different content than the version you trusted on `<date>`".
fn changed_name_banner_text(name: &crate::pins::MutableNameTrust) -> String {
    let blessed = name
        .blessed
        .as_ref()
        .expect("a changed name is a blessed name");
    format!(
        "⚠ {} now points to different content than the version you trusted on {}: \
         it resolves to {} now, not {}. Nothing is blocked; look, then re-trust it or leave.",
        name.name,
        blessed.blessed_on(),
        name.cid,
        blessed.cid,
    )
}

/// Every class [`error_banner_css_class`] can return: the error banner's
/// MUTUALLY-EXCLUSIVE severity family (hard vs transient).
///
/// A painter toggles exactly ONE of these on and every other one off, so it must
/// iterate THIS rather than its own literal list — see [`CHROME_CSS_CLASS_SETS`]
/// for why a painter-local list is a latent stale-badge bug.
pub const ERROR_BANNER_CSS_CLASSES: &[&str] = &["error-banner", "error-banner-transient"];

/// The CSS class for the error banner, distinguishing a TRANSIENT/timeout failure
/// (a softer, retryable amber banner) from a HARD failure (the prominent red
/// banner). A pure function of [`ChromeState`] so the banner styling is testable
/// without a display; each edge's painter toggles the two classes exactly
/// like the trust-indicator classes.
///
/// The complete set of classes this can return is
/// [`ERROR_BANNER_CSS_CLASSES`]; a painter derives its toggle list from there.
///
/// The TOFU changed-trusted-name banner deliberately reuses the HARD severity
/// (it is not a retryable timeout, and re-loading will not un-change the name),
/// so it needs no third class and every edge (including the two mobile ones,
/// which colour from the `retryable` FACT rather than from a class) paints it in
/// the loudest treatment they already have.
#[must_use]
pub fn error_banner_css_class(state: &ChromeState) -> &'static str {
    if state.failure_is_retryable() {
        "error-banner-transient"
    } else {
        "error-banner"
    }
}

/// Whether the small "invalid URL" BADGE should be shown: exactly when the last
/// URL-bar entry was INVALID (a scheme-less garbage entry that did not navigate).
///
/// This is the field-finding-D surface (finding D,
/// `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`):
/// a garbage entry does not navigate; instead of silently resetting the bar, the
/// chrome shows a small badge and renders the URL-bar text as invalid (red
/// underline), keeping the typed text for the user to fix. A pure read of the
/// orthogonal [`ChromeState::has_invalid_entry`] axis — distinct from a load
/// failure ([`error_banner_visible`]) — so it is testable without a display and
/// the mobile shells apply the SAME rule from the chrome JSON.
#[must_use]
pub fn invalid_entry_badge_visible(state: &ChromeState) -> bool {
    state.has_invalid_entry()
}

/// The small "invalid URL" badge text for an invalid entry, empty otherwise (the
/// badge is hidden then). Pure, for the same reason as [`invalid_entry_badge_visible`].
#[must_use]
pub fn invalid_entry_badge_text(state: &ChromeState) -> &'static str {
    if state.has_invalid_entry() {
        "⛔ invalid URL"
    } else {
        ""
    }
}

/// The short label the chrome's trust indicator shows: a distinct, legible badge
/// for a content-verified load vs a served-by-an-unverified-origin load
/// (`docs/adr/0001`: the trust posture is a product surface, not a silent
/// internal). A pure function of [`ChromeState`] so it is trivially correct and
/// testable without a display; the label text carries a shield vs a plain-globe
/// glyph so the states read at a glance even before colour.
///
/// The name-via-trusted-RPC state (an ENS-resolved Phase-1 page: bytes verified,
/// but the name->CID mapping came from a trusted RPC) is a DISTINCT middle badge
/// that is deliberately NOT labelled "verified" — Phase 1 makes no
/// name-verification claim.
///
/// While a load is IN FLIGHT (`is_loading()`) the indicator is a NEUTRAL loading
/// state that WINS over the posture, making NO trust claim at all — the
/// trust-honesty fix (`chrome-loading-state-resets-trust-indicator`): on
/// navigation to a possibly differently-trusted page, the indicator must not keep
/// asserting the previous page's (or a not-yet-proven) trust while the new page
/// loads. The real posture is revealed only once the load SETTLES
/// (finished/failed/idle). This loading-wins precedence lives at the same display
/// layer as the two-axis posture precedence, and is applied identically on the
/// mobile shells (they consult the same `loading` fact from the chrome JSON).
///
/// A blessed name that now points to DIFFERENT content is the LOUDEST settled
/// state and wins over every posture (task `ipns-tofu-pin-and-warn-on-change`):
/// `MutableName` and `NameViaTrustedRpc` both say the name COULD be repointed,
/// while this says it WAS, against something the user personally verified. It
/// must never be flattened into either of them. It still loses to the in-flight
/// loading state, like every other settled fact: while a load is running werust
/// asserts nothing at all.
#[must_use]
pub fn trust_indicator(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "⋯ loading…"
    } else if state.mutable_name_changed() {
        "⚠ name points to NEW content"
    } else if state.is_content_verified() {
        "✓ verified"
    } else if state.is_name_via_trusted_rpc() {
        "◈ name via trusted RPC"
    } else if state.is_mutable_name() {
        "◇ content verified, mutable name"
    } else {
        "⚠ unverified origin"
    }
}

/// The longer explanation shown as the trust indicator's tooltip, so the badge is
/// self-explaining on hover. Pure, for the same reason as [`trust_indicator`].
#[must_use]
pub fn trust_indicator_detail(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "werust is loading this page and is not yet asserting a trust level for it: the trust indicator shows the real posture only once the load settles."
    } else if state.mutable_name_changed() {
        "You TRUSTED this name at a specific version, and it now points to DIFFERENT content. The bytes still hash-verified, so this is not a broken page, it is a CHANGED one: the name's controller repointed it, which may be a normal site update or may not be. werust neither accepts this silently nor blocks it; open this surface to compare the versions, then re-trust the name or leave."
    } else if state.is_content_verified() {
        "This page was content-verified: its bytes were hash-checked against their content identifier on the content-addressed path."
    } else if state.is_name_via_trusted_rpc() {
        "This page's content was hash-verified, but its name was resolved over a TRUSTED RPC (not a light client), which could misdirect the name to different content. werust makes no name-verification claim here."
    } else if state.is_mutable_name() {
        "This page's content was hash-verified, but its name is MUTABLE: the controller (an IPNS key holder, or an ENS name owner) can repoint it to different content at any time. werust makes no immutability claim here."
    } else {
        "This page was served by an origin werust does not trust by default; its content was not hash-verified."
    }
}

/// Every class [`trust_indicator_css_class`] can return: the trust indicator's
/// MUTUALLY-EXCLUSIVE posture family (the neutral loading badge plus the four
/// settled [`TrustPosture`] badges).
///
/// A painter toggles exactly ONE of these on and every other one off, so it must
/// iterate THIS rather than its own literal list — see [`CHROME_CSS_CLASS_SETS`]
/// for why a painter-local list is a latent stale-badge bug. The debug view's
/// per-request trust column REUSES this family (ADR-0006, one vocabulary), so it
/// is covered by the same guarantee.
pub const TRUST_INDICATOR_CSS_CLASSES: &[&str] = &[
    "trust-loading",
    "trust-verified",
    "trust-name-trusted-rpc",
    "trust-mutable-name",
    "trust-name-changed",
    "trust-unverified",
];

/// The CSS class for the current posture's badge — exactly one of the trust
/// classes ([`TRUST_INDICATOR_CSS_CLASSES`]). A pure function of [`ChromeState`]
/// so the badge styling is testable without a display.
///
/// The complete set of classes this can return is
/// [`TRUST_INDICATOR_CSS_CLASSES`]; a painter derives its toggle list from there.
#[must_use]
pub fn trust_indicator_css_class(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "trust-loading"
    } else if state.mutable_name_changed() {
        "trust-name-changed"
    } else if state.is_content_verified() {
        "trust-verified"
    } else if state.is_name_via_trusted_rpc() {
        "trust-name-trusted-rpc"
    } else if state.is_mutable_name() {
        "trust-mutable-name"
    } else {
        "trust-unverified"
    }
}

/// Whether the trust surface offers the TOFU BLESS action for the current page:
/// exactly when the page is a name-resolved load whose name is not already
/// blessed at this very CID ([`ChromeState::can_bless_name`]).
///
/// The BLESS is an EXPLICIT user action reached FROM the trust indicator, never a
/// first-visit prompt: a prompt on first visit trains people to dismiss it, and
/// the trust surface is already the place the posture is explained (the settled
/// UX decision, `docs/spikes/ipns-tofu-pin-and-warn-on-change/DECISIONS.md`). So
/// the core says only whether the AFFORDANCE exists; every edge already has a
/// surface behind the badge (a GTK popover, an AppKit tooltip/panel, the mobile
/// alert both phones show on tap), and this is one more line + one more button in
/// it. Nothing here pops anything up.
///
/// Hidden while a load is in flight, like every other settled trust fact: what
/// the user would be blessing is not known until the load settles.
#[must_use]
pub fn trust_pin_action_visible(state: &ChromeState) -> bool {
    !state.is_loading() && state.can_bless_name()
}

/// The label of the trust surface's BLESS action, empty when it is not offered.
///
/// Two wordings, because the action means two different things: on a name with no
/// pin it is the first-use bless ("trust this content"), and on a name that has
/// CHANGED since it was blessed it is the SSH-host-key "I have looked, and I
/// accept the new content": the same button, a materially different decision, so
/// it must not read the same.
#[must_use]
pub fn trust_pin_action_label(state: &ChromeState) -> &'static str {
    if !trust_pin_action_visible(state) {
        ""
    } else if state.mutable_name_changed() {
        "🔒 Trust the NEW content of this name"
    } else {
        "🔒 Trust this content"
    }
}

/// The body of the trust surface's TOFU section: the name, the CID it resolves to
/// right now, and what (if anything) the user blessed for it. Empty when the
/// current page has no mutable name, in which case the surface shows only the
/// posture explanation it always did.
///
/// Composed here rather than at an edge for the same reason
/// [`trust_indicator_detail`] is: it was written once in the GTK popover and
/// would otherwise be re-written in AppKit, Kotlin and Swift: the exact drift
/// that shipped the trust EXPLANATION desktop-only for months (`docs/adr/0011`).
/// It carries the CIDs verbatim because comparing them is the whole decision the
/// user is being asked to make.
#[must_use]
pub fn trust_pin_detail(state: &ChromeState) -> String {
    let Some(name) = &state.mutable_name else {
        return String::new();
    };
    let head = format!(
        "{} is a MUTABLE name: its controller can repoint it at any time.\nIt resolves to {} right now.",
        name.name, name.cid
    );
    match &name.blessed {
        None => format!("{head}\nYou have not trusted a version of this name yet."),
        Some(pin) if pin.cid == name.cid => format!(
            "{head}\nYou trusted exactly this content on {}.",
            pin.blessed_on()
        ),
        Some(pin) => format!(
            "{head}\nOn {} you trusted {} instead, when werust reported it as “{}”.",
            pin.blessed_on(),
            pin.cid,
            crate::debug::trust_posture_wire_name(pin.posture),
        ),
    }
}

/// The COMPLETE chrome CSS-class set, grouped into the MUTUALLY-EXCLUSIVE
/// FAMILIES a painter toggles as a unit: the trust indicator's postures
/// ([`TRUST_INDICATOR_CSS_CLASSES`]) and the error banner's severities
/// ([`ERROR_BANNER_CSS_CLASSES`]).
///
/// WHY THIS EXISTS: the class NAMES are decided here (the `*_css_class` rules
/// above), but a painter's correctness depends on knowing EVERY name a family can
/// produce — it turns one class on and must turn all the others OFF, so a name
/// its own literal list omits is a name nothing ever clears, and a stale badge
/// colour lingers across a transition. With the list copied into each painter,
/// adding a fifth posture here would leave every painter stale with a GREEN test
/// suite (the latent bug this const closes, before the AppKit and Win32 painters
/// inherit it). Two tests pin it: the core's exhaustiveness test (every value a
/// `*_css_class` function can return is a member, and the set carries no dead
/// name) and each edge's no-unstyled-class test (every exported name has a rule
/// in that edge's stylesheet, so a correctly-toggled but INVISIBLE state reds the
/// gate too).
///
/// Decisions recorded at
/// `docs/spikes/export-the-chrome-css-class-set-from-core/DECISIONS.md`: why a
/// `pub const` slice rather than an enum, why the set is grouped by family rather
/// than exported flat, and how the exhaustiveness test's drive is made exhaustive
/// BY CONSTRUCTION (it iterates [`TrustPosture::ALL`] / [`FailureKind::ALL`],
/// which a compile-time check keeps complete, so a fifth posture cannot arrive
/// with a stale set and a green suite).
///
/// LAYERING: these are stable state IDENTIFIERS, not styling. The stylesheet
/// that gives each name a colour stays in the edge that has a stylesheet (the
/// GTK `APP_CSS`); core has no notion of colour.
///
/// SCOPE: this is the TOGGLING set — the chrome families a painter turns on and
/// off on one widget — and stays deliberately narrower than the set of families
/// this crate exports (the debug view's console levels colour a ROW, so they are
/// not a member). A painter's no-unstyled-class GATE must therefore not iterate
/// this: it iterates [`CssClassFamily::ALL`], which covers every exported family.
pub const CHROME_CSS_CLASS_SETS: &[&[&str]] =
    &[TRUST_INDICATOR_CSS_CLASSES, ERROR_BANNER_CSS_CLASSES];

/// Every CSS-class FAMILY this crate exports, as one aggregate for the painters'
/// COVERAGE gates.
///
/// WHY THIS EXISTS: [`CHROME_CSS_CLASS_SETS`] and friends made each family
/// exhaustive over its CLASSES, so a fifth [`TrustPosture`] cannot arrive with a
/// stale set and a green suite. The set of FAMILIES had no such tooth: each
/// painter's no-unstyled-class gate hand-wrote which families it checked (the GTK
/// `APP_CSS` guard, the macOS palette guard), so a SIXTH family would join
/// neither gate and render invisibly on BOTH desktops while both suites stayed
/// green — the same failure one level up. Both gates now iterate [`ALL`](CssClassFamily::ALL),
/// which the const check below keeps complete, so a new family reds both gates
/// the moment it lands.
///
/// NOT a replacement for [`CHROME_CSS_CLASS_SETS`], which keeps its NARROWER
/// meaning: that set is what a chrome painter TOGGLES on one widget (exactly one
/// class on, every other one off), which is why the debug view's row levels
/// ([`DEBUG_CONSOLE_CSS_CLASSES`](crate::debug::DEBUG_CONSOLE_CSS_CLASSES)) are
/// deliberately not a member of it. This aggregate is for coverage only: a
/// painter must never toggle a console class on a chrome widget.
///
/// LAYERING is unchanged: these are stable state IDENTIFIERS, never styling. The
/// stylesheet that gives each name a colour stays in the edge that has one.
///
/// Shape decisions (why an enum here when the families are `const` slices, and
/// why the stop label is a parameter next door) are recorded at
/// `docs/spikes/one-derivation-close-the-aggregate-and-tooltip-gaps/DECISIONS.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssClassFamily {
    /// The chrome trust indicator's posture badges
    /// ([`TRUST_INDICATOR_CSS_CLASSES`]), toggled on the trust badge.
    TrustIndicator,
    /// The prominent error banner's severities ([`ERROR_BANNER_CSS_CLASSES`]),
    /// toggled on the banner.
    ErrorBanner,
    /// The debug view's console LEVELS
    /// ([`DEBUG_CONSOLE_CSS_CLASSES`](crate::debug::DEBUG_CONSOLE_CSS_CLASSES)),
    /// which colour a row rather than being toggled on one widget.
    DebugConsole,
}

impl CssClassFamily {
    /// Every exported class family. The single source of truth for "which
    /// families exist", so a coverage gate iterates THIS instead of re-listing
    /// the families in a literal that silently goes stale. Kept complete by the
    /// const check below.
    pub const ALL: [CssClassFamily; 3] = [
        CssClassFamily::TrustIndicator,
        CssClassFamily::ErrorBanner,
        CssClassFamily::DebugConsole,
    ];

    /// The complete list of class names in this family — the very `const` the
    /// rules that produce them are exported beside, never a second copy.
    #[must_use]
    pub const fn classes(self) -> &'static [&'static str] {
        match self {
            CssClassFamily::TrustIndicator => TRUST_INDICATOR_CSS_CLASSES,
            CssClassFamily::ErrorBanner => ERROR_BANNER_CSS_CLASSES,
            CssClassFamily::DebugConsole => crate::debug::DEBUG_CONSOLE_CSS_CLASSES,
        }
    }
}

/// Keeps [`CssClassFamily::ALL`] EXHAUSTIVE at compile time, by exactly the
/// construction [`LoadStep::ALL`]'s check uses (see it for the full reasoning):
/// the total `listed` match refuses to compile until a new family is named here,
/// and the arm it is named in (`… => CssClassFamily::ALL[3]`) refuses to compile
/// until the family is in `ALL` as well.
///
/// This is the tooth the two painters' coverage gates hang from: both iterate
/// `ALL`, so a sixth exported family cannot reach a green build without joining
/// the GTK stylesheet gate AND the macOS palette gate (task
/// `one-derivation-close-the-aggregate-and-tooltip-gaps`). [`CssClassFamily::classes`]
/// is a total match for the same reason: a family that named itself here but
/// listed no classes would not compile either.
const _CSS_CLASS_FAMILY_ALL_IS_EVERY_FAMILY_IN_SLOT_ORDER: () = {
    const fn listed(family: CssClassFamily) -> CssClassFamily {
        match family {
            CssClassFamily::TrustIndicator => CssClassFamily::ALL[0],
            CssClassFamily::ErrorBanner => CssClassFamily::ALL[1],
            CssClassFamily::DebugConsole => CssClassFamily::ALL[2],
        }
    }
    let mut i = 0;
    while i < CssClassFamily::ALL.len() {
        assert!(
            listed(CssClassFamily::ALL[i]) as u8 == CssClassFamily::ALL[i] as u8,
            "CssClassFamily::ALL must hold every family, once, in slot order"
        );
        i += 1;
    }
};

/// The chrome as the JSON document a NON-RUST edge paints from: the [`ChromeState`]
/// FACTS plus every string the presentation rules above DERIVE from them.
///
/// The shape (stable, pinned whole by
/// `the_chrome_json_document_is_exactly_the_facts_plus_the_derived_fields`):
///
/// ```json
/// {
///   "url": "", "loadState": "idle", "loading": false, "loadStep": "idle",
///   "canGoBack": false, "canGoForward": false, "trustPosture": "unverified-origin",
///   "error": null, "failureKind": null, "retryable": false, "invalidEntry": null,
///   "mutableName": null, "mutableNameCid": null, "blessedCid": null, "nameChanged": false,
///   "statusLine": "idle", "trustIndicator": "⚠ unverified origin",
///   "trustIndicatorDetail": "…", "errorBannerVisible": false, "errorBannerText": "",
///   "invalidEntryBadgeVisible": false, "invalidEntryBadgeText": "",
///   "loadProgressVisible": false, "loadProgressFraction": 0.0, "loadProgressHint": "",
///   "trustPinActionVisible": false, "trustPinActionLabel": "", "trustPinDetail": ""
/// }
/// ```
///
/// # Why the derived strings ride the chrome JSON
///
/// This is the CARRIER for the two mobile edges, the counterpart of
/// `desktop_paint::ChromePaint` for the native-widget desktop
/// windows: Kotlin and Swift cannot call these `pub fn`s, so before this landed
/// each had written its OWN `statusLine()` / `trustIndicator()` / `errorBanner()`
/// / `invalidEntryBadge()` / `loadProgress*()` twin: one rule set in three
/// languages, which had already DRIFTED (the trust EXPLANATION,
/// [`trust_indicator_detail`], shipped desktop-only for months; the load-progress
/// unit was a fraction in Rust and Swift but a percent in Kotlin). Carrying the
/// derived strings HERE lets each mobile edge read a field instead of running a
/// `when`/`switch`, with no new FFI surface: both edges already decode this exact
/// document on every chrome refresh. The rejected alternative (exposing each rule
/// over the FFI and calling it per field) is recorded, with the measured cost of
/// this one, at
/// `docs/spikes/mobile-chrome-presentation-from-one-derivation/DECISIONS.md`.
///
/// It is a CARRIER, not a second derivation: every derived field below is the
/// return value of one of the `pub fn`s above, and
/// `the_chrome_json_carries_the_derivation_verbatim_for_every_chrome_shape`
/// asserts exactly that across every shape of [`ChromeState`] a rule can branch
/// on. Nothing here decides anything.
///
/// # Vocabulary
///
/// Every enum FACT keeps the one wire spelling the rest of the system speaks
/// ([`LoadStep::wire_name`], [`FailureKind::wire_name`] and
/// [`trust_posture_wire_name`](crate::debug::trust_posture_wire_name), the very
/// names the debug view's Network tab uses, `docs/adr/0006`), so mobile never
/// reads a second spelling of a posture or a phase. The derived fields are named
/// after the RULES that produce them (`statusLine` is [`status_line`],
/// `trustIndicatorDetail` is [`trust_indicator_detail`], …), so an edge field and
/// a core function can be matched by name.
///
/// # Layering
///
/// COLOUR is not here, deliberately: the `*_css_class` rules are exported for the
/// painters that have a stylesheet (GTK) or a palette (`desktop-paint`), and the
/// mobile edges pick their own native colours off the same FACTS the classes
/// branch on (`retryable`, `trustPosture`), the same split that keeps the GTK
/// stylesheet in the GTK edge.
#[must_use]
pub fn chrome_json(state: &ChromeState) -> String {
    serde_json::json!({
        // --- The FACTS: `ChromeState`, in the wire vocabulary. ---
        "url": state.url_text,
        "loadState": load_state_wire_name(state.load_state),
        "loading": state.is_loading(),
        "loadStep": state.load_step().wire_name(),
        "canGoBack": state.can_go_back,
        "canGoForward": state.can_go_forward,
        "trustPosture": crate::debug::trust_posture_wire_name(state.trust_posture),
        "error": state.last_error,
        "failureKind": state.failure_kind().map(FailureKind::wire_name),
        "retryable": state.failure_is_retryable(),
        "invalidEntry": state.invalid_entry,
        "mutableName": state.mutable_name.as_ref().map(|n| n.name.clone()),
        "mutableNameCid": state.mutable_name.as_ref().map(|n| n.cid.clone()),
        "blessedCid": state.mutable_name.as_ref().and_then(|n| n.blessed.as_ref()).map(|p| p.cid.clone()),
        "nameChanged": state.mutable_name_changed(),
        // --- The DERIVATION: the presentation rules above, verbatim. ---
        "statusLine": status_line(state),
        "trustIndicator": trust_indicator(state),
        "trustIndicatorDetail": trust_indicator_detail(state),
        "errorBannerVisible": error_banner_visible(state),
        "errorBannerText": error_banner_text(state),
        "invalidEntryBadgeVisible": invalid_entry_badge_visible(state),
        "invalidEntryBadgeText": invalid_entry_badge_text(state),
        "loadProgressVisible": load_progress_visible(state),
        "loadProgressFraction": load_progress_fraction(state),
        "loadProgressHint": load_progress_hint(state),
        "trustPinActionVisible": trust_pin_action_visible(state),
        "trustPinActionLabel": trust_pin_action_label(state),
        "trustPinDetail": trust_pin_detail(state),
    })
    .to_string()
}

/// The stable, lower-case wire name of a [`LoadState`] for [`chrome_json`].
///
/// Private, and the only fact name decided here rather than beside its type: the
/// other three (`LoadStep`, `FailureKind`, `TrustPosture`) own their `wire_name`
/// already, while [`LoadState`] lives in the `renderer` seam crate, which has no
/// wire concern of its own. The names are unchanged from the pre-collapse mobile
/// encoders.
fn load_state_wire_name(state: LoadState) -> &'static str {
    match state {
        LoadState::Idle => "idle",
        LoadState::Started => "started",
        LoadState::Committed => "committed",
        LoadState::Finished => "finished",
        LoadState::Failed => "failed",
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
    /// The hand-off from the `ipfs://` scheme handler for a site's `_redirects`
    /// 3xx rule: the `ipfs://<rootcid><to>` URL to NAVIGATE to (IPIP-0002's
    /// redirect, [`crate::ipfs::RedirectSink`]).
    ///
    /// A 3xx is a navigation, not an answer to the intercepted request, and the
    /// scheme handler cannot navigate (it is a `Send` closure that on desktop runs
    /// off the UI thread entirely). So it PUSHES the target into this shared sink
    /// and [`pump`](BrowserShell::pump) DRAINS it on the shell's existing cadence,
    /// navigating through the normal path so the URL bar + history move and the
    /// target is hash-verified by the fresh retrieval that navigation triggers.
    ///
    /// The edge that installs the scheme handler holds the OTHER clone of this
    /// sink ([`with_redirect_sink`](BrowserShell::with_redirect_sink)); a shell
    /// built without one keeps an unused empty sink, so nothing changes for a
    /// caller that does not wire `ipfs://` at all.
    redirects: crate::ipfs::RedirectSink,
    /// The history entries the IN-FLIGHT Back move must SKIP over: the
    /// `frame_key`s of the urls the redirect chain being left redirected AWAY
    /// from ([`crate::ipfs::RedirectSink::redirect_sources`]), snapshotted by
    /// [`go_back`](BrowserShell::go_back).
    ///
    /// werust PUSHES a redirect target as a NEW history entry rather than
    /// REPLACING the redirecting one (the seam has no replace-current-entry), so
    /// the redirected-FROM url is still in history and a plain Back would land on
    /// it, re-match its 3xx rule, and bounce the user forward again. Skipping it
    /// is the standard emulation of the entry a real browser would have replaced.
    ///
    /// The skip is driven from [`pump`](BrowserShell::pump) rather than from
    /// `go_back` itself because a history move settles ASYNCHRONOUSLY: right after
    /// `Renderer::go_back` the backend still reports the PREVIOUS entry as its
    /// `current_url` (WebKitGTK lands it only on the `load-changed` signal), so the
    /// LOAD EVENT is the first place the landed url is knowable. Emptied as soon as
    /// a load lands on anything that is not a remembered source, so it can never
    /// skip an entry the user reached deliberately later.
    back_skip: Vec<String>,
    /// The `frame_key` of the entry the in-flight Back skip has ALREADY stepped
    /// off, so the trailing lifecycle events of that abandoned load
    /// (`Committed`/`Finished`, already queued when the skip was issued) are
    /// dropped instead of folded in.
    ///
    /// Without this the bar would flash the entry the user is not staying on, and
    /// — worse — `pump`'s `note_navigation` would re-adopt it as the TOP-LEVEL
    /// document, so a scheme-handler request for it resolving late would look like
    /// the main frame and re-queue the very redirect the skip exists to avoid.
    /// Cleared as soon as an event for any other url arrives.
    back_skip_issued: Option<String>,
    /// The bounded CONSOLE + NETWORK capture store behind werust's in-app debug
    /// menu ([`crate::debug::DebugCapture`]).
    ///
    /// The shell owns it so it reaches every edge over the SAME surface the
    /// chrome does ([`debug_json`](BrowserShell::debug_json)), and shares it with
    /// the per-platform CAPTURE POINTS by handle: they hold a clone (created at
    /// the edge and handed here via
    /// [`with_debug_capture`](BrowserShell::with_debug_capture), or taken from
    /// [`debug_capture`](BrowserShell::debug_capture)) and push entries into the
    /// same store, possibly off the UI thread: the same shared-sink shape
    /// [`redirects`](BrowserShell::redirects) uses.
    ///
    /// Capture is READ-ONLY observation: nothing here feeds back into the load
    /// path, the verification, or the chrome's own
    /// [`trust_posture`](ChromeState::trust_posture). A shell whose edge wires no
    /// capture point simply keeps an empty store, so nothing changes for a caller
    /// that does not use the debug menu at all.
    debug: crate::debug::DebugCapture,
    /// The trust-on-first-use pin store: the CIDs the user has BLESSED for
    /// MUTABLE names (task `ipns-tofu-pin-and-warn-on-change`,
    /// `docs/adr/0006`'s mutability axis).
    ///
    /// A READ-THROUGH CACHE of the store, so the paint path never touches the
    /// filesystem: read once at launch, and REPLACED by the re-read store every
    /// time the user blesses something
    /// ([`bless_current_name`](BrowserShell::bless_current_name)). It is a cache
    /// and not the truth, because another window may be writing the same file
    /// (see [`pin_store`](BrowserShell::pin_store)); the file is the truth.
    /// The shell holds it for the same reason it holds the chrome: it is the one
    /// place that knows BOTH the name the user typed and the CID it resolved to.
    ///
    /// ADVISORY ONLY: it feeds [`ChromeState::mutable_name`] and nothing else. No
    /// load path, no verification and no posture reads it, so an empty store
    /// (a fresh install, or an unreadable file) is exactly the pre-TOFU browser.
    pins: crate::pins::TrustedNamePins,
    /// WHERE the pin store is read from and written back to: the settings
    /// directory in production, a scratch directory in a test that opts in, and
    /// NOWHERE inside this crate's own test binary. See [`PinStoreLocation`].
    pin_store: PinStoreLocation,
}

/// WHERE a [`BrowserShell`]'s trusted name pins are read from and written back
/// to: the ONE place that answers "which `pins.json`, if any".
///
/// Three states rather than an `Option<PathBuf>`, because "no directory was
/// given" means two different things and conflating them is the hermeticity hole
/// this type exists to close: production wants the real settings directory, and
/// a test wants NO store at all. Owning `load`/`save` here also means a FUTURE
/// mutation (a "forget this pin" action, say) cannot re-introduce the wholesale
/// rewrite [`bless_current_name`](BrowserShell::bless_current_name) was fixed of,
/// by forgetting to re-read. Task `pin-store-read-modify-write-and-test-isolation`
/// (`docs/spikes/pin-store-read-modify-write-and-test-isolation/DECISIONS.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum PinStoreLocation {
    /// The [`retrieval`](crate::retrieval) settings directory: `pins.json` beside
    /// `retrieval.json`, under the same
    /// [`WERUST_SETTINGS_DIR`](crate::retrieval::SETTINGS_DIR_ENV) lever (one
    /// mechanism, `pins`' settled decision 2). The production default.
    Settings,
    /// A SPECIFIC directory ([`with_pins_dir`](BrowserShell::with_pins_dir)): the
    /// shell-level twin of
    /// [`TrustedNamePins::load_from`](crate::pins::TrustedNamePins::load_from),
    /// so a test drives the WHOLE bless -> persist -> re-resolve -> warn loop
    /// against its own scratch directory, touching neither the real store nor
    /// process-global env.
    Dir(std::path::PathBuf),
    /// NO durable store: nothing is read, nothing is written. A bless still holds
    /// for THIS session (the chrome updates) and simply reports itself
    /// unpersisted, exactly as it does when there is no settings directory.
    ///
    /// This is the DEFAULT inside `werust_core`'s own test binary (see
    /// [`PinStoreLocation::default`]): a core test that has not asked for a store
    /// must not read the DEVELOPER's blessed names, or a machine where
    /// `ronan.eth` happens to be blessed would flip a TOFU axis inside a fixture
    /// and red an unrelated chrome assertion, reproducing nowhere else. It is the
    /// read-side twin of the work contract's shared-write rule.
    Ephemeral,
}

impl Default for PinStoreLocation {
    fn default() -> Self {
        if cfg!(test) {
            Self::Ephemeral
        } else {
            Self::Settings
        }
    }
}

impl PinStoreLocation {
    /// Re-read the store from disk, or `None` when there is nothing durable to
    /// read (an [`Ephemeral`](PinStoreLocation::Ephemeral) shell, or
    /// [`Settings`](PinStoreLocation::Settings) with no settings directory on
    /// this system).
    ///
    /// `None` is deliberately distinct from "an empty store": a caller that has
    /// pins in memory keeps them rather than dropping them on the floor, because
    /// there is no file whose contents could have superseded them.
    fn load(&self) -> Option<crate::pins::TrustedNamePins> {
        match self {
            Self::Settings => crate::retrieval::settings_dir()
                .map(|dir| crate::pins::TrustedNamePins::load_from(&dir)),
            Self::Dir(dir) => Some(crate::pins::TrustedNamePins::load_from(dir)),
            Self::Ephemeral => None,
        }
    }

    /// Persist `pins` here, reporting whether it reached disk. Always `false` for
    /// [`Ephemeral`](PinStoreLocation::Ephemeral), which has nowhere to write.
    fn save(&self, pins: &crate::pins::TrustedNamePins) -> bool {
        match self {
            Self::Settings => pins.save(),
            Self::Dir(dir) => pins.save_to(dir),
            Self::Ephemeral => false,
        }
    }
}

/// The ENS identity behind an underlying `ipfs://<cid>` load: the `.eth` name to
/// show in the bar, and whether the name is MUTABLE (an `ipns-ns` / repointable
/// name), so the right posture axes can be re-marked on a reload / history move.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EnsIdentity {
    /// The `.eth` identity the user typed — the name, PLUS any sub-path
    /// (`ronan.eth/blog/`) — kept in the URL bar in place of the CID(+path).
    name: String,
    /// Whether the resolved name is MUTABLE (`ipns-ns`), so a history/reload load
    /// re-marks the mutable axis too ([`Renderer::mark_mutable_name`]).
    mutable: bool,
    /// The ROOT CID this ENS site resolved to (the bare `<cid>`, no path), so a
    /// history/reload/SPA nav onto ANY `<rootcid>/<path>` of the SAME site is
    /// recognised by a ROOT-CID-PREFIX match, not only the exact normalized entry
    /// key. This is what closes the v0.2.4 `ipfs://`-reappears leak (`ens_pages`
    /// was root-entry-only, so a sub-path return missed and leaked the raw CID).
    root_cid: String,
    /// The ROOT `.eth` name of this site (`ronan.eth`, never a sub-path display),
    /// so a root-CID-prefix match on a sub-path re-derives `ronan.eth/<in-site-path>`
    /// (the whole-site identity), not the exact stored entry's display.
    root_name: String,
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
        // The default IPNS record source (`ipns::default_record_source`): a
        // trustless-gateway fetch over the bound HTTP `Fetcher`, pointed at the
        // user's chosen retrieval backend, with the record step's own split-out
        // timeouts. Built by the CORE helper, not here, so the headless
        // `werust resolve` — which resolves the same names with no shell —
        // cannot end up pointed at a different gateway or budget.
        let ipns_source = Box::new(crate::ipns::default_record_source());
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
        // WHERE the pins live: the settings directory in production, and NOTHING
        // inside this crate's own test binary, so no core test can read the
        // developer's blessed names (see `PinStoreLocation::Ephemeral`).
        let pin_store = PinStoreLocation::default();
        let mut shell = Self {
            renderer,
            chrome: ChromeState::default(),
            provider,
            ipns_source,
            url_override: None,
            pinned_root_key: None,
            ens_pages: HashMap::new(),
            resolving_step: None,
            redirects: crate::ipfs::RedirectSink::new(),
            back_skip: Vec::new(),
            back_skip_issued: None,
            debug: crate::debug::DebugCapture::new(),
            // The blessed CIDs, read once per launch (and re-read on every bless).
            // A missing/unreadable store is simply empty, which is the pre-TOFU
            // browser (fail-safe).
            pins: pin_store.load().unwrap_or_default(),
            pin_store,
        };
        shell.refresh_chrome();
        shell
    }

    /// Point the trust-on-first-use pin store at a SPECIFIC directory instead of
    /// the [`retrieval`](crate::retrieval) settings directory, re-reading it.
    ///
    /// The shell-level explicit-directory seam (the shell's private
    /// `PinStoreLocation`): a test drives the whole
    /// bless -> persist -> re-resolve -> warn loop against its own scratch
    /// directory, so it never touches the real `pins.json` (the shared-write rule)
    /// and never mutates process-global env. Production leaves it unset; an edge
    /// that wants a platform-specific location sets
    /// [`WERUST_SETTINGS_DIR`](crate::retrieval::SETTINGS_DIR_ENV) instead, which
    /// moves `retrieval.json` and `pins.json` together (one mechanism).
    #[must_use]
    pub fn with_pins_dir(mut self, dir: &std::path::Path) -> Self {
        self.pin_store = PinStoreLocation::Dir(dir.to_path_buf());
        self.pins = self.pin_store.load().unwrap_or_default();
        self.refresh_chrome();
        self
    }

    /// Share the `_redirects` 3xx [`RedirectSink`](crate::ipfs::RedirectSink) the
    /// platform's `ipfs://` scheme handler pushes redirect targets into, so
    /// [`pump`](BrowserShell::pump) performs them as real navigations.
    ///
    /// The scheme handler is installed on the backend BEFORE the shell owns it
    /// (every edge's `install_ipfs`), so the sink is created at the edge, cloned
    /// into the handler, and handed here — both clones are the same sink. Without
    /// this call a matched 3xx still fails closed (nothing is served for the old
    /// URL); it simply never navigates, which is the pre-3xx behaviour.
    #[must_use]
    pub fn with_redirect_sink(mut self, redirects: crate::ipfs::RedirectSink) -> Self {
        self.redirects = redirects;
        self
    }

    /// Share the [`DebugCapture`](crate::debug::DebugCapture) the platform's
    /// console/network CAPTURE POINTS push into, so the debug view renders the
    /// entries they captured.
    ///
    /// The capture points are installed on the backend BEFORE the shell owns it
    /// (each edge's console/resource-load hooks), so the store is created at the
    /// edge, cloned into the hooks, and handed here, so both clones are the same
    /// store. This mirrors [`with_redirect_sink`](BrowserShell::with_redirect_sink)
    /// exactly. Without this call the shell keeps its own empty store, which is
    /// simply never fed (the pre-capture behaviour); an edge may equally take the
    /// shell's own store via [`debug_capture`](BrowserShell::debug_capture)
    /// instead of building one.
    #[must_use]
    pub fn with_debug_capture(mut self, debug: crate::debug::DebugCapture) -> Self {
        self.debug = debug;
        self
    }

    /// The shell's bounded console + network capture store.
    ///
    /// Both a READ surface (the debug view lists
    /// [`console`](crate::debug::DebugCapture::console) /
    /// [`network`](crate::debug::DebugCapture::network) and its Clear button calls
    /// [`clear`](crate::debug::DebugCapture::clear)) and the PUSH surface a
    /// capture point clones. It is `&` rather than `&mut` because the store is a
    /// shared handle with its own interior locking, so a `Send` capture closure
    /// can own a clone.
    #[must_use]
    pub fn debug_capture(&self) -> &crate::debug::DebugCapture {
        &self.debug
    }

    /// Whether a request for `uri` is the MAIN FRAME (the top-level document this
    /// shell is loading) rather than a sub-resource of it.
    ///
    /// The ONE main-frame predicate in the codebase, re-exported from the
    /// [`RedirectSink`](crate::ipfs::RedirectSink::is_main_frame) the 3xx gate
    /// already drives: the shell reports every top-level navigation into the sink
    /// (`note_navigation`), so the sink holds the authoritative top-level URL,
    /// normalized through `frame_key` so the WebKit authority-less `ipfs:///<cid>`
    /// form and a query/fragment still match.
    ///
    /// The debug NETWORK capture calls this to decide which row is the
    /// main-document row (which takes the LOAD's own two-axis posture so the
    /// Network tab cannot contradict the trust indicator, ADR-0006). It must NOT
    /// compare against [`ChromeState::url_text`], which is the DISPLAY identity:
    /// on an ENS load the shell pins the name there, so the compare would never
    /// fire on exactly the page it was mandated for.
    #[must_use]
    pub fn is_main_frame(&self, uri: &str) -> bool {
        self.redirects.is_main_frame(uri)
    }

    /// The LIVE trust posture of the current load, read fresh from the backend
    /// (the seam's [`Renderer::trust_posture`]), NOT the CACHED
    /// [`ChromeState::trust_posture`] snapshot.
    ///
    /// The cached snapshot is only as fresh as the last `refresh_chrome`, which
    /// runs on the page commit/finish load signals — AFTER the `ipfs://` scheme
    /// handler has already resolved, marked the backend content-verified, and
    /// the debug NETWORK capture has recorded the main-document row. A capture
    /// that reads the cache in that window stamps the stale pre-verify
    /// `unverified-origin`, DOWNGRADING the row below the honest posture the
    /// indicator is about to show. The debug capture's main-document
    /// reconciliation reads THIS, the same fact the desktop capture reads
    /// directly from its load lifecycle.
    #[must_use]
    pub fn live_trust_posture(&self) -> TrustPosture {
        self.renderer.trust_posture()
    }

    /// The capture store as the debug JSON document each edge's debug view
    /// renders ([`crate::debug::debug_json`]).
    ///
    /// A DEDICATED accessor beside [`chrome`](BrowserShell::chrome) rather than a
    /// section of the chrome JSON: the chrome is re-encoded on every refresh,
    /// while this is read only while the debug view is open (the recorded FFI
    /// decision, see the [`debug`](crate::debug) module docs). Additive either
    /// way: no existing chrome field is touched.
    #[must_use]
    pub fn debug_json(&self) -> String {
        crate::debug::debug_json(&self.debug)
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

    /// The trusted name pins this shell is holding, for tests that assert a
    /// DEFAULT shell (one built without [`with_pins_dir`](BrowserShell::with_pins_dir))
    /// read nothing at all. Test-only: production reads the pin the current page
    /// is subject to off [`chrome`](BrowserShell::chrome)'s `mutable_name` axis,
    /// never the store.
    #[cfg(test)]
    fn pins_for_test(&self) -> &crate::pins::TrustedNamePins {
        &self.pins
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
        // A `.eth` entry, WITH or WITHOUT a sub-path, is the ENS front door: split
        // it into the NAME and the optional PATH so `ronan.eth/blog/` resolves
        // `ronan.eth` and loads the sub-path `ipfs://<cid>/blog/`, instead of
        // falling through to `https://ronan.eth/blog/` (field finding B). A bare
        // `.eth` yields an empty path (unchanged).
        if let Some((name, path)) = eth_name_and_path_from_entry(url) {
            return self.navigate_ens_name(name, path);
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
        // A USER-initiated navigation starts a FRESH redirect chain, so the hop
        // budget bounds one site's `_redirects` chain rather than a whole session.
        // And `target` is the TOP-LEVEL document about to load, which is what tells
        // the scheme handler that an intercepted request for it is the MAIN FRAME
        // (a 3xx on a SUB-RESOURCE must never navigate the page away). Both are
        // reported BEFORE the backend starts the load, so the main document's own
        // request cannot be intercepted before the sink knows about it.
        self.redirects.reset();
        self.redirects.note_navigation(&target);
        // Any pending Back skip belongs to a Back the user has now overtaken.
        self.end_back_skip();
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
    /// This is the tracer-bullet path. The name-to-CID step itself is NOT written
    /// here: it is [`crate::name_resolution::resolve_name_with_progress`], the ONE
    /// callable resolution path (namehash -> registry -> resolver -> ENSIP-7
    /// decode over the shell's
    /// [`EthereumProvider`](crate::ethereum::EthereumProvider), and, for a MUTABLE
    /// `ipns-ns` contenthash, the client-VERIFIED IPNS record fetched over the
    /// shell's untrusted [`IpnsRecordSource`](crate::ipns::IpnsRecordSource)).
    /// The headless `werust resolve` calls the SAME function, so the CLI and the
    /// GUI cannot disagree about what a name resolves to, and a record that fails
    /// verification fails identically in both (task
    /// `cli-resolve-follows-mutable-names-to-the-cid`). What stays HERE is the
    /// shell's own half: the load-step pin, feeding the resolved CID into the
    /// verified `ipfs://` path, and the TRUST flagging:
    ///
    /// * an IMMUTABLE (`ipfs-ns`) name's load is flagged ENS-originated
    ///   ([`Renderer::mark_ens_origin`]) so the resulting posture is
    ///   "content-verified, name via trusted RPC" rather than plain
    ///   `ContentVerified`;
    /// * a MUTABLE (followed `ipns-ns`) name's load is flagged BOTH ENS-originated
    ///   AND mutable-named, so the loudest applicable posture wins
    ///   (`NameViaTrustedRpc` via ENS today; `MutableName` once Phase 2 clears the
    ///   RPC warning) — NEVER immutable `ContentVerified`;
    /// * an unsupported contenthash (swarm/arweave/unknown) never reaches a load
    ///   at all: it is the decoder's graceful, protocol-named failure.
    ///
    /// The `path` (from [`eth_name_and_path_from_entry`], with its leading `/`, or
    /// `""` for a bare name) is threaded into the resolved load: the backend loads
    /// `ipfs://<cid><path>` (e.g. `ipfs://<cid>/blog/`, resolved by the existing
    /// ipfs sub-path + directory-index path), and the bar keeps `<name><path>`
    /// (e.g. `ronan.eth/blog/`) — the identity+path the user typed.
    ///
    /// The address bar keeps `name` (+ any `path`), not the resolved CID: there is
    /// no `https://` rewrite and no gateway redirect.
    ///
    /// Fail-closed: a resolution failure or an unsupported/absent contenthash
    /// FAILS the load with a legible reason surfaced in
    /// [`ChromeState::last_error`], and nothing unverified is ever rendered. A
    /// failed resolution returns `Ok(())` (the front door handled the entry and
    /// surfaced the failure in the chrome), not an `Err`, so the URL bar keeps the
    /// name (+ path) for the user to see the reason — mirroring how a failed load
    /// surfaces its reason rather than throwing.
    fn navigate_ens_name(&mut self, name: &str, path: &str) -> Result<(), RendererError> {
        // The ENS front door proceeds, so any prior invalid-entry state is cleared
        // (a valid route never leaves the badge showing).
        self.chrome.invalid_entry = None;
        // Resolve the name to the content it points at, through the ONE shared
        // resolution path. The pipeline STEP it reaches is reported back through
        // the progress callback (`ResolvingName`, then `FetchingRecord` for a
        // mutable name) and pinned here, so a FAILURE still surfaces the stage it
        // failed at. The callback writes to a local cell rather than to `self`
        // because the resolution borrows the shell's provider + record source; the
        // pin is applied the moment it returns, which is the same state the old
        // inline match left behind at this point (resolution is synchronous, so
        // no caller can observe the shell in between).
        let reached_step = std::cell::Cell::new(None);
        let resolved = crate::name_resolution::resolve_name_with_progress(
            self.provider.as_ref(),
            self.ipns_source.as_ref(),
            name,
            &mut |step| reached_step.set(Some(step)),
        );
        self.resolving_step = reached_step.get();
        match resolved {
            Ok(resolved) => {
                // Feed the resolved CID + the typed sub-path into the verified
                // `ipfs://` path. A MUTABLE name (a followed `ipns-ns` pointer)
                // ALSO flags the load mutable-named: its honest posture is at most
                // `MutableName`, NEVER immutable `ContentVerified`. Via ENS the
                // LOUDER `NameViaTrustedRpc` still wins today (the two-axis display
                // rule); it falls back to `MutableName` once Phase 2 clears the RPC
                // warning — no rule change here.
                let mutable = resolved.is_mutable();
                self.load_resolved_content(name, path, resolved.uri(), mutable);
                Ok(())
            }
            // Any typed resolution failure (unnormalizable name, no resolver, no/
            // malformed/unsupported contenthash, an RPC/seam error, or a record
            // that did not fetch/decode/VERIFY) is fail-closed with its distinct,
            // legible reason — nothing unverified is rendered.
            Err(e) => {
                self.fail_ens_load(name, path, &e.to_string());
                Ok(())
            }
        }
    }

    /// Feed an already-resolved `ipfs://<cid>` `uri` (plus the typed sub-`path`)
    /// into the EXISTING verified `ipfs://` render path, keeping the front-door
    /// `name` (+ `path`) in the address bar, and flag the load's trust axes.
    ///
    /// The `path` (with its leading `/`, or `""`) is appended to BOTH the backend
    /// load target (`ipfs://<cid><path>`, resolved by the ipfs sub-path +
    /// directory-index path) and the displayed identity (`<name><path>`, e.g.
    /// `ronan.eth/blog/`), so a `.eth/<path>` entry loads its sub-path while the
    /// bar keeps what the user typed. The `ens_pages` / `pinned_root_key`
    /// association is keyed on the normalized CID+path form the resolved
    /// `ipfs://<cid><path>` produces, so reload / back / forward onto THIS entry
    /// re-derive the name+path (not the bare-CID root, nor the raw CID+path).
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
    fn load_resolved_content(&mut self, name: &str, path: &str, uri: &str, mutable: bool) {
        // The backend target is the resolved CID PLUS the typed sub-path
        // (`ipfs://<cid>/blog/`); the displayed identity is the name PLUS the path
        // (`ronan.eth/blog/`) — the identity the user typed, never the CID+path.
        let target = format!("{uri}{path}");
        let display = format!("{name}{path}");
        // Resolution is done; the backend now drives the CONTENT step. Hand the
        // step off to the backend's load state (via `refresh_chrome`).
        self.resolving_step = None;
        // The ENS front door is a USER-initiated navigation too (a typed `.eth`, or
        // a reload re-resolving one), so it starts a FRESH redirect chain and
        // reports the resolved `ipfs://<cid><path>` as the top-level document,
        // before the load starts — exactly as the plain branch of
        // [`navigate`](BrowserShell::navigate) does for its target.
        self.redirects.reset();
        self.redirects.note_navigation(&target);
        self.end_back_skip();
        if let Err(e) = self.renderer.navigate(&target) {
            // A backend that cannot even start the content load failed at the
            // content step, not resolution.
            self.resolving_step = None;
            self.fail_ens_load(name, path, &e.to_string());
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
        // The ROOT CID this site resolved to (the bare `<cid>` from `uri`, before
        // the sub-path append), so a later history/reload/SPA nav onto ANY
        // `<rootcid>/<path>` of this SAME site is recognised by a root-CID-PREFIX
        // match (`ens_identity_for_url`), not only the exact normalized entry key.
        // `name` is the bare `.eth` name (no path), the whole-site display root.
        let root_cid = crate::ipfs::ipfs_root_cid_and_path(uri)
            .map(|(cid, _)| cid)
            .unwrap_or_default();
        if let Some(current) = self.renderer.current_url() {
            self.ens_pages.insert(
                crate::ipfs::normalize_ens_page_key(&current),
                EnsIdentity {
                    name: display.clone(),
                    mutable,
                    root_cid,
                    root_name: name.to_string(),
                },
            );
        }
        // Keep the front-door NAME (+ path) the user typed in the bar (no
        // `https://` rewrite, no gateway redirect). The override PERSISTS across
        // pumps so the name stays put for the whole load — until the user navigates
        // OFF the resolved entry (an in-page link click), which `pump` detects by
        // the event URL's normalized key differing from `pinned_root_key`.
        self.url_override = Some(display);
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Fail an ENS front-door load closed: surface `reason` in the chrome and keep
    /// the `.eth` `name` (+ any `path`) in the bar, without navigating the backend
    /// to anything.
    ///
    /// This is the fail-closed path (spec story 3): a resolution failure or an
    /// unsupported/absent contenthash renders NOTHING — it only reports the
    /// legible reason the shell surfaces via [`ChromeState::last_error`], with the
    /// load state left settled so the chrome shows the failure rather than a
    /// spinner. The trust posture stays untrusted (no verified load happened). The
    /// typed `<name><path>` (e.g. `ronan.eth/blog/`) stays in the bar so a failed
    /// `.eth/<path>` shows what the user typed, never an https fallthrough.
    fn fail_ens_load(&mut self, name: &str, path: &str, reason: &str) {
        // Pin the `.eth` name (+ path) in the bar (the front door did not navigate
        // the backend anywhere, so there is no underlying URL to fall back to). The
        // load has SETTLED (failed), so no step is in flight: clear the pinned
        // resolution step BEFORE refreshing so the failed chrome shows the `Idle`
        // step, and so it never lingers onto the next load.
        self.resolving_step = None;
        self.url_override = Some(format!("{name}{path}"));
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

    /// BLESS the current page's mutable name at the CID it resolves to right now:
    /// the trust-on-first-use action the trust surface offers (task
    /// `ipns-tofu-pin-and-warn-on-change`).
    ///
    /// This is the ONLY way a pin is ever created: an EXPLICIT user action from
    /// the trust surface, never a first-visit prompt and never an automatic
    /// bless-what-you-loaded. Re-blessing a CHANGED name replaces its pin (the
    /// SSH-host-key model's "I looked at the change and I accept the new
    /// content"), so the next change is measured against what the user last
    /// accepted.
    ///
    /// The pin records the name, the CID, the moment, and the [`TrustPosture`]
    /// werust was showing, so a later change can say WHICH trust level was being
    /// blessed.
    ///
    /// Returns whether the pin was DURABLY recorded. It is `false` in three
    /// distinct-but-uninteresting-to-the-caller cases: there was nothing to bless
    /// (no mutable name, or the name is already blessed at exactly this CID, both
    /// of which the edge avoids by only offering the action when
    /// [`trust_pin_action_visible`] is true), or there is no settings directory /
    /// the write failed, in which case the bless still holds for THIS session
    /// (the chrome updates), it simply cannot survive a relaunch. It is never an
    /// error: a pin store that cannot be written must not break browsing.
    pub fn bless_current_name(&mut self) -> bool {
        // ONE gate, the very rule the edge's button visibility is painted from,
        // so "the button is shown" and "the action does something" cannot drift.
        if !trust_pin_action_visible(&self.chrome) {
            return false;
        }
        let Some(current) = self.chrome.mutable_name.clone() else {
            return false;
        };
        // READ -> MODIFY -> WRITE, per action, against the store on DISK: exactly
        // the shape the sibling settings store already uses
        // (`retrieval::apply_settings_request_in`). Saving `self.pins` instead
        // would rewrite the whole file from the snapshot THIS shell took at its
        // own launch, silently ERASING every pin another window blessed since --
        // and two windows is not exotic (a second `werust` launch opens a second
        // window in the same GTK application, and two VERSIONS are two processes).
        // That is the one direction of failure a TOFU store cannot have: the user
        // believes a name is blessed, the pin is gone, and the next resolution to
        // a different CID warns about nothing.
        //
        // With no durable store to re-read, the in-memory pins ARE the truth (no
        // file could have superseded them), so this session's earlier blesses are
        // carried rather than dropped.
        let mut pins = self.pin_store.load().unwrap_or_else(|| self.pins.clone());
        pins.bless(
            &current.name,
            &current.cid,
            self.chrome.trust_posture,
            crate::pins::now_unix_secs(),
        );
        let persisted = self.pin_store.save(&pins);
        // The re-read store (plus this bless) becomes the shell's cache, so a
        // concurrent writer's pins are visible here from now on too.
        self.pins = pins;
        // Re-derive the chrome's TOFU axis from the store, so the surface reflects
        // the bless immediately (the action label and the warning both change).
        self.refresh_chrome();
        persisted
    }

    /// Go one step back in session history, through the seam.
    ///
    /// A no-op when [`ChromeState::can_go_back`] is `false`. Delegates to the
    /// backend's session history (the shell keeps no URL stack of its own — see
    /// [`Renderer::go_back`]).
    ///
    /// # Back SKIPS a `_redirects` redirect source
    ///
    /// A real browser REPLACES the current history entry when it follows a 3xx,
    /// so Back from the target lands on whatever preceded the redirecting url. The
    /// seam has no replace-current-entry (and WebKitGTK exposes no public API to
    /// replace or remove a back-forward-list entry), so werust PUSHES instead and
    /// the redirected-FROM url stays in history. Landing on it would re-match its
    /// 3xx rule and bounce the user straight forward again, making Back unusable
    /// after any redirect. So a Back that lands on a url this chain redirected
    /// AWAY from ([`RedirectSink::redirect_sources`]) goes back ONCE MORE,
    /// transparently skipping it — the standard emulation of a replaced entry.
    ///
    /// Bounded by the same hop cap as the chain itself (a chain records at most
    /// [`MAX_REDIRECT_HOPS`](crate::ipfs::MAX_REDIRECT_HOPS) sources, and each
    /// skip consumes one), so a pathological history cannot spin here. If the
    /// redirect source is the FIRST history entry there is nothing further back:
    /// the user is left there and the redirect re-fires, rather than being trapped
    /// in a no-op Back
    /// (`docs/spikes/ipfs-redirects-3xx-navigation-support/DECISIONS.md`,
    /// Decision 8). The skip itself happens in [`pump`](BrowserShell::pump), which
    /// is where the landed url first becomes knowable — see
    /// [`back_skip`](BrowserShell::back_skip).
    pub fn go_back(&mut self) {
        // Snapshot the sources BEFORE the chain is reset below — they are what says
        // which entry this Back must skip over.
        self.back_skip = self.redirects.redirect_sources();
        self.back_skip_issued = None;
        self.renderer.go_back();
        // A user-initiated history move starts a FRESH redirect chain.
        self.redirects.reset();
        self.note_top_level_navigation();
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
        self.redirects.reset();
        // Forward is the user overruling the skip: whatever entry they are heading
        // to, they asked for it explicitly.
        self.end_back_skip();
        self.note_top_level_navigation();
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
    /// For a `.eth/<path>` page the SAME sub-path is re-loaded (the stored identity
    /// is re-split into name + path). Either way the `.eth` name (+ path) stays
    /// pinned in the bar and its ENS posture
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
        // current entry is recognised by its underlying CID via `ens_pages`. The
        // stored identity is the DISPLAY form (`ronan.eth/blog/`), so it is re-split
        // into name + sub-path via `eth_name_and_path_from_entry` to re-resolve the
        // name and re-load the SAME sub-path.
        let ens_identity = self
            .renderer
            .current_url()
            // Matched on the ROOT CID PREFIX so a reload from ANY sub-path of a
            // known ENS site re-resolves the site (never the raw CID).
            .and_then(|url| self.ens_identity_for_url(&url).map(|(name, _)| name))
            // A FAILED ENS load never navigated the backend (no `current_url`), but
            // still pinned the name (+ path) in the bar; reloading it re-runs the
            // resolution from that pinned identity, so a transient failure is
            // retryable.
            .or_else(|| self.url_override.clone());
        // Re-split the display identity into name + path; a non-`.eth` pinned
        // string (a plain page's URL) yields `None`, so it falls through to a plain
        // reload.
        if let Some((name, path)) = ens_identity
            .as_deref()
            .and_then(eth_name_and_path_from_entry)
        {
            let (name, path) = (name.to_string(), path.to_string());
            return self.navigate_ens_name(&name, &path);
        }
        self.renderer.reload()?;
        self.redirects.reset();
        self.end_back_skip();
        self.note_top_level_navigation();
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

    /// Recognise a backend `url` as belonging to a known ENS site, returning the
    /// `.eth` name to display (`ronan.eth` or `ronan.eth/<in-site-path>`) + the
    /// site's `mutable` axis, so reload / back / forward / SPA nav onto it
    /// re-derive the name + re-mark the posture instead of leaking the raw CID.
    ///
    /// The match is on the ROOT CID PREFIX of the entry, not only its exact
    /// normalized key — the fix for the v0.2.4 `ipfs://`-reappears leak
    /// (`ens_pages` was populated root-entry-only, so a history return onto a
    /// DIFFERENT sub-path of the same site missed the exact-key lookup and leaked
    /// `ipfs://<rootcid>/<path>`):
    ///
    /// 1. An EXACT normalized-key hit wins (unchanged behaviour): it keeps the
    ///    stored display (e.g. a `.eth/blog/` entry's own `ronan.eth/blog/`) and
    ///    its `mutable` flag.
    /// 2. Otherwise the entry's ROOT CID (the first `ipfs://` segment) is matched
    ///    against every known site's `root_cid`; on a hit the display is the
    ///    site's ROOT `.eth` name plus the entry's IN-SITE path
    ///    (`ronan.eth/<path>`, or bare `ronan.eth` at the root), and the site's
    ///    `mutable` axis is carried so the posture re-marks correctly.
    ///
    /// A non-`ipfs://` (plain served) URL has no root CID, so it never matches —
    /// plain pages are wholly unaffected.
    fn ens_identity_for_url(&self, url: &str) -> Option<(String, bool)> {
        self.ens_entry_for_url(url)
            .map(|(display, entry)| (display, entry.mutable))
    }

    /// [`ens_identity_for_url`](BrowserShell::ens_identity_for_url)'s WHOLE
    /// answer: the display identity PLUS the site entry behind it (its root name,
    /// root CID and mutability).
    ///
    /// Split out because the TOFU pin needs the two facts the display string
    /// deliberately hides: the ROOT `.eth` name (the identity a pin is keyed on,
    /// so `ronan.eth/blog/` and `ronan.eth` share ONE pin) and the ROOT CID (what
    /// the name resolves to right now). Returning them from the SAME lookup keeps
    /// one match rule: a pin can never be checked against a different entry than
    /// the one the URL bar is showing. The entry is CLONED rather than borrowed so
    /// the caller can go on to `&mut self` the renderer (re-marking the posture
    /// axes) in the same block.
    fn ens_entry_for_url(&self, url: &str) -> Option<(String, EnsIdentity)> {
        // 1. Exact normalized-key hit: the entry the user actually resolved.
        let key = crate::ipfs::normalize_ens_page_key(url);
        if let Some(entry) = self.ens_pages.get(&key) {
            return Some((entry.name.clone(), entry.clone()));
        }
        // 2. Root-CID-PREFIX match: ANY `<rootcid>/<path>` of a known ENS site.
        let (root_cid, in_site_path) = crate::ipfs::ipfs_root_cid_and_path(url)?;
        let entry = self
            .ens_pages
            .values()
            .find(|e| e.root_cid == root_cid && !e.root_cid.is_empty())?;
        // Display the whole-site ROOT name plus the in-site path
        // (`ronan.eth/<path>`), never the raw CID. `in_site_path` carries its
        // leading `/` (or is empty at the root).
        let display = format!("{}{in_site_path}", entry.root_name);
        Some((display, entry.clone()))
    }

    /// Step further BACK when an IN-FLIGHT Back landed on `url`, a url the
    /// redirect chain it is leaving redirected AWAY from: the user skips the
    /// redirecting entry instead of being bounced forward by its rule again (see
    /// [`go_back`](BrowserShell::go_back) for why that entry is in history at
    /// all). Returns whether it skipped, so the caller can drop the load event on
    /// the floor rather than paint an entry the user is not staying on.
    ///
    /// Bounded twice over: [`back_skip`](BrowserShell::back_skip) holds at most
    /// [`MAX_REDIRECT_HOPS`](crate::ipfs::MAX_REDIRECT_HOPS) sources and each skip
    /// consumes one, and a landing on anything else empties it — so it terminates
    /// even on a history of nothing but redirect sources. The skipped load is
    /// ABANDONED in the sink: the shell started (and is now leaving) it, and a
    /// scheme-handler request for it resolving LATE (it runs off the UI thread)
    /// must not still look like the main frame and re-queue the very redirect this
    /// skip avoids.
    fn skip_back_over_redirect_source(&mut self, url: &str) -> bool {
        if self.back_skip.is_empty() && self.back_skip_issued.is_none() {
            return false;
        }
        let landed = crate::ipfs::frame_key(url);
        if self.back_skip_issued.as_deref() == Some(landed.as_str()) {
            // A trailing event of the load this skip already stepped off (its
            // `Committed`/`Finished` were queued before the further Back was
            // issued). Dropped whole: see
            // [`back_skip_issued`](BrowserShell::back_skip_issued).
            return true;
        }
        let Some(at) = self.back_skip.iter().position(|source| *source == landed) else {
            // Landed somewhere the user means to stay: this Back is done.
            self.end_back_skip();
            return false;
        };
        // Spend this source whatever happens next, so a history of repeated
        // redirect sources cannot spin.
        self.back_skip.swap_remove(at);
        if !self.renderer.can_go_back() {
            // The redirect source is the FIRST entry: there is nothing further
            // back, so leave the user here (its rule re-fires) rather than trapping
            // Back in a no-op.
            self.end_back_skip();
            return false;
        }
        self.redirects.abandon_navigation();
        self.back_skip_issued = Some(landed);
        self.renderer.go_back();
        true
    }

    /// Forget the in-flight Back skip: no further entry is skipped, and no event
    /// is dropped. Called wherever the user takes over (any navigation they ask
    /// for) and as soon as a Back lands somewhere they mean to stay.
    fn end_back_skip(&mut self) {
        self.back_skip.clear();
        self.back_skip_issued = None;
    }

    /// Report the backend's CURRENT top-level URL to the redirect sink, for a
    /// navigation whose target the shell does not name itself (`go_back` /
    /// `go_forward` / `reload`): the backend knows which entry it moved to, so the
    /// sink learns which intercepted request is the MAIN FRAME from it.
    ///
    /// Best-effort: a backend whose history move settles ASYNCHRONOUSLY has no
    /// current URL yet, and the load event that follows reports the settled URL
    /// through [`pump`](BrowserShell::pump) anyway. A stale value cannot mislead
    /// either — the worst case is that a redirect on the FIRST intercepted request
    /// of the new page is treated as a sub-resource and fails closed, which is the
    /// pre-3xx behaviour, never a wrong navigation.
    fn note_top_level_navigation(&self) {
        if let Some(url) = self.renderer.current_url() {
            self.redirects.note_navigation(&url);
        }
    }

    /// Perform a `_redirects` 3xx NAVIGATION the `ipfs://` scheme handler queued,
    /// if any: the IPIP-0002 redirect, made real.
    ///
    /// Drained on the shell's existing pump cadence (no new loop, no busy poll).
    /// The target is an absolute `ipfs://<rootcid><to>` under the SAME root CID as
    /// the request that matched the rule (the sink's producer enforces that), so:
    ///
    /// * it is a REAL navigation through the seam — the URL bar and session
    ///   history move, exactly as a browser follows a 3xx;
    /// * it re-enters the `ipfs://` scheme handler, so the redirect target is
    ///   hash-verified by the SAME retrieval as any other page (werust never
    ///   vouches for a target it did not fetch), and a target that does not
    ///   resolve fails closed there;
    /// * the site IDENTITY survives: the root CID is unchanged, so the existing
    ///   root-CID-prefix `ens_pages` association re-derives `name/<new-path>` in
    ///   the bar for a redirect inside an ENS site, rather than leaking a raw CID.
    ///
    /// The chain is NOT reset here (only a navigation that is NOT this chain's own
    /// target does that), which is what bounds a chain of redirects: the sink
    /// refuses a cycle or an over-long chain and queues nothing, leaving the
    /// legible fail-closed error the handler already surfaced. Returns `true` when
    /// a navigation was performed, so the caller repaints.
    fn follow_pending_redirect(&mut self) -> bool {
        let Some(target) = self.redirects.take_pending() else {
            return false;
        };
        // The redirect target is the new TOP-LEVEL document, so an intercepted
        // request for it is the MAIN FRAME (and may itself redirect again, up to
        // the chain bound). Draining it above already marked it as the target this
        // chain is FOLLOWING, so this reports the top level WITHOUT ending the
        // chain — which is exactly the one navigation that must not reset the
        // budget.
        self.redirects.note_navigation(&target);
        if self.renderer.navigate(&target).is_err() {
            // A backend that cannot even start the redirected load leaves the
            // handler's fail-closed error standing: nothing is rendered, and no
            // further hop is attempted.
            return false;
        }
        // The redirect proceeded, so the "navigating" failure the intercepted (old)
        // request answered with is spent; the redirected load's own outcome is what
        // the chrome should show from here.
        self.chrome.last_error = None;
        // The bar FOLLOWS the redirect target (it is a different in-site path than
        // the pinned root), so drop any pin; `refresh_chrome` then re-derives the
        // ENS identity for the new address off the site's root CID.
        self.url_override = None;
        self.pinned_root_key = None;
        self.resolving_step = None;
        true
    }

    /// Drain every pending [`LoadEvent`] off the seam and fold it into the chrome.
    ///
    /// The window calls this on its main loop (a periodic pump). Each event moves
    /// the URL bar / load indicator: a `Started` clears any error and shows the
    /// target, `Committed`/`Finished` settle the URL bar on the effective URL,
    /// and a `Failed` surfaces the reason. It also performs any `_redirects` 3xx
    /// NAVIGATION the `ipfs://` scheme handler queued
    /// ([`follow_pending_redirect`](BrowserShell::follow_pending_redirect)).
    /// Returns `true` if any event was processed (or a redirect followed), so a
    /// caller can repaint only on change.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.renderer.poll_event() {
            changed = true;
            // A Back that landed on a url THIS page was redirected away from must
            // skip over it (werust pushes rather than replaces the redirecting
            // entry, so it is still in history and its rule would bounce the user
            // straight forward again). Checked FIRST, on the load event, because a
            // history move settles asynchronously: this is the earliest the landed
            // url is knowable. The skipped-over entry is not painted — the user is
            // not staying on it — and the further Back it just issued reports its
            // own events on a later turn of this same loop.
            if self.skip_back_over_redirect_source(event.url()) {
                continue;
            }
            // An IN-PAGE navigation off the pinned ENS root (a link click) is a
            // FRESH backend load whose event URL normalizes to a DIFFERENT key than
            // the resolved root the name was pinned FOR. When that happens, the
            // user has navigated WITHIN/away, so drop the pin here and let the bar
            // FOLLOW the backend URL (the pin-vs-follow decision). The ROOT entry
            // stays recoverable via `ens_pages` on a history return. A load whose
            // URL is the pinned root (the front-door root still loading) keeps the
            // pin, so the name holds for the whole root load.
            self.drop_pin_on_in_page_nav(event.url());
            // Tell the redirect sink which TOP-LEVEL document the backend is on.
            // This is the ONLY signal an IN-PAGE navigation (a link click, an SPA
            // push) gives the core — it never passes through `navigate` /
            // `go_back` / `reload` — so it is what makes the redirect chain
            // PER-CHAIN rather than per-session: any load that is NOT this chain's
            // own target ENDS the chain and restores the full hop budget. It also
            // re-establishes which intercepted request is the MAIN FRAME, so a 3xx
            // matched on a sub-resource of the new page cannot navigate it away.
            self.redirects.note_navigation(event.url());
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
                // A SAME-DOCUMENT URL change (an SPA `pushState`/`replaceState`):
                // FOLLOW the new URL exactly as an in-page load event does
                // (`drop_pin_on_in_page_nav` above already dropped a pin off the
                // resolved root), but do NOT fake a load lifecycle — it does not
                // touch the load state or the error. `refresh_chrome` below then
                // re-derives the ENS identity for the new address (a nav back onto
                // a known ENS root/sub-path re-shows `ronan.eth[/path]`).
                LoadEvent::UrlChanged { url } => {
                    if !pinned {
                        self.chrome.url_text = url;
                    }
                }
                LoadEvent::Failed { url, reason } => {
                    if !pinned {
                        self.chrome.url_text = url;
                    }
                    // A `_redirects` 3xx answers the intercepted request
                    // fail-closed (nothing may render under the redirected-FROM
                    // url), which the backend reports as a failed load. That is
                    // BOOKKEEPING for a navigation about to happen, not something
                    // to show the user, so its banner is suppressed by the marker
                    // the reason carries. A REFUSED redirect (off-root, or a chain
                    // the sink bounded) carries no marker and surfaces normally —
                    // it is a real failure and the chain stops there.
                    if reason.contains(crate::ipfs::REDIRECT_NAVIGATING_MARKER) {
                        self.chrome.last_error = None;
                    } else {
                        self.chrome.last_error = Some(reason);
                    }
                }
            }
        }
        // A site's `_redirects` 3xx: the scheme handler queued a navigation target
        // (it cannot navigate itself). Perform it here, on the SAME cadence, so the
        // redirect is a real bar+history move whose target re-enters the verified
        // `ipfs://` path. Done AFTER the events are folded so the intercepted
        // request's own fail-closed outcome is recorded first and then superseded.
        changed |= self.follow_pending_redirect();
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
        // Matched on the ROOT CID PREFIX (`ens_identity_for_url`), so ANY
        // `<rootcid>/<path>` of a known ENS site re-derives the name + re-marks the
        // posture — not only the exact resolved entry (the v0.2.4 leak fix).
        let ens_entry = self
            .renderer
            .current_url()
            .and_then(|url| self.ens_entry_for_url(&url));
        if let Some((_, entry)) = &ens_entry {
            self.renderer.mark_ens_origin();
            if entry.mutable {
                self.renderer.mark_mutable_name();
            }
        }
        // The TOFU axis: pair the current entry's MUTABLE NAME with whatever the
        // user has blessed for it (`docs/adr/0006`'s mutability axis, task
        // `ipns-tofu-pin-and-warn-on-change`). Derived here, beside the other
        // per-entry re-derivations, so back / forward / reload onto a known ENS
        // site re-checks the pin exactly as it re-derives the name: a warning the
        // user only saw on the first load would be no warning at all.
        //
        // EVERY name-resolved entry is checked, `ipfs-ns` as well as `ipns-ns`:
        // both are controller-repointable per `docs/adr/0006`, so both are
        // blessable (see the `pins` module's MUTABILITY-AXIS note for why this is
        // deliberately wider than the `MutableName` POSTURE, which loses to the
        // louder `NameViaTrustedRpc` on every ENS load today). A page that is not
        // name-resolved at all has no axis value, so nothing changes for it.
        let mutable_name = ens_entry.as_ref().and_then(|(_, entry)| {
            (!entry.root_cid.is_empty() && !entry.root_name.is_empty())
                .then(|| self.pins.check(&entry.root_name, &entry.root_cid))
        });
        self.chrome.mutable_name = mutable_name;
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
        } else if let Some((display, _)) = &ens_entry {
            self.chrome.url_text = display.clone();
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

        /// Simulate a SAME-DOCUMENT URL change the way a real webview delivers a
        /// client-side history navigation (an SPA `pushState`/`replaceState`): the
        /// address the webview reports changes to `url` WITHOUT a fresh page load,
        /// so NO `load-changed`/lifecycle signal fires. It emits ONLY a
        /// [`LoadEvent::UrlChanged`] and leaves the load state, posture, and
        /// per-load flags UNTOUCHED (the document, and its already-established
        /// trust, are unchanged) — exactly what the real backends do on
        /// `notify::uri` / KVO / `doUpdateVisitedHistory`. This is the SPA path
        /// `navigate_in_page` (a real fresh in-page load) does NOT model: a SPA nav
        /// fires no load event, which is precisely why the bar used to freeze.
        ///
        /// It updates the reported URL and the session-history entry (a
        /// same-document nav pushes a history entry the back button returns to),
        /// but does not reset the lifecycle: `state`/`posture`/`ens_origin`/
        /// `mutable_name` keep the current document's values.
        fn change_url_in_page(&self, url: &str) {
            let mut b = self.inner.borrow_mut();
            // A same-document history push adds a forward entry from mid-history,
            // dropping any forward entries, just like a real navigation — but with
            // NO load lifecycle reset.
            let next = b.cursor.map_or(0, |c| c + 1);
            b.history.truncate(next);
            b.history.push(webkit_normalize(url));
            b.cursor = Some(b.history.len() - 1);
            // The webview now reports the new same-document URL. Crucially the load
            // state and trust posture are NOT touched: this is not a fresh load.
            b.reported_url = Some(url.to_string());
            b.pending_history = None;
            b.events.push_back(LoadEvent::UrlChanged {
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
    fn eth_name_and_path_splits_a_dot_eth_entry_into_name_and_optional_path() {
        // Acceptance: a `.eth` entry WITH a path (`ronan.eth/blog/`) is recognised
        // as the ENS front door for `ronan.eth` + the sub-path `/blog/`, so the
        // path can be threaded into the resolved `ipfs://<cid>/<path>` load — while
        // `eth_name_from_entry` (the bare-name recogniser) STILL rejects a `/`.
        //
        // A bare `.eth` (no path) is unchanged: no path component.
        assert_eq!(
            eth_name_and_path_from_entry("ronan.eth"),
            Some(("ronan.eth", ""))
        );
        // A bare `.eth` with a lone trailing `/` is still the bare name (the path
        // is empty), so it stays identical to the no-slash entry.
        assert_eq!(
            eth_name_and_path_from_entry("ronan.eth/"),
            Some(("ronan.eth", ""))
        );
        // A `.eth` WITH a real path: split into the name and the `/<path>` (with
        // its leading `/`, and the trailing `/` preserved verbatim).
        assert_eq!(
            eth_name_and_path_from_entry("ronan.eth/blog/"),
            Some(("ronan.eth", "/blog/"))
        );
        assert_eq!(
            eth_name_and_path_from_entry("ronan.eth/blog"),
            Some(("ronan.eth", "/blog"))
        );
        assert_eq!(
            eth_name_and_path_from_entry("a.b.eth/x/y/z.html"),
            Some(("a.b.eth", "/x/y/z.html"))
        );
        // Case-insensitive on the `.eth` label, same as the bare recogniser.
        assert_eq!(
            eth_name_and_path_from_entry("Ronan.ETH/Blog/"),
            Some(("Ronan.ETH", "/Blog/"))
        );
        // An explicit scheme is STILL literal, never hijacked into an ENS name.
        assert_eq!(
            eth_name_and_path_from_entry("https://ronan.eth/blog/"),
            None
        );
        assert_eq!(eth_name_and_path_from_entry("ipfs://bafycid/blog/"), None);
        assert_eq!(eth_name_and_path_from_entry("ens://ronan.eth/blog"), None);
        // A non-`.eth` host with a path is NOT an ENS name (it stays an https
        // candidate): only a `.eth` TLD label routes to ENS.
        assert_eq!(eth_name_and_path_from_entry("github.com/foo"), None);
        assert_eq!(eth_name_and_path_from_entry("example.com"), None);
        // A bare `.eth` label (empty name before the TLD) is not a name, path or
        // no path.
        assert_eq!(eth_name_and_path_from_entry(".eth"), None);
        assert_eq!(eth_name_and_path_from_entry(".eth/blog"), None);
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

    // ---- Trust-on-first-use for MUTABLE names (task -------------------------
    //      `ipns-tofu-pin-and-warn-on-change`, `docs/adr/0006`'s mutability axis).

    /// A unique scratch directory for the TOFU pin store, removed on drop, so the
    /// whole bless -> persist -> re-resolve -> warn loop runs against ITS OWN
    /// directory and NEVER the real `pins.json` (the shared-write rule), driven
    /// through the shell's directory-taking `with_pins_dir` seam, so no test
    /// mutates the process-global `WERUST_SETTINGS_DIR`.
    struct PinScratchDir {
        path: std::path::PathBuf,
    }

    impl PinScratchDir {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "werust-shell-pins-test-{tag}-{pid}-{n}",
                pid = std::process::id(),
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for PinScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// A shell whose ENS front door resolves through a scripted RPC fixture AND
    /// whose TOFU pin store lives in `pins_dir`: a "launch" of werust with a
    /// known blessed set, off the live network and off the real store.
    fn shell_with_provider_and_pins(
        answers: Vec<Result<Vec<u8>, ProviderError>>,
        pins_dir: &std::path::Path,
    ) -> (BrowserShell, BackendHandle) {
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let provider = ScriptedProvider::new(answers);
        (
            BrowserShell::with_provider(Box::new(backend), Box::new(provider))
                .with_pins_dir(pins_dir),
            handle,
        )
    }

    /// Drive a whole `.eth` load to a settled, hash-verified page.
    fn load_eth_name(shell: &mut BrowserShell, handle: &BackendHandle, name: &str) {
        shell.navigate(name).expect("the front door handles .eth");
        handle.serve_via_verified_content_path();
        settle(shell, handle);
    }

    #[test]
    fn an_unblessed_mutable_name_behaves_exactly_as_before_but_offers_the_bless() {
        // Acceptance (FAIL-SAFE): a name nobody has blessed is UNCHANGED by this
        // feature: same posture, no banner, no louder badge. The only thing that
        // is new is that the trust surface OFFERS the bless, which is an
        // affordance, not a prompt.
        let scratch = PinScratchDir::new("unblessed");
        let (contenthash, _uri) = ipfs_contenthash_fixture(b"the site as it is today");
        let (mut shell, handle) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&contenthash)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut shell, &handle, "ronan.eth");

        let chrome = shell.chrome();
        assert_eq!(chrome.trust_posture, TrustPosture::NameViaTrustedRpc);
        assert_eq!(trust_indicator(chrome), "◈ name via trusted RPC");
        assert_eq!(trust_indicator_css_class(chrome), "trust-name-trusted-rpc");
        assert!(!chrome.mutable_name_changed());
        assert!(
            !error_banner_visible(chrome),
            "an unblessed name raises no banner"
        );
        // But the name IS recognised as blessable, and the surface says what the
        // user would be blessing.
        assert!(chrome.can_bless_name());
        assert!(trust_pin_action_visible(chrome));
        assert_eq!(trust_pin_action_label(chrome), "🔒 Trust this content");
        let detail = trust_pin_detail(chrome);
        assert!(detail.contains("ronan.eth"), "names the name: {detail}");
        assert!(
            detail.contains("not trusted a version"),
            "says nothing is blessed yet: {detail}"
        );
        // Nothing has been written: an unblessed browse never touches the store.
        assert!(!scratch.path.join(crate::pins::PINS_FILE).exists());
    }

    #[test]
    fn blessing_a_name_persists_across_launches_and_a_later_change_warns_loudly() {
        // THE acceptance property, end to end and offline: the user blesses
        // `ronan.eth` at the CID it resolves to today; a LATER launch resolves the
        // same name to a DIFFERENT CID and is warned legibly, not silently
        // accepted, and not hard-blocked.
        let scratch = PinScratchDir::new("bless-then-change");
        let (ch_old, _) = ipfs_contenthash_fixture(b"the version the user trusts");
        let (ch_new, _) = ipfs_contenthash_fixture(b"a DIFFERENT version, published later");

        // --- Launch 1: load the name and BLESS what it points at. ---
        let (mut first, handle) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_old)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut first, &handle, "ronan.eth");
        let blessed_cid = first
            .chrome()
            .mutable_name
            .as_ref()
            .expect("a name-resolved page carries the TOFU axis")
            .cid
            .clone();
        assert!(first.bless_current_name(), "the bless is recorded durably");

        // It is on disk, in the scratch dir ONLY, and the surface now says so.
        assert!(scratch.path.join(crate::pins::PINS_FILE).is_file());
        let chrome = first.chrome();
        assert!(chrome.mutable_name.as_ref().unwrap().is_unchanged());
        assert!(
            !trust_pin_action_visible(chrome),
            "a name already blessed at this very CID has nothing left to record"
        );
        assert!(trust_pin_detail(chrome).contains("You trusted exactly this content on"));
        // Blessing changed NOTHING else: same posture, same badge, no banner.
        assert_eq!(chrome.trust_posture, TrustPosture::NameViaTrustedRpc);
        assert!(!error_banner_visible(chrome));

        // --- Launch 2: a fresh shell, same store, the name now resolves elsewhere. ---
        let (mut second, handle2) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_new)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut second, &handle2, "ronan.eth");
        let chrome = second.chrome();

        // The pin SURVIVED the relaunch, and the change is detected.
        assert!(chrome.mutable_name_changed());
        let axis = chrome.mutable_name.as_ref().unwrap();
        assert_ne!(axis.cid, blessed_cid);
        assert_eq!(axis.blessed.as_ref().unwrap().cid, blessed_cid);

        // 1. A DISTINCT, LOUDER badge that is neither `MutableName` nor
        //    `NameViaTrustedRpc`: the change is never flattened into either.
        let badge = trust_indicator(chrome);
        let class = trust_indicator_css_class(chrome);
        assert_eq!(class, "trust-name-changed");
        assert_ne!(class, "trust-mutable-name");
        assert_ne!(class, "trust-name-trusted-rpc");
        assert!(badge.contains("NEW content"), "a legible badge: {badge}");
        assert!(TRUST_INDICATOR_CSS_CLASSES.contains(&class));
        // The underlying POSTURE is untouched: the bytes really did verify, and
        // the pin is a separate axis, not a re-meaning of the seam's truth.
        assert_eq!(chrome.trust_posture, TrustPosture::NameViaTrustedRpc);

        // 2. The failure-class BANNER, carrying the settled sentence: what
        //    changed, when it was trusted, and both CIDs to compare.
        assert!(error_banner_visible(chrome));
        let banner = error_banner_text(chrome);
        assert!(banner.contains("ronan.eth"), "{banner}");
        assert!(
            banner.contains("points to different content than the version you trusted on"),
            "the settled wording: {banner}"
        );
        assert!(banner.contains(&blessed_cid), "the blessed CID: {banner}");
        assert!(banner.contains(&axis.cid), "the current CID: {banner}");
        assert_eq!(
            error_banner_css_class(chrome),
            "error-banner",
            "failure-class prominence, not the softer retryable treatment"
        );

        // 3. NOT hard-blocked: the page loaded and rendered exactly as it would
        //    have, and the user is offered the re-trust decision.
        assert_eq!(chrome.load_state, LoadState::Finished);
        assert_eq!(chrome.last_error, None);
        assert!(trust_pin_action_visible(chrome));
        assert_eq!(
            trust_pin_action_label(chrome),
            "🔒 Trust the NEW content of this name",
            "accepting a CHANGE must not read like a first-use bless"
        );

        // 4. Re-blessing clears the warning (the SSH-host-key "I looked, and I
        //    accept"), and the NEW CID is what a later change is measured against.
        let current_cid = axis.cid.clone();
        assert!(second.bless_current_name());
        assert!(!second.chrome().mutable_name_changed());
        assert!(!error_banner_visible(second.chrome()));
        assert_eq!(
            crate::pins::TrustedNamePins::load_from(&scratch.path)
                .get("ronan.eth")
                .map(|p| p.cid.clone()),
            Some(current_cid)
        );
    }

    #[test]
    fn a_mutable_ipns_ns_name_is_blessed_and_warned_exactly_like_an_ens_ipfs_ns_one() {
        // Acceptance (settled decision 3): BOTH axes' names are
        // controller-repointable, so both are blessable and both warn identically.
        // Here the ENS contenthash is an `ipns-ns` POINTER followed through a
        // client-verified record: the CID can change because the ENS owner
        // repoints OR because the key holder publishes, and the pin (keyed on the
        // name the user actually sees) catches both.
        let scratch = PinScratchDir::new("ipns-ns");
        let key = IpnsKeyFixture::new();
        let old_cid = cid_v1_raw_sha256(b"the ipns site the user trusts").expect("cid");
        let new_cid = cid_v1_raw_sha256(b"what the key holder published later").expect("cid");

        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::with_provider_and_ipns_source(
            Box::new(backend),
            Box::new(ScriptedProvider::new(vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ])),
            Box::new(PinnedIpnsSource::with_record(
                &key.name,
                key.signed_record_for(&old_cid),
            )),
        )
        .with_pins_dir(&scratch.path);
        load_eth_name(&mut shell, &handle, "mutable.eth");
        assert_eq!(
            shell.chrome().mutable_name.as_ref().map(|n| n.cid.clone()),
            Some(old_cid.clone()),
            "the axis carries the CID the RECORD currently points at"
        );
        assert!(shell.bless_current_name());

        // A later launch: the key holder has published a different target.
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut later = BrowserShell::with_provider_and_ipns_source(
            Box::new(backend),
            Box::new(ScriptedProvider::new(vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&key.contenthash)),
            ])),
            Box::new(PinnedIpnsSource::with_record(
                &key.name,
                key.signed_record_for(&new_cid),
            )),
        )
        .with_pins_dir(&scratch.path);
        load_eth_name(&mut later, &handle, "mutable.eth");

        assert!(later.chrome().mutable_name_changed());
        assert_eq!(
            trust_indicator_css_class(later.chrome()),
            "trust-name-changed"
        );
        assert!(error_banner_text(later.chrome()).contains(&new_cid));
    }

    #[test]
    fn the_pin_never_chooses_what_loads_and_never_makes_unverified_bytes_look_verified() {
        // FAIL-SAFE, the security-relevant half: a pin is ADVISORY. It must not
        // steer the load back to the blessed CID (that would be a browser showing
        // stale content it was not asked for), and it must not upgrade a load whose
        // bytes never hash-verified.
        let scratch = PinScratchDir::new("advisory");
        let (ch_old, _) = ipfs_contenthash_fixture(b"blessed version");
        let (ch_new, new_uri) = ipfs_contenthash_fixture(b"the version served now");

        let (mut first, handle) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_old)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut first, &handle, "ronan.eth");
        assert!(first.bless_current_name());

        // The name now resolves elsewhere, and the bytes are NOT served through the
        // verified content path this time.
        let (mut second, handle2) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_new)),
            ],
            &scratch.path,
        );
        second.navigate("ronan.eth").expect("the front door runs");
        // The backend was pointed at the FRESHLY RESOLVED CID, never the blessed one.
        assert_eq!(
            second.current_url_for_test().as_deref(),
            Some(new_uri.as_str()),
            "the pin must not steer the load back to the CID the user blessed"
        );
        settle(&mut second, &handle2);
        // Unverified bytes stay unverified: the pin adds a warning, never trust.
        assert!(!second.chrome().is_content_verified());
        assert_ne!(second.chrome().trust_posture, TrustPosture::ContentVerified);
    }

    #[test]
    fn a_page_that_is_not_a_name_resolved_load_carries_no_tofu_axis_at_all() {
        // The axis is `None` for a direct `ipfs://<cid>` and for an ordinary
        // served page: there is no name to bless and nothing that could change.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ipfs://bafyplaincid/index.html").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().mutable_name, None);
        assert!(!shell.chrome().can_bless_name());
        assert!(!trust_pin_action_visible(shell.chrome()));
        assert_eq!(trust_pin_detail(shell.chrome()), "");
        assert!(!shell.bless_current_name(), "nothing to bless");

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().mutable_name, None);
    }

    #[test]
    fn the_change_warning_is_re_derived_on_a_history_move_not_only_the_first_load() {
        // A warning the user sees once and loses on Back would be no warning at
        // all: the TOFU axis is re-derived beside the `.eth` name itself, off the
        // SAME entry lookup, so returning to a changed site warns again.
        let scratch = PinScratchDir::new("history");
        let (ch_old, _) = ipfs_contenthash_fixture(b"blessed");
        let (ch_new, _) = ipfs_contenthash_fixture(b"changed");

        let (mut first, handle) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_old)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut first, &handle, "ronan.eth");
        assert!(first.bless_current_name());

        let (mut second, handle2) = shell_with_provider_and_pins(
            vec![
                Ok(address_word(&[0x11u8; 20])),
                Ok(abi_bytes_return(&ch_new)),
            ],
            &scratch.path,
        );
        load_eth_name(&mut second, &handle2, "ronan.eth");
        assert!(second.chrome().mutable_name_changed());

        // Navigate away, then Back onto the changed site: the bar re-derives the
        // `.eth` name from the entry's CID, and the warning comes back with it.
        second.navigate("https://example.com/").unwrap();
        settle(&mut second, &handle2);
        assert!(!second.chrome().mutable_name_changed());
        second.go_back();
        settle(&mut second, &handle2);
        assert_eq!(second.chrome().url_text, "ronan.eth");
        assert!(
            second.chrome().mutable_name_changed(),
            "the warning is re-derived on the history move, like the name is"
        );
    }

    /// The REAL `pins.json`'s bytes, or `None` when the developer has none (or
    /// there is no settings directory at all): the before/after snapshot a test
    /// asserts the suite NEVER writes the developer's own pin store with.
    fn real_pin_store_snapshot() -> Option<Vec<u8>> {
        crate::pins::pins_file_path().and_then(|path| std::fs::read(path).ok())
    }

    #[test]
    fn a_bless_in_one_window_never_erases_a_pin_blessed_in_another() {
        // Acceptance: a bless is READ-MODIFY-WRITE against the store on disk, not
        // a wholesale rewrite of the snapshot this shell took at its own launch.
        // Two windows sharing one directory is not exotic — a second `werust`
        // launch activates the same GTK application and opens a second window
        // in-process, and two VERSIONS are simply two processes — so a shell that
        // saves its own stale snapshot silently DROPS whatever the other one
        // blessed meanwhile. That is the one direction of failure a TOFU store
        // cannot have: the user believes the name is blessed, the pin is gone, and
        // the next resolution to a different CID warns about nothing. The sibling
        // settings store already does it right (`retrieval`'s
        // `apply_settings_request_in` is load -> mutate -> save per action).
        let scratch = PinScratchDir::new("two-windows");
        let (ch_a, _) = ipfs_contenthash_fixture(b"the site behind a.eth");
        let (ch_b, _) = ipfs_contenthash_fixture(b"the site behind b.eth");

        // BOTH windows are constructed BEFORE either blesses, so each holds the
        // same (empty) snapshot: exactly the two-window situation.
        let (mut window_a, handle_a) = shell_with_provider_and_pins(
            vec![Ok(address_word(&[0x11u8; 20])), Ok(abi_bytes_return(&ch_a))],
            &scratch.path,
        );
        let (mut window_b, handle_b) = shell_with_provider_and_pins(
            vec![Ok(address_word(&[0x22u8; 20])), Ok(abi_bytes_return(&ch_b))],
            &scratch.path,
        );

        load_eth_name(&mut window_a, &handle_a, "a.eth");
        assert!(window_a.bless_current_name());
        let cid_a = window_a
            .chrome()
            .mutable_name
            .as_ref()
            .expect("a name-resolved page carries the TOFU axis")
            .cid
            .clone();

        load_eth_name(&mut window_b, &handle_b, "b.eth");
        assert!(window_b.bless_current_name());
        let cid_b = window_b
            .chrome()
            .mutable_name
            .as_ref()
            .expect("a name-resolved page carries the TOFU axis")
            .cid
            .clone();

        // BOTH pins survive: the second writer merged into what it found on disk.
        let on_disk = crate::pins::TrustedNamePins::load_from(&scratch.path);
        assert_eq!(
            on_disk.len(),
            2,
            "the second window's bless must not erase the first window's pin"
        );
        assert_eq!(on_disk.get("a.eth").map(|p| p.cid.clone()), Some(cid_a));
        assert_eq!(on_disk.get("b.eth").map(|p| p.cid.clone()), Some(cid_b));

        // The merge is visible to the writer itself, so its NEXT bless cannot
        // re-drop what it just merged in.
        assert_eq!(window_b.pins_for_test().len(), 2);
    }

    #[test]
    fn a_test_shell_starts_from_an_empty_store_and_never_touches_the_real_pins_json() {
        // Acceptance (hermeticity — the READ-side twin of the work contract's
        // shared-write rule): a shell built WITHOUT `with_pins_dir` inside this
        // crate's test binary reads NO store at all. Reading the real one would
        // mean every core test sees whatever the DEVELOPER has blessed in their
        // own build, so a machine where `ronan.eth` is blessed could flip a TOFU
        // axis inside a fixture and red an unrelated chrome assertion — a failure
        // that reproduces on one machine and nowhere else.
        let real_before = real_pin_store_snapshot();

        let (contenthash, _) = ipfs_contenthash_fixture(b"a fixture site");
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);
        load_eth_name(&mut shell, &handle, "ronan.eth");

        assert!(
            shell.pins_for_test().is_empty(),
            "a default test shell reads no pin store"
        );
        let axis = shell
            .chrome()
            .mutable_name
            .as_ref()
            .expect("a name-resolved page carries the TOFU axis");
        assert!(
            !axis.is_blessed(),
            "whatever the developer blessed locally must not reach a fixture"
        );
        assert!(!shell.chrome().mutable_name_changed());

        // And the bless has nowhere DURABLE to go: it holds for this session and
        // is reported unpersisted, rather than being written into the developer's
        // real store.
        assert!(trust_pin_action_visible(shell.chrome()));
        assert!(
            !shell.bless_current_name(),
            "a test shell has no durable store to record into"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the REAL pin store is untouched by this suite"
        );
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

    // ---- SPA client-side (same-document) URL tracking + root-CID-prefix ENS ----
    // (task `track-webview-url-on-spa-clientside-navigation`)

    #[test]
    fn a_spa_same_document_url_change_updates_the_bar_and_drops_the_pin() {
        // Acceptance (Part 1, the frozen-bar-on-internal-nav): a SvelteKit SPA link
        // click is a CLIENT-SIDE `pushState` — the webview's reported URL changes
        // but NO load-changed / LoadEvent lifecycle fires. The seam surfaces this
        // as a distinct `LoadEvent::UrlChanged`; the shell must FOLLOW it (drop the
        // pinned `.eth` name and show the new location) exactly as it does for an
        // in-page load event — instead of freezing on `ronan.eth`.
        let page = b"<!doctype html><title>ronan</title><h1>ronan.eth root</h1>";
        let (contenthash, _ipfs_uri) = ipfs_contenthash_fixture(page);
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

        // The user clicks an INTERNAL SvelteKit link: a client-side `pushState` to
        // a sub-path of the SAME ipfs document, with NO load lifecycle event — only
        // a same-document URL change. This is the exact path that used to freeze
        // the bar (the desktop pump had no events to drain).
        let sub_path = format!("{}/portfolio", ipfs_root_of(&handle));
        handle.change_url_in_page(&sub_path);
        shell.pump();
        // The bar no longer freezes on `ronan.eth`; it FOLLOWS the new location.
        // Because the new URL is UNDER the known ENS site's root CID (Part 2), it
        // re-derives `ronan.eth/portfolio`, never the raw `ipfs://<cid>/portfolio`.
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/portfolio",
            "a SPA same-document nav updates the bar to the in-site path"
        );
        assert!(!shell.chrome().url_text.starts_with("ipfs://"));
        // A same-document nav within a verified ipfs site keeps the document's
        // established trust: the SPA nav must NOT reset or fake the load posture.
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "a same-document nav within the verified ENS site stays verified"
        );
        // The load state is unchanged: a same-document URL change is NOT a load.
        assert_eq!(
            shell.chrome().load_state,
            LoadState::Finished,
            "a same-document URL change does not restart the load lifecycle"
        );
    }

    #[test]
    fn history_return_onto_any_subpath_of_a_known_ens_site_re_derives_the_name_never_the_cid() {
        // Acceptance (Part 2, the `ipfs://`-reappears leak): after loading an ENS
        // site and navigating to a SUB-PATH within it, a back/forward/reload that
        // lands on ANY `<rootcid>/<path>` of that site shows the `.eth` name
        // (`ronan.eth/<path>`) + its ENS posture — NEVER the raw
        // `ipfs://<rootcid>/<path>`. The association is matched on the ROOT CID
        // PREFIX of the current entry, not only its exact normalized key (which is
        // why the v0.2.4 root-only `ens_pages` leaked on a sub-path return).
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, _ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // Load the ENS root, then SPA-navigate deep into the site (a sub-path whose
        // normalized key DIFFERS from the stored root `<cid>` key).
        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        let deep = format!("{}/blog/post-1", ipfs_root_of(&handle));
        handle.change_url_in_page(&deep);
        shell.pump();
        assert_eq!(shell.chrome().url_text, "ronan.eth/blog/post-1");

        // Navigate AWAY to a plain page, then BACK: the back lands on the deep
        // sub-path entry, whose exact normalized key is NOT in `ens_pages` (only
        // the root `<cid>` is). The root-CID-prefix lookup must still recognise it
        // as under `ronan.eth` and show `ronan.eth/blog/post-1`, never the CID.
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/blog/post-1",
            "a history return onto a sub-path of a known ENS site re-derives the name+path"
        );
        assert!(
            !shell.chrome().url_text.starts_with("ipfs://"),
            "the raw ipfs://<rootcid>/<path> must never leak into the bar"
        );
        // The ENS posture is re-marked for the sub-path entry too (the verified
        // content path surfaces NameViaTrustedRpc, not a bare-CID ContentVerified).
        handle.serve_via_verified_content_path();
        shell.pump();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "a sub-path of a known ENS site keeps its ENS posture"
        );
    }

    #[test]
    fn a_spa_url_change_on_a_plain_page_follows_the_url_unregressed() {
        // Acceptance: a plain (non-ENS) page tracks a same-document URL change too,
        // exactly as a browser does, and never re-derives an ENS name (a plain URL
        // has no root CID to match). This guards the full-page-load / plain path
        // against regression by the new UrlChanged handling.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://example.com/");

        // A client-side history push within the plain page (no load event).
        handle.change_url_in_page("https://example.com/spa/route");
        shell.pump();
        assert_eq!(
            shell.chrome().url_text,
            "https://example.com/spa/route",
            "a same-document URL change on a plain page follows the URL"
        );
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
        assert_eq!(
            shell.chrome().load_state,
            LoadState::Finished,
            "a same-document URL change is not a load"
        );
    }

    // ---- A site's `_redirects` 3xx: a REAL navigation, chain-bounded ---------
    // (task `ipfs-redirects-3xx-navigation-support`)

    #[test]
    fn a_queued_redirect_navigates_the_shell_on_the_pump_and_moves_the_bar_and_history() {
        // Acceptance: a matching 3xx rule NAVIGATES. The scheme handler cannot
        // navigate (it is a `Send` closure, off the UI thread on desktop), so it
        // pushes the absolute `ipfs://<rootcid><to>` into the shared sink; the
        // shell drains it on its EXISTING pump cadence and performs a real
        // navigation — bar + history move, and the target re-enters the verified
        // `ipfs://` path (that is what hash-verifies it).
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/old/thing").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ipfs://bafyroot/old/thing");
        assert!(!shell.chrome().can_go_back, "one entry so far");

        // The scheme handler matched `/old/* -> /new/:splat 301` and queued the
        // target (the producer side is unit-tested in `crate::ipfs`).
        assert!(queue_for_test(
            &shell,
            &redirects,
            "ipfs://bafyroot/new/thing"
        ));
        assert!(shell.pump(), "the pump follows the queued redirect");
        settle(&mut shell, &handle);

        assert_eq!(
            shell.chrome().url_text,
            "ipfs://bafyroot/new/thing",
            "the redirect target is in the bar (a real navigation, not a rewrite)"
        );
        assert!(
            shell.chrome().can_go_back,
            "the redirect PUSHES an entry (the seam has no replace-current-entry), \
             so back is available"
        );
        assert_eq!(
            shell.chrome().last_error,
            None,
            "the intercepted request's `navigating` failure is spent once the redirect runs"
        );
        assert!(
            !shell.pump(),
            "the sink drains ONCE, so the pump cannot re-navigate the same target"
        );

        // And Back SKIPS the redirecting entry rather than landing on it: because
        // werust pushes, `ipfs://bafyroot/old/thing` is still in history, and
        // landing there would re-match its 3xx rule and bounce the user forward
        // again. There is nothing BEFORE it here (it is the first entry), so the
        // documented edge case applies: the user is left there rather than trapped
        // in a no-op Back.
        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ipfs:///bafyroot/old/thing",
            "with no entry before the redirect source, back lands on it (Decision 8's edge case)"
        );
    }

    #[test]
    fn back_after_a_redirect_skips_the_redirecting_entry_instead_of_bouncing_forward() {
        // Gate-2 defect: werust PUSHES the redirect target instead of REPLACING the
        // redirecting entry (the seam has no replace-current-entry, and WebKitGTK
        // exposes no public API to replace/remove a back-forward-list entry). So
        // the redirected-FROM url stays in history, and a plain Back lands on it,
        // re-matches its 3xx rule, and bounces the user straight forward again —
        // Back is unusable after any redirect. The fix: Back SKIPS a remembered
        // redirect source, the standard emulation of the replaced entry.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        // The page the user actually wants to get BACK to.
        shell.navigate("ipfs://bafyroot/home").unwrap();
        settle(&mut shell, &handle);

        // They click a link to `/old/thing`, whose `_redirects` sends them to
        // `/new/thing`.
        handle.navigate_in_page("ipfs://bafyroot/old/thing");
        shell.pump();
        assert!(queue_for_test(
            &shell,
            &redirects,
            "ipfs://bafyroot/new/thing"
        ));
        shell.pump();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ipfs://bafyroot/new/thing");

        // Back must land on `/home`, NOT on the redirecting `/old/thing`. The skip
        // issues a SECOND history move, whose load signals arrive on a later turn
        // (the same async lag every history move has), so the shell is settled
        // again.
        shell.go_back();
        settle(&mut shell, &handle);
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ipfs:///bafyroot/home",
            "back skips the redirecting entry and reaches the page the user came from"
        );
        assert_eq!(
            redirects.take_pending(),
            None,
            "the skipped-over entry must not re-queue its redirect and bounce the user forward"
        );
        assert!(
            !redirects.is_main_frame("ipfs://bafyroot/old/thing"),
            "the abandoned load must stop counting as the main frame, or a late \
             scheme-handler request for it would re-queue the redirect"
        );
    }

    #[test]
    fn back_skips_every_hop_of_a_multi_hop_redirect_chain() {
        // A chain `/a -> /b -> /c` pushes an entry for EACH hop, so one skip is not
        // enough: Back must skip every remembered source to reach the page the user
        // came from. Bounded by the hop cap, so this can never spin.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/home").unwrap();
        settle(&mut shell, &handle);

        handle.navigate_in_page("ipfs://bafyroot/a");
        shell.pump();
        for hop in ["ipfs://bafyroot/b", "ipfs://bafyroot/c"] {
            assert!(queue_for_test(&shell, &redirects, hop));
            shell.pump();
            settle(&mut shell, &handle);
        }
        assert_eq!(shell.chrome().url_text, "ipfs://bafyroot/c");

        shell.go_back();
        // One settle per history move: the skip walks back over BOTH redirecting
        // entries, each an async move of its own.
        for _ in 0..3 {
            settle(&mut shell, &handle);
        }
        assert_eq!(
            shell.chrome().url_text,
            "ipfs:///bafyroot/home",
            "back skips BOTH redirecting entries of the chain"
        );
    }

    #[test]
    fn back_over_an_ordinary_entry_is_untouched_by_the_redirect_skip() {
        // The skip must be surgical: only a url THIS chain redirected away from is
        // skipped. Ordinary browsing history is unaffected — no page is ever
        // silently jumped over.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/one").unwrap();
        settle(&mut shell, &handle);
        handle.navigate_in_page("ipfs://bafyroot/two");
        settle(&mut shell, &handle);
        handle.navigate_in_page("ipfs://bafyroot/three");
        settle(&mut shell, &handle);

        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ipfs:///bafyroot/two",
            "a plain back moves exactly one entry, skipping nothing"
        );
    }

    #[test]
    fn a_redirect_inside_an_ens_site_keeps_the_eth_identity_in_the_bar() {
        // Acceptance (compose with the root-CID-prefix `ens_pages` association): a
        // 3xx WITHIN an ENS site lands on the SAME root CID, so the site identity
        // survives — the bar shows `ronan.eth/<new-path>` and the ENS posture is
        // re-marked, never the raw `ipfs://<rootcid>/<path>`. No new mechanism:
        // this is the existing association doing its job because the redirect is
        // confined to the root CID.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, _ipfs_uri) = ipfs_contenthash_fixture(page);
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let provider = ScriptedProvider::new(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);
        let mut shell = BrowserShell::with_provider(Box::new(backend), Box::new(provider))
            .with_redirect_sink(redirects.clone());

        shell.navigate("ronan.eth").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth");

        // The site's `_redirects` sends `/old` to `/new/page` — under the SAME root
        // CID (the only kind of target the rules may name).
        let root = ipfs_root_of(&handle);
        assert!(queue_for_test(
            &shell,
            &redirects,
            &format!("{root}/new/page")
        ));
        shell.pump();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);

        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/new/page",
            "a redirect inside an ENS site keeps the site identity in the bar"
        );
        assert!(
            !shell.chrome().url_text.starts_with("ipfs://"),
            "the raw root cid must never leak into the bar on a redirect"
        );
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "the redirected page keeps the ENS site's posture"
        );
    }

    #[test]
    fn the_redirected_from_requests_own_failure_is_not_shown_but_a_refusal_is() {
        // A matched 3xx answers the intercepted request fail-closed (nothing may
        // render under the OLD url), which the backend reports as a FAILED load.
        // That failure is bookkeeping for a navigation about to happen, so its
        // banner is suppressed — the user should see the redirected page, not an
        // error flash. A REFUSED redirect (off-root, or a chain the sink bounded)
        // carries no such marker and MUST still surface: the chain stops there and
        // nothing will render.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ipfs://bafyroot/old").unwrap();
        handle.drive_to_failed(&format!(
            "ipfs:// {marker}: 301 to `/new.html`",
            marker = crate::ipfs::REDIRECT_NAVIGATING_MARKER
        ));
        shell.pump();
        assert_eq!(
            shell.chrome().last_error,
            None,
            "the redirected-FROM request's own failure is not shown to the user"
        );

        shell.navigate("ipfs://bafyroot/off").unwrap();
        handle.drive_to_failed(
            "ipfs:// _redirects fallback failed: target `https://evil.example/x` leaves the \
             site's root cid",
        );
        shell.pump();
        let shown = shell
            .chrome()
            .last_error
            .as_deref()
            .expect("a refused redirect is a real failure the user must see");
        assert!(shown.contains("root cid"), "got: {shown}");
    }

    #[test]
    fn a_user_navigation_resets_the_redirect_chain_budget() {
        // The chain bound is PER-CHAIN, not per-session: a user-initiated
        // navigation (typed URL, back/forward, reload) starts fresh, so a site that
        // legitimately redirects on every visit is never progressively starved.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/a").unwrap();
        settle(&mut shell, &handle);
        // Walk the chain to its bound: the sink refuses the hop past the cap.
        for hop in 0..crate::ipfs::MAX_REDIRECT_HOPS {
            assert!(
                queue_for_test(&shell, &redirects, &format!("ipfs://bafyroot/hop-{hop}")),
                "hop {hop} is within the bound"
            );
            shell.pump();
            settle(&mut shell, &handle);
        }
        assert!(
            !queue_for_test(&shell, &redirects, "ipfs://bafyroot/hop-over"),
            "the hop past the cap is refused, so the chain cannot loop"
        );

        // A USER navigation resets the budget.
        shell.navigate("ipfs://bafyroot/fresh").unwrap();
        settle(&mut shell, &handle);
        assert!(
            queue_for_test(&shell, &redirects, "ipfs://bafyroot/hop-0"),
            "a user-initiated navigation starts a fresh chain"
        );
    }

    #[test]
    fn an_in_page_link_click_resets_the_redirect_chain_budget_too() {
        // The chain bound must be PER-CHAIN, and an IN-PAGE LINK CLICK is the case
        // `shell.navigate` does NOT cover: the webview loads the link itself and
        // only REPORTS it back as a load event, so the shell's `navigate` /
        // `go_back` / `reload` reset points never fire. Without the pump reporting
        // that load to the sink, the visited set accumulates for the whole session
        // and the SAME redirecting link is refused as a cycle the second time it is
        // clicked (Gate-2 finding).
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/home").unwrap();
        settle(&mut shell, &handle);

        // The user clicks the site's `/docs` nav link, which its `_redirects` sends
        // to `/docs/index.html`. Twice, with an unrelated page in between.
        for round in 0..3 {
            handle.navigate_in_page("ipfs://bafyroot/docs");
            shell.pump();
            assert!(
                queue_for_test(&shell, &redirects, "ipfs://bafyroot/docs/index.html"),
                "click {round} on the SAME redirecting link must still be followed, \
                 not refused as a cycle"
            );
            shell.pump();
            settle(&mut shell, &handle);
            assert_eq!(shell.chrome().url_text, "ipfs://bafyroot/docs/index.html");

            handle.navigate_in_page("ipfs://bafyroot/about");
            shell.pump();
            settle(&mut shell, &handle);
        }
    }

    #[test]
    fn many_unrelated_redirected_link_clicks_never_exhaust_the_session() {
        // The other half of the same defect: MORE than `MAX_REDIRECT_HOPS`
        // DIFFERENT redirecting links clicked in one session must each get the full
        // budget. Session-scoped state would refuse the sixth.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/home").unwrap();
        settle(&mut shell, &handle);

        for click in 0..(crate::ipfs::MAX_REDIRECT_HOPS * 2 + 1) {
            handle.navigate_in_page(&format!("ipfs://bafyroot/link-{click}"));
            shell.pump();
            assert!(
                queue_for_test(&shell, &redirects, &format!("ipfs://bafyroot/dest-{click}")),
                "unrelated click {click} must get a fresh budget"
            );
            shell.pump();
            settle(&mut shell, &handle);
        }
    }

    #[test]
    fn the_chain_bound_still_holds_across_the_redirect_hops_the_shell_itself_performs() {
        // The bound must survive the per-chain reset: each hop the SHELL performs
        // is reported to the sink as a top-level navigation too, and that one
        // report must CONTINUE the chain (it is the chain's own target) rather than
        // restore the budget — otherwise the reset that fixes the link-click gap
        // would reintroduce the unbounded loop.
        let redirects = crate::ipfs::RedirectSink::new();
        let backend = FakeBackend::default();
        let handle = backend.handle();
        let mut shell = BrowserShell::new(Box::new(backend)).with_redirect_sink(redirects.clone());

        shell.navigate("ipfs://bafyroot/a").unwrap();
        settle(&mut shell, &handle);
        for hop in 0..crate::ipfs::MAX_REDIRECT_HOPS {
            assert!(
                queue_for_test(&shell, &redirects, &format!("ipfs://bafyroot/hop-{hop}")),
                "hop {hop} is within the bound"
            );
            // The shell performs the hop and the backend reports the load: neither
            // may restore the budget.
            shell.pump();
            settle(&mut shell, &handle);
        }
        assert!(
            !queue_for_test(&shell, &redirects, "ipfs://bafyroot/hop-over"),
            "the hop past the cap is still refused after the shell followed every hop"
        );
    }

    /// Stand in for the `ipfs://` scheme handler queueing a redirect target into
    /// the shared sink (the producer side is exercised for real in
    /// `crate::ipfs`'s tests). Returns whether the sink ACCEPTED the hop, so a
    /// test can assert the chain bound refusing one.
    ///
    /// The hop's SOURCE is the backend's current URL, because that is exactly what
    /// the real handler intercepts: a 3xx is only ever matched on the top-level
    /// document the shell is loading. The sink remembers it so `go_back` can skip
    /// the redirecting entry.
    fn queue_for_test(
        shell: &BrowserShell,
        sink: &crate::ipfs::RedirectSink,
        target: &str,
    ) -> bool {
        let source = shell
            .current_url_for_test()
            .expect("a redirect is matched on a document the backend is loading");
        sink.queue(&source, target).is_ok()
    }

    /// The `ipfs://<rootcid>` root URL the backend currently reports (the RAW,
    /// pre-WebKit-normalize form the shell resolved the ENS name to), so a test can
    /// build a same-document sub-path URL `<rootcid>/<path>` of the SAME site.
    fn ipfs_root_of(handle: &BackendHandle) -> String {
        handle
            .inner
            .borrow()
            .current()
            .cloned()
            .expect("a current ipfs root url")
    }

    // ---- A `.eth` name WITH a path -> ENS front door + `ipfs://<cid>/<path>` ---
    // (task `eth-name-with-path-routes-to-ens-and-subpath`)

    #[test]
    fn a_dot_eth_with_a_path_routes_to_ens_and_loads_the_subpath_keeping_the_name_path_in_the_bar()
    {
        // Acceptance (the DONE bar, offline): `ronan.eth/blog/` is detected as the
        // ENS front door for `ronan.eth` (NOT `https://ronan.eth/blog/`), resolves
        // the name to its `ipfs-ns` contenthash, and loads the SUB-PATH
        // `ipfs://<cid>/blog/` (its index.html via the existing ipfs path
        // resolution). The bar keeps the identity+path the user typed
        // (`ronan.eth/blog/`) with the ENS posture — never the raw CID+path.
        let page = b"<!doctype html><title>ronan blog</title>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),    // registry.resolver(node)
            Ok(abi_bytes_return(&contenthash)), // resolver.contenthash(node)
        ]);

        shell
            .navigate("ronan.eth/blog/")
            .expect("the front door handles a .eth entry with a path");
        // The bar shows the NAME+PATH, not the CID, even while it loads.
        assert_eq!(shell.chrome().url_text, "ronan.eth/blog/");
        assert!(
            shell.chrome().is_loading(),
            "the ipfs subpath load is in flight"
        );
        // The underlying load went to the resolved `ipfs://<cid>/blog/` — the CID
        // from the name PLUS the typed sub-path, NOT `https://ronan.eth/blog/`.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(format!("{ipfs_uri}/blog/").as_str())
        );

        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/blog/",
            "the .eth name + path stays in the bar through the whole verified load"
        );
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );
        assert!(shell.chrome().is_name_via_trusted_rpc());
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn a_github_com_path_still_routes_to_https_not_ens() {
        // Acceptance: a NON-`.eth` host with a path (`github.com/foo`) still routes
        // to `https://github.com/foo` — the name+path split only fires for a `.eth`
        // TLD label, so a plausible dotted host is untouched (no ENS hijack).
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("github.com/foo")
            .expect("navigates as https");
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some("https://github.com/foo"),
            "a non-.eth host+path is an https candidate, never ENS"
        );
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://github.com/foo");
        assert_eq!(shell.chrome().trust_posture, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn a_dot_eth_path_that_resolves_to_no_dag_entity_fails_closed_keeping_the_bar() {
        // Acceptance: a `.eth/<path>` whose PATH resolves to no entity in the DAG
        // fails closed with the existing legible reason (the ipfs path-not-found
        // class, surfaced as a failed load), keeps the typed `.eth/<path>` in the
        // bar, and NEVER falls through to https or silently resets — mirroring a
        // failed bare-name load. The name RESOLVES fine (a valid contenthash); it
        // is the sub-path load that fails at the backend/scheme handler.
        let page = b"<!doctype html><title>ronan</title>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        shell
            .navigate("ronan.eth/no-such-path/")
            .expect("the front door handles the entry");
        // The resolved sub-path load is in flight against the CID+path.
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(format!("{ipfs_uri}/no-such-path/").as_str())
        );
        // The `ipfs://` scheme handler fails the sub-path load (the DAG has no such
        // entity): a hard, legible reason surfaces and the typed name+path stays.
        handle.drive_to_failed(
            "ipfs:// content-addressed load failed: sub-resource path not found: /no-such-path/",
        );
        shell.pump();
        assert_eq!(shell.chrome().load_state, LoadState::Failed);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/no-such-path/",
            "a failed sub-path load keeps the typed .eth/<path> in the bar"
        );
        assert!(
            !shell.chrome().url_text.starts_with("https://"),
            "no https fallthrough on a failed sub-path load"
        );
        assert!(
            shell.chrome().last_error.is_some(),
            "a legible reason is shown"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn reload_and_back_re_derive_the_name_and_path_of_a_dot_eth_path_page() {
        // Acceptance: reload / back / forward of a `.eth/<path>` page keep the
        // name+path+posture. Reload RE-RESOLVES the name and re-loads the same
        // sub-path; back onto the sub-path entry re-derives `ronan.eth/blog/` from
        // the CID+path `ens_pages` key (never leaking the raw CID+path).
        let page = b"<!doctype html><title>ronan blog</title>";
        let (contenthash, ipfs_uri) = ipfs_contenthash_fixture(page);
        // Three resolutions worth of answers: the initial load, the reload
        // re-resolve, and the back re-resolve are each a fresh namehash ->
        // resolver -> contenthash pair.
        let (mut shell, handle) = shell_with_provider(vec![
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
            Ok(address_word(&[0x11u8; 20])),
            Ok(abi_bytes_return(&contenthash)),
        ]);

        // Load the `.eth/<path>` page.
        shell.navigate("ronan.eth/blog/").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth/blog/");
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(format!("{ipfs_uri}/blog/").as_str())
        );

        // Reload RE-RESOLVES the name+path (the recorded reload decision): the
        // sub-path is reloaded and the name+path+posture stay in the bar.
        shell
            .reload()
            .expect("reload re-resolves the .eth/<path> page");
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/blog/",
            "reload keeps the name+path in the bar"
        );
        assert_eq!(
            shell.current_url_for_test().as_deref(),
            Some(format!("{ipfs_uri}/blog/").as_str()),
            "reload re-loads the same resolved sub-path"
        );
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "ronan.eth/blog/");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc
        );

        // Navigate away to a plain page, then back onto the ENS sub-path entry: its
        // name+path is re-derived from the CID+path `ens_pages` key (the pin was
        // dropped, but the entry is recoverable), never the raw CID.
        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().url_text,
            "ronan.eth/blog/",
            "back onto the ENS sub-path re-derives the name+path via ens_pages"
        );
        assert!(!shell.chrome().url_text.starts_with("ipfs://"));
        handle.serve_via_verified_content_path();
        shell.pump();
        assert_eq!(shell.chrome().url_text, "ronan.eth/blog/");
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::NameViaTrustedRpc,
            "the ENS sub-path entry keeps its posture on history return"
        );
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

    // ---- The debug capture store on the shell (task -------------------------
    // `debug-capture-store-console-and-network-in-core`)

    #[test]
    fn the_shell_owns_a_debug_capture_the_edges_read_and_the_capture_points_feed() {
        use crate::debug::{ConsoleEntry, ConsoleLevel, NetworkEntry};

        // The shell owns ONE bounded store; a capture point pushes into the clone
        // it holds and the shell's own accessor sees the SAME entries, so every
        // edge renders one shared fact (exactly as `RedirectSink` is shared).
        let (shell, _handle) = shell_with_backend();
        let capture_point = shell.debug_capture().clone();
        capture_point.push_console(ConsoleEntry::new(ConsoleLevel::Error, "boom"));
        capture_point.push_network(
            NetworkEntry::new("GET", "ipfs://bafy/x").with_trust(TrustPosture::ContentVerified),
        );

        assert_eq!(shell.debug_capture().console().len(), 1);
        assert_eq!(shell.debug_capture().network().len(), 1);
        assert_eq!(
            shell.debug_capture().network()[0].trust,
            TrustPosture::ContentVerified,
            "the entry keeps its honest per-request posture"
        );
    }

    #[test]
    fn a_shared_debug_capture_can_be_installed_at_the_edge_before_the_shell_owns_it() {
        use crate::debug::{DebugCapture, NetworkEntry};

        // The platform capture points are installed on the backend BEFORE the
        // shell owns it (the same shape as `with_redirect_sink`), so the edge can
        // create the store, clone it into its hooks, and hand it to the shell.
        let capture = DebugCapture::new();
        let backend = FakeBackend::default();
        let shell = BrowserShell::new(Box::new(backend)).with_debug_capture(capture.clone());
        capture.push_network(NetworkEntry::new("GET", "https://x/y"));
        assert_eq!(
            shell.debug_capture().network().len(),
            1,
            "the edge's clone and the shell's are the same store"
        );
    }

    #[test]
    fn the_shell_exposes_the_capture_as_the_debug_json_document_the_edges_render() {
        use crate::debug::{ConsoleEntry, ConsoleLevel, NetworkEntry};

        let (shell, _handle) = shell_with_backend();
        shell
            .debug_capture()
            .push_console(ConsoleEntry::new(ConsoleLevel::Warn, "careful"));
        shell.debug_capture().push_network(
            NetworkEntry::new("GET", "ipfs://bafy/x")
                .with_status(200)
                .with_trust(TrustPosture::ContentVerified),
        );

        let json: serde_json::Value =
            serde_json::from_str(&shell.debug_json()).expect("valid debug JSON");
        assert_eq!(json["console"][0]["level"], "warn");
        assert_eq!(json["network"][0]["trust"], "content-verified");
        assert_eq!(json["networkCaptureEnabled"], true);
    }

    #[test]
    fn the_debug_capture_does_not_disturb_the_chrome_state() {
        use crate::debug::{ConsoleEntry, ConsoleLevel, NetworkEntry};

        // Capture is READ-ONLY observation: pushing entries must not touch the URL
        // bar, the load state, or the PAGE's trust posture (a captured
        // unverified subresource can never downgrade a verified page, and a
        // captured verified entry can never upgrade an unverified one).
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://example.com/").expect("navigate");
        settle(&mut shell, &handle);
        let before = shell.chrome().clone();

        shell
            .debug_capture()
            .push_console(ConsoleEntry::new(ConsoleLevel::Error, "page error"));
        shell.debug_capture().push_network(
            NetworkEntry::new("GET", "ipfs://bafy/x").with_trust(TrustPosture::ContentVerified),
        );

        assert_eq!(
            shell.chrome(),
            &before,
            "capturing entries changes nothing about the page's chrome"
        );
    }

    // -- The chrome PRESENTATION rules (moved out of the GTK edge) -----------
    //
    // These were unit-tested in `crates/werust/src/main.rs` while the derivation
    // lived there; they moved HERE with the functions (task
    // `desktop-chrome-presentation-into-core`), unchanged in substance, so the
    // rules are proven in the toolkit-free core with no display.
    #[test]
    fn status_line_names_the_live_pipeline_step_while_loading() {
        // Acceptance (loading progress): while a load is in flight the status line
        // names the REAL pipeline step (resolving name / fetching content /
        // rendering) so a slow load reads as working, not frozen. A settled/idle
        // load shows no step, and a failure still wins.
        let fetching = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingContent,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&fetching), "loading… — fetching content");

        let resolving = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&resolving), "loading… — resolving name");

        let rendering = ChromeState {
            load_state: LoadState::Committed,
            load_step: LoadStep::Rendering,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&rendering), "loading… — rendering");

        // A loading state with no known step (Idle step) falls back to plain
        // "loading…" rather than a dangling dash.
        let loading_no_step = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::Idle,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&loading_no_step), "loading…");

        // A settled load shows idle; a failure still wins over any step.
        assert_eq!(status_line(&ChromeState::default()), "idle");
        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        assert_eq!(
            status_line(&failed),
            "failed: points to Swarm, not supported"
        );
    }

    #[test]
    fn load_progress_is_a_url_bar_fraction_that_never_displaces_the_page() {
        // Acceptance (task `loading-progress-in-the-url-bar-not-a-banner`): the
        // in-flight load indicator is a PROGRESS FRACTION painted INSIDE the URL
        // bar, not a banner that takes height from the page view. The old banner
        // was a real child above the view, so every navigation resized the page
        // twice (banner in, banner out) and the content jumped under the pointer.
        // A fraction changes NO widget geometry: `0.0` paints nothing, so an idle
        // chrome and a loading chrome are exactly the same size.
        //
        // Pure functions of `ChromeState` (driven by the existing chrome-refresh
        // pump, no new timer / poll / tight loop), so they are testable without a
        // display; the mobile shells apply the SAME rules from the SAME chrome
        // facts.

        // Nothing in flight (idle / finished / failed): no progress, no hint.
        for settled in [
            ChromeState::default(),
            ChromeState {
                load_state: LoadState::Finished,
                ..ChromeState::default()
            },
            ChromeState {
                load_state: LoadState::Failed,
                last_error: Some("name not resolved".into()),
                ..ChromeState::default()
            },
        ] {
            assert!(!load_progress_visible(&settled));
            assert_eq!(
                load_progress_fraction(&settled),
                0.0,
                "a settled chrome paints NO progress in the URL bar"
            );
            assert_eq!(load_progress_hint(&settled), "");
        }

        // In flight: a visible fraction that ADVANCES with the real pipeline
        // phase, so a slow load reads as working rather than frozen. The hint is
        // the existing `LoadStep` vocabulary (the same words the status line and
        // the debug Network tab speak), so no surface can disagree.
        let phases = [
            (LoadState::Started, LoadStep::Idle, "loading"),
            (
                LoadState::Started,
                LoadStep::ResolvingName,
                "resolving name",
            ),
            (
                LoadState::Started,
                LoadStep::FetchingRecord,
                "fetching record",
            ),
            (
                LoadState::Started,
                LoadStep::FetchingContent,
                "fetching content",
            ),
            (LoadState::Committed, LoadStep::Rendering, "rendering"),
        ];
        let mut previous = 0.0;
        for (load_state, load_step, hint) in phases {
            let state = ChromeState {
                load_state,
                load_step,
                ..ChromeState::default()
            };
            assert!(load_progress_visible(&state), "{load_step:?} is in flight");
            let fraction = load_progress_fraction(&state);
            assert!(
                fraction > previous,
                "{load_step:?} advances the bar: {fraction} must exceed {previous}"
            );
            assert!(
                fraction < 1.0,
                "{load_step:?} is not done, so the bar is never full: {fraction}"
            );
            assert_eq!(load_progress_hint(&state), hint);
            previous = fraction;
        }

        // The PRE-CONTENT resolution window is covered too: while the shell is
        // resolving a name the backend has not started a load yet (`is_loading()`
        // is false, the step is pinned), which is EXACTLY the long `ronan.eth`
        // freeze the old banner missed. A pinned step means work is in flight, so
        // the URL bar shows progress (the named follow-up of the banner task).
        let resolving_before_handoff = ChromeState {
            load_state: LoadState::Idle,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        };
        assert!(!resolving_before_handoff.is_loading());
        assert!(
            load_progress_visible(&resolving_before_handoff),
            "the name-resolution window shows progress even before the backend load starts"
        );
        assert!(load_progress_fraction(&resolving_before_handoff) > 0.0);
        assert_eq!(
            load_progress_hint(&resolving_before_handoff),
            "resolving name"
        );
    }

    #[test]
    fn the_url_bar_progress_tooltip_is_composed_once_here_for_every_painter() {
        // Acceptance (task `one-derivation-close-the-aggregate-and-tooltip-gaps`):
        // the URL bar's progress tooltip is a pure function of `ChromeState`, so
        // it is composed HERE beside the other `load_progress_*` rules and every
        // painter CALLS it. It was written out twice — verbatim, comments and all
        // — in the GTK and the AppKit edges, which is exactly how the Kotlin and
        // Swift twins started drifting.

        // Settled: no tooltip at all, so a stale phase never lingers on hover.
        for settled in [
            ChromeState::default(),
            ChromeState {
                load_state: LoadState::Finished,
                ..ChromeState::default()
            },
        ] {
            assert_eq!(
                load_progress_tooltip(&settled, STOP_AFFORDANCE_LABEL),
                None,
                "a settled chrome has no phase to name"
            );
        }

        // A BACKEND load in flight: the phase name plus the cancel hint, which is
        // honest exactly then — Stop is sensitive/enabled on precisely this fact.
        let loading = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingContent,
            ..ChromeState::default()
        };
        assert_eq!(
            load_progress_tooltip(&loading, STOP_AFFORDANCE_LABEL).as_deref(),
            Some("fetching content… — press Stop (✕) to cancel"),
            "the sentence both desktop edges show today, now derived once"
        );

        // The PRE-CONTENT resolution window: work is in flight but there is no
        // backend load for Stop to cancel, so the sentence promises no cancel.
        let resolving = ChromeState {
            load_state: LoadState::Idle,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        };
        assert!(!resolving.is_loading());
        assert_eq!(
            load_progress_tooltip(&resolving, STOP_AFFORDANCE_LABEL).as_deref(),
            Some("resolving name…"),
            "promising a cancel with no backend load to stop would lie"
        );

        // The phase half is the shared `load_progress_hint`, never a second
        // vocabulary, over the whole in-flight axis.
        for step in LoadStep::ALL {
            let state = ChromeState {
                load_state: LoadState::Started,
                load_step: step,
                ..ChromeState::default()
            };
            let tooltip = load_progress_tooltip(&state, STOP_AFFORDANCE_LABEL)
                .expect("a load in flight names its phase");
            assert!(
                tooltip.starts_with(load_progress_hint(&state)),
                "`{tooltip}` must open with the shared phase hint"
            );
        }

        // The STOP AFFORDANCE is the painter's, so the label is a parameter: an
        // edge whose Stop control is labelled differently passes its own label
        // instead of forking the sentence.
        assert_eq!(
            load_progress_tooltip(&loading, "Esc").as_deref(),
            Some("fetching content… — press Stop (Esc) to cancel")
        );
        assert_eq!(
            STOP_AFFORDANCE_LABEL, "✕",
            "the label every werust edge shows on its Stop control today"
        );
    }

    /// One failure REASON per [`FailureKind`], plus `None` (nothing failed): the
    /// failure-severity axis of [`every_chrome_state_shape`].
    ///
    /// [`ChromeState`] carries the reason as free TEXT and derives the severity
    /// from it ([`FailureKind::classify`]), so driving the severity axis needs a
    /// sample reason per kind. Both halves are pinned: the sample list is a match
    /// over [`FailureKind`] with no wildcard arm (a third severity does not
    /// COMPILE until it names its sample here) and each sample is asserted to
    /// really classify as its own kind (so a sample cannot rot into a duplicate of
    /// another severity and quietly stop driving one).
    fn every_failure_reason() -> Vec<Option<&'static str>> {
        let mut reasons: Vec<Option<&'static str>> = vec![None];
        for kind in FailureKind::ALL {
            let reason = match kind {
                FailureKind::Transient => "transport error: timeout: global",
                FailureKind::Hard => "points to Swarm, not supported",
            };
            assert_eq!(
                FailureKind::classify(reason),
                kind,
                "`{reason}` is this drive's sample for {kind:?} but classifies as something else"
            );
            reasons.push(Some(reason));
        }
        reasons
    }

    /// Every state of the TOFU MUTABLE-NAME axis a chrome rule can branch on: no
    /// name-resolved page at all, a name nobody has blessed, a blessed name still
    /// on its blessed CID, and a blessed name that has CHANGED.
    ///
    /// A match over the four cases the axis really has, so it is exhaustive by
    /// inspection the way the enum axes are by construction: the axis is an
    /// `Option<MutableNameTrust>` whose inner state is decided by the presence and
    /// the CID of a pin, and these are all four combinations.
    fn every_mutable_name_shape() -> Vec<Option<crate::pins::MutableNameTrust>> {
        let pin = crate::pins::TrustedNamePin {
            name: "ronan.eth".to_string(),
            cid: "bafyblessed".to_string(),
            blessed_at: 1_800_000_000,
            posture: TrustPosture::NameViaTrustedRpc,
        };
        vec![
            // Not a name-resolved page (a direct `ipfs://<cid>`, an https page).
            None,
            // A mutable name nobody has blessed: behaves exactly as before TOFU.
            Some(crate::pins::MutableNameTrust {
                name: "ronan.eth".to_string(),
                cid: "bafyblessed".to_string(),
                blessed: None,
            }),
            // Blessed, and still the blessed content.
            Some(crate::pins::MutableNameTrust {
                name: "ronan.eth".to_string(),
                cid: "bafyblessed".to_string(),
                blessed: Some(pin.clone()),
            }),
            // Blessed, and now pointing somewhere else: the TOFU warning.
            Some(crate::pins::MutableNameTrust {
                name: "ronan.eth".to_string(),
                cid: "bafychanged".to_string(),
                blessed: Some(pin),
            }),
        ]
    }

    /// Every SHAPE of [`ChromeState`] a chrome rule can branch on: the cartesian
    /// product of all its axes (load state x pipeline step x trust posture x
    /// failure severity x the invalid-entry axis x the TOFU mutable-name axis x
    /// the history flags x an empty/non-empty URL).
    ///
    /// Every ENUM axis is driven EXHAUSTIVELY BY CONSTRUCTION — from
    /// [`LoadState::ALL`], [`LoadStep::ALL`], [`TrustPosture::ALL`] and (through
    /// [`every_failure_reason`]) [`FailureKind::ALL`], each of which a compile-time
    /// check keeps complete — so adding a variant to any of them cannot compile
    /// until the new variant joins its list, and it is then driven through the
    /// callers below rather than silently escaping them. That is the tooth that
    /// makes the CSS-class-set test bite on the Phase-2 name-verified posture.
    ///
    /// The remaining axes are NOT enum-shaped and are driven over representative
    /// values instead: an empty vs non-empty URL, an absent vs present
    /// invalid-entry text, and both history flags. So a rule that started
    /// branching on the CONTENT of one of those strings (a particular scheme, say)
    /// could still escape this drive; a rule that branches on a state MACHINE
    /// cannot. A few thousand plain values, so it stays a fast unit test.
    fn every_chrome_state_shape() -> Vec<ChromeState> {
        let mut shapes = Vec::new();
        let failure_reasons = every_failure_reason();
        for load_state in LoadState::ALL {
            for load_step in LoadStep::ALL {
                for posture in TrustPosture::ALL {
                    for last_error in failure_reasons.iter().copied() {
                        for invalid_entry in [None, Some("not a url")] {
                            for mutable_name in every_mutable_name_shape() {
                                for can_go_back in [false, true] {
                                    for can_go_forward in [false, true] {
                                        for url_text in ["", "ipfs://bafy/index.html"] {
                                            shapes.push(ChromeState {
                                                url_text: url_text.to_string(),
                                                load_state,
                                                load_step,
                                                trust_posture: posture,
                                                last_error: last_error.map(str::to_string),
                                                invalid_entry: invalid_entry.map(str::to_string),
                                                mutable_name: mutable_name.clone(),
                                                can_go_back,
                                                can_go_forward,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        shapes
    }

    #[test]
    fn every_chrome_css_class_the_derivation_can_return_is_in_the_exported_set() {
        // Acceptance (task `export-the-chrome-css-class-set-from-core`): the
        // COMPLETE chrome CSS-class set is exported beside the `*_css_class`
        // functions, and it is EXHAUSTIVE — every value those functions can
        // return is a member. A painter toggles exactly one class of a family on
        // and every other one off, so a class the core can return but the set
        // omits is a class no painter ever CLEARS: a stale badge colour lingering
        // across a transition. Adding a fifth posture (or a third failure
        // severity) without extending the set therefore reds the gate HERE,
        // before three painters inherit the stale list.
        //
        // The drive is the CARTESIAN PRODUCT of every ChromeState axis (see
        // `every_chrome_state_shape`), not just the axes today's rules happen to
        // read, and every ENUM axis is exhaustive BY CONSTRUCTION (`LoadState::ALL`,
        // `LoadStep::ALL`, `TrustPosture::ALL`, `FailureKind::ALL`, each kept
        // complete by a compile-time check). So a FIFTH trust posture cannot be
        // added without compiling against this drive: the new posture is driven
        // here, its class comes back, and this test reds unless the exported set
        // grew with it.
        let mut produced_trust = std::collections::BTreeSet::new();
        let mut produced_banner = std::collections::BTreeSet::new();
        for state in every_chrome_state_shape() {
            let trust = trust_indicator_css_class(&state);
            assert!(
                TRUST_INDICATOR_CSS_CLASSES.contains(&trust),
                "`{trust}` is returned for {state:?} but is not in the exported set"
            );
            produced_trust.insert(trust);
            // The banner class is only painted when the banner shows.
            if error_banner_visible(&state) {
                let banner = error_banner_css_class(&state);
                assert!(
                    ERROR_BANNER_CSS_CLASSES.contains(&banner),
                    "`{banner}` is returned for {state:?} but is not in the exported set"
                );
                produced_banner.insert(banner);
            }
        }

        // The other direction, so the set cannot carry a DEAD name either (a
        // class every painter toggles off forever, and whose stylesheet rule the
        // no-unstyled-class guard then demands for nothing): the set is EXACTLY
        // what the derivation produces.
        assert_eq!(
            produced_trust,
            TRUST_INDICATOR_CSS_CLASSES
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            "the exported trust set is exactly what `trust_indicator_css_class` produces"
        );
        assert_eq!(
            produced_banner,
            ERROR_BANNER_CSS_CLASSES
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            "the exported banner set is exactly what `error_banner_css_class` produces"
        );

        // The COMPLETE set is both families and nothing else, with no name shared
        // between them (a class in two families would be toggled off by one
        // painter loop while the other turned it on).
        let complete: Vec<&str> = CHROME_CSS_CLASS_SETS
            .iter()
            .flat_map(|family| family.iter().copied())
            .collect();
        let unique: std::collections::BTreeSet<&str> = complete.iter().copied().collect();
        assert_eq!(
            unique.len(),
            complete.len(),
            "no chrome CSS class appears in two families: {complete:?}"
        );
        assert_eq!(
            unique,
            produced_trust
                .union(&produced_banner)
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            "the complete set is the trust postures plus the banner severities"
        );
        // Every exported name is a usable CSS class name (non-empty, no dot, no
        // whitespace): an edge interpolates it straight into a stylesheet rule.
        for class in complete {
            assert!(
                !class.is_empty()
                    && !class.contains('.')
                    && !class.chars().any(char::is_whitespace),
                "`{class}` is not a plain CSS class name"
            );
        }
    }

    #[test]
    fn the_family_aggregate_holds_every_exported_class_family_for_the_coverage_gates() {
        // Acceptance (task `one-derivation-close-the-aggregate-and-tooltip-gaps`):
        // the previous task made each family exhaustive over its CLASSES, but the
        // SET OF FAMILIES was hand-written in each painter's coverage gate, so a
        // SIXTH family would join neither gate and render invisibly on both
        // desktops with both suites green. `CssClassFamily::ALL` is that missing
        // aggregate, kept exhaustive BY CONSTRUCTION (the const check beside it).
        let aggregate: Vec<&[&str]> = CssClassFamily::ALL
            .iter()
            .map(|family| family.classes())
            .collect();
        for exported in [
            TRUST_INDICATOR_CSS_CLASSES,
            ERROR_BANNER_CSS_CLASSES,
            crate::debug::DEBUG_CONSOLE_CSS_CLASSES,
        ] {
            assert!(
                aggregate.contains(&exported),
                "the core exports {exported:?} but the coverage aggregate omits it, so no \
                 painter's gate would ever check it"
            );
        }
        assert_eq!(
            aggregate.len(),
            3,
            "a new family must be NAMED here too, so its arrival is a deliberate edit"
        );

        // The narrower TOGGLING set keeps its own meaning: `CHROME_CSS_CLASS_SETS`
        // is what a chrome painter turns on/off on ONE widget (exactly one on),
        // and the debug view's row levels are deliberately NOT part of it. The
        // aggregate is for COVERAGE gates only, so it is a strict SUPERSET.
        for toggled in CHROME_CSS_CLASS_SETS {
            assert!(
                aggregate.contains(toggled),
                "every toggling family is covered by the aggregate too"
            );
        }
        assert!(
            !CHROME_CSS_CLASS_SETS.contains(&crate::debug::DEBUG_CONSOLE_CSS_CLASSES),
            "the console levels colour a debug ROW; no painter toggles them on a chrome widget"
        );
        assert!(
            aggregate.len() > CHROME_CSS_CLASS_SETS.len(),
            "the aggregate is the wider coverage view, not a rename of the toggling set"
        );

        // No class belongs to two families (one painter loop would clear what
        // another just set), every family is non-empty, and every name is a plain
        // CSS class an edge can interpolate straight into a stylesheet selector.
        let all: Vec<&str> = aggregate.iter().flat_map(|f| f.iter().copied()).collect();
        let unique: std::collections::BTreeSet<&str> = all.iter().copied().collect();
        assert_eq!(
            unique.len(),
            all.len(),
            "no class is in two families: {all:?}"
        );
        for family in &aggregate {
            assert!(!family.is_empty(), "an empty family styles nothing");
        }
        for class in all {
            assert!(
                !class.is_empty()
                    && !class.contains('.')
                    && !class.chars().any(char::is_whitespace),
                "`{class}` is not a plain CSS class name"
            );
        }
    }

    #[test]
    fn the_chrome_json_carries_the_derivation_verbatim_for_every_chrome_shape() {
        // THE acceptance property of the mobile carrier (task
        // `mobile-chrome-presentation-from-one-derivation`): the chrome JSON is a
        // CARRIER of this crate's derivation, never a second one. So for EVERY
        // shape of `ChromeState` a rule can branch on, each derived field must
        // EQUAL the core function that decides it, which is what lets Kotlin and
        // Swift delete their `statusLine()` / `trustIndicator()` /
        // `errorBanner()` / `invalidEntryBadge()` / `loadProgress*()` twins and
        // read a field instead.
        //
        // The drive is the same cartesian product the CSS-class test uses
        // (`every_chrome_state_shape`), exhaustive BY CONSTRUCTION over every enum
        // axis, so a fifth trust posture or a sixth pipeline step cannot land with
        // a carrier that silently stopped agreeing with the rule.
        for state in every_chrome_state_shape() {
            let json = chrome_json(&state);
            let doc: serde_json::Value =
                serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON {json}: {e}"));
            let derived: Vec<(&str, serde_json::Value)> = vec![
                ("statusLine", status_line(&state).into()),
                ("trustIndicator", trust_indicator(&state).into()),
                (
                    "trustIndicatorDetail",
                    trust_indicator_detail(&state).into(),
                ),
                ("errorBannerVisible", error_banner_visible(&state).into()),
                ("errorBannerText", error_banner_text(&state).into()),
                (
                    "invalidEntryBadgeVisible",
                    invalid_entry_badge_visible(&state).into(),
                ),
                (
                    "invalidEntryBadgeText",
                    invalid_entry_badge_text(&state).into(),
                ),
                ("loadProgressVisible", load_progress_visible(&state).into()),
                (
                    "loadProgressFraction",
                    load_progress_fraction(&state).into(),
                ),
                ("loadProgressHint", load_progress_hint(&state).into()),
                (
                    "trustPinActionVisible",
                    trust_pin_action_visible(&state).into(),
                ),
                ("trustPinActionLabel", trust_pin_action_label(&state).into()),
                ("trustPinDetail", trust_pin_detail(&state).into()),
            ];
            for (field, expected) in derived {
                assert_eq!(
                    doc[field], expected,
                    "`{field}` must be the core's own derivation for {state:?}: {json}"
                );
            }
        }
    }

    #[test]
    fn the_chrome_json_keeps_the_facts_in_the_existing_shared_wire_vocabulary() {
        // The carrier ADDS the derived strings; it does not re-mean or re-spell
        // the FACTS the two mobile edges already decode. Each enum fact keeps the
        // one wire spelling the rest of the system speaks: `LoadStep::wire_name`,
        // `FailureKind::wire_name` and `debug::trust_posture_wire_name` (the very
        // names the debug view's Network tab uses, ADR-0006), so no second
        // spelling is minted for mobile.
        for state in every_chrome_state_shape() {
            let json = chrome_json(&state);
            let doc: serde_json::Value =
                serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON {json}: {e}"));
            assert_eq!(doc["url"], serde_json::json!(state.url_text));
            assert_eq!(doc["loading"], serde_json::json!(state.is_loading()));
            assert_eq!(
                doc["loadStep"],
                serde_json::json!(state.load_step().wire_name())
            );
            assert_eq!(
                doc["trustPosture"],
                serde_json::json!(crate::debug::trust_posture_wire_name(state.trust_posture))
            );
            assert_eq!(doc["canGoBack"], serde_json::json!(state.can_go_back));
            assert_eq!(doc["canGoForward"], serde_json::json!(state.can_go_forward));
            assert_eq!(doc["error"], serde_json::json!(state.last_error));
            assert_eq!(
                doc["failureKind"],
                serde_json::json!(state.failure_kind().map(FailureKind::wire_name))
            );
            assert_eq!(
                doc["retryable"],
                serde_json::json!(state.failure_is_retryable())
            );
            assert_eq!(doc["invalidEntry"], serde_json::json!(state.invalid_entry));
        }
    }

    #[test]
    fn the_chrome_json_document_is_exactly_the_facts_plus_the_derived_fields() {
        // The wire SHAPE, pinned whole: an idle default chrome encodes to exactly
        // this document. Two things this catches that the per-field tests above do
        // not: a field QUIETLY DROPPED (a mobile edge would then paint a default),
        // and a field quietly ADDED under a second spelling of something the
        // vocabulary already names. Asserted on the parsed value rather than the
        // string, because JSON object order is not a contract (both edges decode
        // with a real parser) while the key set and every value are.
        let doc: serde_json::Value = serde_json::from_str(&chrome_json(&ChromeState::default()))
            .expect("the chrome JSON is valid JSON");
        assert_eq!(
            doc,
            serde_json::json!({
                // The FACTS (unchanged from the pre-collapse wire form).
                "url": "",
                "loadState": "idle",
                "loading": false,
                "loadStep": "idle",
                "canGoBack": false,
                "canGoForward": false,
                "trustPosture": "unverified-origin",
                "error": null,
                "failureKind": null,
                "retryable": false,
                "invalidEntry": null,
                // The TOFU mutable-name axis (task `ipns-tofu-pin-and-warn-on-change`):
                // absent on a page that is not a name-resolved load at all.
                "mutableName": null,
                "mutableNameCid": null,
                "blessedCid": null,
                "nameChanged": false,
                // The DERIVED strings, named after the core rules that produce them.
                "statusLine": "idle",
                "trustIndicator": "⚠ unverified origin",
                "trustIndicatorDetail": trust_indicator_detail(&ChromeState::default()),
                "errorBannerVisible": false,
                "errorBannerText": "",
                "invalidEntryBadgeVisible": false,
                "invalidEntryBadgeText": "",
                "loadProgressVisible": false,
                "loadProgressFraction": 0.0,
                "loadProgressHint": "",
                "trustPinActionVisible": false,
                "trustPinActionLabel": "",
                "trustPinDetail": "",
            })
        );
    }

    #[test]
    fn the_chrome_json_stays_valid_when_a_reason_or_a_url_carries_json_punctuation() {
        // A URL or a protocol-named reason with a quote/backslash/newline must not
        // break the document, since it rides into the DERIVED strings too (the banner
        // text embeds the reason verbatim), so the escaping has to survive both
        // the fact and its derivation.
        let state = ChromeState {
            url_text: "https://x/\"a\\b".into(),
            load_state: LoadState::Failed,
            last_error: Some("bad \"quote\"\nline".into()),
            ..ChromeState::default()
        };
        let json = chrome_json(&state);
        let doc: serde_json::Value =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON {json}: {e}"));
        assert_eq!(doc["url"], serde_json::json!("https://x/\"a\\b"));
        assert_eq!(doc["error"], serde_json::json!("bad \"quote\"\nline"));
        assert_eq!(
            doc["errorBannerText"],
            serde_json::json!(error_banner_text(&state))
        );
    }

    #[test]
    fn a_transient_timeout_banner_is_distinct_and_retryable_while_a_hard_fail_keeps_its_reason() {
        // Acceptance: a transient/timeout failure is surfaced DISTINCTLY from a
        // hard failure, with an obvious retry affordance; a hard failure keeps its
        // prominent protocol-named reason. Both banners carry the honest reason
        // verbatim; only the framing + the CSS class differ.
        let transient = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("transport error: timeout: global".into()),
            ..ChromeState::default()
        };
        assert!(error_banner_visible(&transient));
        let text = error_banner_text(&transient);
        assert!(
            text.to_lowercase().contains("retry"),
            "a transient failure offers a retry affordance: {text}"
        );
        assert!(
            text.contains("transport error: timeout: global"),
            "the honest reason is kept: {text}"
        );
        assert_eq!(error_banner_css_class(&transient), "error-banner-transient");

        // A hard failure: the prominent "failed to load" wording, its
        // protocol-named reason, and NO retry affordance.
        let hard = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        let hard_text = error_banner_text(&hard);
        assert!(
            hard_text.contains("failed to load"),
            "a hard failure reads as a load failure: {hard_text}"
        );
        assert!(hard_text.contains("points to Swarm, not supported"));
        assert!(
            !hard_text.to_lowercase().contains("retry"),
            "a hard failure offers no retry: {hard_text}"
        );
        assert_eq!(error_banner_css_class(&hard), "error-banner");

        // A verification failure is HARD even though it is a failure of a fetched
        // record: retrying will not make it verify.
        let verify_fail = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("IPNS record did not verify: bad signature".into()),
            ..ChromeState::default()
        };
        assert_eq!(error_banner_css_class(&verify_fail), "error-banner");
        assert!(!error_banner_text(&verify_fail)
            .to_lowercase()
            .contains("retry"));
    }

    #[test]
    fn status_line_prefers_a_failure_then_loading_then_idle() {
        // The chrome's status line is a pure function of ChromeState: a surfaced
        // failure wins, otherwise loading vs idle follows the load state.
        let idle = ChromeState::default();
        assert_eq!(status_line(&idle), "idle");

        let loading = ChromeState {
            load_state: LoadState::Started,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&loading), "loading…");

        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert_eq!(status_line(&failed), "failed: name not resolved");
    }

    #[test]
    fn an_invalid_entry_shows_the_badge_distinct_from_a_load_error() {
        // Field finding D: an INVALID URL-bar entry (a scheme-less garbage entry
        // that did not navigate) shows the small "invalid URL" badge, distinct
        // from a load-error banner. A valid/idle chrome hides it; a LOAD failure
        // (`last_error`) is NOT the invalid badge (the two axes are orthogonal).
        let idle = ChromeState::default();
        assert!(!invalid_entry_badge_visible(&idle));
        assert_eq!(invalid_entry_badge_text(&idle), "");

        let load_failure = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert!(
            !invalid_entry_badge_visible(&load_failure),
            "a load failure is not the invalid-entry badge"
        );

        let invalid = ChromeState {
            url_text: "not a url".into(),
            invalid_entry: Some("not a url".into()),
            ..ChromeState::default()
        };
        assert!(invalid_entry_badge_visible(&invalid));
        assert!(invalid_entry_badge_text(&invalid).contains("invalid URL"));
        // The invalid-entry badge is orthogonal to a load error: it carries no
        // `last_error`, so the error banner stays hidden.
        assert!(!error_banner_visible(&invalid));
    }

    #[test]
    fn a_failed_load_raises_a_prominent_error_banner_with_the_accurate_protocol_named_reason() {
        // Acceptance (the fail-closed honesty fix): a failed load raises a
        // PROMINENT in-view error banner the user cannot miss, carrying the
        // accurate, protocol-named reason (the resolver/decoder taxonomy verbatim),
        // NOT only the subtle footer status line the human missed. It is hidden on
        // an idle or an in-flight load, and only appears on a failure.
        let idle = ChromeState::default();
        assert!(
            !error_banner_visible(&idle),
            "no banner when nothing has failed"
        );
        assert_eq!(error_banner_text(&idle), "");

        let loading = ChromeState {
            load_state: LoadState::Started,
            ..ChromeState::default()
        };
        assert!(
            !error_banner_visible(&loading),
            "no banner while a load is in flight"
        );

        // A real IPNS failure (the ronan.eth taxonomy): the banner is VISIBLE and
        // carries the protocol-named reason, not a generic "failed".
        let ipns_failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some(
                "IPNS record did not verify: dag-cbor data does not match the protobuf fields"
                    .into(),
            ),
            ..ChromeState::default()
        };
        assert!(
            error_banner_visible(&ipns_failed),
            "a failed load raises the prominent banner"
        );
        let text = error_banner_text(&ipns_failed);
        assert!(
            text.contains("IPNS record did not verify"),
            "the banner carries the accurate protocol-named reason: {text}"
        );
        assert!(
            text.contains("failed to load"),
            "the banner reads as a load failure the user cannot miss: {text}"
        );

        // An unsupported-protocol failure likewise surfaces its named reason.
        let unsupported = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        assert!(error_banner_visible(&unsupported));
        assert!(error_banner_text(&unsupported).contains("points to Swarm"));
    }

    #[test]
    fn trust_indicator_shows_a_neutral_loading_state_that_hides_the_posture_while_loading() {
        // Acceptance (the trust-honesty fix): while a load is in flight the trust
        // indicator is a NEUTRAL loading state (no trust claim), NOT the
        // carried-over posture of the previous page. The display rule is
        // loading-wins: even a load whose backend posture still reads
        // content-verified (mid-transition) must show the loading badge, so the
        // indicator never asserts a trust level for a page that is not yet shown.
        for posture in [
            TrustPosture::UnverifiedOrigin,
            TrustPosture::ContentVerified,
            TrustPosture::NameViaTrustedRpc,
            TrustPosture::MutableName,
        ] {
            let loading = ChromeState {
                load_state: LoadState::Started,
                trust_posture: posture,
                ..ChromeState::default()
            };
            assert_eq!(
                trust_indicator(&loading),
                "⋯ loading…",
                "while loading, the indicator is a neutral loading state, not the {posture:?} posture"
            );
            // The loading badge makes NO trust claim: it never reads "verified"
            // and never asserts the origin is (un)verified.
            assert!(!trust_indicator(&loading)
                .to_lowercase()
                .contains("verified"));
            assert_eq!(trust_indicator_css_class(&loading), "trust-loading");
            // The tooltip is honest that werust is not yet asserting a trust level.
            assert!(trust_indicator_detail(&loading)
                .to_lowercase()
                .contains("loading"));
            assert!(!trust_indicator_detail(&loading)
                .to_lowercase()
                .contains("verified"));
        }

        // A Committed load is still in flight, so it is still the neutral state.
        let committed = ChromeState {
            load_state: LoadState::Committed,
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&committed), "⋯ loading…");

        // Once the load SETTLES (Finished), the real posture appears — the loading
        // state does not swallow the settled badge.
        let settled = ChromeState {
            load_state: LoadState::Finished,
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&settled), "✓ verified");

        // A FAILED load is not "loading": it shows its (unverified) posture, not the
        // spinner — a failed load must never read as a stale success.
        let failed = ChromeState {
            load_state: LoadState::Failed,
            trust_posture: TrustPosture::UnverifiedOrigin,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&failed), "⚠ unverified origin");
    }

    #[test]
    fn trust_indicator_distinguishes_verified_from_unverified_and_is_a_pure_fn_of_posture() {
        // Acceptance: the chrome's trust indicator shows a clear, distinct state
        // for a content-verified load vs an unverified served-origin load, and it
        // is driven by the posture the seam reports (the actual load path), not by
        // any URL string. The two labels are visibly different and legible.
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };

        assert_eq!(trust_indicator(&served), "⚠ unverified origin");
        assert_eq!(trust_indicator(&verified), "✓ verified");
        assert_ne!(
            trust_indicator(&served),
            trust_indicator(&verified),
            "the two trust states are visually distinct"
        );

        // The detail/tooltip likewise distinguishes the two and names the reason.
        assert!(trust_indicator_detail(&verified).contains("content-verified"));
        assert!(trust_indicator_detail(&served).contains("not"));

        // The default (nothing loaded yet) is the untrusted posture: werust does
        // not claim verification it has not proven.
        assert_eq!(
            trust_indicator(&ChromeState::default()),
            "⚠ unverified origin"
        );
    }

    #[test]
    fn trust_indicator_shows_a_distinct_name_via_trusted_rpc_badge_never_labelled_verified() {
        // Acceptance: an ENS-resolved Phase-1 page (bytes verified, name resolved
        // over a trusted RPC) renders as its OWN legible, visually-distinct badge
        // — distinct from BOTH the verified and the unverified-origin badges — and
        // it is NEVER surfaced as "verified" / "name-verified".
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        let name_via_rpc = ChromeState {
            trust_posture: TrustPosture::NameViaTrustedRpc,
            ..ChromeState::default()
        };

        let label = trust_indicator(&name_via_rpc);
        assert_eq!(label, "◈ name via trusted RPC");
        // Distinct from the other two badges.
        assert_ne!(label, trust_indicator(&verified));
        assert_ne!(label, trust_indicator(&served));
        // NEVER labelled "verified" / "name-verified": Phase 1 makes no such claim.
        assert!(
            !label.to_lowercase().contains("verified"),
            "the name-via-trusted-RPC badge must never read as verified: {label}"
        );
        assert!(!trust_indicator_detail(&name_via_rpc)
            .to_lowercase()
            .contains("name-verified"));
        // The tooltip is honest that the name came from a trusted RPC.
        assert!(trust_indicator_detail(&name_via_rpc).contains("TRUSTED RPC"));

        // The badge carries its own CSS class, distinct from the other two, so the
        // three states are visually distinct.
        assert_eq!(
            trust_indicator_css_class(&name_via_rpc),
            "trust-name-trusted-rpc"
        );
        assert_eq!(trust_indicator_css_class(&verified), "trust-verified");
        assert_eq!(trust_indicator_css_class(&served), "trust-unverified");
    }

    #[test]
    fn trust_indicator_shows_a_distinct_mutable_name_badge_never_labelled_verified() {
        // Acceptance: a client-verified IPNS page (bytes verified, name mutable)
        // renders as its OWN legible, visually-distinct badge — distinct from the
        // verified, name-via-trusted-RPC, and unverified badges — and it is NEVER
        // surfaced as "verified".
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        let name_via_rpc = ChromeState {
            trust_posture: TrustPosture::NameViaTrustedRpc,
            ..ChromeState::default()
        };
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let mutable = ChromeState {
            trust_posture: TrustPosture::MutableName,
            ..ChromeState::default()
        };

        let label = trust_indicator(&mutable);
        assert_eq!(label, "◇ content verified, mutable name");
        // Distinct from the other three badges.
        assert_ne!(label, trust_indicator(&verified));
        assert_ne!(label, trust_indicator(&name_via_rpc));
        assert_ne!(label, trust_indicator(&served));
        // Its "verified" only ever appears as part of "content verified", never as
        // a bare immutability claim; the badge is honest that the NAME is mutable.
        assert!(
            label.contains("mutable name"),
            "the mutable-name badge must say the name is mutable: {label}"
        );
        // The tooltip is honest that the name is mutable / controller-repointable,
        // and makes NO immutability claim (it may say it makes "no immutability
        // claim", but must not assert the name IS immutable).
        let detail = trust_indicator_detail(&mutable);
        assert!(detail.contains("MUTABLE"));
        assert!(
            detail.contains("can repoint"),
            "the tooltip is honest the controller can repoint the name: {detail}"
        );

        // Its own CSS class, distinct from the other three.
        assert_eq!(trust_indicator_css_class(&mutable), "trust-mutable-name");
        assert_ne!(
            trust_indicator_css_class(&mutable),
            trust_indicator_css_class(&name_via_rpc)
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
