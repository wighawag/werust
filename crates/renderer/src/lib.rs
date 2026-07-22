//! The `Renderer` seam: the wide, hot-swappable rendering-backend interface.
//!
//! This crate declares the *whole* seam surface as the [`Renderer`] trait so a
//! native Rust renderer can later be swapped in behind it without touching the
//! rest of the browser (the "webview now, native later" hedge — see `CONTEXT.md`
//! and `docs/adr/0001`). The rest of werust talks to a rendering backend ONLY
//! through this trait; no backend internals (WebKitGTK, a future native engine)
//! leak past the seam.
//!
//! The surface is DECLARED here in full even where a given method is not yet
//! exercised by any backend, because a backend qualifies as *real* only if it can
//! satisfy the **trust hooks**: EIP-1193 provider injection (via the
//! [script-message bridge](Renderer::register_script_message_handler)) and
//! `ipfs://` resolution (via the [custom-scheme / request-interception
//! hook](Renderer::register_scheme_handler)). Those methods are part of the seam
//! from day one; the tasks `eip1193-provider-injection-via-script-bridge` and
//! `ipfs-scheme-resolution-through-renderer-seam` wire real behaviour onto them.
//!
//! The FIRST backend over this seam is the WebKitGTK system webview
//! (`webkitgtk` feature); this task implements navigate + show-the-page there.

use std::fmt;

/// Where a load is in its lifecycle.
///
/// A backend drives a page load through these states in order. `navigate`,
/// `reload`, and `stop` move it; the browser observes the transitions via
/// [`LoadEvent`]s (see [`Renderer::poll_event`]). This is the load-lifecycle
/// surface of the seam, modelled explicitly so it is testable at the trait level
/// (a backend need not own a GTK main loop to have a well-defined lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadState {
    /// No load has started, or a load was stopped before it committed.
    #[default]
    Idle,
    /// A navigation has been requested and the load has started.
    Started,
    /// The load committed — the first bytes of the main resource arrived and the
    /// destination URL is settled.
    Committed,
    /// The load finished successfully; the page is shown.
    Finished,
    /// The load failed (network error, cancelled, bad scheme, …).
    Failed,
}

impl LoadState {
    /// Whether a load is currently in flight (started or committed but not yet
    /// finished/failed/idle).
    #[must_use]
    pub fn is_loading(self) -> bool {
        matches!(self, LoadState::Started | LoadState::Committed)
    }
}

/// The **trust posture** of the current load: was the page CONTENT-VERIFIED
/// (fetched and hash-checked on the content-addressed path) or merely SERVED by
/// an origin werust does not trust by default?
///
/// This is a first-class seam fact, not a chrome cosmetic: `docs/adr/0001` fixes
/// that "the trust posture is itself a product surface" and calls out exactly
/// this indicator ("this was content-verified" vs "this was served by an
/// unverified origin") as something the chrome MUST expose rather than keep as a
/// silent internal. A backend reports it through
/// [`Renderer::trust_posture`](Renderer::trust_posture); the shell folds it into
/// its chrome and the window paints the indicator from it.
///
/// Crucially it reflects the **actual load path**, not the URL string: a load is
/// [`ContentVerified`](TrustPosture::ContentVerified) only when its bytes came
/// back through the hash-verified content-addressed fetch path (the `ipfs://`
/// custom-scheme hook resolving through
/// [`fetch_verified`](fetcher::ContentAddressedFetcher::fetch_verified)), so a
/// page whose URL merely LOOKS content-addressed but was not actually verified is
/// never reported as verified. Anything served straight to the engine (the
/// ordinary `http(s)://` path) is [`UnverifiedOrigin`](TrustPosture::UnverifiedOrigin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrustPosture {
    /// The page was served by an origin that is not trusted by default: the
    /// ordinary server-web load path, where verification did NOT move to a hash.
    /// This is the DEFAULT posture — werust does not trust the origin unless the
    /// bytes were actually content-verified, so anything not proven verified is
    /// reported here.
    #[default]
    UnverifiedOrigin,
    /// The page was content-verified: its bytes were fetched on the
    /// content-addressed path and hash-checked against their content identifier
    /// before rendering (the `ipfs://` trust hook resolving through the verified
    /// `Fetcher` path). Verification moved to the hash, so the origin need not be
    /// trusted.
    ContentVerified,
    /// The page's BYTES were content-verified (hash-checked on the
    /// content-addressed path, exactly like [`ContentVerified`](TrustPosture::ContentVerified)),
    /// but the NAME->CID mapping that chose which content to load was taken on a
    /// TRUSTED RPC's word: an ENS name resolved to its contenthash over a trusted
    /// (non-light-client) Ethereum RPC. So the RPC could have MISDIRECTED the name
    /// to a different (still valid, still hash-consistent) CID.
    ///
    /// This is the honest Phase-1 middle state (`ens-to-ipfs-resolution-phase1-rpc-skeleton`):
    /// a page here is NEITHER "verified" (Phase 1 has no light client, so the
    /// name is NOT verified) NOR merely "served" (the bytes DID hash-verify). It
    /// MUST never be surfaced as "verified"/"name-verified"; the name-verified
    /// state is a Phase-2 addition once a light client removes the RPC trust.
    ///
    /// Like [`ContentVerified`](TrustPosture::ContentVerified) it tracks the ACTUAL
    /// load path: it is set ONLY when a load genuinely went through ENS
    /// trusted-RPC resolution, never inferred from a `.eth`-looking URL.
    NameViaTrustedRpc,
}

impl TrustPosture {
    /// Whether the current page was content-verified (its bytes hash-checked on
    /// the content-addressed path), as opposed to merely served.
    #[must_use]
    pub fn is_content_verified(self) -> bool {
        matches!(self, TrustPosture::ContentVerified)
    }

    /// Whether the current page's bytes were content-verified but its name->CID
    /// mapping came from a TRUSTED RPC (an ENS-resolved Phase-1 page). This is a
    /// distinct middle state: it is NOT [`is_content_verified`](TrustPosture::is_content_verified)
    /// (there is no "verified" claim without a light client) and NOT the plain
    /// unverified origin (the bytes did hash-verify).
    #[must_use]
    pub fn is_name_via_trusted_rpc(self) -> bool {
        matches!(self, TrustPosture::NameViaTrustedRpc)
    }
}

/// A load-lifecycle event emitted by a backend as a load progresses.
///
/// Backends push these as the underlying engine reports progress; the browser
/// pulls them with [`Renderer::poll_event`] and updates chrome (URL bar, spinner,
/// the content-verified vs served-origin indicator, …) from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadEvent {
    /// A navigation to `url` started.
    Started { url: String },
    /// The load committed on `url` (the effective URL after any redirects).
    Committed { url: String },
    /// The load of `url` finished successfully.
    Finished { url: String },
    /// The load of `url` failed, with a human-readable reason.
    Failed { url: String, reason: String },
}

/// A message sent from an injected page script up to the browser over the
/// script-message bridge.
///
/// This is the channel the EIP-1193 provider shim uses to forward RPC requests
/// from the page to the native provider (task
/// `eip1193-provider-injection-via-script-bridge`). Declared here as part of the
/// seam surface; a backend routes these from its script-message handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptMessage {
    /// The bridge name the page posted to (e.g. the provider channel).
    pub handler: String,
    /// The raw message body (typically a JSON string).
    pub body: String,
}

/// A request a backend intercepted for a custom scheme (e.g. `ipfs://`).
///
/// The request-interception / custom-scheme hook hands the browser the intercepted
/// request; the browser resolves it (for `ipfs://`: hash-verified content-addressed
/// fetch — task `ipfs-scheme-resolution-through-renderer-seam`) and answers with a
/// [`SchemeResponse`]. Declared here as part of the seam surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeRequest {
    /// The full URI of the intercepted request (e.g. `ipfs://<cid>/index.html`).
    pub uri: String,
}

/// The browser's answer to an intercepted [`SchemeRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeResponse {
    /// The MIME type of the response body.
    pub mime_type: String,
    /// The response body bytes.
    pub body: Vec<u8>,
}

/// A pointer input event forwarded to the live view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// X position in view-local logical pixels.
    pub x: f64,
    /// Y position in view-local logical pixels.
    pub y: f64,
    /// The kind of pointer action.
    pub kind: PointerKind,
}

/// The kind of a [`PointerEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    /// The pointer moved.
    Moved,
    /// A button went down.
    Pressed,
    /// A button was released.
    Released,
}

/// A keyboard input event forwarded to the live view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// A logical key name (backend-neutral, e.g. `"Enter"`, `"a"`).
    pub key: String,
    /// Whether this is a key-down (`true`) or key-up (`false`).
    pub pressed: bool,
}

/// A scroll delta forwarded to the live view, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollDelta {
    /// Horizontal scroll amount.
    pub dx: f64,
    /// Vertical scroll amount.
    pub dy: f64,
}

/// An opaque handle to a backend's live, interactive native view.
///
/// The shell embeds this in its window; the browser never renders the page itself.
/// The concrete pointer meaning is backend-specific (a `GtkWidget*` for WebKitGTK,
/// a native view for a future backend), so it is carried opaquely and must not be
/// interpreted past the platform edge that embeds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewHandle(pub *mut std::ffi::c_void);

// A raw view pointer is only ever handed to the single-threaded UI toolkit that
// owns it; carrying it across the seam does not itself make it shared.
unsafe impl Send for ViewHandle {}

/// An error from a [`Renderer`] operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererError {
    /// The requested URL was malformed or used an unsupported scheme.
    InvalidUrl(String),
    /// The backend failed to carry out the operation.
    Backend(String),
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::InvalidUrl(u) => write!(f, "invalid or unsupported url: {u}"),
            RendererError::Backend(m) => write!(f, "renderer backend error: {m}"),
        }
    }
}

impl std::error::Error for RendererError {}

/// One of the two **trust hooks** a backend must be able to satisfy to qualify.
///
/// These are the concrete seam capabilities that carry the thesis (`docs/adr/0001`,
/// `CONTEXT.md`): a backend that renders beautifully but cannot expose EITHER of
/// these is not a real backend for werust. They are checked as a pass/fail
/// qualifying set (see [`TrustHooks`] and [`qualify`]), not graded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrustHook {
    /// EIP-1193 provider injection over the script-message bridge
    /// ([`register_script_message_handler`](Renderer::register_script_message_handler)
    /// with [`inject_script`](Renderer::inject_script)) — task
    /// `eip1193-provider-injection-via-script-bridge`.
    ProviderInjection,
    /// `ipfs://` custom-scheme / request-interception resolution
    /// ([`register_scheme_handler`](Renderer::register_scheme_handler)) — task
    /// `ipfs-scheme-resolution-through-renderer-seam`.
    IpfsScheme,
}

impl TrustHook {
    /// Every trust hook a qualifying backend must satisfy, in a stable order.
    ///
    /// This is the single source of truth for "which hooks qualify a backend":
    /// [`TrustHooks::all`], [`TrustHooks::is_qualifying`], and [`qualify`] are all
    /// defined against it, so adding a future trust hook here tightens the gate
    /// everywhere at once.
    pub const ALL: [TrustHook; 2] = [TrustHook::ProviderInjection, TrustHook::IpfsScheme];
}

impl fmt::Display for TrustHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustHook::ProviderInjection => write!(f, "EIP-1193 provider injection"),
            TrustHook::IpfsScheme => write!(f, "ipfs:// custom-scheme resolution"),
        }
    }
}

/// The set of [`TrustHook`]s a backend DECLARES it can satisfy.
///
/// This is the checkable capability a backend reports through
/// [`Renderer::trust_hooks`]; the [`qualify`] gate accepts a backend ONLY when
/// this set covers every [`TrustHook`]. It is a value (not a comment), so "a
/// backend qualifies only if it satisfies the trust hooks" is an enforced,
/// testable property rather than documentation — the same gate qualifies the
/// webview now and the native renderer later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustHooks {
    provider_injection: bool,
    ipfs_scheme: bool,
}

impl TrustHooks {
    /// A backend that declares BOTH trust hooks — the qualifying set.
    #[must_use]
    pub const fn all() -> Self {
        TrustHooks {
            provider_injection: true,
            ipfs_scheme: true,
        }
    }

    /// A backend that declares NO trust hook — a render-only backend.
    #[must_use]
    pub const fn none() -> Self {
        TrustHooks {
            provider_injection: false,
            ipfs_scheme: false,
        }
    }

    /// The set declaring exactly the one given [`TrustHook`].
    #[must_use]
    pub const fn with(hook: TrustHook) -> Self {
        let mut set = TrustHooks::none();
        set = set.and(hook);
        set
    }

    /// The same set, additionally declaring `hook`.
    #[must_use]
    pub const fn and(mut self, hook: TrustHook) -> Self {
        match hook {
            TrustHook::ProviderInjection => self.provider_injection = true,
            TrustHook::IpfsScheme => self.ipfs_scheme = true,
        }
        self
    }

    /// Whether this set declares `hook`.
    #[must_use]
    pub const fn contains(&self, hook: TrustHook) -> bool {
        match hook {
            TrustHook::ProviderInjection => self.provider_injection,
            TrustHook::IpfsScheme => self.ipfs_scheme,
        }
    }

    /// Whether this set covers EVERY trust hook — i.e. the backend qualifies.
    #[must_use]
    pub fn is_qualifying(&self) -> bool {
        TrustHook::ALL.iter().all(|h| self.contains(*h))
    }

    /// The trust hooks NOT declared, in [`TrustHook::ALL`] order (empty iff the
    /// set qualifies).
    #[must_use]
    pub fn missing(&self) -> Vec<TrustHook> {
        TrustHook::ALL
            .into_iter()
            .filter(|h| !self.contains(*h))
            .collect()
    }
}

/// A backend that declares NO trust hook by default.
///
/// The DEFAULT is FAIL-CLOSED: a backend declares NOTHING until it opts in, so
/// trust is never inherited by omission. A backend that stubs the hook methods
/// (as every `Renderer` impl must) but does not say what it actually wires is
/// treated as satisfying no hook — and is therefore rejected by [`qualify`].
/// Trust must be OPTED INTO, hook by hook: a backend that genuinely wires a hook
/// declares it (via [`Renderer::trust_hooks`]), and only then does the gate
/// accept it. (Wiring real behaviour onto the hooks, and asserting that
/// behaviour, is the sibling provider/ipfs tasks' job.)
impl Default for TrustHooks {
    fn default() -> Self {
        TrustHooks::none()
    }
}

/// The reason a backend failed the trust-hook [`qualify`] gate.
///
/// Carries the exact [`TrustHook`]s the backend did not declare, so the caller
/// (the seam's own conformance test now; the native-renderer benchmark harness
/// later) can report precisely why a render-only backend was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disqualified {
    /// The trust hooks the backend did not declare, in [`TrustHook::ALL`] order.
    pub missing: Vec<TrustHook>,
}

impl fmt::Display for Disqualified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "backend does not satisfy the trust hooks:")?;
        for (i, hook) in self.missing.iter().enumerate() {
            let sep = if i == 0 { " " } else { ", " };
            write!(f, "{sep}{hook}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Disqualified {}

/// The trust-hook qualification GATE: accept a backend ONLY if it satisfies both
/// trust hooks.
///
/// This is the enforced form of the seam's qualifying rule (`CONTEXT.md`,
/// `docs/adr/0001`): a backend qualifies as *real* for werust only if it can
/// satisfy the trust hooks, not merely if it renders well. A qualifying backend
/// returns `Ok(())`; a render-only backend is rejected with a [`Disqualified`]
/// naming exactly the hooks it does not declare. The webview backend passes this
/// gate today; the native renderer will be held to the SAME gate later, and the
/// benchmark harness reuses it as its pass/fail trust-hook check.
pub fn qualify(backend: &dyn Renderer) -> Result<(), Disqualified> {
    let declared = backend.trust_hooks();
    if declared.is_qualifying() {
        Ok(())
    } else {
        Err(Disqualified {
            missing: declared.missing(),
        })
    }
}

/// A callback invoked with a [`ScriptMessage`] posted by an injected page script.
pub type ScriptMessageHandler = Box<dyn FnMut(ScriptMessage) + Send>;

/// A callback invoked to resolve an intercepted [`SchemeRequest`] for a custom
/// scheme, returning the body to hand back to the engine.
pub type SchemeHandler =
    Box<dyn FnMut(SchemeRequest) -> Result<SchemeResponse, RendererError> + Send>;

/// The wide, hot-swappable rendering-backend interface.
///
/// Every rendering backend (the WebKitGTK webview first; a native Rust renderer
/// later) implements this trait, and the rest of the browser drives rendering
/// ONLY through it. The surface is intentionally wide — it declares the trust
/// hooks (script-message bridge + custom-scheme interception) alongside plain
/// navigation — because a backend qualifies as *real* only if it can satisfy the
/// trust hooks, not merely if it renders well (`CONTEXT.md`).
pub trait Renderer {
    /// Start navigating the view to `url`.
    ///
    /// Kicks off the load lifecycle: on success the backend begins emitting
    /// [`LoadEvent`]s and [`load_state`](Renderer::load_state) leaves
    /// [`LoadState::Idle`]. An unusable URL is rejected with
    /// [`RendererError::InvalidUrl`] and does not start a load.
    fn navigate(&mut self, url: &str) -> Result<(), RendererError>;

    /// Reload the current page.
    fn reload(&mut self) -> Result<(), RendererError>;

    /// Stop the in-flight load, if any, returning the view to a settled state.
    fn stop(&mut self);

    /// Navigate one step back in the session history, if possible.
    ///
    /// The session history is the backend's, not the browser's: a real webview
    /// keeps the back/forward list itself (as does a future native backend that
    /// owns navigation), so the shell drives history THROUGH the seam rather than
    /// tracking a URL stack of its own. A back navigation restarts the load
    /// lifecycle (a fresh [`LoadEvent::Started`]) exactly like [`navigate`]. When
    /// [`can_go_back`](Renderer::can_go_back) is `false` this is a no-op.
    ///
    /// The provided default is a no-op, for the fixed-subset backends that have no
    /// session history yet; a backend with real history (the webview) overrides
    /// it. [`navigate`]: Renderer::navigate
    fn go_back(&mut self) {}

    /// Navigate one step forward in the session history, if possible.
    ///
    /// The mirror of [`go_back`](Renderer::go_back): forward is only available
    /// after a back navigation left a forward entry. Restarts the load lifecycle
    /// like [`navigate`](Renderer::navigate); a no-op when
    /// [`can_go_forward`](Renderer::can_go_forward) is `false`. Defaults to a
    /// no-op for backends without session history.
    fn go_forward(&mut self) {}

    /// Whether a back navigation is currently possible.
    ///
    /// This is the checkable half of the back control: the shell greys out its
    /// Back button when this is `false`. Defaults to `false` for backends without
    /// session history.
    fn can_go_back(&self) -> bool {
        false
    }

    /// Whether a forward navigation is currently possible.
    ///
    /// The checkable half of the forward control (see
    /// [`can_go_back`](Renderer::can_go_back)). Defaults to `false`.
    fn can_go_forward(&self) -> bool {
        false
    }

    /// The current load-lifecycle state.
    fn load_state(&self) -> LoadState;

    /// The URL of the current (committed or in-flight) load, if any.
    ///
    /// Returned by value, not borrowed: a real event-driven backend (the
    /// WebKitGTK webview) keeps its load state behind interior mutability so its
    /// load-lifecycle signals can update it, and cannot lend a borrow out from
    /// there. Owning the returned `String` keeps the seam implementable by such
    /// backends (see the `webview-renderer` crate).
    fn current_url(&self) -> Option<String>;

    /// Pull the next pending [`LoadEvent`], or `None` if the queue is empty.
    ///
    /// The browser drains these to drive its chrome. Backends that own a native
    /// main loop enqueue events from their load-lifecycle signals.
    fn poll_event(&mut self) -> Option<LoadEvent>;

    /// An opaque handle to the live, interactive native view to embed in a window.
    fn view_handle(&self) -> ViewHandle;

    /// Forward a pointer (mouse/touch) event to the live view.
    fn send_pointer(&mut self, event: PointerEvent);

    /// Forward a keyboard event to the live view.
    fn send_key(&mut self, event: KeyEvent);

    /// Forward a scroll delta to the live view.
    fn send_scroll(&mut self, delta: ScrollDelta);

    /// Give (`true`) or take (`false`) keyboard focus of the live view.
    fn set_focus(&mut self, focused: bool);

    /// Register a script-message bridge handler under `name`.
    ///
    /// Pages post to `window.webkit.messageHandlers.<name>` (or the backend's
    /// equivalent) and the `handler` receives each [`ScriptMessage`]. This is the
    /// channel the EIP-1193 provider is injected over (task
    /// `eip1193-provider-injection-via-script-bridge`).
    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler);

    /// Inject `script` to run in every page loaded from now on (at document start).
    ///
    /// Used to install the provider shim that talks back over the script-message
    /// bridge. Part of the trust-hook surface.
    fn inject_script(&mut self, script: &str);

    /// Evaluate `script` in the CURRENT live page, now (browser -> page).
    ///
    /// This is the RESPONSE half of the script-message bridge:
    /// [`register_script_message_handler`](Renderer::register_script_message_handler)
    /// carries a message page -> browser, and this pushes a result back browser ->
    /// page by evaluating JS in the live document. It is what lets the EIP-1193
    /// provider round-trip: the page's `request(...)` posts up the bridge, the
    /// native handler answers, and the answer is delivered back into the page's
    /// pending Promise by evaluating a small settle-call here (task
    /// `eip1193-provider-injection-via-script-bridge`).
    ///
    /// Fire-and-forget: evaluation is asynchronous on the backend's own loop, and
    /// takes `&self` because a real event-driven backend (the webview) evaluates
    /// through an interior-mutable handle. A backend with no live document (a
    /// fixed-subset native path with no JS) may leave the provided no-op default.
    fn evaluate_javascript(&self, _script: &str) {}

    /// Register a custom-scheme / request-interception handler for `scheme`.
    ///
    /// Requests to `<scheme>://…` are handed to `handler`, which resolves them
    /// (for `ipfs://`: a hash-verified content-addressed fetch — task
    /// `ipfs-scheme-resolution-through-renderer-seam`). Part of the trust-hook
    /// surface: a backend that cannot intercept a custom scheme is not a real
    /// backend for werust.
    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler);

    /// Which [`TrustHook`]s this backend can actually satisfy.
    ///
    /// This is the CHECKABLE half of the qualifying rule. The hook methods above
    /// are structural (every `Renderer` impl has them, and a render-only backend
    /// can stub them), so structural presence alone does not prove a backend can
    /// satisfy the trust hooks. A backend reports its real capability here, and
    /// the [`qualify`] gate accepts it ONLY if this set covers both hooks — making
    /// "a backend qualifies only if it satisfies the trust hooks" an enforced seam
    /// property, not a comment.
    ///
    /// The provided default is FAIL-CLOSED: it declares NO hook, so a backend
    /// that does not override this OPTS INTO nothing and is rejected by
    /// [`qualify`]. Trust is never inherited by omission — a backend qualifies
    /// ONLY if it EXPLICITLY declares the trust hooks it actually wires. A
    /// backend that genuinely wires both (the webview, wired by the sibling
    /// provider/ipfs tasks) OVERRIDES this to return [`TrustHooks::all`]; one
    /// that wires only some declares only those; a render-only backend leaves
    /// the default and is disqualified.
    fn trust_hooks(&self) -> TrustHooks {
        TrustHooks::none()
    }

    /// The [`TrustPosture`] of the CURRENT load: content-verified vs served by an
    /// unverified origin.
    ///
    /// This is the checkable seam fact behind the chrome's trust indicator
    /// (`docs/adr/0001`: the trust posture is a product surface, not a silent
    /// internal). The shell reads it into its chrome exactly as it reads
    /// [`load_state`](Renderer::load_state), and the window paints the indicator
    /// from it. It MUST reflect the ACTUAL load path — a backend reports
    /// [`TrustPosture::ContentVerified`] only when the current page's bytes came
    /// back through the hash-verified content-addressed path, NOT because the URL
    /// happened to start with `ipfs://`.
    ///
    /// The provided default is [`TrustPosture::UnverifiedOrigin`]: a backend that
    /// has no verified content-addressed path (a plain renderer, a fixed-subset
    /// native path) serves ordinary origins, so it reports the untrusted posture
    /// unless it overrides this to track real verification. werust does not trust
    /// the origin unless the bytes were proven verified, so "not proven verified"
    /// defaults to unverified.
    fn trust_posture(&self) -> TrustPosture {
        TrustPosture::UnverifiedOrigin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-memory backend that drives the load lifecycle deterministically
    /// so the SEAM contract can be exercised at the trait level, without a GTK main
    /// loop or a display. It is NOT a rendering backend — it exists only to test
    /// that navigate/stop drive [`LoadState`] and emit the matching [`LoadEvent`]s
    /// the way any real backend must.
    struct FakeBackend {
        state: LoadState,
        url: Option<String>,
        events: std::collections::VecDeque<LoadEvent>,
        scheme_handlers: Vec<String>,
        script_handlers: Vec<String>,
        injected: Vec<String>,
        // Which trust hooks this backend declares it can satisfy. Defaults to all
        // (it models the real qualifying backend, which explicitly wires both
        // hooks); tests override it to model partial or no capability. NOTE: this
        // is NOT `TrustHooks::default()` (which is fail-closed `none()`) — this
        // double stands in for a backend that has OPTED INTO both hooks, so it
        // declares them explicitly, exactly as the real `WebViewRenderer` does.
        declares_hooks: TrustHooks,
        // The trust posture of the current load. Defaults to the untrusted origin;
        // tests set it to model a content-verified load path.
        posture: TrustPosture,
    }

    impl Default for FakeBackend {
        fn default() -> Self {
            FakeBackend {
                state: LoadState::default(),
                url: None,
                events: std::collections::VecDeque::new(),
                scheme_handlers: Vec::new(),
                script_handlers: Vec::new(),
                injected: Vec::new(),
                // A qualifying backend explicitly declares both hooks (fail-closed
                // default means omission would disqualify it).
                declares_hooks: TrustHooks::all(),
                posture: TrustPosture::default(),
            }
        }
    }

    impl Renderer for FakeBackend {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(RendererError::InvalidUrl(url.to_string()));
            }
            self.url = Some(url.to_string());
            self.state = LoadState::Started;
            self.events.push_back(LoadEvent::Started {
                url: url.to_string(),
            });
            Ok(())
        }

        fn reload(&mut self) -> Result<(), RendererError> {
            let url = self
                .url
                .clone()
                .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?;
            self.navigate(&url)
        }

        fn stop(&mut self) {
            self.state = LoadState::Idle;
        }

        fn load_state(&self) -> LoadState {
            self.state
        }

        fn current_url(&self) -> Option<String> {
            self.url.clone()
        }

        fn poll_event(&mut self) -> Option<LoadEvent> {
            self.events.pop_front()
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

        fn trust_hooks(&self) -> TrustHooks {
            self.declares_hooks
        }

        fn trust_posture(&self) -> TrustPosture {
            self.posture
        }
    }

    impl FakeBackend {
        /// Simulate the backend's native signals advancing the load to done.
        fn drive_to_finished(&mut self) {
            let url = self.url.clone().expect("a load in flight");
            self.state = LoadState::Committed;
            self.events
                .push_back(LoadEvent::Committed { url: url.clone() });
            self.state = LoadState::Finished;
            self.events.push_back(LoadEvent::Finished { url });
        }
    }

    #[test]
    fn navigate_transitions_load_lifecycle_state() {
        let mut r = FakeBackend::default();
        assert_eq!(r.load_state(), LoadState::Idle);
        assert!(!r.load_state().is_loading());

        r.navigate("https://example.com/").expect("valid https url");
        assert_eq!(r.load_state(), LoadState::Started);
        assert!(r.load_state().is_loading());
        assert_eq!(r.current_url().as_deref(), Some("https://example.com/"));

        // The Started transition emits a matching lifecycle event.
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Started {
                url: "https://example.com/".into()
            })
        );

        // Native signals carry it to Finished, emitting Committed then Finished.
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
        let mut r = FakeBackend::default();
        let err = r.navigate("not-a-url").expect_err("unusable url rejected");
        assert_eq!(err, RendererError::InvalidUrl("not-a-url".into()));
        // A rejected navigation does not move the lifecycle off Idle.
        assert_eq!(r.load_state(), LoadState::Idle);
        assert_eq!(r.current_url(), None);
        assert_eq!(r.poll_event(), None);
    }

    #[test]
    fn stop_returns_lifecycle_to_settled() {
        let mut r = FakeBackend::default();
        r.navigate("https://example.com/").unwrap();
        assert!(r.load_state().is_loading());
        r.stop();
        assert_eq!(r.load_state(), LoadState::Idle);
    }

    #[test]
    fn history_navigation_defaults_to_a_no_history_backend() {
        // The session-history methods are part of the seam so the shell can drive
        // back/forward THROUGH it, but they carry a no-op default so a backend
        // without session history (a fixed-subset native path) is not forced to
        // fake one: it simply reports no back/forward is possible, and the
        // go_back/go_forward calls do nothing.
        let mut r = FakeBackend::default();
        r.navigate("https://example.com/").unwrap();
        assert!(!r.can_go_back(), "a no-history backend can never go back");
        assert!(!r.can_go_forward());
        // Driving history on such a backend is a harmless no-op: it does not move
        // the lifecycle off the current load.
        r.go_back();
        r.go_forward();
        assert_eq!(r.current_url().as_deref(), Some("https://example.com/"));
    }

    #[test]
    fn trust_hooks_are_part_of_the_seam() {
        // A backend qualifies only if it exposes the trust hooks. Here we assert
        // the seam surface accepts a script-message handler, an injected script,
        // and a custom-scheme handler — the shape the provider and ipfs:// tasks
        // wire real behaviour onto.
        let mut r = FakeBackend::default();
        r.register_script_message_handler("werustProvider", Box::new(|_msg| {}));
        r.inject_script("globalThis.ethereum = {};");
        r.register_scheme_handler(
            "ipfs",
            Box::new(|req| {
                Ok(SchemeResponse {
                    mime_type: "text/html".into(),
                    body: format!("resolved {}", req.uri).into_bytes(),
                })
            }),
        );
        assert_eq!(r.script_handlers, ["werustProvider"]);
        assert_eq!(r.injected, ["globalThis.ethereum = {};"]);
        assert_eq!(r.scheme_handlers, ["ipfs"]);
    }

    /// A backend that renders but declares NO trust-hook capability: it stubs the
    /// hook methods (they compile, as any `Renderer` impl must) yet reports it
    /// cannot actually satisfy either trust hook. This is the "renders well but
    /// cannot satisfy the thesis" case the qualification gate must reject.
    #[derive(Default)]
    struct RenderOnlyBackend {
        state: LoadState,
        url: Option<String>,
    }

    impl Renderer for RenderOnlyBackend {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            self.url = Some(url.to_string());
            self.state = LoadState::Started;
            Ok(())
        }
        fn reload(&mut self) -> Result<(), RendererError> {
            Ok(())
        }
        fn stop(&mut self) {
            self.state = LoadState::Idle;
        }
        fn load_state(&self) -> LoadState {
            self.state
        }
        fn current_url(&self) -> Option<String> {
            self.url.clone()
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
        fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {
        }
        fn inject_script(&mut self, _script: &str) {}
        fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {}

        // The whole point: this backend renders, but cannot satisfy the trust
        // hooks, so it declares NO trust-hook capability.
        fn trust_hooks(&self) -> TrustHooks {
            TrustHooks::none()
        }
    }

    #[test]
    fn qualification_gate_accepts_a_backend_that_declares_both_trust_hooks() {
        // A backend that declares both trust hooks (provider injection + ipfs://
        // scheme) QUALIFIES: it is a real backend for werust, not just a renderer.
        let backend = FakeBackend::default();
        assert_eq!(
            backend.trust_hooks(),
            TrustHooks::all(),
            "a qualifying backend declares both trust hooks"
        );
        qualify(&backend).expect("a backend declaring both trust hooks qualifies");
    }

    #[test]
    fn qualification_gate_rejects_a_render_only_backend() {
        // A backend that renders but declares neither trust hook is DISQUALIFIED,
        // naming BOTH missing hooks — the enforced seam property.
        let backend = RenderOnlyBackend::default();
        let err = qualify(&backend).expect_err("a render-only backend is rejected");
        assert_eq!(
            err.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme],
            "both trust hooks are reported missing"
        );
    }

    /// A backend that renders but does NOT override [`Renderer::trust_hooks`] at
    /// all — it relies on the seam's provided default. Under the FAIL-CLOSED
    /// default this backend has opted into no hook, so it must be disqualified:
    /// trust is never inherited by omission. This is the inverse of the old
    /// fail-open behaviour, where such a backend silently qualified.
    #[derive(Default)]
    struct DefaultRelyingBackend {
        state: LoadState,
        url: Option<String>,
    }

    impl Renderer for DefaultRelyingBackend {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            self.url = Some(url.to_string());
            self.state = LoadState::Started;
            Ok(())
        }
        fn reload(&mut self) -> Result<(), RendererError> {
            Ok(())
        }
        fn stop(&mut self) {
            self.state = LoadState::Idle;
        }
        fn load_state(&self) -> LoadState {
            self.state
        }
        fn current_url(&self) -> Option<String> {
            self.url.clone()
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
        fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {
        }
        fn inject_script(&mut self, _script: &str) {}
        fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {}
        // NOTE: no `trust_hooks` override — this backend relies on the seam
        // default, which is what this test exists to pin as fail-closed.
    }

    #[test]
    fn qualification_gate_rejects_a_backend_relying_on_the_default() {
        // FAIL-CLOSED: a backend that does NOT override `trust_hooks` inherits the
        // seam default, which now declares NO hook — so it is DISQUALIFIED, naming
        // both missing hooks. Trust must be opted into, never inherited by
        // omission; a stub-but-undeclared backend no longer silently qualifies.
        let backend = DefaultRelyingBackend::default();
        assert_eq!(
            backend.trust_hooks(),
            TrustHooks::none(),
            "the fail-closed default declares no trust hook"
        );
        let err = qualify(&backend)
            .expect_err("a backend relying on the fail-closed default is disqualified");
        assert_eq!(
            err.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme],
            "an un-declared backend is missing BOTH hooks under the fail-closed default"
        );
    }

    #[test]
    fn trust_hooks_default_is_fail_closed() {
        // The seam's `TrustHooks::default()` is fail-closed: it declares nothing,
        // so it does not qualify. This is the value backing the provided
        // `Renderer::trust_hooks` default — pinning it here guards the flip.
        assert_eq!(TrustHooks::default(), TrustHooks::none());
        assert!(!TrustHooks::default().is_qualifying());
    }

    #[test]
    fn qualification_gate_rejects_a_backend_missing_only_one_hook() {
        // Satisfying ONE trust hook is not enough: a backend must satisfy BOTH to
        // qualify. Missing exactly the ipfs:// scheme is still a disqualification
        // that names precisely the missing hook.
        let backend = FakeBackend {
            declares_hooks: TrustHooks::with(TrustHook::ProviderInjection),
            ..FakeBackend::default()
        };
        let err = qualify(&backend).expect_err("one hook is not enough to qualify");
        assert_eq!(err.missing, vec![TrustHook::IpfsScheme]);
    }

    #[test]
    fn trust_hooks_capability_set_reports_membership() {
        // The capability set is a checkable value: contains/all/none behave as a
        // set of the two trust hooks.
        let both = TrustHooks::all();
        assert!(both.contains(TrustHook::ProviderInjection));
        assert!(both.contains(TrustHook::IpfsScheme));
        assert!(both.is_qualifying());

        let none = TrustHooks::none();
        assert!(!none.contains(TrustHook::ProviderInjection));
        assert!(!none.is_qualifying());

        let one = TrustHooks::with(TrustHook::ProviderInjection);
        assert!(one.contains(TrustHook::ProviderInjection));
        assert!(!one.contains(TrustHook::IpfsScheme));
        assert!(!one.is_qualifying());
    }

    #[test]
    fn trust_posture_defaults_to_the_unverified_origin() {
        // A backend with no verified content-addressed path serves ordinary
        // origins, so the seam default posture is the untrusted one: werust does
        // not trust the origin unless the bytes were proven content-verified.
        assert_eq!(TrustPosture::default(), TrustPosture::UnverifiedOrigin);
        assert!(!TrustPosture::default().is_content_verified());
        // The seam's provided `trust_posture` default is the untrusted posture,
        // so a backend that does not track verification is never mislabelled as
        // verified.
        struct BareBackend;
        impl Renderer for BareBackend {
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
        }
        assert_eq!(BareBackend.trust_posture(), TrustPosture::UnverifiedOrigin);
    }

    #[test]
    fn a_backend_reports_the_content_verified_posture_through_the_seam() {
        // The seam carries the trust posture as a first-class fact: a backend that
        // served the current page through its verified content-addressed path
        // reports `ContentVerified`, which the shell reads into its chrome exactly
        // as it reads `load_state`.
        let verified = FakeBackend {
            posture: TrustPosture::ContentVerified,
            ..FakeBackend::default()
        };
        assert_eq!(verified.trust_posture(), TrustPosture::ContentVerified);
        assert!(verified.trust_posture().is_content_verified());

        // A plain served load reports the untrusted origin posture.
        let served = FakeBackend::default();
        assert_eq!(served.trust_posture(), TrustPosture::UnverifiedOrigin);
        assert!(!served.trust_posture().is_content_verified());
    }

    #[test]
    fn the_name_via_trusted_rpc_posture_is_a_distinct_middle_state() {
        // Acceptance: the ENS-resolved Phase-1 posture is a THIRD state, separate
        // from both `ContentVerified` and `UnverifiedOrigin`. Its bytes verified
        // but its name came from a trusted RPC, so it is honestly NOT "verified":
        // `is_content_verified()` is false, and it is its own predicate.
        let name_via_rpc = FakeBackend {
            posture: TrustPosture::NameViaTrustedRpc,
            ..FakeBackend::default()
        };
        assert_eq!(
            name_via_rpc.trust_posture(),
            TrustPosture::NameViaTrustedRpc
        );
        assert!(name_via_rpc.trust_posture().is_name_via_trusted_rpc());
        // It is NEVER surfaced as verified: Phase 1 makes no name-verification
        // claim (there is no light client).
        assert!(!name_via_rpc.trust_posture().is_content_verified());

        // It is distinct from the other two postures.
        assert_ne!(
            TrustPosture::NameViaTrustedRpc,
            TrustPosture::ContentVerified
        );
        assert_ne!(
            TrustPosture::NameViaTrustedRpc,
            TrustPosture::UnverifiedOrigin
        );
        // And the other two are not this one.
        assert!(!TrustPosture::ContentVerified.is_name_via_trusted_rpc());
        assert!(!TrustPosture::UnverifiedOrigin.is_name_via_trusted_rpc());
    }
}
