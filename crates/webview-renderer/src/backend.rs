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
    LoadEvent as WkLoadEvent, Settings, UserContentInjectedFrames, UserContentManager, UserScript,
    UserScriptInjectionTime, WebContext, WebView,
};

use renderer::{
    KeyEvent, LoadEvent, LoadState, OsColorScheme, PointerEvent, Renderer, RendererError,
    SchemeHandler, ScriptMessageHandler, ScrollDelta, TrustHooks, ViewHandle,
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
        // Enable the WebKit Web Inspector (a real console REPL + network + DOM,
        // opened in-window by the shell's F12 shortcut) ONLY in a debug build
        // (task `enable-web-inspector-devtools-all-platforms`,
        // `work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`).
        // `WebInspector::show` is a no-op unless `enable-developer-extras` is set,
        // and the builder set NO `WebKitSettings` before this, so the inspector
        // could not be opened at all. Gating on `developer_extras_enabled()`
        // (which is `cfg!(debug_assertions)`) keeps a RELEASE build
        // (`cargo build --release`, the GoReleaser Rust builder path, ADR-0002)
        // NOT silently inspectable, while a developer `cargo run` build is.
        let settings = Settings::builder()
            .enable_developer_extras(developer_extras_enabled())
            .build();
        let view = WebView::builder()
            .user_content_manager(&content_manager)
            .web_context(&context)
            .settings(&settings)
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

        // SAME-DOCUMENT URL tracking (task
        // `track-webview-url-on-spa-clientside-navigation`): a SvelteKit SPA link
        // click is a CLIENT-SIDE `pushState`/`replaceState` navigation — the
        // document does NOT reload, so `load-changed` NEVER fires and the pump had
        // no event to drain (the bar froze on the pinned `.eth` name). But
        // WebKitGTK's `WebView::uri` property DOES update on such same-document
        // history changes, firing `notify::uri`. Observe it and surface a
        // same-document [`LoadEvent::UrlChanged`] through the shared lifecycle, so
        // the shell FOLLOWS the new URL (drops the pin / re-derives the ENS name)
        // exactly as it does for an in-page load event — WITHOUT faking a load.
        //
        // `url_changed` is a NO-OP when the new URI already matches the lifecycle's
        // current URL, so a `notify::uri` that merely echoes a real load's URL
        // (`navigate` already `begin`s it optimistically) emits nothing; only a
        // genuine URL change with no accompanying load transition surfaces an event.
        let life_uri = life.clone();
        view.connect_uri_notify(move |view| {
            if let Some(uri) = view.uri() {
                life_uri.borrow_mut().url_changed(&uri);
            }
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

    /// Wire the DESKTOP console + network CAPTURE POINTS into this webview, so
    /// the in-app debug menu's Console and Network tabs are fed by the real page
    /// (task `debug-console-network-capture-per-platform`, spec
    /// `in-app-debug-menu-console-and-network`).
    ///
    /// Capture is READ-ONLY OBSERVATION: nothing here answers a request, alters a
    /// load, touches the `ipfs://` verification, or changes a trust posture. It
    /// only pushes entries into the SHARED [`DebugCapture`] the shell also holds
    /// (`BrowserShell::with_debug_capture`), which the debug view renders.
    /// Always-on for Phase 1; the store's own `network_capture_enabled` flag is
    /// the seam the Phase-2 toggle drives.
    ///
    /// # CONSOLE: an injected shim, because WebKitGTK 6 has no console signal
    ///
    /// WebKitGTK's `WebView` exposes NO console-message signal in the pinned
    /// `webkit6` binding (the GTK3-era `WebKitWebView::console-message` is not
    /// there), so the console is captured the same way iOS captures it: by
    /// injecting the SHARED [`console_shim`] at document start and registering the
    /// [`CAPTURE_BRIDGE`] script-message channel it posts to. Desktop and iOS
    /// therefore run the byte-for-byte SAME page-side shim from ONE place in
    /// `werust-core`, while ANDROID uses its real native
    /// `WebChromeClient.onConsoleMessage` callback (strictly better than a shim).
    /// That deliberate per-platform difference is recorded in
    /// `docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md`.
    ///
    /// The shim CHAINS to the original `console.*`, so the page's own console and
    /// the WebKit Web Inspector's console are untouched.
    ///
    /// # NETWORK: the resource-load signals, which see https too
    ///
    /// `resource-load-started` fires for EVERY resource the page loads — including
    /// the `https://`/`http://` ones the `ipfs://` scheme handler never sees (they
    /// go direct through WebKit) — so this is the capture point with the widest
    /// desktop reach. Each started resource gets `finished`/`failed` handlers that
    /// read its [`URIResponse`](webkit6::URIResponse) for the status, MIME and
    /// size, and push ONE [`NetworkEntry`] built through the core's
    /// [`network_entry`] constructor (so the store's `MAX_TEXT_CHARS` bound cannot
    /// be bypassed).
    ///
    /// # The honest per-request posture (ADR-0006)
    ///
    /// * A SUB-RESOURCE carries the posture of what that ONE request actually did:
    ///   an `ipfs://` resource that came back WITHOUT failing went through the
    ///   hash-verified scheme handler, so it is `content-verified`; everything
    ///   else — an `https://` subresource, a `werust://` internal page, an
    ///   `ipfs://` request that FAILED (a hash mismatch fails the request) — is
    ///   `unverified-origin`. The rule itself is the core's shared
    ///   [`request_trust_posture`], never re-derived here.
    /// * The MAIN DOCUMENT instead takes the LOAD's OWN posture from the shared
    ///   lifecycle (the same fact the chrome trust indicator paints), so the
    ///   Network tab cannot show `content-verified` for the page row while the
    ///   indicator shows the louder `name-via-trusted-rpc` on an ENS-named page
    ///   (ADR-0006's two-axis rule; the store's DECISIONS.md Decision 4 assigns
    ///   this obligation here). WHICH resource is the main document is answered by
    ///   the core's ONE shared main-frame predicate (the `_redirects`
    ///   [`RedirectSink`](werust_core::ipfs::RedirectSink) the shell already drives
    ///   with every top-level navigation), never by a raw URL compare here — the
    ///   naive compares miss the WebKit authority-less `ipfs:///<cid>` form and a
    ///   post-redirect URL, and comparing against the chrome's DISPLAYED url would
    ///   never fire at all on an ENS page (the name is pinned there).
    ///
    /// # ONE request, ONE row
    ///
    /// WebKit emits a resource's `failed` signal and then ALSO emits `finished`
    /// (`webkitWebResourceFailed` ends by calling `webkitWebResourceFinished`), so
    /// pushing from both would record every failed load TWICE, and the second row
    /// would claim the success the first just disproved — stamping a failed,
    /// possibly hash-MISMATCHED `ipfs://` subresource `content-verified`. `failed`
    /// therefore only FLAGS the failure on the shared per-resource state;
    /// `finished` is the single push, and it reads that flag for the honest
    /// outcome.
    ///
    /// # Threading
    ///
    /// Both capture points run on the GTK loop the signal already fires on, doing
    /// only a bounded push into a `Mutex`-guarded ring buffer — no retrieval, no
    /// I/O, no lock held across a wait (`docs/adr/0008`'s discipline). The store is
    /// a cheap `Arc` handle, so the wiring captures its OWN clone and never reaches
    /// through the shell.
    ///
    /// Wired directly on the concrete backend (not through the `Renderer` seam) for
    /// the same reason `install_provider`/`install_ipfs` are: the handlers capture
    /// the `Rc`-shared, non-`Send` lifecycle and view, and run only on the single
    /// GTK thread.
    ///
    /// [`DebugCapture`]: werust_core::debug::DebugCapture
    /// [`console_shim`]: werust_core::debug::console_shim
    /// [`CAPTURE_BRIDGE`]: werust_core::debug::CAPTURE_BRIDGE
    /// [`network_entry`]: werust_core::debug::network_entry
    /// [`request_trust_posture`]: werust_core::debug::request_trust_posture
    pub fn install_debug_capture(
        &mut self,
        capture: werust_core::debug::DebugCapture,
        redirects: werust_core::ipfs::RedirectSink,
    ) {
        use werust_core::debug::{console_shim, route_capture_message, CAPTURE_BRIDGE};

        // --- CONSOLE: the shared injected shim over its own capture channel ---
        self.content_manager
            .register_script_message_handler(CAPTURE_BRIDGE, None);
        let console_capture = capture.clone();
        self.content_manager.connect_script_message_received(
            Some(CAPTURE_BRIDGE),
            move |_cm, value| {
                // One shared parse+push for both shim-fed platforms: an unreadable
                // or hostile body is dropped, never fabricated into an entry.
                route_capture_message(&console_capture, value.to_str().as_ref());
            },
        );
        self.inject_script(&console_shim());

        // --- NETWORK: the resource-load signals (https included) --------------
        // NOTE: the page-side `fetch`/`XHR` shim iOS also injects is deliberately
        // NOT injected here: these signals already report every resource,
        // including the browser-internal subresource loads the page-side wrapper
        // cannot see, so adding it would only double-record a subset.
        let life_for_posture = self.life.clone();
        self.view
            .connect_resource_load_started(move |_view, resource, request| {
                let method = request
                    .http_method()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "GET".to_string());
                let url = request.uri().map(|u| u.to_string()).unwrap_or_default();

                // ONE state per resource, SHARED by both signals: `failed` only
                // FLAGS the failure, `finished` performs the single push. (`Rc` +
                // `Cell` rather than an `Arc`/atomic: both signals fire on the one
                // GTK thread this whole wiring lives on.)
                let state = std::rc::Rc::new(ResourceCaptureState {
                    capture: capture.clone(),
                    life: life_for_posture.clone(),
                    redirects: redirects.clone(),
                    method,
                    url,
                    started: std::time::Instant::now(),
                    failed: std::cell::Cell::new(false),
                });

                let failed_state = state.clone();
                resource.connect_failed(move |_resource, _error| {
                    // FLAG ONLY, never a push: WebKit emits `failed` and then ALSO
                    // emits `finished` for the same resource, so pushing here would
                    // record the request twice and the second row would claim the
                    // success this one just disproved.
                    failed_state.failed.set(true);
                });

                resource.connect_finished(move |resource| {
                    // The SINGLE push for this resource, at the point WebKit is done
                    // with it either way. A request that failed proved nothing, so
                    // it is never reported content-verified.
                    state.record(Some(resource));
                });
            });
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
    /// through the pure [`resolve_ipfs_request`] resolver, backed by the default
    /// [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever)
    /// (fetch the DAG blocks as a CAR from a trustless gateway over the bound
    /// HTTP [`HttpFetcher`](fetcher::HttpFetcher), verify EACH block against its
    /// own CID, and reassemble/traverse the UnixFS DAG client-side). The gateway
    /// is UNTRUSTED; the per-block verify above it is what makes the load safe,
    /// so a real multi-block directory site renders legitimately content-verified.
    /// The pure resolution (scheme -> verified-retrieve -> render, and its
    /// tamper/incomplete/budget-fails-the-load guarantees) is exercised headlessly
    /// against real CAR fixtures by the `fetcher::retriever` and
    /// `werust_core::ipfs` tests.
    ///
    /// It RETURNS the `_redirects` 3xx [`RedirectSink`](werust_core::ipfs::RedirectSink)
    /// the handler pushes redirect targets into: a matched 3xx rule is a
    /// NAVIGATION, which a scheme handler cannot perform (this one does not even
    /// run on the UI thread), so the caller hands the sink to the shell
    /// (`BrowserShell::with_redirect_sink`) and the shell drains it on its pump.
    /// Ignoring the returned sink is safe and simply means a matched 3xx fails
    /// closed without navigating.
    ///
    /// [`resolve_ipfs_request`]: werust_core::ipfs::resolve_ipfs_request
    pub fn install_ipfs(&mut self) -> werust_core::ipfs::RedirectSink {
        use std::sync::Arc;

        use fetcher::{HttpFetcher, TrustlessGatewayCarRetriever};
        use werust_core::ipfs::{RedirectSink, IPFS_SCHEME};

        use webview_shared::offthread::{complete_ipfs_request, retrieve_off_thread};

        // The production content retriever: the DAG blocks fetched as a CAR from
        // a trustless gateway over the bound HTTP+TLS stack, each block verified
        // against its own CID and the UnixFS DAG reassembled/traversed locally
        // before any byte is handed back.
        //
        // The gateway endpoint is the USER'S CHOSEN retrieval backend, read from
        // the persisted setting (task `retrieval-backend-user-setting`): a custom
        // gateway/local-node URL if the user picked one, else the default public
        // trustless gateway. So switching the setting (via `werust://settings`)
        // switches the ACTUAL load path on the next launch. The per-block verify
        // above the gateway is unchanged: whatever endpoint the user picks, no
        // unverified byte is ever served.
        //
        // Wrapped in an `Arc` because the blocking retrieval now runs OFF the GTK
        // main thread (see below): each intercepted request spawns a worker that
        // holds a cheap clone of this `Send + Sync` retriever, so concurrent
        // sub-resource fetches share one connection-pooling agent without racing.
        let retriever = Arc::new(TrustlessGatewayCarRetriever::with_gateway(
            HttpFetcher::new(),
            &werust_core::retrieval::active_gateway_endpoint(),
        ));
        // Share the load lifecycle into the scheme handler so a SUCCESSFUL verified
        // resolution can mark the current load content-verified — this is what
        // drives the chrome's trust indicator from the ACTUAL load path (every
        // block came back verified through the retriever), not from the `ipfs://`
        // URL string.
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
        // The `_redirects` 3xx hand-off: the handler pushes a redirect target here
        // (it cannot navigate) and the shell drains it on its pump. Cloned into the
        // handler, returned to the caller — both clones are the SAME sink.
        let redirects = RedirectSink::new();
        let Some(context) = self.view.web_context() else {
            return redirects;
        };
        let life = self.life.clone();
        let redirects_for_handler = redirects.clone();
        context.register_uri_scheme(IPFS_SCHEME, move |request| {
            // OFF THE UI THREAD (`docs/adr/0008`). The scheme-handler closure fires
            // on the single GTK main thread, once per request (the main document
            // AND every sub-resource). Running the blocking CAR fetch + per-block
            // verify + DAG reassembly here synchronously froze the window (GNOME's
            // "not responding" dialog on a real load). So instead:
            //
            // 1. `gio::spawn_blocking` runs the blocking `retrieve_off_thread` on
            //    gio's I/O thread pool. It captures only a `Send` retriever clone
            //    and the request URI (a plain `String`) and returns a `Send`
            //    `RetrievalOutcome` value — NOTHING GTK and NOTHING `!Send` (not the
            //    `WebKitURISchemeRequest`, not the `Rc`-shared lifecycle) crosses
            //    the thread boundary. Verification is unchanged: the same verifying
            //    path runs, just off the UI thread.
            // 2. `MainContext::spawn_local` awaits that outcome and runs the
            //    completion BACK on the GTK loop (a `!Send` future is allowed to
            //    capture the `!Send` request + lifecycle). `complete_ipfs_request`
            //    then marks the shared posture and finishes the request ON THE MAIN
            //    THREAD — so the worker never races the UI thread's posture updates
            //    (the desktop analogue of the Android Mutex fix), and the event loop
            //    keeps turning so concurrent sub-resource fetches do not serialize.
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            let retriever = retriever.clone();
            let life = life.clone();
            // The redirect sink is `Send + Sync`, so the worker may push into it;
            // the `!Send` lifecycle still never leaves this thread.
            let redirects = redirects_for_handler.clone();
            // A `WebKitURISchemeRequest` is a refcounted GObject; clone bumps the
            // refcount so the request lives until the completion future finishes it.
            let request = request.clone();
            let blocking = gtk4::gio::spawn_blocking(move || {
                retrieve_off_thread(retriever.as_ref(), uri, &redirects)
            });
            glib::MainContext::default().spawn_local(async move {
                // If the worker panicked, `join` is an `Err`; surface that as a
                // fail-closed load rather than rendering anything.
                let outcome = blocking.await.unwrap_or_else(|_| {
                    Err(renderer::RendererError::Backend(
                        "ipfs:// content-addressed load failed: retrieval worker panicked".into(),
                    ))
                });
                let mut sink = WebKitRequestSink { request };
                complete_ipfs_request(outcome, &mut sink, &life);
            });
        });

        // The internal `werust://settings` page (task
        // `retrieval-backend-user-setting`): registered on the SAME web context so
        // typing `werust://settings` renders the retrieval-backend selector, and a
        // `werust://settings?backend=…` selection is applied + persisted by the
        // shared core `apply_settings_request`. It is a normal (unverified) internal
        // page, so it does NOT mark the load content-verified.
        context.register_uri_scheme(werust_core::retrieval::WERUST_SCHEME, move |request| {
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            match werust_core::retrieval::apply_settings_request(&renderer::SchemeRequest { uri }) {
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

        redirects
    }

    /// Wire WebKitGTK's new-window (`create`) hook so a `target="_blank"` link /
    /// `window.open(url)` navigates IN THE CURRENT view instead of being dropped
    /// (task `blank-and-window-open-links-navigate-in-place`, field finding C,
    /// `docs/adr/0010`).
    ///
    /// WHY THIS IS NEEDED: werust has NO tab/window model yet. WebKitGTK emits the
    /// `create` signal for a `_blank`/`window.open`/new-window request; the
    /// builder wired `connect_load_changed`/`connect_load_failed` but NO
    /// `connect_create`, so such a request was UNHANDLED and the navigation
    /// silently DROPPED (every external `ronan.eth` link is `target="_blank"`, so
    /// they all did nothing). The recorded decision (in-place until a tab/window
    /// model exists, `docs/adr/0010`) is to load the requested URI in the EXISTING
    /// view, so a `_blank` link behaves like an ordinary in-view navigation.
    ///
    /// HOW: the handler reads the target URI from the `create` signal's
    /// [`NavigationAction`](webkit6::NavigationAction) request, applies the ONE
    /// shared [`renderer::new_window_action`] rule (the same rule the mobile hooks
    /// mirror), and on [`NavigateInPlace`](renderer::NewWindowAction::NavigateInPlace)
    /// loads that URI into `self.view` via the SAME `load_uri` the seam's
    /// [`navigate`](Renderer::navigate) drives — so an `ipfs://`/ENS `_blank`
    /// target still flows through the hash-verified `ipfs://` scheme handler
    /// (`install_ipfs`) and an unsupported scheme is still refused: the hook is a
    /// ROUTER, not a trust bypass. It optimistically begins the lifecycle so the
    /// shell's URL bar follows the new URL exactly as a normal navigation does,
    /// and then returns **NULL** so WebKitGTK creates NO second WebView (no real
    /// new window/tab) — the navigation already happened in place.
    ///
    /// WHY THE RAW `connect_local("create", …)` AND NOT THE TYPED
    /// [`connect_create`](webkit6::prelude::WebViewExt::connect_create) (task
    /// `fix-desktop-create-signal-crash-on-blank-links`,
    /// `docs/spikes/fix-desktop-create-signal-crash-on-blank-links/README.md`):
    /// WebKitGTK's `create` contract is "return a NEWLY ALLOCATED
    /// `WebKitWebView`, or NULL". The typed `connect_create` binding is generated
    /// as `Fn(&Self, &NavigationAction) -> gtk::Widget` — a NON-nullable return —
    /// so there is no way to answer NULL through it, which is what pushed the
    /// first version of this hook into returning the EXISTING view. That answer is
    /// neither a new view nor NULL: WebKitGTK then applied the new window's
    /// `WindowFeatures` to the returned view and dereferenced an EMPTY
    /// `std::optional<WebCore::WindowFeatures>`, ABORTING the process
    /// (`_M_is_engaged()` failed) on every `_blank` click — the v0.2.5 desktop
    /// crash. The RAW glib signal API has no such nullability restriction, so the
    /// handler below answers a real NULL `GtkWidget`, the documented
    /// "no new view" answer. This keeps ONE hook covering BOTH triggers: a
    /// `target="_blank"` link AND `window.open(url)` both reach `create`, whereas
    /// `decide-policy`/`NewWindowAction` fires only for the `_blank` link
    /// (`window.open` goes `LocalDOMWindow::open` -> `WebCore::createWindow` ->
    /// `Chrome::createWindow` -> `create`, never through `PolicyChecker::checkNewWindowPolicy`),
    /// so routing via decide-policy alone would silently drop `window.open`.
    ///
    /// It is registered directly on `self.view` (not through the seam) for the
    /// same reason `install_provider`/`install_ipfs` are: the handler captures a
    /// clone of the `Rc`-shared, non-`Send` lifecycle and the (non-`Send`) view,
    /// and runs only on the single GTK loop the `create` signal fires on.
    pub fn install_new_window_in_place(&mut self) {
        use renderer::{new_window_action, NewWindowAction};

        // Capture a WebView clone (a refcounted GObject handle) + the shared
        // lifecycle so the handler can load in place and reflect the started load
        // on the GTK loop the `create` signal fires on.
        let view_for_create = self.view.clone();
        let life = self.life.clone();
        self.view.connect_local("create", false, move |args| {
            // `create` is `GtkWidget* (WebKitWebView*, WebKitNavigationAction*)`:
            // args[0] is the emitting view, args[1] the navigation action carrying
            // the `_blank`/`window.open` destination. `NavigationAction::request`
            // takes `&mut`, so take an owned action to read it.
            let target = args
                .get(1)
                .and_then(|arg| arg.get::<webkit6::NavigationAction>().ok())
                .and_then(|mut action| action.request().and_then(|req| req.uri()));
            if let NewWindowAction::NavigateInPlace { url } = new_window_action(target.as_deref()) {
                // Load in the EXISTING view via the SAME path `navigate` drives, so
                // the normal navigation/scheme handling (ipfs:// verification,
                // unsupported-scheme refusal) applies — no trust bypass. Reflect the
                // started load on the shared lifecycle so the shell follows the URL.
                life.borrow_mut().begin(&url);
                view_for_create.load_uri(&url);
            }
            // Answer NULL: WebKitGTK creates NO new WebView (and applies no
            // `WindowFeatures` to anything), so there is no second window/tab and
            // no abort — the navigation happened in place above.
            Some(None::<gtk4::Widget>.to_value())
        });
    }

    /// Make this webview FOLLOW the OS light/dark color-scheme setting, so
    /// `prefers-color-scheme` and UA-styled controls match the user's OS
    /// preference instead of silently defaulting to light (task
    /// `webview-follow-os-color-scheme`, `docs/adr/0009`).
    ///
    /// WHY THIS IS NEEDED (the confirmed diagnosis,
    /// `docs/spikes/webview-follow-os-color-scheme/DIAGNOSIS.md`): WebKitGTK ties
    /// the page color scheme + UA control theming to the GTK theme — its web
    /// process reports `prefers-color-scheme: dark` iff
    /// `gtk-application-prefer-dark-theme` is set (WebKit bugs 196685/197947,
    /// changeset 255342). But a plain GTK4 app does NOT inherit the desktop dark
    /// preference: on a dark-mode GNOME the portal reports prefer-dark while
    /// `gtk-application-prefer-dark-theme` defaults to FALSE, so WebKitGTK reports
    /// LIGHT UA defaults — the mandalas.eth.limo white-on-white buttons. The fix is
    /// to read the OS preference and set that GTK flag to MATCH it.
    ///
    /// FOLLOW, never force (the human's scope decision, `docs/adr/0009`): the OS
    /// signal is read from the XDG desktop portal's
    /// `org.freedesktop.appearance color-scheme` (the cross-desktop OS preference,
    /// not the app's own GTK theme name), mapped through the shared
    /// [`OsColorScheme`] rule — only an explicit OS dark preference sets
    /// prefer-dark; light / no-preference keep light. It does NOT override a
    /// page's declared `color-scheme`: changeset 255342 keeps the page on the
    /// light theme UNLESS the page declares dark support, so setting this flag only
    /// supplies the OS default the page and UA styling resolve against.
    ///
    /// Applied at load time AND kept LIVE: it subscribes to the portal's
    /// `SettingChanged` signal so toggling the OS light/dark setting at runtime
    /// re-applies the matching preference to the running web process.
    ///
    /// A missing portal (no session bus, an older desktop) is not fatal: the read
    /// falls back to [`OsColorScheme::NoPreference`], leaving the light CSS default
    /// — werust never forces dark when it cannot read the OS.
    pub fn follow_os_color_scheme(&self) {
        // Apply the OS preference once now, at load time.
        apply_os_color_scheme(read_portal_color_scheme());

        // Then track LIVE OS changes: the portal emits `SettingChanged` when the
        // desktop color-scheme is toggled. Re-read + re-apply so a runtime OS
        // light<->dark switch flows into the running web process.
        let proxy = gtk4::gio::DBusProxy::for_bus_sync(
            gtk4::gio::BusType::Session,
            gtk4::gio::DBusProxyFlags::NONE,
            None,
            PORTAL_BUS_NAME,
            PORTAL_OBJECT_PATH,
            PORTAL_SETTINGS_INTERFACE,
            gtk4::gio::Cancellable::NONE,
        );
        if let Ok(proxy) = proxy {
            proxy.connect_local("g-signal", false, move |args| {
                // `g-signal(sender, signal_name, parameters)`. The portal's
                // `SettingChanged(namespace, key, value)` fires on any setting; we
                // re-read the color-scheme rather than decode the payload shape, so
                // one path handles the reading and the mapping consistently.
                let signal_name = args
                    .get(2)
                    .and_then(|v| v.get::<String>().ok())
                    .unwrap_or_default();
                if signal_name == "SettingChanged" {
                    apply_os_color_scheme(read_portal_color_scheme());
                }
                None
            });
            // Keep the proxy alive for the run of the webview so the subscription
            // survives: leak the handle (the webview outlives the process's
            // browsing session, so this is a one-time, bounded leak of a single
            // proxy, not a per-navigation growth).
            std::mem::forget(proxy);
        }
    }

    /// Open the WebKitGTK Web Inspector over the current page: a REAL browser
    /// devtools surface (a console with a JS REPL you can type into, a network
    /// tab, DOM/sources), the SAME WebKit Web Inspector a desktop WebKit browser
    /// shows (task `enable-web-inspector-devtools-all-platforms`). This is NOT the
    /// GTK interactive debugger (widget tree / CSS): that is a separate GTK-level
    /// surface on Ctrl+Shift+I / Ctrl+Shift+D; this opens the WEB inspector for
    /// the page content, which is what the human asked for.
    ///
    /// Wired to the shell's F12 shortcut (`crates/werust/src/main.rs`), chosen
    /// because F12 is the desktop-browser-idiomatic devtools key and does NOT
    /// collide with the GTK debugger's Ctrl+Shift+I / Ctrl+Shift+D. It only does
    /// anything when `enable-developer-extras` is set, which
    /// [`WebViewRenderer::new`] does only in a debug build
    /// (`developer_extras_enabled`); in a release build the view has no inspector
    /// and this is a safe no-op, so the shortcut cannot open devtools on a shipped
    /// build.
    ///
    /// `WebView::inspector()` returns `None` when developer-extras is off; in that
    /// case there is nothing to show, so this returns without error.
    pub fn show_inspector(&self) {
        if let Some(inspector) = self.view.inspector() {
            inspector.show();
        }
    }
}

/// What one in-flight resource's capture handlers need to record it when it
/// finishes or fails: the shared store, the lifecycle the main-document decision
/// reads, and the request facts only the START signal carried.
///
/// `resource-load-started` is where the METHOD and the request URL are available;
/// `finished`/`failed` are where the RESPONSE is. This carries the former across
/// to the latter so exactly ONE [`NetworkEntry`] is pushed per resource, at the
/// point its outcome is known.
///
/// It deliberately holds NO [`WebView`] clone: this value lives inside a signal
/// closure on a `WebResource` the view itself owns, so retaining the view here
/// would keep it alive from an object it owns for the life of every resource.
/// The posture comes from the shared lifecycle instead (the SAME fact the chrome
/// paints, already shared into the view's own load signals), and WHICH resource
/// is the main document comes from the core's one shared main-frame predicate on
/// the `_redirects` sink.
///
/// ONE of these is shared (via an `Rc`) by BOTH the `failed` and the `finished`
/// handler, because WebKit emits both for a failed resource: `failed` sets
/// [`failed`](ResourceCaptureState::failed) and `finished` performs the single
/// push, so one request produces exactly one honest row.
struct ResourceCaptureState {
    capture: werust_core::debug::DebugCapture,
    life: SharedLifecycle,
    /// The `_redirects` sink the shell drives with every top-level navigation:
    /// the codebase's ONE main-frame predicate, reused here rather than
    /// re-derived (see [`WebViewRenderer::install_debug_capture`]).
    redirects: werust_core::ipfs::RedirectSink,
    method: String,
    url: String,
    started: std::time::Instant,
    /// Set by the `failed` handler; READ by the `finished` handler, which is the
    /// only pusher. A `Cell` (not an atomic) because both signals fire on the one
    /// GTK thread.
    failed: std::cell::Cell<bool>,
}

impl ResourceCaptureState {
    /// Push this resource's ONE [`NetworkEntry`] into the shared store.
    ///
    /// Called only from the `finished` handler, which WebKit emits for every
    /// resource including a failed one; the honest outcome is read from
    /// [`failed`](ResourceCaptureState::failed), because a request that failed
    /// proved nothing and can never be reported verified. Runs on the GTK loop the
    /// signal fires on and does only a bounded push; the decision itself is the
    /// pure, display-free [`resource_network_entry`].
    fn record(&self, resource: Option<&webkit6::WebResource>) {
        let finished_ok = !self.failed.get();
        let response: Option<webkit6::URIResponse> =
            resource.and_then(webkit6::WebResource::response);
        let status = response.as_ref().map(webkit6::URIResponse::status_code);
        let mime = response
            .as_ref()
            .and_then(|r| r.mime_type())
            .map(|m| m.to_string())
            .unwrap_or_default();
        let size = response.as_ref().map(webkit6::URIResponse::content_length);

        // The MAIN DOCUMENT takes the LOAD's own (two-axis) posture so the Network
        // tab cannot contradict the chrome trust indicator on the same screen;
        // every other row keeps its honest per-request posture. WHICH resource is
        // the main document is the core's ONE shared predicate (the `_redirects`
        // sink the shell drives with every top-level navigation), normalized so it
        // survives the WebKit authority-less `ipfs:///<cid>` form and a
        // query/fragment — never a raw URL compare here.
        let load_posture = self
            .redirects
            .is_main_frame(&self.url)
            .then(|| self.life.borrow().posture());

        let entry = resource_network_entry(&ResourceLoadFacts {
            method: &self.method,
            url: &self.url,
            status,
            mime: &mime,
            size,
            finished_ok,
            load_posture,
            timestamp: epoch_millis(),
            duration: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
        self.capture.push_network(entry);
    }
}

/// Everything WebKitGTK's resource-load signals observed about ONE resource: the
/// request facts from `resource-load-started` and the response facts from
/// `finished`/`failed`, plus the outcome and the main-document decision.
///
/// A struct rather than a long argument list because the two signals contribute
/// different halves of it, and because every field is the same shape of thing:
/// an OBSERVATION, never an instruction (capture is read-only).
pub(crate) struct ResourceLoadFacts<'a> {
    /// The HTTP-style method the request carried (`GET` when it reported none).
    pub method: &'a str,
    /// The requested URL.
    pub url: &'a str,
    /// The response status, or `None`/`0` when there was no response (a
    /// custom-scheme answer carries no HTTP status).
    pub status: Option<u32>,
    /// The response MIME type, empty when unknown.
    pub mime: &'a str,
    /// The response body size in bytes, or `None`/`0` when unknown.
    pub size: Option<u64>,
    /// Whether the resource COMPLETED. A failed request proved nothing, so it is
    /// never reported verified.
    pub finished_ok: bool,
    /// `Some(posture)` ONLY for the MAIN-DOCUMENT resource, whose row takes the
    /// LOAD's own two-axis posture (so the Network tab and the chrome trust
    /// indicator cannot disagree, ADR-0006). `None` for a sub-resource, which
    /// keeps the honest per-request posture the shared
    /// [`request_trust_posture`](werust_core::debug::request_trust_posture)
    /// derives.
    pub load_posture: Option<renderer::TrustPosture>,
    /// When it was captured, in milliseconds since the Unix epoch.
    pub timestamp: u64,
    /// How long the resource took, in milliseconds.
    pub duration: u64,
}

/// Build the [`NetworkEntry`] for one desktop resource load: the pure,
/// display-free half of [`ResourceCaptureState::record`], so the whole mapping
/// (including the trust rule) is unit-testable with no GTK loop and no network.
///
/// A `0` status/size means WebKitGTK reported none, and the core constructor
/// keeps it honestly ABSENT rather than a fabricated `0`.
pub(crate) fn resource_network_entry(
    facts: &ResourceLoadFacts<'_>,
) -> werust_core::debug::NetworkEntry {
    let &ResourceLoadFacts {
        method,
        url,
        status,
        mime,
        size,
        finished_ok,
        load_posture,
        timestamp,
        duration,
    } = facts;
    // A verified request is a CONTENT-ADDRESSED one that actually completed:
    // an `ipfs://` load that hash-verified is finished through the scheme handler,
    // while a mismatch fails the request. The scheme test itself lives in the
    // core's `request_trust_posture`, so this only supplies the honest
    // "did it complete" half and never re-derives the trust rule.
    let entry = werust_core::debug::network_entry(
        method,
        url,
        status.and_then(|s| u16::try_from(s).ok()),
        mime,
        size,
        finished_ok,
        timestamp,
    )
    .with_duration(duration);
    match load_posture {
        // The MAIN DOCUMENT reports the LOAD's posture verbatim.
        Some(posture) => entry.with_trust(posture),
        None => entry,
    }
}

/// Milliseconds since the Unix epoch, for a captured entry's timestamp (`0` if
/// the clock is before the epoch). The core store takes a caller-supplied
/// timestamp so it binds no clock; this is the desktop edge's supply.
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Whether the WebKit Web Inspector's `enable-developer-extras` is turned on for
/// this build: TRUE in a debug build, FALSE in a release build.
///
/// This is the desktop half of the task's gating decision
/// (`work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`):
/// the inspector is a developer surface, so a RELEASE build
/// (`cargo build --release` — the shipped GoReleaser path, ADR-0002) is NOT
/// silently inspectable, while a developer `cargo run` / `cargo test` build is.
/// It keys off `debug_assertions`, which the Rust toolchain sets exactly on
/// non-optimized (debug) builds — the desktop analogue of Android's
/// `BuildConfig.DEBUG` and iOS's `#if DEBUG`. Pure so the gate is pinned
/// display-free by the backend tests.
#[must_use]
pub fn developer_extras_enabled() -> bool {
    cfg!(debug_assertions)
}

/// The XDG desktop portal address for reading the OS `color-scheme` preference.
const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS_INTERFACE: &str = "org.freedesktop.portal.Settings";

/// Map the XDG desktop portal's `org.freedesktop.appearance color-scheme` value
/// to the shared cross-platform [`OsColorScheme`].
///
/// The portal defines exactly three values (freedesktop settings spec):
/// `0` = no preference, `1` = prefer dark, `2` = prefer light. Anything else is a
/// value werust does not understand, treated as [`OsColorScheme::NoPreference`]
/// so an unknown/future value can never silently flip the WebView to dark —
/// following the OS never means guessing dark.
///
/// Pure so the decision is pinned display-free by
/// `desktop_maps_the_xdg_portal_color_scheme_to_the_os_signal`; the GTK-apply
/// half ([`apply_os_color_scheme`]) needs a display and is covered by the ignored
/// `real_webview_follows_the_os_color_scheme`.
pub(crate) fn os_color_scheme_from_portal(value: u32) -> OsColorScheme {
    match value {
        1 => OsColorScheme::Dark,
        2 => OsColorScheme::Light,
        // 0 (no preference) and any unknown value: supply no dark preference.
        _ => OsColorScheme::NoPreference,
    }
}

/// Read the OS color-scheme preference from the XDG desktop portal, returning
/// [`OsColorScheme::NoPreference`] if the portal is unavailable (no session bus,
/// an older desktop) — werust never forces dark when it cannot read the OS.
fn read_portal_color_scheme() -> OsColorScheme {
    use gtk4::glib::variant::ToVariant;
    use gtk4::prelude::*;

    let proxy = match gtk4::gio::DBusProxy::for_bus_sync(
        gtk4::gio::BusType::Session,
        gtk4::gio::DBusProxyFlags::NONE,
        None,
        PORTAL_BUS_NAME,
        PORTAL_OBJECT_PATH,
        PORTAL_SETTINGS_INTERFACE,
        gtk4::gio::Cancellable::NONE,
    ) {
        Ok(proxy) => proxy,
        Err(_) => return OsColorScheme::NoPreference,
    };

    // `Read(namespace, key) -> (v)`: the reply is a tuple carrying a variant that
    // wraps (possibly nesting another variant around) the `u32` color-scheme.
    let reply = proxy.call_sync(
        "Read",
        Some(&("org.freedesktop.appearance", "color-scheme").to_variant()),
        gtk4::gio::DBusCallFlags::NONE,
        5_000,
        gtk4::gio::Cancellable::NONE,
    );
    let value = match reply {
        Ok(v) => v,
        Err(_) => return OsColorScheme::NoPreference,
    };

    os_color_scheme_from_portal(unwrap_portal_u32(&value))
}

/// Dig the inner `u32` out of the portal `Read` reply, unwrapping the tuple and
/// any nested variant layers. Returns `0` (no preference) if the shape is not
/// what we expect, so a surprising payload never forces dark.
fn unwrap_portal_u32(value: &gtk4::glib::Variant) -> u32 {
    // The reply is `(v)`; the boxed child is itself a variant, and in practice
    // nests one more `v` layer (`(v)` -> `v` -> `v` -> `u32`). Peel every variant
    // (`v`) layer by descending into its child rather than `as_variant()`, which
    // ASSERTS (a GLib-CRITICAL) when called on a non-variant — checking the type
    // string first keeps the read clean whatever depth the portal boxes it at.
    let mut current = value.child_value(0);
    while current.type_().as_str() == "v" {
        current = current.child_value(0);
    }
    current.get::<u32>().unwrap_or(0)
}

/// Apply an [`OsColorScheme`] to the running WebKitGTK web process by setting
/// `gtk-application-prefer-dark-theme` to match, the flag WebKitGTK reads for
/// `prefers-color-scheme` (changeset 255342). A no-op if GTK settings are
/// unavailable. Setting the flag does NOT override a page's declared
/// `color-scheme`: it only supplies the OS default the engine resolves against.
fn apply_os_color_scheme(scheme: OsColorScheme) {
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(scheme.prefer_dark());
    }
}

/// A [`RequestSink`](webview_shared::offthread::RequestSink) over the live WebKitGTK
/// [`WebKitURISchemeRequest`](webkit6::URISchemeRequest): the marshalling-thread
/// completion of an off-thread `ipfs://` resolution.
///
/// It is created and used ONLY inside the `MainContext::spawn_local` future
/// `install_ipfs` schedules, so it runs on the GTK main thread — which is exactly
/// where a `WebKitURISchemeRequest` may be finished. `finish` streams the verified
/// bytes to the renderer; `fail` fails the load with the legible reason. The
/// posture mark is done by `complete_ipfs_request` on the shared lifecycle before
/// `finish`, so the two-thread split does not weaken the fail-closed trust path.
struct WebKitRequestSink {
    request: webkit6::URISchemeRequest,
}

impl webview_shared::offthread::RequestSink for WebKitRequestSink {
    fn finish(&mut self, response: renderer::SchemeResponse) {
        let bytes = glib::Bytes::from(&response.body);
        let stream = gtk4::gio::MemoryInputStream::from_bytes(&bytes);
        if response.status == renderer::STATUS_OK {
            self.request.finish(
                &stream,
                response.body.len() as i64,
                Some(&response.mime_type),
            );
            return;
        }
        // A NON-OK status with a body: the site's own error page, named by its
        // `_redirects` (IPIP-0002) for a path that is not in its DAG. WebKitGTK's
        // plain `finish` can only say 200, which would LIE about a page the site
        // declared missing, so the status travels on a `URISchemeResponse` and the
        // page still renders — exactly what a gateway does. The bytes are the same
        // hash-verified bytes; only the reported status differs.
        let scheme_response = webkit6::URISchemeResponse::new(&stream, response.body.len() as i64);
        scheme_response.set_content_type(&response.mime_type);
        scheme_response.set_status(u32::from(response.status), None);
        self.request.finish_with_response(&scheme_response);
    }

    fn fail(&mut self, error: RendererError) {
        let mut err = glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &error.to_string());
        self.request.finish_error(&mut err);
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

    fn mark_ens_origin(&mut self) {
        // Flag the current load as ENS-originated on the SHARED lifecycle (the
        // same one the `install_ipfs` scheme handler marks on the GTK loop). The
        // front door calls this right after starting the `ipfs://<cid>` load, so
        // when the scheme handler later verifies the bytes and calls
        // `mark_content_verified`, the lifecycle surfaces `NameViaTrustedRpc`
        // instead of the plain `ContentVerified` — the ENS-origin posture winning
        // over the handler's unconditional mark. A fresh `begin` clears the flag.
        self.life.borrow_mut().mark_ens_origin();
    }

    fn mark_mutable_name(&mut self) {
        // Flag the current load as pointing at a MUTABLE name on the SHARED
        // lifecycle (the same one the `install_ipfs` scheme handler marks). The
        // front door calls this right after starting an IPNS-resolved
        // `ipfs://<cid>` load, so when the scheme handler later verifies the bytes
        // and calls `mark_content_verified`, the lifecycle surfaces the honest
        // `MutableName` posture instead of the immutable `ContentVerified` — or,
        // if the load is ALSO ENS-originated, the louder `NameViaTrustedRpc` wins
        // (the two-axis display rule). A fresh `begin` clears the flag.
        self.life.borrow_mut().mark_mutable_name();
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
