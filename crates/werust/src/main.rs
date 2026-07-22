//! The `werust` browser binary: the day-one product shell.
//!
//! werust opens a real window with a URL bar and back/forward/reload/stop
//! controls over a LIVE, interactive page view, driven ENTIRELY through the
//! [`Renderer`] seam (`CONTEXT.md`, `docs/adr/0001`). The seam-facing logic — the
//! URL bar, the nav controls, and the chrome that reflects load state — lives in
//! [`shell`] as a GTK-free [`BrowserShell`]; this file is the thin GTK view over
//! it: it builds the window, paints [`ChromeState`] into widgets, forwards
//! button/entry actions to the shell, and pumps the seam's load-lifecycle events
//! on the GTK main loop. It never calls WebKitGTK directly; the live view is
//! embedded via the seam's opaque [`ViewHandle`], and page interaction
//! (scroll/click/focus/type) is served by that embedded, focused widget.
//!
//! The seam-driving logic itself is NOT here: [`BrowserShell`]/[`ChromeState`]
//! live in the shared `werust-core` crate ("the Rust core"), so the SAME core
//! backs this GTK view, the Android Kotlin edge, and the iOS Swift edge.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{
    gdk, glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    Orientation, Widget,
};

use webview_renderer::WebViewRenderer;
use werust_core::{BrowserShell, ChromeState};

/// The URL werust opens when none is given on the command line.
const DEFAULT_URL: &str = "https://example.com/";

/// The GTK application id for the shell window.
const APP_ID: &str = "com.github.wighawag.werust";

/// Builds the startup banner shown when the browser launches.
fn banner() -> String {
    format!(
        "werust {} — a Rust web browser (webview backend)",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() -> glib::ExitCode {
    println!("{}", banner());

    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_URL.into());

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        if let Err(e) = open_window(app, &url) {
            eprintln!("werust: {e}");
        }
    });
    // Do not treat CLI args as files to open.
    app.run_with_args::<&str>(&[])
}

/// The widgets the pump refreshes from [`ChromeState`]: the URL bar, the nav
/// controls, and the load indicator. Grouped so a single [`refresh_chrome`] call
/// keeps every piece of chrome in step with the seam's state.
struct Chrome {
    url_entry: Entry,
    back: Button,
    forward: Button,
    reload: Button,
    stop: Button,
    status: Label,
    /// The trust indicator: shows whether the current page was content-verified
    /// (hash-checked on the content-addressed path) or served by an unverified
    /// origin (`docs/adr/0001`: the trust posture is a product surface).
    trust: Label,
}

impl Chrome {
    /// Paint the given [`ChromeState`] into the widgets: URL bar text, control
    /// availability (Back/Forward greyed as history allows), the Stop vs Reload
    /// active state (Stop only while loading), and the status/failure line.
    fn refresh(&self, state: &ChromeState) {
        // Only overwrite the URL bar when it does not already hold this text, so
        // the caret does not jump while the user is mid-edit and the value is
        // unchanged.
        if self.url_entry.text() != state.url_text {
            self.url_entry.set_text(&state.url_text);
        }
        self.back.set_sensitive(state.can_go_back);
        self.forward.set_sensitive(state.can_go_forward);
        // Stop is meaningful only while a load is in flight; Reload only once it
        // has settled.
        self.stop.set_sensitive(state.is_loading());
        self.reload.set_sensitive(!state.is_loading());
        self.status.set_text(&status_line(state));
        // The trust indicator: a distinct, legible label for content-verified vs
        // served-by-an-unverified-origin, plus a CSS class so the two states are
        // visually distinct (a verified badge vs an unverified one).
        self.trust.set_text(trust_indicator(state));
        self.trust
            .set_tooltip_text(Some(trust_indicator_detail(state)));
        if state.is_content_verified() {
            self.trust.remove_css_class("trust-unverified");
            self.trust.add_css_class("trust-verified");
        } else {
            self.trust.remove_css_class("trust-verified");
            self.trust.add_css_class("trust-unverified");
        }
    }
}

/// The one-line status shown in the chrome: a surfaced failure wins, otherwise a
/// loading/idle indicator. Kept pure so it is trivially correct and reusable.
fn status_line(state: &ChromeState) -> String {
    if let Some(reason) = &state.last_error {
        format!("failed: {reason}")
    } else if state.is_loading() {
        "loading…".to_string()
    } else {
        "idle".to_string()
    }
}

/// The short label the chrome's trust indicator shows: a distinct, legible badge
/// for a content-verified load vs a served-by-an-unverified-origin load
/// (`docs/adr/0001`: the trust posture is a product surface, not a silent
/// internal). A pure function of [`ChromeState`] so it is trivially correct and
/// testable without a display; the label text carries a shield vs a plain-globe
/// glyph so the two states read at a glance even before colour.
fn trust_indicator(state: &ChromeState) -> &'static str {
    if state.is_content_verified() {
        "✓ verified"
    } else {
        "⚠ unverified origin"
    }
}

/// The longer explanation shown as the trust indicator's tooltip, so the badge is
/// self-explaining on hover. Pure, for the same reason as [`trust_indicator`].
fn trust_indicator_detail(state: &ChromeState) -> &'static str {
    if state.is_content_verified() {
        "This page was content-verified: its bytes were hash-checked against their content identifier on the content-addressed path."
    } else {
        "This page was served by an origin werust does not trust by default; its content was not hash-verified."
    }
}

/// The stylesheet that makes the two trust-indicator states visually distinct: a
/// green content-verified badge vs an amber unverified-origin one. Kept as one
/// constant next to the classes the chrome toggles (`trust-verified` /
/// `trust-unverified`).
const TRUST_INDICATOR_CSS: &str = "\
.trust-verified { color: #0a7d28; font-weight: bold; padding: 0 6px; }\
.trust-unverified { color: #9a6a00; font-weight: bold; padding: 0 6px; }";

/// Load the trust-indicator stylesheet onto the default display, so the
/// `trust-verified` / `trust-unverified` classes the chrome toggles render as
/// two visually distinct badges. A no-op if there is no display.
fn install_trust_indicator_css() {
    let provider = CssProvider::new();
    provider.load_from_string(TRUST_INDICATOR_CSS);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Open the shell window over the webview backend and navigate it to `url`.
///
/// Builds the URL bar + back/forward/reload/stop toolbar over the embedded live
/// view, wires each control to drive the [`BrowserShell`] (never the webview
/// directly), and starts a periodic pump that folds the seam's load-lifecycle
/// events into the chrome.
fn open_window(app: &Application, url: &str) -> Result<(), renderer::RendererError> {
    // Build the webview backend and install the native EIP-1193 provider on it
    // (the first trust hook) BEFORE handing it to the shell: pages then see a
    // native `window.ethereum` whose `request(...)` calls round-trip across the
    // script-message bridge to native code and back — with no keys involved (task
    // `eip1193-provider-injection-via-script-bridge`).
    let mut backend = WebViewRenderer::new()?;
    backend.install_provider();
    // Wire the second trust hook: native `ipfs://` resolution through the seam's
    // custom-scheme hook, backed by the hash-verified content-addressed fetch
    // path. An `ipfs://<cid>/…` URL then loads and renders a VERIFIED
    // content-addressed page at parity with a served page; a hash mismatch fails
    // the load rather than rendering unverified bytes (task
    // `ipfs-scheme-resolution-through-renderer-seam`).
    backend.install_ipfs();
    let shell = Rc::new(RefCell::new(BrowserShell::new(Box::new(backend))));

    // Make the two trust-indicator states VISUALLY DISTINCT: a green verified
    // badge vs an amber unverified-origin one. Loaded once for the display so the
    // `trust-verified` / `trust-unverified` classes the chrome toggles are styled.
    install_trust_indicator_css();

    // Embed the live, interactive view. The seam hands the shell an opaque
    // pointer to the backend's native view; the shell reconstructs it as a plain
    // GtkWidget to pack into its window without knowing it is a WebKitGTK view.
    let handle = shell.borrow().view_handle();
    // SAFETY: `view_handle()` returns a live GtkWidget pointer owned by the
    // backend for the shell to embed; `from_glib_none` takes a borrowed ref and
    // does not consume ownership. The backend outlives the window (the shell is
    // held for the run of the loop below).
    let view: Widget = unsafe { glib::translate::from_glib_none(handle.0 as *mut _) };
    view.set_vexpand(true);
    view.set_hexpand(true);

    // The toolbar: back / forward / reload / stop + the URL bar.
    let back = Button::from_icon_name("go-previous-symbolic");
    let forward = Button::from_icon_name("go-next-symbolic");
    let reload = Button::from_icon_name("view-refresh-symbolic");
    let stop = Button::from_icon_name("process-stop-symbolic");
    let url_entry = Entry::builder()
        .hexpand(true)
        .placeholder_text("Enter a URL and press Enter")
        .build();
    let status = Label::new(Some("idle"));
    // The trust indicator sits in the toolbar, next to the URL bar, so the trust
    // posture of the current page is always visible in the chrome.
    let trust = Label::new(Some(trust_indicator(&ChromeState::default())));
    trust.add_css_class("trust-unverified");

    let toolbar = GtkBox::new(Orientation::Horizontal, 4);
    toolbar.append(&back);
    toolbar.append(&forward);
    toolbar.append(&reload);
    toolbar.append(&stop);
    toolbar.append(&url_entry);
    toolbar.append(&trust);

    let chrome = Rc::new(Chrome {
        url_entry: url_entry.clone(),
        back: back.clone(),
        forward: forward.clone(),
        reload: reload.clone(),
        stop: stop.clone(),
        status: status.clone(),
        trust: trust.clone(),
    });

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&toolbar);
    root.append(&view);
    root.append(&status);

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(1024)
        .default_height(768)
        .title("werust")
        .child(&root)
        .build();

    // Wire each control to drive the shell THROUGH the seam, then repaint chrome.
    // Shared as an `Rc<dyn Fn()>` so every handler (and the pump) can hold it.
    let refresh: Rc<dyn Fn()> = {
        let chrome = chrome.clone();
        let shell = shell.clone();
        Rc::new(move || chrome.refresh(shell.borrow().chrome()))
    };

    url_entry.connect_activate({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |entry| {
            let _ = shell.borrow_mut().navigate(&entry.text());
            refresh();
        }
    });
    back.connect_clicked({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |_| {
            shell.borrow_mut().go_back();
            refresh();
        }
    });
    forward.connect_clicked({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |_| {
            shell.borrow_mut().go_forward();
            refresh();
        }
    });
    reload.connect_clicked({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |_| {
            let _ = shell.borrow_mut().reload();
            refresh();
        }
    });
    stop.connect_clicked({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |_| {
            shell.borrow_mut().stop();
            refresh();
        }
    });

    // Navigate to the startup URL through the seam and focus the live view so the
    // OS/GTK routes scroll/click/focus/keyboard input to the page (interactive).
    shell.borrow_mut().navigate(url)?;
    shell.borrow_mut().focus_page(true);
    refresh();

    // Pump the seam's load-lifecycle events on the GTK loop and keep the chrome in
    // step; this is what turns WebKitGTK's async load into a live, reflected UI.
    glib::timeout_add_local(Duration::from_millis(50), {
        let shell = shell.clone();
        let refresh = refresh.clone();
        move || {
            if shell.borrow_mut().pump() {
                refresh();
            }
            glib::ControlFlow::Continue
        }
    });

    window.present();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{banner, status_line, trust_indicator, trust_indicator_detail, DEFAULT_URL};
    use renderer::{LoadState, TrustPosture};
    use werust_core::ChromeState;

    #[test]
    fn banner_names_werust() {
        assert!(banner().starts_with("werust "));
    }

    #[test]
    fn default_url_is_an_https_url() {
        assert!(DEFAULT_URL.starts_with("https://"));
    }

    #[test]
    fn status_line_prefers_a_failure_then_loading_then_idle() {
        // The chrome's status line is a pure function of ChromeState: a surfaced
        // failure wins, otherwise loading vs idle follows the load state.
        let idle = ChromeState::default();
        assert_eq!(status_line(&idle), "idle");

        let loading = ChromeState {
            load_state: LoadState::Started,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&loading), "loading…");

        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert_eq!(status_line(&failed), "failed: name not resolved");
    }

    #[test]
    fn trust_indicator_distinguishes_verified_from_unverified_and_is_a_pure_fn_of_posture() {
        // Acceptance: the chrome's trust indicator shows a clear, distinct state
        // for a content-verified load vs an unverified served-origin load, and it
        // is driven by the posture the seam reports (the actual load path), not by
        // any URL string. The two labels are visibly different and legible.
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };

        assert_eq!(trust_indicator(&served), "⚠ unverified origin");
        assert_eq!(trust_indicator(&verified), "✓ verified");
        assert_ne!(
            trust_indicator(&served),
            trust_indicator(&verified),
            "the two trust states are visually distinct"
        );

        // The detail/tooltip likewise distinguishes the two and names the reason.
        assert!(trust_indicator_detail(&verified).contains("content-verified"));
        assert!(trust_indicator_detail(&served).contains("not"));

        // The default (nothing loaded yet) is the untrusted posture: werust does
        // not claim verification it has not proven.
        assert_eq!(
            trust_indicator(&ChromeState::default()),
            "⚠ unverified origin"
        );
    }
}
