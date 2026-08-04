//! The native-widget desktop painter's HOST-INDEPENDENT half: every value an OS
//! edge paints, derived HERE from the shared `werust-core` rules — ONCE, for
//! every such edge.
//!
//! This crate is the seam between "decide" and "draw". `werust-core` decides
//! (`status_line`, `trust_indicator` / `_detail` / `_css_class`,
//! `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, and the debug
//! view's `console_row_text` / `network_*` row rules); the AppKit window
//! (`werust-macos`) and the Win32 window (`werust-windows`) draw. Between them
//! sits one plain-Rust snapshot per surface — [`ChromePaint`],
//! [`ConsoleRowPaint`], [`NetworkRowPaint`], [`MenuItemPaint`] — assembled by
//! calling the core, never by restating it.
//!
//! # Why ONE crate rather than one module per edge
//!
//! It landed as `werust-macos::paint` with the AppKit window, and the Win32
//! window would have been its second copy — of the CARRIER and, worse, of the
//! PALETTE, whose hex values are already transcribed once into the GTK edge's
//! `APP_CSS`. "The same green on both desktops" was a promise kept by
//! transcription; a third transcription is how the Kotlin and Swift chrome twins
//! drifted and how the trust EXPLANATION shipped desktop-only for months
//! (`docs/adr/0011` Consequences). So the module was EXTRACTED here verbatim,
//! with its tests, and both windows consume it. It is the painter's face of what
//! `webview-shared` is for the backends: the toolkit-free half every edge of a
//! kind shares.
//!
//! The GTK edge is deliberately NOT a consumer: it has a real stylesheet
//! (`APP_CSS`) and toggles CSS classes on GTK widgets, so it needs the core's
//! class NAMES but not an in-code palette. AppKit and Win32 have no stylesheet at
//! all, which is why they need this. Folding GTK in would be a rewrite of a
//! working painter for no new guarantee; instead
//! `the_gtk_stylesheet_and_the_shared_palette_agree` asserts the two never
//! disagree about a colour.
//!
//! # Why a snapshot rather than calling the core from the toolkit layer
//!
//! Two reasons, both about being CHECKED rather than merely written:
//!
//! 1. **It is testable on the gate.** This crate compiles on Ubuntu against the
//!    REAL `werust-core`, so `cargo test -p desktop-paint` asserts that what a
//!    window will paint IS the core's derivation — on a machine with no Mac and
//!    no Windows box. Only the widget assignment is left unproven by the gate,
//!    and that is what the `macos-14` / `windows-latest` CI legs and the recorded
//!    manual steps cover.
//! 2. **It keeps the un-gated half small and dumb.** The AppKit and Win32 layers
//!    are the code the Ubuntu gate can never compile, so the less DECIDING they
//!    contain, the less can be wrong there. Each reads a struct field and sets a
//!    widget property.
//!
//! This is the same shape the mobile edges already use — Kotlin and Swift paint
//! from `chrome_json()`, a carrier of the same one derivation — with a Rust
//! struct instead of JSON because both sides here are Rust. It is a CARRIER, not
//! a second derivation: every field below is the return value of a core function,
//! and the tests assert exactly that.
//!
//! # Colour
//!
//! The core exports stable class NAMES (`TRUST_INDICATOR_CSS_CLASSES`,
//! `ERROR_BANNER_CSS_CLASSES`, `DEBUG_CONSOLE_CSS_CLASSES`) and has no notion of
//! colour; the stylesheet stays in the edge (`docs/adr/0011`'s layering, kept by
//! the GTK edge's `APP_CSS`). [`CLASS_COLORS`] is the native-widget edges'
//! stylesheet: the same palette the GTK window uses, so a content-verified badge
//! is the same green on every desktop, and [`class_color`] is total over every
//! exported class (the gate drives the core's `CssClassFamily::ALL` — every
//! exported FAMILY, not a list named here — so a new family or a new state reds
//! this gate exactly as it does on GTK).

use renderer::Renderer;
#[cfg(test)]
use renderer::TrustPosture;
use werust_core::debug::{
    console_level_css_class, console_row_text, network_mime_text, network_size_text,
    network_status_text, network_trust_css_class, network_trust_label, tail_plan, ConsoleEntry,
    DebugCapture, NetworkEntry, TailPlan,
};
use werust_core::menu::{BrowserMenu, MenuItemKind};
use werust_core::{
    error_banner_css_class, error_banner_text, error_banner_visible, invalid_entry_badge_text,
    invalid_entry_badge_visible, load_progress_fraction, load_progress_tooltip,
    load_progress_visible, load_spinner_visible, reload_stop_control, status_line, trust_indicator,
    trust_indicator_css_class, trust_indicator_detail, trust_pin_action_label,
    trust_pin_action_visible, trust_pin_detail, ChromeState, ReloadStopControl,
    STOP_AFFORDANCE_LABEL,
};

/// A colour, as three 0.0–1.0 components.
///
/// Deliberately NOT a toolkit colour type: this crate must compile on the Ubuntu
/// gate, where neither AppKit nor GDI exists. Each window converts it in ONE
/// place (`NSColor::colorWithSRGBRed_green_blue_alpha` on macOS, a `COLORREF`
/// on Win32).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    /// The red component, 0.0–1.0.
    pub red: f64,
    /// The green component, 0.0–1.0.
    pub green: f64,
    /// The blue component, 0.0–1.0.
    pub blue: f64,
}

/// Build an [`Rgb`] from a `0xRRGGBB` literal, so the table below reads as the
/// same hex the GTK stylesheet uses and the two cannot silently diverge in
/// transcription.
const fn rgb(hex: u32) -> Rgb {
    Rgb {
        red: ((hex >> 16) & 0xff) as f64 / 255.0,
        green: ((hex >> 8) & 0xff) as f64 / 255.0,
        blue: (hex & 0xff) as f64 / 255.0,
    }
}

/// The native-widget edges' STYLESHEET: the colour each exported state class is
/// painted in.
///
/// For a label state (`trust-*`, `debug-console-*`) that is the TEXT colour; for
/// the error banner (`error-banner*`) it is the banner's FILL, whose text is
/// always white — the same split the GTK `APP_CSS` rules make, with the same hex
/// values, so the trust badge that is green on the GTK window is the same green
/// here.
///
/// A class the core exports but this table omits would render with no colour at
/// all — the "correctly toggled but invisible" failure the GTK edge's
/// no-unstyled-class test exists for. [`class_color`] is therefore driven by the
/// core's family AGGREGATE (`CssClassFamily::ALL`) in
/// `every_exported_class_has_a_colour`, so the gate reds both when a new state
/// lands unstyled and when a whole new FAMILY lands — which a family list written
/// out here would have missed entirely (task
/// `one-derivation-close-the-aggregate-and-tooltip-gaps`).
///
/// # ADR-0009 (follow the OS colour scheme)
///
/// These are ACCENT colours on otherwise system-drawn controls: the window, the
/// toolbar, the URL field, the buttons, the tabs and the debug rows are drawn by
/// the platform and therefore already follow the user's light/dark setting, and
/// no edge here forces an appearance (forcing dark — or light — is exactly what
/// ADR-0009 forbids). The accent hues are shared with the GTK edge rather than
/// re-picked per platform or per appearance, so werust's trust vocabulary reads
/// the same on every desktop; if a hue proves unreadable in dark mode that is a
/// tuning follow-up for ALL edges, not a reason for one to mint its own.
pub const CLASS_COLORS: &[(&str, Rgb)] = &[
    // The chrome's trust-indicator family (`TRUST_INDICATOR_CSS_CLASSES`).
    ("trust-loading", rgb(0x5c_5c_5c)),
    ("trust-verified", rgb(0x0a_7d_28)),
    ("trust-name-trusted-rpc", rgb(0x1a_5f_b4)),
    ("trust-mutable-name", rgb(0x6c_3f_b4)),
    // A blessed name that now points to DIFFERENT content: the loudest badge
    // there is, so it wears the error banner's own red rather than a fifth hue
    // (task `ipns-tofu-pin-and-warn-on-change`).
    ("trust-name-changed", rgb(0xc0_1c_28)),
    ("trust-unverified", rgb(0x9a_6a_00)),
    // The error banner's severity family (`ERROR_BANNER_CSS_CLASSES`): the FILL.
    ("error-banner", rgb(0xc0_1c_28)),
    ("error-banner-transient", rgb(0xb5_82_0a)),
    // The debug view's console-level family (`DEBUG_CONSOLE_CSS_CLASSES`).
    ("debug-console-log", rgb(0x3d_3d_3d)),
    ("debug-console-info", rgb(0x1a_5f_b4)),
    ("debug-console-warn", rgb(0x9a_6a_00)),
    ("debug-console-error", rgb(0xc0_1c_28)),
    ("debug-console-debug", rgb(0x5c_5c_5c)),
];

/// The colour for one exported state class, or [`None`] when this edge has no
/// rule for it (which the gate treats as a defect, not a fallback).
#[must_use]
pub fn class_color(class: &str) -> Option<Rgb> {
    CLASS_COLORS
        .iter()
        .find(|(name, _)| *name == class)
        .map(|(_, color)| *color)
}

/// The colour used when the core exports a class this edge has no rule for.
///
/// It exists so a missing rule degrades to a VISIBLE (if unstyled) badge rather
/// than a panic in the paint path — but it is never reached in a green build:
/// `every_exported_class_has_a_colour` fails the gate the moment a class has no
/// entry in [`CLASS_COLORS`], which is the same guarantee the GTK edge's
/// no-unstyled-class test gives.
const FALLBACK_COLOR: Rgb = rgb(0x00_00_00);

/// The INVALID-entry colour: the URL bar's text while the last entry was invalid
/// (the red the GTK edge's `.url-invalid` rule uses), paired with the badge.
pub const INVALID_ENTRY_COLOR: Rgb = rgb(0xc0_1c_28);

/// The URL bar's load-progress fill (the GTK edge's `entry > progress` rule).
pub const LOAD_PROGRESS_COLOR: Rgb = rgb(0x1a_5f_b4);

/// Everything the toolbar, the error surface and the status line show for one
/// [`ChromeState`], with every value taken from the shared core derivation.
///
/// One struct per refresh rather than a dozen accessors, so the window's paint
/// path is a single straight-line assignment block that cannot half-apply a
/// state (the stale-badge failure mode the exported class sets exist to prevent).
#[derive(Debug, Clone, PartialEq)]
pub struct ChromePaint {
    /// The URL bar's text (`ChromeState::url_text`). The window writes it only
    /// when it differs, so the caret does not jump mid-edit.
    pub url_text: String,
    /// Whether the last URL-bar entry was INVALID: the bar's text is rendered in
    /// [`INVALID_ENTRY_COLOR`] and the badge is shown, while the typed text is
    /// KEPT for the user to fix (field finding D).
    pub invalid_entry: bool,
    /// The invalid-entry badge's text (empty when hidden).
    pub invalid_badge_text: &'static str,
    /// Whether Back is enabled.
    pub can_go_back: bool,
    /// Whether Forward is enabled.
    pub can_go_forward: bool,
    /// Whether a load is in flight: Stop is enabled exactly then, Reload exactly
    /// when it is not.
    pub is_loading: bool,
    /// Which mode werust's ONE reload/stop control is in
    /// ([`reload_stop_control`]): it RELOADS a settled page and STOPS a load in
    /// flight, so the window builds one button and re-labels it instead of
    /// enabling one of a pair. The cancel affordance is unmoved: `Stop` is
    /// offered on exactly the fact the separate Stop button was enabled on.
    pub reload_stop_control: ReloadStopControl,
    /// The glyph that mode's affordance wears ([`ReloadStopControl::label`]). An
    /// edge with a themed icon set may draw its own icon for the mode instead;
    /// the ACCESSIBLE name is still
    /// [`reload_stop_description`](ChromePaint::reload_stop_description).
    pub reload_stop_label: &'static str,
    /// What the control does, in words ([`ReloadStopControl::description`]): the
    /// window's tooltip / accessible name for it.
    pub reload_stop_description: &'static str,
    /// Whether the chrome's LOADING SPINNER is showing
    /// ([`load_spinner_visible`]): a second PRESENTATION of the load the URL bar
    /// already reports, on the same rule as
    /// [`progress_visible`](ChromePaint::progress_visible), never a second truth.
    /// The window shows/hides (and spins) its own indicator from this; nothing
    /// here starts a timer.
    pub spinner_visible: bool,
    /// The one-line status shown under the page view.
    pub status_text: String,
    /// The trust indicator's badge text.
    pub trust_text: &'static str,
    /// The trust indicator's longer explanation (the badge's tooltip).
    pub trust_detail: &'static str,
    /// The trust indicator's state class.
    pub trust_class: &'static str,
    /// The colour for [`trust_class`](ChromePaint::trust_class).
    pub trust_color: Rgb,
    /// Whether the PROMINENT error banner is shown (a failed load, and only a
    /// failed load — the one state allowed to displace the page).
    pub error_visible: bool,
    /// The banner's protocol-named reason (empty when hidden).
    pub error_text: String,
    /// The banner's severity class (hard vs transient/timeout).
    pub error_class: &'static str,
    /// The fill colour for [`error_class`](ChromePaint::error_class).
    pub error_color: Rgb,
    /// Whether the URL bar's progress indicator is showing.
    pub progress_visible: bool,
    /// The progress fraction, 0.0–1.0.
    pub progress_fraction: f64,
    /// The URL bar's tooltip while a load is in flight: the phase name, plus the
    /// cancel hint exactly when there is a backend load Stop can cancel. [`None`]
    /// clears it, so a stale phase never lingers on hover.
    pub progress_tooltip: Option<String>,
    /// Whether the trust surface should offer the trust-on-first-use BLESS action
    /// for this page ([`trust_pin_action_visible`]): the page is a name-resolved
    /// load whose mutable name is not already blessed at this very CID.
    ///
    /// An AFFORDANCE, never a prompt: the window shows the action inside the
    /// surface the user opened from the trust badge, and pops nothing up on its
    /// own (task `ipns-tofu-pin-and-warn-on-change`).
    pub trust_pin_action_visible: bool,
    /// The BLESS action's label ([`trust_pin_action_label`]), empty when it is not
    /// offered. Two wordings, because a first-use bless and accepting a CHANGE are
    /// materially different decisions.
    pub trust_pin_action_label: &'static str,
    /// The trust surface's TOFU body ([`trust_pin_detail`]): the mutable name, the
    /// CID it resolves to right now, and what (if anything) the user blessed for
    /// it. Empty when the page has no mutable name.
    pub trust_pin_detail: String,
}

impl ChromePaint {
    /// Derive everything the window paints from one [`ChromeState`].
    ///
    /// Every field is the return value of a `werust-core` function; this
    /// constructor adds no rule of its own beyond looking a colour up for a class
    /// the core chose. That is the property
    /// `the_paint_is_the_cores_derivation_verbatim` asserts, so "nothing is
    /// re-derived in an OS edge" is checked rather than promised.
    #[must_use]
    pub fn of(state: &ChromeState) -> Self {
        let trust_class = trust_indicator_css_class(state);
        let error_class = error_banner_css_class(state);
        // The phase NAME rides along as the URL bar's tooltip (the status line
        // already names it too), so the bar says WHICH phase is slow without
        // taking a fixed slot in the toolbar. The SENTENCE is the CORE's one rule
        // — phase, plus the cancel hint exactly while there is a backend load Stop
        // can cancel — not a second copy here; this edge contributes only the
        // label its own Stop button carries (`window::build`, a "✕" title).
        let progress_tooltip = load_progress_tooltip(state, STOP_AFFORDANCE_LABEL);
        // The ONE reload/stop control: the core says which mode it is in, what it
        // wears and what it does, so the window re-labels a single button rather
        // than enabling one of a pair on a condition of its own.
        let reload_stop = reload_stop_control(state);
        Self {
            url_text: state.url_text.clone(),
            invalid_entry: invalid_entry_badge_visible(state),
            invalid_badge_text: invalid_entry_badge_text(state),
            can_go_back: state.can_go_back,
            can_go_forward: state.can_go_forward,
            is_loading: state.is_loading(),
            reload_stop_control: reload_stop,
            reload_stop_label: reload_stop.label(),
            reload_stop_description: reload_stop.description(),
            spinner_visible: load_spinner_visible(state),
            status_text: status_line(state),
            trust_text: trust_indicator(state),
            trust_detail: trust_indicator_detail(state),
            trust_class,
            trust_color: class_color(trust_class).unwrap_or(FALLBACK_COLOR),
            error_visible: error_banner_visible(state),
            error_text: error_banner_text(state),
            error_class,
            error_color: class_color(error_class).unwrap_or(FALLBACK_COLOR),
            progress_visible: load_progress_visible(state),
            progress_fraction: load_progress_fraction(state),
            progress_tooltip,
            trust_pin_action_visible: trust_pin_action_visible(state),
            trust_pin_action_label: trust_pin_action_label(state),
            trust_pin_detail: trust_pin_detail(state),
        }
    }
}

/// One item of the ⋮ menu, from the core's [`BrowserMenu`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemPaint {
    /// The item's STABLE id, which the window dispatches on (never the label).
    pub id: String,
    /// The display label.
    pub label: String,
    /// Whether the item is activatable: an [`Action`](MenuItemKind::Action) item
    /// is a live menu entry, an [`Info`](MenuItemKind::Info) item (the
    /// `werust <version>` line) is rendered disabled.
    pub activatable: bool,
}

/// The ⋮ menu's items, in order, from the shared [`BrowserMenu`].
///
/// Each native menu is BUILT from this list, so a new core menu item appears in
/// every window with no per-OS change at all (and no hand-written platform list
/// can drift from what Android, iOS and the GTK desktop show).
#[must_use]
pub fn menu_items() -> Vec<MenuItemPaint> {
    BrowserMenu::new()
        .items()
        .iter()
        .map(|item| MenuItemPaint {
            id: item.id.clone(),
            label: item.label.clone(),
            activatable: matches!(item.kind, MenuItemKind::Action),
        })
        .collect()
}

/// One CONSOLE row: the core's row text, in the colour of the core's level class.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsoleRowPaint {
    /// `[<level>] <message> (<source>:<line>)`, from
    /// [`console_row_text`](werust_core::debug::console_row_text).
    pub text: String,
    /// The level's state class.
    pub class: &'static str,
    /// The colour for that class.
    pub color: Rgb,
}

impl ConsoleRowPaint {
    fn of(entry: &ConsoleEntry) -> Self {
        let class = console_level_css_class(entry.level);
        Self {
            text: console_row_text(entry),
            class,
            color: class_color(class).unwrap_or(FALLBACK_COLOR),
        }
    }
}

/// One NETWORK row: the core's columns, with the per-request trust posture in the
/// SAME vocabulary and the SAME colour the chrome's trust indicator uses
/// (`docs/adr/0006` — the debug view never mints a second trust label).
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkRowPaint {
    /// The request method.
    pub method: String,
    /// The response status, or `?`.
    pub status: String,
    /// The response MIME type, or `?`.
    pub mime: String,
    /// The response size, human-scaled, or `?`.
    pub size: String,
    /// The honest per-request trust label (glyph + wire name).
    pub trust: String,
    /// The trust column's state class (one of the chrome's own `trust-*`).
    pub trust_class: &'static str,
    /// The colour for that class.
    pub trust_color: Rgb,
    /// The request URL (the long column).
    pub url: String,
}

impl NetworkRowPaint {
    fn of(entry: &NetworkEntry) -> Self {
        let trust_class = network_trust_css_class(entry.trust);
        Self {
            method: entry.method.clone(),
            status: network_status_text(entry.status),
            mime: network_mime_text(&entry.mime),
            size: network_size_text(entry.size),
            trust: network_trust_label(entry.trust),
            trust_class,
            trust_color: class_color(trust_class).unwrap_or(FALLBACK_COLOR),
            url: entry.url.clone(),
        }
    }
}

/// What one debug-view tab must DO on this refresh, with the rows it needs.
///
/// The plan itself is the core's [`tail_plan`] (sequence-anchored, so ring-buffer
/// eviction at the cap cannot freeze the view); this enum carries only the rows
/// that plan actually needs built, so an idle tick allocates nothing and a
/// steady-state tick builds ONE row rather than re-rendering the whole 300-entry
/// store.
#[derive(Debug, Clone, PartialEq)]
pub enum TabUpdate<R> {
    /// Nothing to do.
    Noop,
    /// Replace every row with these.
    Rebuild(Vec<R>),
    /// Remove `drop` rows from the TOP (the ones the ring buffer evicted), then
    /// append these.
    Append {
        /// How many rows to remove from the view's top first.
        drop: usize,
        /// The new tail rows, in order.
        rows: Vec<R>,
    },
}

/// One tab's refresh: what to do, plus the two anchors the caller stores for the
/// next tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TabRefresh<R> {
    /// The update to apply.
    pub update: TabUpdate<R>,
    /// The sequence of the last store entry now rendered (the next tick's
    /// anchor), or [`None`] when the store is empty.
    pub last_sequence: Option<u64>,
    /// How many rows the view holds AFTER applying the update.
    pub rendered_rows: usize,
}

/// Plan and build the CONSOLE tab's next refresh from the shared store.
///
/// `rendered_rows` / `last_sequence` are what the caller stored from the previous
/// refresh. See [`TabUpdate`]; the eviction-at-the-cap rule is the core's.
#[must_use]
pub fn console_refresh(
    capture: &DebugCapture,
    rendered_rows: usize,
    last_sequence: Option<u64>,
) -> TabRefresh<ConsoleRowPaint> {
    let snapshot = capture.console();
    let sequences: Vec<u64> = snapshot.iter().map(ConsoleEntry::sequence).collect();
    refresh_from(
        &sequences,
        rendered_rows,
        last_sequence,
        &snapshot,
        ConsoleRowPaint::of,
    )
}

/// Plan and build the NETWORK tab's next refresh from the shared store.
#[must_use]
pub fn network_refresh(
    capture: &DebugCapture,
    rendered_rows: usize,
    last_sequence: Option<u64>,
) -> TabRefresh<NetworkRowPaint> {
    let snapshot = capture.network();
    let sequences: Vec<u64> = snapshot.iter().map(NetworkEntry::sequence).collect();
    refresh_from(
        &sequences,
        rendered_rows,
        last_sequence,
        &snapshot,
        NetworkRowPaint::of,
    )
}

/// The shared body of both tabs' refresh: apply the core's [`tail_plan`] to a
/// snapshot and build ONLY the rows that plan needs.
fn refresh_from<E, R>(
    sequences: &[u64],
    rendered_rows: usize,
    last_sequence: Option<u64>,
    snapshot: &[E],
    row: fn(&E) -> R,
) -> TabRefresh<R> {
    let plan = tail_plan(sequences, rendered_rows, last_sequence);
    let (update, rendered_rows) = match plan {
        TailPlan::Noop => (TabUpdate::Noop, rendered_rows),
        TailPlan::Rebuild => (
            TabUpdate::Rebuild(snapshot.iter().map(row).collect()),
            snapshot.len(),
        ),
        TailPlan::AppendFrom { drop, from } => {
            let rows: Vec<R> = snapshot[from..].iter().map(row).collect();
            let after = rendered_rows - drop + rows.len();
            (TabUpdate::Append { drop, rows }, after)
        }
    };
    TabRefresh {
        update,
        last_sequence: sequences.last().copied(),
        rendered_rows,
    }
}

/// Install the CONSOLE + NETWORK capture points on `backend`, the twin of the
/// WebKitGTK backend's `install_debug_capture` and of the iOS edge's.
///
/// # Why these edges inject shims, and what that honestly covers
///
/// `WKWebView` exposes NO console callback and NO per-resource load callback (the
/// WebKitGTK `console-message` / `resource-load-started` signals have no WebKit
/// API equivalent), and WebView2's equivalents live behind the DevTools protocol
/// and `AddWebResourceRequestedFilter("*")`, neither of which is wired (they are
/// named follow-ons, not silent omissions). So the only page-wide reach these
/// shells have today is INJECTED JS — the position iOS is already in, with the
/// same answer:
///
/// * CONSOLE: the SHARED [`console_shim`](werust_core::debug::console_shim), the
///   byte-for-byte same string desktop and iOS inject, from ONE place in
///   `werust-core`. It chains to the original `console.*`, so the page's console
///   and Safari's Web Inspector are unchanged.
/// * NETWORK: the [`network_shim`](werust_core::debug::network_shim), a
///   best-effort `fetch`/`XHR` wrapper. It sees only requests the PAGE makes
///   through those APIs — NOT browser-internal subresource loads (`<img>`,
///   `<script>`, CSS `url()`) and not the main document itself. That gap is
///   recorded rather than papered over, in
///   `docs/spikes/macos-appkit-window-and-chrome/README.md` and
///   `docs/spikes/windows-win32-window-and-chrome/README.md`.
///
/// Both shims post on the DEDICATED
/// [`CAPTURE_BRIDGE`](werust_core::debug::CAPTURE_BRIDGE) channel (never the
/// EIP-1193 provider's trust channel), and the registered handler routes each
/// body through the core's total, fail-quiet
/// [`route_capture_message`](werust_core::debug::route_capture_message). Capture
/// is READ-ONLY observation: nothing here answers a request, alters a load, or
/// changes a trust posture.
///
/// Taken over the `Renderer` seam (`&mut dyn Renderer`) rather than over the
/// concrete backend, so the Ubuntu gate can drive it against a fake and prove the
/// wiring exists — the exact class of "silently no-op'd on one platform" gap
/// `docs/adr/0005` exists to forbid.
pub fn install_debug_capture(backend: &mut dyn Renderer, capture: DebugCapture) {
    use werust_core::debug::{console_shim, network_shim, route_capture_message, CAPTURE_BRIDGE};

    backend.register_script_message_handler(
        CAPTURE_BRIDGE,
        Box::new(move |message| route_capture_message(&capture, &message.body)),
    );
    // Document-start user scripts, so a page's very first `console.log` and its
    // earliest `fetch` are captured.
    backend.inject_script(&console_shim());
    backend.inject_script(&network_shim());
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{LoadEvent, LoadState, RendererError, ViewHandle};
    use werust_core::debug::{ConsoleLevel, CAPTURE_BRIDGE, MAX_CONSOLE_ENTRIES};
    use werust_core::menu::{MENU_ITEM_DEBUG, MENU_ITEM_VERSION};
    use werust_core::pins::{MutableNameTrust, TrustedNamePin};
    use werust_core::{
        CssClassFamily, LoadStep, ERROR_BANNER_CSS_CLASSES, TRUST_INDICATOR_CSS_CLASSES,
    };

    #[test]
    fn the_paint_is_the_cores_derivation_verbatim() {
        // THE acceptance property of this crate: every surface a native window
        // shows reads the SHARED derivation, and nothing is re-derived per edge.
        // So for a spread of chrome states, each painted field must EQUAL the
        // core function that decides it — asserted against the real `werust-core`
        // on the Ubuntu gate, with no Mac and no Windows box in sight.
        let pin = TrustedNamePin {
            name: "ronan.eth".into(),
            cid: "bafyblessed".into(),
            blessed_at: 1_800_000_000,
            posture: TrustPosture::NameViaTrustedRpc,
        };
        let states = vec![
            ChromeState::default(),
            // Loading, mid-pipeline.
            ChromeState {
                url_text: "ipfs://bafy/index.html".into(),
                load_state: LoadState::Started,
                load_step: LoadStep::FetchingContent,
                ..Default::default()
            },
            // A settled, content-verified page.
            ChromeState {
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::ContentVerified,
                ..Default::default()
            },
            // A mutable name (never labelled "verified": ADR-0006/0007).
            ChromeState {
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::MutableName,
                ..Default::default()
            },
            // A hard failure, and a transient one.
            ChromeState {
                load_state: LoadState::Failed,
                last_error: Some("points to Swarm, not supported".into()),
                ..Default::default()
            },
            ChromeState {
                load_state: LoadState::Failed,
                last_error: Some("timed out fetching the IPNS record".into()),
                ..Default::default()
            },
            // An invalid URL-bar entry (the orthogonal axis).
            ChromeState {
                url_text: "not a url".into(),
                invalid_entry: Some("not a url".into()),
                ..Default::default()
            },
            // The TOFU mutable-name axis (task `ipns-tofu-pin-and-warn-on-change`):
            // a blessable-but-unblessed name, and a blessed name that has CHANGED
            // (the loudest settled state there is).
            ChromeState {
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::NameViaTrustedRpc,
                mutable_name: Some(MutableNameTrust {
                    name: "ronan.eth".into(),
                    cid: "bafyblessed".into(),
                    blessed: None,
                }),
                ..Default::default()
            },
            ChromeState {
                load_state: LoadState::Finished,
                trust_posture: TrustPosture::NameViaTrustedRpc,
                mutable_name: Some(MutableNameTrust {
                    name: "ronan.eth".into(),
                    cid: "bafychanged".into(),
                    blessed: Some(pin),
                }),
                ..Default::default()
            },
        ];

        for state in &states {
            let paint = ChromePaint::of(state);
            assert_eq!(paint.url_text, state.url_text);
            assert_eq!(paint.status_text, status_line(state));
            assert_eq!(paint.trust_text, trust_indicator(state));
            assert_eq!(paint.trust_detail, trust_indicator_detail(state));
            assert_eq!(paint.trust_class, trust_indicator_css_class(state));
            assert_eq!(paint.error_visible, error_banner_visible(state));
            assert_eq!(paint.error_text, error_banner_text(state));
            assert_eq!(paint.error_class, error_banner_css_class(state));
            assert_eq!(paint.invalid_entry, invalid_entry_badge_visible(state));
            assert_eq!(paint.invalid_badge_text, invalid_entry_badge_text(state));
            assert_eq!(paint.progress_visible, load_progress_visible(state));
            assert_eq!(paint.progress_fraction, load_progress_fraction(state));
            assert_eq!(
                paint.progress_tooltip,
                load_progress_tooltip(state, STOP_AFFORDANCE_LABEL),
                "the URL bar's progress sentence is the core's one rule, not a second copy here"
            );
            assert_eq!(
                paint.trust_pin_action_visible,
                trust_pin_action_visible(state)
            );
            assert_eq!(paint.trust_pin_action_label, trust_pin_action_label(state));
            assert_eq!(paint.trust_pin_detail, trust_pin_detail(state));
            assert_eq!(paint.can_go_back, state.can_go_back);
            assert_eq!(paint.can_go_forward, state.can_go_forward);
            assert_eq!(paint.is_loading, state.is_loading());
            assert_eq!(paint.reload_stop_control, reload_stop_control(state));
            assert_eq!(paint.reload_stop_label, reload_stop_control(state).label());
            assert_eq!(
                paint.reload_stop_description,
                reload_stop_control(state).description()
            );
            assert_eq!(paint.spinner_visible, load_spinner_visible(state));
            // Every class painted is one the core exports, so the colour lookup
            // can never be a name only this edge knows.
            assert!(TRUST_INDICATOR_CSS_CLASSES.contains(&paint.trust_class));
            assert!(ERROR_BANNER_CSS_CLASSES.contains(&paint.error_class));
            assert_eq!(paint.trust_color, class_color(paint.trust_class).unwrap());
            assert_eq!(paint.error_color, class_color(paint.error_class).unwrap());
        }
    }

    #[test]
    fn the_loading_chrome_makes_no_trust_claim_and_never_takes_the_page_area() {
        // Two product rules this window FOLLOWS rather than re-decides.
        //
        // 1. While a load is in flight the trust indicator is the core's neutral
        //    loading badge — the previous page's posture is never left asserting
        //    over a new page (`chrome-loading-state-resets-trust-indicator`).
        // 2. In-flight progress is visible but the ERROR BANNER — the only
        //    surface allowed to displace the page — stays hidden. Progress lives
        //    in the URL bar (task `loading-progress-in-the-url-bar-not-a-banner`).
        let loading = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::ResolvingName,
            trust_posture: TrustPosture::ContentVerified,
            ..Default::default()
        };
        let paint = ChromePaint::of(&loading);
        assert_eq!(paint.trust_class, "trust-loading");
        assert_eq!(paint.trust_text, trust_indicator(&loading));
        assert!(!paint.error_visible, "a load in flight is not a failure");
        assert!(paint.progress_visible && paint.progress_fraction > 0.0);
        assert!(
            paint
                .progress_tooltip
                .as_deref()
                .is_some_and(|t| t.contains("Stop")),
            "the cancel hint appears exactly while there is a load to stop: {:?}",
            paint.progress_tooltip
        );

        // Settled: no progress, and no tooltip left to linger on hover.
        let settled = ChromeState {
            load_state: LoadState::Finished,
            ..Default::default()
        };
        let paint = ChromePaint::of(&settled);
        assert!(!paint.progress_visible);
        assert_eq!(paint.progress_fraction, 0.0);
        assert_eq!(paint.progress_tooltip, None);

        // A FAILURE is the one state that may take the banner.
        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("boom".into()),
            ..Default::default()
        };
        assert!(ChromePaint::of(&failed).error_visible);
    }

    #[test]
    fn one_control_reloads_a_settled_page_and_stops_a_load_with_the_spinner_beside_it() {
        // Acceptance for the native-widget desktops (task
        // `reload-stop-collapse-and-loading-spinner-core-and-gtk`): the snapshot
        // carries the collapsed control's MODE and the spinner's visibility, so
        // the AppKit and Win32 windows build ONE button and re-label it instead of
        // enabling one of a pair on a condition of their own. Both are the core's
        // rules, driven here on the Ubuntu gate with no Mac and no Windows box.
        let settled = ChromeState {
            load_state: LoadState::Finished,
            ..Default::default()
        };
        let paint = ChromePaint::of(&settled);
        assert_eq!(paint.reload_stop_control, ReloadStopControl::Reload);
        assert_eq!(
            paint.reload_stop_label,
            werust_core::RELOAD_AFFORDANCE_LABEL
        );
        assert!(!paint.spinner_visible, "a settled chrome spins nothing");

        let loading = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingContent,
            ..Default::default()
        };
        let paint = ChromePaint::of(&loading);
        assert_eq!(
            paint.reload_stop_control,
            ReloadStopControl::Stop,
            "the cancel affordance survives the collapse, on the same fact Stop was enabled on"
        );
        assert_eq!(paint.reload_stop_label, STOP_AFFORDANCE_LABEL);
        assert_eq!(
            paint.reload_stop_control.action(),
            werust_core::shortcuts::ChromeAction::Stop,
            "activating it drives the SAME chrome action Escape resolves to"
        );
        assert!(paint.spinner_visible);

        // The pre-content resolution window: work is in flight (so the spinner
        // turns, beside the URL bar's own progress) but there is no backend load
        // for Stop to act on yet, so the control still offers Reload — exactly
        // what the core's progress tooltip promises there.
        let resolving = ChromeState {
            load_state: LoadState::Idle,
            load_step: LoadStep::ResolvingName,
            ..Default::default()
        };
        let paint = ChromePaint::of(&resolving);
        assert!(paint.spinner_visible && paint.progress_visible);
        assert_eq!(paint.reload_stop_control, ReloadStopControl::Reload);
    }

    #[test]
    fn a_changed_trusted_name_paints_the_loudest_badge_and_takes_the_failure_banner() {
        // Acceptance for the native-widget desktops (task
        // `ipns-tofu-pin-and-warn-on-change`): a blessed name that now points to
        // DIFFERENT content is painted at failure-class prominence: its own badge
        // class (never flattened into the mutable-name or trusted-RPC one), in the
        // banner's own red, plus the high-contrast banner itself, while an
        // UNBLESSED name is painted exactly as it was before this feature.
        let pin = TrustedNamePin {
            name: "ronan.eth".into(),
            cid: "bafyblessed".into(),
            blessed_at: 1_800_000_000,
            posture: TrustPosture::NameViaTrustedRpc,
        };
        let changed = ChromeState {
            load_state: LoadState::Finished,
            trust_posture: TrustPosture::NameViaTrustedRpc,
            mutable_name: Some(MutableNameTrust {
                name: "ronan.eth".into(),
                cid: "bafychanged".into(),
                blessed: Some(pin),
            }),
            ..Default::default()
        };
        let paint = ChromePaint::of(&changed);
        assert_eq!(paint.trust_class, "trust-name-changed");
        assert_ne!(paint.trust_class, "trust-mutable-name");
        assert_ne!(paint.trust_class, "trust-name-trusted-rpc");
        assert_eq!(
            paint.trust_color,
            class_color("error-banner").unwrap(),
            "the loudest badge wears the failure colour, not a fifth hue"
        );
        assert!(
            paint.error_visible,
            "a changed trusted name is failure-class"
        );
        assert!(paint.error_text.contains("ronan.eth"));
        assert_eq!(paint.error_class, "error-banner");
        assert!(paint.trust_pin_action_visible);
        assert!(!paint.trust_pin_action_label.is_empty());
        assert!(paint.trust_pin_detail.contains("bafychanged"));

        // Unblessed: unchanged chrome, plus the bless AFFORDANCE (not a prompt).
        let mut unblessed = changed.clone();
        unblessed.mutable_name = Some(MutableNameTrust {
            name: "ronan.eth".into(),
            cid: "bafychanged".into(),
            blessed: None,
        });
        let paint = ChromePaint::of(&unblessed);
        assert_eq!(paint.trust_class, "trust-name-trusted-rpc");
        assert!(!paint.error_visible);
        assert!(paint.trust_pin_action_visible);
    }

    #[test]
    fn every_exported_class_has_a_colour() {
        // The native-widget face of the GTK edge's no-unstyled-class guard: the
        // core exports the class NAMES, this crate owns the palette, and a state
        // that is derived perfectly but has no colour here would paint invisibly.
        // Driven from the core's aggregate over EVERY exported family
        // (`CssClassFamily::ALL`, kept complete by a compile-time check), never a
        // family list written out here: each family is already exhaustive over its
        // CLASSES, so a fifth trust posture or a sixth console level reds this
        // gate — and now a whole new FAMILY does too, instead of joining no gate
        // at all and painting invisibly (task
        // `one-derivation-close-the-aggregate-and-tooltip-gaps`).
        let mut checked = 0;
        for family in CssClassFamily::ALL {
            for class in family.classes().iter().copied() {
                assert!(
                    class_color(class).is_some(),
                    "the core exports `{class}` but this edge has no colour for it, so the state \
                     would render invisibly"
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            CLASS_COLORS.len(),
            "the palette must carry exactly the exported classes: no unstyled state, no dead entry"
        );
        // Teeth: a class the core does NOT export has no colour either, so the
        // assertion above is not vacuously true.
        assert!(class_color("trust-not-a-posture").is_none());
    }

    #[test]
    fn the_menu_comes_from_the_shared_core_not_a_platform_list() {
        // Acceptance: the ⋮ menu's items are the core's `BrowserMenu`, so every
        // native menu shows the SAME version line and Debug entry the GTK,
        // Android and iOS menus show — and a future core item needs no per-OS
        // change.
        let items = menu_items();
        let version = items
            .iter()
            .find(|i| i.id == MENU_ITEM_VERSION)
            .expect("a version entry");
        assert_eq!(version.label, format!("werust {}", werust_core::version()));
        assert!(
            !version.activatable,
            "the version line is rendered as a disabled info item"
        );
        let debug = items
            .iter()
            .find(|i| i.id == MENU_ITEM_DEBUG)
            .expect("a debug entry");
        assert_eq!(debug.label, "Debug");
        assert!(debug.activatable, "the Debug entry opens the debug view");
        assert_eq!(
            items.len(),
            BrowserMenu::new().items().len(),
            "every core item is offered to the menu, in order"
        );
    }

    #[test]
    fn debug_rows_are_the_cores_row_derivation() {
        // The second half of "paints, does not derive": the debug view's row text
        // and its level/trust classes are the core's, so the AppKit and Win32
        // Console and Network tabs read exactly like the GTK ones.
        let capture = DebugCapture::new();
        capture.push_console(
            ConsoleEntry::new(ConsoleLevel::Warn, "deprecated API")
                .with_source("https://x/app.js")
                .with_line(42),
        );
        capture.push_network(
            NetworkEntry::new("GET", "ipfs://bafy/pic.png")
                .with_status(200)
                .with_mime("image/png")
                .with_size(1536)
                .with_trust(TrustPosture::ContentVerified),
        );

        let refresh = console_refresh(&capture, 0, None);
        let TabUpdate::Rebuild(rows) = &refresh.update else {
            panic!("the first paint rebuilds: {:?}", refresh.update);
        };
        assert_eq!(rows[0].text, "[warn] deprecated API (https://x/app.js:42)");
        assert_eq!(rows[0].class, "debug-console-warn");
        assert_eq!(rows[0].color, class_color("debug-console-warn").unwrap());

        let refresh = network_refresh(&capture, 0, None);
        let TabUpdate::Rebuild(rows) = &refresh.update else {
            panic!("the first paint rebuilds: {:?}", refresh.update);
        };
        let row = &rows[0];
        assert_eq!(row.method, "GET");
        assert_eq!(row.status, "200");
        assert_eq!(row.mime, "image/png");
        assert_eq!(row.size, "1.5 KB");
        assert_eq!(row.url, "ipfs://bafy/pic.png");
        // The trust column speaks the CHROME's vocabulary (ADR-0006), never a
        // platform-local label, and wears the chrome's own class + colour.
        assert_eq!(row.trust, "✓ content-verified");
        assert_eq!(row.trust_class, "trust-verified");
        assert_eq!(row.trust_color, class_color("trust-verified").unwrap());
        assert!(TRUST_INDICATOR_CSS_CLASSES.contains(&row.trust_class));
    }

    /// Apply a console update exactly as a native row list does, so these
    /// assertions are about what the real view would show.
    fn apply(view: &mut Vec<String>, update: &TabUpdate<ConsoleRowPaint>) {
        match update {
            TabUpdate::Noop => {}
            TabUpdate::Rebuild(rows) => {
                view.clear();
                view.extend(rows.iter().map(|r| r.text.clone()));
            }
            TabUpdate::Append { drop, rows } => {
                view.drain(..*drop);
                view.extend(rows.iter().map(|r| r.text.clone()));
            }
        }
    }

    #[test]
    fn the_debug_refresh_is_incremental_and_survives_eviction_at_the_cap() {
        // The refresh drives the core's sequence-anchored `tail_plan`, so every
        // native view inherits the property the GTK view was fixed to have: at the
        // ring buffer's cap the store's LENGTH stops changing, and a
        // length-anchored view freezes on rows the store already evicted. Driven
        // against the REAL store, with no display.
        let capture = DebugCapture::new();
        let mut rendered = 0usize;
        let mut last: Option<u64> = None;
        let mut view: Vec<String> = Vec::new();

        // An idle tick over an empty store does nothing at all.
        assert_eq!(
            console_refresh(&capture, rendered, last).update,
            TabUpdate::Noop
        );

        for i in 0..MAX_CONSOLE_ENTRIES {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("m{i}")));
        }
        let refresh = console_refresh(&capture, rendered, last);
        apply(&mut view, &refresh.update);
        rendered = refresh.rendered_rows;
        last = refresh.last_sequence;
        assert_eq!(rendered, MAX_CONSOLE_ENTRIES);
        assert_eq!(view.len(), MAX_CONSOLE_ENTRIES);

        // The steady tick AT the cap: exactly ONE row is built, one is dropped,
        // and the view keeps mirroring the store.
        for i in 0..5 {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("new{i}")));
            let refresh = console_refresh(&capture, rendered, last);
            let TabUpdate::Append { drop, rows } = &refresh.update else {
                panic!("an at-cap tick appends: {:?}", refresh.update);
            };
            assert_eq!(*drop, 1, "the evicted row is dropped off the top");
            assert_eq!(rows.len(), 1, "only the new row is built, not all 300");
            apply(&mut view, &refresh.update);
            rendered = refresh.rendered_rows;
            last = refresh.last_sequence;
            assert_eq!(rendered, MAX_CONSOLE_ENTRIES, "the view stays at the cap");
            assert_eq!(view.len(), rendered);
            assert!(
                view.last().unwrap().contains(&format!("new{i}")),
                "the newest entry renders even at the cap"
            );
            assert!(
                view.first()
                    .unwrap()
                    .contains(&capture.console()[0].message),
                "the view's top row is the store's head, not an evicted row"
            );
        }

        // An idle tick after that is a genuine no-op (no rows rebuilt).
        assert_eq!(
            console_refresh(&capture, rendered, last).update,
            TabUpdate::Noop
        );

        // A CLEAR shrinks the store: the view rebuilds to empty.
        capture.clear();
        let refresh = console_refresh(&capture, rendered, last);
        apply(&mut view, &refresh.update);
        assert_eq!(refresh.rendered_rows, 0);
        assert!(view.is_empty());
    }

    /// A `Renderer` that records what was installed on it. Enough of the seam to
    /// drive [`install_debug_capture`] on the gate: the capture points go
    /// through the seam precisely so they can be asserted with no real webview.
    #[derive(Default)]
    struct RecordingRenderer {
        scripts: Vec<String>,
        handlers: Vec<(String, renderer::ScriptMessageHandler)>,
    }

    impl RecordingRenderer {
        /// Deliver a page message on `channel`, as WebKit's script-message
        /// bridge would.
        fn post(&mut self, channel: &str, body: &str) {
            for (name, handler) in &mut self.handlers {
                if name == channel {
                    handler(renderer::ScriptMessage {
                        handler: channel.into(),
                        body: body.into(),
                    });
                }
            }
        }
    }

    impl Renderer for RecordingRenderer {
        fn navigate(&mut self, _url: &str) -> Result<(), RendererError> {
            Ok(())
        }
        fn reload(&mut self) -> Result<(), RendererError> {
            Ok(())
        }
        fn stop(&mut self) {}
        fn load_state(&self) -> LoadState {
            LoadState::Idle
        }
        fn current_url(&self) -> Option<String> {
            None
        }
        fn poll_event(&mut self) -> Option<LoadEvent> {
            None
        }
        fn view_handle(&self) -> ViewHandle {
            ViewHandle(std::ptr::null_mut())
        }
        fn send_pointer(&mut self, _event: renderer::PointerEvent) {}
        fn send_key(&mut self, _event: renderer::KeyEvent) {}
        fn send_scroll(&mut self, _delta: renderer::ScrollDelta) {}
        fn set_focus(&mut self, _focused: bool) {}
        fn register_script_message_handler(
            &mut self,
            name: &str,
            handler: renderer::ScriptMessageHandler,
        ) {
            self.handlers.push((name.to_string(), handler));
        }
        fn inject_script(&mut self, script: &str) {
            self.scripts.push(script.to_string());
        }
        fn register_scheme_handler(&mut self, _scheme: &str, _handler: renderer::SchemeHandler) {}
    }

    #[test]
    fn the_capture_points_inject_the_shared_shims_and_route_to_the_shared_store() {
        // A native debug view is only as good as what feeds it. The capture
        // points must be the SHARED ones (the same shim strings iOS injects, from
        // one place in the core) on the DEDICATED capture channel — never the
        // provider's trust channel — and a captured message must land in the SAME
        // store the view renders.
        use werust_core::debug::{console_shim, network_shim};

        let capture = DebugCapture::new();
        let mut backend = RecordingRenderer::default();
        install_debug_capture(&mut backend, capture.clone());

        assert!(
            backend.scripts.iter().any(|s| *s == console_shim()),
            "the SHARED console shim must be injected, not a per-edge copy"
        );
        assert!(
            backend.scripts.iter().any(|s| *s == network_shim()),
            "the shared network shim must be injected (neither WKWebView nor a\n             WebResourceRequested-less WebView2 wiring gives a resource signal)"
        );
        assert_eq!(
            backend
                .handlers
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            vec![CAPTURE_BRIDGE]
        );
        assert_ne!(
            CAPTURE_BRIDGE,
            werust_core::provider::PROVIDER_BRIDGE,
            "capture never rides the EIP-1193 trust channel"
        );

        // A real captured console message reaches the store the view renders,
        // through the core's own parser (no per-edge parsing).
        backend.post(
            CAPTURE_BRIDGE,
            r#"{"kind":"console","level":"error","message":"boom","source":"ipfs://cid/a.js","line":3}"#,
        );
        let console = capture.console();
        assert_eq!(console.len(), 1);
        assert_eq!(console[0].message, "boom");
        assert_eq!(console[0].level, ConsoleLevel::Error);
        let rows = console_refresh(&capture, 0, None);
        let TabUpdate::Rebuild(rows) = rows.update else {
            panic!("the first paint rebuilds");
        };
        assert_eq!(rows[0].text, "[error] boom (ipfs://cid/a.js:3)");

        // A hostile / unreadable body is DROPPED, never fabricated into an entry.
        backend.post(CAPTURE_BRIDGE, "not json at all");
        assert_eq!(capture.console().len(), 1);
    }
}
