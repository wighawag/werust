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
    MenuButton, Orientation, Popover, Widget,
};

use webkit6::prelude::WebViewExt;
use webview_renderer::WebViewRenderer;
use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG};
use werust_core::{BrowserShell, ChromeState};

/// The URL werust opens when none is given on the command line.
const DEFAULT_URL: &str = "https://example.com/";

/// The GTK application id for the shell window.
const APP_ID: &str = "com.github.wighawag.werust";

/// Whether a key press should open the WebKit Web Inspector (the in-window
/// devtools: a console REPL + network + DOM for the page), given the pressed
/// key and the active modifiers.
///
/// The chosen shortcut is F12 with NO modifiers (task
/// `enable-web-inspector-devtools-all-platforms`,
/// `work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`):
/// F12 is the desktop-browser-idiomatic devtools key and, crucially, does NOT
/// collide with the GTK INTERACTIVE debugger (widget tree / CSS), which GTK4
/// binds to Ctrl+Shift+I and Ctrl+Shift+D. So opening the WEB inspector on F12
/// leaves the GTK debugger's own keys untouched, satisfying the
/// "does not conflict with the GTK interactive debugger" acceptance criterion.
///
/// Pure (a function of the keyval + modifiers) so the shortcut decision — in
/// particular that it is F12 and NOT Ctrl+Shift+I — is pinned display-free; the
/// GTK key controller that calls it, and the `show_inspector` it triggers, need a
/// display and are covered by the ignored end-to-end tests.
fn should_open_web_inspector(keyval: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    // F12 alone, ignoring lock modifiers (Caps/Num Lock) but rejecting any
    // Ctrl/Shift/Alt combination, so this never fires on the GTK debugger's
    // Ctrl+Shift+I / Ctrl+Shift+D.
    let chord = modifiers
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::ALT_MASK);
    keyval == gdk::Key::F12 && chord.is_empty()
}

/// Builds the startup banner shown when the browser launches.
///
/// The version comes from [`werust_core::version`], the ONE shared version source
/// the browser menu and the mobile edges also read, so the banner and the menu
/// can never disagree.
fn banner() -> String {
    format!(
        "werust {} — a Rust web browser (webview backend)",
        werust_core::version()
    )
}

/// The message the DEBUG menu entry shows until the debug VIEW exists.
///
/// The menu (this task) lands BEFORE the tabbed debug view (the follow-on tasks
/// `debug-view-console-network-tabs-desktop` / `-mobile`, which are blocked on
/// this menu plus the capture store). So the Debug entry is wired to an
/// open-debug-view HOOK — [`open_debug_view`] — that today states honestly that
/// the view is not built yet rather than silently doing nothing. Pure, so the
/// wording is pinned without a display; the recorded decision is in
/// `docs/spikes/general-browser-menu-with-version-and-debug-entry/DECISIONS.md`.
fn debug_view_placeholder_message() -> String {
    format!(
        "werust {} — the in-app debug view (Console + Network) is not built yet.",
        werust_core::version()
    )
}

/// The OPEN-DEBUG-VIEW hook the browser menu's Debug entry calls.
///
/// THIS is the one function `debug-view-console-network-tabs-desktop` replaces:
/// it will open the tabbed Console/Network panel over the capture store
/// ([`BrowserShell::debug_capture`]). Until then it states the placeholder
/// ([`debug_view_placeholder_message`]) so activating the entry has an honest,
/// visible effect. Keeping the hook a named function (rather than an inline
/// closure) is what makes the swap a one-site change.
fn open_debug_view(parent: &ApplicationWindow) {
    gtk4::AlertDialog::builder()
        .message("Debug")
        .detail(debug_view_placeholder_message())
        .build()
        .show(Some(parent));
}

/// Build the general browser MENU button: the ⋮ affordance every browser has,
/// opening a popover of the core's [`BrowserMenu`] items.
///
/// The menu is USER-FACING and always available (it is NOT debug-build-gated —
/// only its Debug ENTRY leads anywhere debug-ish, and the in-app debug view is
/// itself a user feature). The item LIST is the shared core's, so this function
/// only maps each [`MenuItemKind`] onto a widget: an
/// [`Info`](MenuItemKind::Info) item (the `werust <version>` line) becomes a
/// non-interactive label, an [`Action`](MenuItemKind::Action) item a flat button
/// dispatched by its stable id. A FUTURE menu item therefore needs no change
/// here at all unless it is an action with new behaviour — that is the
/// "structured to grow" property, expressed in code.
fn build_menu_button(window: &ApplicationWindow) -> MenuButton {
    let menu = BrowserMenu::new();
    let list = GtkBox::new(Orientation::Vertical, 2);
    for item in menu.items() {
        match item.kind {
            MenuItemKind::Info => {
                let label = Label::builder()
                    .label(&item.label)
                    .xalign(0.0)
                    .sensitive(false)
                    .build();
                label.add_css_class("menu-info-item");
                list.append(&label);
            }
            MenuItemKind::Action => {
                let button = Button::builder().label(&item.label).build();
                button.add_css_class("flat");
                // Dispatch on the STABLE id, never the display label.
                let id = item.id.clone();
                let window = window.clone();
                button.connect_clicked(move |button| {
                    // Close the popover first, so the menu does not sit over
                    // whatever the entry opens.
                    if let Some(popover) = button.ancestor(Popover::static_type()) {
                        if let Ok(popover) = popover.downcast::<Popover>() {
                            popover.popdown();
                        }
                    }
                    if id == MENU_ITEM_DEBUG {
                        open_debug_view(&window);
                    }
                });
                list.append(&button);
            }
        }
    }

    let popover = Popover::builder().child(&list).build();
    MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Menu")
        .popover(&popover)
        .build()
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
    /// The PROMINENT in-view error banner: a high-contrast bar across the top of
    /// the view that appears ONLY when a load failed, carrying the accurate,
    /// protocol-named reason. This is the fail-closed honesty fix — the subtle
    /// footer [`status_line`] was "not easily seen" (a real `ronan.eth` IPNS
    /// failure was missed), so a failed load now also raises this banner the user
    /// cannot miss. Hidden on a loading/idle/settled-ok chrome.
    error_banner: Label,
    /// The small "invalid URL" BADGE next to the URL bar, shown ONLY when the last
    /// URL-bar entry was INVALID (a scheme-less garbage entry that did not
    /// navigate). Paired with the URL-bar text rendered invalid (red underline via
    /// the `url-invalid` class), it surfaces the distinct invalid-URL state while
    /// KEEPING the typed text for the user to fix (field finding D) — orthogonal to
    /// the trust indicator and the error banner. Hidden on a valid entry.
    invalid_badge: Label,
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
        // The INVALID-URL surface (field finding D): when the last entry was
        // invalid (a scheme-less garbage entry that did not navigate), show the
        // small badge and render the URL-bar text as invalid (red underline),
        // keeping the typed text for the user to fix. Toggled from the orthogonal
        // `invalid_entry` axis — distinct from the trust indicator and the load
        // error banner — so a valid entry hides the badge and clears the class.
        let show_invalid = invalid_entry_badge_visible(state);
        self.invalid_badge.set_visible(show_invalid);
        self.invalid_badge.set_text(invalid_entry_badge_text(state));
        if show_invalid {
            self.url_entry.add_css_class("url-invalid");
        } else {
            self.url_entry.remove_css_class("url-invalid");
        }
        self.back.set_sensitive(state.can_go_back);
        self.forward.set_sensitive(state.can_go_forward);
        // Stop is meaningful only while a load is in flight; Reload only once it
        // has settled.
        self.stop.set_sensitive(state.is_loading());
        self.reload.set_sensitive(!state.is_loading());
        self.status.set_text(&status_line(state));
        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate, protocol-named reason across the top of the view so the user
        // cannot miss why nothing rendered (the fail-closed honesty fix). Hidden
        // otherwise, so it never nags on a normal load.
        let show_error = error_banner_visible(state);
        self.error_banner.set_visible(show_error);
        if show_error {
            self.error_banner.set_text(&error_banner_text(state));
            // Distinguish a transient/timeout banner (softer amber, retryable)
            // from a hard-failure banner (prominent red): toggle exactly one
            // class, like the trust-indicator set, so a stale class never lingers
            // across a transition.
            let active = error_banner_css_class(state);
            for class in ["error-banner", "error-banner-transient"] {
                if class == active {
                    self.error_banner.add_css_class(class);
                } else {
                    self.error_banner.remove_css_class(class);
                }
            }
        }
        // The trust indicator: a distinct, legible label for each state (a
        // neutral loading badge while a load is in flight, else the trust posture:
        // content-verified / name-via-trusted-RPC / mutable-name / unverified
        // origin), plus a CSS class so the states are visually distinct. Exactly
        // one class is active at a time, so the toggle set must list EVERY class
        // `trust_indicator_css_class` can return — including `trust-loading` and
        // `trust-mutable-name` — or a stale class would linger on a transition.
        self.trust.set_text(trust_indicator(state));
        self.trust
            .set_tooltip_text(Some(trust_indicator_detail(state)));
        let active = trust_indicator_css_class(state);
        for class in [
            "trust-loading",
            "trust-verified",
            "trust-name-trusted-rpc",
            "trust-mutable-name",
            "trust-unverified",
        ] {
            if class == active {
                self.trust.add_css_class(class);
            } else {
                self.trust.remove_css_class(class);
            }
        }
    }
}

/// The one-line status shown in the chrome: a surfaced failure wins, otherwise a
/// loading indicator that names the REAL pipeline STEP (resolving name / fetching
/// record / fetching content / rendering) so a slow load reads as "working",
/// otherwise idle. Kept pure so it is trivially correct and reusable.
///
/// The step hint is the core's [`ChromeState::load_step`] (driven by the actual
/// lifecycle), so "loading…" gains a live "— <step>" tail while a load is in
/// flight (task `clearer-loading-and-error-indicator`).
fn status_line(state: &ChromeState) -> String {
    if let Some(reason) = &state.last_error {
        format!("failed: {reason}")
    } else if state.is_loading() {
        let hint = state.load_step().hint();
        if hint.is_empty() {
            "loading…".to_string()
        } else {
            format!("loading… — {hint}")
        }
    } else {
        "idle".to_string()
    }
}

/// Whether the PROMINENT in-view error banner should be shown: exactly when the
/// last load failed ([`ChromeState::last_error`] is set).
///
/// The whole point of fail-closed is that the user UNDERSTANDS why nothing
/// rendered (`docs/adr/0001`: the honesty stance). The subtle one-line
/// [`status_line`] footer was "not easily seen" (the human missed a real
/// `ronan.eth` IPNS failure), so a failed load ALSO raises this high-contrast
/// banner across the top of the view — an error state the user cannot miss —
/// while a loading/idle chrome hides it. A pure function of [`ChromeState`] so it
/// is testable without a display; the mobile shells apply the same rule from the
/// chrome JSON.
fn error_banner_visible(state: &ChromeState) -> bool {
    state.last_error.is_some()
}

/// The PROMINENT error-banner text for a failed load: a protocol-named,
/// accurate reason drawn straight from [`ChromeState::last_error`] (the decoder /
/// resolver taxonomy — e.g. "IPNS record did not verify: …", "points to Swarm,
/// not supported"), never a generic "failed". Empty when there is no failure (the
/// banner is hidden then). Pure, for the same reason as [`status_line`].
///
/// The reason text is the SAME `last_error` the core surfaces, so the banner and
/// the footer never disagree; it is only shown far more prominently.
///
/// A TRANSIENT/timeout failure (retryable) is surfaced DISTINCTLY from a HARD
/// failure (task `clearer-loading-and-error-indicator`): a transient failure
/// reads as a softer "timed out" with an explicit "reload to retry" affordance
/// (the Reload button IS the retry — a failed ENS load re-resolves), while a hard
/// failure keeps the prominent "failed to load" wording with its protocol-named
/// reason. The distinction is the core's [`ChromeState::failure_is_retryable`]
/// (a pure classification of the reason), so the two never disagree with the
/// footer.
fn error_banner_text(state: &ChromeState) -> String {
    match &state.last_error {
        Some(reason) if state.failure_is_retryable() => {
            format!("⏳ This page timed out — reload to retry: {reason}")
        }
        Some(reason) => format!("⚠ This page failed to load: {reason}"),
        None => String::new(),
    }
}

/// The CSS class for the error banner, distinguishing a TRANSIENT/timeout failure
/// (a softer, retryable amber banner) from a HARD failure (the prominent red
/// banner). A pure function of [`ChromeState`] so the banner styling is testable
/// without a display; the two classes are toggled in [`Chrome::refresh`] exactly
/// like the trust-indicator classes.
fn error_banner_css_class(state: &ChromeState) -> &'static str {
    if state.failure_is_retryable() {
        "error-banner-transient"
    } else {
        "error-banner"
    }
}

/// Whether the small "invalid URL" BADGE should be shown: exactly when the last
/// URL-bar entry was INVALID (a scheme-less garbage entry that did not navigate).
///
/// This is the field-finding-D surface (finding D,
/// `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`):
/// a garbage entry does not navigate; instead of silently resetting the bar, the
/// chrome shows a small badge and renders the URL-bar text as invalid (red
/// underline), keeping the typed text for the user to fix. A pure read of the
/// orthogonal [`ChromeState::has_invalid_entry`] axis — distinct from a load
/// failure ([`error_banner_visible`]) — so it is testable without a display and
/// the mobile shells apply the SAME rule from the chrome JSON.
fn invalid_entry_badge_visible(state: &ChromeState) -> bool {
    state.has_invalid_entry()
}

/// The small "invalid URL" badge text for an invalid entry, empty otherwise (the
/// badge is hidden then). Pure, for the same reason as [`invalid_entry_badge_visible`].
fn invalid_entry_badge_text(state: &ChromeState) -> &'static str {
    if state.has_invalid_entry() {
        "⛔ invalid URL"
    } else {
        ""
    }
}

/// The short label the chrome's trust indicator shows: a distinct, legible badge
/// for a content-verified load vs a served-by-an-unverified-origin load
/// (`docs/adr/0001`: the trust posture is a product surface, not a silent
/// internal). A pure function of [`ChromeState`] so it is trivially correct and
/// testable without a display; the label text carries a shield vs a plain-globe
/// glyph so the states read at a glance even before colour.
///
/// The name-via-trusted-RPC state (an ENS-resolved Phase-1 page: bytes verified,
/// but the name->CID mapping came from a trusted RPC) is a DISTINCT middle badge
/// that is deliberately NOT labelled "verified" — Phase 1 makes no
/// name-verification claim.
///
/// While a load is IN FLIGHT (`is_loading()`) the indicator is a NEUTRAL loading
/// state that WINS over the posture, making NO trust claim at all — the
/// trust-honesty fix (`chrome-loading-state-resets-trust-indicator`): on
/// navigation to a possibly differently-trusted page, the indicator must not keep
/// asserting the previous page's (or a not-yet-proven) trust while the new page
/// loads. The real posture is revealed only once the load SETTLES
/// (finished/failed/idle). This loading-wins precedence lives at the same display
/// layer as the two-axis posture precedence, and is applied identically on the
/// mobile shells (they consult the same `loading` fact from the chrome JSON).
fn trust_indicator(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "⋯ loading…"
    } else if state.is_content_verified() {
        "✓ verified"
    } else if state.is_name_via_trusted_rpc() {
        "◈ name via trusted RPC"
    } else if state.is_mutable_name() {
        "◇ content verified, mutable name"
    } else {
        "⚠ unverified origin"
    }
}

/// The longer explanation shown as the trust indicator's tooltip, so the badge is
/// self-explaining on hover. Pure, for the same reason as [`trust_indicator`].
fn trust_indicator_detail(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "werust is loading this page and is not yet asserting a trust level for it: the trust indicator shows the real posture only once the load settles."
    } else if state.is_content_verified() {
        "This page was content-verified: its bytes were hash-checked against their content identifier on the content-addressed path."
    } else if state.is_name_via_trusted_rpc() {
        "This page's content was hash-verified, but its name was resolved over a TRUSTED RPC (not a light client), which could misdirect the name to different content. werust makes no name-verification claim here."
    } else if state.is_mutable_name() {
        "This page's content was hash-verified, but its name is MUTABLE: the controller (an IPNS key holder, or an ENS name owner) can repoint it to different content at any time. werust makes no immutability claim here."
    } else {
        "This page was served by an origin werust does not trust by default; its content was not hash-verified."
    }
}

/// The CSS class for the current posture's badge — exactly one of the three
/// trust classes. A pure function of [`ChromeState`] so the badge styling is
/// testable without a display.
fn trust_indicator_css_class(state: &ChromeState) -> &'static str {
    if state.is_loading() {
        "trust-loading"
    } else if state.is_content_verified() {
        "trust-verified"
    } else if state.is_name_via_trusted_rpc() {
        "trust-name-trusted-rpc"
    } else if state.is_mutable_name() {
        "trust-mutable-name"
    } else {
        "trust-unverified"
    }
}

/// The stylesheet that makes the trust-indicator states visually distinct: a
/// NEUTRAL grey loading badge (no trust claim, shown while a load is in flight),
/// a green content-verified badge, a blue name-via-trusted-RPC badge (an honest
/// middle state), a purple mutable-name badge, and an amber unverified-origin
/// one. Kept as one constant next to the classes the chrome toggles
/// (`trust-loading` / `trust-verified` / `trust-name-trusted-rpc` /
/// `trust-mutable-name` / `trust-unverified`).
const TRUST_INDICATOR_CSS: &str = "\
.trust-loading { color: #5c5c5c; font-weight: bold; padding: 0 6px; }\
.trust-verified { color: #0a7d28; font-weight: bold; padding: 0 6px; }\
.trust-name-trusted-rpc { color: #1a5fb4; font-weight: bold; padding: 0 6px; }\
.trust-mutable-name { color: #6c3fb4; font-weight: bold; padding: 0 6px; }\
.trust-unverified { color: #9a6a00; font-weight: bold; padding: 0 6px; }\
.error-banner { background-color: #c01c28; color: #ffffff; font-weight: bold; padding: 10px 12px; }\
.error-banner-transient { background-color: #b5820a; color: #ffffff; font-weight: bold; padding: 10px 12px; }\
.invalid-url-badge { color: #c01c28; font-weight: bold; padding: 0 6px; }\
.menu-info-item { padding: 4px 8px; }\
.url-invalid { color: #c01c28; text-decoration: underline; text-decoration-color: #c01c28; }";

/// Load the trust-indicator stylesheet onto the default display, so the
/// `trust-verified` / `trust-name-trusted-rpc` / `trust-unverified` classes the
/// chrome toggles render as visually distinct badges. A no-op if there is no
/// display.
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
    // `install_ipfs` hands back the `_redirects` 3xx redirect sink: a matching
    // 3xx rule (IPIP-0002) is a NAVIGATION the scheme handler cannot perform (it
    // runs off the UI thread), so it queues the `ipfs://<rootcid><to>` target here
    // and the shell drains it on its existing pump, navigating for real (bar +
    // history move) with the target hash-verified by the fresh retrieval that
    // navigation triggers (task `ipfs-redirects-3xx-navigation-support`).
    let redirects = backend.install_ipfs();
    // Make a `target="_blank"` link / `window.open(url)` navigate IN THE CURRENT
    // view instead of being silently dropped. werust has no tab/window model yet,
    // so WebKitGTK's new-window (`create`) request is routed into the existing
    // view through the SAME navigation/scheme path (an `ipfs://`/ENS `_blank`
    // target is still hash-verified, an unsupported one still refused) and no
    // second window is spawned (task
    // `blank-and-window-open-links-navigate-in-place`, field finding C,
    // `docs/adr/0010`).
    backend.install_new_window_in_place();
    // Make the webview FOLLOW the OS light/dark color-scheme setting, so
    // `prefers-color-scheme` and UA-styled controls match the user's OS preference
    // instead of silently defaulting to light. On a dark-mode desktop this is what
    // makes UA-styled controls (e.g. mandalas.eth.limo's nav buttons) theme dark
    // and readable, at parity with Firefox; on a light desktop it keeps light. It
    // reads the OS preference from the XDG desktop portal and sets
    // `gtk-application-prefer-dark-theme` to match (WebKitGTK ties
    // `prefers-color-scheme` to that flag), tracking live OS changes — without
    // forcing dark or overriding a page's own declared `color-scheme` (task
    // `webview-follow-os-color-scheme`, `docs/adr/0009`).
    backend.follow_os_color_scheme();
    // Wire the DESKTOP console + network CAPTURE POINTS that feed the in-app debug
    // menu's Console and Network tabs: an injected `console.*` shim over the
    // script-message bridge (WebKitGTK 6 exposes no console signal, so desktop
    // uses the SAME shared shim iOS does) plus the webview's resource-load signals
    // (which see `https://` too, not just the `ipfs://` the scheme handler
    // intercepts). Capture is READ-ONLY observation: it never answers a request,
    // alters a load, or changes a trust posture — each entry merely REPORTS the
    // honest per-request posture (ADR-0006), with the main-document row taking the
    // load's own posture so the Network tab cannot contradict the trust indicator
    // (task `debug-console-network-capture-per-platform`).
    //
    // The store is created HERE and handed to BOTH sides: a clone into the capture
    // hooks (installed before the backend is boxed behind the seam) and the other
    // into the shell via `with_debug_capture`, exactly as the redirect sink is
    // shared — both clones are the SAME store, so the debug view renders what the
    // hooks captured.
    //
    // The `_redirects` sink is handed in too: it is the codebase's ONE main-frame
    // predicate (the shell reports every top-level navigation into it), which the
    // network capture reads to decide which row is the main-document row. A raw
    // URL compare would miss the WebKit authority-less `ipfs:///<cid>` form, and
    // comparing against the chrome's DISPLAYED url would never fire on an ENS page
    // at all (the name is pinned there) — exactly the page the reconciliation
    // exists for.
    let debug_capture = werust_core::debug::DebugCapture::new();
    backend.install_debug_capture(debug_capture.clone(), redirects.clone());
    // Capture the live WebKitGTK view BEFORE the backend is boxed behind the
    // `Renderer` seam, so the shell can open the WEB inspector (F12) on it. The
    // web inspector is a WebKitGTK-specific surface (not part of the cross-backend
    // seam), so it is wired here on the concrete view rather than through the
    // seam — the same reason `install_provider`/`install_ipfs` wire directly. A
    // clone is a cheap refcounted GObject handle; it stays valid because the
    // backend (which owns the view) outlives the window (task
    // `enable-web-inspector-devtools-all-platforms`).
    let inspector_view = backend.web_view().clone();
    let shell = Rc::new(RefCell::new(
        BrowserShell::new(Box::new(backend))
            .with_redirect_sink(redirects)
            .with_debug_capture(debug_capture),
    ));

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

    // The PROMINENT in-view error banner: a high-contrast red bar across the top
    // of the view, shown ONLY on a failed load (the fail-closed honesty fix). It
    // starts hidden and wraps its (protocol-named) reason so a long message stays
    // legible.
    let error_banner = Label::builder()
        .halign(gtk4::Align::Fill)
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error_banner.add_css_class("error-banner");

    // The small "invalid URL" badge sits in the toolbar next to the URL bar,
    // shown ONLY when the last entry was invalid (field finding D). It starts
    // hidden; when shown it pairs with the URL-bar text rendered invalid (red
    // underline via the `url-invalid` class).
    let invalid_badge = Label::builder().visible(false).build();
    invalid_badge.add_css_class("invalid-url-badge");

    let toolbar = GtkBox::new(Orientation::Horizontal, 4);
    toolbar.append(&back);
    toolbar.append(&forward);
    toolbar.append(&reload);
    toolbar.append(&stop);
    toolbar.append(&url_entry);
    toolbar.append(&invalid_badge);
    toolbar.append(&trust);
    // The general browser MENU (⋮) sits at the END of the toolbar, where every
    // other browser puts it. Appended after the window exists so the Debug entry
    // can parent its hook's dialog; see below.

    let chrome = Rc::new(Chrome {
        url_entry: url_entry.clone(),
        back: back.clone(),
        forward: forward.clone(),
        reload: reload.clone(),
        stop: stop.clone(),
        status: status.clone(),
        trust: trust.clone(),
        error_banner: error_banner.clone(),
        invalid_badge: invalid_badge.clone(),
    });

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&toolbar);
    // The prominent error banner sits directly under the toolbar and ABOVE the
    // page view, so a failed load's reason is unmissable in the content area, not
    // buried in the footer status line.
    root.append(&error_banner);
    root.append(&view);
    root.append(&status);

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(1024)
        .default_height(768)
        .title("werust")
        .child(&root)
        .build();

    // The GENERAL browser menu: a ⋮ button at the end of the toolbar opening a
    // popover of the shared core's `BrowserMenu` items — today the werust version
    // line and a Debug entry that calls `open_debug_view`. User-facing and always
    // available (never debug-build-gated), and built to grow: a new core menu item
    // shows up here with no layout change (task
    // `general-browser-menu-with-version-and-debug-entry`).
    toolbar.append(&build_menu_button(&window));

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

    // Wire the WEB inspector shortcut: F12 opens the WebKitGTK Web Inspector
    // (a real console REPL + network + DOM) over the current page IN-WINDOW
    // (task `enable-web-inspector-devtools-all-platforms`). F12 is chosen to NOT
    // collide with the GTK interactive debugger (Ctrl+Shift+I / Ctrl+Shift+D),
    // which is a separate GTK-level widget/CSS surface, not web content. The key
    // controller is added to the WINDOW so the shortcut works wherever focus is,
    // and `show_inspector` is a safe no-op in a release build (developer-extras is
    // off there — the inspector is gated on a debug build), so this shortcut
    // cannot open devtools on a shipped build.
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.connect_key_pressed(move |_controller, keyval, _keycode, modifiers| {
        if should_open_web_inspector(keyval, modifiers) {
            if let Some(inspector) = inspector_view.inspector() {
                inspector.show();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

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
    use super::{
        banner, debug_view_placeholder_message, error_banner_css_class, error_banner_text,
        error_banner_visible, invalid_entry_badge_text, invalid_entry_badge_visible,
        should_open_web_inspector, status_line, trust_indicator, trust_indicator_css_class,
        trust_indicator_detail, DEFAULT_URL,
    };
    use gtk4::gdk;
    use renderer::{LoadState, TrustPosture};
    use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG, MENU_ITEM_VERSION};
    use werust_core::{ChromeState, LoadStep};

    #[test]
    fn f12_opens_the_web_inspector_and_the_gtk_debugger_chord_does_not() {
        // Acceptance: the desktop web-inspector shortcut is F12 (a real console
        // REPL + network in-window), and it does NOT conflict with the GTK
        // interactive debugger, which GTK4 binds to Ctrl+Shift+I / Ctrl+Shift+D.
        // So F12 (no modifiers) opens the WEB inspector, while the GTK debugger's
        // own chords must NOT trigger it — the two surfaces stay distinct.
        assert!(
            should_open_web_inspector(gdk::Key::F12, gdk::ModifierType::empty()),
            "F12 opens the web inspector"
        );
        // Caps/Num Lock (non-chord modifiers) must not stop F12 firing.
        assert!(should_open_web_inspector(
            gdk::Key::F12,
            gdk::ModifierType::LOCK_MASK
        ));

        // The GTK interactive debugger's chords must NOT open the web inspector.
        let gtk_debugger_chord = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        assert!(
            !should_open_web_inspector(gdk::Key::i, gtk_debugger_chord),
            "Ctrl+Shift+I is the GTK debugger, not the web inspector"
        );
        assert!(
            !should_open_web_inspector(gdk::Key::d, gtk_debugger_chord),
            "Ctrl+Shift+D is the GTK debugger, not the web inspector"
        );
        // A modified F12 (any Ctrl/Shift/Alt) is not the plain-F12 shortcut either,
        // so the web-inspector key is unambiguous and cannot be a debugger chord.
        assert!(!should_open_web_inspector(
            gdk::Key::F12,
            gdk::ModifierType::CONTROL_MASK
        ));
        // An unrelated key never opens it.
        assert!(!should_open_web_inspector(
            gdk::Key::a,
            gdk::ModifierType::empty()
        ));
    }

    #[test]
    fn status_line_names_the_live_pipeline_step_while_loading() {
        // Acceptance (loading progress): while a load is in flight the status line
        // names the REAL pipeline step (resolving name / fetching content /
        // rendering) so a slow load reads as working, not frozen. A settled/idle
        // load shows no step, and a failure still wins.
        let fetching = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingContent,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&fetching), "loading… — fetching content");

        let resolving = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&resolving), "loading… — resolving name");

        let rendering = ChromeState {
            load_state: LoadState::Committed,
            load_step: LoadStep::Rendering,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&rendering), "loading… — rendering");

        // A loading state with no known step (Idle step) falls back to plain
        // "loading…" rather than a dangling dash.
        let loading_no_step = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::Idle,
            ..ChromeState::default()
        };
        assert_eq!(status_line(&loading_no_step), "loading…");

        // A settled load shows idle; a failure still wins over any step.
        assert_eq!(status_line(&ChromeState::default()), "idle");
        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        assert_eq!(
            status_line(&failed),
            "failed: points to Swarm, not supported"
        );
    }

    #[test]
    fn a_transient_timeout_banner_is_distinct_and_retryable_while_a_hard_fail_keeps_its_reason() {
        // Acceptance: a transient/timeout failure is surfaced DISTINCTLY from a
        // hard failure, with an obvious retry affordance; a hard failure keeps its
        // prominent protocol-named reason. Both banners carry the honest reason
        // verbatim; only the framing + the CSS class differ.
        let transient = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("transport error: timeout: global".into()),
            ..ChromeState::default()
        };
        assert!(error_banner_visible(&transient));
        let text = error_banner_text(&transient);
        assert!(
            text.to_lowercase().contains("retry"),
            "a transient failure offers a retry affordance: {text}"
        );
        assert!(
            text.contains("transport error: timeout: global"),
            "the honest reason is kept: {text}"
        );
        assert_eq!(error_banner_css_class(&transient), "error-banner-transient");

        // A hard failure: the prominent "failed to load" wording, its
        // protocol-named reason, and NO retry affordance.
        let hard = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        let hard_text = error_banner_text(&hard);
        assert!(
            hard_text.contains("failed to load"),
            "a hard failure reads as a load failure: {hard_text}"
        );
        assert!(hard_text.contains("points to Swarm, not supported"));
        assert!(
            !hard_text.to_lowercase().contains("retry"),
            "a hard failure offers no retry: {hard_text}"
        );
        assert_eq!(error_banner_css_class(&hard), "error-banner");

        // A verification failure is HARD even though it is a failure of a fetched
        // record: retrying will not make it verify.
        let verify_fail = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("IPNS record did not verify: bad signature".into()),
            ..ChromeState::default()
        };
        assert_eq!(error_banner_css_class(&verify_fail), "error-banner");
        assert!(!error_banner_text(&verify_fail)
            .to_lowercase()
            .contains("retry"));
    }

    #[test]
    fn banner_names_werust() {
        assert!(banner().starts_with("werust "));
    }

    #[test]
    fn the_desktop_menu_renders_the_shared_core_items_with_the_one_shared_version() {
        // Acceptance (desktop): the ⋮ menu is built from the SHARED core
        // `BrowserMenu`, so the desktop popover shows the SAME version line and
        // the SAME Debug entry the Android and iOS menus show — one source, three
        // native surfaces. Asserted on the model the GTK builder consumes, since
        // the widget tree itself needs a display (the manual steps for the real
        // popover are recorded in the task's spike dir).
        let menu = BrowserMenu::new();

        let version = menu.item(MENU_ITEM_VERSION).expect("a version entry");
        assert_eq!(version.label, format!("werust {}", werust_core::version()));
        assert_eq!(
            version.kind,
            MenuItemKind::Info,
            "the version line is rendered non-interactive in the popover"
        );
        // The startup banner and the menu read the SAME version source, so the
        // two can never drift apart.
        assert!(
            banner().contains(werust_core::version()),
            "the banner and the menu agree on the version: {}",
            banner()
        );

        let debug = menu.item(MENU_ITEM_DEBUG).expect("a debug entry");
        assert_eq!(debug.label, "Debug");
        assert_eq!(
            debug.kind,
            MenuItemKind::Action,
            "the Debug entry is the activatable one: it opens the debug view"
        );
    }

    #[test]
    fn the_debug_entry_hook_states_the_view_is_not_built_yet_rather_than_doing_nothing() {
        // Acceptance: the Debug entry opens the debug view via an
        // OPEN-DEBUG-VIEW HOOK the debug-view task fills. This task lands the menu
        // FIRST (the view tasks are blocked on it), so the hook must have an
        // HONEST visible effect meanwhile — not a silent no-op that reads as a
        // broken menu item. The wording names the version and the view's real
        // content (Console + Network) so the user knows what is coming.
        let message = debug_view_placeholder_message();
        assert!(
            message.contains(werust_core::version()),
            "the placeholder names the running version: {message}"
        );
        assert!(
            message.contains("Console") && message.contains("Network"),
            "the placeholder names what the debug view will show: {message}"
        );
        assert!(
            message.contains("not built yet"),
            "the placeholder is honest that the view does not exist yet: {message}"
        );
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
    fn an_invalid_entry_shows_the_badge_distinct_from_a_load_error() {
        // Field finding D: an INVALID URL-bar entry (a scheme-less garbage entry
        // that did not navigate) shows the small "invalid URL" badge, distinct
        // from a load-error banner. A valid/idle chrome hides it; a LOAD failure
        // (`last_error`) is NOT the invalid badge (the two axes are orthogonal).
        let idle = ChromeState::default();
        assert!(!invalid_entry_badge_visible(&idle));
        assert_eq!(invalid_entry_badge_text(&idle), "");

        let load_failure = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert!(
            !invalid_entry_badge_visible(&load_failure),
            "a load failure is not the invalid-entry badge"
        );

        let invalid = ChromeState {
            url_text: "not a url".into(),
            invalid_entry: Some("not a url".into()),
            ..ChromeState::default()
        };
        assert!(invalid_entry_badge_visible(&invalid));
        assert!(invalid_entry_badge_text(&invalid).contains("invalid URL"));
        // The invalid-entry badge is orthogonal to a load error: it carries no
        // `last_error`, so the error banner stays hidden.
        assert!(!error_banner_visible(&invalid));
    }

    #[test]
    fn a_failed_load_raises_a_prominent_error_banner_with_the_accurate_protocol_named_reason() {
        // Acceptance (the fail-closed honesty fix): a failed load raises a
        // PROMINENT in-view error banner the user cannot miss, carrying the
        // accurate, protocol-named reason (the resolver/decoder taxonomy verbatim),
        // NOT only the subtle footer status line the human missed. It is hidden on
        // an idle or an in-flight load, and only appears on a failure.
        let idle = ChromeState::default();
        assert!(
            !error_banner_visible(&idle),
            "no banner when nothing has failed"
        );
        assert_eq!(error_banner_text(&idle), "");

        let loading = ChromeState {
            load_state: LoadState::Started,
            ..ChromeState::default()
        };
        assert!(
            !error_banner_visible(&loading),
            "no banner while a load is in flight"
        );

        // A real IPNS failure (the ronan.eth taxonomy): the banner is VISIBLE and
        // carries the protocol-named reason, not a generic "failed".
        let ipns_failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some(
                "IPNS record did not verify: dag-cbor data does not match the protobuf fields"
                    .into(),
            ),
            ..ChromeState::default()
        };
        assert!(
            error_banner_visible(&ipns_failed),
            "a failed load raises the prominent banner"
        );
        let text = error_banner_text(&ipns_failed);
        assert!(
            text.contains("IPNS record did not verify"),
            "the banner carries the accurate protocol-named reason: {text}"
        );
        assert!(
            text.contains("failed to load"),
            "the banner reads as a load failure the user cannot miss: {text}"
        );

        // An unsupported-protocol failure likewise surfaces its named reason.
        let unsupported = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        assert!(error_banner_visible(&unsupported));
        assert!(error_banner_text(&unsupported).contains("points to Swarm"));
    }

    #[test]
    fn trust_indicator_shows_a_neutral_loading_state_that_hides_the_posture_while_loading() {
        // Acceptance (the trust-honesty fix): while a load is in flight the trust
        // indicator is a NEUTRAL loading state (no trust claim), NOT the
        // carried-over posture of the previous page. The display rule is
        // loading-wins: even a load whose backend posture still reads
        // content-verified (mid-transition) must show the loading badge, so the
        // indicator never asserts a trust level for a page that is not yet shown.
        for posture in [
            TrustPosture::UnverifiedOrigin,
            TrustPosture::ContentVerified,
            TrustPosture::NameViaTrustedRpc,
            TrustPosture::MutableName,
        ] {
            let loading = ChromeState {
                load_state: LoadState::Started,
                trust_posture: posture,
                ..ChromeState::default()
            };
            assert_eq!(
                trust_indicator(&loading),
                "⋯ loading…",
                "while loading, the indicator is a neutral loading state, not the {posture:?} posture"
            );
            // The loading badge makes NO trust claim: it never reads "verified"
            // and never asserts the origin is (un)verified.
            assert!(!trust_indicator(&loading)
                .to_lowercase()
                .contains("verified"));
            assert_eq!(trust_indicator_css_class(&loading), "trust-loading");
            // The tooltip is honest that werust is not yet asserting a trust level.
            assert!(trust_indicator_detail(&loading)
                .to_lowercase()
                .contains("loading"));
            assert!(!trust_indicator_detail(&loading)
                .to_lowercase()
                .contains("verified"));
        }

        // A Committed load is still in flight, so it is still the neutral state.
        let committed = ChromeState {
            load_state: LoadState::Committed,
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&committed), "⋯ loading…");

        // Once the load SETTLES (Finished), the real posture appears — the loading
        // state does not swallow the settled badge.
        let settled = ChromeState {
            load_state: LoadState::Finished,
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&settled), "✓ verified");

        // A FAILED load is not "loading": it shows its (unverified) posture, not the
        // spinner — a failed load must never read as a stale success.
        let failed = ChromeState {
            load_state: LoadState::Failed,
            trust_posture: TrustPosture::UnverifiedOrigin,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert_eq!(trust_indicator(&failed), "⚠ unverified origin");
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

    #[test]
    fn trust_indicator_shows_a_distinct_name_via_trusted_rpc_badge_never_labelled_verified() {
        // Acceptance: an ENS-resolved Phase-1 page (bytes verified, name resolved
        // over a trusted RPC) renders as its OWN legible, visually-distinct badge
        // — distinct from BOTH the verified and the unverified-origin badges — and
        // it is NEVER surfaced as "verified" / "name-verified".
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        let name_via_rpc = ChromeState {
            trust_posture: TrustPosture::NameViaTrustedRpc,
            ..ChromeState::default()
        };

        let label = trust_indicator(&name_via_rpc);
        assert_eq!(label, "◈ name via trusted RPC");
        // Distinct from the other two badges.
        assert_ne!(label, trust_indicator(&verified));
        assert_ne!(label, trust_indicator(&served));
        // NEVER labelled "verified" / "name-verified": Phase 1 makes no such claim.
        assert!(
            !label.to_lowercase().contains("verified"),
            "the name-via-trusted-RPC badge must never read as verified: {label}"
        );
        assert!(!trust_indicator_detail(&name_via_rpc)
            .to_lowercase()
            .contains("name-verified"));
        // The tooltip is honest that the name came from a trusted RPC.
        assert!(trust_indicator_detail(&name_via_rpc).contains("TRUSTED RPC"));

        // The badge carries its own CSS class, distinct from the other two, so the
        // three states are visually distinct.
        assert_eq!(
            trust_indicator_css_class(&name_via_rpc),
            "trust-name-trusted-rpc"
        );
        assert_eq!(trust_indicator_css_class(&verified), "trust-verified");
        assert_eq!(trust_indicator_css_class(&served), "trust-unverified");
    }

    #[test]
    fn trust_indicator_shows_a_distinct_mutable_name_badge_never_labelled_verified() {
        // Acceptance: a client-verified IPNS page (bytes verified, name mutable)
        // renders as its OWN legible, visually-distinct badge — distinct from the
        // verified, name-via-trusted-RPC, and unverified badges — and it is NEVER
        // surfaced as "verified".
        let verified = ChromeState {
            trust_posture: TrustPosture::ContentVerified,
            ..ChromeState::default()
        };
        let name_via_rpc = ChromeState {
            trust_posture: TrustPosture::NameViaTrustedRpc,
            ..ChromeState::default()
        };
        let served = ChromeState {
            trust_posture: TrustPosture::UnverifiedOrigin,
            ..ChromeState::default()
        };
        let mutable = ChromeState {
            trust_posture: TrustPosture::MutableName,
            ..ChromeState::default()
        };

        let label = trust_indicator(&mutable);
        assert_eq!(label, "◇ content verified, mutable name");
        // Distinct from the other three badges.
        assert_ne!(label, trust_indicator(&verified));
        assert_ne!(label, trust_indicator(&name_via_rpc));
        assert_ne!(label, trust_indicator(&served));
        // Its "verified" only ever appears as part of "content verified", never as
        // a bare immutability claim; the badge is honest that the NAME is mutable.
        assert!(
            label.contains("mutable name"),
            "the mutable-name badge must say the name is mutable: {label}"
        );
        // The tooltip is honest that the name is mutable / controller-repointable,
        // and makes NO immutability claim (it may say it makes "no immutability
        // claim", but must not assert the name IS immutable).
        let detail = trust_indicator_detail(&mutable);
        assert!(detail.contains("MUTABLE"));
        assert!(
            detail.contains("can repoint"),
            "the tooltip is honest the controller can repoint the name: {detail}"
        );

        // Its own CSS class, distinct from the other three.
        assert_eq!(trust_indicator_css_class(&mutable), "trust-mutable-name");
        assert_ne!(
            trust_indicator_css_class(&mutable),
            trust_indicator_css_class(&name_via_rpc)
        );
    }
}
