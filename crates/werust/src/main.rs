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

use gtk4::pango::EllipsizeMode;
use gtk4::prelude::*;
use gtk4::{
    gdk, glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    ListBox, MenuButton, Notebook, Orientation, Popover, ScrolledWindow, Widget, Window,
};

use renderer::TrustPosture;
use webkit6::prelude::WebViewExt;
use webview_renderer::WebViewRenderer;
use werust_core::debug::{
    trust_posture_wire_name, ConsoleEntry, ConsoleLevel, DebugCapture, NetworkEntry,
};
use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG};
use werust_core::{BrowserShell, ChromeState, LoadStep};

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
    /// The NON-BLOCKING loading banner: a bar across the top of the view that
    /// appears WHILE a load is in flight, naming the current pipeline phase (one
    /// of the existing `LoadStep` values, verbatim) and offering a Cancel that
    /// calls the SAME `BrowserShell::stop` the toolbar Stop button uses. This is
    /// the field-test v0.2.7 fix — on a long retrieval the user stared at a frozen
    /// page with no signal anything was happening; this banner says "working:
    /// fetching content…" with a way out. Driven by the existing chrome-refresh
    /// pump (no new timer / poll / tight loop), so the Android ANR guard is not
    /// regressed. Hidden on a settled/failed chrome (the [`error_banner`] takes
    /// the slot then). Task `loading-banner-with-phase-and-cancel`.
    loading_banner: GtkBox,
    /// The phase-name label inside [`loading_banner`](Chrome::loading_banner),
    /// updated by [`refresh`](Chrome::refresh) as the phase advances.
    loading_banner_label: Label,
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
        // The NON-BLOCKING loading banner: shown ONLY while a load is in flight,
        // naming the current pipeline phase (one of the existing `LoadStep`
        // values, verbatim). Its CANCEL calls the SAME `BrowserShell::stop` the
        // toolbar Stop button uses (wired once at construction). Hidden on a
        // settled/failed chrome (the error banner takes the slot on a failure) —
        // the two are mutually exclusive, since a load is either in flight or has
        // settled. Driven by this existing refresh, so no new timer / poll / tight
        // loop (the Android ANR guard is not regressed). Task
        // `loading-banner-with-phase-and-cancel`.
        let show_loading = loading_banner_visible(state);
        self.loading_banner.set_visible(show_loading);
        if show_loading {
            self.loading_banner_label
                .set_text(&loading_banner_text(state));
        }
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

/// Whether the NON-BLOCKING loading banner should be shown: exactly while a
/// load is in flight ([`ChromeState::is_loading`]). A passive view update driven
/// by the existing chrome-refresh pump (NOT a new timer / poll / tight loop), so
/// the Android ANR guard is not regressed. A pure function of [`ChromeState`] so
/// it is testable without a display; the mobile shells apply the SAME rule from
/// the chrome JSON `loading` fact (task `loading-banner-with-phase-and-cancel`).
///
/// This is the IN-FLIGHT counterpart of [`error_banner_visible`]: the error
/// banner appears on a FAILED load, the loading banner appears while a load is
/// STILL RUNNING. The two are mutually exclusive in practice (a load is either
/// in flight or has settled as finished/failed/idle), and both hide on a
/// settled-ok chrome, so they never compete for the same slot.
fn loading_banner_visible(state: &ChromeState) -> bool {
    state.is_loading()
}

/// The loading-banner text: names the current pipeline phase (one of the
/// existing [`LoadStep`] values, verbatim) so a slow load reads as working, not
/// frozen — the field-test v0.2.7 finding this task answers (the user stares at a
/// frozen page on long retrievals with no signal anything is happening). The
/// phase names are the [`LoadStep`] vocabulary verbatim (capitalised +
/// ellipsised for the banner surface), so the banner and the debug Network tab
/// cannot disagree. Empty of a phase only when a load is in flight but no step is
/// known yet (an [`LoadStep::Idle`] with [`LoadState::Started`]), in which case a
/// generic "Loading…" is shown so the banner never lies about a frozen phase.
/// Pure, for the same reason as [`status_line`].
///
/// The banner CANCEL calls the SAME `BrowserShell::stop` the toolbar Stop button
/// uses (no new mechanic); that wiring lives in the GTK handler, not here.
fn loading_banner_text(state: &ChromeState) -> String {
    match state.load_step() {
        LoadStep::Idle => "Loading…".to_string(),
        LoadStep::ResolvingName => "Resolving name…".to_string(),
        LoadStep::FetchingRecord => "Fetching record…".to_string(),
        LoadStep::FetchingContent => "Fetching content…".to_string(),
        LoadStep::Rendering => "Rendering…".to_string(),
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

/// The app stylesheet: the classes that make the trust-indicator states
/// visually distinct (a NEUTRAL grey loading badge shown while a load is in
/// flight, a green content-verified badge, a blue name-via-trusted-RPC badge, a
/// purple mutable-name badge, an amber unverified-origin one), plus the error
/// banner, the invalid-URL badge, the menu info item, and the debug view's
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
.loading-banner { background-color: #1a5fb4; color: #ffffff; font-weight: bold; padding: 10px 12px; }\
.loading-banner button { font-weight: bold; }\
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

    // The NON-BLOCKING loading banner: a bar across the top of the view, shown
    // ONLY while a load is in flight, naming the current pipeline phase (one of
    // the existing `LoadStep` values, verbatim) and offering a Cancel that calls
    // the SAME `BrowserShell::stop` the toolbar Stop button uses (task
    // `loading-banner-with-phase-and-cancel`). It starts hidden; the phase label
    // is hexpand so the Cancel button sits at the END of the row. Driven by the
    // existing chrome-refresh pump (no new timer / poll / tight loop).
    let loading_banner_label = Label::builder()
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .xalign(0.0)
        .wrap(true)
        .build();
    let loading_banner_cancel = Button::with_label("Cancel");
    let loading_banner = GtkBox::new(Orientation::Horizontal, 8);
    loading_banner.append(&loading_banner_label);
    loading_banner.append(&loading_banner_cancel);
    loading_banner.add_css_class("loading-banner");
    loading_banner.set_visible(false);

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
        loading_banner: loading_banner.clone(),
        loading_banner_label: loading_banner_label.clone(),
        invalid_badge: invalid_badge.clone(),
    });

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&toolbar);
    // The loading banner and the error banner share the slot directly under the
    // toolbar and ABOVE the page view. They are mutually exclusive (a load is
    // either in flight or has settled as finished/failed/idle), so only one is
    // visible at a time; both surface a load state the user cannot miss in the
    // content area, not buried in the footer status line.
    root.append(&loading_banner);
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
    // The loading banner's CANCEL calls the SAME `BrowserShell::stop` the toolbar
    // Stop button uses — no new mechanic, just a second affordance surfaced in the
    // banner while a load is in flight (task `loading-banner-with-phase-and-cancel`).
    loading_banner_cancel.connect_clicked({
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
        banner, console_level_css_class, console_row_text, console_source_line,
        error_banner_css_class, error_banner_text, error_banner_visible, invalid_entry_badge_text,
        invalid_entry_badge_visible, loading_banner_text, loading_banner_visible,
        network_mime_text, network_size_text, network_status_text, network_trust_css_class,
        network_trust_label, should_open_web_inspector, status_line, tail_plan, trust_indicator,
        trust_indicator_css_class, trust_indicator_detail, TailPlan, DEFAULT_URL,
    };
    use gtk4::prelude::*;
    use gtk4::{gdk, gio, Label};
    use renderer::{LoadState, TrustPosture};
    use std::cell::RefCell;
    use std::rc::Rc;
    use werust_core::debug::{
        trust_posture_wire_name, ConsoleEntry, ConsoleLevel, DebugCapture, NetworkEntry,
    };
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
    fn loading_banner_names_the_phase_while_a_load_is_in_flight_and_hides_when_settled() {
        // Acceptance (task `loading-banner-with-phase-and-cancel`): while a load
        // is in flight a NON-BLOCKING banner in the chrome names the current
        // pipeline phase (one of the existing `LoadStep` values, verbatim) and
        // updates as the phase advances; it disappears on Finished / Failed /
        // Idle. The banner is a pure function of `ChromeState` (driven by the
        // existing chrome-refresh pump), so it is testable without a display.
        use renderer::LoadState;
        use werust_core::LoadStep;

        // Hidden when nothing is loading (idle / finished / failed).
        assert!(!loading_banner_visible(&ChromeState::default()));
        let failed = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        assert!(!loading_banner_visible(&failed));

        // Shown while loading, naming the LIVE step. The phase names are the
        // existing `LoadStep` vocabulary verbatim (so the banner and the debug
        // Network tab cannot disagree), capitalised + ellipsised for the banner.
        let resolving = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        };
        assert!(loading_banner_visible(&resolving));
        assert_eq!(loading_banner_text(&resolving), "Resolving name…");

        let record = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingRecord,
            ..ChromeState::default()
        };
        assert_eq!(loading_banner_text(&record), "Fetching record…");

        let content = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::FetchingContent,
            ..ChromeState::default()
        };
        assert_eq!(loading_banner_text(&content), "Fetching content…");

        let rendering = ChromeState {
            load_state: LoadState::Committed,
            load_step: LoadStep::Rendering,
            ..ChromeState::default()
        };
        assert_eq!(loading_banner_text(&rendering), "Rendering…");

        // A loading state with no known step (Idle step) still shows the banner
        // (a load IS in flight), with a generic "Loading…" so it never lies about
        // a frozen phase.
        let loading_no_step = ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::Idle,
            ..ChromeState::default()
        };
        assert!(loading_banner_visible(&loading_no_step));
        assert_eq!(loading_banner_text(&loading_no_step), "Loading…");
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
