//! The WebView2 half: drive ONE case to a [`CaseFacts`] on a real Windows
//! runtime. `#[cfg(windows)]` only; the Ubuntu `verify` gate never sees it.
//!
//! Bindings are `webview2-com` + `webview2-com-sys` (ADR-0011 finding 4), the
//! same crates `wry` depends on.
//!
//! # One case per PROCESS
//!
//! WebView2 fixes the SET of custom scheme registrations at environment
//! creation and makes it immutable for the browser-process lifetime; every
//! environment sharing a browser process must register an IDENTICAL set or
//! creation fails. Case A registers `ipfs://` and case B registers nothing, so
//! running them in one process would make the second case's environment
//! creation depend on how WebView2 happened to reuse a browser process — a
//! confound in the middle of the experiment. Each case therefore runs in its
//! own child process with its own user-data folder, and `main` aggregates.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, CoreWebView2CustomSchemeRegistration, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DevToolsProtocolEventReceivedEventHandler, NavigationCompletedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
};
use windows::core::{w, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{E_POINTER, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, PeekMessageW,
    RegisterClassW, ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG, PM_REMOVE, SW_SHOWNOACTIVATE,
    WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

use crate::facts::{CaseFacts, CaseId};
use crate::page;

/// How long one case may take before the harness gives up and reports what it
/// has. Generous: a cold WebView2 first-run on a CI image creates a user-data
/// folder and starts a browser process from scratch.
const CASE_TIMEOUT: Duration = Duration::from_secs(90);

/// The WebView2 Runtime version this machine has. Recorded WITH the result,
/// because the runtime is evergreen, cannot be pinned, and this exact corner
/// regressed in stable 144 in January 2026 (WebView2Feedback #5495).
pub fn runtime_version() -> Result<String, String> {
    unsafe {
        let mut version = PWSTR::null();
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version)
            .map_err(|e| format!("no WebView2 Runtime is available: {e}"))?;
        Ok(take_pwstr(version))
    }
}

/// Everything the HOST observed while the case ran, as opposed to what the page
/// reported about itself.
#[derive(Default)]
struct Observed {
    /// Every URI `WebResourceRequested` was asked about.
    handler_uris: Vec<String>,
    /// Every Blink console/Log entry (the only signal Android's opaque origin
    /// left behind, so worth capturing here too).
    console: Vec<String>,
    navigation: Option<String>,
    /// The page's own outcome JSON, over `postMessage`.
    result_json: Option<String>,
}

/// Run one case to completion and report what it measured. Never panics: a
/// harness failure becomes [`CaseFacts::harness_error`] so the aggregate report
/// can distinguish "the mechanism does not work" from "the probe did not run".
pub fn run_case(case: CaseId, cid: &str) -> CaseFacts {
    let mut facts = CaseFacts {
        page_url: page::case_page_url(case, cid),
        ..CaseFacts::default()
    };
    if let Err(err) = drive(case, cid, &mut facts) {
        facts.harness_error = Some(err);
    }
    facts
}

fn drive(case: CaseId, cid: &str, facts: &mut CaseFacts) -> Result<(), String> {
    let observed = Rc::new(RefCell::new(Observed::default()));

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| format!("CoInitializeEx failed: {e}"))?;
    }

    let hwnd = create_host_window()?;
    let environment = create_environment(case, cid)?;
    let controller = create_controller(&environment, hwnd)?;
    let webview = unsafe { controller.CoreWebView2() }
        .map_err(|e| format!("ICoreWebView2Controller::CoreWebView2 failed: {e}"))?;

    // Keep the receivers alive for the whole run: dropping them detaches the
    // console capture.
    let _receivers = wire_events(case, cid, &webview, &environment, &observed)?;

    unsafe {
        let settings = webview
            .Settings()
            .map_err(|e| format!("ICoreWebView2::Settings failed: {e}"))?;
        let _ = settings.SetIsScriptEnabled(true);
        let _ = settings.SetIsWebMessageEnabled(true);
        let _ = settings.SetAreDevToolsEnabled(true);
        // A built-in error page would replace the probe page with Edge's own
        // document (a different origin) and silently destroy the measurement.
        let _ = settings.SetIsBuiltInErrorPageEnabled(false);

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let _ = controller.SetBounds(rect);
        let _ = controller.SetIsVisible(true);

        webview
            .Navigate(&HSTRING::from(&facts.page_url))
            .map_err(|e| format!("Navigate({}) failed: {e}", facts.page_url))?;
    }

    pump_until(CASE_TIMEOUT, || observed.borrow().result_json.is_some());
    // A trailing console entry (a CORS rejection, say) can land just after the
    // page reports, and it is often the most useful line in the whole run.
    pump_for(Duration::from_millis(750));

    let raw = observed.borrow().result_json.clone().or_else(|| {
        // Fallback channel: if the message bridge itself did not survive the
        // origin the page ended up with, the page also wrote its outcome into
        // document.title.
        let mut title = PWSTR::null();
        unsafe { webview.DocumentTitle(&mut title).ok()? };
        let title = take_pwstr(title);
        title.starts_with('{').then_some(title)
    });

    {
        let observed = observed.borrow();
        facts.navigation = observed
            .navigation
            .clone()
            .unwrap_or_else(|| "never completed".to_string());
        facts.handler_uris = observed.handler_uris.clone();
        facts.console = observed.console.clone();
        facts.fetch_handler_fired = observed
            .handler_uris
            .iter()
            .any(|uri| page::path_of(uri).starts_with(page::DATA_PATH));
        facts.css_font_handler_fired = observed
            .handler_uris
            .iter()
            .any(|uri| page::path_of(uri).split('?').next() == Some(page::FONT_PATH));
    }

    unsafe {
        let _ = controller.Close();
        let _ = DestroyWindow(hwnd);
    }

    let raw = raw.ok_or_else(|| {
        format!(
            "the probe page never reported an outcome within {}s (navigation: {})",
            CASE_TIMEOUT.as_secs(),
            facts.navigation
        )
    })?;
    apply_page_outcome(&raw, facts)
}

/// Fold the page's self-reported JSON into the facts. Missing fields stay empty
/// rather than defaulting to something that could read as a pass.
fn apply_page_outcome(raw: &str, facts: &mut CaseFacts) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("the probe page reported unparseable JSON ({e}): {raw}"))?;
    let string = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    facts.origin = string("origin");
    facts.secure_context = value
        .get("secureContext")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    facts.fetch = string("fetch");
    facts.push_state = string("pushState");
    facts.module_script = string("moduleScript");
    facts.service_worker = string("serviceWorker");
    Ok(())
}

/// A plain top-level window to host the controller. WebView2 does not support
/// message-only windows, so this is a real (if unfocused) window; the CI runner
/// has a desktop session.
fn create_host_window() -> Result<HWND, String> {
    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let class_name = w!("werust_origin_probe");
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            ..Default::default()
        };
        RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("werust origin probe"),
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
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(hwnd)
    }
}

/// THE experiment, in ten lines:
///
/// * **case A** registers `ipfs` as a real scheme with `HasAuthorityComponent`
///   + `TreatAsSecure`;
/// * **case B** registers nothing and serves the identical bytes from an
///   ordinary internal `https` origin;
/// * **the control** registers the same scheme with `HasAuthorityComponent =
///   false`, which Microsoft documents as producing an opaque origin. It is the
///   run that must FAIL, so that case A passing is evidence rather than a
///   tautology.
fn create_environment(case: CaseId, cid: &str) -> Result<ICoreWebView2Environment, String> {
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe {
        options.set_additional_browser_arguments(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection".to_string(),
        );
        if let Some(has_authority_component) = match case {
            CaseId::A => Some(true),
            CaseId::Control => Some(false),
            CaseId::B => None,
        } {
            let registration =
                CoreWebView2CustomSchemeRegistration::new(page::IPFS_SCHEME.to_string());
            registration.set_has_authority_component(has_authority_component);
            // `TreatAsSecure` is documented to be effective only alongside
            // `HasAuthorityComponent`, so the control turns both off together:
            // one flag's worth of difference from case A.
            registration.set_treat_as_secure(has_authority_component);
            registration.set_allowed_origins(vec![format!("{}://*", page::IPFS_SCHEME)]);
            options.set_scheme_registrations(vec![Some(registration.into())]);
        }
    }

    let (tx, rx) = std::sync::mpsc::channel();
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            &HSTRING::from(user_data_folder(case, cid).as_os_str()),
            &ICoreWebView2EnvironmentOptions::from(options),
            &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |code, environment| {
                    let result = (|| {
                        code?;
                        environment.ok_or_else(|| windows::core::Error::from(E_POINTER))
                    })();
                    let _ = tx.send(result);
                    Ok(())
                },
            )),
        )
        .map_err(|e| format!("CreateCoreWebView2EnvironmentWithOptions failed: {e}"))?;
    }
    webview2_com::wait_with_pump(rx)
        .map_err(|e| format!("waiting for the WebView2 environment failed: {e}"))?
        .map_err(|e| format!("the WebView2 environment was refused: {e}"))
}

/// A per-case user-data folder, so the two cases cannot share a browser process
/// and therefore cannot collide over their differing scheme registrations.
fn user_data_folder(case: CaseId, cid: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "werust-origin-probe-{}-{}",
        case.as_str().to_lowercase(),
        &cid[..8.min(cid.len())]
    ))
}

fn create_controller(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
) -> Result<ICoreWebView2Controller, String> {
    let (tx, rx) = std::sync::mpsc::channel();
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
                        let _ = tx.send(result);
                        Ok(())
                    },
                )),
            )
            .map_err(|e| format!("CreateCoreWebView2Controller failed: {e}"))?;
    }
    webview2_com::wait_with_pump(rx)
        .map_err(|e| format!("waiting for the WebView2 controller failed: {e}"))?
        .map_err(|e| format!("the WebView2 controller was refused: {e}"))
}

/// Attach the four observation points: the scheme handler (which also SERVES
/// the canned bytes), the page's message channel, navigation completion, and
/// Blink's console.
fn wire_events(
    case: CaseId,
    cid: &str,
    webview: &ICoreWebView2,
    environment: &ICoreWebView2Environment,
    observed: &Rc<RefCell<Observed>>,
) -> Result<Vec<ICoreWebView2DevToolsProtocolEventReceiver>, String> {
    let mut token = 0i64;

    unsafe {
        // ONE filter, covering exactly this case's origin. Overlapping filters
        // would raise WebResourceRequested more than once per request and
        // corrupt the very count this probe reports.
        let filter = HSTRING::from(page::case_filter(case, cid));
        webview
            .AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            .map_err(|e| format!("AddWebResourceRequestedFilter({filter}) failed: {e}"))?;
    }

    let handler_state = Rc::clone(observed);
    let environment_for_handler = environment.clone();
    unsafe {
        webview
            .add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(move |_webview, args| {
                    let Some(args) = args else { return Ok(()) };
                    let request = args.Request()?;
                    let mut uri = PWSTR::null();
                    request.Uri(&mut uri)?;
                    let uri = take_pwstr(uri);
                    handler_state.borrow_mut().handler_uris.push(uri.clone());

                    if let Some(resource) = page::resource_for(page::path_of(&uri)) {
                        let stream = SHCreateMemStream(Some(resource.body));
                        // Deliberately NO Access-Control-Allow-Origin: adding
                        // one would let an opaque origin's request through and
                        // hide exactly the failure this probe exists to detect.
                        let headers =
                            HSTRING::from(format!("Content-Type: {}", resource.content_type));
                        let response = environment_for_handler.CreateWebResourceResponse(
                            stream.as_ref(),
                            200,
                            w!("OK"),
                            &headers,
                        )?;
                        args.SetResponse(&response)?;
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| format!("add_WebResourceRequested failed: {e}"))?;
    }

    let message_state = Rc::clone(observed);
    unsafe {
        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
                    if let Some(args) = args {
                        let mut message = PWSTR::null();
                        args.TryGetWebMessageAsString(&mut message)?;
                        message_state.borrow_mut().result_json = Some(take_pwstr(message));
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| format!("add_WebMessageReceived failed: {e}"))?;
    }

    let navigation_state = Rc::clone(observed);
    unsafe {
        webview
            .add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |_webview, args| {
                    if let Some(args) = args {
                        let mut success = BOOL::default();
                        args.IsSuccess(&mut success)?;
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut status)?;
                        navigation_state.borrow_mut().navigation = Some(if success.as_bool() {
                            "completed:success".to_string()
                        } else {
                            format!("completed:failed:WebErrorStatus({})", status.0)
                        });
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| format!("add_NavigationCompleted failed: {e}"))?;
    }

    // Blink's own console. WebView2 has no console event, so this goes through
    // the DevTools protocol (ADR-0011's spike, section 5's "debug console
    // capture" row). Best-effort: a missing console never invalidates a run,
    // it only makes a failure harder to explain.
    let mut receivers = Vec::new();
    unsafe {
        for method in ["Log.enable", "Runtime.enable"] {
            let _ = webview.CallDevToolsProtocolMethod(&HSTRING::from(method), w!("{}"), None);
        }
        for event in ["Log.entryAdded", "Runtime.consoleAPICalled"] {
            let Ok(receiver) = webview.GetDevToolsProtocolEventReceiver(&HSTRING::from(event))
            else {
                continue;
            };
            let console_state = Rc::clone(observed);
            let label = event.to_string();
            let _ = receiver.add_DevToolsProtocolEventReceived(
                &DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                    move |_webview, args| {
                        if let Some(args) = args {
                            let mut json = PWSTR::null();
                            args.ParameterObjectAsJson(&mut json)?;
                            console_state
                                .borrow_mut()
                                .console
                                .push(format!("{label}: {}", take_pwstr(json)));
                        }
                        Ok(())
                    },
                )),
                &mut token,
            );
            receivers.push(receiver);
        }
    }

    Ok(receivers)
}

/// Pump the message loop until `done` or the deadline.
fn pump_until(timeout: Duration, done: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return;
        }
        pump_once();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pump_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        pump_once();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pump_once() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
