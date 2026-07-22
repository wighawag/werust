//! The werust **Android core**: the Rust core cross-compiled as a JNI shared
//! library the Android app links and drives from its Kotlin OS edge.
//!
//! This crate is the Android realisation of "the browsing logic stays in the Rust
//! core behind the seams" (`CONTEXT.md`, the `mobile-android-shell-and-static-lib`
//! task). It builds a `cdylib` (`libwerust_mobile.so`) that the Kotlin `Activity`
//! loads with `System.loadLibrary("werust_mobile")` and drives over a small JNI
//! surface. The core itself is the SHARED [`werust_core`] crate the desktop GTK
//! shell uses; the only Android-specific pieces are:
//!
//! * [`AndroidBackend`] — the [`Renderer`](renderer::Renderer) seam backend over
//!   the platform `android.webkit.WebView` (the Android system webview), driven
//!   from Kotlin across JNI. It owns the browsing LOGIC (session history, load
//!   lifecycle, chrome), so Kotlin stays confined to the OS edge.
//! * [`CoreSession`] — a thin, JVM-free wrapper binding a
//!   [`BrowserShell`](werust_core::BrowserShell) over an [`AndroidBackend`] with
//!   the WebView-signal callbacks, so the whole session is testable with ordinary
//!   `cargo test` (no JVM), and the JNI layer is a mechanical marshalling shim.
//! * The `Java_..._nativeXxx` JNI exports — the mechanical bridge Kotlin calls.
//!
//! # The Kotlin ↔ core protocol
//!
//! One [`CoreSession`] per `Activity`. On a user action (typed URL, Back, Forward,
//! Reload, Stop) Kotlin drives the session, then reads back:
//!
//! * [`take_pending_load`](CoreSession::take_pending_load) — the URL (if any) to
//!   apply to the platform `WebView` via `WebView.loadUrl`.
//! * [`chrome_json`](CoreSession::chrome_json) — the [`ChromeState`] as JSON to
//!   paint the URL bar, the Back/Forward/Reload/Stop enablement, and the status
//!   line. Kotlin holds NO browsing logic; every one of those is the core's truth.
//!
//! And Kotlin reports the platform `WebView`'s real load signals back in
//! ([`on_page_committed`](CoreSession::on_page_committed) /
//! [`on_page_finished`](CoreSession::on_page_finished) /
//! [`on_page_failed`](CoreSession::on_page_failed)), which the core folds into the
//! chrome exactly as the desktop pump folds WebKitGTK's signals.

mod backend;
mod ffi_json;

pub use backend::{AndroidBackend, AndroidHandle};

use werust_core::{BrowserShell, ChromeState};

/// A single browsing session for one Android `Activity`: a
/// [`BrowserShell`](werust_core::BrowserShell) over an [`AndroidBackend`], plus the
/// WebView-signal callbacks Kotlin reports into.
///
/// This is the JVM-free heart the JNI layer wraps: every method here is plain Rust
/// so the Kotlin↔core protocol is exercised by `cargo test` without a JVM, and the
/// `Java_...` exports below are a mechanical marshalling shim over it.
pub struct CoreSession {
    shell: BrowserShell,
    /// A handle to the shell's [`AndroidBackend`]'s shared state, for the
    /// platform-`WebView` protocol (pending-load + load signals) that the
    /// cross-backend seam does not carry.
    backend: AndroidHandle,
}

impl Default for CoreSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreSession {
    /// Build a fresh session over an [`AndroidBackend`].
    #[must_use]
    pub fn new() -> Self {
        let backend = AndroidBackend::new();
        let handle = backend.handle();
        Self {
            shell: BrowserShell::new(Box::new(backend)),
            backend: handle,
        }
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam. Returns
    /// `true` on success; an unusable URL is rejected and leaves the chrome
    /// untouched (Kotlin keeps the bad text for the user to fix).
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

    /// The URL (if any) the core has committed to but the platform `WebView` has
    /// not yet loaded. Kotlin drains this after driving the session and calls
    /// `WebView.loadUrl` with it.
    pub fn take_pending_load(&mut self) -> Option<String> {
        self.backend.take_pending_load()
    }

    /// Report the platform `WebView`'s commit signal into the core, then fold the
    /// resulting lifecycle events into the chrome.
    pub fn on_page_committed(&mut self, url: &str) {
        self.backend.on_page_committed(url);
        self.shell.pump();
    }

    /// Report the platform `WebView`'s finished signal into the core.
    pub fn on_page_finished(&mut self, url: &str) {
        self.backend.on_page_finished(url);
        self.shell.pump();
    }

    /// Report the platform `WebView`'s error signal into the core.
    pub fn on_page_failed(&mut self, url: &str, reason: &str) {
        self.backend.on_page_failed(url, reason);
        self.shell.pump();
    }

    /// The current [`ChromeState`] the Kotlin edge paints its URL bar, nav-control
    /// enablement, and status line from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        self.shell.chrome()
    }

    /// The current [`ChromeState`] as a JSON object, the wire form Kotlin reads
    /// across JNI (a single string return is the simplest robust JNI marshalling).
    #[must_use]
    pub fn chrome_json(&self) -> String {
        ffi_json::chrome_to_json(self.shell.chrome())
    }
}

// ---------------------------------------------------------------------------
// JNI export surface. These are the mechanical `Java_<pkg>_<Class>_<method>`
// bridges the Kotlin `WerustCore` class (package com.github.wighawag.werust)
// declares as `external`. They only marshal to/from the JVM and delegate to
// `CoreSession`; all logic lives above so it is JVM-free-testable.
//
// The session is handed to Kotlin as an opaque `jlong` pointer from
// `nativeNew`, threaded back through every call, and freed by `nativeFree`.
// ---------------------------------------------------------------------------
#[cfg(target_os = "android")]
mod jni_exports {
    use super::CoreSession;
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, jlong, jstring, JNI_FALSE, JNI_TRUE};
    use jni::JNIEnv;

    /// Reconstruct a `&mut CoreSession` from the opaque handle Kotlin threads back.
    ///
    /// # Safety
    /// `handle` must be a pointer returned by `nativeNew` and not yet freed by
    /// `nativeFree`; Kotlin guarantees this by construction (one handle per
    /// `Activity`, threaded through every call on the UI thread).
    unsafe fn session<'a>(handle: jlong) -> &'a mut CoreSession {
        &mut *(handle as *mut CoreSession)
    }

    fn read(env: &mut JNIEnv, s: &JString) -> String {
        env.get_string(s).map(|js| js.into()).unwrap_or_default()
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeNew(
        _env: JNIEnv,
        _class: JClass,
    ) -> jlong {
        Box::into_raw(Box::new(CoreSession::new())) as jlong
    }

    /// # Safety
    /// `handle` must be a live handle from `nativeNew`, freed at most once.
    #[no_mangle]
    pub unsafe extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeFree(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        if handle != 0 {
            drop(Box::from_raw(handle as *mut CoreSession));
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeNavigate(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) -> jboolean {
        let url = read(&mut env, &url);
        let ok = unsafe { session(handle) }.navigate(&url);
        if ok {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeGoBack(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.go_back();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeGoForward(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.go_forward();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeReload(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jboolean {
        if unsafe { session(handle) }.reload() {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeStop(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        unsafe { session(handle) }.stop();
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeTakePendingLoad(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let pending = unsafe { session(handle) }.take_pending_load();
        // An empty string means "no pending load" on the Kotlin side.
        let s = pending.unwrap_or_default();
        env.new_string(s)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageCommitted(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) {
        let url = read(&mut env, &url);
        unsafe { session(handle) }.on_page_committed(&url);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageFinished(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
    ) {
        let url = read(&mut env, &url);
        unsafe { session(handle) }.on_page_finished(&url);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeOnPageFailed(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        url: JString,
        reason: JString,
    ) {
        let url = read(&mut env, &url);
        let reason = read(&mut env, &reason);
        unsafe { session(handle) }.on_page_failed(&url, &reason);
    }

    #[no_mangle]
    pub extern "system" fn Java_com_github_wighawag_werust_WerustCore_nativeChromeJson(
        env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jstring {
        let json = unsafe { session(handle) }.chrome_json();
        env.new_string(json)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::LoadState;

    /// Drive the in-flight load to done the way the Kotlin edge would from the
    /// platform `WebView`'s commit + finished signals.
    fn settle(session: &mut CoreSession) {
        let url = session
            .take_pending_load()
            .expect("a pending load to apply to the WebView");
        session.on_page_committed(&url);
        session.on_page_finished(&url);
    }

    #[test]
    fn a_typed_url_navigates_through_the_core_and_moves_the_chrome() {
        // The end-to-end Kotlin↔core protocol for the URL bar's Enter action: the
        // core navigates through the seam, surfaces the URL for the WebView, and
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
            "the URL is surfaced for WebView.loadUrl"
        );
        assert_eq!(s.take_pending_load(), None, "drained once");

        // The WebView's real signals (reported by Kotlin) settle the load.
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
        // Back/Forward availability is the CORE's truth, read by Kotlin from the
        // chrome — Kotlin keeps no history of its own.
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
    fn chrome_json_carries_every_field_the_kotlin_edge_paints() {
        // The JSON is the wire form Kotlin reads across JNI; it must carry the URL
        // bar text, nav-control enablement, load state, and any failure so the
        // Kotlin edge can paint the whole chrome without any logic of its own.
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
}
