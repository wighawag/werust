//! The werust **iOS core**: the Rust core cross-compiled as a C-ABI static
//! library the iOS app links and drives from its Swift OS edge.
//!
//! This crate is the iOS realisation of "the browsing logic stays in the Rust
//! core behind the seams" (`CONTEXT.md`, the `mobile-ios-shell-and-static-lib`
//! task) — the twin of the Android core crate, swapping JNI for a plain C-ABI.
//! It builds a `staticlib` (`libwerust_mobile.a`) that the Xcode app links with
//! `-force_load` and drives over a small `extern "C"` surface (declared for Swift
//! in the bridging header `Sources/werust_mobile.h`). The core itself is the
//! SHARED [`werust_core`] crate the desktop GTK shell and the Android app use;
//! the only iOS-specific pieces are:
//!
//! * [`IosBackend`] — the [`Renderer`](renderer::Renderer) seam backend over the
//!   platform `WKWebView` (the iOS system webview), driven from Swift across the
//!   C-ABI. It owns the browsing LOGIC (session history, load lifecycle, chrome),
//!   so Swift stays confined to the OS edge.
//! * [`CoreSession`] — a thin, ObjC-free wrapper binding a
//!   [`BrowserShell`](werust_core::BrowserShell) over an [`IosBackend`] with the
//!   WebView-signal callbacks, so the whole session is testable with ordinary
//!   `cargo test` (no UIKit / no simulator), and the C-ABI layer is a mechanical
//!   marshalling shim.
//! * The `werust_ios_*` C-ABI exports — the mechanical bridge Swift calls.
//!
//! # The Swift ↔ core protocol
//!
//! One [`CoreSession`] per `UIViewController`. On a user action (typed URL, Back,
//! Forward, Reload, Stop) Swift drives the session, then reads back:
//!
//! * [`take_pending_load`](CoreSession::take_pending_load) — the URL (if any) to
//!   apply to the platform `WKWebView` via `WKWebView.load`.
//! * [`chrome_json`](CoreSession::chrome_json) — the [`ChromeState`] as JSON to
//!   paint the URL bar, the Back/Forward/Reload/Stop enablement, and the status
//!   line. Swift holds NO browsing logic; every one of those is the core's truth.
//!
//! And Swift reports the platform `WKWebView`'s real load signals back in
//! ([`on_page_committed`](CoreSession::on_page_committed) /
//! [`on_page_finished`](CoreSession::on_page_finished) /
//! [`on_page_failed`](CoreSession::on_page_failed)), which the core folds into
//! the chrome exactly as the desktop pump folds WebKitGTK's signals.

mod backend;
mod ffi_json;

pub use backend::{IosBackend, IosHandle};

use werust_core::{BrowserShell, ChromeState};

/// A single browsing session for one iOS `UIViewController`: a
/// [`BrowserShell`](werust_core::BrowserShell) over an [`IosBackend`], plus the
/// WebView-signal callbacks Swift reports into.
///
/// This is the ObjC-free heart the C-ABI layer wraps: every method here is plain
/// Rust so the Swift↔core protocol is exercised by `cargo test` without UIKit,
/// and the `werust_ios_*` exports below are a mechanical marshalling shim over it.
pub struct CoreSession {
    shell: BrowserShell,
    /// A handle to the shell's [`IosBackend`]'s shared state, for the
    /// platform-`WKWebView` protocol (pending-load + load signals) that the
    /// cross-backend seam does not carry.
    backend: IosHandle,
}

impl Default for CoreSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreSession {
    /// Build a fresh session over an [`IosBackend`].
    #[must_use]
    pub fn new() -> Self {
        let backend = IosBackend::new();
        let handle = backend.handle();
        Self {
            shell: BrowserShell::new(Box::new(backend)),
            backend: handle,
        }
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam. Returns
    /// `true` on success; an unusable URL is rejected and leaves the chrome
    /// untouched (Swift keeps the bad text for the user to fix).
    pub fn navigate(&mut self, url: &str) -> bool {
        self.shell.navigate(url).is_ok()
    }

    /// Go one step back in session history, through the seam.
    pub fn go_back(&mut self) {
        self.shell.go_back();
    }

    /// Go one step forward in session history, through the seam.
    pub fn go_forward(&mut self) {
        self.shell.go_forward();
    }

    /// Reload the current page, through the seam. Returns `true` on success.
    pub fn reload(&mut self) -> bool {
        self.shell.reload().is_ok()
    }

    /// Stop the in-flight load, through the seam.
    pub fn stop(&mut self) {
        self.shell.stop();
    }

    /// The URL (if any) the core has committed to but the platform `WKWebView`
    /// has not yet loaded. Swift drains this after driving the session and calls
    /// `WKWebView.load` with it.
    pub fn take_pending_load(&mut self) -> Option<String> {
        self.backend.take_pending_load()
    }

    /// Report the platform `WKWebView`'s commit signal into the core, then fold
    /// the resulting lifecycle events into the chrome.
    pub fn on_page_committed(&mut self, url: &str) {
        self.backend.on_page_committed(url);
        self.shell.pump();
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    pub fn on_page_finished(&mut self, url: &str) {
        self.backend.on_page_finished(url);
        self.shell.pump();
    }

    /// Report the platform `WKWebView`'s error signal into the core.
    pub fn on_page_failed(&mut self, url: &str, reason: &str) {
        self.backend.on_page_failed(url, reason);
        self.shell.pump();
    }

    /// The current [`ChromeState`] the Swift edge paints its URL bar, nav-control
    /// enablement, and status line from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        self.shell.chrome()
    }

    /// The current [`ChromeState`] as a JSON object, the wire form Swift reads
    /// across the C-ABI (a single string return is the simplest robust FFI
    /// marshalling).
    #[must_use]
    pub fn chrome_json(&self) -> String {
        ffi_json::chrome_to_json(self.shell.chrome())
    }
}

// ---------------------------------------------------------------------------
// C-ABI export surface. These are the mechanical `werust_ios_*` bridges the
// Swift shell calls (declared in `Sources/werust_mobile.h`, imported as the
// project's bridging header). They only marshal to/from C and delegate to
// `CoreSession`; all logic lives above so it is UIKit-free-testable.
//
// The session is handed to Swift as an opaque `*mut CoreSession` from
// `werust_ios_session_new`, threaded back through every call, and freed by
// `werust_ios_session_free`. Strings cross the boundary as NUL-terminated
// `char *`: inputs are borrowed C strings (Swift owns them); outputs are
// heap-allocated C strings the caller MUST return via `werust_ios_string_free`.
// ---------------------------------------------------------------------------
mod ffi {
    use super::CoreSession;
    use std::ffi::{c_char, CStr, CString};

    /// Reconstruct a `&mut CoreSession` from the opaque handle Swift threads back.
    ///
    /// # Safety
    /// `session` must be a pointer returned by `werust_ios_session_new` and not
    /// yet freed by `werust_ios_session_free`; Swift guarantees this by
    /// construction (one handle per `UIViewController`, threaded through every
    /// call on the main thread).
    unsafe fn session_mut<'a>(session: *mut CoreSession) -> Option<&'a mut CoreSession> {
        if session.is_null() {
            None
        } else {
            Some(&mut *session)
        }
    }

    /// Read a borrowed C string into an owned `String` (empty on null/invalid).
    ///
    /// # Safety
    /// `s`, if non-null, must point to a valid NUL-terminated C string the caller
    /// keeps alive for the duration of the call.
    unsafe fn read(s: *const c_char) -> String {
        if s.is_null() {
            String::new()
        } else {
            CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    /// Move a Rust `String` into a heap C string the caller frees via
    /// [`werust_ios_string_free`]. Returns null only on an interior-NUL string
    /// (never produced by our JSON/URL encoders).
    fn into_c_string(s: String) -> *mut c_char {
        match CString::new(s) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }

    /// Create a fresh browsing session; the returned pointer is threaded back
    /// through every call and freed with [`werust_ios_session_free`].
    #[no_mangle]
    pub extern "C" fn werust_ios_session_new() -> *mut CoreSession {
        Box::into_raw(Box::new(CoreSession::new()))
    }

    /// Free a session created by [`werust_ios_session_new`].
    ///
    /// # Safety
    /// `session` must be a live handle from `werust_ios_session_new`, freed at
    /// most once. A null pointer is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_session_free(session: *mut CoreSession) {
        if !session.is_null() {
            drop(Box::from_raw(session));
        }
    }

    /// Free a C string returned by any `werust_ios_*` function that returns
    /// `char *` (the chrome JSON and the pending-load URL).
    ///
    /// # Safety
    /// `s` must be a pointer returned by one of those functions, freed at most
    /// once. A null pointer is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_string_free(s: *mut c_char) {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }

    /// Navigate to `url` (the URL bar's Enter action). Returns `true` on success.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_navigate(
        session: *mut CoreSession,
        url: *const c_char,
    ) -> bool {
        let url = read(url);
        session_mut(session)
            .map(|s| s.navigate(&url))
            .unwrap_or(false)
    }

    /// Go one step back in the core's session history.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_go_back(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.go_back();
        }
    }

    /// Go one step forward in the core's session history.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_go_forward(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.go_forward();
        }
    }

    /// Reload the current page. Returns `true` on success.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_reload(session: *mut CoreSession) -> bool {
        session_mut(session).map(|s| s.reload()).unwrap_or(false)
    }

    /// Stop the in-flight load.
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_stop(session: *mut CoreSession) {
        if let Some(s) = session_mut(session) {
            s.stop();
        }
    }

    /// The URL the core has committed to but the `WKWebView` has not yet loaded,
    /// as a heap C string, or null if nothing is pending. Free with
    /// [`werust_ios_string_free`].
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_take_pending_load(
        session: *mut CoreSession,
    ) -> *mut c_char {
        match session_mut(session).and_then(|s| s.take_pending_load()) {
            Some(url) => into_c_string(url),
            None => std::ptr::null_mut(),
        }
    }

    /// Report the platform `WKWebView`'s commit signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_committed(
        session: *mut CoreSession,
        url: *const c_char,
    ) {
        let url = read(url);
        if let Some(s) = session_mut(session) {
            s.on_page_committed(&url);
        }
    }

    /// Report the platform `WKWebView`'s finished signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` is a valid NUL-terminated C string.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_finished(
        session: *mut CoreSession,
        url: *const c_char,
    ) {
        let url = read(url);
        if let Some(s) = session_mut(session) {
            s.on_page_finished(&url);
        }
    }

    /// Report the platform `WKWebView`'s error signal into the core.
    ///
    /// # Safety
    /// `session` is a live handle; `url` and `reason` are valid NUL-terminated
    /// C strings.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_on_page_failed(
        session: *mut CoreSession,
        url: *const c_char,
        reason: *const c_char,
    ) {
        let url = read(url);
        let reason = read(reason);
        if let Some(s) = session_mut(session) {
            s.on_page_failed(&url, &reason);
        }
    }

    /// The current chrome as a heap C string (JSON), for Swift to paint the URL
    /// bar, nav-control enablement, and status line. Free with
    /// [`werust_ios_string_free`].
    ///
    /// # Safety
    /// `session` is a live handle from `werust_ios_session_new`.
    #[no_mangle]
    pub unsafe extern "C" fn werust_ios_chrome_json(session: *mut CoreSession) -> *mut c_char {
        match session_mut(session) {
            Some(s) => into_c_string(s.chrome_json()),
            None => std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::LoadState;

    /// Drive the in-flight load to done the way the Swift edge would from the
    /// platform `WKWebView`'s `didCommit` + `didFinish` signals.
    fn settle(session: &mut CoreSession) {
        let url = session
            .take_pending_load()
            .expect("a pending load to apply to the WKWebView");
        session.on_page_committed(&url);
        session.on_page_finished(&url);
    }

    #[test]
    fn a_typed_url_navigates_through_the_core_and_moves_the_chrome() {
        // The end-to-end Swift↔core protocol for the URL bar's Enter action: the
        // core navigates through the seam, surfaces the URL for the WKWebView, and
        // the chrome reflects the in-flight then settled load.
        let mut s = CoreSession::new();
        assert_eq!(s.chrome().url_text, "");
        assert_eq!(s.chrome().load_state, LoadState::Idle);

        assert!(s.navigate("https://example.com/"), "valid https url");
        assert_eq!(s.chrome().url_text, "https://example.com/");
        assert!(s.chrome().is_loading());
        assert_eq!(
            s.take_pending_load().as_deref(),
            Some("https://example.com/"),
            "the URL is surfaced for WKWebView.load"
        );
        assert_eq!(s.take_pending_load(), None, "drained once");

        // The WKWebView's real signals (reported by Swift) settle the load.
        s.on_page_committed("https://example.com/");
        s.on_page_finished("https://example.com/");
        assert_eq!(s.chrome().load_state, LoadState::Finished);
        assert!(!s.chrome().is_loading());
    }

    #[test]
    fn an_unusable_url_is_rejected_and_leaves_the_chrome_untouched() {
        let mut s = CoreSession::new();
        assert!(!s.navigate("not-a-url"), "unusable url rejected");
        assert_eq!(s.chrome().load_state, LoadState::Idle);
        assert_eq!(s.chrome().url_text, "");
        assert_eq!(s.take_pending_load(), None);
    }

    #[test]
    fn back_and_forward_reflect_navigation_state_through_the_core() {
        // Back/Forward availability is the CORE's truth, read by Swift from the
        // chrome — Swift keeps no history of its own.
        let mut s = CoreSession::new();
        assert!(!s.chrome().can_go_back);
        assert!(!s.chrome().can_go_forward);

        assert!(s.navigate("https://a.example/"));
        settle(&mut s);
        assert!(!s.chrome().can_go_back, "one entry: nowhere back");

        assert!(s.navigate("https://b.example/"));
        settle(&mut s);
        assert!(s.chrome().can_go_back);
        assert!(!s.chrome().can_go_forward);

        s.go_back();
        settle(&mut s);
        assert_eq!(s.chrome().url_text, "https://a.example/");
        assert!(!s.chrome().can_go_back);
        assert!(s.chrome().can_go_forward);

        s.go_forward();
        settle(&mut s);
        assert_eq!(s.chrome().url_text, "https://b.example/");
        assert!(!s.chrome().can_go_forward, "back at the tip of history");
    }

    #[test]
    fn reload_re_navigates_and_stop_settles_the_load() {
        let mut s = CoreSession::new();
        assert!(!s.reload(), "nothing to reload yet");

        assert!(s.navigate("https://example.com/"));
        settle(&mut s);
        assert!(s.reload(), "reload the settled page");
        assert!(s.chrome().is_loading());

        s.stop();
        assert_eq!(s.chrome().load_state, LoadState::Idle);
    }

    #[test]
    fn a_failed_load_surfaces_the_failure_in_the_chrome() {
        let mut s = CoreSession::new();
        assert!(s.navigate("https://does-not-resolve.invalid/"));
        let _ = s.take_pending_load();
        assert_eq!(s.chrome().last_error, None);

        s.on_page_failed("https://does-not-resolve.invalid/", "name not resolved");
        assert_eq!(s.chrome().load_state, LoadState::Failed);
        assert_eq!(s.chrome().last_error.as_deref(), Some("name not resolved"));

        // A new navigation clears the surfaced failure.
        assert!(s.navigate("https://example.com/"));
        assert_eq!(s.chrome().last_error, None);
    }

    #[test]
    fn chrome_json_carries_every_field_the_swift_edge_paints() {
        // The JSON is the wire form Swift reads across the C-ABI; it must carry the
        // URL bar text, nav-control enablement, load state, and any failure so the
        // Swift edge can paint the whole chrome without any logic of its own.
        let mut s = CoreSession::new();
        assert!(s.navigate("https://a.example/"));
        settle(&mut s);
        assert!(s.navigate("https://b.example/"));
        settle(&mut s);

        let json = s.chrome_json();
        assert!(json.contains("\"url\":\"https://b.example/\""), "{json}");
        assert!(json.contains("\"canGoBack\":true"), "{json}");
        assert!(json.contains("\"canGoForward\":false"), "{json}");
        assert!(json.contains("\"loading\":false"), "{json}");
        assert!(json.contains("\"loadState\":\"finished\""), "{json}");
    }

    /// Drive the whole Swift↔core protocol across the raw C-ABI exports exactly
    /// as the Swift shell does, proving the mechanical marshalling shim threads
    /// the opaque handle, borrows input C strings, and hands back heap C strings
    /// the caller frees. This is the one test that exercises the FFI layer itself
    /// (the exports are `cfg`-free so the host gate covers them too).
    #[test]
    fn the_c_abi_exports_drive_a_full_navigation_and_free_their_strings() {
        use super::ffi::*;
        use std::ffi::{CStr, CString};

        unsafe {
            let s = werust_ios_session_new();
            assert!(!s.is_null());

            let url = CString::new("https://example.com/").unwrap();
            assert!(werust_ios_navigate(s, url.as_ptr()), "valid https url");

            let pending = werust_ios_take_pending_load(s);
            assert!(!pending.is_null(), "a pending load is surfaced");
            assert_eq!(
                CStr::from_ptr(pending).to_str().unwrap(),
                "https://example.com/"
            );
            werust_ios_string_free(pending);
            assert!(
                werust_ios_take_pending_load(s).is_null(),
                "drained once: null now"
            );

            werust_ios_on_page_committed(s, url.as_ptr());
            werust_ios_on_page_finished(s, url.as_ptr());

            let json_ptr = werust_ios_chrome_json(s);
            assert!(!json_ptr.is_null());
            let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_owned();
            werust_ios_string_free(json_ptr);
            assert!(json.contains("\"url\":\"https://example.com/\""), "{json}");
            assert!(json.contains("\"loadState\":\"finished\""), "{json}");
            assert!(json.contains("\"loading\":false"), "{json}");

            werust_ios_session_free(s);
            // Null handles are tolerated (no crash, sane defaults).
            assert!(!werust_ios_navigate(std::ptr::null_mut(), url.as_ptr()));
            assert!(werust_ios_chrome_json(std::ptr::null_mut()).is_null());
            werust_ios_session_free(std::ptr::null_mut());
            werust_ios_string_free(std::ptr::null_mut());
        }
    }
}
