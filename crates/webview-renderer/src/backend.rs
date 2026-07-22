//! The real WebKitGTK backend: a [`webkit6::WebView`] wired behind the
//! [`Renderer`] seam.
//!
//! [`WebViewRenderer`] is the piece that actually shows a page in a window on
//! Linux. It binds WebKitGTK (`webkit6` over GTK4) rather than hand-rolling a
//! renderer, and connects the webview's native load-lifecycle signals to the
//! GTK-free [`LoadLifecycle`](crate::LoadLifecycle) so the browser sees the same
//! [`LoadState`]/[`LoadEvent`] surface it sees from any backend. Nothing in here
//! is exposed above the seam: callers only ever hold a `dyn Renderer`.

use std::ffi::c_void;

use gtk4::glib;
use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{
    LoadEvent as WkLoadEvent, UserContentInjectedFrames, UserContentManager, UserScript,
    UserScriptInjectionTime, WebContext, WebView,
};

use renderer::{
    KeyEvent, LoadEvent, LoadState, PointerEvent, Renderer, RendererError, SchemeHandler,
    ScriptMessageHandler, ScrollDelta, TrustHooks, ViewHandle,
};

use crate::{validate_url, LoadLifecycle, SharedLifecycle};

/// A [`Renderer`] backed by a WebKitGTK system webview.
///
/// Construct with [`WebViewRenderer::new`]; embed [`view_handle`] in a GTK
/// window and drive the GTK main loop to see pages render. The webview's
/// `load-changed` / `load-failed` signals feed a shared
/// [`LoadLifecycle`](crate::LoadLifecycle), so [`load_state`], [`current_url`],
/// and [`poll_event`] report the same load-lifecycle surface as any other
/// backend.
///
/// [`view_handle`]: Renderer::view_handle
/// [`load_state`]: Renderer::load_state
/// [`current_url`]: Renderer::current_url
/// [`poll_event`]: Renderer::poll_event
pub struct WebViewRenderer {
    view: WebView,
    content_manager: UserContentManager,
    life: SharedLifecycle,
}

impl WebViewRenderer {
    /// Create a webview backend, initializing GTK if needed.
    ///
    /// Fails with [`RendererError::Backend`] if GTK cannot be initialized (e.g.
    /// no display is available). On success the returned renderer owns a live
    /// [`webkit6::WebView`] whose load signals are already wired to drive the
    /// seam's [`LoadState`]/[`LoadEvent`] surface.
    pub fn new() -> Result<Self, RendererError> {
        gtk4::init().map_err(|e| RendererError::Backend(format!("gtk init failed: {e}")))?;

        let content_manager = UserContentManager::new();
        let context = WebContext::new();
        let view = WebView::builder()
            .user_content_manager(&content_manager)
            .web_context(&context)
            .build();

        let life: SharedLifecycle =
            SharedLifecycle::new(std::cell::RefCell::new(LoadLifecycle::default()));

        Self::connect_load_signals(&view, &life);

        Ok(Self {
            view,
            content_manager,
            life,
        })
    }

    /// Wire the webview's native load-lifecycle signals to the shared
    /// [`LoadLifecycle`], so WebKitGTK's progress drives the seam's state.
    fn connect_load_signals(view: &WebView, life: &SharedLifecycle) {
        let life_changed = life.clone();
        view.connect_load_changed(move |view, event| {
            let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
            let mut life = life_changed.borrow_mut();
            match event {
                // `navigate` already optimistically began this load (so the seam
                // is well-defined even before the GTK loop turns). Only emit a
                // fresh Started here if this signal is for a *different* URL than
                // the one already in flight, to avoid a duplicate Started event.
                WkLoadEvent::Started => {
                    let already = life.state() == LoadState::Started
                        && life.current_url() == Some(url.as_str());
                    if !already {
                        life.begin(&url);
                    }
                }
                // A redirect just re-commits under the new URL; the seam only
                // models Started/Committed/Finished/Failed, so fold it into the
                // committed URL update the next Committed/Finished carries.
                WkLoadEvent::Redirected => {}
                WkLoadEvent::Committed => life.commit(&url),
                WkLoadEvent::Finished => life.finish(&url),
                _ => {}
            }
        });

        let life_failed = life.clone();
        view.connect_load_failed(move |_view, _event, failing_uri, error| {
            life_failed
                .borrow_mut()
                .fail(failing_uri, &error.to_string());
            // Let WebKitGTK show its default error page.
            false
        });
    }

    /// The live [`webkit6::WebView`] widget, for the shell to embed in a window.
    #[must_use]
    pub fn web_view(&self) -> &WebView {
        &self.view
    }

    /// Install the native EIP-1193 provider into this webview, over the seam's
    /// script-message bridge.
    ///
    /// This wires the FULL round-trip of werust's first trust hook (`CONTEXT.md`,
    /// `docs/adr/0001`):
    ///
    /// * injects the page-side [`provider_shim`] at document start, so every page
    ///   sees a detectable `window.ethereum` exposing the standard EIP-1193
    ///   `request(...)` interface and event surface;
    /// * registers the [`PROVIDER_BRIDGE`] script-message handler (page ->
    ///   native), routing each posted envelope through a [`ProviderBridge`]
    ///   read-only stub;
    /// * pushes the answer BACK into the page (native -> page) by evaluating the
    ///   settle-call JS in the live document
    ///   ([`Renderer::evaluate_javascript`](renderer::Renderer::evaluate_javascript)),
    ///   resolving the page's pending Promise.
    ///
    /// It holds NO keys: the stub answers only benign read-only methods (a
    /// chain-id / accounts stub). The response push captures a clone of this
    /// webview so the handler can evaluate JS on the GTK loop; that is why the
    /// wiring lives here rather than behind the `&mut dyn Renderer` seam (the
    /// seam's script-message handler is `Send`, which an `Rc`-shared backend is
    /// not — the webview evaluates on its single GTK thread instead).
    ///
    /// [`provider_shim`]: werust_core::provider::provider_shim
    /// [`PROVIDER_BRIDGE`]: werust_core::provider::PROVIDER_BRIDGE
    /// [`ProviderBridge`]: werust_core::provider::ProviderBridge
    pub fn install_provider(&mut self) {
        use werust_core::provider::{
            provider_shim, route_provider_message, ProviderBridge, PROVIDER_BRIDGE,
        };

        // Page -> native: register the provider channel and answer each envelope.
        self.content_manager
            .register_script_message_handler(PROVIDER_BRIDGE, None);
        let bridge = ProviderBridge::new();
        // Native -> page: the response push evaluates the settle-call JS in the
        // live document. Capture a WebView clone (a refcounted GObject handle) so
        // the push runs on the GTK loop the signal fires on.
        let view_for_push = self.view.clone();
        self.content_manager.connect_script_message_received(
            Some(PROVIDER_BRIDGE),
            move |_cm, value| {
                let message = renderer::ScriptMessage {
                    handler: PROVIDER_BRIDGE.to_string(),
                    body: value.to_str().to_string(),
                };
                let view = view_for_push.clone();
                route_provider_message(&bridge, &message, &mut |script| {
                    view.evaluate_javascript(
                        &script,
                        None::<&str>,
                        None,
                        gtk4::gio::Cancellable::NONE,
                        |_result| {},
                    );
                });
            },
        );

        // Make the provider detectable from document start.
        self.inject_script(&provider_shim());
    }

    /// Wire native `ipfs://` resolution into this webview, over the seam's
    /// custom-scheme / request-interception hook.
    ///
    /// This wires werust's SECOND trust hook (`CONTEXT.md`, `docs/adr/0001`): an
    /// `ipfs://<cid>/…` URL typed in the URL bar is intercepted at the seam, its
    /// CID resolved through the hash-verified content-addressed
    /// [`Fetcher`](fetcher::Fetcher) path, and the VERIFIED bytes rendered on the
    /// webview — at parity with a served page. Verification GATES the load: a hash
    /// mismatch (or any other verify failure) fails the load rather than rendering
    /// unverified bytes.
    ///
    /// It registers the `ipfs` scheme handler and routes each intercepted request
    /// through the pure [`resolve_ipfs_request`] resolver, backed by a production
    /// [`GatewayContentSource`] (an IPFS gateway over the bound HTTP
    /// [`HttpFetcher`](fetcher::HttpFetcher)) wrapped in a
    /// [`VerifyingContentFetcher`](fetcher::VerifyingContentFetcher) so the origin
    /// is never trusted, only the hash. The gateway source is UNTRUSTED; the
    /// verify above it is what makes the load safe. The pure resolution
    /// (scheme -> verified-fetch -> render, and its mismatch-fails-the-load
    /// guarantee) is exercised headlessly against a pinned fixture CID by the
    /// `werust_core::ipfs` tests.
    ///
    /// [`resolve_ipfs_request`]: werust_core::ipfs::resolve_ipfs_request
    /// [`GatewayContentSource`]: werust_core::ipfs::GatewayContentSource
    pub fn install_ipfs(&mut self) {
        use fetcher::{HttpFetcher, VerifyingContentFetcher};
        use werust_core::ipfs::{resolve_ipfs_request, GatewayContentSource, IPFS_SCHEME};

        // The production content-addressed fetcher: candidate bytes from an IPFS
        // gateway over the bound HTTP+TLS stack, hash-verified against the CID
        // before they are ever handed back. Owned by the scheme handler closure.
        let fetcher = VerifyingContentFetcher::new(GatewayContentSource::new(HttpFetcher::new()));
        // Share the load lifecycle into the scheme handler so a SUCCESSFUL verified
        // resolution can mark the current load content-verified — this is what
        // drives the chrome's trust indicator from the ACTUAL load path (the bytes
        // came back through `fetch_verified`), not from the `ipfs://` URL string.
        // A hash mismatch fails the load (the resolver returns an error) and never
        // reaches the mark, so a page that merely looks content-addressed but did
        // not verify is never reported verified (task
        // `trust-indicator-verified-vs-served`).
        //
        // This registers the scheme DIRECTLY on the web context rather than
        // through the seam's `register_scheme_handler`, for the same reason
        // `install_provider` pushes its response directly: the seam's
        // `SchemeHandler` is `Send` (so a generic backend can move it across
        // threads), but the lifecycle is an `Rc<RefCell<_>>` shared with the
        // webview's single-GTK-thread load signals and is NOT `Send`. The webview
        // runs this handler only on its own GTK loop, so capturing the `Rc`-shared
        // lifecycle here is sound — exactly the non-`Send` wiring the provider path
        // does for its live-page response push.
        let Some(context) = self.view.web_context() else {
            return;
        };
        let life = self.life.clone();
        context.register_uri_scheme(IPFS_SCHEME, move |request| {
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            match resolve_ipfs_request(&fetcher, &renderer::SchemeRequest { uri }) {
                Ok(response) => {
                    // The bytes verified against their CID: mark the current load
                    // content-verified so the chrome's trust indicator reflects the
                    // real (hash-verified) load path.
                    life.borrow_mut().mark_content_verified();
                    let bytes = glib::Bytes::from(&response.body);
                    let stream = gtk4::gio::MemoryInputStream::from_bytes(&bytes);
                    request.finish(
                        &stream,
                        response.body.len() as i64,
                        Some(&response.mime_type),
                    );
                }
                Err(e) => {
                    // Verification failed (a hash mismatch, an unverifiable CID, a
                    // source error): fail the load WITHOUT marking it verified, so
                    // unverified bytes never render AND the posture stays untrusted.
                    let mut error =
                        glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &e.to_string());
                    request.finish_error(&mut error);
                }
            }
        });
    }
}

impl Renderer for WebViewRenderer {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        validate_url(url)?;
        // Optimistically reflect the started load; WebKitGTK's `load-changed`
        // (Started) will also fire on the main loop and keep the lifecycle in
        // step once it is running.
        self.life.borrow_mut().begin(url);
        self.view.load_uri(url);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        if self.life.borrow().current_url().is_none() {
            return Err(RendererError::Backend("nothing to reload".into()));
        }
        self.view.reload();
        Ok(())
    }

    fn stop(&mut self) {
        self.view.stop_loading();
        self.life.borrow_mut().stop();
    }

    fn go_back(&mut self) {
        // WebKitGTK owns the session (back/forward) list; a back navigation
        // restarts the load, and the webview's `load-changed` signals feed the
        // shared `LoadLifecycle` exactly as a fresh `navigate` does. Guarded by
        // `can_go_back` so a stray call at the start of history is a no-op.
        if self.view.can_go_back() {
            self.view.go_back();
        }
    }

    fn go_forward(&mut self) {
        if self.view.can_go_forward() {
            self.view.go_forward();
        }
    }

    fn can_go_back(&self) -> bool {
        self.view.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.view.can_go_forward()
    }

    fn load_state(&self) -> LoadState {
        self.life.borrow().state()
    }

    fn trust_hooks(&self) -> TrustHooks {
        // OPT IN to BOTH trust hooks: `WebViewRenderer` genuinely wires them —
        // EIP-1193 provider injection over the script-message bridge
        // (`install_provider`: `register_script_message_handler` +
        // `evaluate_javascript` response push) and `ipfs://` custom-scheme
        // resolution (`install_ipfs`: `register_uri_scheme` → hash-verified fetch).
        // The seam default is now FAIL-CLOSED (`TrustHooks::none()`), so trust is
        // never inherited by omission: this backend must EXPLICITLY declare the
        // hooks it satisfies to pass `qualify`. Dropping a hook here would make the
        // real backend render-only — the two webview qualification tests guard
        // against exactly that.
        TrustHooks::all()
    }

    fn trust_posture(&self) -> renderer::TrustPosture {
        // Read the shared lifecycle's posture: `ContentVerified` iff the current
        // page's bytes came back through the hash-verified `ipfs://` path (marked
        // by the scheme handler `install_ipfs` wires), else the served-origin
        // posture. Interior-mutable because the load signals and the scheme
        // handler mutate it on the GTK loop.
        self.life.borrow().posture()
    }

    fn current_url(&self) -> Option<String> {
        // The lifecycle lives behind interior mutability (the load signals mutate
        // it from the GTK main loop), so the URL is returned owned rather than
        // borrowed out of the RefCell.
        self.life.borrow().current_url().map(str::to_string)
    }

    fn poll_event(&mut self) -> Option<LoadEvent> {
        self.life.borrow_mut().poll()
    }

    fn view_handle(&self) -> ViewHandle {
        let widget: &gtk4::Widget = self.view.upcast_ref();
        ViewHandle(widget.as_ptr() as *mut c_void)
    }

    fn send_pointer(&mut self, _event: PointerEvent) {
        // The embedded WebKitGTK widget receives real pointer input through GTK's
        // own event routing when it is focused in the window; explicit synthetic
        // pointer injection is not part of GTK4's public API. This hook is the
        // seam surface a backend that needs explicit forwarding (a future native
        // renderer) implements; the webview backend relies on GTK routing.
    }

    fn send_key(&mut self, _event: KeyEvent) {
        // As with pointer input: real key events reach the focused webview widget
        // via GTK; synthetic injection is not exposed by GTK4. Declared as part
        // of the seam for backends that own their own input path.
    }

    fn send_scroll(&mut self, _delta: ScrollDelta) {
        // Scrolling is handled by the webview widget from real input; the hook is
        // kept on the seam for backends that must forward it explicitly.
    }

    fn set_focus(&mut self, focused: bool) {
        if focused {
            self.view.grab_focus();
        }
    }

    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
        // Route `window.webkit.messageHandlers.<name>.postMessage(...)` up to the
        // handler. This is the channel the EIP-1193 provider is injected over
        // (task `eip1193-provider-injection-via-script-bridge`). The GTK signal
        // wants a `Fn`, but the seam hands us a `FnMut`, so the handler is kept
        // behind a RefCell and called on the single GTK main-loop thread.
        self.content_manager
            .register_script_message_handler(name, None);
        let name_owned = name.to_string();
        let handler = std::cell::RefCell::new(handler);
        self.content_manager
            .connect_script_message_received(Some(name), move |_cm, value| {
                let body = value.to_str().to_string();
                (handler.borrow_mut())(renderer::ScriptMessage {
                    handler: name_owned.clone(),
                    body,
                });
            });
    }

    fn inject_script(&mut self, script: &str) {
        let user_script = UserScript::new(
            script,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &[],
            &[],
        );
        self.content_manager.add_script(&user_script);
    }

    fn evaluate_javascript(&self, script: &str) {
        // Push JS into the live page (browser -> page): the response half of the
        // script-message bridge that settles the EIP-1193 provider's pending
        // Promise. WebKitGTK evaluates asynchronously on the GTK loop; we pass no
        // completion callback (fire-and-forget, matching the seam's `&self`,
        // no-result contract). Arguments after `script` are optional context the
        // day-one path does not need.
        self.view.evaluate_javascript(
            script,
            None::<&str>,
            None,
            gtk4::gio::Cancellable::NONE,
            |_result| {},
        );
    }

    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
        // Intercept `<scheme>://…` requests and answer them from the handler.
        // For `ipfs://` this is where a hash-verified content-addressed fetch is
        // wired in (task `ipfs-scheme-resolution-through-renderer-seam`). As with
        // the script bridge, the GTK callback is a `Fn`, so the `FnMut` handler
        // is held behind a RefCell and called on the GTK main-loop thread.
        let Some(context) = self.view.web_context() else {
            return;
        };
        let handler = std::cell::RefCell::new(handler);
        context.register_uri_scheme(scheme, move |request| {
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            let result = (handler.borrow_mut())(renderer::SchemeRequest { uri });
            match result {
                Ok(response) => {
                    let bytes = glib::Bytes::from(&response.body);
                    let stream = gtk4::gio::MemoryInputStream::from_bytes(&bytes);
                    request.finish(
                        &stream,
                        response.body.len() as i64,
                        Some(&response.mime_type),
                    );
                }
                Err(e) => {
                    let mut error =
                        glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &e.to_string());
                    request.finish_error(&mut error);
                }
            }
        });
    }
}
