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
    ScriptMessageHandler, ScrollDelta, ViewHandle,
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

    fn load_state(&self) -> LoadState {
        self.life.borrow().state()
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
