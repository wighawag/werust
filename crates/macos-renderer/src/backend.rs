//! The real macOS backend: a [`WKWebView`] wired behind the [`Renderer`] seam.
//!
//! [`MacosRenderer`] is the piece that actually shows a page on macOS. It binds
//! Apple's system webview through the `objc2` family of bindings rather than
//! hand-rolling a renderer, and nothing WebKit-specific leaks past the seam: the
//! rest of werust only ever sees the [`Renderer`] trait.
//!
//! # What it leans on, and what it does NOT fork
//!
//! * The load-lifecycle state machine, the `navigate` URL rule and the ADR-0008
//!   off-thread `ipfs://` boundary are the SHARED, toolkit-free
//!   [`webview_shared`] crate -- the very code the WebKitGTK backend runs, MOVED
//!   there rather than copied, so the two desktop backends cannot drift.
//! * `ipfs://` resolution itself is `werust_core::ipfs::resolve_ipfs_request`
//!   over the `fetcher` verifying retriever, the SAME path desktop, Android and
//!   iOS all use.
//! * The EIP-1193 provider is `werust_core::provider` (`provider_shim` +
//!   `route_provider_message` + the keyless read-only `ProviderBridge`), the SAME
//!   path every other edge uses -- including the page-side
//!   `window.webkit.messageHandlers.<name>` API, which is WebKit's own and
//!   therefore identical on WebKitGTK, iOS and here.
//!
//! What is genuinely NEW is only the Objective-C wiring: the three small delegate
//! classes below and the lazy-webview lifecycle.
//!
//! # Why the `WKWebView` is created LAZILY
//!
//! `WKWebViewConfiguration` is COPIED by `-[WKWebView initWithFrame:configuration:]`,
//! and `-[WKWebViewConfiguration setURLSchemeHandler:forURLScheme:]` is the ONLY
//! way to register a custom scheme. So the set of intercepted schemes is fixed
//! when the webview is constructed -- the exact constraint
//! `docs/adr/0011-webview2-for-windows.md` (finding 5) records for WebView2's
//! `ICoreWebView2CustomSchemeRegistration`, and it prescribes the same answer:
//! an EAGER container view (so [`view_handle`](Renderer::view_handle) works from
//! construction) plus a LAZILY created engine, NOT a widening of the `Renderer`
//! trait. [`MacosRenderer::new`] therefore builds only an `NSView` and a
//! configuration; the `WKWebView` is realised on the first
//! [`navigate`](Renderer::navigate) (or explicitly via
//! [`realize`](MacosRenderer::realize)), by which time the shell has installed
//! its scheme handlers exactly as the desktop shell does.
//!
//! # Threading
//!
//! A `WKWebView` and everything around it is MAIN-THREAD-ONLY, and the shared
//! `LoadLifecycle` is `!Send` for that reason. The blocking `ipfs://` retrieval
//! therefore runs on a worker thread and hands back only the `Send`
//! [`RetrievalOutcome`]; the completion (finish the `WKURLSchemeTask`, mark the
//! posture) is applied on the main thread by
//! [`pump_scheme_completions`](MacosRenderer::pump_scheme_completions), which
//! [`poll_event`](Renderer::poll_event) calls on every drain. That is ADR-0008's
//! rule, satisfied with the SAME shared boundary the GTK backend marshals with
//! `gio::spawn_blocking` + `MainContext::spawn_local`.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{
    define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message,
};
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{
    ns_string, NSData, NSDictionary, NSError, NSHTTPURLResponse, NSKeyValueChangeKey,
    NSKeyValueObservingOptions, NSObjectNSKeyValueObserverRegistration, NSPoint, NSRect, NSSize,
    NSString, NSURLRequest, NSURLResponse, NSURL,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationDelegate, WKScriptMessage,
    WKScriptMessageHandler, WKUIDelegate, WKURLSchemeHandler, WKURLSchemeTask,
    WKUserContentController, WKUserScript, WKUserScriptInjectionTime, WKWebView,
    WKWebViewConfiguration, WKWindowFeatures,
};

use renderer::{
    KeyEvent, LoadEvent, LoadState, OsColorScheme, PointerEvent, Renderer, RendererError,
    SchemeHandler, SchemeRequest, SchemeResponse, ScriptMessage, ScriptMessageHandler, ScrollDelta,
    TrustHooks, TrustPosture, ViewHandle,
};
use webview_shared::offthread::{complete_ipfs_request, RequestSink, RetrievalOutcome};
use webview_shared::{validate_url, LoadLifecycle, SharedLifecycle};

use crate::pure::{navigation_failure, os_color_scheme_from_appearance};

/// Resolve one intercepted request OFF the main thread.
///
/// The off-thread half of the verifying `ipfs://` route, kept behind a
/// `Send + Sync` trait so the production path
/// (`werust_core::ipfs::resolve_ipfs_request` over the trustless-gateway CAR
/// retriever) and an offline, PINNED test double are the same code path from the
/// backend's point of view -- which is what lets the trust hook be exercised in
/// CI with no gateway and no network.
///
/// It returns the shared [`RetrievalOutcome`], the ONLY value that crosses the
/// worker/main-thread boundary (`docs/adr/0008`).
pub trait OffThreadResolve: Send + Sync + 'static {
    /// Resolve `uri`, blocking. Runs on a worker thread, never the main thread.
    fn resolve(&self, uri: String) -> RetrievalOutcome;
}

impl<F> OffThreadResolve for F
where
    F: Fn(String) -> RetrievalOutcome + Send + Sync + 'static,
{
    fn resolve(&self, uri: String) -> RetrievalOutcome {
        self(uri)
    }
}

/// How a registered scheme answers an intercepted request.
enum Route {
    /// The seam's [`register_scheme_handler`](Renderer::register_scheme_handler):
    /// answered SYNCHRONOUSLY on the main thread and NOT marked verified --
    /// byte-for-byte the contract the WebKitGTK backend's
    /// `register_scheme_handler` has. Used for the internal `werust://` pages.
    Sync(RefCell<SchemeHandler>),
    /// The production `ipfs://` route (`install_ipfs`): the blocking verify runs
    /// on a worker and the completion -- including the content-verified mark --
    /// is applied on the main thread (`docs/adr/0008`).
    OffThread(Arc<dyn OffThreadResolve>),
}

/// One intercepted `WKURLSchemeTask`, retained until its off-thread resolution
/// completes (or WebKit stops it).
type RetainedSchemeTask = Retained<ProtocolObject<dyn WKURLSchemeTask>>;

/// The ivars of the `WKURLSchemeHandler` bridge: everything ONE registered
/// scheme needs to answer its tasks.
struct SchemeBridgeIvars {
    route: Route,
    /// The SHARED load lifecycle, mutated ONLY on this (the main) thread.
    life: SharedLifecycle,
    /// Tasks whose off-thread resolution is still in flight, in start order. A
    /// `Vec` rather than a map because it is also scanned by task IDENTITY when
    /// WebKit stops a task, and it never holds more than a page's worth of
    /// sub-resources.
    inflight: RefCell<Vec<(u64, RetainedSchemeTask)>>,
    next_id: Cell<u64>,
    outcomes_tx: Sender<(u64, RetrievalOutcome)>,
    outcomes_rx: Receiver<(u64, RetrievalOutcome)>,
}

define_class!(
    // SAFETY:
    // - `NSObject` has no subclassing requirements.
    // - `SchemeBridge` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    // A `WKURLSchemeHandler` is only ever called on the main thread, and it holds
    // the `!Send` shared lifecycle, so the class is main-thread-only.
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacSchemeBridge"]
    #[ivars = SchemeBridgeIvars]
    struct SchemeBridge;

    unsafe impl NSObjectProtocol for SchemeBridge {}

    unsafe impl WKURLSchemeHandler for SchemeBridge {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start_task(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            self.start(task);
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop_task(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            // "After your app is told to stop loading data for a URL scheme
            // handler task it must not perform any callbacks for that task" --
            // WebKit THROWS if it does. So the task is dropped here; the worker's
            // outcome then finds no in-flight entry and is discarded.
            let stopped: *const ProtocolObject<dyn WKURLSchemeTask> = task;
            self.ivars()
                .inflight
                .borrow_mut()
                .retain(|(_, held)| !std::ptr::eq(Retained::as_ptr(held), stopped));
        }
    }
);

impl SchemeBridge {
    fn new(mtm: MainThreadMarker, route: Route, life: SharedLifecycle) -> Retained<Self> {
        let (outcomes_tx, outcomes_rx) = channel();
        let this = Self::alloc(mtm).set_ivars(SchemeBridgeIvars {
            route,
            life,
            inflight: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            outcomes_tx,
            outcomes_rx,
        });
        unsafe { msg_send![super(this), init] }
    }

    /// The URI WebKit intercepted, as werust's core speaks it.
    fn task_uri(task: &ProtocolObject<dyn WKURLSchemeTask>) -> String {
        unsafe {
            task.request()
                .URL()
                .and_then(|url| url.absoluteString())
                .map(|s| s.to_string())
                .unwrap_or_default()
        }
    }

    fn start(&self, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        let uri = Self::task_uri(task);
        match &self.ivars().route {
            Route::Sync(handler) => {
                // Answered inline on the main thread, exactly as the WebKitGTK
                // backend's `register_scheme_handler` does -- and, exactly as
                // there, WITHOUT marking the load content-verified: only the
                // verifying route may do that.
                let outcome = (handler.borrow_mut())(SchemeRequest { uri });
                let mut sink = SchemeTaskSink { task };
                match outcome {
                    Ok(response) => sink.finish(response),
                    Err(error) => sink.fail(error),
                }
            }
            Route::OffThread(resolver) => {
                // OFF THE MAIN THREAD (`docs/adr/0008`). Only a `Send` value
                // crosses: the resolver clone and the URI go out, a
                // `RetrievalOutcome` comes back. NOTHING WebKit and NOTHING
                // `!Send` (not the `WKURLSchemeTask`, not the shared lifecycle)
                // is ever touched off this thread.
                let id = self.ivars().next_id.get();
                self.ivars().next_id.set(id.wrapping_add(1));
                self.ivars().inflight.borrow_mut().push((id, task.retain()));
                let resolver = resolver.clone();
                let tx = self.ivars().outcomes_tx.clone();
                std::thread::spawn(move || {
                    let outcome = resolver.resolve(uri);
                    // A closed channel means the backend is gone; nothing to do.
                    let _ = tx.send((id, outcome));
                });
            }
        }
    }

    /// Apply every off-thread outcome that has arrived, ON THIS (the main)
    /// thread: finish the task with the verified bytes and mark the shared load
    /// content-verified, or fail it closed WITHOUT marking.
    ///
    /// Returns how many completions were applied, so a driver can tell whether
    /// the pump did anything.
    fn drain_completions(&self) -> usize {
        let mut applied = 0;
        while let Ok((id, outcome)) = self.ivars().outcomes_rx.try_recv() {
            let held = {
                let mut inflight = self.ivars().inflight.borrow_mut();
                inflight
                    .iter()
                    .position(|(held_id, _)| *held_id == id)
                    .map(|index| inflight.remove(index).1)
            };
            // `None` means WebKit already STOPPED this task: answering it now
            // would throw, so the outcome is dropped.
            let Some(task) = held else { continue };
            let mut sink = SchemeTaskSink { task: &task };
            // The ONE shared completion rule: verified bytes render and mark the
            // posture; ANY failure fails the load closed and leaves the posture
            // untouched.
            complete_ipfs_request(outcome, &mut sink, &self.ivars().life);
            applied += 1;
        }
        applied
    }
}

/// A [`RequestSink`] over a live `WKURLSchemeTask`, so the SHARED completion
/// logic (mark-verified-only-on-success, fail-closed-on-error) drives WebKit's
/// task API without knowing anything about it.
struct SchemeTaskSink<'a> {
    task: &'a ProtocolObject<dyn WKURLSchemeTask>,
}

impl RequestSink for SchemeTaskSink<'_> {
    fn finish(&mut self, response: SchemeResponse) {
        let url = unsafe { self.task.request().URL() };
        let Some(url) = url else {
            self.fail(RendererError::Backend(
                "intercepted request carried no URL".into(),
            ));
            return;
        };
        // An `NSHTTPURLResponse` rather than a plain `NSURLResponse` so the
        // honest STATUS travels with the bytes: a content-addressed site may name
        // its OWN error page for a missing path through the IPFS `_redirects`
        // convention (IPIP-0002), and answering that page with 200 would LIE
        // about a page the site declared missing (the seam carries
        // `SchemeResponse::status` for exactly this reason).
        let mime = NSString::from_str(&response.mime_type);
        let headers = NSDictionary::from_slices(&[ns_string!("Content-Type")], &[&*mime]);
        let http = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
            NSHTTPURLResponse::alloc(),
            &url,
            response.status as isize,
            Some(ns_string!("HTTP/1.1")),
            Some(&headers),
        );
        let Some(http) = http else {
            self.fail(RendererError::Backend(
                "could not build a response for the intercepted request".into(),
            ));
            return;
        };
        let data = NSData::with_bytes(&response.body);
        unsafe {
            let response: &NSURLResponse = &http;
            self.task.didReceiveResponse(response);
            self.task.didReceiveData(&data);
            self.task.didFinish();
        }
    }

    fn fail(&mut self, error: RendererError) {
        // FAIL CLOSED: nothing is rendered, and (on the verifying route) the
        // posture is never marked. The reason travels as the error's localized
        // description so WebKit's own error page carries it.
        let message = NSString::from_str(&error.to_string());
        let key = unsafe { objc2_foundation::NSLocalizedDescriptionKey };
        let value: &AnyObject = &message;
        let user_info = NSDictionary::from_slices(&[key], &[value]);
        let ns_error = unsafe {
            NSError::initWithDomain_code_userInfo(
                NSError::alloc(),
                ns_string!("WerustSchemeErrorDomain"),
                -1,
                Some(&user_info),
            )
        };
        unsafe { self.task.didFailWithError(&ns_error) };
    }
}

/// The ivars of the script-message bridge: one registered channel name plus the
/// seam handler that answers it.
struct ScriptBridgeIvars {
    name: String,
    handler: RefCell<ScriptMessageHandler>,
}

define_class!(
    // SAFETY:
    // - `NSObject` has no subclassing requirements.
    // - `ScriptBridge` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacScriptBridge"]
    #[ivars = ScriptBridgeIvars]
    struct ScriptBridge;

    unsafe impl NSObjectProtocol for ScriptBridge {}

    unsafe impl WKScriptMessageHandler for ScriptBridge {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            // `window.webkit.messageHandlers.<name>.postMessage(...)` -- WebKit's
            // OWN page-side API, so the SHARED `provider_shim` (and the shared
            // console shim the debug work will add) run here unchanged. The body
            // is taken as a string: the provider channel posts JSON strings, and
            // anything else stringifies through `description` rather than being
            // dropped silently.
            let body = unsafe { message.body() };
            let body: Retained<NSString> = unsafe { msg_send![&*body, description] };
            (self.ivars().handler.borrow_mut())(ScriptMessage {
                handler: self.ivars().name.clone(),
                body: body.to_string(),
            });
        }
    }
);

impl ScriptBridge {
    fn new(mtm: MainThreadMarker, name: &str, handler: ScriptMessageHandler) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ScriptBridgeIvars {
            name: name.to_string(),
            handler: RefCell::new(handler),
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// The ivars of the navigation delegate: the shared lifecycle its signals drive.
struct NavigationBridgeIvars {
    life: SharedLifecycle,
}

define_class!(
    // SAFETY:
    // - `NSObject` has no subclassing requirements.
    // - `NavigationBridge` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacNavigationBridge"]
    #[ivars = NavigationBridgeIvars]
    struct NavigationBridge;

    unsafe impl NSObjectProtocol for NavigationBridge {}

    /// The LOAD LIFECYCLE: WebKit's real navigation signals drive the SHARED,
    /// toolkit-free `LoadLifecycle`, so the browser sees exactly the
    /// `LoadState`/`LoadEvent` surface it sees from the WebKitGTK backend.
    unsafe impl WKNavigationDelegate for NavigationBridge {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            let url = current_url_of(web_view);
            let mut life = self.ivars().life.borrow_mut();
            // `navigate` already optimistically began this load, so only a load
            // for a DIFFERENT url starts a fresh lifecycle here -- the same
            // no-duplicate-Started rule the GTK backend applies.
            let already =
                life.state() == LoadState::Started && life.current_url() == Some(url.as_str());
            if !already {
                life.begin(&url);
            }
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.ivars()
                .life
                .borrow_mut()
                .commit(&current_url_of(web_view));
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.ivars()
                .life
                .borrow_mut()
                .finish(&current_url_of(web_view));
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.report_failure(web_view, error);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.report_failure(web_view, error);
        }
    }

    /// The NEW-WINDOW hook (`docs/adr/0010`): a `target="_blank"` link or a
    /// `window.open(url)` navigates IN THE CURRENT view rather than being
    /// silently dropped, and NO second webview is created.
    unsafe impl WKUIDelegate for NavigationBridge {
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create_web_view(
            &self,
            web_view: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            navigation_action: &WKNavigationAction,
            _window_features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            let target = unsafe {
                navigation_action
                    .request()
                    .URL()
                    .and_then(|url| url.absoluteString())
                    .map(|s| s.to_string())
            };
            if let renderer::NewWindowAction::NavigateInPlace { url } =
                renderer::new_window_action(target.as_deref())
            {
                // Fed back into the NORMAL load path, so an `ipfs://` `_blank`
                // target still goes through the hash-verified scheme handler and
                // an unsupported scheme is still refused: the hook is a ROUTER,
                // not a trust bypass.
                self.ivars().life.borrow_mut().begin(&url);
                load_url(web_view, &url);
            }
            // NIL: WebKit creates no second webview. The navigation (if any)
            // already happened in place above.
            None
        }
    }

    /// SAME-DOCUMENT URL tracking (`track-webview-url-on-spa-clientside-navigation`):
    /// a SvelteKit `pushState`/`replaceState` fires NO navigation-delegate
    /// callback, but it DOES change `WKWebView.URL`, which is KVO-observable --
    /// the same observation the iOS edge already makes.
    impl NavigationBridge {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            object: Option<&AnyObject>,
            _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            if key_path.map(NSString::to_string).as_deref() != Some(URL_KEY_PATH) {
                return;
            }
            let Some(object) = object else { return };
            // The observation is registered on exactly one object, this
            // backend's own `WKWebView`.
            let web_view: &WKWebView = unsafe { &*(object as *const AnyObject).cast() };
            // `url_changed` is a NO-OP when the URL already matches the
            // lifecycle's current URL, so the KVO fire that merely echoes a real
            // load emits nothing: only a genuine same-document change surfaces a
            // `LoadEvent::UrlChanged`, and it moves neither the load state nor
            // the trust posture.
            self.ivars()
                .life
                .borrow_mut()
                .url_changed(&current_url_of(web_view));
        }
    }
);

impl NavigationBridge {
    fn new(mtm: MainThreadMarker, life: SharedLifecycle) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NavigationBridgeIvars { life });
        unsafe { msg_send![super(this), init] }
    }

    fn report_failure(&self, web_view: &WKWebView, error: &NSError) {
        // A cancelled or policy-interrupted navigation is NOT a page failure
        // (Stop and a superseding navigation both produce one), so the lifecycle
        // is left alone rather than flipped to Failed.
        let Some(reason) =
            navigation_failure(error.code(), &error.localizedDescription().to_string())
        else {
            return;
        };
        let url = current_url_of(web_view);
        let url = if url.is_empty() {
            self.ivars()
                .life
                .borrow()
                .current_url()
                .unwrap_or_default()
                .to_string()
        } else {
            url
        };
        self.ivars().life.borrow_mut().fail(&url, &reason);
    }
}

/// The `WKWebView` property the SPA same-document observation watches.
const URL_KEY_PATH: &str = "URL";

/// The webview's current URL as a plain string (empty when it has none).
fn current_url_of(web_view: &WKWebView) -> String {
    unsafe {
        web_view
            .URL()
            .and_then(|url| url.absoluteString())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}

/// Load `url` on the webview through the NORMAL request path, so every
/// registered scheme handler (and every scheme refusal) still applies.
fn load_url(web_view: &WKWebView, url: &str) {
    let Some(ns_url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return;
    };
    let request = NSURLRequest::requestWithURL(&ns_url);
    unsafe { web_view.loadRequest(&request) };
}

impl Drop for MacosRenderer {
    fn drop(&mut self) {
        // The KVO registration `realize` made must not outlive the engine.
        self.stop_observing_url();
    }
}

/// A `Send` queue of browser -> page response JS.
///
/// The seam's [`ScriptMessageHandler`] is `Send`, but a `WKWebView` is
/// main-thread-only, so a provider handler cannot capture the view. It captures
/// THIS instead and the backend drains it on the main thread -- the same split
/// the iOS edge's `eval_sink` uses, for the same reason.
#[derive(Default, Clone)]
struct PendingEval(Arc<Mutex<Vec<String>>>);

impl PendingEval {
    fn push(&self, script: String) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push(script);
        }
    }

    fn drain(&self) -> Vec<String> {
        match self.0.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }
}

/// A [`Renderer`] backed by the macOS system webview (`WKWebView`).
///
/// Construct with [`MacosRenderer::new`], install the trust hooks
/// ([`install_ipfs`](MacosRenderer::install_ipfs) +
/// [`install_provider`](MacosRenderer::install_provider)), then embed
/// [`view_handle`](Renderer::view_handle) in a window and turn the AppKit run
/// loop. The webview's navigation signals feed a shared `LoadLifecycle`, so
/// [`load_state`](Renderer::load_state), [`current_url`](Renderer::current_url)
/// and [`poll_event`](Renderer::poll_event) report the same load-lifecycle
/// surface as any other backend.
///
/// The WINDOW is deliberately NOT this type's business: the AppKit window, the
/// URL bar, the trust indicator, the menus and the debug view are the sibling
/// task `macos-appkit-window-and-chrome`. [`host_in_bare_window`](MacosRenderer::host_in_bare_window)
/// exists only so this crate's own CI smoke can give the view a host.
pub struct MacosRenderer {
    mtm: MainThreadMarker,
    /// The EAGER container the shell embeds. It exists from construction so
    /// [`view_handle`](Renderer::view_handle) is valid before the engine is
    /// realised (see the module docs on lazy creation).
    container: Retained<NSView>,
    configuration: Retained<WKWebViewConfiguration>,
    content: Retained<WKUserContentController>,
    /// The engine, realised on the first navigation.
    view: Option<Retained<WKWebView>>,
    navigation: Retained<NavigationBridge>,
    /// Every registered scheme bridge, kept alive here (WebKit does not own its
    /// scheme handlers) and pumped for off-thread completions.
    scheme_bridges: Vec<Retained<SchemeBridge>>,
    /// Every registered script bridge, kept alive here for the same reason.
    script_bridges: Vec<Retained<ScriptBridge>>,
    /// The browser -> page response-JS queues the script bridges push into.
    pending_eval: Vec<PendingEval>,
    life: SharedLifecycle,
}

impl MacosRenderer {
    /// Create the backend: an eager container `NSView` plus the configuration
    /// the engine will be realised from.
    ///
    /// Fails with [`RendererError::Backend`] when called off the main thread --
    /// every AppKit and WebKit object here is main-thread-only, and the shared
    /// lifecycle is `!Send` for the same reason.
    pub fn new() -> Result<Self, RendererError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            RendererError::Backend(
                "the macOS backend must be created on the main thread (AppKit and WebKit are \
                 main-thread-only)"
                    .into(),
            )
        })?;

        let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
        let content = unsafe { WKUserContentController::new(mtm) };
        unsafe { configuration.setUserContentController(&content) };

        let container = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1024.0, 768.0)),
        );

        let life: SharedLifecycle = SharedLifecycle::new(RefCell::new(LoadLifecycle::default()));
        let navigation = NavigationBridge::new(mtm, life.clone());

        Ok(Self {
            mtm,
            container,
            configuration,
            content,
            view: None,
            navigation,
            scheme_bridges: Vec::new(),
            script_bridges: Vec::new(),
            pending_eval: Vec::new(),
            life,
        })
    }

    /// Realise the `WKWebView` now, if it does not exist yet.
    ///
    /// Called automatically by the first [`navigate`](Renderer::navigate). Every
    /// custom scheme MUST be registered before this point, because
    /// `WKWebViewConfiguration` is copied at construction and a scheme cannot be
    /// added afterwards -- the macOS face of ADR-0011 finding 5's
    /// scheme-set-is-fixed constraint, answered by DEFERRING construction rather
    /// than by widening the seam.
    pub fn realize(&mut self) -> &WKWebView {
        if self.view.is_none() {
            let frame = self.container.frame();
            let view = unsafe {
                WKWebView::initWithFrame_configuration(
                    WKWebView::alloc(self.mtm),
                    frame,
                    &self.configuration,
                )
            };
            unsafe {
                let navigation_delegate = ProtocolObject::from_ref(&*self.navigation);
                view.setNavigationDelegate(Some(navigation_delegate));
                let ui_delegate = ProtocolObject::from_ref(&*self.navigation);
                view.setUIDelegate(Some(ui_delegate));
                // SPA same-document URL tracking: observe the webview's own URL
                // property, the only signal a `pushState` leaves behind.
                let observer: &NSObject = &self.navigation;
                view.addObserver_forKeyPath_options_context(
                    observer,
                    &NSString::from_str(URL_KEY_PATH),
                    NSKeyValueObservingOptions::New,
                    std::ptr::null_mut(),
                );
                self.container.addSubview(&view);
                // The user-content controller the ENGINE actually consults is the
                // one on its own (copied) configuration; adopt it so later
                // `inject_script` / script-bridge registrations still land.
                self.content = view.configuration().userContentController();
            }
            self.view = Some(view);
        }
        self.view.as_ref().expect("realised just above")
    }

    /// The live `WKWebView`, if it has been realised.
    #[must_use]
    pub fn web_view(&self) -> Option<&WKWebView> {
        self.view.as_deref()
    }

    /// The OS light/dark preference this machine reports, mapped through the
    /// shared [`OsColorScheme`] rule (`docs/adr/0009`: FOLLOW, never force).
    ///
    /// macOS is the one edge where following costs nothing: AppKit propagates the
    /// effective `NSAppearance` into the `WKWebView`'s web process, so
    /// `prefers-color-scheme` already matches the OS and werust must set NOTHING
    /// (the GTK edge has to set `gtk-application-prefer-dark-theme` precisely
    /// because GTK does not inherit the desktop preference). This reader exists so
    /// the chrome can paint from the SAME signal the other edges use, and so
    /// "macOS follows the OS" is a checkable fact rather than an assumption.
    #[must_use]
    pub fn os_color_scheme(&self) -> OsColorScheme {
        let app = NSApplication::sharedApplication(self.mtm);
        let name = app.effectiveAppearance().name();
        os_color_scheme_from_appearance(&name.to_string())
    }

    /// Install the native EIP-1193 provider over the seam's script-message
    /// bridge -- werust's FIRST trust hook (`CONTEXT.md`, `docs/adr/0001`).
    ///
    /// The twin of the WebKitGTK backend's `install_provider` and of the iOS
    /// edge's, routed through the SAME `werust_core::provider` path: the page-side
    /// [`provider_shim`](werust_core::provider::provider_shim) is injected at
    /// document start so every page sees a detectable `window.ethereum`; the
    /// [`PROVIDER_BRIDGE`](werust_core::provider::PROVIDER_BRIDGE) channel is
    /// registered page -> native; and each answer is pushed BACK into the live
    /// page by evaluating the settle-call JS, resolving the page's pending
    /// Promise. It holds NO keys (a read-only stub), the same security posture as
    /// every other edge.
    pub fn install_provider(&mut self) {
        use werust_core::provider::{
            provider_shim, route_provider_message, ProviderBridge, PROVIDER_BRIDGE,
        };

        // The response push evaluates JS in the live document. The seam's
        // `ScriptMessageHandler` is `Send` while a `WKWebView` is not, so the
        // handler captures a `Send` queue and the backend drains it on the main
        // thread -- the same shape the iOS edge uses, for the same reason.
        let sink = PendingEval::default();
        let sink_for_handler = sink.clone();
        let bridge = ProviderBridge::new();
        self.register_script_message_handler(
            PROVIDER_BRIDGE,
            Box::new(move |message| {
                route_provider_message(&bridge, &message, &mut |script| {
                    sink_for_handler.push(script);
                });
            }),
        );
        self.pending_eval.push(sink);
        // Make the provider detectable from document start, exactly as desktop
        // and iOS do.
        self.inject_script(&provider_shim());
    }

    /// Install native `ipfs://` resolution over the seam's custom-scheme hook --
    /// werust's SECOND trust hook (`CONTEXT.md`, `docs/adr/0001`).
    ///
    /// An `ipfs://<cid>/...` URL is intercepted by a `WKURLSchemeHandler`,
    /// resolved through the SAME hash-verified
    /// `werust_core::ipfs::resolve_ipfs_request` path desktop and both mobile
    /// edges use (a CAR fetched from an UNTRUSTED trustless gateway, EVERY block
    /// verified against its own CID, the UnixFS DAG reassembled locally), and only
    /// then rendered. Verification GATES the load: a hash mismatch fails it rather
    /// than rendering unverified bytes.
    ///
    /// The blocking retrieval runs OFF the main thread and its completion is
    /// applied back on it (`docs/adr/0008`), through the SHARED
    /// [`webview_shared::offthread`] boundary.
    ///
    /// Returns the `_redirects` 3xx `RedirectSink` the handler pushes redirect
    /// targets into; the shell drains it on its pump to perform the navigation a
    /// scheme handler cannot.
    pub fn install_ipfs(&mut self) -> werust_core::ipfs::RedirectSink {
        use fetcher::{HttpFetcher, TrustlessGatewayCarRetriever};
        use werust_core::ipfs::{RedirectSink, IPFS_SCHEME};

        // The gateway endpoint is the USER'S chosen retrieval backend, read from
        // the persisted setting; the per-block verify above it is what makes any
        // endpoint safe.
        let retriever = Arc::new(TrustlessGatewayCarRetriever::with_gateway(
            HttpFetcher::new(),
            &werust_core::retrieval::active_gateway_endpoint(),
        ));
        let redirects = RedirectSink::new();
        let redirects_for_handler = redirects.clone();
        self.install_verifying_scheme(
            IPFS_SCHEME,
            Arc::new(move |uri: String| {
                webview_shared::offthread::retrieve_off_thread(
                    retriever.as_ref(),
                    uri,
                    &redirects_for_handler,
                )
            }),
        );

        // The internal `werust://settings` page: an ordinary (unverified)
        // internal page, so it takes the SYNCHRONOUS seam route and never marks
        // the load content-verified.
        self.register_scheme_handler(
            werust_core::retrieval::WERUST_SCHEME,
            Box::new(|request| werust_core::retrieval::apply_settings_request(&request)),
        );

        redirects
    }

    /// Register `scheme` on the VERIFYING off-thread route: `resolver` runs on a
    /// worker and a successful outcome marks the current load content-verified
    /// when the completion is applied on the main thread.
    ///
    /// Exposed (rather than kept private to [`install_ipfs`](MacosRenderer::install_ipfs))
    /// so the trust hook can be exercised against a PINNED, network-free
    /// content-addressed fixture in CI: the same verifying core path, the same
    /// off-thread boundary, no gateway.
    pub fn install_verifying_scheme(&mut self, scheme: &str, resolver: Arc<dyn OffThreadResolve>) {
        let bridge = SchemeBridge::new(self.mtm, Route::OffThread(resolver), self.life.clone());
        self.attach_scheme_bridge(scheme, bridge);
    }

    fn attach_scheme_bridge(&mut self, scheme: &str, bridge: Retained<SchemeBridge>) {
        if self.view.is_some() {
            // The configuration was already copied into the engine, so WebKit
            // will never consult a handler added now. This cannot be REPORTED
            // through the seam (`register_scheme_handler` returns unit and the
            // trait must not widen), so it is stated loudly instead: the shell's
            // contract is to install every scheme BEFORE the first navigation,
            // which the lazy realisation exists to make always possible.
            eprintln!(
                "werust(macos): scheme `{scheme}` was registered AFTER the webview was realised \
                 and will NOT be intercepted; register every scheme before the first navigate"
            );
            return;
        }
        unsafe {
            let handler = ProtocolObject::from_ref(&*bridge);
            self.configuration
                .setURLSchemeHandler_forURLScheme(Some(handler), &NSString::from_str(scheme));
        }
        self.scheme_bridges.push(bridge);
    }

    /// Apply every `ipfs://` completion whose off-thread verification has
    /// finished, on THIS (the main) thread.
    ///
    /// [`poll_event`](Renderer::poll_event) calls this on every drain, so a shell
    /// that already pumps the seam needs no extra wiring; it is public so a
    /// driver with its own loop (the CI smoke) can pump explicitly. Returns how
    /// many requests were completed.
    pub fn pump_scheme_completions(&mut self) -> usize {
        self.scheme_bridges
            .iter()
            .map(|bridge| bridge.drain_completions())
            .sum()
    }

    /// Drain the browser -> page response JS the provider bridge queued, and
    /// evaluate it in the live page.
    fn pump_pending_eval(&mut self) {
        let scripts: Vec<String> = self
            .pending_eval
            .iter()
            .flat_map(PendingEval::drain)
            .collect();
        for script in scripts {
            self.evaluate_javascript(&script);
        }
    }

    /// Stop observing the webview's URL, so the engine is never deallocated with
    /// a live KVO registration.
    ///
    /// Called from [`Drop`], because AppKit requires an observer to be removed
    /// BEFORE the observed object goes away (an observed `WKWebView` that
    /// deallocates with a registered observer is a documented hazard, not a
    /// tidiness issue). Idempotent: it does nothing when no engine was realised.
    fn stop_observing_url(&mut self) {
        if let Some(view) = &self.view {
            let observer: &NSObject = &self.navigation;
            unsafe {
                view.removeObserver_forKeyPath(observer, &NSString::from_str(URL_KEY_PATH));
            }
        }
    }

    /// Put the backend's container view in a BARE, borderless window so WebKit
    /// has a host to render into.
    ///
    /// This is NOT the product's window: the AppKit window, the URL bar, the
    /// trust indicator, the menus and the debug view are the sibling task
    /// `macos-appkit-window-and-chrome`. It exists so this crate can be RUN --
    /// the CI smoke drives a real load through a real engine with it -- without
    /// pretending to ship a shell. The window is deliberately positioned far
    /// off-screen and the app runs as an ACCESSORY (no Dock icon, no menu bar),
    /// so a CI run shows nothing and steals no focus.
    pub fn host_in_bare_window(&mut self) -> Retained<objc2_app_kit::NSWindow> {
        use objc2_app_kit::{
            NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
        };

        let app = NSApplication::sharedApplication(self.mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let frame = NSRect::new(
            NSPoint::new(-20_000.0, -20_000.0),
            NSSize::new(1024.0, 768.0),
        );
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(self.mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setContentView(Some(&self.container));
        // A window WebKit will actually render into, without raising it over
        // anything the user is doing.
        window.orderBack(None);
        window
    }
}

impl Renderer for MacosRenderer {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        validate_url(url)?;
        // Optimistically reflect the started load, exactly as the GTK backend
        // does, so the seam is well-defined before the run loop turns.
        self.life.borrow_mut().begin(url);
        let view = self.realize();
        load_url(view, url);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        if self.life.borrow().current_url().is_none() {
            return Err(RendererError::Backend("nothing to reload".into()));
        }
        if let Some(view) = &self.view {
            unsafe { view.reload() };
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(view) = &self.view {
            unsafe { view.stopLoading() };
        }
        self.life.borrow_mut().stop();
    }

    fn go_back(&mut self) {
        // WebKit owns the session (back/forward) list; a back navigation restarts
        // the load and the navigation delegate feeds the shared lifecycle exactly
        // as a fresh `navigate` does. Guarded so a stray call at the start of
        // history is a no-op.
        if let Some(view) = &self.view {
            if unsafe { view.canGoBack() } {
                unsafe { view.goBack() };
            }
        }
    }

    fn go_forward(&mut self) {
        if let Some(view) = &self.view {
            if unsafe { view.canGoForward() } {
                unsafe { view.goForward() };
            }
        }
    }

    fn can_go_back(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|view| unsafe { view.canGoBack() })
    }

    fn can_go_forward(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|view| unsafe { view.canGoForward() })
    }

    fn load_state(&self) -> LoadState {
        self.life.borrow().state()
    }

    fn current_url(&self) -> Option<String> {
        self.life.borrow().current_url().map(str::to_string)
    }

    fn poll_event(&mut self) -> Option<LoadEvent> {
        // Pumping HERE is what makes ADR-0008's marshalling work with no extra
        // wiring: any driver that already drains the seam's events also applies
        // the off-thread `ipfs://` completions and the provider's response push,
        // on the main thread.
        self.pump_scheme_completions();
        self.pump_pending_eval();
        self.life.borrow_mut().poll()
    }

    fn view_handle(&self) -> ViewHandle {
        // The EAGER container, valid from construction even before the engine is
        // realised -- which is the whole reason the split exists.
        ViewHandle(Retained::as_ptr(&self.container) as *mut c_void)
    }

    fn send_pointer(&mut self, _event: PointerEvent) {
        // AppKit routes real pointer input to the webview through the responder
        // chain; synthetic injection is not part of the public API. The hook stays
        // on the seam for backends that own their own input path (a future native
        // renderer) -- the same position the WebKitGTK backend takes.
    }

    fn send_key(&mut self, _event: KeyEvent) {
        // As with pointer input: real key events reach the focused webview via
        // AppKit's responder chain.
    }

    fn send_scroll(&mut self, _delta: ScrollDelta) {
        // Scrolling is handled by the webview from real input.
    }

    fn set_focus(&mut self, focused: bool) {
        if focused {
            if let Some(view) = &self.view {
                if let Some(window) = view.window() {
                    window.makeFirstResponder(Some(view));
                }
            }
        }
    }

    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
        // Route `window.webkit.messageHandlers.<name>.postMessage(...)` up to the
        // handler. This is the channel the EIP-1193 provider is injected over
        // (task `eip1193-provider-injection-via-script-bridge`).
        let bridge = ScriptBridge::new(self.mtm, name, handler);
        unsafe {
            let protocol = ProtocolObject::from_ref(&*bridge);
            self.content
                .addScriptMessageHandler_name(protocol, &NSString::from_str(name));
        }
        self.script_bridges.push(bridge);
    }

    fn inject_script(&mut self, script: &str) {
        // A document-start user script in ALL frames: the same reach WebKitGTK's
        // `UserContentInjectedFrames::AllFrames` + `InjectionTime::Start` gives,
        // so the provider is detectable before the page's first line runs.
        let user_script = unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(self.mtm),
                &NSString::from_str(script),
                WKUserScriptInjectionTime::AtDocumentStart,
                false,
            )
        };
        unsafe { self.content.addUserScript(&user_script) };
    }

    fn evaluate_javascript(&self, script: &str) {
        // Push JS into the live page (browser -> page): the response half of the
        // script-message bridge that settles the EIP-1193 provider's pending
        // Promise. Fire-and-forget, matching the seam's `&self`, no-result
        // contract. A backend with no realised engine has no live document, so
        // there is nothing to evaluate into.
        if let Some(view) = &self.view {
            unsafe {
                view.evaluateJavaScript_completionHandler(&NSString::from_str(script), None);
            }
        }
    }

    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
        // Intercept `<scheme>://...` requests and answer them from the handler,
        // SYNCHRONOUSLY on the main thread -- the same contract the WebKitGTK
        // backend's `register_scheme_handler` has. The VERIFYING `ipfs://` route
        // (which must not block the main thread and which marks the trust
        // posture) is `install_ipfs` / `install_verifying_scheme` instead, exactly
        // as desktop splits them.
        let bridge = SchemeBridge::new(
            self.mtm,
            Route::Sync(RefCell::new(handler)),
            self.life.clone(),
        );
        self.attach_scheme_bridge(scheme, bridge);
    }

    fn trust_hooks(&self) -> TrustHooks {
        // OPT IN to BOTH trust hooks: this backend genuinely wires them -- EIP-1193
        // provider injection over the script-message bridge (`install_provider`:
        // a real `WKScriptMessageHandler` + a document-start `WKUserScript` + the
        // `evaluateJavaScript` response push) and `ipfs://` custom-scheme
        // resolution (`install_ipfs`: a real `WKURLSchemeHandler` -> the
        // hash-verified core path). The seam default is FAIL-CLOSED
        // (`TrustHooks::none()`), so trust is never inherited by omission.
        TrustHooks::all()
    }

    fn trust_posture(&self) -> TrustPosture {
        // Read the shared lifecycle's posture: `ContentVerified` (or the louder
        // ENS/mutable variant) iff the current page's bytes came back through the
        // hash-verified `ipfs://` path, else the served-origin posture. Never
        // inferred from the URL string.
        self.life.borrow().posture()
    }

    fn mark_ens_origin(&mut self) {
        // Flag the current load ENS-originated on the SHARED lifecycle, so when
        // the scheme handler later verifies the bytes the posture surfaces
        // `NameViaTrustedRpc` instead of the plain `ContentVerified`. A fresh
        // `begin` clears the flag.
        self.life.borrow_mut().mark_ens_origin();
    }

    fn mark_mutable_name(&mut self) {
        // Flag the current load's name MUTABLE (an IPNS resolution) on the SHARED
        // lifecycle, so a verified load surfaces at most `MutableName` -- or the
        // louder `NameViaTrustedRpc` if also ENS-originated (the two-axis display
        // rule). A fresh `begin` clears the flag.
        self.life.borrow_mut().mark_mutable_name();
    }
}
