//! The real Windows backend: an Edge **WebView2** wired behind the [`Renderer`]
//! seam.
//!
//! [`Webview2Renderer`] is the piece that actually shows a page on Windows. It
//! binds Microsoft's system webview through `webview2-com` + `webview2-com-sys`
//! (ADR-0011 finding 4: the bindings `wry` itself depends on, NEVER the abandoned
//! `webview2` crate), and nothing WebView2-specific leaks past the seam: the rest
//! of werust only ever sees the [`Renderer`] trait.
//!
//! # What it leans on, and what it does NOT fork
//!
//! * The load-lifecycle state machine, the `navigate` URL rule and the ADR-0008
//!   off-thread `ipfs://` boundary are the SHARED, toolkit-free
//!   [`webview_shared`] crate -- the very code the WebKitGTK and WKWebView
//!   backends run. CONSUMED, never copied, so three system-webview backends
//!   cannot drift in what a load state, a rejected URL or a verified load MEANS.
//! * `ipfs://` resolution itself is `werust_core::ipfs::resolve_ipfs_request` over
//!   the `fetcher` verifying retriever, the SAME path desktop, macOS, Android and
//!   iOS all use.
//! * The EIP-1193 provider is `werust_core::provider` (`provider_shim` +
//!   `route_provider_message` + the keyless read-only `ProviderBridge`), the SAME
//!   path every other edge uses. Only the page-side TRANSPORT differs, and that
//!   is bridged by an adapter (see [`crate::pure::bridge_adapter_script`]) exactly
//!   as the Android edge bridges it, rather than by forking the shared shim.
//! * The seam BOOKKEEPING shape -- eager container + lazy engine, an in-flight
//!   request table keyed by id, a worker boundary only a `Send` outcome crosses,
//!   a `PendingEval` queue for the browser -> page response push, and pumping
//!   those on `poll_event` -- is taken from `crates/macos-renderer` (and through
//!   it the iOS edge). What is genuinely NEW here is only the COM wiring.
//!
//! # Why the environment is created LAZILY
//!
//! WebView2 fixes the SET of custom scheme NAMES at ENVIRONMENT creation
//! (`ICoreWebView2EnvironmentOptions4::SetCustomSchemeRegistrations`) and makes it
//! immutable for the browser-process lifetime, while the seam's
//! [`register_scheme_handler`](Renderer::register_scheme_handler) is called AFTER
//! construction. ADR-0011 finding 5 prescribes the answer, and it is NOT a trait
//! change: an EAGER container `HWND` (so [`view_handle`](Renderer::view_handle)
//! works from construction) plus a LAZY environment + controller, realised on the
//! first [`navigate`](Renderer::navigate) -- by which time the shell has
//! registered its schemes. This is the identical constraint, and the identical
//! answer, the macOS backend needs for `WKWebViewConfiguration`.
//!
//! # Threading
//!
//! A WebView2 controller lives on the thread that created it (an STA with a
//! message loop), and the shared `LoadLifecycle` is `!Send` for that reason. The
//! blocking `ipfs://` retrieval therefore runs on a worker thread and hands back
//! only the `Send` [`RetrievalOutcome`]; the completion (set the response, finish
//! the deferral, mark the posture) is applied on the message-loop thread by
//! [`pump_scheme_completions`](Webview2Renderer::pump_scheme_completions), which
//! [`poll_event`](Renderer::poll_event) calls on every drain. That is ADR-0008's
//! rule, satisfied with the SAME shared boundary the other two backends use --
//! only the glue differs (`GetDeferral` / `Deferral::Complete` instead of
//! `gio::spawn_blocking` or a main-queue hop), exactly as the spike predicted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, ContentLoadingEventHandler, CoreWebView2CustomSchemeRegistration,
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler, SourceChangedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
};
use windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{E_POINTER, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetWindowLongPtrW, PeekMessageW, RegisterClassW, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, MSG, PM_REMOVE, SW_SHOWNOACTIVATE,
    WINDOW_EX_STYLE, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

use renderer::{
    new_window_action, KeyEvent, LoadEvent, LoadState, NewWindowAction, OsColorScheme,
    PointerEvent, Renderer, RendererError, SchemeHandler, SchemeRequest, SchemeResponse,
    ScriptMessageHandler, ScrollDelta, TrustHooks, TrustPosture, ViewHandle,
};
use webview_shared::offthread::{complete_ipfs_request, RequestSink, RetrievalOutcome};
use webview_shared::{validate_url, LoadLifecycle, SharedLifecycle};

use crate::pure::{
    bridge_adapter_script, missing_runtime_error, navigation_failure,
    os_color_scheme_from_apps_use_light_theme, parse_bridge_envelope, reason_phrase, scheme_filter,
    scheme_of, SCHEME_HAS_AUTHORITY_COMPONENT, SCHEME_TREAT_AS_SECURE,
};

/// Resolve one intercepted request OFF the message-loop thread.
///
/// The off-thread half of the verifying `ipfs://` route, kept behind a
/// `Send + Sync` trait so the production path
/// (`werust_core::ipfs::resolve_ipfs_request` over the trustless-gateway CAR
/// retriever) and an offline, PINNED test double are the same code path from the
/// backend's point of view -- which is what lets the trust hook be exercised in
/// CI with no gateway and no network. The twin of the macOS backend's trait of
/// the same name.
///
/// It returns the shared [`RetrievalOutcome`], the ONLY value that crosses the
/// worker/message-loop boundary (`docs/adr/0008`).
pub trait OffThreadResolve: Send + Sync + 'static {
    /// Resolve `uri`, blocking. Runs on a worker thread, never the UI thread.
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
    /// answered SYNCHRONOUSLY on the UI thread and NOT marked verified --
    /// byte-for-byte the contract the other backends' `register_scheme_handler`
    /// has. Used for the internal `werust://` pages.
    Sync(RefCell<SchemeHandler>),
    /// The production `ipfs://` route (`install_ipfs`): the blocking verify runs
    /// on a worker behind a `GetDeferral`, and the completion -- including the
    /// content-verified mark -- is applied on the message-loop thread
    /// (`docs/adr/0008`).
    OffThread(Arc<dyn OffThreadResolve>),
}

/// One intercepted request whose off-thread resolution is still in flight. The
/// event args and the deferral are HELD (that is exactly what a deferral is for)
/// until the worker's outcome comes back.
struct InFlight {
    id: u64,
    uri: String,
    args: ICoreWebView2WebResourceRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
}

/// Everything the ONE `WebResourceRequested` handler needs, shared between the
/// COM closure and the backend.
///
/// WebView2 raises a SINGLE `WebResourceRequested` event for every filter, so
/// there is one handler for all registered schemes and it routes on the request's
/// scheme -- unlike WebKit, which hands each scheme its own handler object.
struct SchemeState {
    /// scheme -> how to answer it. `Rc` so the handler can clone the route out
    /// under a short borrow and never hold the map borrowed while user code runs.
    routes: RefCell<HashMap<String, Rc<Route>>>,
    /// The SHARED load lifecycle, mutated ONLY on this (the message-loop) thread.
    life: SharedLifecycle,
    /// The environment, needed to build responses. Available once realised.
    environment: RefCell<Option<ICoreWebView2Environment>>,
    /// Requests whose off-thread resolution is still in flight, in start order.
    inflight: RefCell<Vec<InFlight>>,
    next_id: Cell<u64>,
    outcomes_tx: Sender<(u64, RetrievalOutcome)>,
    outcomes_rx: Receiver<(u64, RetrievalOutcome)>,
    /// The reason the scheme route last failed a request with.
    ///
    /// WebView2's `NavigationCompleted` can only say that some resource did not
    /// load; the scheme route knows it was a hash mismatch. Carrying the honest
    /// reason across is what gives Windows the same legible failure text
    /// WebKitGTK gets from `finish_error`.
    last_error: RefCell<Option<String>>,
}

impl SchemeState {
    fn new(life: SharedLifecycle) -> Rc<Self> {
        let (outcomes_tx, outcomes_rx) = channel();
        Rc::new(SchemeState {
            routes: RefCell::new(HashMap::new()),
            life,
            environment: RefCell::new(None),
            inflight: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            outcomes_tx,
            outcomes_rx,
            last_error: RefCell::new(None),
        })
    }

    /// Answer one intercepted request: synchronously for the plain seam route, or
    /// on a worker behind a deferral for the verifying `ipfs://` route.
    fn start(
        &self,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
    ) -> windows::core::Result<()> {
        let uri = unsafe {
            let request = args.Request()?;
            let mut uri = PWSTR::null();
            request.Uri(&mut uri)?;
            take_pwstr(uri)
        };
        let Some(route) =
            scheme_of(&uri).and_then(|scheme| self.routes.borrow().get(scheme).cloned())
        else {
            // Not ours: leave it entirely alone so WebView2 handles it normally.
            return Ok(());
        };
        match &*route {
            Route::Sync(handler) => {
                // Answered inline on the UI thread, exactly as the other two
                // backends' `register_scheme_handler` does -- and, exactly as
                // there, WITHOUT marking the load content-verified: only the
                // verifying route may do that.
                let outcome = (handler.borrow_mut())(SchemeRequest { uri: uri.clone() });
                let mut sink = ResponseSink {
                    state: self,
                    args,
                    uri: &uri,
                };
                match outcome {
                    Ok(response) => sink.finish(response),
                    Err(error) => sink.fail(error),
                }
            }
            Route::OffThread(resolver) => {
                // OFF THE UI THREAD (`docs/adr/0008`). The deferral is what lets
                // this event return immediately while the blocking CAR fetch +
                // per-block verify runs elsewhere. Only a `Send` value crosses:
                // the resolver clone and the URI go out, a `RetrievalOutcome`
                // comes back. NOTHING COM and NOTHING `!Send` (not the event
                // args, not the deferral, not the shared lifecycle) is ever
                // touched off this thread.
                let deferral = unsafe { args.GetDeferral()? };
                let id = self.next_id.get();
                self.next_id.set(id.wrapping_add(1));
                self.inflight.borrow_mut().push(InFlight {
                    id,
                    uri: uri.clone(),
                    args: args.clone(),
                    deferral,
                });
                let resolver = resolver.clone();
                let sender = self.outcomes_tx.clone();
                std::thread::spawn(move || {
                    let outcome = resolver.resolve(uri);
                    // A closed channel means the backend is gone; nothing to do.
                    let _ = sender.send((id, outcome));
                });
            }
        }
        Ok(())
    }

    /// Apply every off-thread outcome that has arrived, ON THIS (the
    /// message-loop) thread: set the verified bytes as the response and mark the
    /// shared load content-verified, or fail it closed WITHOUT marking. Either
    /// way the deferral is completed, so WebView2 is never left waiting.
    ///
    /// Returns how many completions were applied.
    fn drain_completions(&self) -> usize {
        let mut applied = 0;
        while let Ok((id, outcome)) = self.outcomes_rx.try_recv() {
            let held = {
                let mut inflight = self.inflight.borrow_mut();
                inflight
                    .iter()
                    .position(|held| held.id == id)
                    .map(|index| inflight.remove(index))
            };
            let Some(held) = held else { continue };
            {
                let mut sink = ResponseSink {
                    state: self,
                    args: &held.args,
                    uri: &held.uri,
                };
                // The ONE shared completion rule: verified bytes render and mark
                // the posture; ANY failure fails the load closed and leaves the
                // posture untouched.
                complete_ipfs_request(outcome, &mut sink, &self.life);
            }
            unsafe {
                let _ = held.deferral.Complete();
            }
            applied += 1;
        }
        applied
    }
}

/// A [`RequestSink`] over a live `WebResourceRequested`, so the SHARED completion
/// logic (mark-verified-only-on-success, fail-closed-on-error) drives WebView2's
/// response API without knowing anything about it.
struct ResponseSink<'a> {
    state: &'a SchemeState,
    args: &'a ICoreWebView2WebResourceRequestedEventArgs,
    uri: &'a str,
}

impl RequestSink for ResponseSink<'_> {
    fn finish(&mut self, response: SchemeResponse) {
        let environment = self.state.environment.borrow().clone();
        let Some(environment) = environment else {
            self.fail(RendererError::Backend(
                "no WebView2 environment to answer an intercepted request with".into(),
            ));
            return;
        };
        // The honest STATUS travels with the bytes: a content-addressed site may
        // name its OWN error page for a missing path through the IPFS
        // `_redirects` convention (IPIP-0002), and answering that page with 200
        // would LIE about a page the site declared missing (the seam carries
        // `SchemeResponse::status` for exactly this reason). WebView2's
        // `CreateWebResourceResponse` takes a real status code, so this row needs
        // no fabrication.
        let built = unsafe {
            let stream = SHCreateMemStream(Some(&response.body));
            let headers = HSTRING::from(format!("Content-Type: {}", response.mime_type));
            let reason = HSTRING::from(reason_phrase(response.status));
            environment.CreateWebResourceResponse(
                stream.as_ref(),
                i32::from(response.status),
                &reason,
                &headers,
            )
        };
        match built {
            Ok(built) => {
                if let Err(error) = unsafe { self.args.SetResponse(&built) } {
                    self.fail(RendererError::Backend(format!(
                        "could not answer the intercepted request: {error}"
                    )));
                }
            }
            Err(error) => self.fail(RendererError::Backend(format!(
                "could not build a response for the intercepted request: {error}"
            ))),
        }
    }

    fn fail(&mut self, error: RendererError) {
        // FAIL CLOSED: NO response is set at all, so not one unverified byte
        // reaches the engine, and (on the verifying route) the posture is never
        // marked. Built-in error pages are disabled, so WebView2 shows nothing
        // rather than substituting a document of its own.
        let reason = error.to_string();
        *self.state.last_error.borrow_mut() = Some(reason.clone());
        // If the resource that failed IS the current main document, fail the load
        // here and now with the HONEST reason. WebView2 also completes the
        // navigation unsuccessfully, but it can only report that some resource
        // did not load, so the `NavigationCompleted` handler skips an
        // already-failed load rather than reporting it twice.
        let mut life = self.state.life.borrow_mut();
        if life.state().is_loading() && life.current_url() == Some(self.uri) {
            life.fail(self.uri, &reason);
        }
    }
}

/// A `Send` queue of browser -> page response JS.
///
/// The seam's [`ScriptMessageHandler`] is `Send`, but a WebView2 is bound to its
/// message-loop thread, so a provider handler cannot capture the engine. It
/// captures THIS instead and the backend drains it on the UI thread -- the same
/// split the macOS backend and the iOS edge use, for the same reason.
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

/// The registered script-message bridges, shared with the `WebMessageReceived`
/// closure. WebView2 has ONE page -> host channel, so the envelope carries the
/// bridge name and this map routes on it.
type ScriptBridges = Rc<RefCell<HashMap<String, Rc<RefCell<ScriptMessageHandler>>>>>;

/// A [`Renderer`] backed by the Windows system webview (Edge WebView2).
///
/// Construct with [`Webview2Renderer::new`], install the trust hooks
/// ([`install_ipfs`](Webview2Renderer::install_ipfs) +
/// [`install_provider`](Webview2Renderer::install_provider)), then embed
/// [`view_handle`](Renderer::view_handle) in a window and turn the Win32 message
/// loop. The engine's navigation events feed a shared `LoadLifecycle`, so
/// [`load_state`](Renderer::load_state), [`current_url`](Renderer::current_url)
/// and [`poll_event`](Renderer::poll_event) report the same load-lifecycle
/// surface as any other backend.
///
/// The WINDOW is deliberately NOT this type's business: the Win32 window, the URL
/// bar, the trust indicator, the menus and the debug view are the sibling task
/// `windows-win32-window-and-chrome`.
/// [`host_in_bare_window`](Webview2Renderer::host_in_bare_window) exists only so
/// this crate's own CI smoke can give the engine a host.
pub struct Webview2Renderer {
    /// The EAGER container `HWND` the shell embeds. It exists from construction
    /// so [`view_handle`](Renderer::view_handle) is valid before the environment
    /// is created (see the module docs on lazy creation).
    container: HWND,
    /// Where the browser keeps its profile. Chosen at construction, used at
    /// realisation.
    user_data_folder: PathBuf,
    environment: Option<ICoreWebView2Environment>,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    /// Everything the one `WebResourceRequested` handler needs.
    schemes: Rc<SchemeState>,
    /// The registered script-message bridges, by channel name.
    bridges: ScriptBridges,
    /// Document-start scripts queued before the engine exists, applied in order
    /// at realisation (`AddScriptToExecuteOnDocumentCreated` is a method on the
    /// engine, which does not exist until the first navigate).
    pending_scripts: Vec<String>,
    /// The browser -> page response-JS queues the script bridges push into.
    pending_eval: Vec<PendingEval>,
    /// The live engine, shared with whatever [`DevTools`] handles the shell took
    /// before boxing this backend behind the seam.
    dev_tools: Rc<RefCell<Option<ICoreWebView2>>>,
    life: SharedLifecycle,
}

/// The shell's handle onto the platform's OWN devtools (`OpenDevToolsWindow`).
///
/// werust never re-implements devtools: this opens Edge's real DevTools window
/// over the live page. It is a handle rather than a method because the chrome
/// acts on a backend that is, by then, behind the `Renderer` seam; see
/// [`Webview2Renderer::dev_tools`].
#[derive(Clone, Default)]
pub struct DevTools {
    engine: Rc<RefCell<Option<ICoreWebView2>>>,
}

impl DevTools {
    /// Open the DevTools window over the live page. `false` when there is no
    /// engine yet (nothing has been navigated) or the runtime refused, so a
    /// caller can say so rather than appear to have done nothing.
    pub fn open(&self) -> bool {
        let engine = self.engine.borrow();
        let Some(engine) = engine.as_ref() else {
            return false;
        };
        unsafe { engine.OpenDevToolsWindow() }.is_ok()
    }
}

impl Webview2Renderer {
    /// Create the backend: an eager container `HWND`, and nothing else.
    ///
    /// Fails with an honest, NAMED [`RendererError::Backend`] when this machine
    /// has no WebView2 Runtime -- never a crash (`docs/adr/0011` finding 6; the
    /// message itself is [`crate::pure::missing_runtime_error`]). The check is
    /// made HERE as well as at environment creation, so a shell learns the truth
    /// before it opens a window it cannot fill.
    pub fn new() -> Result<Self, RendererError> {
        Self::with_user_data_folder(default_user_data_folder())
    }

    /// The same, with an explicit browser PROFILE folder.
    ///
    /// WebView2 stores its profile in a user-data folder that must be writable;
    /// the default is under the OS temp directory (see
    /// [`default_user_data_folder`]), which is right for an engine-only crate and
    /// for CI. A shell that wants a durable profile (the sibling
    /// `windows-win32-window-and-chrome`) passes its own path here rather than
    /// inheriting a temp one by accident.
    pub fn with_user_data_folder(user_data_folder: PathBuf) -> Result<Self, RendererError> {
        unsafe {
            // WebView2 requires an STA with a message loop on the calling thread.
            // A second call on an already-initialised apartment returns S_FALSE
            // (not a failure), so this is safe for a shell that already did it.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        // Fail HONESTLY and EARLY on a machine with no runtime.
        runtime_version()?;

        let container = create_container_window()?;
        let life: SharedLifecycle = SharedLifecycle::new(RefCell::new(LoadLifecycle::default()));
        Ok(Self {
            container,
            user_data_folder,
            environment: None,
            controller: None,
            webview: None,
            schemes: SchemeState::new(life.clone()),
            bridges: Rc::new(RefCell::new(HashMap::new())),
            pending_scripts: Vec::new(),
            pending_eval: Vec::new(),
            dev_tools: Rc::new(RefCell::new(None)),
            life,
        })
    }

    /// The WebView2 Runtime version this machine has, or the honest
    /// runtime-missing error.
    ///
    /// Worth recording WITH any Windows result, because the runtime is EVERGREEN,
    /// cannot be pinned, and this exact corner regressed in stable 144 in January
    /// 2026 (WebView2Feedback #5495).
    pub fn runtime_version() -> Result<String, RendererError> {
        runtime_version()
    }

    /// A handle onto the live engine for the SHELL's devtools affordance.
    ///
    /// Devtools are the platform's OWN (`OpenDevToolsWindow`), never a werust
    /// re-implementation, and opening them is a CHROME action -- but by the time
    /// the chrome exists, this backend has been boxed behind the `Renderer` seam
    /// and cannot be reached concretely. So the shell takes this handle BEFORE
    /// boxing (the same move the GTK shell makes with
    /// `backend.web_view().clone()` for the WebKitGTK inspector) and the COM call
    /// stays inside this crate, which keeps the window crate free of a WebView2
    /// dependency.
    #[must_use]
    pub fn dev_tools(&self) -> DevTools {
        DevTools {
            engine: Rc::clone(&self.dev_tools),
        }
    }

    /// Realise the environment, the controller and the engine now, if they do not
    /// exist yet.
    ///
    /// Called automatically by the first [`navigate`](Renderer::navigate). Every
    /// custom scheme MUST be registered before this point, because the SET of
    /// scheme names is fixed at environment creation and immutable for the
    /// browser-process lifetime -- the Windows face of ADR-0011 finding 5's
    /// constraint, answered by DEFERRING creation rather than by widening the
    /// seam.
    pub fn realize(&mut self) -> Result<(), RendererError> {
        if self.webview.is_some() {
            return Ok(());
        }
        let environment = self.create_environment()?;
        let controller = create_controller(&environment, self.container)?;
        let webview = unsafe { controller.CoreWebView2() }.map_err(|e| {
            RendererError::Backend(format!("ICoreWebView2Controller::CoreWebView2: {e}"))
        })?;

        *self.schemes.environment.borrow_mut() = Some(environment.clone());

        self.configure(&webview)?;
        self.wire_events(&webview)?;
        self.apply_document_start_scripts(&webview);

        unsafe {
            let mut rect = RECT::default();
            let _ = GetClientRect(self.container, &mut rect);
            let _ = controller.SetBounds(rect);
            let _ = controller.SetIsVisible(true);
        }

        // The container's own window proc keeps the controller's bounds in step
        // with the container when a SHELL resizes it (WebView2 has no
        // autoresizing): the controller is BORROWED through this slot, never
        // owned by it, and `Drop` clears the slot before closing the controller.
        unsafe {
            SetWindowLongPtrW(self.container, GWLP_USERDATA, controller.as_raw() as isize);
        }

        *self.dev_tools.borrow_mut() = Some(webview.clone());
        self.environment = Some(environment);
        self.controller = Some(controller);
        self.webview = Some(webview);
        Ok(())
    }

    /// The live engine, if it has been realised.
    #[must_use]
    pub fn web_view(&self) -> Option<&ICoreWebView2> {
        self.webview.as_ref()
    }

    /// Create the environment with EXACTLY the set of scheme names registered so
    /// far, each with the flags the origin probe MEASURED as giving a real
    /// `ipfs://<cid>` tuple origin (ADR-0011 Amendment 2).
    fn create_environment(&self) -> Result<ICoreWebView2Environment, RendererError> {
        let options = CoreWebView2EnvironmentOptions::default();
        // Sorted so the registration set is DETERMINISTIC. WebView2 requires every
        // environment sharing a browser process to register an IDENTICAL set, so
        // a stable order makes a mismatch a bug in the set, never in iteration.
        let mut schemes: Vec<String> = self.schemes.routes.borrow().keys().cloned().collect();
        schemes.sort();
        let registrations: Vec<Option<ICoreWebView2CustomSchemeRegistration>> = schemes
            .iter()
            .map(|scheme| {
                let registration = CoreWebView2CustomSchemeRegistration::new(scheme.clone());
                unsafe {
                    // TRUE + TRUE is the MEASURED combination: a real tuple origin
                    // and a secure context. The probe's negative control is the
                    // identical run with the authority flag OFF, and it
                    // reproduced the Android opaque-origin failure verbatim.
                    registration.set_has_authority_component(SCHEME_HAS_AUTHORITY_COMPONENT);
                    registration.set_treat_as_secure(SCHEME_TREAT_AS_SECURE);
                    registration.set_allowed_origins(vec![scheme_filter(scheme)]);
                }
                Some(registration.into())
            })
            .collect();
        unsafe { options.set_scheme_registrations(registrations) };

        let (sender, receiver) = channel();
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &HSTRING::from(self.user_data_folder.as_os_str()),
                &ICoreWebView2EnvironmentOptions::from(options),
                &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                    move |code, environment| {
                        let result = (|| {
                            code?;
                            environment.ok_or_else(|| windows::core::Error::from(E_POINTER))
                        })();
                        let _ = sender.send(result);
                        Ok(())
                    },
                )),
            )
            .map_err(|e| missing_runtime_error(&e.to_string()))?;
        }
        webview2_com::wait_with_pump(receiver)
            .map_err(|e| {
                RendererError::Backend(format!("waiting for the WebView2 environment failed: {e}"))
            })?
            // A refusal HERE is still most often "no runtime", so it is reported
            // with the same honest, named message rather than as a raw HRESULT.
            .map_err(|e| missing_runtime_error(&e.to_string()))
    }

    /// The engine settings this backend depends on.
    fn configure(&self, webview: &ICoreWebView2) -> Result<(), RendererError> {
        unsafe {
            let settings = webview
                .Settings()
                .map_err(|e| RendererError::Backend(format!("ICoreWebView2::Settings: {e}")))?;
            let _ = settings.SetIsScriptEnabled(true);
            let _ = settings.SetIsWebMessageEnabled(true);
            // FAIL CLOSED, visibly: when a scheme route refuses to serve
            // unverified bytes it sets NO response, and a built-in error page
            // would replace that failure with a document of Edge's own (a
            // different origin) instead of showing nothing.
            let _ = settings.SetIsBuiltInErrorPageEnabled(false);
            // The `web-inspector` capability's recorded rule, applied to the
            // third desktop: the platform's OWN devtools are a DEVELOPER surface,
            // enabled in a debug build and NOT in a release one, so a shipped
            // binary is not silently inspectable (the same gating the WebKitGTK
            // `enable-developer-extras`, the iOS `isInspectable` and the Android
            // `setWebContentsDebuggingEnabled` rows apply). WebView2 defaults this
            // to TRUE, so leaving it unset would have made Windows the one
            // platform that ignores the rule.
            let _ = settings.SetAreDevToolsEnabled(cfg!(debug_assertions));
            // `docs/adr/0009`: FOLLOW the OS, never force dark. AUTO is documented
            // to track the OS setting, so this row is the one-liner ADR-0011's
            // mapping predicted -- no portal read, no registry read. Best-effort:
            // an older runtime without `ICoreWebView2_13` keeps its default
            // rather than failing the whole realisation.
            if let Ok(profile) = webview.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()) {
                let _ = profile.SetPreferredColorScheme(COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO);
            }
        }
        Ok(())
    }

    /// Attach every event this backend listens to, plus the filters the scheme
    /// hook needs.
    fn wire_events(&self, webview: &ICoreWebView2) -> Result<(), RendererError> {
        let mut token = 0i64;
        fn wiring(what: &str, error: windows::core::Error) -> RendererError {
            RendererError::Backend(format!("{what} failed: {error}"))
        }

        // --- custom-scheme interception -------------------------------------
        // ONE filter per registered scheme, deliberately NON-overlapping:
        // WebView2 raises the event once per MATCHING filter, and this handler
        // ANSWERS requests, so a double delivery would be a double answer.
        for scheme in self.schemes.routes.borrow().keys() {
            unsafe {
                webview
                    .AddWebResourceRequestedFilter(
                        &HSTRING::from(scheme_filter(scheme)),
                        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    )
                    .map_err(|e| wiring("AddWebResourceRequestedFilter", e))?;
            }
        }
        let schemes = Rc::clone(&self.schemes);
        unsafe {
            webview
                .add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        schemes.start(&args)
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_WebResourceRequested", e))?;
        }

        // --- the load lifecycle ---------------------------------------------
        let life = self.life.clone();
        unsafe {
            webview
                .add_NavigationStarting(
                    &NavigationStartingEventHandler::create(Box::new(move |_webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut uri = PWSTR::null();
                        args.Uri(&mut uri)?;
                        let uri = take_pwstr(uri);
                        let mut life = life.borrow_mut();
                        // `navigate` already optimistically began this load, so
                        // only a load for a DIFFERENT url starts a fresh lifecycle
                        // here -- the same no-duplicate-Started rule the other
                        // backends apply.
                        let already = life.state() == LoadState::Started
                            && life.current_url() == Some(uri.as_str());
                        if !already {
                            life.begin(&uri);
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_NavigationStarting", e))?;
        }

        let life = self.life.clone();
        unsafe {
            webview
                .add_ContentLoading(
                    &ContentLoadingEventHandler::create(Box::new(move |webview, args| {
                        if let Some(args) = args {
                            let mut is_error_page = BOOL::default();
                            args.IsErrorPage(&mut is_error_page)?;
                            if is_error_page.as_bool() {
                                return Ok(());
                            }
                        }
                        let mut life = life.borrow_mut();
                        // A load the scheme route already failed (fail-closed,
                        // nothing rendered) must not be walked back to Committed.
                        if life.state() == LoadState::Failed {
                            return Ok(());
                        }
                        life.commit(&source_of(webview.as_ref()));
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_ContentLoading", e))?;
        }

        let life = self.life.clone();
        let schemes = Rc::clone(&self.schemes);
        unsafe {
            webview
                .add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut success = BOOL::default();
                        args.IsSuccess(&mut success)?;
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut status)?;
                        let url = source_of(webview.as_ref());
                        let recorded = schemes.last_error.borrow_mut().take();
                        let mut life = life.borrow_mut();
                        if success.as_bool() {
                            if life.state() != LoadState::Failed {
                                life.finish(&url);
                            }
                            return Ok(());
                        }
                        // Already reported, with the honest verify reason, by the
                        // scheme route: do not report it twice.
                        if life.state() == LoadState::Failed {
                            return Ok(());
                        }
                        let Some(reason) = navigation_failure(status.0, recorded.as_deref()) else {
                            // A cancelled navigation (Stop, or a superseding
                            // load) is not a page failure.
                            return Ok(());
                        };
                        let url = if url.is_empty() {
                            life.current_url().unwrap_or_default().to_string()
                        } else {
                            url
                        };
                        life.fail(&url, &reason);
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_NavigationCompleted", e))?;
        }

        // --- SAME-DOCUMENT (SPA) URL tracking --------------------------------
        // NATIVE here, and this is the row WebView2 serves better than any other
        // edge: `IsNewDocument == FALSE` IS the same-document change, so a
        // SvelteKit `pushState` needs no inference (desktop infers it from
        // `notify::uri`, iOS from KVO on `url`, Android from
        // `doUpdateVisitedHistory`).
        let life = self.life.clone();
        unsafe {
            webview
                .add_SourceChanged(
                    &SourceChangedEventHandler::create(Box::new(move |webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut is_new_document = BOOL::default();
                        args.IsNewDocument(&mut is_new_document)?;
                        if is_new_document.as_bool() {
                            // An ordinary load; the lifecycle events above own it.
                            return Ok(());
                        }
                        // `url_changed` is a NO-OP when the URL already matches
                        // the lifecycle's current URL, so only a genuine
                        // same-document change surfaces `LoadEvent::UrlChanged`
                        // -- and it moves neither the load state nor the posture.
                        life.borrow_mut().url_changed(&source_of(webview.as_ref()));
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_SourceChanged", e))?;
        }

        // --- the new-window hook (`docs/adr/0010`) ---------------------------
        let life = self.life.clone();
        unsafe {
            webview
                .add_NewWindowRequested(
                    &NewWindowRequestedEventHandler::create(Box::new(move |webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut uri = PWSTR::null();
                        args.Uri(&mut uri)?;
                        let uri = take_pwstr(uri);
                        // HANDLED: werust has no tab/window model, so WebView2
                        // must never open a second window.
                        args.SetHandled(true)?;
                        let NewWindowAction::NavigateInPlace { url } =
                            new_window_action(Some(uri.as_str()))
                        else {
                            return Ok(());
                        };
                        life.borrow_mut().begin(&url);
                        // Fed back into the NORMAL load path, so an `ipfs://`
                        // `_blank` target still goes through the hash-verified
                        // scheme route and an unsupported scheme is still
                        // refused: the hook is a ROUTER, not a trust bypass.
                        if let Some(webview) = webview.as_ref() {
                            let _ = webview.Navigate(&HSTRING::from(&url));
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_NewWindowRequested", e))?;
        }

        // --- the script-message bridge ---------------------------------------
        let bridges = Rc::clone(&self.bridges);
        unsafe {
            webview
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut raw = PWSTR::null();
                        if args.TryGetWebMessageAsString(&mut raw).is_err() {
                            // Not a string: not one of our envelopes.
                            return Ok(());
                        }
                        let raw = take_pwstr(raw);
                        let Some(message) = parse_bridge_envelope(&raw) else {
                            // Addressed to no registered bridge: dropped, never
                            // mis-delivered.
                            return Ok(());
                        };
                        let handler = bridges.borrow().get(&message.handler).cloned();
                        if let Some(handler) = handler {
                            (handler.borrow_mut())(message);
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| wiring("add_WebMessageReceived", e))?;
        }

        Ok(())
    }

    /// Install the document-start scripts: the bridge ADAPTER first (so the
    /// shared shims find the channel shape they post to), then every queued
    /// injected script in order.
    fn apply_document_start_scripts(&mut self, webview: &ICoreWebView2) {
        let names: Vec<String> = self.bridges.borrow().keys().cloned().collect();
        if !names.is_empty() {
            add_document_start_script(webview, &bridge_adapter_script(&names));
        }
        for script in &self.pending_scripts {
            add_document_start_script(webview, script);
        }
    }

    /// The OS light/dark preference this machine reports, mapped through the
    /// shared [`OsColorScheme`] rule (`docs/adr/0009`: FOLLOW, never force).
    ///
    /// The ENGINE already follows the OS through
    /// `COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO` and needs nothing from this. It
    /// exists so the CHROME (the sibling window task) can paint from the SAME
    /// signal every other edge paints from, and so "Windows follows the OS" is a
    /// checkable fact rather than an assumption. The mapping rule itself is pure
    /// and unit-tested on the Ubuntu gate.
    #[must_use]
    pub fn os_color_scheme(&self) -> OsColorScheme {
        os_color_scheme()
    }

    /// Install the native EIP-1193 provider over the seam's script-message
    /// bridge -- werust's FIRST trust hook (`CONTEXT.md`, `docs/adr/0001`).
    ///
    /// The twin of the WebKitGTK, macOS and iOS `install_provider`, routed through
    /// the SAME `werust_core::provider` path: the page-side
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
        // `ScriptMessageHandler` is `Send` while a WebView2 is thread-bound, so
        // the handler captures a `Send` queue and the backend drains it on the
        // message-loop thread -- the same shape macOS and iOS use.
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
        // Make the provider detectable from document start, exactly as every
        // other edge does.
        self.inject_script(&provider_shim());
    }

    /// Install native `ipfs://` resolution over the seam's custom-scheme hook --
    /// werust's SECOND trust hook (`CONTEXT.md`, `docs/adr/0001`).
    ///
    /// An `ipfs://<cid>/...` URL is intercepted through a REGISTERED custom
    /// scheme (a real tuple origin, ADR-0011 Amendment 2), resolved through the
    /// SAME hash-verified `werust_core::ipfs::resolve_ipfs_request` path desktop,
    /// macOS and both mobile edges use (a CAR fetched from an UNTRUSTED trustless
    /// gateway, EVERY block verified against its own CID, the UnixFS DAG
    /// reassembled locally), and only then rendered. Verification GATES the load:
    /// a hash mismatch fails it rather than rendering unverified bytes.
    ///
    /// The blocking retrieval runs OFF the message-loop thread behind a WebView2
    /// deferral, and its completion is applied back on it (`docs/adr/0008`),
    /// through the SHARED [`webview_shared::offthread`] boundary.
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
    /// when the completion is applied on the message-loop thread.
    ///
    /// Exposed (rather than kept private to [`install_ipfs`](Webview2Renderer::install_ipfs))
    /// so the trust hook can be exercised against a PINNED, network-free
    /// content-addressed fixture in CI: the same verifying core path, the same
    /// off-thread boundary, no gateway.
    pub fn install_verifying_scheme(&mut self, scheme: &str, resolver: Arc<dyn OffThreadResolve>) {
        self.add_route(scheme, Route::OffThread(resolver));
    }

    /// Record a scheme route, refusing (loudly) to pretend a scheme registered
    /// after realisation will ever be intercepted.
    fn add_route(&mut self, scheme: &str, route: Route) {
        if self.webview.is_some() {
            // The SET of scheme names was fixed when the environment was created,
            // and WebView2 makes it immutable for the browser-process lifetime.
            // This cannot be REPORTED through the seam (`register_scheme_handler`
            // returns unit and the trait must not widen), so it is stated loudly
            // instead: the shell's contract is to install every scheme BEFORE the
            // first navigation, which the lazy environment exists to make always
            // possible. The same wording, and the same reasoning, as the macOS
            // backend's.
            eprintln!(
                "werust(windows): scheme `{scheme}` was registered AFTER the WebView2 environment \
                 was created and will NOT be intercepted; register every scheme before the first \
                 navigate"
            );
            return;
        }
        self.schemes
            .routes
            .borrow_mut()
            .insert(scheme.to_string(), Rc::new(route));
    }

    /// Apply every `ipfs://` completion whose off-thread verification has
    /// finished, on THIS (the message-loop) thread.
    ///
    /// [`poll_event`](Renderer::poll_event) calls this on every drain, so a shell
    /// that already pumps the seam needs no extra wiring; it is public so a driver
    /// with its own loop (the CI smoke) can pump explicitly. Returns how many
    /// requests were completed.
    pub fn pump_scheme_completions(&mut self) -> usize {
        self.schemes.drain_completions()
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

    /// Turn the Win32 message loop once, dispatching whatever is pending.
    ///
    /// WebView2 delivers EVERY event (navigation, web-resource, web-message)
    /// through the message loop of the thread that created the controller, so a
    /// driver that never pumps sees nothing happen at all. The real shell owns a
    /// proper loop; this exists so this crate can be RUN on its own (the CI
    /// smoke), the Win32 counterpart of turning the AppKit run loop.
    pub fn pump_messages(&mut self) {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    /// Show the eager container as a BARE top-level window, so WebView2 has a
    /// host to render into.
    ///
    /// This is NOT the product's window: the Win32 window, the URL bar, the trust
    /// indicator, the menus and the debug view are the sibling task
    /// `windows-win32-window-and-chrome`. It exists so this crate can be RUN --
    /// the CI smoke drives a real load through a real engine with it -- without
    /// pretending to ship a shell. It is shown WITHOUT activation, so it steals no
    /// focus; this is the same host shape `crates/windows-origin-probe` was
    /// MEASURED working with on a `windows-latest` runner.
    pub fn host_in_bare_window(&mut self) {
        unsafe {
            let _ = ShowWindow(self.container, SW_SHOWNOACTIVATE);
        }
        if let Some(controller) = &self.controller {
            unsafe {
                let mut rect = RECT::default();
                let _ = GetClientRect(self.container, &mut rect);
                let _ = controller.SetBounds(rect);
                let _ = controller.SetIsVisible(true);
            }
        }
    }
}

impl Drop for Webview2Renderer {
    fn drop(&mut self) {
        // Clear the borrowed-controller slot BEFORE closing it, so a `WM_SIZE`
        // arriving during teardown cannot reach a closed controller.
        unsafe {
            SetWindowLongPtrW(self.container, GWLP_USERDATA, 0);
        }
        if let Some(controller) = self.controller.take() {
            unsafe {
                let _ = controller.Close();
            }
        }
        unsafe {
            let _ = DestroyWindow(self.container);
        }
    }
}

impl Renderer for Webview2Renderer {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        // The ONE shared URL rule, from `webview-shared` -- the same rule the
        // WebKitGTK and WKWebView backends apply, never a Windows-local copy.
        validate_url(url)?;
        // Optimistically reflect the started load, exactly as the other backends
        // do, so the seam is well-defined before the message loop turns.
        self.life.borrow_mut().begin(url);
        // LAZY: the environment (and therefore the immutable set of custom scheme
        // names) is created HERE, on the first navigation, by which time every
        // scheme handler is registered. ADR-0011 finding 5's prescribed answer.
        self.realize()?;
        let Some(webview) = &self.webview else {
            return Err(RendererError::Backend("no WebView2 engine".into()));
        };
        unsafe { webview.Navigate(&HSTRING::from(url)) }
            .map_err(|e| RendererError::Backend(format!("Navigate({url}) failed: {e}")))
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        if self.life.borrow().current_url().is_none() {
            return Err(RendererError::Backend("nothing to reload".into()));
        }
        if let Some(webview) = &self.webview {
            unsafe { webview.Reload() }
                .map_err(|e| RendererError::Backend(format!("Reload failed: {e}")))?;
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(webview) = &self.webview {
            unsafe {
                let _ = webview.Stop();
            }
        }
        self.life.borrow_mut().stop();
    }

    fn go_back(&mut self) {
        // WebView2 owns the session (back/forward) list; a back navigation
        // restarts the load and the navigation events feed the shared lifecycle
        // exactly as a fresh `navigate` does. Guarded so a stray call at the start
        // of history is a no-op.
        if let Some(webview) = &self.webview {
            if can_go_back(webview) {
                unsafe {
                    let _ = webview.GoBack();
                }
            }
        }
    }

    fn go_forward(&mut self) {
        if let Some(webview) = &self.webview {
            if can_go_forward(webview) {
                unsafe {
                    let _ = webview.GoForward();
                }
            }
        }
    }

    fn can_go_back(&self) -> bool {
        self.webview.as_ref().is_some_and(can_go_back)
    }

    fn can_go_forward(&self) -> bool {
        self.webview.as_ref().is_some_and(can_go_forward)
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
        // on the message-loop thread.
        self.pump_scheme_completions();
        self.pump_pending_eval();
        self.life.borrow_mut().poll()
    }

    fn view_handle(&self) -> ViewHandle {
        // The EAGER container HWND, valid from construction even before the
        // environment is created -- which is the whole reason the split exists.
        // `ViewHandle` already carries an opaque platform pointer, and an `HWND`
        // IS one, so this row needs no trait change (ADR-0011 finding 5).
        ViewHandle(self.container.0.cast::<c_void>())
    }

    fn send_pointer(&mut self, _event: PointerEvent) {
        // The WebView2 controller owns a child HWND that receives real Win32
        // input directly; synthetic injection is not part of its public API. The
        // hook stays on the seam for backends that own their own input path (a
        // future native renderer) -- the same position the WebKitGTK and macOS
        // backends take.
    }

    fn send_key(&mut self, _event: KeyEvent) {
        // As with pointer input: real key events reach the focused WebView2
        // through the normal Win32 focus/message path.
    }

    fn send_scroll(&mut self, _delta: ScrollDelta) {
        // Scrolling is handled by the engine from real input.
    }

    fn set_focus(&mut self, focused: bool) {
        if !focused {
            return;
        }
        if let Some(controller) = &self.controller {
            unsafe {
                let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        }
    }

    fn register_script_message_handler(&mut self, name: &str, handler: ScriptMessageHandler) {
        // WebView2 has ONE page -> host channel
        // (`window.chrome.webview.postMessage`), where WebKit has NAMED handlers.
        // So the adapter script gives the page the
        // `window.webkit.messageHandlers.<name>` shape the SHARED core shims post
        // to, the envelope carries the name, and this map routes on it -- the
        // same answer the Android edge's Kotlin preamble gives, rather than a
        // Windows fork of the shared shim.
        self.bridges
            .borrow_mut()
            .insert(name.to_string(), Rc::new(RefCell::new(handler)));
        if let Some(webview) = &self.webview {
            // Registered after realisation: the adapter for this ONE name still
            // reaches every future document, so (unlike a scheme) a late script
            // bridge is not lost.
            add_document_start_script(webview, &bridge_adapter_script(&[name.to_string()]));
        }
    }

    fn inject_script(&mut self, script: &str) {
        // A document-start script in the top-level document, the same reach
        // WebKitGTK's `InjectionTime::Start` and WKWebView's
        // `AtDocumentStart` give, so the provider is detectable before the page's
        // first line runs. Queued until the engine exists, because
        // `AddScriptToExecuteOnDocumentCreated` is a method ON the engine and the
        // engine is created lazily.
        self.pending_scripts.push(script.to_string());
        if let Some(webview) = &self.webview {
            add_document_start_script(webview, script);
        }
    }

    fn evaluate_javascript(&self, script: &str) {
        // Push JS into the live page (browser -> page): the response half of the
        // script-message bridge that settles the EIP-1193 provider's pending
        // Promise. Fire-and-forget, matching the seam's `&self`, no-result
        // contract. A backend with no realised engine has no live document, so
        // there is nothing to evaluate into.
        if let Some(webview) = &self.webview {
            unsafe {
                let _ = webview.ExecuteScript(&HSTRING::from(script), None);
            }
        }
    }

    fn register_scheme_handler(&mut self, scheme: &str, handler: SchemeHandler) {
        // Intercept `<scheme>://...` requests and answer them from the handler,
        // SYNCHRONOUSLY on the message-loop thread -- the same contract the
        // WebKitGTK and macOS backends' `register_scheme_handler` has. The
        // VERIFYING `ipfs://` route (which must not block the UI thread and which
        // marks the trust posture) is `install_ipfs` / `install_verifying_scheme`
        // instead, exactly as the other backends split them.
        self.add_route(scheme, Route::Sync(RefCell::new(handler)));
    }

    fn trust_hooks(&self) -> TrustHooks {
        // OPT IN to BOTH trust hooks: this backend genuinely wires them --
        // EIP-1193 provider injection over the script-message bridge
        // (`install_provider`: a real `add_WebMessageReceived` channel + a real
        // `AddScriptToExecuteOnDocumentCreated` shim + the `ExecuteScript`
        // response push) and `ipfs://` custom-scheme resolution (`install_ipfs`: a
        // real REGISTERED scheme + `WebResourceRequested` -> the hash-verified
        // core path). The seam default is FAIL-CLOSED (`TrustHooks::none()`), so
        // trust is never inherited by omission.
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

/// The WebView2 Runtime version on this machine, or the honest, NAMED
/// runtime-missing error.
///
/// `GetAvailableCoreWebView2BrowserVersionString` is Microsoft's own recommended
/// presence check, and it is the ONLY place werust can tell "no runtime" apart
/// from "the runtime refused". Its failure never propagates as a panic.
fn runtime_version() -> Result<String, RendererError> {
    unsafe {
        let mut version = PWSTR::null();
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version)
            .map_err(|e| missing_runtime_error(&e.to_string()))?;
        Ok(take_pwstr(version))
    }
}

/// Where WebView2 keeps its profile by default for this crate.
///
/// Under the OS temp directory, per-user: an ENGINE-only crate has no business
/// minting a durable profile location, and CI wants a writable one that is not
/// next to the executable (the WebView2 default, which is often read-only). A
/// shell passes its own with
/// [`with_user_data_folder`](Webview2Renderer::with_user_data_folder).
#[must_use]
pub fn default_user_data_folder() -> PathBuf {
    std::env::temp_dir().join("werust-webview2")
}

/// The eager container window the controller is created into, and the handle
/// [`view_handle`](Renderer::view_handle) hands the shell.
///
/// A real (initially hidden) top-level window: WebView2 does not support
/// message-only windows. A shell embeds it by re-parenting rather than by
/// creating a second one, which is why it exists from construction.
fn create_container_window() -> Result<HWND, RendererError> {
    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // WebView2 has no autoresizing: a controller keeps the bounds it was
        // given until someone sets new ones. A SHELL that hosts this container in
        // a resizable window would therefore leave the page at the size it had
        // when the engine was realised -- the Win32 twin of the one engine line
        // the AppKit window needed (`WKWebView`'s autoresizing mask). Keeping it
        // HERE, in the container's own window proc, means the page follows its
        // container for every host, with no per-shell wiring and no seam change.
        if message == WM_SIZE {
            let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                // BORROWED: the renderer owns the reference and clears this slot
                // before closing the controller, so this must not release it.
                let controller = std::mem::ManuallyDrop::new(unsafe {
                    ICoreWebView2Controller::from_raw(raw as *mut c_void)
                });
                let mut rect = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut rect);
                    let _ = controller.SetBounds(rect);
                }
            }
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    unsafe {
        let instance = GetModuleHandleW(None)
            .map_err(|e| RendererError::Backend(format!("GetModuleHandleW: {e}")))?;
        let class_name = w!("werust_webview2_container");
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            ..Default::default()
        };
        // A second registration of the same class fails harmlessly; the class is
        // process-wide and this backend may be constructed more than once.
        RegisterClassW(&class);

        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("werust"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1024,
            768,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| RendererError::Backend(format!("CreateWindowExW: {e}")))
    }
}

/// Create the controller into `hwnd`, pumping the message loop until WebView2
/// answers.
fn create_controller(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
) -> Result<ICoreWebView2Controller, RendererError> {
    let (sender, receiver) = channel();
    unsafe {
        environment
            .CreateCoreWebView2Controller(
                hwnd,
                &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                    move |code, controller| {
                        let result = (|| {
                            code?;
                            controller.ok_or_else(|| windows::core::Error::from(E_POINTER))
                        })();
                        let _ = sender.send(result);
                        Ok(())
                    },
                )),
            )
            .map_err(|e| {
                RendererError::Backend(format!("CreateCoreWebView2Controller failed: {e}"))
            })?;
    }
    webview2_com::wait_with_pump(receiver)
        .map_err(|e| {
            RendererError::Backend(format!("waiting for the WebView2 controller failed: {e}"))
        })?
        .map_err(|e| RendererError::Backend(format!("the WebView2 controller was refused: {e}")))
}

/// Add one document-start script, ignoring the (asynchronous) registration id.
fn add_document_start_script(webview: &ICoreWebView2, script: &str) {
    unsafe {
        let _ = webview.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), None);
    }
}

/// The engine's current URL as a plain string (empty when it has none).
fn source_of(webview: Option<&ICoreWebView2>) -> String {
    let Some(webview) = webview else {
        return String::new();
    };
    unsafe {
        let mut source = PWSTR::null();
        if webview.Source(&mut source).is_err() {
            return String::new();
        }
        take_pwstr(source)
    }
}

fn can_go_back(webview: &ICoreWebView2) -> bool {
    unsafe {
        let mut value = BOOL::default();
        webview.CanGoBack(&mut value).is_ok() && value.as_bool()
    }
}

fn can_go_forward(webview: &ICoreWebView2) -> bool {
    unsafe {
        let mut value = BOOL::default();
        webview.CanGoForward(&mut value).is_ok() && value.as_bool()
    }
}

/// The OS light/dark preference this machine reports, mapped through the shared
/// [`OsColorScheme`] rule (`docs/adr/0009`: FOLLOW, never force).
///
/// A free function as well as a method because the CHROME must re-read it on
/// `WM_SETTINGCHANGE`, when it no longer holds the backend (it is behind the
/// `Renderer` seam by then). The registry read stays HERE, with the rest of the
/// platform bindings, rather than being copied into the window crate: ONE reader,
/// one mapping, for the engine and the chrome alike.
#[must_use]
pub fn os_color_scheme() -> OsColorScheme {
    os_color_scheme_from_apps_use_light_theme(apps_use_light_theme())
}

/// Read `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`.
///
/// [`None`] when the value is absent or unreadable, which the shared rule maps to
/// [`OsColorScheme::NoPreference`] rather than to a guess.
fn apps_use_light_theme() -> Option<u32> {
    let mut value: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).ok()?;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut value).cast::<c_void>()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(value)
}
