//! The AppKit window: the widgets, and nothing else.
//!
//! This is the only file in werust that talks to AppKit, and it is deliberately
//! the DUMBEST one. Every string, fraction, colour and enabled-flag it assigns
//! comes from [`crate::paint`] (which reads the shared `werust-core` derivation);
//! every user action it receives is forwarded to the shared
//! [`BrowserShell`]. It decides nothing about browsing, trust or wording.
//!
//! # The window
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ ◀ ▶ ⟳ ✕ │ URL bar (+ progress) │ ⛔ badge │ trust badge │ ⋮ │  toolbar
//! ├─────────────────────────────────────────────────────────────┤
//! │ ⚠ This page failed to load: <protocol-named reason>         │  error banner
//! ├─────────────────────────────────────────────────────────────┤  (failures only)
//! │                                                             │
//! │                    the WKWebView page view                  │
//! │                                                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │ loading… — fetching content                                 │  status line
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! The layout is computed by hand in [`Chrome::relayout`] from the content view's
//! bounds, in a FLIPPED container so the arithmetic reads top-down. No Auto
//! Layout: the window is a handful of fixed-height strips over one flexible page
//! area, and hand-computed frames are the shape that can be reasoned about (and
//! reviewed) without a running Mac.
//!
//! **Only a FAILURE moves the page.** The error banner is the one strip whose
//! visibility changes the page view's height, and it appears only when a load
//! failed — there is nothing rendered to displace, and the user must act.
//! In-flight progress is painted INSIDE the URL bar's rectangle in the
//! fixed-height toolbar, so a navigation never resizes the page view and content
//! cannot jump under the pointer (task
//! `loading-progress-in-the-url-bar-not-a-banner`).
//!
//! # The debug view
//!
//! A SEPARATE window (the same choice the GTK edge made and recorded: an in-window
//! panel would crowd the page view on every open, and a window closes with its own
//! close button). It is an `NSTabView` of a CONSOLE and a NETWORK tab over the
//! shared capture store, refreshed on the SAME pump tick as the chrome — the rows
//! come from [`crate::paint`], so they read exactly like the GTK ones — and it is
//! READ-ONLY: every row is a non-editable label. A typeable REPL is Safari's Web
//! Inspector's job.
//!
//! # ADR-0009: what this file does NOT do
//!
//! It never sets an `NSAppearance`, on any window, view or webview. AppKit
//! propagates the user's effective appearance into the chrome AND into the
//! `WKWebView`'s web process, so following the OS costs exactly nothing here and
//! forcing dark (or light) is what ADR-0009 forbids. The source-shape guard
//! asserts the absence.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBezelStyle, NSButton,
    NSColor, NSFont, NSMenu, NSMenuItem, NSProgressIndicator, NSProgressIndicatorStyle,
    NSScrollView, NSTabView, NSTabViewItem, NSTextField, NSView, NSWindow, NSWindowDelegate,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSPoint, NSRect, NSSize, NSString, NSTimeInterval, NSTimer,
};

use renderer::RendererError;
use werust_core::debug::DebugCapture;
use werust_core::menu::MENU_ITEM_DEBUG;
use werust_core::BrowserShell;

use crate::paint::{
    console_refresh, install_debug_capture, menu_items, network_refresh, ChromePaint,
    ConsoleRowPaint, MenuItemPaint, NetworkRowPaint, Rgb, TabUpdate, INVALID_ENTRY_COLOR,
};

/// The window's initial size.
const DEFAULT_WINDOW_SIZE: NSSize = NSSize::new(1024.0, 768.0);
/// The toolbar strip's height.
const TOOLBAR_HEIGHT: f64 = 40.0;
/// The error banner's height (a failure is the one state allowed to take it).
const BANNER_HEIGHT: f64 = 44.0;
/// The status line's height.
const STATUS_HEIGHT: f64 = 22.0;
/// The gap around chrome items.
const MARGIN: f64 = 8.0;
/// A nav button's width.
const BUTTON_WIDTH: f64 = 36.0;
/// The trust indicator's width (it carries a whole phrase, not a glyph).
const TRUST_WIDTH: f64 = 210.0;
/// The invalid-entry badge's width.
const BADGE_WIDTH: f64 = 110.0;
/// The URL bar's progress strip: a few points along its bottom edge, INSIDE the
/// bar, so it takes no height from the page.
const PROGRESS_HEIGHT: f64 = 3.0;
/// One debug-view row's height.
const ROW_HEIGHT: f64 = 18.0;
/// The debug view's initial size.
const DEBUG_WINDOW_SIZE: NSSize = NSSize::new(760.0, 480.0);
/// The chrome pump cadence: the same 50ms the GTK shell uses.
const PUMP_INTERVAL: NSTimeInterval = 0.05;

/// Build an `NSString` without the ceremony at every call site.
fn ns(text: &str) -> Retained<NSString> {
    NSString::from_str(text)
}

/// Convert one of [`crate::paint`]'s colours into an `NSColor`.
fn color(rgb: Rgb) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(rgb.red, rgb.green, rgb.blue, 1.0)
}

/// A rectangle, spelled once.
fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

define_class!(
    // SAFETY:
    // - `NSView` has no subclassing requirements beyond main-thread use.
    // - `FlippedView` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacFlippedView"]
    struct FlippedView;

    unsafe impl NSObjectProtocol for FlippedView {}

    impl FlippedView {
        /// Lay subviews out TOP-DOWN. AppKit's default origin is bottom-left,
        /// which makes a stack of chrome strips (and a list of debug rows) read
        /// upside down in the arithmetic; flipping the container is the standard
        /// answer and keeps every frame calculation in this file pointing the
        /// same way as the picture in the module docs.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm);
        unsafe { msg_send![this, initWithFrame: frame] }
    }
}

/// A non-editable, single-line label: the building block of every read-only
/// surface here (the trust badge, the invalid badge, the status line, the error
/// banner and every debug row).
fn label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&ns(text), mtm);
    field.setSelectable(true);
    field.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    field
}

/// A push button, wired later by [`wire_actions`] (the controller does not exist
/// yet when the widgets are built).
fn button(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet, which is always valid.
    let button = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), None, None, mtm) };
    button.setBezelStyle(NSBezelStyle::Push);
    button
}

/// The widgets the pump repaints from [`ChromePaint`], plus the window they live
/// in. Grouped so ONE [`Chrome::apply`] keeps every surface in step with the
/// shell's state — the same shape the GTK edge's `Chrome` has, for the same
/// reason: a half-applied chrome is how a stale badge survives a transition.
struct Chrome {
    window: Retained<NSWindow>,
    content: Retained<FlippedView>,
    toolbar: Retained<FlippedView>,
    back: Retained<NSButton>,
    forward: Retained<NSButton>,
    reload: Retained<NSButton>,
    stop: Retained<NSButton>,
    menu_button: Retained<NSButton>,
    url_field: Retained<NSTextField>,
    /// The load-progress strip, laid out INSIDE the URL bar (never its own row).
    progress: Retained<NSProgressIndicator>,
    invalid_badge: Retained<NSTextField>,
    trust: Retained<NSTextField>,
    error_banner: Retained<NSTextField>,
    status: Retained<NSTextField>,
    /// The backend's container view, embedded through the seam's `ViewHandle`.
    page: Retained<NSView>,
    /// The ⋮ menu, built once from the core's `BrowserMenu`.
    menu: Retained<NSMenu>,
    /// Whether the error banner is currently taking its strip. Tracked because a
    /// change in it is the ONE thing that re-lays-out the page area.
    banner_visible: Cell<bool>,
    /// Whether the invalid-entry badge is taking its toolbar slot.
    badge_visible: Cell<bool>,
}

impl Chrome {
    /// Paint one [`ChromePaint`] into the widgets.
    ///
    /// Straight-line assignment: no rule is evaluated here, and every value is a
    /// field of the snapshot the core derived.
    fn apply(&self, paint: &ChromePaint) {
        // Only overwrite the URL bar when it does not already hold this text, so
        // the caret does not jump while the user is mid-edit.
        if self.url_field.stringValue().to_string() != paint.url_text {
            self.url_field.setStringValue(&ns(&paint.url_text));
        }
        // The INVALID-entry surface (field finding D): the badge appears and the
        // typed text is rendered invalid, while the text itself is KEPT for the
        // user to fix.
        let url_color = if paint.invalid_entry {
            color(INVALID_ENTRY_COLOR)
        } else {
            NSColor::textColor()
        };
        self.url_field.setTextColor(Some(&url_color));
        self.invalid_badge
            .setStringValue(&ns(paint.invalid_badge_text));
        self.invalid_badge.setHidden(!paint.invalid_entry);

        self.back.setEnabled(paint.can_go_back);
        self.forward.setEnabled(paint.can_go_forward);
        // Stop is meaningful only while a load is in flight; Reload only once it
        // has settled.
        self.stop.setEnabled(paint.is_loading);
        self.reload.setEnabled(!paint.is_loading);

        self.status.setStringValue(&ns(&paint.status_text));

        // The trust indicator: the core's badge text, its explanation as the
        // tooltip, and the colour of the class the core chose. Exactly one state
        // is painted, so no stale colour can survive a transition.
        self.trust.setStringValue(&ns(paint.trust_text));
        self.trust.setToolTip(Some(&ns(paint.trust_detail)));
        self.trust.setTextColor(Some(&color(paint.trust_color)));

        // The URL bar's own progress strip: it advances with the real pipeline
        // phase and disappears once the load settles. It changes NO geometry, so
        // a navigation never resizes the page view.
        self.progress.setDoubleValue(paint.progress_fraction);
        self.progress.setHidden(!paint.progress_visible);
        let tooltip = paint.progress_tooltip.as_deref().map(ns);
        self.url_field.setToolTip(tooltip.as_deref());

        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate, protocol-named reason across the top of the view.
        if paint.error_visible {
            self.error_banner.setStringValue(&ns(&paint.error_text));
            self.error_banner.setDrawsBackground(true);
            self.error_banner
                .setBackgroundColor(Some(&color(paint.error_color)));
            self.error_banner.setTextColor(Some(&NSColor::whiteColor()));
        }
        self.error_banner.setHidden(!paint.error_visible);

        // A change in either optional surface changes the geometry, so the strips
        // are re-laid-out; nothing else in a repaint moves a frame.
        if self.banner_visible.get() != paint.error_visible
            || self.badge_visible.get() != paint.invalid_entry
        {
            self.banner_visible.set(paint.error_visible);
            self.badge_visible.set(paint.invalid_entry);
            self.relayout();
        }
    }

    /// Recompute every frame from the content view's bounds: fixed strips top and
    /// bottom, the page view taking everything between. Called on open, on every
    /// window resize, and whenever the banner or badge appears/disappears.
    fn relayout(&self) {
        let bounds = self.content.bounds();
        let width = bounds.size.width;
        let height = bounds.size.height;
        let banner_height = if self.banner_visible.get() {
            BANNER_HEIGHT
        } else {
            0.0
        };

        self.toolbar.setFrame(rect(0.0, 0.0, width, TOOLBAR_HEIGHT));
        self.error_banner.setFrame(rect(
            MARGIN,
            TOOLBAR_HEIGHT + 6.0,
            (width - 2.0 * MARGIN).max(0.0),
            (banner_height - 12.0).max(0.0),
        ));
        let page_top = TOOLBAR_HEIGHT + banner_height;
        let page_height = (height - page_top - STATUS_HEIGHT).max(0.0);
        self.page.setFrame(rect(0.0, page_top, width, page_height));
        self.status.setFrame(rect(
            MARGIN,
            height - STATUS_HEIGHT + 3.0,
            (width - 2.0 * MARGIN).max(0.0),
            STATUS_HEIGHT - 6.0,
        ));

        // The toolbar's own row, left to right: the nav controls, then the URL
        // bar taking the slack, then (optionally) the invalid badge, the trust
        // indicator and the ⋮ menu pinned to the right.
        let row_y = 6.0;
        let row_height = TOOLBAR_HEIGHT - 12.0;
        let mut x = MARGIN;
        for control in [&self.back, &self.forward, &self.reload, &self.stop] {
            control.setFrame(rect(x, row_y, BUTTON_WIDTH, row_height));
            x += BUTTON_WIDTH + 2.0;
        }
        let badge_width = if self.badge_visible.get() {
            BADGE_WIDTH + 6.0
        } else {
            0.0
        };
        let right = width - MARGIN - BUTTON_WIDTH - TRUST_WIDTH - badge_width - 12.0;
        let url_width = (right - x - 6.0).max(60.0);
        self.url_field
            .setFrame(rect(x, row_y, url_width, row_height));
        // INSIDE the URL bar, along its bottom edge: the progress strip takes no
        // height of its own and therefore cannot move the page.
        self.progress.setFrame(rect(
            x + 2.0,
            row_y + row_height - PROGRESS_HEIGHT - 2.0,
            (url_width - 4.0).max(0.0),
            PROGRESS_HEIGHT,
        ));
        let mut x = x + url_width + 6.0;
        if self.badge_visible.get() {
            self.invalid_badge
                .setFrame(rect(x, row_y, BADGE_WIDTH, row_height));
            x += BADGE_WIDTH + 6.0;
        }
        self.trust.setFrame(rect(x, row_y, TRUST_WIDTH, row_height));
        x += TRUST_WIDTH + 6.0;
        self.menu_button
            .setFrame(rect(x, row_y, BUTTON_WIDTH, row_height));
    }
}

/// One tab of the debug view: a scrollable, flipped document of read-only rows,
/// plus the two anchors its incremental refresh needs.
struct DebugTab {
    scroll: Retained<NSScrollView>,
    document: Retained<FlippedView>,
    rows: RefCell<Vec<Retained<NSView>>>,
    rendered: Cell<usize>,
    last_sequence: Cell<Option<u64>>,
}

impl DebugTab {
    fn new(mtm: MainThreadMarker) -> Self {
        let document = FlippedView::new(mtm, rect(0.0, 0.0, DEBUG_WINDOW_SIZE.width, ROW_HEIGHT));
        let scroll = NSScrollView::new(mtm);
        scroll.setHasVerticalScroller(true);
        scroll.setDocumentView(Some(&document));
        Self {
            scroll,
            document,
            rows: RefCell::new(Vec::new()),
            rendered: Cell::new(0),
            last_sequence: Cell::new(None),
        }
    }

    /// Whether the tab is scrolled to the bottom (so newly appended rows should
    /// follow — the devtools-console idiom: a user who scrolled up to read an
    /// earlier entry is never yanked back down).
    fn is_at_bottom(&self) -> bool {
        let clip = self.scroll.contentView().bounds();
        let document = self.document.frame();
        clip.origin.y + clip.size.height >= document.size.height - 1.0
    }

    /// Apply one [`TabUpdate`], building only the rows it carries, then re-frame
    /// the rows and (if the view was following) scroll to the newest.
    fn apply<R>(
        &self,
        mtm: MainThreadMarker,
        update: TabUpdate<R>,
        build: fn(MainThreadMarker, &R) -> Retained<NSView>,
    ) {
        let stick = self.is_at_bottom();
        match update {
            TabUpdate::Noop => return,
            TabUpdate::Rebuild(new_rows) => {
                let drained: Vec<Retained<NSView>> = self.rows.borrow_mut().drain(..).collect();
                for view in drained {
                    view.removeFromSuperview();
                }
                for row in &new_rows {
                    let view = build(mtm, row);
                    self.document.addSubview(&view);
                    self.rows.borrow_mut().push(view);
                }
            }
            TabUpdate::Append { drop, rows: tail } => {
                // The rows the ring buffer evicted from the store's front drop
                // off the view's TOP; without that the list would climb past the
                // store's cap on rows the store already discarded.
                let dropped: Vec<Retained<NSView>> = self.rows.borrow_mut().drain(..drop).collect();
                for view in dropped {
                    view.removeFromSuperview();
                }
                for row in &tail {
                    let view = build(mtm, row);
                    self.document.addSubview(&view);
                    self.rows.borrow_mut().push(view);
                }
            }
        }
        self.reframe(stick);
    }

    /// Re-frame the document and its rows after a change (or a window resize).
    fn reframe(&self, stick: bool) {
        let clip = self.scroll.contentView().bounds();
        let rows = self.rows.borrow();
        let width = clip.size.width.max(200.0);
        let height = (rows.len() as f64 * ROW_HEIGHT).max(clip.size.height);
        self.document.setFrame(rect(0.0, 0.0, width, height));
        for (index, view) in rows.iter().enumerate() {
            view.setFrame(rect(0.0, index as f64 * ROW_HEIGHT, width, ROW_HEIGHT));
        }
        if stick {
            self.document
                .scrollPoint(NSPoint::new(0.0, (height - clip.size.height).max(0.0)));
        }
    }

    /// How many rows the tab currently shows (the CI smoke asserts on this).
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }
}

/// The debug view: a separate window with a CLEAR action over a tabbed Console +
/// Network view of the shared capture store.
struct DebugWindow {
    window: Retained<NSWindow>,
    root: Retained<FlippedView>,
    tabs: Retained<NSTabView>,
    clear: Retained<NSButton>,
    console: DebugTab,
    network: DebugTab,
}

impl DebugWindow {
    fn new(mtm: MainThreadMarker) -> Self {
        let frame = rect(0.0, 0.0, DEBUG_WINDOW_SIZE.width, DEBUG_WINDOW_SIZE.height);
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Resizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(&ns("werust Debug"));
        // Closing the window must not deallocate it while the controller still
        // holds the slot; the slot is cleared from `windowWillClose:` instead.
        unsafe { window.setReleasedWhenClosed(false) };

        let root = FlippedView::new(mtm, frame);
        let title = label(mtm, "Console + Network capture");
        title.setFrame(rect(MARGIN, MARGIN, 400.0, 20.0));
        root.addSubview(&title);
        // The CLEAR action empties the SHARED store (both buffers); the next
        // refresh then resets both tabs.
        let clear = button(mtm, "Clear");
        clear.setFrame(rect(
            frame.size.width - MARGIN - 90.0,
            MARGIN - 4.0,
            90.0,
            26.0,
        ));
        root.addSubview(&clear);

        let tabs = NSTabView::new(mtm);
        let console = DebugTab::new(mtm);
        let network = DebugTab::new(mtm);
        for (name, tab) in [("Console", &console), ("Network", &network)] {
            let item = NSTabViewItem::new();
            item.setLabel(&ns(name));
            item.setView(Some(&tab.scroll));
            tabs.addTabViewItem(&item);
        }
        tabs.setFrame(rect(
            MARGIN,
            36.0,
            frame.size.width - 2.0 * MARGIN,
            frame.size.height - 36.0 - MARGIN,
        ));
        root.addSubview(&tabs);
        window.setContentView(Some(&root));

        Self {
            window,
            root,
            tabs,
            clear,
            console,
            network,
        }
    }

    /// Re-frame the debug window's own strips after a resize.
    fn relayout(&self) {
        let bounds = self.root.bounds();
        self.clear.setFrame(rect(
            bounds.size.width - MARGIN - 90.0,
            MARGIN - 4.0,
            90.0,
            26.0,
        ));
        self.tabs.setFrame(rect(
            MARGIN,
            36.0,
            (bounds.size.width - 2.0 * MARGIN).max(100.0),
            (bounds.size.height - 36.0 - MARGIN).max(60.0),
        ));
        self.console.reframe(false);
        self.network.reframe(false);
    }
}

/// One CONSOLE row: the core's row text, coloured by the core's level class.
fn console_row(mtm: MainThreadMarker, row: &ConsoleRowPaint) -> Retained<NSView> {
    let field = label(mtm, &row.text);
    field.setTextColor(Some(&color(row.color)));
    // An `NSTextField` IS an `NSView`; the rows are held uniformly as views.
    Retained::into_super(Retained::into_super(field))
}

/// One NETWORK row: fixed columns (method, status, MIME, size, trust) with the
/// URL taking the slack, the trust column in the chrome's own posture colour.
fn network_row(mtm: MainThreadMarker, row: &NetworkRowPaint) -> Retained<NSView> {
    let container = FlippedView::new(mtm, rect(0.0, 0.0, DEBUG_WINDOW_SIZE.width, ROW_HEIGHT));
    let columns: [(&str, f64); 5] = [
        (row.method.as_str(), 50.0),
        (row.status.as_str(), 44.0),
        (row.mime.as_str(), 130.0),
        (row.size.as_str(), 70.0),
        (row.trust.as_str(), 170.0),
    ];
    let mut x = 0.0;
    for (index, (text, width)) in columns.into_iter().enumerate() {
        let field = label(mtm, text);
        // The trust column (the last fixed one) wears the SAME class colour the
        // chrome's trust indicator uses for that posture (`docs/adr/0006`).
        if index == 4 {
            field.setTextColor(Some(&color(row.trust_color)));
        }
        field.setFrame(rect(x, 0.0, width, ROW_HEIGHT));
        container.addSubview(&field);
        x += width + 6.0;
    }
    let url = label(mtm, &row.url);
    url.setFrame(rect(
        x,
        0.0,
        (DEBUG_WINDOW_SIZE.width - x).max(80.0),
        ROW_HEIGHT,
    ));
    container.addSubview(&url);
    Retained::into_super(container)
}

/// Everything the window controller owns.
struct ControllerIvars {
    /// The SHARED shell: every control drives this, never the webview.
    shell: Rc<RefCell<BrowserShell>>,
    /// The capture store the debug view renders (the same handle the capture
    /// points push into).
    capture: DebugCapture,
    chrome: Chrome,
    /// The core's menu items, in the order the `NSMenu` lists them; a chosen item
    /// is dispatched by its STABLE id, looked up by the menu item's tag.
    menu_items: Vec<MenuItemPaint>,
    /// The open debug view, if any (re-activating Debug raises it rather than
    /// opening a second copy).
    debug: RefCell<Option<DebugWindow>>,
    /// The chrome pump, held so it can outlive `start_pump`.
    timer: RefCell<Option<Retained<NSTimer>>>,
}

define_class!(
    // SAFETY:
    // - `NSObject` has no subclassing requirements.
    // - `WindowController` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    // Everything it touches (AppKit, WebKit, the `!Send` shell) is
    // main-thread-only.
    #[thread_kind = MainThreadOnly]
    #[name = "WerustMacWindowController"]
    #[ivars = ControllerIvars]
    struct WindowController;

    unsafe impl NSObjectProtocol for WindowController {}

    /// The chrome's actions. Each drives the SHARED `BrowserShell` through the
    /// seam and then repaints; none of them touches the webview directly.
    impl WindowController {
        #[unsafe(method(goBack:))]
        fn go_back(&self, _sender: Option<&AnyObject>) {
            self.ivars().shell.borrow_mut().go_back();
            self.refresh_chrome();
        }

        #[unsafe(method(goForward:))]
        fn go_forward(&self, _sender: Option<&AnyObject>) {
            self.ivars().shell.borrow_mut().go_forward();
            self.refresh_chrome();
        }

        #[unsafe(method(reloadPage:))]
        fn reload_page(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().shell.borrow_mut().reload();
            self.refresh_chrome();
        }

        #[unsafe(method(stopLoading:))]
        fn stop_loading(&self, _sender: Option<&AnyObject>) {
            self.ivars().shell.borrow_mut().stop();
            self.refresh_chrome();
        }

        /// Enter in the URL bar: navigate through the shell, which owns the
        /// front-door rule (a bare `.eth` name, a scheme-less host, an invalid
        /// entry that must NOT navigate).
        #[unsafe(method(urlEntered:))]
        fn url_entered(&self, _sender: Option<&AnyObject>) {
            let typed = self.ivars().chrome.url_field.stringValue().to_string();
            let _ = self.ivars().shell.borrow_mut().navigate(&typed);
            self.refresh_chrome();
        }

        /// The ⋮ button: pop the core-derived menu up under it.
        #[unsafe(method(showBrowserMenu:))]
        fn show_browser_menu(&self, _sender: Option<&AnyObject>) {
            let chrome = &self.ivars().chrome;
            let height = chrome.menu_button.frame().size.height;
            chrome.menu.popUpMenuPositioningItem_atLocation_inView(
                None,
                NSPoint::new(0.0, height),
                Some(&chrome.menu_button),
            );
        }

        /// A menu item was chosen: dispatch on the core item's STABLE id (never
        /// the display label), found by the item's tag.
        #[unsafe(method(browserMenuItemChosen:))]
        fn browser_menu_item_chosen(&self, sender: Option<&NSMenuItem>) {
            let Some(item) = sender else { return };
            let index = usize::try_from(item.tag()).unwrap_or(usize::MAX);
            let Some(chosen) = self.ivars().menu_items.get(index) else {
                return;
            };
            if chosen.id == MENU_ITEM_DEBUG {
                self.open_debug_view();
            }
        }

        /// CLEAR in the debug view: empty the SHARED store (both buffers).
        #[unsafe(method(clearDebugCapture:))]
        fn clear_debug_capture(&self, _sender: Option<&AnyObject>) {
            self.ivars().capture.clear();
            self.refresh_debug_view();
        }

        /// The chrome pump: fold the seam's load-lifecycle events into the chrome
        /// and catch the open debug view up, on ONE timer (no busy loop, and no
        /// second timer for the debug view).
        #[unsafe(method(pumpTick:))]
        fn pump_tick(&self, _sender: Option<&AnyObject>) {
            self.tick();
        }
    }

    /// Both windows' delegate: re-layout on resize, and drop the debug-view slot
    /// when its window closes so the next Debug activation opens a fresh one.
    unsafe impl NSWindowDelegate for WindowController {
        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, notification: &NSNotification) {
            if self.is_debug_window(notification) {
                if let Some(debug) = self.ivars().debug.borrow().as_ref() {
                    debug.relayout();
                }
            } else {
                self.ivars().chrome.relayout();
            }
        }

        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            if self.is_debug_window(notification) {
                *self.ivars().debug.borrow_mut() = None;
            }
        }
    }
);

impl WindowController {
    fn new(
        mtm: MainThreadMarker,
        shell: Rc<RefCell<BrowserShell>>,
        capture: DebugCapture,
        chrome: Chrome,
        menu_items: Vec<MenuItemPaint>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ControllerIvars {
            shell,
            capture,
            chrome,
            menu_items,
            debug: RefCell::new(None),
            timer: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// This controller as a plain object, for target/action wiring.
    fn as_target(&self) -> &AnyObject {
        let object: &NSObject = self;
        object
    }

    /// Whether a window notification came from the debug view (rather than the
    /// browser window, whose delegate this also is).
    fn is_debug_window(&self, notification: &NSNotification) -> bool {
        let Some(object) = notification.object() else {
            return false;
        };
        let debug = self.ivars().debug.borrow();
        let Some(debug) = debug.as_ref() else {
            return false;
        };
        std::ptr::eq(
            Retained::as_ptr(&object).cast::<AnyObject>(),
            Retained::as_ptr(&debug.window).cast::<AnyObject>(),
        )
    }

    /// Repaint the chrome from the shell's current `ChromeState`, through the
    /// shared derivation.
    fn refresh_chrome(&self) {
        let paint = {
            let shell = self.ivars().shell.borrow();
            ChromePaint::of(shell.chrome())
        };
        self.ivars().chrome.apply(&paint);
    }

    /// Catch the open debug view up with the shared store (a no-op when it is
    /// closed, and a no-op tick when nothing was captured).
    fn refresh_debug_view(&self) {
        let mtm = MainThreadMarker::from(self);
        let debug = self.ivars().debug.borrow();
        let Some(debug) = debug.as_ref() else {
            return;
        };
        let capture = &self.ivars().capture;

        let refresh = console_refresh(
            capture,
            debug.console.rendered.get(),
            debug.console.last_sequence.get(),
        );
        debug.console.apply(mtm, refresh.update, console_row);
        debug.console.rendered.set(refresh.rendered_rows);
        debug.console.last_sequence.set(refresh.last_sequence);

        let refresh = network_refresh(
            capture,
            debug.network.rendered.get(),
            debug.network.last_sequence.get(),
        );
        debug.network.apply(mtm, refresh.update, network_row);
        debug.network.rendered.set(refresh.rendered_rows);
        debug.network.last_sequence.set(refresh.last_sequence);
    }

    /// One pump tick: the seam's events into the chrome, then the debug view.
    fn tick(&self) {
        if self.ivars().shell.borrow_mut().pump() {
            self.refresh_chrome();
        }
        // The capture store changes off the seam's load events, so a `pump()`
        // that returned false does not mean the store is unchanged; the refresh
        // is incremental, so an idle tick over an open view is one sequence
        // comparison.
        self.refresh_debug_view();
    }

    /// Open (or raise) the debug view.
    fn open_debug_view(&self) {
        let mtm = MainThreadMarker::from(self);
        if let Some(debug) = self.ivars().debug.borrow().as_ref() {
            debug.window.makeKeyAndOrderFront(None);
            return;
        }
        let debug = DebugWindow::new(mtm);
        debug
            .window
            .setDelegate(Some(ProtocolObject::from_ref(self)));
        // SAFETY: this controller implements `clearDebugCapture:`.
        unsafe {
            debug.clear.setTarget(Some(self.as_target()));
            debug.clear.setAction(Some(sel!(clearDebugCapture:)));
        }
        *self.ivars().debug.borrow_mut() = Some(debug);
        // Paint what was captured so far BEFORE presenting, so the window never
        // opens visibly empty when there are already entries.
        self.refresh_debug_view();
        if let Some(debug) = self.ivars().debug.borrow().as_ref() {
            debug.window.makeKeyAndOrderFront(None);
        }
    }
}

/// Build the ⋮ menu from the core's items: an `Info` item is a DISABLED entry
/// (the `werust <version>` line), an `Action` item is a live one dispatched by
/// its index-tag back to its stable id.
///
/// A FUTURE core menu item therefore needs no change here at all unless it is an
/// action with new behaviour — the "structured to grow" property, expressed in
/// code, exactly as the GTK popover expresses it.
fn build_browser_menu(mtm: MainThreadMarker, items: &[MenuItemPaint]) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // AppKit otherwise auto-disables items by asking the target whether it
    // responds; the enabled state here is the CORE's item kind, not AppKit's
    // guess.
    menu.setAutoenablesItems(false);
    for (index, item) in items.iter().enumerate() {
        // SAFETY: the action is set (with a target) in `wire_actions`; an item
        // with no action yet is valid.
        let entry = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &ns(&item.label),
                None,
                &ns(""),
            )
        };
        entry.setEnabled(item.activatable);
        entry.setTag(index as isize);
        menu.addItem(&entry);
    }
    menu
}

/// Point every control at the controller. Done after construction because the
/// widgets must exist before the controller that owns them, and an
/// Objective-C object can be re-targeted freely afterwards.
fn wire_actions(controller: &WindowController) {
    let chrome = &controller.ivars().chrome;
    let target = controller.as_target();
    // SAFETY: the controller implements every selector named here (they are
    // defined in its `define_class!` block above).
    unsafe {
        for (control, action) in [
            (&chrome.back, sel!(goBack:)),
            (&chrome.forward, sel!(goForward:)),
            (&chrome.reload, sel!(reloadPage:)),
            (&chrome.stop, sel!(stopLoading:)),
            (&chrome.menu_button, sel!(showBrowserMenu:)),
        ] {
            control.setTarget(Some(target));
            control.setAction(Some(action));
        }
        chrome.url_field.setTarget(Some(target));
        chrome.url_field.setAction(Some(sel!(urlEntered:)));
        for item in chrome.menu.itemArray().iter() {
            if item.isEnabled() {
                item.setTarget(Some(target));
                item.setAction(Some(sel!(browserMenuItemChosen:)));
            }
        }
    }
}

/// Install the application's MENU BAR: the conventional macOS app menu, carrying
/// the SAME core `BrowserMenu` items the ⋮ button shows plus the platform's own
/// Quit.
///
/// macOS has real menu-bar conventions and werust uses them — but the menu's
/// CONTENT is still the shared core's, so the menu bar and the ⋮ button cannot
/// disagree and neither is a hand-written macOS list.
fn install_main_menu(mtm: MainThreadMarker, controller: &WindowController) {
    let main = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = build_browser_menu(mtm, &controller.ivars().menu_items);
    // SAFETY: the controller implements `browserMenuItemChosen:`.
    unsafe {
        for item in app_menu.itemArray().iter() {
            if item.isEnabled() {
                item.setTarget(Some(controller.as_target()));
                item.setAction(Some(sel!(browserMenuItemChosen:)));
            }
        }
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        // Quit is AppKit's own responder-chain action, not werust's.
        let quit = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &ns("Quit werust"),
            Some(sel!(terminate:)),
            &ns("q"),
        );
        app_menu.addItem(&quit);
    }
    app_item.setSubmenu(Some(&app_menu));
    main.addItem(&app_item);
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
}

/// The macOS browser window: the product surface, over the shared shell.
///
/// Construction is separate from `NSApplication::run` on purpose: the CI smoke
/// (`examples/window_smoke.rs`) builds a real window, pumps it by hand and
/// asserts what the real widgets show, which is the only way this file gets
/// EXECUTED anywhere before a human opens it.
pub struct BrowserWindow {
    controller: Retained<WindowController>,
}

/// Where a freshly opened window is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Centred on the user's screen: the product path.
    OnScreen,
    /// Far off-screen, ordered back: the CI smoke's path, so a run shows nothing
    /// and steals no focus (the same discipline the engine's
    /// `host_in_bare_window` uses).
    OffScreen,
}

impl BrowserWindow {
    /// Build the window over an already-wired shell and capture store.
    ///
    /// `shell` must already own the backend with its trust hooks installed; this
    /// function only paints and forwards.
    pub fn open(
        mtm: MainThreadMarker,
        shell: Rc<RefCell<BrowserShell>>,
        capture: DebugCapture,
        placement: Placement,
    ) -> Self {
        let size = DEFAULT_WINDOW_SIZE;
        let frame = match placement {
            Placement::OnScreen => rect(0.0, 0.0, size.width, size.height),
            Placement::OffScreen => rect(-20_000.0, -20_000.0, size.width, size.height),
        };
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(&ns("werust"));
        window.setMinSize(NSSize::new(520.0, 320.0));
        // A programmatically created `NSWindow` defaults to releasing itself when
        // closed, which is incompatible with holding it as a `Retained` (the
        // controller and this struct both do): closing it would leave both
        // pointing at a freed object. Ownership stays with Rust.
        unsafe { window.setReleasedWhenClosed(false) };

        let content = FlippedView::new(mtm, rect(0.0, 0.0, size.width, size.height));
        window.setContentView(Some(&content));

        // The live page view: the seam hands over an opaque pointer to the
        // backend's container `NSView`, which this window embeds without knowing
        // it is a WKWebView host.
        let handle = shell.borrow().view_handle();
        // SAFETY: `view_handle()` returns a live `NSView` the backend owns for
        // the shell to embed; the backend outlives the window (the shell holds
        // it), and `retain` takes its own reference rather than consuming the
        // backend's.
        let page: Retained<NSView> = unsafe { Retained::retain(handle.0.cast::<NSView>()) }
            .expect("the seam's view handle is a live NSView");
        content.addSubview(&page);

        let toolbar = FlippedView::new(mtm, rect(0.0, 0.0, size.width, TOOLBAR_HEIGHT));
        content.addSubview(&toolbar);

        let back = button(mtm, "◀");
        let forward = button(mtm, "▶");
        let reload = button(mtm, "⟳");
        let stop = button(mtm, "✕");
        let menu_button = button(mtm, "⋮");
        for control in [&back, &forward, &reload, &stop, &menu_button] {
            toolbar.addSubview(control);
        }

        let url_field = NSTextField::new(mtm);
        url_field.setEditable(true);
        url_field.setBezeled(true);
        url_field.setPlaceholderString(Some(&ns("Enter a URL and press Enter")));
        toolbar.addSubview(&url_field);

        let progress = NSProgressIndicator::new(mtm);
        progress.setStyle(NSProgressIndicatorStyle::Bar);
        progress.setIndeterminate(false);
        progress.setMinValue(0.0);
        progress.setMaxValue(1.0);
        progress.setHidden(true);
        toolbar.addSubview(&progress);

        let invalid_badge = label(mtm, "");
        invalid_badge.setTextColor(Some(&color(INVALID_ENTRY_COLOR)));
        invalid_badge.setHidden(true);
        toolbar.addSubview(&invalid_badge);

        let trust = label(mtm, "");
        toolbar.addSubview(&trust);

        // The error banner sits directly under the toolbar and ABOVE the page
        // view, so a failed load's reason is unmissable rather than buried in the
        // footer status line.
        let error_banner = label(mtm, "");
        error_banner.setHidden(true);
        content.addSubview(&error_banner);
        let status = label(mtm, "");
        content.addSubview(&status);

        let items = menu_items();
        let menu = build_browser_menu(mtm, &items);

        let chrome = Chrome {
            window: window.clone(),
            content,
            toolbar,
            back,
            forward,
            reload,
            stop,
            menu_button,
            url_field,
            progress,
            invalid_badge,
            trust,
            error_banner,
            status,
            page,
            menu,
            banner_visible: Cell::new(false),
            badge_visible: Cell::new(false),
        };

        let controller = WindowController::new(mtm, shell, capture, chrome, items);
        wire_actions(&controller);
        window.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
        controller.ivars().chrome.relayout();
        controller.refresh_chrome();

        match placement {
            Placement::OnScreen => {
                window.center();
                window.makeKeyAndOrderFront(None);
            }
            // A window WebKit will render into, without raising it over anything
            // the user is doing.
            Placement::OffScreen => window.orderBack(None),
        }

        Self { controller }
    }

    /// Start the 50ms chrome pump on the run loop (the product path; the smoke
    /// pumps by hand instead).
    pub fn start_pump(&self) {
        // SAFETY: the controller implements `pumpTick:` and outlives the timer
        // (this window holds it, and the caller holds the window for the run).
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                PUMP_INTERVAL,
                self.controller.as_target(),
                sel!(pumpTick:),
                None,
                true,
            )
        };
        *self.controller.ivars().timer.borrow_mut() = Some(timer);
    }

    /// Run ONE pump tick by hand (the CI smoke's entry point).
    pub fn tick(&self) {
        self.controller.tick();
    }

    /// Give the page view the keyboard focus, so AppKit routes scroll / click /
    /// typing to the live page.
    pub fn focus_page(&self) {
        self.controller.ivars().shell.borrow_mut().focus_page(true);
    }

    /// Open the debug view, exactly as the ⋮ menu's Debug entry does.
    pub fn open_debug_view(&self) {
        self.controller.open_debug_view();
    }

    /// Close the debug view, as its own close button does (the CI smoke drives
    /// the same path a user does, so the slot-clearing delegate is exercised).
    pub fn close_debug_view(&self) {
        let window = self
            .controller
            .ivars()
            .debug
            .borrow()
            .as_ref()
            .map(|debug| debug.window.clone());
        if let Some(window) = window {
            window.close();
        }
    }

    /// The window itself (the smoke closes it; the product hands it to AppKit).
    #[must_use]
    pub fn window(&self) -> Retained<NSWindow> {
        self.controller.ivars().chrome.window.clone()
    }

    /// What the URL bar currently SHOWS.
    #[must_use]
    pub fn url_text(&self) -> String {
        self.controller
            .ivars()
            .chrome
            .url_field
            .stringValue()
            .to_string()
    }

    /// What the trust indicator currently SHOWS.
    #[must_use]
    pub fn trust_text(&self) -> String {
        self.controller
            .ivars()
            .chrome
            .trust
            .stringValue()
            .to_string()
    }

    /// The trust indicator's explanation tooltip (`docs/adr/0006`: the badge is
    /// self-explaining on hover).
    #[must_use]
    pub fn trust_detail(&self) -> Option<String> {
        self.controller
            .ivars()
            .chrome
            .trust
            .toolTip()
            .map(|t| t.to_string())
    }

    /// What the status line currently SHOWS.
    #[must_use]
    pub fn status_text(&self) -> String {
        self.controller
            .ivars()
            .chrome
            .status
            .stringValue()
            .to_string()
    }

    /// The error banner's text when it is VISIBLE, [`None`] when it is hidden —
    /// so a caller cannot mistake a stale string for a shown banner.
    #[must_use]
    pub fn error_banner(&self) -> Option<String> {
        let banner = &self.controller.ivars().chrome.error_banner;
        (!banner.isHidden()).then(|| banner.stringValue().to_string())
    }

    /// Whether the invalid-entry badge is showing.
    #[must_use]
    pub fn invalid_badge_visible(&self) -> bool {
        !self.controller.ivars().chrome.invalid_badge.isHidden()
    }

    /// The page view's frame, so the smoke can prove that in-flight progress does
    /// NOT displace the page while a failure banner does.
    #[must_use]
    pub fn page_frame(&self) -> NSRect {
        self.controller.ivars().chrome.page.frame()
    }

    /// The ⋮ menu's item titles, in order, as AppKit holds them.
    #[must_use]
    pub fn menu_titles(&self) -> Vec<String> {
        self.controller
            .ivars()
            .chrome
            .menu
            .itemArray()
            .iter()
            .map(|item| item.title().to_string())
            .collect()
    }

    /// The row counts of the open debug view's two tabs, or [`None`] when it is
    /// closed.
    #[must_use]
    pub fn debug_row_counts(&self) -> Option<(usize, usize)> {
        self.controller
            .ivars()
            .debug
            .borrow()
            .as_ref()
            .map(|debug| (debug.console.row_count(), debug.network.row_count()))
    }

    /// Activate the ⋮ menu item with this core id, exactly as choosing it does.
    /// The smoke drives the DEBUG entry through it, so the menu's dispatch (by
    /// stable id, never by label) is exercised rather than bypassed.
    pub fn activate_menu_item(&self, id: &str) -> bool {
        let Some(index) = self
            .controller
            .ivars()
            .menu_items
            .iter()
            .position(|item| item.id == id)
        else {
            return false;
        };
        let menu = &self.controller.ivars().chrome.menu;
        let Some(item) = menu.itemArray().iter().nth(index) else {
            return false;
        };
        // The item's target/action were wired by `wire_actions`; this performs
        // the same message AppKit would on a click.
        menu.performActionForItemAtIndex(index as isize);
        item.isEnabled()
    }
}

/// Build the whole macOS shell over the WKWebView backend and RUN it: the
/// product entry point.
///
/// The construction order mirrors the GTK shell's `open_window` exactly, because
/// the constraints are the same shared ones: the trust hooks are installed on the
/// backend BEFORE it is boxed behind the seam (and, on macOS, before the first
/// navigation realises the `WKWebView`, since `WKWebViewConfiguration` fixes the
/// scheme set at that point), the redirect sink and the capture store are handed
/// to BOTH the backend and the shell so each side sees the same one, and the
/// window is then a painter over the result.
///
/// ADR-0010 (`target="_blank"` / `window.open` navigates in place) needs no call
/// here: the backend's own `WKUIDelegate` routes it through the shared
/// `renderer::new_window_action`, so this window neither opens a second window
/// nor re-decides the rule. ADR-0009 (follow the OS colour scheme) likewise needs
/// no call: AppKit propagates the effective appearance into the chrome and the
/// web process, and forcing one is exactly what the ADR forbids.
pub fn run(url: &str) -> Result<(), RendererError> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        RendererError::Backend("the macOS shell must be started on the main thread".into())
    })?;

    let mut backend = macos_renderer::MacosRenderer::new()?;
    // Trust hook 1: the native EIP-1193 provider over the script-message bridge.
    backend.install_provider();
    // Trust hook 2: native `ipfs://` resolution through the hash-verified core
    // path. It hands back the `_redirects` 3xx sink the shell drains on its pump.
    let redirects = backend.install_ipfs();
    // The debug CAPTURE POINTS, on the same store the debug view renders.
    let capture = DebugCapture::new();
    install_debug_capture(&mut backend, capture.clone());

    let shell = Rc::new(RefCell::new(
        BrowserShell::new(Box::new(backend))
            .with_redirect_sink(redirects)
            .with_debug_capture(capture.clone()),
    ));

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let window = BrowserWindow::open(mtm, shell.clone(), capture, Placement::OnScreen);
    install_main_menu(mtm, &window.controller);

    // Navigate through the seam and focus the live view, so the OS routes
    // scroll/click/focus/keyboard input to the page.
    shell.borrow_mut().navigate(url)?;
    window.focus_page();
    window.tick();
    window.start_pump();

    app.activate();
    app.run();
    Ok(())
}
