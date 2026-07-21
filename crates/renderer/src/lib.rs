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

    /// Register a custom-scheme / request-interception handler for `scheme`.
    ///
    /// Requests to `<scheme>://…` are handed to `handler`, which resolves them
    /// (for `ipfs://`: a hash-verified content-addressed fetch — task
    /// `ipfs-scheme-resolution-through-renderer-seam`). Part of the trust-hook
    /// surface: a backend that cannot intercept a custom scheme is not a real
    /// backend for werust.
    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-memory backend that drives the load lifecycle deterministically
    /// so the SEAM contract can be exercised at the trait level, without a GTK main
    /// loop or a display. It is NOT a rendering backend — it exists only to test
    /// that navigate/stop drive [`LoadState`] and emit the matching [`LoadEvent`]s
    /// the way any real backend must.
    #[derive(Default)]
    struct FakeBackend {
        state: LoadState,
        url: Option<String>,
        events: std::collections::VecDeque<LoadEvent>,
        scheme_handlers: Vec<String>,
        script_handlers: Vec<String>,
        injected: Vec<String>,
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
}
