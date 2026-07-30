//! The macOS half: drive a REAL `WKWebView` and produce the measured
//! [`CaseFacts`]. `#[cfg(target_os = "macos")]`, so the Ubuntu `verify` gate
//! never sees it (and never needs an SDK) while the decision rule, the canned
//! site and the CLI stay inside the gate.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, NSData, NSDate, NSDictionary, NSHTTPURLResponse, NSPoint, NSProcessInfo, NSRect,
    NSRunLoop, NSSize, NSString, NSURLRequest, NSURLResponse, NSURL,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationDelegate, WKScriptMessage, WKScriptMessageHandler,
    WKURLSchemeHandler, WKURLSchemeTask, WKUserContentController, WKWebView,
    WKWebViewConfiguration,
};

use crate::facts::{CaseFacts, CaseId};
use crate::page;

/// Everything the probe host records while a case runs.
struct ProbeIvars {
    /// Every URI the registered scheme handler was ASKED about, in order.
    handler_uris: RefCell<Vec<String>>,
    /// The page's own JSON report, once it arrives.
    report: RefCell<Option<String>>,
    /// What the navigation delegate saw.
    navigation: RefCell<String>,
}

define_class!(
    // SAFETY:
    // - `NSObject` has no subclassing requirements.
    // - `ProbeHost` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacOriginProbeHost"]
    #[ivars = ProbeIvars]
    struct ProbeHost;

    unsafe impl NSObjectProtocol for ProbeHost {}

    /// The mechanism UNDER TEST: `ipfs://` served by a registered
    /// `WKURLSchemeHandler`. It answers from the canned site and RECORDS every
    /// URI it was asked about, which is how "did the fetch reach the handler"
    /// becomes a measurement rather than an inference.
    unsafe impl WKURLSchemeHandler for ProbeHost {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start_task(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let request = unsafe { task.request() };
            let uri = request
                .URL()
                .and_then(|url| url.absoluteString())
                .map(|s| s.to_string())
                .unwrap_or_default();
            self.ivars().handler_uris.borrow_mut().push(uri.clone());

            let Some(url) = request.URL() else {
                return;
            };
            let path = page::path_of(&uri);
            let (body, content_type, status) = match page::resource_for(path) {
                Some(resource) => (resource.body, resource.content_type, 200_isize),
                // Answer an undefined path with an honest 404 rather than HTML,
                // so an unexpected request cannot look like a success.
                None => (b"not found" as &[u8], "text/plain; charset=utf-8", 404),
            };
            let mime = NSString::from_str(content_type);
            let headers = NSDictionary::from_slices(&[ns_string!("Content-Type")], &[&*mime]);
            let Some(response) = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
                NSHTTPURLResponse::alloc(),
                &url,
                status,
                Some(ns_string!("HTTP/1.1")),
                Some(&headers),
            ) else {
                return;
            };
            let data = NSData::with_bytes(body);
            unsafe {
                let response: &NSURLResponse = &response;
                task.didReceiveResponse(response);
                task.didReceiveData(&data);
                task.didFinish();
            }
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop_task(&self, _web_view: &WKWebView, _task: &ProtocolObject<dyn WKURLSchemeTask>) {
            // Every task above is answered synchronously, so there is never one
            // in flight to stop.
        }
    }

    /// The page reports its outcome here.
    unsafe impl WKScriptMessageHandler for ProbeHost {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            let body = unsafe { message.body() };
            let body: Retained<NSString> = unsafe { msg_send![&*body, description] };
            *self.ivars().report.borrow_mut() = Some(body.to_string());
        }
    }

    /// Diagnostic only: whether the top-level navigation completed at all. It
    /// never decides the verdict; it is there so a failing case says WHY.
    unsafe impl WKNavigationDelegate for ProbeHost {
        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            *self.ivars().navigation.borrow_mut() = "completed:success".to_string();
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &objc2_foundation::NSError,
        ) {
            *self.ivars().navigation.borrow_mut() =
                format!("failed:{}", error.localizedDescription());
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &objc2_foundation::NSError,
        ) {
            *self.ivars().navigation.borrow_mut() =
                format!("failed-provisional:{}", error.localizedDescription());
        }
    }
);

impl ProbeHost {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ProbeIvars {
            handler_uris: RefCell::new(Vec::new()),
            report: RefCell::new(None),
            navigation: RefCell::new("no-navigation-callback".to_string()),
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// The OS build this run measured on. A result without it is not a result.
#[must_use]
pub fn os_version() -> String {
    NSProcessInfo::processInfo()
        .operatingSystemVersionString()
        .to_string()
}

/// MEASURED, not read from the documentation: WebKit handles `https` itself and
/// refuses to give it to a `WKURLSchemeHandler`. This is the reason there is no
/// "case B" (the Android/Windows internal-`https` origin) on WebKit.
#[must_use]
pub fn https_is_handled_natively(mtm: MainThreadMarker) -> bool {
    unsafe { WKWebView::handlesURLScheme(&NSString::from_str(page::NATIVELY_HANDLED_SCHEME), mtm) }
}

/// Run ONE case on a fresh `WKWebView` and return what it measured.
pub fn run_case(case: CaseId, cid: &str, mtm: MainThreadMarker) -> CaseFacts {
    let mut facts = CaseFacts {
        page_url: page::case_page_url(case, cid),
        ..CaseFacts::default()
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let host = ProbeHost::new(mtm);
    let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
    let content = unsafe { WKUserContentController::new(mtm) };
    unsafe {
        // The SAME registered handler is installed for BOTH cases, so the
        // control's "the handler never fired" is a measured difference rather
        // than an absence.
        let scheme_handler: &ProtocolObject<dyn WKURLSchemeHandler> =
            ProtocolObject::from_ref(&*host);
        configuration.setURLSchemeHandler_forURLScheme(
            Some(scheme_handler),
            &NSString::from_str(page::IPFS_SCHEME),
        );
        let script_handler: &ProtocolObject<dyn WKScriptMessageHandler> =
            ProtocolObject::from_ref(&*host);
        content
            .addScriptMessageHandler_name(script_handler, &NSString::from_str(page::REPORT_BRIDGE));
        configuration.setUserContentController(&content);
    }

    let frame = NSRect::new(
        NSPoint::new(-20_000.0, -20_000.0),
        NSSize::new(1024.0, 768.0),
    );
    let web_view = unsafe {
        WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, &configuration)
    };
    unsafe {
        let delegate: &ProtocolObject<dyn WKNavigationDelegate> = ProtocolObject::from_ref(&*host);
        web_view.setNavigationDelegate(Some(delegate));
    }
    // A real (but off-screen, borderless, un-raised) window, because WebKit needs
    // a host to render into.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    window.setContentView(Some(&web_view));
    window.orderBack(None);

    match case {
        CaseId::A => {
            let url = NSURL::URLWithString(&NSString::from_str(&page::case_page_url(case, cid)));
            let Some(url) = url else {
                facts.harness_error = Some("could not build the case URL".to_string());
                return facts;
            };
            let request = NSURLRequest::requestWithURL(&url);
            unsafe { web_view.loadRequest(&request) };
        }
        CaseId::Control => {
            // The IDENTICAL bytes, with a NIL base URL: WebKit gives the document
            // an OPAQUE origin. The registered handler is still installed.
            unsafe {
                web_view.loadHTMLString_baseURL(&NSString::from_str(page::PAGE_HTML), None);
            }
        }
    }

    // Turn the run loop until the page reports (or we give up).
    let mut reported = None;
    for _ in 0..(30 * 50) {
        let until = NSDate::dateWithTimeIntervalSinceNow(0.02);
        NSRunLoop::currentRunLoop().runUntilDate(&until);
        if let Some(report) = host.ivars().report.borrow().clone() {
            reported = Some(report);
            break;
        }
    }
    // The fallback channel: if the message bridge was itself a casualty of the
    // origin, the page also wrote its JSON into the document title.
    if reported.is_none() {
        if let Some(title) = unsafe { web_view.title() } {
            let title = title.to_string();
            if title.starts_with('{') {
                reported = Some(title);
            }
        }
    }

    facts.navigation = host.ivars().navigation.borrow().clone();
    facts.handler_uris = host.ivars().handler_uris.borrow().clone();
    // "Did the fetch reach the handler" is answered by what the handler was
    // ASKED for, never by what the page believes.
    facts.fetch_handler_fired = facts
        .handler_uris
        .iter()
        .any(|uri| page::path_of(uri).starts_with(page::DATA_PATH));
    facts.css_font_handler_fired = facts
        .handler_uris
        .iter()
        .any(|uri| page::path_of(uri).starts_with(page::FONT_PATH));

    let Some(reported) = reported else {
        facts.harness_error = Some(format!(
            "the page never reported (navigation: {}, handler saw {} uris)",
            facts.navigation,
            facts.handler_uris.len()
        ));
        return facts;
    };
    apply_report(&mut facts, &reported);
    facts
}

/// Fold the page's own JSON report into the measured facts.
///
/// Split out and PURE-ish on purpose: the JSON shape is the contract between the
/// canned page and the host, and a change on either side should be visible in one
/// place.
fn apply_report(facts: &mut CaseFacts, reported: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(reported) {
        Ok(parsed) => parsed,
        Err(error) => {
            facts.harness_error = Some(format!("the page's report was unparseable: {error}"));
            return;
        }
    };
    let string_at = |key: &str| {
        parsed
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    facts.origin = string_at("origin");
    facts.secure_context = parsed
        .get("secureContext")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    facts.user_agent = string_at("userAgent");
    facts.fetch = string_at("fetch");
    facts.push_state = string_at("pushState");
    facts.module_script = string_at("moduleScript");
    facts.service_worker = string_at("serviceWorker");
}
