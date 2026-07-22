//! The werust **core**: the browser product shell, driven entirely through the
//! [`Renderer`] seam.
//!
//! This crate is "the Rust core" `CONTEXT.md` names: the browsing *logic* — the
//! URL bar, the back/forward/reload/stop controls, and the chrome that reflects
//! load state — behind the seams, WITHOUT any OS toolkit. Every OS edge is a thin
//! view over this SAME core: the desktop GTK window (`werust` binary) renders
//! [`ChromeState`] into GTK widgets and forwards actions to [`BrowserShell`]; the
//! Android Kotlin `Activity` and the iOS Swift shell drive the exact same shell
//! over an FFI surface. Keeping the logic toolkit-free is what lets the shell↔seam
//! wiring be tested at the seam boundary (a `dyn Renderer`), not against any GUI
//! internals — exactly the boundary `CONTEXT.md` and the mobile tasks call for.
//!
//! All page navigation goes THROUGH the seam: [`navigate`](BrowserShell::navigate),
//! [`go_back`](BrowserShell::go_back), [`go_forward`](BrowserShell::go_forward),
//! [`reload`](BrowserShell::reload), and [`stop`](BrowserShell::stop) call the
//! matching [`Renderer`] methods, and [`pump`](BrowserShell::pump) drains the
//! seam's [`LoadEvent`]s to refresh the chrome. The shell never reaches past the
//! seam into the webview: page *interaction* (scroll/click/focus/type) is served
//! by embedding the live [`ViewHandle`](renderer::ViewHandle) widget and giving
//! it focus (the webview's `send_*` methods are deliberate no-ops — see the
//! forward-pointer in the task), so this module wires navigation + chrome and
//! leaves raw input to the embedded widget.

use renderer::{LoadEvent, LoadState, Renderer, RendererError, TrustPosture};

pub mod ethereum;
pub mod ipfs;
pub mod provider;

/// The chrome state the shell reflects: everything the window must draw ABOUT the
/// current page, distinct from the page content itself.
///
/// This is the observable output of driving the seam: after any action plus a
/// [`pump`](BrowserShell::pump), the window paints its URL bar, its
/// back/forward/reload/stop controls, and its load indicator from this struct.
/// It is a plain value so a test can assert the chrome without a display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChromeState {
    /// The text shown in the URL bar. Tracks the committed/in-flight URL as the
    /// load lifecycle progresses (so the bar follows redirects and history
    /// navigations), not just what the user last typed.
    pub url_text: String,
    /// The current load-lifecycle state, driving the loading/idle indicator
    /// (e.g. a spinner while [`LoadState::is_loading`], stop enabled).
    pub load_state: LoadState,
    /// Whether the Back control is enabled (a back navigation is possible).
    pub can_go_back: bool,
    /// Whether the Forward control is enabled.
    pub can_go_forward: bool,
    /// A human-readable failure surfaced to the user when the last load failed,
    /// cleared when a new load starts. `None` when nothing has failed.
    pub last_error: Option<String>,
    /// The [`TrustPosture`] of the current page, driving the chrome's trust
    /// indicator: content-verified vs served by an unverified origin
    /// (`docs/adr/0001`: the trust posture is a product surface). Read straight
    /// from the seam's [`Renderer::trust_posture`], so it tracks the ACTUAL load
    /// path (a page whose bytes came back through the hash-verified
    /// content-addressed path), not the URL string.
    pub trust_posture: TrustPosture,
}

impl ChromeState {
    /// Whether the Stop control should be active (a load is in flight) versus the
    /// Reload control (a settled page). The window swaps/enables the two from this.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.load_state.is_loading()
    }

    /// Whether the current page was content-verified (its bytes hash-checked on
    /// the content-addressed path), as opposed to merely served by an unverified
    /// origin. The window paints its trust indicator from this.
    #[must_use]
    pub fn is_content_verified(&self) -> bool {
        self.trust_posture.is_content_verified()
    }
}

/// The browser shell: the seam-driven logic behind the window.
///
/// Holds the rendering backend as a `dyn Renderer` (the seam) and the derived
/// [`ChromeState`]. Every user action is a method that drives the seam; the
/// window calls [`pump`](BrowserShell::pump) on the main loop to fold the seam's
/// [`LoadEvent`]s into the chrome. It is generic-free (`Box<dyn Renderer>`) so
/// the SAME shell drives the webview today and a native backend later.
pub struct BrowserShell {
    renderer: Box<dyn Renderer>,
    chrome: ChromeState,
}

impl BrowserShell {
    /// Build a shell over the given rendering backend.
    ///
    /// The initial chrome reflects the backend's starting state (Idle, empty URL
    /// bar, back/forward derived from the backend's session history), so a caller
    /// can paint the window before any navigation.
    #[must_use]
    pub fn new(renderer: Box<dyn Renderer>) -> Self {
        let mut shell = Self {
            renderer,
            chrome: ChromeState::default(),
        };
        shell.refresh_chrome();
        shell
    }

    /// The current chrome state to paint the window from.
    #[must_use]
    pub fn chrome(&self) -> &ChromeState {
        &self.chrome
    }

    /// Navigate to `url` (the URL bar's Enter action), through the seam.
    ///
    /// On success the URL bar immediately reflects the target and any prior
    /// failure is cleared; an unusable URL is rejected by the backend with
    /// [`RendererError::InvalidUrl`] and leaves the chrome untouched (the bad text
    /// stays for the user to fix). The load lifecycle then advances via
    /// [`pump`](BrowserShell::pump).
    pub fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        self.renderer.navigate(url)?;
        self.chrome.last_error = None;
        self.refresh_chrome();
        Ok(())
    }

    /// Go one step back in session history, through the seam.
    ///
    /// A no-op when [`ChromeState::can_go_back`] is `false`. Delegates to the
    /// backend's session history (the shell keeps no URL stack of its own — see
    /// [`Renderer::go_back`]).
    pub fn go_back(&mut self) {
        self.renderer.go_back();
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Go one step forward in session history, through the seam.
    pub fn go_forward(&mut self) {
        self.renderer.go_forward();
        self.chrome.last_error = None;
        self.refresh_chrome();
    }

    /// Reload the current page, through the seam.
    pub fn reload(&mut self) -> Result<(), RendererError> {
        self.renderer.reload()?;
        self.chrome.last_error = None;
        self.refresh_chrome();
        Ok(())
    }

    /// Stop the in-flight load, through the seam.
    pub fn stop(&mut self) {
        self.renderer.stop();
        self.refresh_chrome();
    }

    /// Give (`true`) or take (`false`) keyboard focus of the live page view.
    ///
    /// This is how the shell makes the embedded page INTERACTIVE: with the live
    /// view focused, the OS/GTK routes scroll/click/focus/keyboard input to it
    /// natively (the webview's `send_*` forwarders are no-ops — the task's
    /// forward-pointer). The shell calls this through the seam rather than
    /// touching the webview.
    pub fn focus_page(&mut self, focused: bool) {
        self.renderer.set_focus(focused);
    }

    /// Drain every pending [`LoadEvent`] off the seam and fold it into the chrome.
    ///
    /// The window calls this on its main loop (a periodic pump). Each event moves
    /// the URL bar / load indicator: a `Started` clears any error and shows the
    /// target, `Committed`/`Finished` settle the URL bar on the effective URL,
    /// and a `Failed` surfaces the reason. Returns `true` if any event was
    /// processed, so a caller can repaint only on change.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.renderer.poll_event() {
            changed = true;
            match event {
                LoadEvent::Started { url } => {
                    self.chrome.url_text = url;
                    self.chrome.last_error = None;
                }
                LoadEvent::Committed { url } | LoadEvent::Finished { url } => {
                    self.chrome.url_text = url;
                }
                LoadEvent::Failed { url, reason } => {
                    self.chrome.url_text = url;
                    self.chrome.last_error = Some(reason);
                }
            }
        }
        // The lifecycle state and history availability are read straight from the
        // seam (they are the backend's truth), so refresh them whether or not an
        // event fired — a failed/settled load and can_go_* can change without a
        // queued event.
        self.refresh_chrome();
        changed
    }

    /// The opaque live-view handle for the window to embed.
    #[must_use]
    pub fn view_handle(&self) -> renderer::ViewHandle {
        self.renderer.view_handle()
    }

    /// Re-read the seam's authoritative state (load state, history availability,
    /// and current URL) into the chrome. Load state and back/forward availability
    /// are the backend's truth and always pulled fresh; the URL bar tracks the
    /// backend's `current_url` whenever it has one (the effective URL after
    /// redirects/history moves), so an action that changes the current entry
    /// without a queued event (e.g. a synchronous back on a backend with history)
    /// still moves the bar.
    fn refresh_chrome(&mut self) {
        self.chrome.load_state = self.renderer.load_state();
        self.chrome.can_go_back = self.renderer.can_go_back();
        self.chrome.can_go_forward = self.renderer.can_go_forward();
        // The trust posture is the backend's truth about the current load path
        // (content-verified vs served), pulled fresh like the load state so the
        // indicator tracks the page actually shown — including after a scheme
        // handler verifies the bytes mid-load, which flips the posture without a
        // queued LoadEvent.
        self.chrome.trust_posture = self.renderer.trust_posture();
        if let Some(url) = self.renderer.current_url() {
            self.chrome.url_text = url;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{
        KeyEvent, PointerEvent, SchemeHandler, ScriptMessageHandler, ScrollDelta, ViewHandle,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// The mutable innards of the [`FakeBackend`], modelling a REAL session
    /// history the way a webview does (a back/forward list) plus the native load
    /// signals a running GTK loop would deliver. It lives behind `Rc<RefCell>` so
    /// a test can hold a `handle` to drive the simulated native signals while the
    /// shell owns the `dyn Renderer` — exactly the interior-mutability shape the
    /// real `WebViewRenderer` uses to share its `LoadLifecycle` with the webview's
    /// signal closures, so the test drives the seam, never reaches past it.
    #[derive(Default)]
    struct BackendInner {
        /// The back/forward list; `cursor` is the index of the current entry.
        history: Vec<String>,
        cursor: Option<usize>,
        state: LoadState,
        events: VecDeque<LoadEvent>,
        /// Records that the shell forwarded focus/input through the seam, so the
        /// test can assert the shell drives interaction via the seam (not by
        /// reaching past it). On the webview these are no-ops; here we only prove
        /// the CALL crosses the seam.
        focus_calls: Vec<bool>,
        pointer_calls: u32,
        key_calls: u32,
        scroll_calls: u32,
        /// The trust posture of the current load, mirroring the real backend's
        /// shared `LoadLifecycle`: reset to the untrusted origin on every fresh
        /// navigation and flipped to content-verified only when the simulated
        /// verified content-addressed path served this load's bytes.
        posture: TrustPosture,
    }

    impl BackendInner {
        fn current(&self) -> Option<&String> {
            self.cursor.and_then(|c| self.history.get(c))
        }
    }

    /// A seam-level fake backend over a shared [`BackendInner`]. It renders
    /// nothing; it exists ONLY to exercise the shell↔seam wiring (navigation
    /// state transitions, chrome, history availability, focus/input forwarding)
    /// at the trait boundary without a GTK main loop or a display.
    #[derive(Default, Clone)]
    struct FakeBackend {
        inner: Rc<RefCell<BackendInner>>,
    }

    impl FakeBackend {
        /// A handle a test keeps to drive the backend's simulated native signals.
        fn handle(&self) -> BackendHandle {
            BackendHandle {
                inner: self.inner.clone(),
            }
        }
    }

    /// A test-side handle to the same [`BackendInner`] the shell drives, used to
    /// simulate the backend's native load signals (the stand-in for a running GTK
    /// loop turning the webview's `load-changed`/`load-failed` signals).
    struct BackendHandle {
        inner: Rc<RefCell<BackendInner>>,
    }

    impl BackendHandle {
        /// Carry the in-flight load to done (commit then finish), as a real
        /// webview's load signals would.
        fn drive_to_finished(&self) {
            let mut b = self.inner.borrow_mut();
            let url = b.current().expect("a load in flight").clone();
            b.state = LoadState::Committed;
            b.events
                .push_back(LoadEvent::Committed { url: url.clone() });
            b.state = LoadState::Finished;
            b.events.push_back(LoadEvent::Finished { url });
        }

        /// Report a failed load.
        fn drive_to_failed(&self, reason: &str) {
            let mut b = self.inner.borrow_mut();
            let url = b.current().expect("a load in flight").clone();
            b.state = LoadState::Failed;
            b.events.push_back(LoadEvent::Failed {
                url,
                reason: reason.to_string(),
            });
        }

        fn focus_calls(&self) -> Vec<bool> {
            self.inner.borrow().focus_calls.clone()
        }

        /// Simulate the `ipfs://` scheme handler serving the current load's main
        /// resource through the hash-verified content-addressed path: it marks the
        /// current load content-verified exactly as the real backend does when
        /// `resolve_ipfs_request` returns verified bytes. Only a load that actually
        /// went through this path flips the posture — a plain served load never
        /// calls it.
        fn serve_via_verified_content_path(&self) {
            self.inner.borrow_mut().posture = TrustPosture::ContentVerified;
        }
    }

    impl Renderer for FakeBackend {
        fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
            // Accept any `scheme://rest` URL (the day-one http(s) path plus the
            // ipfs:// trust-hook scheme), mirroring the real backend's
            // `validate_url`; a scheme-less string is still rejected.
            match url.split_once("://") {
                Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => {}
                _ => return Err(RendererError::InvalidUrl(url.to_string())),
            }
            let mut b = self.inner.borrow_mut();
            // A fresh navigation from mid-history drops the forward entries.
            let next = b.cursor.map_or(0, |c| c + 1);
            b.history.truncate(next);
            b.history.push(url.to_string());
            b.cursor = Some(b.history.len() - 1);
            b.state = LoadState::Started;
            // A fresh load starts UNVERIFIED and is only marked verified if this
            // load's bytes actually go through the verified content path — exactly
            // the real `LoadLifecycle::begin` reset that keeps the posture tracking
            // the CURRENT page's load path, never a stale value.
            b.posture = TrustPosture::UnverifiedOrigin;
            b.events.push_back(LoadEvent::Started {
                url: url.to_string(),
            });
            Ok(())
        }

        fn reload(&mut self) -> Result<(), RendererError> {
            let mut b = self.inner.borrow_mut();
            let url = b
                .current()
                .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?
                .clone();
            b.state = LoadState::Started;
            b.posture = TrustPosture::UnverifiedOrigin;
            b.events.push_back(LoadEvent::Started { url });
            Ok(())
        }

        fn stop(&mut self) {
            let mut b = self.inner.borrow_mut();
            if b.state.is_loading() {
                b.state = LoadState::Idle;
            }
        }

        fn go_back(&mut self) {
            let mut b = self.inner.borrow_mut();
            if let Some(c) = b.cursor {
                if c > 0 {
                    b.cursor = Some(c - 1);
                    let url = b.history[c - 1].clone();
                    b.state = LoadState::Started;
                    b.posture = TrustPosture::UnverifiedOrigin;
                    b.events.push_back(LoadEvent::Started { url });
                }
            }
        }

        fn go_forward(&mut self) {
            let mut b = self.inner.borrow_mut();
            if let Some(c) = b.cursor {
                if c + 1 < b.history.len() {
                    b.cursor = Some(c + 1);
                    let url = b.history[c + 1].clone();
                    b.state = LoadState::Started;
                    b.posture = TrustPosture::UnverifiedOrigin;
                    b.events.push_back(LoadEvent::Started { url });
                }
            }
        }

        fn can_go_back(&self) -> bool {
            matches!(self.inner.borrow().cursor, Some(c) if c > 0)
        }

        fn can_go_forward(&self) -> bool {
            let b = self.inner.borrow();
            matches!(b.cursor, Some(c) if c + 1 < b.history.len())
        }

        fn load_state(&self) -> LoadState {
            self.inner.borrow().state
        }

        fn trust_posture(&self) -> TrustPosture {
            self.inner.borrow().posture
        }

        fn current_url(&self) -> Option<String> {
            self.inner.borrow().current().cloned()
        }

        fn poll_event(&mut self) -> Option<LoadEvent> {
            self.inner.borrow_mut().events.pop_front()
        }

        fn view_handle(&self) -> ViewHandle {
            ViewHandle(std::ptr::null_mut())
        }

        fn send_pointer(&mut self, _event: PointerEvent) {
            self.inner.borrow_mut().pointer_calls += 1;
        }
        fn send_key(&mut self, _event: KeyEvent) {
            self.inner.borrow_mut().key_calls += 1;
        }
        fn send_scroll(&mut self, _delta: ScrollDelta) {
            self.inner.borrow_mut().scroll_calls += 1;
        }
        fn set_focus(&mut self, focused: bool) {
            self.inner.borrow_mut().focus_calls.push(focused);
        }

        fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {
        }
        fn inject_script(&mut self, _script: &str) {}
        fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {}
    }

    /// Build a shell over a fresh fake backend, returning both the shell and a
    /// handle to drive the backend's simulated native load signals. `settle`
    /// drives the in-flight load to done and pumps the shell — the test stand-in
    /// for a GTK loop turning the webview's load signals.
    fn shell_with_backend() -> (BrowserShell, BackendHandle) {
        let backend = FakeBackend::default();
        let handle = backend.handle();
        (BrowserShell::new(Box::new(backend)), handle)
    }

    fn settle(shell: &mut BrowserShell, handle: &BackendHandle) {
        handle.drive_to_finished();
        shell.pump();
    }

    #[test]
    fn typed_url_navigates_through_the_seam_and_updates_the_chrome() {
        // Acceptance: a window with a URL bar navigates to a typed URL through the
        // seam, and the chrome reflects the in-flight load.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert_eq!(shell.chrome().url_text, "");

        shell
            .navigate("https://example.com/")
            .expect("valid https url");
        assert_eq!(shell.chrome().url_text, "https://example.com/");
        assert!(shell.chrome().is_loading(), "load is in flight after Enter");

        // Draining the seam's lifecycle events settles the chrome on Finished.
        handle.drive_to_finished();
        assert!(shell.pump());
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert!(!shell.chrome().is_loading());
        assert_eq!(shell.chrome().url_text, "https://example.com/");
    }

    #[test]
    fn navigate_rejects_an_unusable_url_and_leaves_the_chrome_untouched() {
        let (mut shell, _handle) = shell_with_backend();
        let err = shell
            .navigate("not-a-url")
            .expect_err("unusable url rejected");
        assert_eq!(err, RendererError::InvalidUrl("not-a-url".into()));
        // A rejected navigation does not start a load or move the chrome.
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert_eq!(shell.chrome().url_text, "");
    }

    #[test]
    fn back_and_forward_work_and_reflect_navigation_state() {
        // Acceptance: back/forward work and reflect navigation state (the Back /
        // Forward controls enable/disable as history allows), all through the seam.
        let (mut shell, handle) = shell_with_backend();
        assert!(!shell.chrome().can_go_back, "no history at the start");
        assert!(!shell.chrome().can_go_forward);

        shell.navigate("https://a.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(!shell.chrome().can_go_back, "one entry: nowhere back");

        shell.navigate("https://b.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_back, "two entries: can go back");
        assert!(!shell.chrome().can_go_forward);

        shell.go_back();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://a.example/");
        assert!(!shell.chrome().can_go_back, "back at the first entry");
        assert!(shell.chrome().can_go_forward, "a forward entry now exists");

        shell.go_forward();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().url_text, "https://b.example/");
        assert!(shell.chrome().can_go_back);
        assert!(!shell.chrome().can_go_forward, "back at the tip of history");
    }

    #[test]
    fn a_fresh_navigation_from_mid_history_drops_the_forward_entries() {
        // Navigating after a Back truncates forward history, so Forward greys out
        // again — the navigation-state contract the chrome must reflect.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://a.example/").unwrap();
        settle(&mut shell, &handle);
        shell.navigate("https://b.example/").unwrap();
        settle(&mut shell, &handle);
        shell.go_back();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_forward);

        shell.navigate("https://c.example/").unwrap();
        settle(&mut shell, &handle);
        assert!(shell.chrome().can_go_back);
        assert!(
            !shell.chrome().can_go_forward,
            "a new navigation dropped the forward entry"
        );
        assert_eq!(shell.chrome().url_text, "https://c.example/");
    }

    #[test]
    fn reload_re_navigates_and_stop_settles_the_load() {
        let (mut shell, handle) = shell_with_backend();
        assert!(shell.reload().is_err(), "nothing to reload yet");

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);

        shell.reload().expect("reload the settled page");
        assert!(shell.chrome().is_loading(), "reload restarts the load");

        // Stop mid-load returns the chrome to a settled (idle) state.
        shell.stop();
        assert_eq!(shell.chrome().load_state, LoadState::Idle);
        assert!(!shell.chrome().is_loading());
    }

    #[test]
    fn a_failed_load_surfaces_the_failure_in_the_chrome() {
        // Acceptance: load-lifecycle failure is surfaced through the seam into the
        // chrome (the shell shows the reason), and clears on the next navigation.
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("https://does-not-resolve.invalid/").unwrap();
        assert!(shell.pump()); // drain the Started event
        assert_eq!(shell.chrome().last_error, None);

        handle.drive_to_failed("name not resolved");
        assert!(shell.pump());
        assert_eq!(shell.chrome().load_state, LoadState::Failed);
        assert_eq!(
            shell.chrome().last_error.as_deref(),
            Some("name not resolved")
        );

        // A new navigation clears the surfaced failure.
        shell.navigate("https://example.com/").unwrap();
        assert_eq!(shell.chrome().last_error, None);
    }

    #[test]
    fn the_chrome_shows_the_unverified_posture_for_a_plain_served_load() {
        // Acceptance: an ordinary served-origin load surfaces the UNVERIFIED trust
        // posture in the chrome. It is read straight from the seam (the actual
        // load path), and a plain load never went through the verified
        // content-addressed path, so it is content-verified == false.
        let (mut shell, handle) = shell_with_backend();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "nothing loaded yet: the untrusted default"
        );

        shell
            .navigate("https://example.com/")
            .expect("valid https url");
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "a plain served page is not content-verified"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_chrome_shows_the_content_verified_posture_when_served_via_the_verified_path() {
        // Acceptance: a page whose bytes came back through the hash-verified
        // content-addressed path surfaces the CONTENT-VERIFIED posture in the
        // chrome — and it tracks the ACTUAL load path, not the URL: the posture
        // only flips after the verified content path serves this load's main
        // resource (mirroring the real `ipfs://` scheme handler marking the
        // lifecycle on a verified resolution).
        let (mut shell, handle) = shell_with_backend();
        shell
            .navigate("ipfs://bafyfixturecid/index.html")
            .expect("an ipfs url is navigable through the seam");
        // Before the verified content path serves the bytes, the load is untrusted
        // — the URL looking like `ipfs://` is NOT enough to claim verified.
        shell.pump();
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "an ipfs:// URL is not content-verified until its bytes actually verify"
        );

        // The scheme handler resolves the main resource through the hash-verified
        // path and marks the load verified; then the load settles.
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert_eq!(shell.chrome().load_state, LoadState::Finished);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::ContentVerified,
            "the verified content path surfaces the content-verified posture"
        );
        assert!(shell.chrome().is_content_verified());
    }

    #[test]
    fn the_verified_posture_does_not_leak_into_a_later_served_load() {
        // The indicator must track the CURRENT page: after a content-verified load,
        // navigating to a plain served origin resets the chrome to the untrusted
        // posture (a fresh navigation begins unverified until proven otherwise).
        let (mut shell, handle) = shell_with_backend();
        shell.navigate("ipfs://bafyfixturecid/").unwrap();
        handle.serve_via_verified_content_path();
        settle(&mut shell, &handle);
        assert!(shell.chrome().is_content_verified());

        shell.navigate("https://example.com/").unwrap();
        settle(&mut shell, &handle);
        assert_eq!(
            shell.chrome().trust_posture,
            TrustPosture::UnverifiedOrigin,
            "the verified posture does not leak onto a later plain served load"
        );
        assert!(!shell.chrome().is_content_verified());
    }

    #[test]
    fn the_shell_forwards_focus_through_the_seam() {
        // Acceptance: the shell makes the page interactive THROUGH the seam. It
        // focuses the live view via the seam (how the embedded webview widget
        // receives real OS scroll/click/focus/keyboard input). We assert the CALL
        // crosses the seam, not that the webview's no-ops move anything (per the
        // task's forward-pointer).
        let (mut shell, handle) = shell_with_backend();
        shell.focus_page(true);
        assert_eq!(
            handle.focus_calls(),
            [true],
            "focus was forwarded via the seam"
        );
    }
}
