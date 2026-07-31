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
//!
//! The same is now true of the DEBUG VIEW's rows: `console_row_text`, the level
//! and trust CSS classes, the network columns and the incremental-refresh
//! `tail_plan` were private to this file until the macOS debug view needed the
//! same derivation, and MOVED into [`werust_core::debug`] beside the store they
//! render (task `macos-appkit-window-and-chrome`). What stays here is the GTK
//! half: building a `Label`/`Box` per row and applying the plan to a `ListBox`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::pango::EllipsizeMode;
use gtk4::prelude::*;
use gtk4::{
    gdk, glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    ListBox, MenuButton, Notebook, Orientation, Popover, ScrolledWindow, Widget, Window,
};

use webkit6::prelude::WebViewExt;
use webview_renderer::WebViewRenderer;
use werust_core::name_resolution::ResolvedName;
// The debug view's ROW rules are the shared core's, not this edge's: they moved
// there with their tests when the macOS debug view needed the same derivation
// (task `macos-appkit-window-and-chrome`).
use werust_core::debug::{
    console_level_css_class, console_row_text, network_mime_text, network_size_text,
    network_status_text, network_trust_css_class, network_trust_label, tail_plan, ConsoleEntry,
    DebugCapture, NetworkEntry, TailPlan,
};
use werust_core::ethereum::RpcProvider;
use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG};
use werust_core::{
    error_banner_css_class, error_banner_text, error_banner_visible, invalid_entry_badge_text,
    invalid_entry_badge_visible, load_progress_fraction, load_progress_tooltip, status_line,
    trust_indicator, trust_indicator_css_class, trust_indicator_detail, trust_pin_action_label,
    trust_pin_action_visible, trust_pin_detail, BrowserShell, ChromeState,
    ERROR_BANNER_CSS_CLASSES, STOP_AFFORDANCE_LABEL, TRUST_INDICATOR_CSS_CLASSES,
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
    /// the `ipfs://<cid>` it loads — FOLLOWING a mutable `ipns-ns` pointer through
    /// its client-verified record, exactly as the GUI does.
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
         \x20 werust resolve <ens-name>   resolve an ENS name to the ipfs://<cid> it loads\n\
         \x20 werust resolve --json <n>   the same, as one JSON object\n\
         \x20 werust version              print the version banner (also --version, -V)\n\
         \x20 werust --help               print this message (also -h)\n\
         \n\
         `resolve` performs the FULL resolution the browser performs: a name whose\n\
         ENS contenthash is a MUTABLE ipns-ns pointer is followed through its\n\
         client-verified IPNS record to the CID it points at right now. stdout is\n\
         always the bare ipfs://<cid>; the mutable step is noted on stderr, and\n\
         --json carries both the followed pointer and the CID.\n\
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

/// What a successful headless `resolve` writes: the stdout `line` (the RESULT),
/// plus the optional stderr `note` that keeps the answer honest.
///
/// Two streams because they serve two readers. stdout stays a single bare
/// machine-usable value (`$(werust resolve …)` is directly usable in a script,
/// the property `headless-cli-mode` established), so the mutable-name warning
/// cannot be appended to it; stderr is the HUMAN channel this binary already uses
/// for its reasons, so the warning goes there and nothing is hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolveOutput {
    /// The stdout line: the `ipfs://<cid>` reference, or the one-object JSON form.
    line: String,
    /// The stderr note, for a MUTABLE name in the human (non-`--json`) form.
    note: Option<String>,
}

/// Format the OUTPUT of a headless `resolve` for a resolved name: the
/// `ipfs://<cid>` line (or its one-object JSON form), plus the mutable-name note
/// when the CID came from a followed `ipns-ns` pointer.
///
/// `resolve` performs the FULL resolution the browser performs — a name whose
/// contenthash is a MUTABLE `ipns-ns` pointer is followed through its
/// client-verified record to the CID it points at right now (task
/// `cli-resolve-follows-mutable-names-to-the-cid`) — so the printed reference is
/// the one werust itself can open, not the `ipns://` pointer its own URL bar
/// cannot (`docs/adr/0007` decision 4).
///
/// But following is NOT flattening (`docs/adr/0006`): a mutable name's CID is
/// "what it points at right now", so the mutable fact rides along with it — a
/// stderr NOTE in the human form, and BOTH facts (`mutable`, the followed
/// `pointer`, AND the resolved `cid`) in the `--json` object, so a script that
/// pins the CID can see it came from a mutable name. The `--json` form gets no
/// note: it already carries the fact in the object, and a script's stderr should
/// stay quiet on success.
///
/// The `kind` value is the CORE's protocol vocabulary
/// ([`ProtoCode::wire_name`](werust_core::contenthash::ProtoCode::wire_name):
/// the ENSIP-7 `ipfs-ns` / `ipns-ns` spelling the decoder dispatches on), never a
/// spelling minted in this binary, so a later `fetch` verb cannot fork a second
/// one. The object itself is hand-rolled with `format!` — no serde in the binary
/// (task `headless-cli-mode`).
fn resolve_output(name: &str, resolved: &ResolvedName, json: bool) -> ResolveOutput {
    if json {
        // A stable shape: the same keys for both kinds, with `pointer` null when
        // there was no mutable pointer to follow, so a consumer reads one form.
        let pointer = match resolved.mutable_pointer() {
            Some(pointer) => format!("\"{}\"", json_escape(pointer)),
            None => "null".to_string(),
        };
        return ResolveOutput {
            line: format!(
                "{{\"name\":\"{name}\",\"kind\":\"{kind}\",\"reference\":\"{reference}\",\
                 \"cid\":\"{cid}\",\"mutable\":{mutable},\"pointer\":{pointer}}}",
                name = json_escape(name),
                kind = resolved.proto_code().wire_name(),
                reference = json_escape(resolved.uri()),
                cid = json_escape(resolved.cid()),
                mutable = resolved.is_mutable(),
            ),
            note: None,
        };
    }
    ResolveOutput {
        line: resolved.uri().to_string(),
        note: resolved.mutable_pointer().map(|pointer| {
            format!(
                "werust: {name} is a MUTABLE name ({pointer}): this is the CID its \
                 client-verified IPNS record points at right now, and its controller \
                 can repoint it."
            )
        }),
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

/// Run the headless `resolve` subcommand: resolve `name` through the core's ONE
/// name-resolution path and print [`resolve_output`], with a Unix exit status.
///
/// The resolution is [`werust_core::name_resolution::resolve_name`] — the SAME
/// function [`BrowserShell`]'s ENS front door calls, so the CLI prints exactly
/// what the GUI would load for that name, including following a MUTABLE
/// `ipns-ns` contenthash through its client-VERIFIED IPNS record. There is no
/// second implementation to drift: a record that fails verification fails this
/// command the same way it fails the browser's load.
///
/// NO GTK is touched on this path — no [`Application`], no window, not even
/// `gtk::init` — so it runs over ssh, in CI and in any environment with no
/// display. The provider is [`RpcProvider::new`] and the record source is
/// [`werust_core::ipns::default_record_source`], the SAME endpoint sources the
/// GUI shell builds (the `WERUST_RPC_URL` env lever and the user's chosen
/// retrieval backend), so a CLI resolution and the browser's own address-bar
/// resolution can never disagree about which chain or which gateway they read.
///
/// A failure prints the core's OWN typed reason to stderr (`werust: {e}`, the
/// formatting the GUI surfaces too) and exits 1, so a script can branch on the
/// status instead of parsing stdout.
fn run_resolve(name: &str, json: bool) -> glib::ExitCode {
    let provider = RpcProvider::new();
    let ipns_source = werust_core::ipns::default_record_source();
    match werust_core::name_resolution::resolve_name(&provider, &ipns_source, name) {
        Ok(resolved) => {
            let output = resolve_output(name, &resolved, json);
            // The mutable-name note goes FIRST and to stderr, so stdout stays the
            // bare result a script consumes.
            if let Some(note) = output.note {
                eprintln!("{note}");
            }
            println!("{}", output.line);
            glib::ExitCode::SUCCESS
        }
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
    /// The trust SURFACE's explanation line: the same sentence the badge carries
    /// as its hover tooltip ([`trust_indicator_detail`]), shown in full inside the
    /// popover the badge opens, so the explanation is READABLE, not only
    /// hover-discoverable.
    trust_detail: Label,
    /// The trust surface's TRUST-ON-FIRST-USE line ([`trust_pin_detail`]): the
    /// mutable name, the CID it resolves to now, and what the user blessed for it.
    /// Hidden on a page with no mutable name.
    trust_pin_detail: Label,
    /// The trust surface's BLESS action ([`trust_pin_action_label`]): shown only
    /// when there is something new to record. An affordance INSIDE the surface the
    /// user opened, never a prompt (task `ipns-tofu-pin-and-warn-on-change`).
    trust_pin_button: Button,
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
        // slow without stealing a fixed slot from the toolbar. The SENTENCE — the
        // phase, and the cancel hint exactly while there is a backend load Stop
        // can cancel — is the core's one rule, not this edge's: it was written out
        // verbatim here and in the AppKit painter, which is how the Kotlin and
        // Swift twins started drifting. This edge contributes only the label its
        // own Stop control carries.
        let phase_tooltip = load_progress_tooltip(state, STOP_AFFORDANCE_LABEL);
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
            // across a transition. The toggle set is the CORE's exported severity
            // family, never a literal restated here (see the trust set below).
            let active = error_banner_css_class(state);
            for class in ERROR_BANNER_CSS_CLASSES.iter().copied() {
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
        // one class is active at a time, so the toggle set must contain EVERY
        // class `trust_indicator_css_class` can return, or a stale class would
        // linger on a transition. So the set is the CORE's own exported posture
        // family — the same crate that DECIDES the names lists them — and never a
        // literal copied into this painter: a copy silently goes stale the moment
        // a fifth posture lands, and nothing would clear the missing class
        // (task `export-the-chrome-css-class-set-from-core`).
        self.trust.set_text(trust_indicator(state));
        self.trust
            .set_tooltip_text(Some(trust_indicator_detail(state)));
        let active = trust_indicator_css_class(state);
        for class in TRUST_INDICATOR_CSS_CLASSES.iter().copied() {
            if class == active {
                self.trust.add_css_class(class);
            } else {
                self.trust.remove_css_class(class);
            }
        }
        // The TRUST SURFACE behind the badge: the posture explanation in full,
        // plus the trust-on-first-use section: what this MUTABLE name resolves to
        // now, what (if anything) the user blessed for it, and the bless action
        // when there is something new to record. Shown only when the user opens
        // the surface: the bless is an explicit action reached FROM the indicator,
        // never a first-visit prompt (task `ipns-tofu-pin-and-warn-on-change`).
        self.trust_detail.set_text(trust_indicator_detail(state));
        let pin_detail = trust_pin_detail(state);
        self.trust_pin_detail.set_visible(!pin_detail.is_empty());
        self.trust_pin_detail.set_text(&pin_detail);
        let offer_bless = trust_pin_action_visible(state);
        self.trust_pin_button.set_visible(offer_bless);
        self.trust_pin_button
            .set_label(trust_pin_action_label(state));
    }
}

/// The app stylesheet: the classes that make the trust-indicator states
/// visually distinct (a NEUTRAL grey loading badge shown while a load is in
/// flight, a green content-verified badge, a blue name-via-trusted-RPC badge, a
/// purple mutable-name badge, the error banner's own RED for a blessed name that
/// now points to DIFFERENT content (the loudest settled state there is), and an
/// amber unverified-origin one), plus the error
/// banner, the invalid-URL badge, the URL bar's own load-progress bar (the
/// `entry > progress` node, painted from the live pipeline phase), the menu info
/// item, and the debug view's
/// level-coloured console rows. Kept as one constant next to the classes the
/// chrome and the debug view toggle (`trust-loading` / `trust-verified` /
/// `trust-name-trusted-rpc` / `trust-mutable-name` / `trust-name-changed` /
/// `trust-unverified`, `debug-console-*`). The debug view's Network tab REUSES the `trust-*`
/// classes for its per-request trust column, so a content-verified request is
/// the same green the indicator's verified badge is (ADR-0006, one vocabulary).
const APP_CSS: &str = "\
.trust-loading { color: #5c5c5c; font-weight: bold; padding: 0 6px; }\
.trust-verified { color: #0a7d28; font-weight: bold; padding: 0 6px; }\
.trust-name-trusted-rpc { color: #1a5fb4; font-weight: bold; padding: 0 6px; }\
.trust-mutable-name { color: #6c3fb4; font-weight: bold; padding: 0 6px; }\
.trust-name-changed { color: #c01c28; font-weight: bold; padding: 0 6px; }\
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
    // Both the label and its class come from the shared derivation for the SAME
    // default state, so the badge's first paint cannot disagree with the class it
    // carries (and the class name is not restated in this painter).
    let trust = Label::new(Some(trust_indicator(&ChromeState::default())));
    trust.add_css_class(trust_indicator_css_class(&ChromeState::default()));
    // The TRUST SURFACE: clicking the badge opens a small popover explaining the
    // posture in full and, for a MUTABLE name, showing what it resolves to now,
    // what the user blessed for it, and the bless action. This is the settled UX
    // of task `ipns-tofu-pin-and-warn-on-change`: the bless is an EXPLICIT action
    // reached FROM the trust indicator, never a first-visit prompt (a prompt on
    // first visit trains people to dismiss it, and this surface is already where
    // the posture is explained). Every string in it is the shared derivation.
    let trust_detail = Label::builder().xalign(0.0).wrap(true).build();
    let trust_pin_detail = Label::builder()
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    let trust_pin_button = Button::builder().visible(false).build();
    let trust_surface = GtkBox::new(Orientation::Vertical, 8);
    trust_surface.set_size_request(360, -1);
    trust_surface.append(&trust_detail);
    trust_surface.append(&trust_pin_detail);
    trust_surface.append(&trust_pin_button);
    let trust_button = MenuButton::builder()
        .popover(&Popover::builder().child(&trust_surface).build())
        .child(&trust)
        .build();
    trust_button.add_css_class("flat");

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
    error_banner.add_css_class(error_banner_css_class(&ChromeState::default()));

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
    toolbar.append(&trust_button);
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
        trust_detail: trust_detail.clone(),
        trust_pin_detail: trust_pin_detail.clone(),
        trust_pin_button: trust_pin_button.clone(),
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
    // The TRUST-ON-FIRST-USE bless: record the current mutable name's CID as the
    // version this user trusts, so a LATER resolution to different content is
    // warned about (task `ipns-tofu-pin-and-warn-on-change`). The DECISION of
    // whether there is anything to bless (and what the button says) is the
    // core's (`Chrome::refresh` paints both from `trust_pin_action_*`); this
    // handler only drives the shell and repaints, exactly like every control
    // above. The surface closes afterwards, because the answer to "did that
    // work?" is the badge and banner behind it, both of which just changed.
    trust_pin_button.connect_clicked({
        let shell = shell.clone();
        let refresh = refresh.clone();
        move |button| {
            shell.borrow_mut().bless_current_name();
            if let Some(popover) = button.ancestor(Popover::static_type()) {
                if let Ok(popover) = popover.downcast::<Popover>() {
                    popover.popdown();
                }
            }
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
        app_id, banner, parse_args, resolve_output, should_open_web_inspector, usage, Command,
        ResolveOutput, ResolvedName, APP_CSS, DEFAULT_URL,
    };
    use gtk4::prelude::*;
    use gtk4::{gdk, gio, Label};
    use renderer::TrustPosture;
    use std::cell::RefCell;
    use std::rc::Rc;
    use werust_core::contenthash::ProtoCode;
    use werust_core::debug::{ConsoleEntry, ConsoleLevel, DebugCapture, NetworkEntry};
    use werust_core::menu::{BrowserMenu, MenuItemKind, MENU_ITEM_DEBUG, MENU_ITEM_VERSION};
    use werust_core::CssClassFamily;

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
    fn every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css() {
        // Acceptance (task `export-the-chrome-css-class-set-from-core`): the
        // painter toggles the core's exported class set, so every name in that
        // set must also be STYLED here. The cousin of a stale class is an
        // UNSTYLED one: a new state that is toggled perfectly but has no rule in
        // `APP_CSS` renders invisibly, so the user sees no badge change at all.
        // `APP_CSS` is a plain `&str`, so containment of the rule's selector is
        // enough — and it stays HERE, in the edge that has a stylesheet, because
        // the core has no notion of colour.
        //
        // WHICH families are checked is the CORE's aggregate (`CssClassFamily::ALL`),
        // never a list written out here: a hand-written list was exhaustive over
        // the CLASSES of the families it named but not over the FAMILIES, so a
        // sixth family joined no gate at all and painted invisibly with a green
        // suite (task `one-derivation-close-the-aggregate-and-tooltip-gaps`). The
        // aggregate deliberately covers MORE than `CHROME_CSS_CLASS_SETS`: the
        // debug view's console levels colour a row and are not toggled on a chrome
        // widget, but an unstyled level is exactly as invisible as an unstyled
        // badge, so coverage spans every exported family.
        let styled = |class: &str| APP_CSS.contains(&format!(".{class} {{"));
        let mut checked = 0;
        for family in CssClassFamily::ALL {
            for class in family.classes().iter().copied() {
                assert!(
                    styled(class),
                    "the core exports `{class}` but `APP_CSS` has no `.{class} {{ … }}` rule, so the state would render invisibly"
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            CssClassFamily::ALL
                .iter()
                .map(|family| family.classes().len())
                .sum::<usize>(),
            "every class of every exported family is checked, not just one family"
        );
        assert!(
            CssClassFamily::ALL.len() > 1 && checked > CssClassFamily::ALL.len(),
            "the drive is the whole aggregate, not a degenerate one-family loop"
        );
        // The guard has teeth: a class the core did NOT export is not styled
        // either, so the assertion above is not vacuously true.
        assert!(!styled("trust-not-a-posture"));
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
    fn resolve_prints_the_ipfs_reference_for_an_immutable_name() {
        // Acceptance (task `headless-cli-mode`, kept): `werust resolve <name>`
        // prints the resolved reference on stdout as ONE bare line (so
        // `$(werust resolve …)` is directly usable), and `--json` prints the same
        // facts as one machine-readable object. Pure, so the formatting is pinned
        // without a network or a display.
        //
        // The `kind` is the CORE's ENSIP-7 vocabulary (`ProtoCode::wire_name`),
        // never a spelling minted in this binary (task
        // `cli-resolve-follows-mutable-names-to-the-cid`).
        let immutable = ResolvedName::Immutable {
            uri: "ipfs://bafkreiabc".into(),
            cid: "bafkreiabc".into(),
        };
        assert_eq!(
            resolve_output("example.eth", &immutable, false),
            ResolveOutput {
                line: "ipfs://bafkreiabc".to_string(),
                note: None,
            },
            "an immutable name prints its CID with nothing to warn about"
        );
        assert_eq!(
            resolve_output("example.eth", &immutable, true),
            ResolveOutput {
                line: "{\"name\":\"example.eth\",\"kind\":\"ipfs-ns\",\
                       \"reference\":\"ipfs://bafkreiabc\",\"cid\":\"bafkreiabc\",\
                       \"mutable\":false,\"pointer\":null}"
                    .to_string(),
                note: None,
            }
        );
        assert_eq!(
            ProtoCode::Ipfs.wire_name(),
            "ipfs-ns",
            "the printed kind IS the core's vocabulary, not a literal here"
        );
    }

    #[test]
    fn resolve_follows_a_mutable_name_to_the_cid_and_keeps_saying_it_is_mutable() {
        // Acceptance (task `cli-resolve-follows-mutable-names-to-the-cid`): a name
        // whose ENS contenthash is a MUTABLE `ipns-ns` pointer resolves through to
        // the `ipfs://<cid>` the GUI would actually load — the CLI no longer prints
        // the `ipns://` pointer werust's own URL bar cannot open. But the mutable
        // fact is NOT lost (`docs/adr/0006`): the human form says so on stderr, and
        // `--json` carries BOTH the followed pointer and the resolved CID, so a
        // script that pins the CID can see where it came from.
        let mutable = ResolvedName::Mutable {
            pointer: "ipns://k51qzifixture".into(),
            uri: "ipfs://bafkreicurrent".into(),
            cid: "bafkreicurrent".into(),
        };

        let human = resolve_output("ronan.eth", &mutable, false);
        assert_eq!(
            human.line, "ipfs://bafkreicurrent",
            "stdout is the loadable CID, one bare line"
        );
        let note = human.note.expect("a mutable name carries a note");
        assert!(
            note.contains("MUTABLE") && note.contains("ipns://k51qzifixture"),
            "the note names the mutability AND the followed pointer: {note}"
        );

        assert_eq!(
            resolve_output("ronan.eth", &mutable, true),
            ResolveOutput {
                line: "{\"name\":\"ronan.eth\",\"kind\":\"ipns-ns\",\
                       \"reference\":\"ipfs://bafkreicurrent\",\"cid\":\"bafkreicurrent\",\
                       \"mutable\":true,\"pointer\":\"ipns://k51qzifixture\"}"
                    .to_string(),
                // `--json` already carries the fact in the object, so a scripted
                // success stays quiet on stderr.
                note: None,
            }
        );
        assert_eq!(
            ProtoCode::Ipns.wire_name(),
            "ipns-ns",
            "the mutable kind is the core's ENSIP-7 spelling too"
        );

        // The JSON is escaped, so a name carrying a quote/backslash cannot break
        // the object it is embedded in (hand-rolled output, no serde).
        assert!(resolve_output("a\"b\\c", &mutable, true)
            .line
            .starts_with("{\"name\":\"a\\\"b\\\\c\","));
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
