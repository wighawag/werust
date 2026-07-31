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
//! # Eviction is OBSERVABLE (the monotonic sequence)
//!
//! A buffer AT its cap never changes length (every push is paired with a
//! `pop_front`), so a view that anchors its incremental refresh on the LENGTH
//! freezes exactly then. The store therefore stamps every pushed entry with a
//! MONOTONIC [`sequence`](ConsoleEntry::sequence) (one shared counter,
//! surviving `pop_front`, never rewound by [`clear`](DebugCapture::clear)): a
//! view anchors on the last sequence it rendered and appends only what follows
//! it in the next snapshot, rebuilding when the anchor itself was evicted. The
//! sequence is a store/render-path concern; it never reaches the FFI debug
//! JSON (the edges re-render from each snapshot).
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
    /// Every console level, in declaration order.
    ///
    /// The single source of truth for "which console levels exist", so a caller
    /// that must cover the whole axis iterates THIS instead of re-listing the
    /// variants in a literal that silently goes stale — the same discipline
    /// [`TrustPosture::ALL`] carries, and load-bearing for the same reason: the
    /// debug view derives one CSS class per level and exports the complete class
    /// family ([`DEBUG_CONSOLE_CSS_CLASSES`]) for painters to colour from, and
    /// the test that proves the family covers every level drives it from here.
    /// Kept complete by the const check below.
    pub const ALL: [ConsoleLevel; 5] = [
        ConsoleLevel::Log,
        ConsoleLevel::Info,
        ConsoleLevel::Warn,
        ConsoleLevel::Error,
        ConsoleLevel::Debug,
    ];

    /// Map a PLATFORM's console level name onto werust's one console vocabulary.
    ///
    /// The three capture points report a level as text, and each platform spells
    /// it its own way: the injected [`console_shim`] posts the `console.*` method
    /// name (`log`/`info`/`warn`/`error`/`debug`), while Android's
    /// `ConsoleMessage.MessageLevel` is `LOG`/`WARNING`/`ERROR`/`DEBUG`/`TIP`.
    /// This is the ONE place that mapping lives, so a Console tab shows the same
    /// level for the same page log whatever platform captured it.
    ///
    /// Case-insensitive, and deliberately TOTAL: an unrecognised level is
    /// [`Log`](ConsoleLevel::Log) (the type's default) rather than a new level or
    /// a dropped entry — a capture point never invents vocabulary and never
    /// silently loses a message.
    #[must_use]
    pub fn from_platform(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "info" => ConsoleLevel::Info,
            // Android spells `console.warn` `WARNING`.
            "warn" | "warning" => ConsoleLevel::Warn,
            "error" => ConsoleLevel::Error,
            // Android's `VERBOSE`-ish `DEBUG`, and the `console.debug` method.
            "debug" | "verbose" => ConsoleLevel::Debug,
            // Android's `TIP` is an advisory hint the engine emits: closest to
            // `console.info`, and definitely not a warning.
            "tip" => ConsoleLevel::Info,
            _ => ConsoleLevel::Log,
        }
    }

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

/// Keeps [`ConsoleLevel::ALL`] EXHAUSTIVE, at compile time.
///
/// The same const check [`LoadState::ALL`](renderer::LoadState::ALL) and
/// [`TrustPosture::ALL`] carry: `listed` is a TOTAL match (no wildcard arm) whose
/// every arm hands back the level's OWN entry in the list, so a sixth
/// [`ConsoleLevel`] cannot reach a build — the match stops compiling until the
/// variant is named here, and the arm the author then writes
/// (`… => ConsoleLevel::ALL[5]`) stops compiling too (`index out of bounds`, the
/// deny-by-default `unconditional_panic` lint) unless the variant is ALSO added
/// to `ALL`. The loop closes the last hole: each level is held once, at the slot
/// its arm claims.
const _CONSOLE_LEVEL_ALL_IS_EVERY_LEVEL_IN_SLOT_ORDER: () = {
    const fn listed(level: ConsoleLevel) -> ConsoleLevel {
        match level {
            ConsoleLevel::Log => ConsoleLevel::ALL[0],
            ConsoleLevel::Info => ConsoleLevel::ALL[1],
            ConsoleLevel::Warn => ConsoleLevel::ALL[2],
            ConsoleLevel::Error => ConsoleLevel::ALL[3],
            ConsoleLevel::Debug => ConsoleLevel::ALL[4],
        }
    }
    let mut i = 0;
    while i < ConsoleLevel::ALL.len() {
        assert!(
            listed(ConsoleLevel::ALL[i]) as u8 == ConsoleLevel::ALL[i] as u8,
            "ConsoleLevel::ALL must hold every level, once, in slot order"
        );
        i += 1;
    }
};

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
    /// The store-assigned MONOTONIC sequence (`0` until pushed). Private: a
    /// capture point reports an entry, it never numbers one; the store stamps
    /// it on push so the sequence always reflects store order.
    sequence: u64,
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
            sequence: 0,
        }
    }

    /// The MONOTONIC sequence the store stamped on push (`0` for an entry that
    /// was never pushed): it survives the ring buffer's `pop_front` eviction,
    /// so a debug VIEW can anchor on the last sequence it rendered and tell "N
    /// appended" from "N appended AND M evicted", which a length alone cannot,
    /// because a buffer AT its cap never changes length. A store/render-path
    /// concern only: it never reaches the FFI debug JSON.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
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
    /// The store-assigned MONOTONIC sequence (`0` until pushed); see
    /// [`ConsoleEntry::sequence`]. Private for the same reason.
    sequence: u64,
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
            sequence: 0,
        }
    }

    /// The MONOTONIC sequence the store stamped on push (`0` for an entry that
    /// was never pushed); see [`ConsoleEntry::sequence`], whose eviction-anchor
    /// rationale this shares.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
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
    /// The next MONOTONIC sequence to stamp, shared by both buffers (each
    /// buffer's sequences stay strictly increasing). Starts at `1` so `0` on an
    /// entry always means "never pushed", and is NEVER rewound, not even by
    /// `clear()`, so a post-clear entry can never carry a sequence a view
    /// already rendered and be mistaken for an old row.
    next_sequence: u64,
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
            next_sequence: 1,
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
        let mut entry = entry;
        entry.sequence = inner.next_sequence;
        inner.next_sequence += 1;
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
        let mut entry = entry;
        entry.sequence = inner.next_sequence;
        inner.next_sequence += 1;
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

// ---------------------------------------------------------------------------
// The SHARED half of the per-platform CAPTURE POINTS (task
// `debug-console-network-capture-per-platform`).
//
// The store above is fed by six capture points across three platforms. Two of
// them (desktop and iOS) have NO native console callback, so they capture the
// console by INJECTING a page-side shim over the seam's script-message bridge —
// and iOS additionally captures what network it can reach with a best-effort
// `fetch`/`XHR` wrapper. Everything those points share (the injected JS, the
// envelope they post, the parse, and the event -> entry mapping the NATIVE hooks
// use too) lives HERE, in the toolkit-free core, so:
//
// * desktop and iOS inject the byte-for-byte SAME shim (one string, one channel
//   name, one envelope shape) rather than two drifting copies;
// * the mapping from a platform console/network event to a core entry is a pure
//   function, unit-tested with no webview, no GTK loop and no network;
// * every entry is built through `new()` + the `with_*` setters, so the
//   `MAX_TEXT_CHARS` truncation that makes the store bounded can never be
//   bypassed by a capture point assigning a field directly.
//
// It lives in THIS module (not a new one) because it is the same concept: the
// debug capture. `DebugCapture` is the sink; this is the shared plumbing that
// fills it.
// ---------------------------------------------------------------------------

/// The script-message bridge name the injected capture shim posts to.
///
/// Deliberately its OWN channel, NOT the EIP-1193
/// [`PROVIDER_BRIDGE`](crate::provider::PROVIDER_BRIDGE): the provider channel is
/// a trust surface with a request/response contract, and folding a debug
/// observation stream into it would re-mean it. The page posts
/// `window.webkit.messageHandlers.werustDebug.postMessage(<json>)`; nothing is
/// ever pushed back down this channel (capture is one-way, READ-ONLY
/// observation).
pub const CAPTURE_BRIDGE: &str = "werustDebug";

/// The page-side CONSOLE capture shim, injected at document start by the
/// platforms with no native console callback (desktop WebKitGTK and iOS
/// WKWebView).
///
/// It wraps `console.log/info/warn/error/debug`, posts a
/// `{"kind":"console", …}` envelope up the [`CAPTURE_BRIDGE`], and then CHAINS
/// to the original method, so the page's own console behaviour (and the native
/// remote inspector's console) is unchanged — capture never swallows a message.
/// It also guards against double-installation (`__werustConsoleCaptured`), takes
/// the source/line best-effort from a synthetic stack (reporting line `0`, i.e.
/// "unknown", rather than guessing when the frame is unreadable), and swallows
/// its OWN errors: a debug surface must never be able to break a page.
///
/// Android does NOT use this: it has the REAL native callback
/// (`WebChromeClient.onConsoleMessage`), which reports level/message/source/line
/// directly and is strictly better than a shim. That deliberate per-platform
/// difference is recorded in
/// `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`.
#[must_use]
pub fn console_shim() -> String {
    // The channel name is substituted rather than `format!`-templated so the JS
    // below stays readable (a `format!` would need every brace doubled).
    CONSOLE_SHIM_JS.replace(BRIDGE_PLACEHOLDER, CAPTURE_BRIDGE)
}

/// The page-side BEST-EFFORT network capture shim (`fetch` + `XMLHttpRequest`),
/// injected at document start by iOS ONLY.
///
/// WKWebView exposes no per-resource load callback, so this is the pragmatic
/// route to a non-empty Network tab on iOS. Its coverage is honestly PARTIAL: it
/// sees only requests the PAGE makes through `fetch`/`XHR`, never the
/// browser-internal subresource loads (`<img>`, `<script>`, CSS `url()`,
/// navigation preloads). The limits are recorded in
/// `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`.
///
/// Desktop does NOT use this: WebKitGTK's `resource-load-started` signal already
/// reports EVERY resource (including the internal subresource loads this cannot
/// see), so injecting this there would only double-record a subset.
///
/// It SKIPS `ipfs:`/`werust:` URLs, which the native scheme handler already
/// records with their REAL (verified) posture: capturing them here as well would
/// produce a second, contradicting row claiming the weaker
/// [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin) posture for the same
/// request. It observes on a SEPARATE promise chain and never rethrows, so it
/// cannot alter the page's own request outcome (READ-ONLY).
#[must_use]
pub fn network_shim() -> String {
    NETWORK_SHIM_JS.replace(BRIDGE_PLACEHOLDER, CAPTURE_BRIDGE)
}

/// The token the shim sources carry where [`CAPTURE_BRIDGE`] is substituted in.
const BRIDGE_PLACEHOLDER: &str = "__WERUST_CAPTURE_BRIDGE__";

const CONSOLE_SHIM_JS: &str = r#"(function () {
  "use strict";
  var BRIDGE = "__WERUST_CAPTURE_BRIDGE__";
  // The shim is a document-start user script; guard so a re-injection (a second
  // frame, an edge that also evaluates it on page-start) wraps console only once
  // and cannot stack wrappers.
  if (window.__werustConsoleCaptured) { return; }
  window.__werustConsoleCaptured = true;

  function post(envelope) {
    // Page -> native, over the debug capture channel. Guarded and silent: a
    // missing bridge (or a serialisation failure) must never throw into the page.
    try {
      var mh = window.webkit
        && window.webkit.messageHandlers
        && window.webkit.messageHandlers[BRIDGE];
      if (mh && typeof mh.postMessage === "function") {
        mh.postMessage(JSON.stringify(envelope));
      }
    } catch (e) {}
  }

  // Flatten console arguments to ONE text message (the shape every native
  // console hook reports, and what the store's `message` field holds).
  function flatten(args) {
    var parts = [];
    for (var i = 0; i < args.length; i++) {
      var a = args[i];
      if (typeof a === "string") { parts.push(a); continue; }
      var text;
      try { text = JSON.stringify(a); } catch (e) { text = null; }
      // JSON.stringify yields undefined for undefined/functions/symbols.
      parts.push(text === undefined || text === null ? String(a) : text);
    }
    return parts.join(" ");
  }

  // Best-effort source/line from a synthetic stack. Frame 0 is this function and
  // frame 1 the console wrapper, so frame 2 is the page's own call site. If the
  // stack is unreadable we report NO line (0), never a guessed one.
  function callSite() {
    var site = { source: "", line: 0 };
    try {
      var stack = (new Error()).stack;
      if (!stack) { return site; }
      var frames = String(stack).split("\n");
      var frame = frames[2] || frames[frames.length - 1] || "";
      var at = frame.indexOf("@");
      var loc = (at === -1 ? frame : frame.slice(at + 1)).trim();
      var m = /^(.*):(\d+):(\d+)$/.exec(loc);
      if (m) {
        site.source = m[1];
        site.line = parseInt(m[2], 10) || 0;
      } else {
        site.source = loc;
      }
    } catch (e) {}
    return site;
  }

  ["log", "info", "warn", "error", "debug"].forEach(function (level) {
    var original = console[level];
    console[level] = function () {
      var site = callSite();
      post({
        kind: "console",
        level: level,
        message: flatten(arguments),
        source: site.source,
        line: site.line,
        ts: Date.now()
      });
      // CHAIN to the original: capture observes, it never swallows. The page's
      // own console (and the native remote inspector) behaves exactly as before.
      if (typeof original === "function") { original.apply(console, arguments); }
    };
  });
})();"#;

const NETWORK_SHIM_JS: &str = r#"(function () {
  "use strict";
  var BRIDGE = "__WERUST_CAPTURE_BRIDGE__";
  if (window.__werustNetworkCaptured) { return; }
  window.__werustNetworkCaptured = true;

  function post(envelope) {
    try {
      var mh = window.webkit
        && window.webkit.messageHandlers
        && window.webkit.messageHandlers[BRIDGE];
      if (mh && typeof mh.postMessage === "function") {
        mh.postMessage(JSON.stringify(envelope));
      }
    } catch (e) {}
  }

  // The NATIVE custom-scheme handler already records these with their REAL
  // (hash-verified) trust posture; recording them here too would add a second,
  // contradicting row claiming the weaker unverified posture for the same
  // request.
  function skip(url) {
    var u = String(url || "").toLowerCase();
    return u.indexOf("ipfs:") === 0 || u.indexOf("werust:") === 0;
  }

  function record(method, url, status, mime, size, started) {
    post({
      kind: "network",
      method: String(method || "GET").toUpperCase(),
      url: String(url || ""),
      status: status || 0,
      mime: String(mime || "").split(";")[0].trim(),
      size: size || 0,
      ts: Date.now(),
      duration: Date.now() - started
    });
  }

  var originalFetch = window.fetch;
  if (typeof originalFetch === "function") {
    window.fetch = function (input, init) {
      var url = (typeof input === "string") ? input : ((input && input.url) || "");
      var method = (init && init.method) || (input && input.method) || "GET";
      var started = Date.now();
      var promise = originalFetch.apply(this, arguments);
      if (!skip(url)) {
        // Observe on a SEPARATE chain with BOTH handlers supplied, so the page's
        // own promise is returned untouched and this observation can neither
        // alter the outcome nor raise an unhandled rejection.
        try {
          promise.then(function (response) {
            var mime = "";
            var size = 0;
            try {
              mime = (response.headers && response.headers.get("content-type")) || "";
              size = parseInt((response.headers && response.headers.get("content-length")) || "0", 10) || 0;
            } catch (e) {}
            record(method, url, response.status, mime, size, started);
          }, function () {
            // A failed request has no status: report it as unknown, not a fake 0.
            record(method, url, 0, "", 0, started);
          });
        } catch (e) {}
      }
      return promise;
    };
  }

  var OriginalXHR = window.XMLHttpRequest;
  if (typeof OriginalXHR === "function" && OriginalXHR.prototype) {
    var open = OriginalXHR.prototype.open;
    var send = OriginalXHR.prototype.send;
    OriginalXHR.prototype.open = function (method, url) {
      try {
        this.__werustMethod = method;
        this.__werustUrl = url;
      } catch (e) {}
      return open.apply(this, arguments);
    };
    OriginalXHR.prototype.send = function () {
      var xhr = this;
      var started = Date.now();
      try {
        if (!skip(xhr.__werustUrl)) {
          xhr.addEventListener("loadend", function () {
            var mime = "";
            try { mime = xhr.getResponseHeader("content-type") || ""; } catch (e) {}
            record(xhr.__werustMethod, xhr.__werustUrl, xhr.status, mime, 0, started);
          });
        }
      } catch (e) {}
      return send.apply(this, arguments);
    };
  }
})();"#;

/// One event a capture point observed: already mapped onto a core entry, ready
/// to push.
///
/// The injected shim posts BOTH kinds over the one [`CAPTURE_BRIDGE`], so the
/// parse must be able to answer either; the native hooks build their entry
/// directly ([`console_entry`] / [`network_entry`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedEvent {
    /// A `console.*` call the injected console shim reported.
    Console(ConsoleEntry),
    /// A `fetch`/`XHR` request the injected network shim reported.
    Network(NetworkEntry),
}

/// Parse one envelope the injected shim posted up the [`CAPTURE_BRIDGE`] into a
/// core entry, or [`None`] for anything unreadable.
///
/// Total and fail-quiet by design: a page can post ARBITRARY text on this channel
/// (the shim is page-side JS a hostile page can call directly), so a malformed,
/// hostile, or unknown-`kind` body yields `None` rather than an error, a panic, or
/// a fabricated entry. Every field is bounded by the entry constructors, and an
/// absent/zero optional (`line`, `status`, `size`, `duration`) stays honestly
/// ABSENT rather than becoming a fake `0`.
///
/// A shim-reported NETWORK entry always carries the conservative
/// [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin) posture
/// ([`request_trust_posture`] with `verified: false`): page-side JS cannot prove
/// anything about the load path, so it never claims verification. The shim skips
/// the content-addressed schemes the NATIVE handler records with their real
/// posture, so this can never contradict a verified row.
#[must_use]
pub fn parse_capture_message(body: &str) -> Option<CapturedEvent> {
    let value: Value = serde_json::from_str(body).ok()?;
    let timestamp = value.get("ts").and_then(Value::as_u64).unwrap_or_default();
    match value.get("kind").and_then(Value::as_str)? {
        "console" => {
            let level = ConsoleLevel::from_platform(
                value
                    .get("level")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source = value
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let line = value
                .get("line")
                .and_then(Value::as_u64)
                .and_then(|l| u32::try_from(l).ok())
                .unwrap_or_default();
            Some(CapturedEvent::Console(console_entry(
                level, message, source, line, timestamp,
            )))
        }
        "network" => {
            let url = value.get("url").and_then(Value::as_str).unwrap_or_default();
            let method = value.get("method").and_then(Value::as_str).unwrap_or("GET");
            let status = value
                .get("status")
                .and_then(Value::as_u64)
                .and_then(|s| u16::try_from(s).ok());
            let mime = value
                .get("mime")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let size = value.get("size").and_then(Value::as_u64);
            let mut entry = network_entry(method, url, status, mime, size, false, timestamp);
            if let Some(duration) = value.get("duration").and_then(Value::as_u64) {
                entry = entry.with_duration(duration);
            }
            Some(CapturedEvent::Network(entry))
        }
        _ => None,
    }
}

/// Parse an envelope the injected shim posted and PUSH it into `capture`.
///
/// The one line every shim-fed capture point runs (desktop's script-message
/// handler, iOS's `WKScriptMessageHandler`), so the parse, the bound, and the
/// posture rule cannot drift between the two platforms. Unreadable bodies are
/// dropped silently ([`parse_capture_message`]).
pub fn route_capture_message(capture: &DebugCapture, body: &str) {
    match parse_capture_message(body) {
        Some(CapturedEvent::Console(entry)) => capture.push_console(entry),
        Some(CapturedEvent::Network(entry)) => capture.push_network(entry),
        None => {}
    }
}

/// Build a [`ConsoleEntry`] from what a console capture point reports, through
/// the CONSTRUCTORS (so `MAX_TEXT_CHARS` truncation applies) and with an absent
/// field left honestly absent.
///
/// `line` is 1-based; `0` means "the platform reported none" and yields
/// [`None`], never a fabricated line 0. An empty `source` is left empty. Shared
/// by the shim path and the native Android `onConsoleMessage` path (through its
/// FFI), so all three platforms map onto the store identically.
#[must_use]
pub fn console_entry(
    level: ConsoleLevel,
    message: &str,
    source: &str,
    line: u32,
    timestamp: u64,
) -> ConsoleEntry {
    let mut entry = ConsoleEntry::new(level, message).with_timestamp(timestamp);
    if !source.is_empty() {
        entry = entry.with_source(source);
    }
    if line > 0 {
        entry = entry.with_line(line);
    }
    entry
}

/// Build a [`NetworkEntry`] from what a network capture point reports, through
/// the CONSTRUCTORS (so `MAX_TEXT_CHARS` truncation applies) and with the HONEST
/// per-request trust posture.
///
/// `verified` says whether THIS request's bytes actually came back through the
/// hash-verified content-addressed path; the posture is derived from it by
/// [`request_trust_posture`], never from the URL string, so a request that merely
/// LOOKS content-addressed is never labelled verified. A `None`/`0` optional
/// (`status`, `size`) stays honestly absent rather than becoming a fake `0`.
///
/// The MAIN-DOCUMENT entry is the one exception the store's DECISIONS.md hands to
/// the capture points: its posture is overwritten with the LOAD's own posture
/// (via [`NetworkEntry::with_trust`]) so the Network tab cannot show
/// `content-verified` while the chrome trust indicator shows the louder
/// `name-via-trusted-rpc` for the same page (ADR-0006's two-axis rule).
#[must_use]
pub fn network_entry(
    method: &str,
    url: &str,
    status: Option<u16>,
    mime: &str,
    size: Option<u64>,
    verified: bool,
    timestamp: u64,
) -> NetworkEntry {
    let mut entry = NetworkEntry::new(method, url).with_timestamp(timestamp);
    let trust = request_trust_posture(&entry.scheme, verified);
    entry = entry.with_trust(trust);
    if let Some(status) = status.filter(|s| *s > 0) {
        entry = entry.with_status(status);
    }
    if !mime.is_empty() {
        entry = entry.with_mime(mime);
    }
    if let Some(size) = size.filter(|s| *s > 0) {
        entry = entry.with_size(size);
    }
    entry
}

/// The stable, lower-kebab wire name of a [`TrustPosture`]: the SAME names the
/// mobile chrome JSON ([`chrome_json`](crate::chrome_json), which calls this) and
/// the desktop trust indicator use.
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

/// The inverse of [`trust_posture_wire_name`]: a wire name back to its
/// [`TrustPosture`], or `None` for a spelling this build does not know.
///
/// It lives HERE, beside the encoder, so the one vocabulary has one home: the
/// TOFU pin store ([`pins`](crate::pins)) PERSISTS a posture and must read it
/// back, and a private parser over there would be a second table to drift from.
/// Deliberately total over [`TrustPosture::ALL`] (asserted by
/// `every_posture_wire_name_round_trips`) and deliberately FALLIBLE: an unknown
/// spelling in a persisted file is dropped, never silently defaulted to a
/// posture the user never saw.
#[must_use]
pub fn trust_posture_from_wire_name(name: &str) -> Option<TrustPosture> {
    TrustPosture::ALL
        .into_iter()
        .find(|posture| trust_posture_wire_name(*posture) == name)
}

// ---------------------------------------------------------------------------
// ROW PRESENTATION: the debug view's display rules.
//
// Pure functions of ONE captured entry (plus the refresh PLAN a tab applies to
// its rows), living beside the store they render — the same layering the chrome
// presentation has in `lib.rs` (`status_line`, `trust_indicator*`, …,
// `docs/adr/0011`): the RULE is decided once in the toolkit-free core and each OS
// edge is a PAINTER that assigns the result to widgets.
//
// They started life private to the GTK edge (`crates/werust/src/main.rs`, task
// `debug-view-console-network-tabs-desktop`) and were moved here
// behaviour-preservingly, with their tests, when the macOS debug view needed the
// SAME derivation (task `macos-appkit-window-and-chrome`). That is the standing
// lesson of the Kotlin and Swift twins: a second painter that re-implements a
// display rule drifts from the first, and the drift ships (the trust EXPLANATION
// was desktop-only for months). Nothing here knows about a widget, a toolkit or a
// colour: a class NAME is a stable state identifier, and the stylesheet that
// gives it a colour stays in the edge that has one.
// ---------------------------------------------------------------------------

/// Every class [`console_level_css_class`] can return: the console row's
/// MUTUALLY-EXCLUSIVE level family, one class per [`ConsoleLevel`].
///
/// The debug-view twin of the chrome's
/// [`TRUST_INDICATOR_CSS_CLASSES`](crate::TRUST_INDICATOR_CSS_CLASSES), exported
/// for the same reason (task `export-the-chrome-css-class-set-from-core`): a
/// painter that keeps its OWN literal list of level classes goes stale the moment
/// a sixth level lands, and an edge whose stylesheet has no rule for an exported
/// name renders that state INVISIBLY. Each edge's stylesheet guard iterates THIS
/// (the GTK `APP_CSS` test, the macOS colour-table test), so a class with no
/// styling reds the gate.
///
/// Deliberately NOT folded into
/// [`CHROME_CSS_CLASS_SETS`](crate::CHROME_CSS_CLASS_SETS): that set is the
/// browser CHROME's families (the trust badge, the error banner), which a painter
/// toggles on ONE widget at a time; these colour a debug-view ROW. Same pattern,
/// different surface, so they stay separate names rather than one set that means
/// two things.
pub const DEBUG_CONSOLE_CSS_CLASSES: &[&str] = &[
    "debug-console-log",
    "debug-console-info",
    "debug-console-warn",
    "debug-console-error",
    "debug-console-debug",
];

/// The CSS class colouring one console row by its level: error red, warn amber,
/// info blue, debug grey, log neutral. Pure, so the level-to-class mapping is
/// pinned without a display.
///
/// The complete set of classes this can return is
/// [`DEBUG_CONSOLE_CSS_CLASSES`]; a painter derives its list from there.
#[must_use]
pub fn console_level_css_class(level: ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Log => "debug-console-log",
        ConsoleLevel::Info => "debug-console-info",
        ConsoleLevel::Warn => "debug-console-warn",
        ConsoleLevel::Error => "debug-console-error",
        ConsoleLevel::Debug => "debug-console-debug",
    }
}

/// The `<source>:<line>` tail of a console row: empty when the platform
/// reported no source (the injected shim reports none for an unreadable stack
/// frame), source-only when it reported no line. An absent field stays honestly
/// absent rather than rendering a fabricated `:0`. Pure, for display-free tests.
#[must_use]
pub fn console_source_line(entry: &ConsoleEntry) -> String {
    match (entry.source.is_empty(), entry.line) {
        (true, _) => String::new(),
        (false, Some(line)) => format!("{}:{line}", entry.source),
        (false, None) => entry.source.clone(),
    }
}

/// The full text of one console row: `[<level>] <message>` plus the source tail
/// in parentheses when there is one. The level tag is the store's OWN wire name
/// (`log`/`info`/`warn`/`error`/`debug`), so the Console tab speaks the capture
/// store's vocabulary exactly. Pure, for display-free tests.
#[must_use]
pub fn console_row_text(entry: &ConsoleEntry) -> String {
    let source = console_source_line(entry);
    if source.is_empty() {
        format!("[{}] {}", entry.level.wire_name(), entry.message)
    } else {
        format!("[{}] {} ({source})", entry.level.wire_name(), entry.message)
    }
}

/// The status column of a network row: the response code, or `?` when the
/// request has no status (a custom scheme answered without one, or the request
/// failed before a response): an unknown stays honestly unknown.
#[must_use]
pub fn network_status_text(status: Option<u16>) -> String {
    status.map_or_else(|| "?".to_string(), |s| s.to_string())
}

/// The MIME column of a network row, or `?` when unknown.
#[must_use]
pub fn network_mime_text(mime: &str) -> String {
    if mime.is_empty() {
        "?".to_string()
    } else {
        mime.to_string()
    }
}

/// The size column of a network row: a human byte count (`512 B`, `1.5 KB`,
/// `2.0 MB`), or `?` when unknown.
#[must_use]
pub fn network_size_text(size: Option<u64>) -> String {
    match size {
        None => "?".to_string(),
        Some(bytes) if bytes < 1024 => format!("{bytes} B"),
        Some(bytes) if bytes < 1024 * 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
        Some(bytes) => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

/// The per-request trust label of a network row: the SAME posture vocabulary
/// the chrome trust indicator speaks (ADR-0006) — the indicator's glyph for the
/// posture plus the core's wire name (`content-verified`, `unverified-origin`,
/// `name-via-trusted-rpc`, `mutable-name`), never a new label minted for the
/// debug view. Pure, for display-free tests.
#[must_use]
pub fn network_trust_label(posture: TrustPosture) -> String {
    let glyph = match posture {
        TrustPosture::ContentVerified => "✓",
        TrustPosture::NameViaTrustedRpc => "◈",
        TrustPosture::MutableName => "◇",
        TrustPosture::UnverifiedOrigin => "⚠",
    };
    format!("{glyph} {}", trust_posture_wire_name(posture))
}

/// The CSS class colouring a network row's trust column: one of the SAME
/// `trust-*` classes the chrome trust indicator toggles
/// ([`TRUST_INDICATOR_CSS_CLASSES`](crate::TRUST_INDICATOR_CSS_CLASSES)), so a
/// content-verified request is the same green the indicator's verified badge is.
/// Pure, for display-free tests.
#[must_use]
pub fn network_trust_css_class(posture: TrustPosture) -> &'static str {
    match posture {
        TrustPosture::ContentVerified => "trust-verified",
        TrustPosture::NameViaTrustedRpc => "trust-name-trusted-rpc",
        TrustPosture::MutableName => "trust-mutable-name",
        TrustPosture::UnverifiedOrigin => "trust-unverified",
    }
}

/// What one debug-view tab must do to catch up with a store snapshot. Pure, so
/// the eviction-at-the-cap behaviour is pinned display-free; each edge's
/// application of it is one `match` (the GTK `ListBox`, the AppKit row stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailPlan {
    /// Rebuild the whole list from the snapshot: the first paint, a `clear`
    /// (the snapshot is shorter than what the view rendered), or the ring
    /// buffer having evicted past the last-rendered entry, so every row the
    /// view holds is stale.
    Rebuild,
    /// Append only the snapshot entries from `from` onward (the tail AFTER the
    /// last-rendered entry), first DROPPING `drop` rows from the view's top:
    /// the rows the ring buffer has already evicted from the store's front.
    /// The drop is EXPLICIT, not implicit: appending alone leaves the view
    /// holding rows the store discarded, its count climbing past the cap until
    /// the anchor itself is evicted. After the drop + append the view's rows
    /// match the snapshot exactly.
    AppendFrom {
        /// How many rows to remove from the view's TOP before appending.
        drop: usize,
        /// The snapshot index the appended tail starts at.
        from: usize,
    },
    /// Nothing new to render (the steady idle tick).
    Noop,
}

/// Plan one tab's refresh from the snapshot's entry SEQUENCES (oldest first,
/// strictly increasing), how many rows the view currently holds, and the
/// sequence of the last entry the view rendered.
///
/// The anchor is the sequence, never the length, because a ring buffer AT its
/// cap never changes length: `pop_front` eviction keeps it pinned, so "same
/// length" means both "nothing new" AND "N new, N evicted". The sequence tells
/// those apart: if the anchor still falls inside the snapshot, exactly the
/// entries after it are new; if it is ABSENT, everything the view holds was
/// evicted (or the store was cleared) and only a rebuild is honest.
///
/// `rendered_rows` makes the eviction REMOVAL explicit: the view's rows end at
/// the anchor, so of them only the snapshot rows up to and including the
/// anchor's position are still in the store; the rest were evicted from the
/// store's front and drop off the view's TOP on the append (zero while the
/// buffer sits below its cap).
#[must_use]
pub fn tail_plan(sequences: &[u64], rendered_rows: usize, last_rendered: Option<u64>) -> TailPlan {
    let Some(anchor) = last_rendered else {
        // Nothing rendered yet (the first paint, or just after a clear): an
        // empty snapshot is a no-op, a non-empty one renders everything.
        return if sequences.is_empty() {
            TailPlan::Noop
        } else {
            TailPlan::Rebuild
        };
    };
    match sequences.iter().position(|&s| s == anchor) {
        Some(index) if index + 1 < sequences.len() => TailPlan::AppendFrom {
            drop: rendered_rows.saturating_sub(index + 1),
            from: index + 1,
        },
        Some(_) => TailPlan::Noop,
        None => TailPlan::Rebuild,
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
    fn pushed_entries_carry_a_monotonic_sequence_that_survives_eviction() {
        // The ring buffer's length PINNED AT THE CAP cannot distinguish "10
        // appended" from "10 appended AND 10 evicted"; the per-entry monotonic
        // sequence can, and it survives the `pop_front` eviction. This is the
        // fact the debug view's refresh anchors on.
        let capture = DebugCapture::new();
        for i in 0..(MAX_CONSOLE_ENTRIES + 3) {
            capture.push_console(console(&format!("m{i}")));
        }
        let entries = capture.console();
        assert_eq!(entries.len(), MAX_CONSOLE_ENTRIES);
        assert!(
            entries
                .windows(2)
                .all(|w| w[0].sequence() < w[1].sequence()),
            "console sequences strictly increase in capture order"
        );
        // The first three pushes were evicted, so the surviving sequences span
        // exactly the surviving window.
        let first = entries.first().unwrap().sequence();
        let last = entries.last().unwrap().sequence();
        assert_eq!(last - first, (MAX_CONSOLE_ENTRIES - 1) as u64);

        let capture = DebugCapture::new();
        for i in 0..(MAX_NETWORK_ENTRIES + 3) {
            capture.push_network(network(&format!("https://x/{i}")));
        }
        let entries = capture.network();
        assert!(
            entries
                .windows(2)
                .all(|w| w[0].sequence() < w[1].sequence()),
            "network sequences strictly increase in capture order"
        );
    }

    #[test]
    fn a_clear_does_not_rewind_the_sequence() {
        // If a clear rewound the counter, a post-clear entry could carry the
        // SAME sequence a view remembers rendering before the clear, and the
        // view would mistake brand-new rows for already-rendered ones. The
        // counter is monotonic for the life of the store.
        let capture = DebugCapture::new();
        capture.push_console(console("before"));
        let before = capture.console()[0].sequence();
        capture.clear();
        capture.push_console(console("after"));
        let after = capture.console()[0].sequence();
        assert!(
            after > before,
            "a post-clear entry never reuses a pre-clear sequence"
        );
    }

    #[test]
    fn the_sequence_is_internal_and_never_reaches_the_debug_json() {
        // The sequence is a store/render-path concern (the desktop view's
        // incremental refresh); the edges re-render from each snapshot, so the
        // wire document stays exactly as recorded.
        let capture = DebugCapture::new();
        capture.push_console(console("hello"));
        capture.push_network(network("https://x/1"));
        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&capture)).expect("valid JSON");
        assert!(json["console"][0].get("sequence").is_none());
        assert!(json["network"][0].get("sequence").is_none());
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

    // -- the shared capture-point half (task
    //    `debug-console-network-capture-per-platform`) ------------------------

    #[test]
    fn every_platform_console_level_spelling_maps_onto_the_one_vocabulary() {
        // The three capture points spell a level differently (the shim posts the
        // `console.*` method name, Android reports `ConsoleMessage.MessageLevel`),
        // and this is the ONE place that mapping lives.
        for (platform, expected) in [
            ("log", ConsoleLevel::Log),
            ("info", ConsoleLevel::Info),
            ("warn", ConsoleLevel::Warn),
            ("WARNING", ConsoleLevel::Warn), // Android spells console.warn WARNING
            ("error", ConsoleLevel::Error),
            ("ERROR", ConsoleLevel::Error),
            ("debug", ConsoleLevel::Debug),
            ("TIP", ConsoleLevel::Info), // Android's advisory hint
        ] {
            assert_eq!(
                ConsoleLevel::from_platform(platform),
                expected,
                "platform level {platform}"
            );
        }
        assert_eq!(
            ConsoleLevel::from_platform("something-new"),
            ConsoleLevel::Log,
            "an unrecognised platform level falls back to log, never a new level"
        );
    }

    #[test]
    fn the_injected_shims_post_on_the_capture_channel_not_the_provider_channel() {
        // The debug channel is its OWN bridge: folding a one-way observation
        // stream into the EIP-1193 provider's request/response trust channel would
        // re-mean it.
        assert_ne!(CAPTURE_BRIDGE, crate::provider::PROVIDER_BRIDGE);
        for shim in [console_shim(), network_shim()] {
            assert!(shim.contains(CAPTURE_BRIDGE), "the shim names the channel");
            assert!(
                !shim.contains(BRIDGE_PLACEHOLDER),
                "the placeholder is substituted, not shipped to the page"
            );
            assert!(
                !shim.contains(crate::provider::PROVIDER_BRIDGE),
                "capture never posts on the provider channel"
            );
        }
    }

    #[test]
    fn the_console_shim_chains_to_the_original_and_wraps_every_level_once() {
        let shim = console_shim();
        for level in ["log", "info", "warn", "error", "debug"] {
            assert!(
                shim.contains(&format!("\"{level}\"")),
                "console.{level} is wrapped"
            );
        }
        assert!(
            shim.contains("original.apply(console, arguments)"),
            "capture CHAINS to the original console method, never swallows it"
        );
        assert!(
            shim.contains("__werustConsoleCaptured"),
            "a re-injection must not stack wrappers"
        );
    }

    #[test]
    fn the_network_shim_skips_the_schemes_the_native_handler_records_verified() {
        // Recording an `ipfs://` request page-side too would add a SECOND row for
        // the same request claiming the weaker unverified posture, contradicting
        // the native handler's honest verified one.
        let shim = network_shim();
        assert!(shim.contains("ipfs:"), "the shim skips ipfs:");
        assert!(shim.contains("werust:"), "the shim skips werust:");
        assert!(
            shim.contains("__werustNetworkCaptured"),
            "a re-injection must not stack wrappers"
        );
    }

    #[test]
    fn a_shim_console_envelope_maps_onto_a_console_entry() {
        let event = parse_capture_message(
            r#"{"kind":"console","level":"warn","message":"deprecated",
               "source":"https://x/app.js","line":42,"ts":1700000000123}"#,
        )
        .expect("a console envelope");
        let CapturedEvent::Console(entry) = event else {
            panic!("expected a console entry");
        };
        assert_eq!(entry.level, ConsoleLevel::Warn);
        assert_eq!(entry.message, "deprecated");
        assert_eq!(entry.source, "https://x/app.js");
        assert_eq!(entry.line, Some(42));
        assert_eq!(entry.timestamp, 1_700_000_000_123);
    }

    #[test]
    fn a_shim_network_envelope_maps_onto_an_unverified_network_entry() {
        // Page-side JS can prove NOTHING about the load path, so a shim-reported
        // request never claims verification.
        let event = parse_capture_message(
            r#"{"kind":"network","method":"post","url":"https://api.example/x",
               "status":201,"mime":"application/json","size":12,"ts":7,"duration":33}"#,
        )
        .expect("a network envelope");
        let CapturedEvent::Network(entry) = event else {
            panic!("expected a network entry");
        };
        assert_eq!(entry.method, "post");
        assert_eq!(entry.url, "https://api.example/x");
        assert_eq!(entry.status, Some(201));
        assert_eq!(entry.mime, "application/json");
        assert_eq!(entry.size, Some(12));
        assert_eq!(entry.duration, Some(33));
        assert_eq!(entry.scheme, "https");
        assert_eq!(entry.trust, TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn a_hostile_or_unreadable_capture_body_is_dropped_not_fabricated() {
        // The capture channel is page-reachable JS: a hostile page can post
        // anything. Every one of these must be dropped, never panic and never
        // become a fabricated entry.
        for body in [
            "",
            "not json",
            "[]",
            "null",
            r#"{"kind":"unknown"}"#,
            r#"{"message":"no kind"}"#,
            r#"{"kind":"console","level":{"nested":true},"line":"nope"}"#,
        ] {
            let capture = DebugCapture::new();
            route_capture_message(&capture, body);
            let dropped = capture.console().is_empty() && capture.network().is_empty();
            // A `console` envelope with junk FIELDS still yields an entry (the
            // fields degrade to their honest defaults); only a body with no
            // readable `kind` is dropped entirely.
            if body.contains("\"kind\":\"console\"") {
                assert_eq!(capture.console().len(), 1, "degraded, not dropped: {body}");
                assert_eq!(capture.console()[0].level, ConsoleLevel::Log);
                assert_eq!(capture.console()[0].line, None, "no fabricated line");
            } else {
                assert!(dropped, "unreadable body captured something: {body}");
            }
        }
    }

    #[test]
    fn routing_a_capture_message_pushes_into_the_shared_store() {
        let capture = DebugCapture::new();
        route_capture_message(
            &capture,
            r#"{"kind":"console","level":"error","message":"boom"}"#,
        );
        route_capture_message(&capture, r#"{"kind":"network","url":"https://x/y"}"#);
        assert_eq!(capture.console().len(), 1);
        assert_eq!(capture.console()[0].level, ConsoleLevel::Error);
        assert_eq!(capture.network().len(), 1);
        assert_eq!(capture.network()[0].url, "https://x/y");
    }

    #[test]
    fn a_capture_point_entry_leaves_an_absent_field_absent_not_a_fake_zero() {
        let entry = console_entry(ConsoleLevel::Log, "m", "", 0, 0);
        assert_eq!(entry.line, None, "line 0 means unknown, not line zero");
        assert_eq!(entry.source, "");

        let entry = network_entry("GET", "https://x/y", Some(0), "", Some(0), false, 0);
        assert_eq!(entry.status, None, "status 0 means unknown");
        assert_eq!(entry.size, None, "size 0 means unknown");
        assert_eq!(entry.mime, "");
    }

    #[test]
    fn a_capture_point_entry_is_bounded_because_it_goes_through_the_constructors() {
        // The `MAX_TEXT_CHARS` truncation lives ONLY in `new()`/`with_*`, so a
        // capture point that assigned a field directly would silently break the
        // store's boundedness in exactly the pathological case it guards.
        let huge = "x".repeat(MAX_TEXT_CHARS * 3);
        let entry = console_entry(ConsoleLevel::Log, &huge, &huge, 1, 0);
        assert_eq!(entry.message.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(entry.source.chars().count(), MAX_TEXT_CHARS);

        let entry = network_entry("GET", &huge, None, &huge, None, false, 0);
        assert_eq!(entry.url.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(entry.mime.chars().count(), MAX_TEXT_CHARS);

        // …and through the SHIM path too (the parse must not bypass them).
        let capture = DebugCapture::new();
        let body =
            serde_json::json!({"kind": "console", "level": "log", "message": huge}).to_string();
        route_capture_message(&capture, &body);
        assert_eq!(capture.console()[0].message.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn a_capture_point_never_labels_a_request_verified_from_its_url_alone() {
        // The posture tracks the ACTUAL load path (ADR-0006 / the store's
        // Decision 4): only a content-addressed request whose bytes really
        // verified is content-verified.
        assert_eq!(
            network_entry("GET", "ipfs://cid/x", None, "", None, false, 0).trust,
            TrustPosture::UnverifiedOrigin,
            "an ipfs:// request that did not verify claims nothing"
        );
        assert_eq!(
            network_entry("GET", "ipfs://cid/x", None, "", None, true, 0).trust,
            TrustPosture::ContentVerified
        );
        assert_eq!(
            network_entry("GET", "https://x/y", None, "", None, true, 0).trust,
            TrustPosture::UnverifiedOrigin,
            "no https request is ever content-verified, whatever a caller claims"
        );
    }

    #[test]
    fn the_main_document_entry_can_carry_the_loads_own_two_axis_posture() {
        // The store's DECISIONS.md Decision 4 hands THIS obligation to the capture
        // points: the main-document row takes the LOAD's posture, so the Network
        // tab cannot show `content-verified` while the chrome trust indicator
        // shows the louder `name-via-trusted-rpc` for the same page.
        let entry = network_entry(
            "GET",
            "ipfs://cid/index.html",
            Some(200),
            "text/html",
            None,
            true,
            0,
        )
        .with_trust(TrustPosture::NameViaTrustedRpc);
        assert_eq!(entry.trust, TrustPosture::NameViaTrustedRpc);
        assert_eq!(
            trust_posture_wire_name(entry.trust),
            "name-via-trusted-rpc",
            "and it renders in the trust indicator's exact vocabulary"
        );
    }

    #[test]
    fn the_empty_store_serializes_as_empty_arrays() {
        let json: serde_json::Value =
            serde_json::from_str(&debug_json(&DebugCapture::new())).expect("valid JSON");
        assert_eq!(json["console"].as_array().map(Vec::len), Some(0));
        assert_eq!(json["network"].as_array().map(Vec::len), Some(0));
    }

    // -- ROW PRESENTATION ---------------------------------------------------
    //
    // These tests MOVED here with the rules they cover, out of the GTK edge
    // (`crates/werust/src/main.rs`) where they were written by
    // `debug-view-console-network-tabs-desktop`, when the macOS debug view
    // needed the same derivation (task `macos-appkit-window-and-chrome`). They
    // are unchanged in substance: the extraction is behaviour-preserving, and
    // these are the assertions that say so.

    #[test]
    fn console_rows_carry_the_level_message_and_source_line_coloured_by_level() {
        // Acceptance (Console tab): a captured console entry renders level +
        // message + source:line, level-distinguished. The level tag is the
        // store's own wire name, and the row's CSS class is distinct per level
        // (error red, warn amber via each edge's `debug-console-*` styling).
        let entry = ConsoleEntry::new(ConsoleLevel::Warn, "deprecated API")
            .with_source("https://x/app.js")
            .with_line(42);
        assert_eq!(
            console_row_text(&entry),
            "[warn] deprecated API (https://x/app.js:42)"
        );
        assert_eq!(console_level_css_class(entry.level), "debug-console-warn");

        // Every level has its OWN class, so the levels are visually distinct.
        // Driven from `ConsoleLevel::ALL`, so a SIXTH level cannot arrive
        // untested (the exhaustiveness lesson of
        // `export-the-chrome-css-class-set-from-core`).
        let classes: Vec<&str> = ConsoleLevel::ALL
            .into_iter()
            .map(console_level_css_class)
            .collect();
        for (i, class) in classes.iter().enumerate() {
            assert!(class.starts_with("debug-console-"));
            assert!(
                !classes[..i].contains(class),
                "every level is coloured distinctly: {classes:?}"
            );
        }

        // An absent source/line stays honestly absent: no fabricated `:0`, no
        // dangling parentheses.
        let no_source = ConsoleEntry::new(ConsoleLevel::Error, "boom");
        assert_eq!(console_source_line(&no_source), "");
        assert_eq!(console_row_text(&no_source), "[error] boom");
        let source_no_line =
            ConsoleEntry::new(ConsoleLevel::Log, "hi").with_source("ipfs://cid/a.js");
        assert_eq!(
            console_row_text(&source_no_line),
            "[log] hi (ipfs://cid/a.js)"
        );
    }

    #[test]
    fn every_console_class_the_derivation_can_return_is_in_the_exported_set() {
        // The debug-view twin of the chrome's exported-class-set guarantee: the
        // family a painter colours from must hold EVERY class
        // `console_level_css_class` can return (or an edge would find no colour
        // for a level and render it invisibly), and no dead name (or an edge
        // would style a state that cannot happen). Exhaustive BY CONSTRUCTION:
        // the drive is `ConsoleLevel::ALL`, which its own const check keeps
        // complete, so a sixth level reds this test instead of landing with a
        // stale family and a green suite.
        let produced: Vec<&str> = ConsoleLevel::ALL
            .into_iter()
            .map(console_level_css_class)
            .collect();
        for class in &produced {
            assert!(
                DEBUG_CONSOLE_CSS_CLASSES.contains(class),
                "`{class}` is returned for a real console level but is not in the exported set"
            );
        }
        for exported in DEBUG_CONSOLE_CSS_CLASSES.iter().copied() {
            assert!(
                produced.contains(&exported),
                "`{exported}` is exported but no console level produces it (a dead name)"
            );
        }
        assert_eq!(DEBUG_CONSOLE_CSS_CLASSES.len(), ConsoleLevel::ALL.len());
    }

    #[test]
    fn network_rows_carry_method_status_mime_and_size_with_unknowns_honest() {
        // Acceptance (Network tab): the row columns render the entry's method,
        // status, mime and size, and an UNKNOWN field renders as `?`, never a
        // fabricated `0` (a failed request has no status; a shim-reported
        // request may have no size).
        let entry = NetworkEntry::new("GET", "ipfs://bafy/pic.png")
            .with_status(200)
            .with_mime("image/png")
            .with_size(1536);
        assert_eq!(entry.method, "GET");
        assert_eq!(network_status_text(entry.status), "200");
        assert_eq!(network_mime_text(&entry.mime), "image/png");
        assert_eq!(network_size_text(entry.size), "1.5 KB");

        let unknown = NetworkEntry::new("GET", "https://x/y");
        assert_eq!(network_status_text(unknown.status), "?");
        assert_eq!(network_mime_text(&unknown.mime), "?");
        assert_eq!(network_size_text(unknown.size), "?");

        // The size column is human-scaled at the unit boundaries.
        assert_eq!(network_size_text(Some(0)), "0 B");
        assert_eq!(network_size_text(Some(512)), "512 B");
        assert_eq!(network_size_text(Some(1024)), "1.0 KB");
        assert_eq!(network_size_text(Some(1024 * 1024)), "1.0 MB");
    }

    #[test]
    fn every_posture_wire_name_round_trips() {
        // The one posture vocabulary has one home: whatever `trust_posture_wire_name`
        // writes, `trust_posture_from_wire_name` reads back, for EVERY posture,
        // driven from `TrustPosture::ALL` (a compile-time check keeps it
        // complete), so a fifth posture cannot land persistable-but-unreadable in
        // the TOFU pin store. An unknown spelling stays `None`: a persisted file
        // from a newer build must be DROPPED, never defaulted to a posture the
        // user never saw.
        for posture in TrustPosture::ALL {
            assert_eq!(
                trust_posture_from_wire_name(trust_posture_wire_name(posture)),
                Some(posture)
            );
        }
        assert_eq!(trust_posture_from_wire_name("totally-trusted"), None);
        assert_eq!(trust_posture_from_wire_name(""), None);
    }

    #[test]
    fn the_network_trust_column_speaks_the_chrome_trust_indicators_exact_vocabulary() {
        // Acceptance (Network tab trust): each request renders werust's HONEST
        // per-request trust posture using the SAME vocabulary as the trust
        // indicator (ADR-0006), never a new label: the indicator's glyph, the
        // core's wire name, and one of the SAME `trust-*` CSS classes the
        // indicator toggles (the chrome's own exported family, so a renamed
        // class cannot leave this column pointing at a name nothing styles).
        let mut labels = Vec::new();
        // EVERY posture, from the seam's own exhaustive list, so a fifth posture
        // cannot reach the Network tab untested.
        for posture in TrustPosture::ALL {
            // The glyph the chrome's own indicator shows for this posture. A
            // total match, so a new posture does not compile until this drive
            // states the glyph it expects.
            let glyph = match posture {
                TrustPosture::ContentVerified => "✓",
                TrustPosture::NameViaTrustedRpc => "◈",
                TrustPosture::MutableName => "◇",
                TrustPosture::UnverifiedOrigin => "⚠",
            };
            let label = network_trust_label(posture);
            assert_eq!(
                label,
                format!("{glyph} {}", trust_posture_wire_name(posture)),
                "the Network tab speaks the trust indicator's vocabulary: {label}"
            );
            assert!(
                crate::TRUST_INDICATOR_CSS_CLASSES.contains(&network_trust_css_class(posture)),
                "the trust column reuses the indicator's own CSS classes"
            );
            assert!(
                !labels.contains(&label),
                "each posture is labelled distinctly: {label}"
            );
            labels.push(label);
        }

        // The honest split the spec names: an ipfs:// request content-verified,
        // an https:// subresource unverified-origin.
        assert_eq!(
            network_trust_label(TrustPosture::ContentVerified),
            "✓ content-verified"
        );
        assert_eq!(
            network_trust_label(TrustPosture::UnverifiedOrigin),
            "⚠ unverified-origin"
        );
        assert_eq!(
            network_trust_css_class(TrustPosture::ContentVerified),
            "trust-verified"
        );
        assert_eq!(
            network_trust_css_class(TrustPosture::UnverifiedOrigin),
            "trust-unverified"
        );
    }

    #[test]
    fn the_refresh_plan_appends_after_the_last_rendered_sequence_or_rebuilds() {
        // First paint (nothing rendered yet): an empty store is a no-op, a
        // non-empty one renders everything.
        assert_eq!(tail_plan(&[], 0, None), TailPlan::Noop);
        assert_eq!(tail_plan(&[1, 2, 3], 0, None), TailPlan::Rebuild);
        // The steady state BELOW the cap: append exactly the entries AFTER the
        // anchor, dropping nothing (nothing was evicted).
        assert_eq!(
            tail_plan(&[1, 2, 3], 1, Some(1)),
            TailPlan::AppendFrom { drop: 0, from: 1 }
        );
        assert_eq!(
            tail_plan(&[1, 2, 3], 2, Some(2)),
            TailPlan::AppendFrom { drop: 0, from: 2 }
        );
        // Caught up: the anchor is the snapshot's last entry.
        assert_eq!(tail_plan(&[1, 2, 3], 3, Some(3)), TailPlan::Noop);
        // AT-CAP EVICTION: the ring buffer's length is pinned at the cap, but
        // the anchor still falls inside the snapshot, so the view appends only
        // the entries after it AND drops the evicted rows from its top. The
        // view's rows end at the anchor, so of the 3 it holds only the snapshot
        // rows up to and including the anchor's position are still in the
        // store; the rest drop.
        assert_eq!(
            tail_plan(&[4, 5, 6], 3, Some(4)),
            TailPlan::AppendFrom { drop: 2, from: 1 }
        );
        assert_eq!(
            tail_plan(&[4, 5, 6], 3, Some(5)),
            TailPlan::AppendFrom { drop: 1, from: 2 }
        );
        // The anchor itself was evicted (a full buffer turned over while the
        // view was open): everything the view holds is stale, so REBUILD.
        assert_eq!(tail_plan(&[4, 5, 6], 3, Some(2)), TailPlan::Rebuild);
        // A CLEAR: the snapshot is shorter than what the view rendered (here
        // empty), so rebuild.
        assert_eq!(tail_plan(&[], 3, Some(2)), TailPlan::Rebuild);
    }

    #[test]
    fn pushing_past_the_cap_still_renders_the_newest_entry_and_drops_the_evicted_rows() {
        // The acceptance defect, driven against the REAL store (network-isolated,
        // no display): once a ring buffer sits AT its cap its length never
        // changes, so a length-anchored refresh freezes on rows the store has
        // already discarded. The sequence-anchored plan must keep the view
        // showing exactly what the store holds. The "view" here is a Vec of the
        // rendered messages, applying `tail_plan` EXACTLY as a real debug view
        // applies it to its rows (drop the evicted rows off the top, append the
        // new tail, rebuild when the anchor is gone), so the assertions are
        // about what a real view shows.
        fn apply(
            capture: &DebugCapture,
            view: &mut Vec<String>,
            last: &mut Option<u64>,
        ) -> TailPlan {
            let snapshot = capture.console();
            let sequences: Vec<u64> = snapshot.iter().map(ConsoleEntry::sequence).collect();
            let plan = tail_plan(&sequences, view.len(), *last);
            match plan {
                TailPlan::Rebuild => {
                    view.clear();
                    view.extend(snapshot.iter().map(|e| e.message.clone()));
                }
                TailPlan::AppendFrom { drop, from } => {
                    view.drain(..drop);
                    view.extend(snapshot[from..].iter().map(|e| e.message.clone()));
                }
                TailPlan::Noop => {}
            }
            *last = sequences.last().copied();
            plan
        }

        let capture = DebugCapture::new();
        let mut view: Vec<String> = Vec::new();
        let mut last_rendered: Option<u64> = None;
        for i in 0..MAX_CONSOLE_ENTRIES {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("m{i}")));
        }
        // The first paint rebuilds from the full (capped) store.
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert_eq!(view.len(), MAX_CONSOLE_ENTRIES);

        // Push 10 past the cap ONE AT A TIME with a refresh between each (the
        // pump-tick case): the store's length is UNCHANGED at every tick, but
        // each tick evicts one row from the front. The view must stay AT the cap
        // and mirror the store after every INCREMENTAL append, not only after a
        // rebuild (an append-only path climbs past the cap on stale rows).
        for i in 0..10 {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("new{i}")));
            let snapshot = capture.console();
            assert_eq!(snapshot.len(), MAX_CONSOLE_ENTRIES);
            let plan = apply(&capture, &mut view, &mut last_rendered);
            assert_eq!(
                plan,
                TailPlan::AppendFrom {
                    drop: 1,
                    from: MAX_CONSOLE_ENTRIES - 1
                },
                "each at-cap tick appends the one new entry and drops the one evicted row"
            );
            assert_eq!(
                view.len(),
                MAX_CONSOLE_ENTRIES,
                "the row count STAYS AT the cap across incremental at-cap appends"
            );
            assert_eq!(
                view.last().unwrap(),
                &format!("new{i}"),
                "the newest entry renders even at the cap"
            );
            assert!(
                view.iter()
                    .zip(snapshot.iter())
                    .all(|(v, e)| v == &e.message),
                "the view mirrors the store exactly: its top rows are never ones the store evicted"
            );
        }

        // A FULL buffer turns over between ticks: the anchor itself is evicted,
        // so the view rebuilds instead of appending onto stale rows.
        for i in 0..MAX_CONSOLE_ENTRIES {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("x{i}")));
        }
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert!(
            view.iter()
                .zip(capture.console().iter())
                .all(|(v, e)| v == &e.message),
            "a rebuild re-mirrors the store exactly"
        );

        // A clear: the snapshot is shorter than rendered, a rebuild empties it.
        capture.clear();
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert!(view.is_empty());
    }
}
