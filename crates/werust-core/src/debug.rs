//! The bounded **debug capture store**: the recent CONSOLE log entries and
//! NETWORK request entries werust's in-app debug menu shows, held in the
//! toolkit-free core and exposed over the SAME chrome/FFI surface every OS edge
//! already reads.
//!
//! This is the foundation of the in-app debug menu (spec
//! `in-app-debug-menu-console-and-network`, task
//! `debug-capture-store-console-and-network-in-core`): ONE shared store that the
//! per-platform CAPTURE POINTS (desktop `console-message` / resource-load
//! signals, Android `onConsoleMessage` / `shouldInterceptRequest`, iOS an
//! injected console user-script + the reachable network points) push into, and
//! that each platform's tabbed debug VIEW renders. Neither the capture points nor
//! the views live here: this module is the store, the entry types, and the wire
//! form, so the whole thing is unit-testable with no webview, no GTK loop, and no
//! network, mirroring the [`ipfs`](crate::ipfs) / [`retrieval`](crate::retrieval)
//! splits.
//!
//! # Bounded by construction
//!
//! A browsing session is unbounded; a debug store must not be. Both buffers are
//! RING buffers capped at [`MAX_CONSOLE_ENTRIES`] / [`MAX_NETWORK_ENTRIES`], with
//! the OLDEST entry evicted on overflow (the same bounded-state discipline the
//! retrieval budget and `ens_pages` follow), and each entry's text fields are
//! truncated to [`MAX_TEXT_CHARS`] so one pathological `console.log` of a whole
//! document cannot blow the bound sideways.
//!
//! # Shared like a sink, not owned by one thread
//!
//! [`DebugCapture`] is an `Arc<Mutex<_>>` handle (the same idiom as
//! [`crate::ipfs::RedirectSink`]): the capture point owns a clone and may run OFF
//! the UI thread (`docs/adr/0008`: the Android `shouldInterceptRequest` worker
//! thread, the desktop scheme handler), the shell owns the other clone, and both
//! see the SAME entries. Capture is READ-ONLY observation: pushing an entry never
//! touches the load path, the verification, or the chrome's own trust posture.
//!
//! # The honest per-request trust posture (ADR-0006, NOT re-meaned)
//!
//! A [`NetworkEntry`] carries werust's existing [`TrustPosture`] for THAT ONE
//! request, with the SAME meaning and the SAME wire names the chrome trust
//! indicator uses: an `ipfs://` request whose bytes hash-verified is
//! [`ContentVerified`](TrustPosture::ContentVerified); an `https://` subresource
//! is [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin). No new trust label is
//! invented for the debug view, and nothing here can UPGRADE a posture:
//! [`request_trust_posture`] returns the verified posture only when the caller
//! reports that the bytes actually verified on the content-addressed path, so a
//! URL that merely LOOKS content-addressed is never labelled verified.
//!
//! # Where the edges read it (the recorded FFI shape)
//!
//! The store is exposed as its OWN [`debug_json`] document behind a DEDICATED
//! accessor (`debug_json()` on each mobile `CoreSession`, alongside
//! `chrome_json()`), NOT as an additive section on the chrome JSON. The chrome
//! JSON is polled on every chrome refresh to paint the URL bar, so folding a
//! few-hundred-entry store into it would re-encode the whole capture on every
//! keystroke-sized refresh; the debug document is instead read only while the
//! debug view is open. Both were additive; the split keeps the chrome JSON lean
//! and leaves every existing chrome reader byte-for-byte unaffected.
//!
//! # The recorded decisions
//!
//! That FFI choice and the other judgement calls this module bakes in (the
//! capture flag gates NETWORK capture only; the per-entry text bound; the
//! conservative per-request posture; why no platform-capability-matrix row lands
//! yet; caller-supplied timestamps; the shared-handle shape) are recorded, with
//! the alternatives considered and what each one touches, in
//! `docs/spikes/debug-capture-store-console-and-network-in-core/DECISIONS.md`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use renderer::TrustPosture;
use serde_json::{json, Value};

/// The maximum number of CONSOLE entries kept; the oldest is evicted past this.
///
/// A few hundred entries is enough to see what a page just did (the debug view is
/// a recent-activity surface, not a log file) while keeping the store's memory
/// trivially bounded for a long session.
pub const MAX_CONSOLE_ENTRIES: usize = 300;

/// The maximum number of NETWORK entries kept; the oldest is evicted past this.
///
/// Deliberately the same order as [`MAX_CONSOLE_ENTRIES`]: a page load is tens of
/// requests, so a few hundred covers the recent loads a debug view is asked
/// about.
pub const MAX_NETWORK_ENTRIES: usize = 300;

/// The maximum number of CHARACTERS kept of any captured text field (a console
/// message, a source or request URL, a MIME type).
///
/// The entry COUNT alone does not bound the store: one `console.log` of a whole
/// serialized document would be megabytes in a single entry. Truncating each text
/// field keeps the worst case proportional to the cap, and a debug view shows the
/// head of a long message anyway. Counted in `char`s (never bytes) so truncation
/// can never split a UTF-8 sequence.
pub const MAX_TEXT_CHARS: usize = 2_000;

/// The severity of a captured console entry: the `console.*` levels every
/// platform's console hook reports.
///
/// Kept as werust's OWN small enum (not a platform type) so the three capture
/// points map their native level onto ONE vocabulary the debug view renders from,
/// exactly as [`TrustPosture`] is the one trust vocabulary. An unrecognised
/// platform level maps to [`Log`](ConsoleLevel::Log) rather than inventing a
/// level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleLevel {
    /// `console.log` (and any level a platform does not distinguish). The DEFAULT.
    #[default]
    Log,
    /// `console.info`.
    Info,
    /// `console.warn`.
    Warn,
    /// `console.error`.
    Error,
    /// `console.debug`.
    Debug,
}

impl ConsoleLevel {
    /// The stable, lower-case wire name for the debug JSON, so every edge paints
    /// the SAME level from the SAME fact (mirroring the trust-posture and
    /// load-step wire names).
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            ConsoleLevel::Log => "log",
            ConsoleLevel::Info => "info",
            ConsoleLevel::Warn => "warn",
            ConsoleLevel::Error => "error",
            ConsoleLevel::Debug => "debug",
        }
    }
}

/// One captured CONSOLE entry: what the page logged, at what level, from where.
///
/// Built with [`new`](ConsoleEntry::new) plus the `with_*` setters, so a capture
/// point fills only the fields its platform actually reports (iOS's injected
/// user-script has no line number for some call sites; a native hook does) and an
/// absent field stays honestly absent rather than a fabricated zero.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConsoleEntry {
    /// The severity the page logged at.
    pub level: ConsoleLevel,
    /// The logged message, already flattened to text by the platform hook and
    /// truncated to [`MAX_TEXT_CHARS`].
    pub message: String,
    /// The source URL the log came from (a script URL / the document), empty when
    /// the platform reports none.
    pub source: String,
    /// The 1-based source line, or [`None`] when the platform reports none.
    pub line: Option<u32>,
    /// When it was captured, as milliseconds since the Unix epoch (the capture
    /// point supplies it; `0` when unknown). Kept as a plain number so the core
    /// binds no clock/time crate and a test can pin it.
    pub timestamp: u64,
}

impl ConsoleEntry {
    /// A console entry at `level` with `message` (truncated to
    /// [`MAX_TEXT_CHARS`]); every other field absent until set.
    #[must_use]
    pub fn new(level: ConsoleLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: truncate(message.into()),
            source: String::new(),
            line: None,
            timestamp: 0,
        }
    }

    /// Set the source URL (truncated to [`MAX_TEXT_CHARS`]).
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = truncate(source.into());
        self
    }

    /// Set the 1-based source line.
    #[must_use]
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Set the capture timestamp (milliseconds since the Unix epoch).
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// One captured NETWORK request: what was requested, how it came back, and the
/// HONEST trust posture of THAT request.
///
/// The [`trust`](NetworkEntry::trust) field is werust's existing [`TrustPosture`]
/// with its existing meaning (ADR-0006) applied per-request rather than
/// per-page: it says what this ONE request earned, so the Network tab can never
/// imply a request was trusted that was not. It defaults to
/// [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin) (the same fail-closed
/// default the seam uses), and a capture point sets it from
/// [`request_trust_posture`] (or from the load's own posture) rather than
/// guessing from the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkEntry {
    /// The HTTP-style method (`GET`, `POST`, …), as the platform reported it.
    pub method: String,
    /// The requested URL, truncated to [`MAX_TEXT_CHARS`].
    pub url: String,
    /// The response status, or [`None`] when the request has no status (a custom
    /// scheme answered without one, or the request failed before a response).
    pub status: Option<u16>,
    /// The response MIME type, empty when unknown.
    pub mime: String,
    /// The response body size in bytes, or [`None`] when unknown.
    pub size: Option<u64>,
    /// Whether the response was served from a cache.
    pub from_cache: bool,
    /// The URL's scheme in lower case (`ipfs`, `https`, `werust`, …), derived by
    /// [`new`](NetworkEntry::new) from the URL so the Network tab can group/filter
    /// by scheme without re-parsing. Empty for a URL with no scheme.
    pub scheme: String,
    /// The honest [`TrustPosture`] of THIS request (ADR-0006), never a new label.
    pub trust: TrustPosture,
    /// When it was captured, as milliseconds since the Unix epoch (`0` when
    /// unknown).
    pub timestamp: u64,
    /// How long the request took, in milliseconds, or [`None`] when unknown.
    pub duration: Option<u64>,
}

impl NetworkEntry {
    /// A network entry for `method` `url`, with its [`scheme`](NetworkEntry::scheme)
    /// derived from the URL and every optional field absent until set. The trust
    /// posture starts at the fail-closed
    /// [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin).
    #[must_use]
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        let url = truncate(url.into());
        Self {
            method: truncate(method.into()),
            scheme: scheme_of(&url),
            url,
            status: None,
            mime: String::new(),
            size: None,
            from_cache: false,
            trust: TrustPosture::default(),
            timestamp: 0,
            duration: None,
        }
    }

    /// Set the response status.
    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// Set the response MIME type (truncated to [`MAX_TEXT_CHARS`]).
    #[must_use]
    pub fn with_mime(mut self, mime: impl Into<String>) -> Self {
        self.mime = truncate(mime.into());
        self
    }

    /// Set the response body size in bytes.
    #[must_use]
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Set whether the response came from a cache.
    #[must_use]
    pub fn from_cache(mut self, from_cache: bool) -> Self {
        self.from_cache = from_cache;
        self
    }

    /// Set this request's honest [`TrustPosture`] (ADR-0006). A capture point
    /// derives it from what the request ACTUALLY did (see
    /// [`request_trust_posture`]), never from the URL alone.
    #[must_use]
    pub fn with_trust(mut self, trust: TrustPosture) -> Self {
        self.trust = trust;
        self
    }

    /// Set the request duration in milliseconds.
    #[must_use]
    pub fn with_duration(mut self, duration: u64) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set the capture timestamp (milliseconds since the Unix epoch).
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// The honest [`TrustPosture`] for ONE captured request, given its `scheme` and
/// whether its bytes ACTUALLY verified on the content-addressed path.
///
/// This is the per-request twin of [`TrustPosture::after_verify`] (the per-page
/// rule), and it exists so the three capture points derive the posture from ONE
/// place instead of each inventing a mapping. It is deliberately conservative:
///
/// * a content-addressed (`ipfs`) request whose bytes verified is
///   [`ContentVerified`](TrustPosture::ContentVerified);
/// * EVERYTHING else (an `https://` subresource, an `ipfs://` request that did
///   NOT verify (a hash mismatch, a failed retrieval), a `werust://` internal
///   page) is [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin).
///
/// So a URL that merely LOOKS content-addressed can never be labelled verified,
/// which is the same rule the chrome indicator obeys. The name-trust axes
/// ([`NameViaTrustedRpc`](TrustPosture::NameViaTrustedRpc) /
/// [`MutableName`](TrustPosture::MutableName)) are properties of the PAGE's name
/// resolution, not of an individual subresource request, so they are set by the
/// caller ([`NetworkEntry::with_trust`]) for the main-document entry rather than
/// derived here.
#[must_use]
pub fn request_trust_posture(scheme: &str, verified: bool) -> TrustPosture {
    if verified && scheme.eq_ignore_ascii_case(crate::ipfs::IPFS_SCHEME) {
        TrustPosture::ContentVerified
    } else {
        TrustPosture::UnverifiedOrigin
    }
}

/// The bounded console + network capture store the shell owns and the capture
/// points push into.
///
/// A cheap `Arc` handle: cloning shares ONE store (the capture point's clone and
/// the shell's clone are the same store), which is the point: see the module
/// docs. Every method takes `&self` so a `Send` capture closure can own a clone
/// without needing `&mut`, exactly like [`crate::ipfs::RedirectSink`].
#[derive(Debug, Clone, Default)]
pub struct DebugCapture {
    inner: Arc<Mutex<CaptureInner>>,
}

/// The interior of a [`DebugCapture`]: the two ring buffers plus the capture
/// gate.
#[derive(Debug)]
struct CaptureInner {
    console: VecDeque<ConsoleEntry>,
    network: VecDeque<NetworkEntry>,
    /// Whether NETWORK capture is on. Phase 1 is ALWAYS-on (this defaults to
    /// `true`), and the flag exists so the Phase-2 debug-menu toggle
    /// (`debug-network-capture-toggle-config`) is a small addition (a setting
    /// that flips this) rather than a rework of the store.
    network_capture_enabled: bool,
}

impl Default for CaptureInner {
    fn default() -> Self {
        Self {
            console: VecDeque::new(),
            network: VecDeque::new(),
            // Phase 1 captures network ALWAYS (spec
            // `in-app-debug-menu-console-and-network`): the default is on.
            network_capture_enabled: true,
        }
    }
}

impl DebugCapture {
    /// A fresh, empty store with network capture ENABLED (the Phase-1 default).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture one CONSOLE entry, evicting the oldest once
    /// [`MAX_CONSOLE_ENTRIES`] is reached.
    ///
    /// Console capture is not gated by the network-capture flag: the Phase-2
    /// toggle the spec names is about NETWORK capture (the request stream), and
    /// re-meaning it as an everything-switch would silently change what a later
    /// task's setting does.
    pub fn push_console(&self, entry: ConsoleEntry) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.console.len() >= MAX_CONSOLE_ENTRIES {
            inner.console.pop_front();
        }
        inner.console.push_back(entry);
    }

    /// Capture one NETWORK entry, evicting the oldest once
    /// [`MAX_NETWORK_ENTRIES`] is reached. A no-op while network capture is
    /// disabled ([`set_network_capture_enabled`](DebugCapture::set_network_capture_enabled)).
    pub fn push_network(&self, entry: NetworkEntry) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if !inner.network_capture_enabled {
            return;
        }
        if inner.network.len() >= MAX_NETWORK_ENTRIES {
            inner.network.pop_front();
        }
        inner.network.push_back(entry);
    }

    /// The captured CONSOLE entries, oldest first (a snapshot: the store keeps
    /// capturing).
    #[must_use]
    pub fn console(&self) -> Vec<ConsoleEntry> {
        match self.inner.lock() {
            Ok(inner) => inner.console.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The captured NETWORK entries, oldest first (a snapshot).
    #[must_use]
    pub fn network(&self) -> Vec<NetworkEntry> {
        match self.inner.lock() {
            Ok(inner) => inner.network.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Empty BOTH buffers: the debug view's Clear action. Leaves the
    /// capture-enabled flag untouched (clearing is not turning capture off).
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.console.clear();
            inner.network.clear();
        }
    }

    /// Whether NETWORK capture is currently on (Phase-1 default: `true`).
    #[must_use]
    pub fn network_capture_enabled(&self) -> bool {
        match self.inner.lock() {
            Ok(inner) => inner.network_capture_enabled,
            // A poisoned lock reports the default rather than claiming capture is
            // off (which a debug view would render as a misleading "disabled").
            Err(_) => true,
        }
    }

    /// Turn NETWORK capture on/off. The seam the Phase-2 debug-menu toggle
    /// (`debug-network-capture-toggle-config`) drives; nothing calls it in Phase 1,
    /// where capture is always on.
    pub fn set_network_capture_enabled(&self, enabled: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.network_capture_enabled = enabled;
        }
    }
}

/// Encode a [`DebugCapture`] as the debug JSON document every edge's debug view
/// renders from.
///
/// The shape (stable, asserted by the tests):
///
/// ```json
/// {
///   "console": [{"level":"warn","message":"…","source":"…","line":42,"ts":1700000000123}],
///   "network": [{"method":"GET","url":"…","status":200,"mime":"…","size":1234,
///                "fromCache":false,"scheme":"ipfs","trust":"content-verified",
///                "ts":1700000000456,"duration":17}],
///   "networkCaptureEnabled": true
/// }
/// ```
///
/// Both arrays are oldest-first. An UNKNOWN numeric field (`line`, `status`,
/// `size`, `duration`) is `null`, never a fabricated `0`: the debug view must be
/// able to show "unknown" honestly. `trust` uses the SAME lower-kebab posture
/// names the chrome JSON uses ([`trust_posture_wire_name`]), so the Network tab
/// speaks the trust indicator's exact vocabulary (ADR-0006).
///
/// This is a DEDICATED document, deliberately NOT a section of the chrome JSON:
/// the chrome JSON is re-encoded on every chrome refresh, while this is read only
/// while the debug view is open (see the module docs).
#[must_use]
pub fn debug_json(capture: &DebugCapture) -> String {
    let console: Vec<Value> = capture
        .console()
        .into_iter()
        .map(|e| {
            json!({
                "level": e.level.wire_name(),
                "message": e.message,
                "source": e.source,
                "line": e.line,
                "ts": e.timestamp,
            })
        })
        .collect();
    let network: Vec<Value> = capture
        .network()
        .into_iter()
        .map(|e| {
            json!({
                "method": e.method,
                "url": e.url,
                "status": e.status,
                "mime": e.mime,
                "size": e.size,
                "fromCache": e.from_cache,
                "scheme": e.scheme,
                "trust": trust_posture_wire_name(e.trust),
                "ts": e.timestamp,
                "duration": e.duration,
            })
        })
        .collect();
    json!({
        "console": console,
        "network": network,
        "networkCaptureEnabled": capture.network_capture_enabled(),
    })
    .to_string()
}

/// The stable, lower-kebab wire name of a [`TrustPosture`]: the SAME names the
/// mobile chrome JSON (`ffi_json`) and the desktop trust indicator use.
///
/// Lifted here so the debug view's Network tab reuses the trust indicator's
/// vocabulary EXACTLY rather than minting a second set of labels for the same
/// postures (spec: "reuse the trust-indicator posture words exactly (ADR-0006);
/// do not invent a new label").
#[must_use]
pub fn trust_posture_wire_name(posture: TrustPosture) -> &'static str {
    match posture {
        TrustPosture::UnverifiedOrigin => "unverified-origin",
        TrustPosture::ContentVerified => "content-verified",
        TrustPosture::NameViaTrustedRpc => "name-via-trusted-rpc",
        TrustPosture::MutableName => "mutable-name",
    }
}

/// The lower-case scheme of `url` (`ipfs://cid` -> `ipfs`, `about:blank` ->
/// `about`), or empty when the URL carries none.
///
/// Deliberately a cheap prefix read, not a URL parse: the capture store only
/// needs the scheme to LABEL an entry, and a capture point must stay cheap (it
/// runs on the platform's request/console callback). A scheme is
/// `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` before the first `:` (RFC 3986),
/// so anything else yields no scheme rather than a wrong one.
fn scheme_of(url: &str) -> String {
    let Some(colon) = url.find(':') else {
        return String::new();
    };
    let candidate = &url[..colon];
    let mut chars = candidate.chars();
    let valid = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if valid {
        candidate.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// Truncate a captured text field to [`MAX_TEXT_CHARS`] CHARACTERS, so one huge
/// message cannot blow the store's bound sideways. Counted in `char`s so the cut
/// can never split a UTF-8 sequence.
fn truncate(mut s: String) -> String {
    if s.chars().count() <= MAX_TEXT_CHARS {
        return s;
    }
    let end = s
        .char_indices()
        .nth(MAX_TEXT_CHARS)
        .map_or(s.len(), |(i, _)| i);
    s.truncate(end);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::TrustPosture;

    fn console(message: &str) -> ConsoleEntry {
        ConsoleEntry::new(ConsoleLevel::Log, message).with_timestamp(7)
    }

    fn network(url: &str) -> NetworkEntry {
        NetworkEntry::new("GET", url).with_timestamp(7)
    }

    #[test]
    fn pushing_past_the_cap_evicts_the_oldest_console_entry() {
        let capture = DebugCapture::new();
        for i in 0..(MAX_CONSOLE_ENTRIES + 3) {
            capture.push_console(console(&format!("m{i}")));
        }
        let entries = capture.console();
        assert_eq!(entries.len(), MAX_CONSOLE_ENTRIES);
        assert_eq!(entries.first().unwrap().message, "m3");
        assert_eq!(
            entries.last().unwrap().message,
            format!("m{}", MAX_CONSOLE_ENTRIES + 2)
        );
    }

    #[test]
    fn pushing_past_the_cap_evicts_the_oldest_network_entry() {
        let capture = DebugCapture::new();
        for i in 0..(MAX_NETWORK_ENTRIES + 3) {
            capture.push_network(network(&format!("https://x/{i}")));
        }
        let entries = capture.network();
        assert_eq!(entries.len(), MAX_NETWORK_ENTRIES);
        assert_eq!(entries.first().unwrap().url, "https://x/3");
    }

    #[test]
    fn clear_empties_both_ring_buffers() {
        let capture = DebugCapture::new();
        capture.push_console(console("hello"));
        capture.push_network(network("ipfs://cid/x"));
        capture.clear();
        assert!(capture.console().is_empty());
        assert!(capture.network().is_empty());
    }

    #[test]
    fn network_capture_is_enabled_by_default_and_the_flag_gates_the_push() {
        let capture = DebugCapture::new();
        assert!(capture.network_capture_enabled());
        capture.push_network(network("https://x/1"));
        assert_eq!(capture.network().len(), 1);

        capture.set_network_capture_enabled(false);
        capture.push_network(network("https://x/2"));
        assert_eq!(capture.network().len(), 1, "a disabled capture drops");
        capture.push_console(console("still captured"));
        assert_eq!(
            capture.console().len(),
            1,
            "the flag gates NETWORK capture only"
        );
    }

    #[test]
    fn an_ipfs_request_that_verified_is_content_verified_and_an_https_one_is_not() {
        assert_eq!(
            request_trust_posture("ipfs", true),
            TrustPosture::ContentVerified
        );
        assert_eq!(
            request_trust_posture("https", true),
            TrustPosture::UnverifiedOrigin,
            "no https request is ever content-verified"
        );
        assert_eq!(
            request_trust_posture("ipfs", false),
            TrustPosture::UnverifiedOrigin,
            "an ipfs request that did NOT verify claims nothing"
        );
    }

    #[test]
    fn a_network_entry_derives_its_scheme_from_the_url() {
        assert_eq!(network("ipfs://cid/x.png").scheme, "ipfs");
        assert_eq!(network("HTTPS://x/y").scheme, "https");
        assert_eq!(network("werust://settings").scheme, "werust");
        assert_eq!(network("about:blank").scheme, "about");
        assert_eq!(network("nonsense").scheme, "");
    }

    #[test]
    fn an_entry_defaults_to_the_unverified_posture() {
        assert_eq!(network("ipfs://cid").trust, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn an_oversized_message_and_url_are_truncated_so_one_entry_cannot_grow_unboundedly() {
        let capture = DebugCapture::new();
        capture.push_console(console(&"x".repeat(MAX_TEXT_CHARS * 2)));
        capture.push_network(network(&format!(
            "https://x/{}",
            "y".repeat(MAX_TEXT_CHARS * 2)
        )));
        assert_eq!(capture.console()[0].message.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(capture.network()[0].url.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn the_json_round_trips_console_and_network_entries_with_their_fields() {
        let capture = DebugCapture::new();
        capture.push_console(
            ConsoleEntry::new(ConsoleLevel::Warn, "deprecated API")
                .with_source("https://x/app.js")
                .with_line(42)
                .with_timestamp(1_700_000_000_123),
        );
        capture.push_network(
            NetworkEntry::new("GET", "ipfs://bafy/pic.png")
                .with_status(200)
                .with_mime("image/png")
                .with_size(1234)
                .from_cache(true)
                .with_trust(TrustPosture::ContentVerified)
                .with_duration(17)
                .with_timestamp(1_700_000_000_456),
        );

        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&capture)).expect("valid JSON");

        let c = &json["console"][0];
        assert_eq!(c["level"], "warn");
        assert_eq!(c["message"], "deprecated API");
        assert_eq!(c["source"], "https://x/app.js");
        assert_eq!(c["line"], 42);
        assert_eq!(c["ts"], 1_700_000_000_123u64);

        let n = &json["network"][0];
        assert_eq!(n["method"], "GET");
        assert_eq!(n["url"], "ipfs://bafy/pic.png");
        assert_eq!(n["status"], 200);
        assert_eq!(n["mime"], "image/png");
        assert_eq!(n["size"], 1234);
        assert_eq!(n["fromCache"], true);
        assert_eq!(n["scheme"], "ipfs");
        assert_eq!(n["trust"], "content-verified");
        assert_eq!(n["duration"], 17);
        assert_eq!(n["ts"], 1_700_000_000_456u64);
        assert_eq!(json["networkCaptureEnabled"], true);
    }

    #[test]
    fn the_json_carries_the_same_trust_wire_names_the_chrome_json_uses() {
        // The debug view must speak the chrome trust indicator's EXACT vocabulary
        // (ADR-0006): the SAME lower-kebab wire names, never a new label.
        for (posture, wire) in [
            (TrustPosture::UnverifiedOrigin, "unverified-origin"),
            (TrustPosture::ContentVerified, "content-verified"),
            (TrustPosture::NameViaTrustedRpc, "name-via-trusted-rpc"),
            (TrustPosture::MutableName, "mutable-name"),
        ] {
            let capture = DebugCapture::new();
            capture.push_network(network("ipfs://cid").with_trust(posture));
            let json: serde_json::Value =
                serde_json::from_str(&debug_json(&capture)).expect("valid JSON");
            assert_eq!(json["network"][0]["trust"], wire, "posture {posture:?}");
        }
    }

    #[test]
    fn every_console_level_has_a_distinct_wire_name() {
        for (level, wire) in [
            (ConsoleLevel::Log, "log"),
            (ConsoleLevel::Info, "info"),
            (ConsoleLevel::Warn, "warn"),
            (ConsoleLevel::Error, "error"),
            (ConsoleLevel::Debug, "debug"),
        ] {
            assert_eq!(level.wire_name(), wire);
        }
    }

    #[test]
    fn an_unknown_numeric_field_serializes_as_null_not_a_fake_zero() {
        let capture = DebugCapture::new();
        capture.push_console(console("no source"));
        capture.push_network(network("ipfs://cid"));
        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&capture)).expect("valid JSON");
        assert!(json["console"][0]["line"].is_null());
        assert!(json["network"][0]["status"].is_null());
        assert!(json["network"][0]["size"].is_null());
        assert!(json["network"][0]["duration"].is_null());
    }

    #[test]
    fn a_surprising_message_cannot_break_the_json() {
        let capture = DebugCapture::new();
        capture.push_console(console("a \"quoted\"\nline\\with\ttabs"));
        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&capture)).expect("valid JSON");
        assert_eq!(
            json["console"][0]["message"],
            "a \"quoted\"\nline\\with\ttabs"
        );
    }

    #[test]
    fn a_clone_shares_the_same_store_so_a_capture_point_and_the_shell_agree() {
        // The store is shared exactly like `ipfs::RedirectSink`: the (possibly
        // off-UI-thread) capture point owns a clone, the shell owns the other, and
        // both see the SAME entries.
        let capture = DebugCapture::new();
        let handle = capture.clone();
        std::thread::spawn(move || handle.push_network(network("ipfs://from-a-worker")))
            .join()
            .expect("the capture point thread");
        assert_eq!(capture.network().len(), 1);
        assert_eq!(capture.network()[0].url, "ipfs://from-a-worker");
    }

    #[test]
    fn the_empty_store_serializes_as_empty_arrays() {
        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&DebugCapture::new())).expect("valid JSON");
        assert_eq!(json["console"].as_array().map(Vec::len), Some(0));
        assert_eq!(json["network"].as_array().map(Vec::len), Some(0));
    }
}
