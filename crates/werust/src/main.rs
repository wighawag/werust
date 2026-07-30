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
//!
//! Neither are the chrome's DISPLAY RULES: the status line, the trust indicator
//! (+ its detail and CSS class), the error banner, the invalid-entry badge and
//! the URL bar's load progress are pure derivations of [`ChromeState`] that live
//! in `werust-core` beside it ([`status_line`], [`trust_indicator`],
//! [`error_banner_text`], … — task `desktop-chrome-presentation-into-core`,
//! `docs/adr/0011`). This file is a PAINTER: it calls them and sets widget
//! properties, so a second desktop window (Win32, AppKit) reuses the rules
//! instead of minting a fourth copy of them.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::pango::EllipsizeMode;
use gtk4::prelude::*;
use gtk4::{
    gdk, glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    ListBox, MenuButton, Notebook, Orientation, Popover, ScrolledWindow, Widget, Window,
};

use renderer::TrustPosture;
use webkit6::prelude::WebViewExt;
use webview_renderer::WebViewRenderer;
use werust_core::contenthash::DecodedContenthash;
use werust_core::debug::{
    trust_posture_wire_name, ConsoleEntry, ConsoleLevel, DebugCapture, NetworkEntry,
};
use werust_core::ens;
use werust_core::ethereum::RpcProvider;
use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG};
use werust_core::{
    error_banner_css_class, error_banner_text, error_banner_visible, invalid_entry_badge_text,
    invalid_entry_badge_visible, load_progress_fraction, load_progress_hint, load_progress_visible,
    status_line, trust_indicator, trust_indicator_css_class, trust_indicator_detail, BrowserShell,
    ChromeState,
};

/// The URL werust opens when none is given on the command line.
const DEFAULT_URL: &str = "https://example.com/";

/// The reverse-DNS stem of the GTK application id: werust's name, WITHOUT the
/// version element [`app_id`] appends.
const APP_ID_STEM: &str = "com.github.wighawag.werust";

/// The GTK application id for the shell window, VERSIONED with werust's own
/// version so two different releases never share one process.
///
/// A GTK [`Application`] with a fixed id is single-instance over D-Bus:
/// launching a second binary ACTIVATES the registered instance and hands the
/// session to it. With an unversioned id that hand-off crosses RELEASES, and the
/// old process answers with its OWN compiled-in behaviour: a different RPC
/// endpoint, older feature flags, every compile-time constant. That is a real
/// field failure: a user launched v0.2.9 (Infura), the running v0.2.8
/// (`1rpc.io/eth`, which blocks `eth_call`) took the window, and every `.eth`
/// site failed while the console said "werust 0.2.9" (task
/// `versioned-gtk-app-id-and-stale-process-detection`). Putting the version IN
/// the id means the two releases simply cannot address each other, so no IPC,
/// version handshake or auto-kill of the old process is needed.
///
/// The version is [`werust_core::version`], the SAME single, build-time-resolved
/// source the startup [`banner`] and the browser menu read, so the bus name can
/// never disagree with the version the user is shown.
///
/// Within ONE release the id is unchanged, so a second copy of the same binary
/// still activates the running window (the expected single-window behaviour).
///
/// # Why the version is not spliced in verbatim
///
/// An application id is a D-Bus well-known bus name: dot-separated elements of
/// `[A-Za-z0-9_-]`, none of which may begin with a digit. So the version's dots
/// become underscores (`0.2.9` -> `0_2_9`, which would otherwise add elements
/// like `2` that start with a digit) and the element is prefixed with `v`
/// (`0_2_9` starts with a digit too). Anything ELSE outside the allowed set is
/// folded to `_` as well, because the resolved version is not always a release
/// triple: a dev build is `git describe` output and an operator can inject an
/// arbitrary `WERUST_VERSION`. An invalid id is not a loud failure (GLib
/// rejects it and the application ends up non-unique), so the id is made valid
/// by construction rather than trusted to be well-shaped (that widening beyond
/// the dots, and the measured before/after D-Bus behaviour, are recorded in
/// `docs/spikes/versioned-gtk-app-id-and-stale-process-detection/README.md`).
fn app_id(version: &str) -> String {
    let mut element = String::with_capacity(version.len() + 1);
    element.push('v');
    element.extend(version.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' {
            c
        } else {
            '_'
        }
    }));
    format!("{APP_ID_STEM}.{element}")
}

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

/// The state the menu's Debug entry and the refresh pump share about the DEBUG
/// VIEW: the capture store the view renders, and the currently-open view, if
/// any. Grouped so the one [`open_debug_view`] hook and the pump see the SAME
/// fact (both clones of the store are the one `Arc` store the capture points
/// feed).
struct DebugViewState {
    capture: DebugCapture,
    open: RefCell<Option<Rc<RefCell<DebugView>>>>,
}

/// The OPEN-DEBUG-VIEW hook the browser menu's Debug entry calls: opens the
/// tabbed Console + Network debug view over the shared capture store. The menu
/// task (`general-browser-menu-with-version-and-debug-entry`) left this hook a
/// placeholder; THIS is the real view that fills it (task
/// `debug-view-console-network-tabs-desktop`).
///
/// The view is a SEPARATE window, transient for the browser window: an
/// in-window panel would crowd the page view on every open, and a window closes
/// with its own close button, so "toggled closed again" is the platform's own
/// affordance (recorded in
/// `docs/spikes/debug-view-console-network-tabs-desktop/DECISIONS.md`).
/// Re-activating Debug while the view is open PRESENTS (raises) it rather than
/// opening a second copy; closing the window drops the slot so the next
/// activation opens a fresh one.
fn open_debug_view(parent: &ApplicationWindow, state: &Rc<DebugViewState>) {
    if let Some(view) = state.open.borrow().as_ref() {
        view.borrow().window.present();
        return;
    }
    let view = DebugView::new(parent, state.capture.clone());
    view.borrow().window.connect_close_request({
        let state = state.clone();
        move |_| {
            // The view is gone: drop the slot so the next Debug activation
            // opens a fresh window rather than presenting a destroyed one.
            *state.open.borrow_mut() = None;
            glib::Propagation::Proceed
        }
    });
    // Paint the store captured so far BEFORE presenting, so the window never
    // opens visibly empty when there are already entries.
    view.borrow_mut().refresh();
    let window = view.borrow().window.clone();
    *state.open.borrow_mut() = Some(view);
    window.present();
}

/// The debug view itself: a separate window with a CLEAR action over a
/// `Notebook` of the CONSOLE and NETWORK tabs, each a scrollable list rendered
/// from the shared capture store ([`DebugCapture`]). READ-ONLY by construction:
/// every row is a non-editable label (a typeable REPL is the native F12 WebKit
/// inspector's job, out of scope here).
///
/// The view is refreshed on the EXISTING chrome pump cadence (the 50ms timeout
/// in [`open_window`]), never a busy loop: the refresh is INCREMENTAL, appending
/// only the rows captured since the last tick and DROPPING from the top the
/// rows the ring buffer evicted from the store's front, so an idle tick is one
/// sequence comparison. The anchor is the store's MONOTONIC per-entry
/// [`sequence`](ConsoleEntry::sequence), NOT the store's length: a ring buffer
/// AT its cap never changes length (every push is paired with a `pop_front`
/// eviction), so a length-anchored refresh silently freezes exactly in the
/// long-session case the ring buffer exists for (the defect Gate-2 caught;
/// recorded in the task's DECISIONS.md, Decision 2). Rows are newest-at-BOTTOM
/// with auto-scroll that sticks only when the user is already at the bottom
/// (the devtools idiom; Decision 3).
struct DebugView {
    window: Window,
    capture: DebugCapture,
    console_list: ListBox,
    network_list: ListBox,
    console_scrolled: ScrolledWindow,
    network_scrolled: ScrolledWindow,
    /// The sequence of the last store entry each tab has rendered (the refresh
    /// anchor), or `None` before the first paint / after a clear.
    last_console_sequence: Option<u64>,
    last_network_sequence: Option<u64>,
    /// How many rows each tab's list currently holds. Tracked alongside the
    /// anchor so an at-cap append can DROP the rows the ring buffer evicted
    /// from the store's front (the view's rows end at the anchor, so the rows
    /// above the anchor's snapshot position are the evicted ones); without it
    /// the append path only ever grew the list past the cap (the round-2
    /// Gate-2 defect).
    console_rows: usize,
    network_rows: usize,
}

impl DebugView {
    /// Build the debug-view window: a header row (a title + the CLEAR action)
    /// over the two-tab `Notebook`. The parent is any window (the browser's
    /// `ApplicationWindow` in production), so the display-requiring end-to-end
    /// test can parent it on a plain `Window`.
    fn new(parent: &impl IsA<Window>, capture: DebugCapture) -> Rc<RefCell<Self>> {
        let console_list = ListBox::new();
        console_list.set_selection_mode(gtk4::SelectionMode::None);
        let console_scrolled = ScrolledWindow::builder()
            .child(&console_list)
            .vexpand(true)
            .hexpand(true)
            .build();

        let network_list = ListBox::new();
        network_list.set_selection_mode(gtk4::SelectionMode::None);
        let network_scrolled = ScrolledWindow::builder()
            .child(&network_list)
            .vexpand(true)
            .hexpand(true)
            .build();

        let notebook = Notebook::new();
        notebook.append_page(&console_scrolled, Some(&Label::new(Some("Console"))));
        notebook.append_page(&network_scrolled, Some(&Label::new(Some("Network"))));
        notebook.set_vexpand(true);

        // The CLEAR action empties the shared store (`DebugCapture::clear`, BOTH
        // buffers); the refresh below then resets both lists.
        let clear = Button::with_label("Clear");
        clear.set_tooltip_text(Some("Clear the captured console + network entries"));
        let title = Label::builder()
            .label("Console + Network capture")
            .xalign(0.0)
            .hexpand(true)
            .build();
        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.append(&title);
        header.append(&clear);

        let root = GtkBox::new(Orientation::Vertical, 6);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.append(&header);
        root.append(&notebook);

        let window = Window::builder()
            .transient_for(parent)
            .title("werust Debug")
            .default_width(760)
            .default_height(480)
            .child(&root)
            .build();

        let view = Rc::new(RefCell::new(Self {
            window,
            capture,
            console_list,
            network_list,
            console_scrolled,
            network_scrolled,
            last_console_sequence: None,
            last_network_sequence: None,
            console_rows: 0,
            network_rows: 0,
        }));

        clear.connect_clicked({
            let view = view.clone();
            move |_| {
                let mut view = view.borrow_mut();
                view.capture.clear();
                view.refresh();
            }
        });

        view
    }

    /// Catch the view up with the store: append the rows captured since the last
    /// refresh, DROPPING from the top the rows the ring buffer evicted from the
    /// store's front, or REBUILD a list when the refresh anchor is gone (a
    /// `clear`, or the ring buffer evicting past the last-rendered entry at the
    /// cap). Called on the existing pump cadence while the view is open.
    fn refresh(&mut self) {
        let console = self.capture.console();
        let sequences: Vec<u64> = console.iter().map(ConsoleEntry::sequence).collect();
        match tail_plan(&sequences, self.console_rows, self.last_console_sequence) {
            TailPlan::Rebuild => {
                clear_list_box(&self.console_list);
                let stick = is_at_bottom(&self.console_scrolled);
                for entry in &console {
                    self.console_list.append(&console_row(entry));
                }
                stick_to_bottom(&self.console_scrolled, stick);
            }
            TailPlan::AppendFrom { drop, from } => {
                let stick = is_at_bottom(&self.console_scrolled);
                drop_top_rows(&self.console_list, drop);
                for entry in &console[from..] {
                    self.console_list.append(&console_row(entry));
                }
                stick_to_bottom(&self.console_scrolled, stick);
            }
            TailPlan::Noop => {}
        }
        self.last_console_sequence = sequences.last().copied();
        self.console_rows = sequences.len();

        let network = self.capture.network();
        let sequences: Vec<u64> = network.iter().map(NetworkEntry::sequence).collect();
        match tail_plan(&sequences, self.network_rows, self.last_network_sequence) {
            TailPlan::Rebuild => {
                clear_list_box(&self.network_list);
                let stick = is_at_bottom(&self.network_scrolled);
                for entry in &network {
                    self.network_list.append(&network_row(entry));
                }
                stick_to_bottom(&self.network_scrolled, stick);
            }
            TailPlan::AppendFrom { drop, from } => {
                let stick = is_at_bottom(&self.network_scrolled);
                drop_top_rows(&self.network_list, drop);
                for entry in &network[from..] {
                    self.network_list.append(&network_row(entry));
                }
                stick_to_bottom(&self.network_scrolled, stick);
            }
            TailPlan::Noop => {}
        }
        self.last_network_sequence = sequences.last().copied();
        self.network_rows = sequences.len();
    }
}

/// What one debug-view tab must do to catch up with a store snapshot. Pure, so
/// the eviction-at-the-cap behaviour (the Gate-2 defect) is pinned
/// display-free; the GTK application of it is one `match` in
/// [`DebugView::refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TailPlan {
    /// Rebuild the whole list from the snapshot: the first paint, a `clear`
    /// (the snapshot is shorter than what the view rendered), or the ring
    /// buffer having evicted past the last-rendered entry, so every row the
    /// view holds is stale.
    Rebuild,
    /// Append only the snapshot entries from `from` onward (the tail AFTER the
    /// last-rendered entry), first DROPPING `drop` rows from the view's top:
    /// the rows the ring buffer has already evicted from the store's front.
    /// The drop is EXPLICIT, not implicit: appending alone leaves the view
    /// holding rows the store discarded, its count climbing past the cap until
    /// the anchor itself is evicted (the round-2 Gate-2 defect). After the
    /// drop + append the view's rows match the snapshot exactly.
    AppendFrom { drop: usize, from: usize },
    /// Nothing new to render (the steady idle tick).
    Noop,
}

/// Plan one tab's refresh from the snapshot's entry SEQUENCES (oldest first,
/// strictly increasing), how many rows the view currently holds, and the
/// sequence of the last entry the view rendered.
///
/// The anchor is the sequence, never the length, because a ring buffer AT its
/// cap never changes length: `pop_front` eviction keeps it pinned, so "same
/// length" means both "nothing new" AND "N new, N evicted". The sequence tells
/// those apart: if the anchor still falls inside the snapshot, exactly the
/// entries after it are new; if it is ABSENT, everything the view holds was
/// evicted (or the store was cleared) and only a rebuild is honest.
///
/// `rendered_rows` makes the eviction REMOVAL explicit: the view's rows end at
/// the anchor, so of them only the snapshot rows up to and including the
/// anchor's position are still in the store; the rest were evicted from the
/// store's front and drop off the view's TOP on the append (zero while the
/// buffer sits below its cap).
fn tail_plan(sequences: &[u64], rendered_rows: usize, last_rendered: Option<u64>) -> TailPlan {
    let Some(anchor) = last_rendered else {
        // Nothing rendered yet (the first paint, or just after a clear): an
        // empty snapshot is a no-op, a non-empty one renders everything.
        return if sequences.is_empty() {
            TailPlan::Noop
        } else {
            TailPlan::Rebuild
        };
    };
    match sequences.iter().position(|&s| s == anchor) {
        Some(index) if index + 1 < sequences.len() => TailPlan::AppendFrom {
            drop: rendered_rows.saturating_sub(index + 1),
            from: index + 1,
        },
        Some(_) => TailPlan::Noop,
        None => TailPlan::Rebuild,
    }
}

/// Remove every row of a [`ListBox`] (after the store's `clear()` shrank it).
fn clear_list_box(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

/// Remove the FIRST `n` rows of a [`ListBox`]: the rows the ring buffer
/// evicted from the store's front, dropped off the view's TOP on an at-cap
/// append so the list keeps mirroring the store instead of climbing past the
/// cap on rows the store already discarded.
fn drop_top_rows(list: &ListBox, n: usize) {
    for _ in 0..n {
        let Some(child) = list.first_child() else {
            break;
        };
        list.remove(&child);
    }
}

/// Whether the scrolled list is already at the bottom (so newly appended rows
/// should auto-scroll into view, the devtools-console idiom).
fn is_at_bottom(scrolled: &ScrolledWindow) -> bool {
    let adj = scrolled.vadjustment();
    adj.value() >= adj.upper() - adj.page_size() - 1.0
}

/// Auto-scroll a list to the bottom after new rows were appended, but ONLY when
/// it was already at the bottom (`stick`): a user scrolled up reading an earlier
/// entry is never yanked back down. Deferred to idle, because the adjustment's
/// upper bound updates only after the new rows are laid out.
fn stick_to_bottom(scrolled: &ScrolledWindow, stick: bool) {
    if stick {
        let scrolled = scrolled.clone();
        glib::idle_add_local_once(move || {
            let adj = scrolled.vadjustment();
            adj.set_value(adj.upper() - adj.page_size());
        });
    }
}

/// One CONSOLE row: a single-line label `[<level>] <message> (<source>:<line>)`,
/// carrying the `debug-console-<level>` class ([`APP_CSS`]) so an error reads
/// red and a warning amber at a glance.
fn console_row(entry: &ConsoleEntry) -> Label {
    let label = single_line(&console_row_text(entry));
    label.add_css_class(console_level_css_class(entry.level));
    label
}

/// One NETWORK row: a horizontal strip of single-line columns (method, status,
/// MIME, size, the honest per-request trust posture, the URL) so the columns
/// stay legible as the list grows. The trust column carries the SAME glyph,
/// wire name and CSS class the chrome trust indicator uses (ADR-0006).
fn network_row(entry: &NetworkEntry) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 10);
    for text in [
        entry.method.clone(),
        network_status_text(entry.status),
        network_mime_text(&entry.mime),
        network_size_text(entry.size),
    ] {
        row.append(&single_line(&text));
    }
    let trust = single_line(&network_trust_label(entry.trust));
    trust.add_css_class(network_trust_css_class(entry.trust));
    row.append(&trust);
    // The URL is the long, unbounded column: it takes the slack and ellipsizes
    // in the MIDDLE so both the scheme and the tail stay visible.
    let url = single_line(&entry.url);
    url.set_hexpand(true);
    url.set_ellipsize(EllipsizeMode::Middle);
    row.append(&url);
    row
}

/// A single-line, selectable, end-ellipsized label aligned left: the building
/// block of every debug-view row (READ-ONLY: labels, never entries).
fn single_line(text: &str) -> Label {
    Label::builder()
        .label(text)
        .xalign(0.0)
        .single_line_mode(true)
        .ellipsize(EllipsizeMode::End)
        .selectable(true)
        .build()
}

/// The CSS class colouring one console row by its level: error red, warn amber,
/// info blue, debug grey, log neutral. Pure, so the level-to-class mapping is
/// pinned without a display.
fn console_level_css_class(level: ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Log => "debug-console-log",
        ConsoleLevel::Info => "debug-console-info",
        ConsoleLevel::Warn => "debug-console-warn",
        ConsoleLevel::Error => "debug-console-error",
        ConsoleLevel::Debug => "debug-console-debug",
    }
}

/// The `<source>:<line>` tail of a console row: empty when the platform
/// reported no source (the injected shim reports none for an unreadable stack
/// frame), source-only when it reported no line. An absent field stays honestly
/// absent rather than rendering a fabricated `:0`. Pure, for display-free tests.
fn console_source_line(entry: &ConsoleEntry) -> String {
    match (entry.source.is_empty(), entry.line) {
        (true, _) => String::new(),
        (false, Some(line)) => format!("{}:{line}", entry.source),
        (false, None) => entry.source.clone(),
    }
}

/// The full text of one console row: `[<level>] <message>` plus the source tail
/// in parentheses when there is one. The level tag is the store's OWN wire name
/// (`log`/`info`/`warn`/`error`/`debug`), so the Console tab speaks the capture
/// store's vocabulary exactly. Pure, for display-free tests.
fn console_row_text(entry: &ConsoleEntry) -> String {
    let source = console_source_line(entry);
    if source.is_empty() {
        format!("[{}] {}", entry.level.wire_name(), entry.message)
    } else {
        format!("[{}] {} ({source})", entry.level.wire_name(), entry.message)
    }
}

/// The status column of a network row: the response code, or `?` when the
/// request has no status (a custom scheme answered without one, or the request
/// failed before a response): an unknown stays honestly unknown.
fn network_status_text(status: Option<u16>) -> String {
    status.map_or_else(|| "?".to_string(), |s| s.to_string())
}

/// The MIME column of a network row, or `?` when unknown.
fn network_mime_text(mime: &str) -> String {
    if mime.is_empty() {
        "?".to_string()
    } else {
        mime.to_string()
    }
}

/// The size column of a network row: a human byte count (`512 B`, `1.5 KB`,
/// `2.0 MB`), or `?` when unknown.
fn network_size_text(size: Option<u64>) -> String {
    match size {
        None => "?".to_string(),
        Some(bytes) if bytes < 1024 => format!("{bytes} B"),
        Some(bytes) if bytes < 1024 * 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
        Some(bytes) => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

/// The per-request trust label of a network row: the SAME posture vocabulary
/// the chrome trust indicator speaks (ADR-0006) — the indicator's glyph for the
/// posture plus the core's wire name (`content-verified`, `unverified-origin`,
/// `name-via-trusted-rpc`, `mutable-name`), never a new label minted for the
/// debug view. Pure, for display-free tests.
fn network_trust_label(posture: TrustPosture) -> String {
    let glyph = match posture {
        TrustPosture::ContentVerified => "✓",
        TrustPosture::NameViaTrustedRpc => "◈",
        TrustPosture::MutableName => "◇",
        TrustPosture::UnverifiedOrigin => "⚠",
    };
    format!("{glyph} {}", trust_posture_wire_name(posture))
}

/// The CSS class colouring a network row's trust column: one of the SAME
/// `trust-*` classes the chrome trust indicator toggles, so a content-verified
/// request is the same green the indicator's verified badge is. Pure, for
/// display-free tests.
fn network_trust_css_class(posture: TrustPosture) -> &'static str {
    match posture {
        TrustPosture::ContentVerified => "trust-verified",
        TrustPosture::NameViaTrustedRpc => "trust-name-trusted-rpc",
        TrustPosture::MutableName => "trust-mutable-name",
        TrustPosture::UnverifiedOrigin => "trust-unverified",
    }
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
fn build_menu_button(window: &ApplicationWindow, debug_view: &Rc<DebugViewState>) -> MenuButton {
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
                let debug_view = debug_view.clone();
                button.connect_clicked(move |button| {
                    // Close the popover first, so the menu does not sit over
                    // whatever the entry opens.
                    if let Some(popover) = button.ancestor(Popover::static_type()) {
                        if let Ok(popover) = popover.downcast::<Popover>() {
                            popover.popdown();
                        }
                    }
                    if id == MENU_ITEM_DEBUG {
                        open_debug_view(&window, &debug_view);
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

/// What the binary was asked to do, parsed from argv by [`parse_args`].
///
/// werust is VERB-FIRST like `git`/`cargo` (task `headless-cli-mode`): the first
/// argument selects a headless subcommand, and ANYTHING else keeps the original
/// meaning — the GUI, on the URL given (or [`DEFAULT_URL`]). Modelling the
/// decision as a plain value keeps the whole dispatch a PURE function of argv, so
/// the routing is unit-testable with no display, no network and no process spawn;
/// [`main`] is then just "parse, then run one arm".
///
/// The judgement calls this dispatch bakes in (what an `ipns-ns` name prints, why
/// the banner moved, why a malformed KNOWN verb refuses while an unknown first
/// argument still opens the GUI) are recorded in
/// `docs/spikes/headless-cli-mode/DECISIONS.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    /// The DEFAULT: open the browser window on this URL (the pre-CLI behaviour —
    /// `werust` with no arguments, or `werust <url>`).
    Gui {
        /// The startup URL: argv[1] when given, else [`DEFAULT_URL`].
        url: String,
    },
    /// `werust resolve <name> [--json]`: resolve an ENS name headlessly and print
    /// the contenthash reference it points at.
    Resolve {
        /// The ENS name to resolve, exactly as typed (the ENS core normalizes it,
        /// and refuses an unnormalizable name fail-closed).
        name: String,
        /// Print the one-object machine-readable form instead of the bare
        /// reference line.
        json: bool,
    },
    /// `werust version` (also `--version` / `-V`): print the [`banner`] and exit.
    Version,
    /// `werust --help` / `-h`: print [`usage`] and exit 0.
    Help,
    /// A MALFORMED invocation of a known subcommand (a missing name, a second
    /// positional, an unknown flag). Carries the specific reason; [`main`] prints
    /// it plus [`usage`] to stderr and exits 1. Distinct from [`Gui`](Command::Gui):
    /// once a known verb is named, werust refuses rather than silently opening a
    /// window on something the user did not mean.
    Usage(String),
}

/// The usage message `werust --help` prints: the subcommands plus the GUI default.
///
/// Hand-written rather than generated by an argument-parsing crate — the whole
/// CLI is one verb-first dispatch, so a dependency (clap) would be pure weight
/// (task `headless-cli-mode`). Pure, so the acceptance criterion "lists the
/// available subcommands (and the GUI default)" is pinned by a test.
fn usage() -> String {
    format!(
        "{}\n\n\
         Usage:\n\
         \x20 werust                      open the browser GUI on {DEFAULT_URL}\n\
         \x20 werust <url>                open the browser GUI on <url>\n\
         \x20 werust resolve <ens-name>   resolve an ENS name to its contenthash reference\n\
         \x20 werust resolve --json <n>   the same, as one JSON object\n\
         \x20 werust version              print the version banner (also --version, -V)\n\
         \x20 werust --help               print this message (also -h)\n\
         \n\
         The ENS read goes through the RPC endpoint WERUST_RPC_URL names, else the\n\
         compiled-in default (the same endpoint the GUI uses).",
        banner()
    )
}

/// Route an argv TAIL (argv[0] already dropped) onto the [`Command`] to run.
///
/// Deliberately minimal and NON-greedy: only the named verbs/flags are taken, so
/// every invocation that opened the GUI before this dispatch existed still opens
/// it (an unknown first argument is a URL, exactly as `env::args().nth(1)` treated
/// it). `--json` is accepted on either side of the name because a hand-rolled
/// flag has no reason to be positional.
fn parse_args(args: impl IntoIterator<Item = String>) -> Command {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Command::Gui {
            url: DEFAULT_URL.into(),
        };
    };
    match first.as_str() {
        "--help" | "-h" => Command::Help,
        // `version` as a verb is the discoverable spelling; the flag spellings are
        // what a user reaching for a version reflexively types (and neither was a
        // meaningful URL before).
        "version" | "--version" | "-V" => Command::Version,
        "resolve" => {
            let mut name: Option<String> = None;
            let mut json = false;
            for arg in args {
                match arg.as_str() {
                    "--json" => json = true,
                    flag if flag.starts_with('-') => {
                        return Command::Usage(format!("unknown flag for `resolve`: {flag}"));
                    }
                    _ if name.is_some() => {
                        return Command::Usage(format!(
                            "`resolve` takes one ENS name, got a second argument: {arg}"
                        ));
                    }
                    _ => name = Some(arg),
                }
            }
            match name {
                Some(name) => Command::Resolve { name, json },
                None => Command::Usage("`resolve` needs an ENS name".into()),
            }
        }
        _ => Command::Gui { url: first },
    }
}

/// Format the OUTPUT of a headless `resolve` for a decoded contenthash: the
/// reference line (or its one-object JSON form), or the protocol-named REFUSAL
/// for a contenthash werust cannot express as a reference.
///
/// The success form is a single BARE reference line so `$(werust resolve …)` is
/// directly usable in a script, and the `--json` form carries the same facts as
/// one flat object (`name`, `kind`, `reference`) hand-rolled with `format!` — no
/// serde in the binary (task `headless-cli-mode`).
///
/// The two loadable contenthash kinds are reported DISTINCTLY, never flattened
/// into one "ipfs" answer: an `ipfs-ns` name yields its immutable `ipfs://<cid>`,
/// and a MUTABLE `ipns-ns` name yields the `ipns://<name>` pointer it really is
/// (`docs/adr/0006`/`0007`: the mutable name is its own honest posture). This
/// subcommand does NOT follow that pointer to its current CID — that is the IPNS
/// record fetch + verify step, which is content retrieval (the out-of-scope
/// `fetch` subcommand), not the ENS read this verb performs. A later `--follow`
/// (or `fetch`) can add it without changing what `resolve` means.
///
/// [`Err`] is the fail-closed arm: a well-formed contenthash for an unsupported
/// protocol is the DECODER's own named reason, printed to stderr with exit 1 —
/// never printed as if it were a loadable reference. (`ens::resolve` already maps
/// that case to `Err(UnsupportedContenthash)`, so it does not normally arrive
/// here; dispatching on the decoded kind's OWN shape means a contract change
/// cannot turn it into fake output.)
fn resolve_output(name: &str, decoded: &DecodedContenthash, json: bool) -> Result<String, String> {
    let (kind, reference) = match decoded {
        DecodedContenthash::Ipfs { uri, .. } => ("ipfs", uri.clone()),
        DecodedContenthash::Ipns { name } => ("ipns", format!("ipns://{name}")),
        other => {
            return Err(other
                .reason()
                .unwrap_or_else(|| "unsupported contenthash protocol".to_string()));
        }
    };
    if json {
        Ok(format!(
            "{{\"name\":\"{}\",\"kind\":\"{kind}\",\"reference\":\"{}\"}}",
            json_escape(name),
            json_escape(&reference)
        ))
    } else {
        Ok(reference)
    }
}

/// Escape `text` for embedding in a JSON string literal (the `--json` output).
///
/// Hand-rolled because the binary pulls in NO serde for one flat object (task
/// `headless-cli-mode`), but escaped rather than interpolated raw: the name comes
/// straight from argv, so a quote or backslash in it would otherwise emit a
/// broken object a consumer cannot parse.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Run the headless `resolve` subcommand: resolve `name` through the ENS core and
/// print [`resolve_output`], with a Unix exit status.
///
/// NO GTK is touched on this path — no [`Application`], no window, not even
/// `gtk::init` — so it runs over ssh, in CI and in any environment with no
/// display. The provider is [`RpcProvider::new`], the SAME endpoint source the GUI
/// shell builds (the `WERUST_RPC_URL` env lever, else the compiled default), so a
/// CLI resolution and the browser's own address-bar resolution can never disagree
/// about which chain they read.
///
/// A failure prints the core's OWN typed reason to stderr (`werust: {e}`, the
/// formatting the GUI surfaces too) and exits 1, so a script can branch on the
/// status instead of parsing stdout.
fn run_resolve(name: &str, json: bool) -> glib::ExitCode {
    let provider = RpcProvider::new();
    match ens::resolve(&provider, name) {
        Ok(decoded) => match resolve_output(name, &decoded, json) {
            Ok(line) => {
                println!("{line}");
                glib::ExitCode::SUCCESS
            }
            Err(reason) => {
                eprintln!("werust: {reason}");
                glib::ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("werust: {e}");
            glib::ExitCode::FAILURE
        }
    }
}

fn main() -> glib::ExitCode {
    // Verb-first dispatch BEFORE any GTK setup (task `headless-cli-mode`): a known
    // subcommand runs headlessly and exits; everything else falls through to the
    // GUI below, unchanged.
    let url = match parse_args(std::env::args().skip(1)) {
        Command::Help => {
            println!("{}", usage());
            return glib::ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("{}", banner());
            return glib::ExitCode::SUCCESS;
        }
        Command::Usage(reason) => {
            eprintln!("werust: {reason}\n\n{}", usage());
            return glib::ExitCode::FAILURE;
        }
        Command::Resolve { name, json } => return run_resolve(&name, json),
        Command::Gui { url } => url,
    };

    // The startup banner belongs to the GUI launch only: a headless subcommand's
    // stdout is its RESULT (a bare reference line, or a JSON object), and a banner
    // in front of it would make `--json` unparseable and `$(werust resolve …)`
    // wrong. `werust version` prints the same banner explicitly.
    println!("{}", banner());

    // Versioned (see `app_id`): a NEW release must open its own window with its
    // own compiled code, never be handed off to a still-running OLD release.
    let app = Application::builder()
        .application_id(app_id(werust_core::version()))
        .build();
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
    /// The URL bar. It doubles as the LOAD-PROGRESS surface: while a load is in
    /// flight its built-in progress fraction is painted from
    /// [`load_progress_fraction`], so the load state is visible in the chrome
    /// WITHOUT any widget taking height from the page view (task
    /// `loading-progress-in-the-url-bar-not-a-banner`).
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
    ///
    /// Every string / fraction / class name painted here is decided by the shared
    /// derivation in `werust-core` (`status_line`, `trust_indicator*`,
    /// `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`); this method
    /// only assigns them to widgets.
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
        // The LOAD-PROGRESS indicator lives IN the URL bar: the entry's own
        // progress fraction advances with the real pipeline phase and falls to
        // zero (painting nothing) once the load settles. Unlike the banner it
        // replaces, this changes NO widget geometry, so a navigation never resizes
        // the page view and the content cannot jump under the pointer (task
        // `loading-progress-in-the-url-bar-not-a-banner`). Cancelling is the
        // toolbar Stop button, which is already sensitive exactly while a load is
        // in flight — the banner's Cancel was a second affordance for the same
        // `BrowserShell::stop`, so nothing is lost. Driven by this existing
        // refresh, so no new timer / poll / tight loop (the Android ANR guard is
        // not regressed).
        self.url_entry
            .set_progress_fraction(load_progress_fraction(state));
        // The phase NAME rides along as the URL bar's tooltip (plus the footer
        // status line, which already names it), so the bar says WHICH phase is
        // slow without stealing a fixed slot from the toolbar. Cleared when
        // nothing is in flight, so a stale phase never lingers on hover. The
        // cancel hint is added ONLY while the backend load is in flight, which is
        // exactly when Stop is sensitive: during the PRE-CONTENT resolution window
        // there is no backend load to stop, so promising a cancel there would lie.
        let phase_tooltip = load_progress_visible(state).then(|| {
            let hint = load_progress_hint(state);
            if state.is_loading() {
                format!("{hint}… — press Stop (✕) to cancel")
            } else {
                format!("{hint}…")
            }
        });
        self.url_entry.set_tooltip_text(phase_tooltip.as_deref());
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

/// The app stylesheet: the classes that make the trust-indicator states
/// visually distinct (a NEUTRAL grey loading badge shown while a load is in
/// flight, a green content-verified badge, a blue name-via-trusted-RPC badge, a
/// purple mutable-name badge, an amber unverified-origin one), plus the error
/// banner, the invalid-URL badge, the URL bar's own load-progress bar (the
/// `entry > progress` node, painted from the live pipeline phase), the menu info
/// item, and the debug view's
/// level-coloured console rows. Kept as one constant next to the classes the
/// chrome and the debug view toggle (`trust-loading` / `trust-verified` /
/// `trust-name-trusted-rpc` / `trust-mutable-name` / `trust-unverified`,
/// `debug-console-*`). The debug view's Network tab REUSES the `trust-*`
/// classes for its per-request trust column, so a content-verified request is
/// the same green the indicator's verified badge is (ADR-0006, one vocabulary).
const APP_CSS: &str = "\
.trust-loading { color: #5c5c5c; font-weight: bold; padding: 0 6px; }\
.trust-verified { color: #0a7d28; font-weight: bold; padding: 0 6px; }\
.trust-name-trusted-rpc { color: #1a5fb4; font-weight: bold; padding: 0 6px; }\
.trust-mutable-name { color: #6c3fb4; font-weight: bold; padding: 0 6px; }\
.trust-unverified { color: #9a6a00; font-weight: bold; padding: 0 6px; }\
.error-banner { background-color: #c01c28; color: #ffffff; font-weight: bold; padding: 10px 12px; }\
.error-banner-transient { background-color: #b5820a; color: #ffffff; font-weight: bold; padding: 10px 12px; }\
.invalid-url-badge { color: #c01c28; font-weight: bold; padding: 0 6px; }\
entry > progress { background-color: #1a5fb4; min-height: 3px; }\
.menu-info-item { padding: 4px 8px; }\
.url-invalid { color: #c01c28; text-decoration: underline; text-decoration-color: #c01c28; }\
.debug-console-log { padding: 2px 6px; }\
.debug-console-info { color: #1a5fb4; padding: 2px 6px; }\
.debug-console-warn { color: #9a6a00; font-weight: bold; padding: 2px 6px; }\
.debug-console-error { color: #c01c28; font-weight: bold; padding: 2px 6px; }\
.debug-console-debug { color: #5c5c5c; padding: 2px 6px; }";

/// Load the app stylesheet onto the default display, so the `trust-*` classes
/// the chrome toggles and the `debug-console-*` classes the debug view toggles
/// render as visually distinct states. A no-op if there is no display.
fn install_app_css() {
    let provider = CssProvider::new();
    provider.load_from_string(APP_CSS);
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
            .with_debug_capture(debug_capture.clone()),
    ));

    // The DEBUG-VIEW state the menu's Debug entry (which opens the view) and the
    // refresh pump (which keeps it live) share: the SAME capture store the
    // capture hooks feed (another clone of the one `Arc` handle), and the slot
    // for the currently-open debug view, if any (task
    // `debug-view-console-network-tabs-desktop`).
    let debug_view = Rc::new(DebugViewState {
        capture: debug_capture,
        open: RefCell::new(None),
    });

    // Load the app stylesheet once for the display so the `trust-*` classes the
    // chrome toggles and the `debug-console-*` classes the debug view toggles
    // are styled.
    install_app_css();

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

    // The LOAD-PROGRESS indicator is the URL bar itself: `Chrome::refresh` paints
    // the entry's own progress fraction from the live pipeline phase, so an
    // in-flight load is visible in the chrome without ANY widget taking height
    // from the page view (task `loading-progress-in-the-url-bar-not-a-banner`).
    // Nothing to construct here: the surface is `url_entry` above, styled by the
    // `entry > progress` rule in `APP_CSS`, and CANCEL is the toolbar Stop button
    // (sensitive exactly while a load is in flight).

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
    // The error banner sits directly under the toolbar and ABOVE the page view, so
    // a failed load's reason is unmissable in the content area rather than buried
    // in the footer status line. A FAILURE is the only load state allowed to
    // displace the page: there is nothing rendered to displace, and the user must
    // act. In-flight progress deliberately does NOT live here — it is painted
    // inside the URL bar, where it cannot resize the page on every navigation
    // (task `loading-progress-in-the-url-bar-not-a-banner`).
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
    toolbar.append(&build_menu_button(&window, &debug_view));

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
        let debug_view = debug_view.clone();
        move || {
            if shell.borrow_mut().pump() {
                refresh();
            }
            // Refresh the OPEN debug view on the SAME existing cadence (no new
            // timer, no busy loop): the capture store changes off the seam's
            // load events (console messages, resource loads), so `pump()`
            // returning false does not mean the store is unchanged. The refresh
            // is incremental (it appends only the rows captured since the last
            // tick, anchored on the store's monotonic entry sequence so
            // ring-buffer eviction at the cap cannot freeze it), so an idle
            // tick over an open view is one sequence comparison.
            if let Some(view) = debug_view.open.borrow().as_ref() {
                view.borrow_mut().refresh();
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
        app_id, banner, console_level_css_class, console_row_text, console_source_line,
        network_mime_text, network_size_text, network_status_text, network_trust_css_class,
        network_trust_label, parse_args, resolve_output, should_open_web_inspector, tail_plan,
        usage, Command, TailPlan, DEFAULT_URL,
    };
    use gtk4::prelude::*;
    use gtk4::{gdk, gio, Label};
    use renderer::TrustPosture;
    use std::cell::RefCell;
    use std::rc::Rc;
    use werust_core::contenthash::{DecodedContenthash, ProtoCode};
    use werust_core::debug::{
        trust_posture_wire_name, ConsoleEntry, ConsoleLevel, DebugCapture, NetworkEntry,
    };
    use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG, MENU_ITEM_VERSION};

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
    fn banner_names_werust() {
        assert!(banner().starts_with("werust "));
    }

    #[test]
    fn the_application_id_carries_the_version_so_two_releases_never_share_a_process() {
        // Acceptance (task `versioned-gtk-app-id-and-stale-process-detection`):
        // the GTK application id embeds the MARKETING version, dots replaced by
        // underscores under a `v` element. A v0.2.9 binary launched while v0.2.8
        // is running therefore addresses a DIFFERENT bus name, so GTK cannot
        // hand the new session to the old process (which carries the old
        // compile-time RPC endpoint and every other stale constant).
        assert_eq!(app_id("0.2.9"), "com.github.wighawag.werust.v0_2_9");
        assert_eq!(app_id("0.2.8"), "com.github.wighawag.werust.v0_2_8");
        assert_ne!(
            app_id("0.2.9"),
            app_id("0.2.8"),
            "different releases must not share a bus name"
        );
        // Intra-version single-instance is PRESERVED: the id is a pure function
        // of the version, so a second copy of the SAME release still activates
        // the running window rather than opening a second one.
        assert_eq!(app_id("0.2.9"), app_id("0.2.9"));
        // Every release stays under werust's one reverse-DNS name, so the ids
        // remain recognisable (and `pkill -f werust.v0_2_8` still finds one).
        assert!(app_id("0.2.9").starts_with("com.github.wighawag.werust."));
    }

    #[test]
    fn the_application_id_is_built_from_the_one_shared_version_and_is_always_valid() {
        // Acceptance: the id uses the SAME `werust_core::version()` the banner
        // and the menu read (no second version source to drift), and whatever
        // shape that build-time-resolved version takes, the result is a VALID
        // GTK/D-Bus application id. An invalid id would make GTK reject it and
        // silently drop uniqueness, which is exactly the failure this task fixes.
        let running = app_id(werust_core::version());
        assert!(
            banner().contains(werust_core::version()),
            "the id and the banner read the same version source: {running}"
        );
        assert!(
            gio::Application::id_is_valid(&running),
            "the id this binary registers must be valid: {running}"
        );

        // Teeth for that check: the NAIVE splice (the version dropped in as-is,
        // with its dots and its leading digit) is rejected by GLib, so the
        // underscore + `v` transformation is doing real work.
        assert!(
            !gio::Application::id_is_valid("com.github.wighawag.werust.0.2.9"),
            "a digit-initial element is an invalid application id"
        );

        // The version is resolved at build time and is NOT always a release
        // triple: a dev build is `git describe` output (`0.2.6-3-gabc1234`, or a
        // bare short hash with no reachable tag), and an operator can inject an
        // arbitrary `WERUST_VERSION`. All of them must still produce a valid id.
        for version in [
            "0.2.9",
            "0.2.6-3-gabc1234",
            "abc1234",
            "vendor-build",
            "1.0.0+build meta",
        ] {
            let id = app_id(version);
            assert!(
                gio::Application::id_is_valid(&id),
                "version {version:?} must yield a valid application id, got {id}"
            );
        }
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
    fn console_rows_carry_the_level_message_and_source_line_coloured_by_level() {
        // Acceptance (Console tab): a captured console entry renders level +
        // message + source:line, level-distinguished. The level tag is the
        // store's own wire name, and the row's CSS class is distinct per level
        // (error red, warn amber via `debug-console-*` in APP_CSS).
        let entry = ConsoleEntry::new(ConsoleLevel::Warn, "deprecated API")
            .with_source("https://x/app.js")
            .with_line(42);
        assert_eq!(
            console_row_text(&entry),
            "[warn] deprecated API (https://x/app.js:42)"
        );
        assert_eq!(console_level_css_class(entry.level), "debug-console-warn");

        // Every level has its OWN class, so the levels are visually distinct.
        let classes: Vec<&str> = [
            ConsoleLevel::Log,
            ConsoleLevel::Info,
            ConsoleLevel::Warn,
            ConsoleLevel::Error,
            ConsoleLevel::Debug,
        ]
        .into_iter()
        .map(console_level_css_class)
        .collect();
        for (i, class) in classes.iter().enumerate() {
            assert!(class.starts_with("debug-console-"));
            assert!(
                !classes[..i].contains(class),
                "every level is coloured distinctly: {classes:?}"
            );
        }

        // An absent source/line stays honestly absent: no fabricated `:0`, no
        // dangling parentheses.
        let no_source = ConsoleEntry::new(ConsoleLevel::Error, "boom");
        assert_eq!(console_source_line(&no_source), "");
        assert_eq!(console_row_text(&no_source), "[error] boom");
        let source_no_line =
            ConsoleEntry::new(ConsoleLevel::Log, "hi").with_source("ipfs://cid/a.js");
        assert_eq!(
            console_row_text(&source_no_line),
            "[log] hi (ipfs://cid/a.js)"
        );
    }

    #[test]
    fn network_rows_carry_method_status_mime_and_size_with_unknowns_honest() {
        // Acceptance (Network tab): the row columns render the entry's method,
        // status, mime and size, and an UNKNOWN field renders as `?`, never a
        // fabricated `0` (a failed request has no status; a shim-reported
        // request may have no size).
        let entry = NetworkEntry::new("GET", "ipfs://bafy/pic.png")
            .with_status(200)
            .with_mime("image/png")
            .with_size(1536);
        assert_eq!(entry.method, "GET");
        assert_eq!(network_status_text(entry.status), "200");
        assert_eq!(network_mime_text(&entry.mime), "image/png");
        assert_eq!(network_size_text(entry.size), "1.5 KB");

        let unknown = NetworkEntry::new("GET", "https://x/y");
        assert_eq!(network_status_text(unknown.status), "?");
        assert_eq!(network_mime_text(&unknown.mime), "?");
        assert_eq!(network_size_text(unknown.size), "?");

        // The size column is human-scaled at the unit boundaries.
        assert_eq!(network_size_text(Some(0)), "0 B");
        assert_eq!(network_size_text(Some(512)), "512 B");
        assert_eq!(network_size_text(Some(1024)), "1.0 KB");
        assert_eq!(network_size_text(Some(1024 * 1024)), "1.0 MB");
    }

    #[test]
    fn the_network_trust_column_speaks_the_chrome_trust_indicators_exact_vocabulary() {
        // Acceptance (Network tab trust): each request renders werust's HONEST
        // per-request trust posture using the SAME vocabulary as the trust
        // indicator (ADR-0006), never a new label: the indicator's glyph, the
        // core's wire name, and one of the SAME `trust-*` CSS classes the
        // indicator toggles.
        let indicator_classes = [
            "trust-verified",
            "trust-name-trusted-rpc",
            "trust-mutable-name",
            "trust-unverified",
        ];
        let mut labels = Vec::new();
        for (posture, glyph) in [
            (TrustPosture::ContentVerified, "✓"),
            (TrustPosture::NameViaTrustedRpc, "◈"),
            (TrustPosture::MutableName, "◇"),
            (TrustPosture::UnverifiedOrigin, "⚠"),
        ] {
            let label = network_trust_label(posture);
            // The label is the indicator's glyph plus the core's wire name, so an
            // ipfs:// row reads `✓ content-verified` and an https:// row
            // `⚠ unverified-origin`: the SAME words the chrome JSON carries.
            assert_eq!(
                label,
                format!("{glyph} {}", trust_posture_wire_name(posture)),
                "the Network tab speaks the trust indicator's vocabulary: {label}"
            );
            assert!(
                indicator_classes.contains(&network_trust_css_class(posture)),
                "the trust column reuses the indicator's own CSS classes"
            );
            assert!(
                !labels.contains(&label),
                "each posture is labelled distinctly: {label}"
            );
            labels.push(label);
        }

        // The honest split the spec names: an ipfs:// request content-verified,
        // an https:// subresource unverified-origin.
        assert_eq!(
            network_trust_label(TrustPosture::ContentVerified),
            "✓ content-verified"
        );
        assert_eq!(
            network_trust_label(TrustPosture::UnverifiedOrigin),
            "⚠ unverified-origin"
        );
        assert_eq!(
            network_trust_css_class(TrustPosture::ContentVerified),
            "trust-verified"
        );
        assert_eq!(
            network_trust_css_class(TrustPosture::UnverifiedOrigin),
            "trust-unverified"
        );
    }

    #[test]
    fn the_refresh_plan_appends_after_the_last_rendered_sequence_or_rebuilds() {
        // First paint (nothing rendered yet): an empty store is a no-op, a
        // non-empty one renders everything.
        assert_eq!(tail_plan(&[], 0, None), TailPlan::Noop);
        assert_eq!(tail_plan(&[1, 2, 3], 0, None), TailPlan::Rebuild);
        // The steady state BELOW the cap: append exactly the entries AFTER the
        // anchor, dropping nothing (nothing was evicted).
        assert_eq!(
            tail_plan(&[1, 2, 3], 1, Some(1)),
            TailPlan::AppendFrom { drop: 0, from: 1 }
        );
        assert_eq!(
            tail_plan(&[1, 2, 3], 2, Some(2)),
            TailPlan::AppendFrom { drop: 0, from: 2 }
        );
        // Caught up: the anchor is the snapshot's last entry.
        assert_eq!(tail_plan(&[1, 2, 3], 3, Some(3)), TailPlan::Noop);
        // AT-CAP EVICTION (the defect Gate-2 caught): the ring buffer's length
        // is pinned at the cap, but the anchor still falls inside the snapshot,
        // so the view appends only the entries after it AND drops the evicted
        // rows from its top. The view's rows end at the anchor, so of the 3 it
        // holds only the snapshot rows up to and including the anchor's
        // position are still in the store; the rest drop.
        assert_eq!(
            tail_plan(&[4, 5, 6], 3, Some(4)),
            TailPlan::AppendFrom { drop: 2, from: 1 }
        );
        assert_eq!(
            tail_plan(&[4, 5, 6], 3, Some(5)),
            TailPlan::AppendFrom { drop: 1, from: 2 }
        );
        // The anchor itself was evicted (a full buffer turned over while the
        // view was open): everything the view holds is stale, so REBUILD.
        assert_eq!(tail_plan(&[4, 5, 6], 3, Some(2)), TailPlan::Rebuild);
        // A CLEAR: the snapshot is shorter than what the view rendered (here
        // empty), so rebuild.
        assert_eq!(tail_plan(&[], 3, Some(2)), TailPlan::Rebuild);
    }

    #[test]
    fn pushing_past_the_cap_still_renders_the_newest_entry_and_drops_the_evicted_rows() {
        // The acceptance defect, driven against the REAL store (network-isolated,
        // no display): once a ring buffer sits AT its cap its length never
        // changes, so a length-anchored refresh freezes on rows the store has
        // already discarded. The sequence-anchored plan must keep the view
        // showing exactly what the store holds. The "view" here is a Vec of the
        // rendered messages, applying `tail_plan` EXACTLY as
        // `DebugView::refresh` applies it to the `ListBox` (drop the evicted
        // rows off the top, append the new tail, rebuild when the anchor is
        // gone), so the assertions are about what the real view shows.
        fn apply(
            capture: &DebugCapture,
            view: &mut Vec<String>,
            last: &mut Option<u64>,
        ) -> TailPlan {
            let snapshot = capture.console();
            let sequences: Vec<u64> = snapshot.iter().map(ConsoleEntry::sequence).collect();
            let plan = tail_plan(&sequences, view.len(), *last);
            match plan {
                TailPlan::Rebuild => {
                    view.clear();
                    view.extend(snapshot.iter().map(|e| e.message.clone()));
                }
                TailPlan::AppendFrom { drop, from } => {
                    view.drain(..drop);
                    view.extend(snapshot[from..].iter().map(|e| e.message.clone()));
                }
                TailPlan::Noop => {}
            }
            *last = sequences.last().copied();
            plan
        }

        let capture = DebugCapture::new();
        let mut view: Vec<String> = Vec::new();
        let mut last_rendered: Option<u64> = None;
        for i in 0..werust_core::debug::MAX_CONSOLE_ENTRIES {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("m{i}")));
        }
        // The first paint rebuilds from the full (capped) store.
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert_eq!(view.len(), werust_core::debug::MAX_CONSOLE_ENTRIES);

        // Push 10 past the cap ONE AT A TIME with a refresh between each (the
        // pump-tick case): the store's length is UNCHANGED at every tick (the
        // freeze the old length-only refresh hit), but each tick evicts one row
        // from the front. The view must stay AT the cap and mirror the store
        // after every INCREMENTAL append, not only after a rebuild (the round-2
        // Gate-2 defect: an append-only path climbs past the cap on stale rows).
        for i in 0..10 {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("new{i}")));
            let snapshot = capture.console();
            assert_eq!(snapshot.len(), werust_core::debug::MAX_CONSOLE_ENTRIES);
            let plan = apply(&capture, &mut view, &mut last_rendered);
            assert_eq!(
                plan,
                TailPlan::AppendFrom {
                    drop: 1,
                    from: werust_core::debug::MAX_CONSOLE_ENTRIES - 1
                },
                "each at-cap tick appends the one new entry and drops the one evicted row"
            );
            assert_eq!(
                view.len(),
                werust_core::debug::MAX_CONSOLE_ENTRIES,
                "the row count STAYS AT the cap across incremental at-cap appends"
            );
            assert_eq!(
                view.last().unwrap(),
                &format!("new{i}"),
                "the newest entry renders even at the cap"
            );
            assert!(
                view.iter()
                    .zip(snapshot.iter())
                    .all(|(v, e)| v == &e.message),
                "the view mirrors the store exactly: its top rows are never ones the store evicted"
            );
        }

        // A FULL buffer turns over between ticks: the anchor itself is evicted,
        // so the view rebuilds instead of appending onto stale rows.
        for i in 0..werust_core::debug::MAX_CONSOLE_ENTRIES {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("x{i}")));
        }
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert!(
            view.iter()
                .zip(capture.console().iter())
                .all(|(v, e)| v == &e.message),
            "a rebuild re-mirrors the store exactly"
        );

        // A clear: the snapshot is shorter than rendered, a rebuild empties it.
        capture.clear();
        assert_eq!(
            apply(&capture, &mut view, &mut last_rendered),
            TailPlan::Rebuild
        );
        assert!(view.is_empty());
    }

    #[test]
    fn default_url_is_an_https_url() {
        assert!(DEFAULT_URL.starts_with("https://"));
    }

    /// The number of row widgets currently in a debug-view list, walking the
    /// widget siblings (the rows are appended as plain children of the
    /// `ListBox`). Test-only.
    fn row_count(list: &gtk4::ListBox) -> usize {
        let mut count = 0;
        let mut child = list.first_child();
        while let Some(widget) = child {
            count += 1;
            child = widget.next_sibling();
        }
        count
    }

    /// The text of the LAST row of a debug-view list (the row widget GTK wraps
    /// each appended child in), or an empty string when the list is empty.
    /// Test-only.
    fn last_row_text(list: &gtk4::ListBox) -> String {
        let mut last = None;
        let mut child = list.first_child();
        while let Some(widget) = child {
            last = Some(widget.clone());
            child = widget.next_sibling();
        }
        let Some(row) = last.and_then(|w| w.downcast::<gtk4::ListBoxRow>().ok()) else {
            return String::new();
        };
        row.child()
            .and_then(|w| w.downcast::<Label>().ok())
            .map_or_else(String::new, |label| label.label().to_string())
    }

    /// The text of the FIRST (top) row of a debug-view list, or an empty string
    /// when the list is empty. Test-only.
    fn first_row_text(list: &gtk4::ListBox) -> String {
        let Some(row) = list
            .first_child()
            .and_then(|w| w.downcast::<gtk4::ListBoxRow>().ok())
        else {
            return String::new();
        };
        row.child()
            .and_then(|w| w.downcast::<Label>().ok())
            .map_or_else(String::new, |label| label.label().to_string())
    }

    /// End-to-end, on the REAL widgets: the menu's Debug entry opens the real
    /// debug-view window (a `Notebook` of the two tabs), the window renders a
    /// real capture store and refreshes incrementally, Clear empties it, and
    /// closing the window drops the slot so a later activation opens a fresh
    /// one. Ignored by default because it initializes GTK, which needs a display
    /// the `verify` gate may not have; run explicitly on a desktop session with
    /// `cargo test -p werust -- --ignored`. It is ONE test on purpose: GTK can
    /// only be initialized on one thread, so the display-requiring steps share
    /// this one test thread. The render-from-store MAPPING itself is pinned
    /// display-free by the pure-function tests above.
    #[test]
    #[ignore = "needs a display: constructs the real menu + debug-view window (GTK init)"]
    fn real_debug_view_end_to_end_on_a_display() {
        use super::{build_menu_button, DebugViewState};
        use gtk4::{Application, ApplicationWindow};

        gtk4::init().expect("gtk init on a desktop session");
        let app = Application::builder()
            .application_id("com.github.wighawag.werust.test")
            .build();
        app.register(gio::Cancellable::NONE)
            .expect("register the test application");
        let window = ApplicationWindow::builder().application(&app).build();

        let capture = DebugCapture::new();
        let state = Rc::new(DebugViewState {
            capture: capture.clone(),
            open: RefCell::new(None),
        });
        let menu_button = build_menu_button(&window, &state);

        // Find the Debug ACTION button inside the popover's item list (the menu
        // is built by iterating the core's items; the Debug entry is the only
        // Action today) and activate it, exactly as a click does.
        let popover = menu_button.popover().expect("the menu has a popover");
        let list = popover.child().expect("the popover has the item list");
        let mut child = list.first_child();
        let mut debug_button = None;
        while let Some(widget) = child {
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                if button.label().as_deref() == Some("Debug") {
                    debug_button = Some(button);
                }
            }
            child = widget.next_sibling();
        }
        let debug_button = debug_button.expect("the menu lists a Debug action");

        debug_button.emit_clicked();
        assert!(
            state.open.borrow().is_some(),
            "activating the Debug entry opens the debug view"
        );

        // The OPEN view renders the store, and refreshes INCREMENTALLY: a tick
        // with one more captured entry appends only the tail, never rebuilds.
        let view = state
            .open
            .borrow()
            .as_ref()
            .expect("the view is open")
            .clone();
        capture.push_console(
            ConsoleEntry::new(ConsoleLevel::Error, "boom")
                .with_source("https://x/app.js")
                .with_line(7),
        );
        capture.push_network(
            NetworkEntry::new("GET", "ipfs://bafy/pic.png")
                .with_status(200)
                .with_mime("image/png")
                .with_size(1536)
                .with_trust(TrustPosture::ContentVerified),
        );
        view.borrow_mut().refresh();
        assert_eq!(row_count(&view.borrow().console_list), 1);
        assert_eq!(row_count(&view.borrow().network_list), 1);
        capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, "later"));
        view.borrow_mut().refresh();
        assert_eq!(row_count(&view.borrow().console_list), 2);
        assert_eq!(row_count(&view.borrow().network_list), 1);

        // Clear: emptying the shared store resets both lists on the next tick
        // (the store shrank below the rendered counts).
        capture.clear();
        view.borrow_mut().refresh();
        assert_eq!(row_count(&view.borrow().console_list), 0);
        assert_eq!(row_count(&view.borrow().network_list), 0);

        // PAST THE CAP (the Gate-2 defect): once the ring buffer sits AT its
        // 300-entry cap its length never changes, but the refresh must still
        // render the newest entry and drop the rows the store evicted.
        for i in 0..(werust_core::debug::MAX_CONSOLE_ENTRIES + 10) {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("m{i}")));
        }
        view.borrow_mut().refresh();
        assert_eq!(
            row_count(&view.borrow().console_list),
            werust_core::debug::MAX_CONSOLE_ENTRIES,
            "the view mirrors the capped store, no frozen stale rows"
        );
        assert!(
            last_row_text(&view.borrow().console_list)
                .contains(&format!("m{}", werust_core::debug::MAX_CONSOLE_ENTRIES + 9)),
            "the newest entry renders even after eviction at the cap"
        );
        capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, "newest"));
        view.borrow_mut().refresh();
        assert!(
            last_row_text(&view.borrow().console_list).contains("newest"),
            "a push at the cap still reaches the view"
        );

        // The round-2 half of the defect: an INCREMENTAL at-cap append must also
        // DROP the evicted rows off the top, so the row count STAYS AT the cap
        // (an append-only path climbs past it) and the top row is never one the
        // store already discarded. Drive several more pushes one at a time (the
        // pump-tick case), asserting the count and the mirror after each.
        assert_eq!(
            row_count(&view.borrow().console_list),
            werust_core::debug::MAX_CONSOLE_ENTRIES,
            "an at-cap append drops the evicted row: the count stays at the cap"
        );
        for i in 0..5 {
            capture.push_console(ConsoleEntry::new(ConsoleLevel::Log, format!("extra{i}")));
            view.borrow_mut().refresh();
            assert_eq!(
                row_count(&view.borrow().console_list),
                werust_core::debug::MAX_CONSOLE_ENTRIES,
                "the row count STAYS AT the cap across incremental at-cap appends"
            );
            assert!(
                last_row_text(&view.borrow().console_list).contains(&format!("extra{i}")),
                "the newest entry renders at every at-cap tick"
            );
            let store_head = capture.console()[0].message.clone();
            assert!(
                first_row_text(&view.borrow().console_list).contains(&store_head),
                "the view's top row is the store's head, not an evicted row"
            );
            assert!(
                !first_row_text(&view.borrow().console_list).contains("m10"),
                "the long-evicted row is gone from the view's top"
            );
        }

        // Activating Debug again PRESENTS the same window rather than opening a
        // second copy (the slot still holds it, so no new view is built).
        let built = Rc::strong_count(&view);
        debug_button.emit_clicked();
        assert_eq!(Rc::strong_count(&view), built, "no second view is built");

        // Closing the debug window drops the slot, so the next activation opens
        // a fresh one.
        view.borrow().window.close();
        assert!(
            state.open.borrow().is_none(),
            "closing the debug window clears the slot"
        );

        window.close();
    }

    // -- The headless CLI dispatch (task `headless-cli-mode`) ----------------

    /// Parse a borrowed argv TAIL (argv[0] already dropped, as [`parse_args`]
    /// expects) the way the shell hands it to the binary.
    fn parse(args: &[&str]) -> Command {
        parse_args(args.iter().map(|a| (*a).to_string()))
    }

    #[test]
    fn argv_routes_the_known_subcommands_and_falls_through_to_the_gui() {
        // Acceptance (task `headless-cli-mode`): the binary is verb-first like
        // `git`/`cargo` — a KNOWN subcommand runs headlessly and exits, and
        // anything else (including NO argument at all) still opens the GUI
        // exactly as before, so the default is backward-compatible.
        assert_eq!(
            parse(&["resolve", "ronan.eth"]),
            Command::Resolve {
                name: "ronan.eth".into(),
                json: false
            }
        );
        // `--json` is a flag on `resolve`, accepted on EITHER side of the name
        // (a hand-rolled parser has no reason to be positional about a flag).
        for argv in [
            ["resolve", "--json", "ronan.eth"],
            ["resolve", "ronan.eth", "--json"],
        ] {
            assert_eq!(
                parse(&argv),
                Command::Resolve {
                    name: "ronan.eth".into(),
                    json: true
                }
            );
        }
        assert_eq!(parse(&["version"]), Command::Version);
        assert_eq!(parse(&["--help"]), Command::Help);
        assert_eq!(parse(&["-h"]), Command::Help);
        // `--version` / `-V` are the flag spellings of the `version` subcommand.
        assert_eq!(parse(&["--version"]), Command::Version);
        assert_eq!(parse(&["-V"]), Command::Version);

        // The GUI default: no argument opens the startup URL, one non-subcommand
        // argument is still the URL to open (the pre-CLI behaviour, unchanged).
        assert_eq!(
            parse(&[]),
            Command::Gui {
                url: DEFAULT_URL.into()
            }
        );
        assert_eq!(
            parse(&["https://example.org/"]),
            Command::Gui {
                url: "https://example.org/".into()
            }
        );
        // An unknown verb is NOT an error: it is a URL to open, so nothing that
        // used to launch the GUI now refuses to (only the named verbs are taken).
        assert_eq!(
            parse(&["ronan.eth"]),
            Command::Gui {
                url: "ronan.eth".into()
            }
        );

        // A malformed `resolve` invocation is a USAGE refusal (stderr + exit 1),
        // never a silent GUI launch or a resolution of the wrong thing.
        assert!(matches!(parse(&["resolve"]), Command::Usage(_)));
        assert!(matches!(
            parse(&["resolve", "ronan.eth", "example.eth"]),
            Command::Usage(_)
        ));
        assert!(matches!(
            parse(&["resolve", "--nope", "ronan.eth"]),
            Command::Usage(_)
        ));
    }

    #[test]
    fn resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one() {
        // Acceptance (task `headless-cli-mode`): `werust resolve <name>` prints the
        // resolved contenthash REFERENCE on stdout (one bare line, so
        // `$(werust resolve …)` is directly usable), and `--json` prints the same
        // facts as one machine-readable object. Pure, so the formatting is pinned
        // without a network or a display.
        let ipfs = DecodedContenthash::Ipfs {
            uri: "ipfs://bafkreiabc".into(),
            cid: "bafkreiabc".into(),
        };
        assert_eq!(
            resolve_output("example.eth", &ipfs, false),
            Ok("ipfs://bafkreiabc".to_string())
        );
        assert_eq!(
            resolve_output("example.eth", &ipfs, true),
            Ok(
                "{\"name\":\"example.eth\",\"kind\":\"ipfs\",\"reference\":\"ipfs://bafkreiabc\"}"
                    .to_string()
            )
        );

        // A MUTABLE `ipns-ns` contenthash is reported as the `ipns://<name>`
        // pointer it is — honestly distinct from an immutable `ipfs://` reference,
        // and NOT followed (following a record is the fetch path, not this ENS
        // read; see the module docs).
        let ipns = DecodedContenthash::Ipns {
            name: "k51qzifixture".into(),
        };
        assert_eq!(
            resolve_output("ronan.eth", &ipns, false),
            Ok("ipns://k51qzifixture".to_string())
        );
        assert_eq!(
            resolve_output("ronan.eth", &ipns, true),
            Ok(
                "{\"name\":\"ronan.eth\",\"kind\":\"ipns\",\"reference\":\"ipns://k51qzifixture\"}"
                    .to_string()
            )
        );

        // Fail-closed: a well-formed contenthash for a protocol werust does not
        // support is the decoder's OWN protocol-named refusal on stderr (exit 1),
        // never printed as if it were a loadable reference.
        let unsupported = DecodedContenthash::Unsupported(ProtoCode::Swarm);
        let reason = resolve_output("example.eth", &unsupported, false)
            .expect_err("an unsupported contenthash must be a refusal, not output");
        assert!(
            reason.contains("Swarm"),
            "the refusal names the protocol: {reason}"
        );

        // The JSON is escaped, so a name carrying a quote/backslash cannot break
        // the object it is embedded in (hand-rolled output, no serde).
        assert_eq!(
            resolve_output("a\"b\\c", &ipns, true),
            Ok(
                "{\"name\":\"a\\\"b\\\\c\",\"kind\":\"ipns\",\"reference\":\"ipns://k51qzifixture\"}"
                    .to_string()
            )
        );
    }

    #[test]
    fn usage_lists_every_subcommand_and_the_gui_default() {
        // Acceptance (task `headless-cli-mode`): `werust --help` prints a usage
        // message listing the available subcommands AND that the default (no
        // subcommand) opens the GUI — so the CLI is discoverable from the binary
        // itself.
        let usage = usage();
        for expected in ["werust resolve", "--json", "werust version", "--help"] {
            assert!(
                usage.contains(expected),
                "usage must mention `{expected}`: {usage}"
            );
        }
        assert!(
            usage.to_lowercase().contains("gui") || usage.to_lowercase().contains("browser"),
            "usage must say the default opens the browser GUI: {usage}"
        );
    }
}
