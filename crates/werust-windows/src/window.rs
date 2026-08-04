//! The Win32 window: the top-level window, its message loop and the actions it
//! forwards to the shared [`BrowserShell`].
//!
//! This module and its two neighbours ([`chrome`](crate::chrome),
//! [`debugview`](crate::debugview)) are the only Win32 code in werust, and they
//! are deliberately the DUMBEST code in it. Every value they paint comes from
//! [`crate::paint`] (the shared carrier over `werust-core`); every user action
//! they receive is handed to the shell. They decide nothing about browsing,
//! trust or wording.
//!
//! # The window
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ ◀ ▶ ⟳/✕ ◐ │ URL bar (+ progress) │ ⛔ badge │ trust badge │ ⋮ │  toolbar
//! ├─────────────────────────────────────────────────────────────┤
//! │ ⚠ This page failed to load: <protocol-named reason>         │  error banner
//! ├─────────────────────────────────────────────────────────────┤  (failures only)
//! │                                                             │
//! │                  the WebView2 page window                   │
//! │                                                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │ loading… — fetching content                                 │  status line
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # ADR-0009: following the OS, on a toolkit that does not
//!
//! AppKit propagates the user's appearance into every control for free, so the
//! macOS window follows the OS by NOT acting. Win32 does not. So this window
//! READS the OS setting -- through the ONE reader that already exists, in the
//! engine crate beside the rest of the platform bindings, mapped by the shared
//! `renderer::OsColorScheme` rule -- paints its own surfaces to match, re-reads it
//! on `WM_SETTINGCHANGE`, and NEVER forces dark: `NoPreference` paints light,
//! exactly as the shared rule says. The PAGE's colour scheme is not touched here
//! at all; the engine gives WebView2 `PREFERRED_COLOR_SCHEME_AUTO`, which follows
//! the same OS setting from a single source.
//!
//! # ADR-0010: what this file does NOT do
//!
//! It never opens a second browser window. A `target="_blank"` / `window.open`
//! navigates IN PLACE until tabs exist, and that rule is the ENGINE's
//! `add_NewWindowRequested` hook over the SHARED `renderer::new_window_action`.
//! The only other window here is the debug view, which hosts no page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{SetBkColor, SetTextColor, HBRUSH, HDC, HFONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, SetWindowTheme, ICC_BAR_CLASSES, ICC_LISTVIEW_CLASSES,
    ICC_PROGRESS_CLASS, ICC_TAB_CLASSES, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, NMHDR,
    NMLVCUSTOMDRAW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_F12, VK_F5,
    VK_LEFT, VK_LWIN, VK_MENU, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

use renderer::{OsColorScheme, RendererError};
use werust_core::debug::DebugCapture;
use werust_core::menu::MENU_ITEM_DEBUG;
use werust_core::shortcuts::{self, ChromeAction, Focus};
use werust_core::BrowserShell;

use crate::chrome::{Chrome, PBM_SETBARCOLOR, PBM_SETRANGE32};
use crate::debugview::{
    add_tab, current_tab, DebugTab, DebugWindow, NETWORK_TRUST_COLUMN, TAB_CONSOLE, TAB_NETWORK,
};
use crate::dpi::{Metrics, DEFAULT_HEIGHT, DEFAULT_WIDTH};
use crate::paint::{
    console_refresh, install_debug_capture, menu_items, network_refresh, ChromePaint,
    MenuItemPaint, INVALID_ENTRY_COLOR, LOAD_PROGRESS_COLOR,
};
use crate::shortcuts::{app_command_pointer_button, shortcut_action, shortcut_pointer_button};
use crate::win32::{
    client_rect, colorref, control_rect, is_visible, place, release_font, set_font, set_text, show,
    ui_font, wide, window_dpi, window_text, Theme,
};

/// The chrome pump cadence, in milliseconds: the same 50ms the GTK and AppKit
/// shells use.
const PUMP_INTERVAL_MS: u32 = 50;
/// The pump timer's id.
const PUMP_TIMER: usize = 1;

/// Win32 control constants the `windows` crate does not generate (window STYLES
/// and notification codes are `#define`s in the SDK headers, not typed enums, so
/// the bindings omit them). Spelled here, once, with the header they come from,
/// rather than scattered as bare numbers.
///
/// `commctrl.h`: the list-view report mode and its notification codes.
const LVS_REPORT: u32 = 0x0001;
const LVS_SINGLESEL: u32 = 0x0004;
const LVS_NOSORTHEADER: u32 = 0x8000;
/// `NM_FIRST - 12`, the custom-draw notification (the only way a list view
/// paints one row in its own colour).
const NM_CUSTOMDRAW: u32 = 0u32.wrapping_sub(12);
/// `TCN_FIRST - 1`, the tab control's selection change.
const TCN_SELCHANGE: u32 = 0u32.wrapping_sub(551);
/// `commctrl.h`: a tooltip that shows even when its owner is inactive, and that
/// does not eat `&` as a mnemonic.
const TTS_ALWAYSTIP: u32 = 0x01;
const TTS_NOPREFIX: u32 = 0x02;
/// `winuser.h`: a single-line STATIC that clips rather than wraps.
const SS_LEFTNOWORDWRAP: u32 = 0x0000_000c;
/// `winuser.h`: a STATIC that centres its text in its own rectangle. The spinner
/// is one glyph in a fixed slot, and its frames are not all the same width, so
/// centring is what keeps it from jittering as it turns.
const SS_CENTER: u32 = 0x0000_0001;
/// `winuser.h`: the EDIT control's "select this range" message. Selecting the
/// whole address is half of what the focus-the-URL-bar shortcut means ("focus
/// AND select", so the next keystroke replaces it).
const EM_SETSEL: u32 = 0x00b1;

/// werust's OWN message: perform one [`ChromeAction`], carried as its slot in
/// [`ChromeAction::ALL`].
///
/// It exists for exactly one caller, and for a documented platform reason: a key
/// pressed while the PAGE has the focus reaches this shell as a WebView2
/// `AcceleratorKeyPressed` callback that runs SYNCHRONOUSLY with the browser
/// process blocked, so a COM call back into WebView2 from inside it fails with
/// `RPC_E_CANTCALLOUT_ININPUTSYNCCALL`. Every chrome action ends in such a call,
/// so that path POSTS the action here and this window performs it on its own
/// message loop a moment later, exactly as Microsoft's own sample does.
const WM_WERUST_CHROME_ACTION: u32 = WM_APP + 1;

/// The virtual-key codes the pure translation ([`crate::shortcuts`]) spells out
/// as plain `u16`, checked against the SDK's own `VK_*` at COMPILE time.
///
/// The translation must stay host-independent so the Ubuntu gate can test it,
/// which means it cannot use the `windows` crate's constants -- and a mistyped
/// virtual-key code is a shortcut that silently never fires. This is the cheapest
/// possible guard: on Windows, where both spellings exist, they must agree.
const _VIRTUAL_KEY_CODES_MATCH_THE_SDK: () = {
    assert!(crate::shortcuts::VK_SHIFT == VK_SHIFT.0);
    assert!(crate::shortcuts::VK_CONTROL == VK_CONTROL.0);
    assert!(crate::shortcuts::VK_MENU == VK_MENU.0);
    assert!(crate::shortcuts::VK_ESCAPE == VK_ESCAPE.0);
    assert!(crate::shortcuts::VK_LEFT == VK_LEFT.0);
    assert!(crate::shortcuts::VK_RIGHT == VK_RIGHT.0);
    assert!(crate::shortcuts::VK_LWIN == VK_LWIN.0);
    assert!(crate::shortcuts::VK_RWIN == VK_RWIN.0);
    assert!(crate::shortcuts::VK_F5 == VK_F5.0);
    assert!(crate::shortcuts::VK_F12 == VK_F12.0);
};

/// Control ids. A click arrives as `WM_COMMAND` carrying one of these in the low
/// word of `wParam`; the ⋮ menu's items are [`ID_MENU_BASE`] + their index in the
/// CORE's item list, so dispatch stays keyed to the core's order and then to its
/// STABLE id -- never to a label.
const ID_BACK: usize = 101;
const ID_FORWARD: usize = 102;
/// The ONE reload/stop control, where there were two (task
/// `reload-stop-collapse-and-spinner-on-the-windows-chrome`). Its MODE, and
/// therefore what a click on it does, is the shared derivation's.
const ID_RELOAD_STOP: usize = 103;
const ID_MENU_BUTTON: usize = 105;
const ID_URL_EDIT: usize = 106;
const ID_URL_ENTER: usize = 107;
const ID_DEBUG_CLEAR: usize = 108;
const ID_MENU_BASE: usize = 2000;

/// The browser window's class name.
const WINDOW_CLASS: PCWSTR = w!("werust_win32_window");
/// The debug view's class name.
const DEBUG_CLASS: PCWSTR = w!("werust_win32_debug");

/// Where a freshly opened window is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// On the user's screen: the product path.
    OnScreen,
    /// Far off-screen, unactivated: the CI smoke's path, so a run shows nothing
    /// and steals no focus (the same discipline the engine's
    /// `host_in_bare_window` uses).
    OffScreen,
}

/// Everything the window owns. One `Rc`, whose pointer both windows carry in
/// `GWLP_USERDATA` so a window proc can find it.
struct Controller {
    /// The SHARED shell: every control drives this, never the webview.
    shell: Rc<RefCell<BrowserShell>>,
    /// The capture store the debug view renders (the same handle the capture
    /// points push into).
    capture: DebugCapture,
    chrome: Chrome,
    /// The core's menu items, in the order the `HMENU` lists them; a chosen item
    /// is dispatched by its STABLE id, found by the command id's offset.
    menu_items: Vec<MenuItemPaint>,
    /// The platform's OWN devtools, opened by the chrome (never re-implemented).
    dev_tools: windows_renderer::DevTools,
    /// The open debug view, if any (re-activating Debug raises it rather than
    /// opening a second copy).
    debug: RefCell<Option<DebugWindow>>,
    /// The OS colour scheme this chrome is currently painted for (`docs/adr/0009`).
    scheme: Cell<OsColorScheme>,
    theme: RefCell<Theme>,
    /// The chrome's font, shared by every control.
    font: Cell<isize>,
}

impl Controller {
    /// Repaint the chrome from the shell's current `ChromeState`, through the
    /// shared derivation.
    fn refresh_chrome(&self) {
        let paint = {
            let shell = self.shell.borrow();
            ChromePaint::of(shell.chrome())
        };
        self.chrome.apply(&paint);
    }

    /// What the ONE reload/stop control does RIGHT NOW: the mode's own
    /// [`ChromeAction`], taken off the shared snapshot.
    ///
    /// The mode is `reload_stop_control`'s call, made once in `werust-core` for
    /// every edge and carried here by `desktop-paint`; this edge asks the
    /// snapshot which action the control it just painted performs, and never
    /// re-decides it from the loading fact.
    fn reload_stop_action(&self) -> ChromeAction {
        let shell = self.shell.borrow();
        ChromePaint::of(shell.chrome()).reload_stop_control.action()
    }

    /// Catch the open debug view up with the shared store (a no-op when it is
    /// closed, and a no-op tick when nothing was captured).
    fn refresh_debug_view(&self) {
        let debug = self.debug.borrow();
        let Some(debug) = debug.as_ref() else {
            return;
        };

        let refresh = console_refresh(
            &self.capture,
            debug.console.rendered.get(),
            debug.console.last_sequence.get(),
        );
        debug.console.apply_console(refresh.update);
        debug.console.rendered.set(refresh.rendered_rows);
        debug.console.last_sequence.set(refresh.last_sequence);

        let refresh = network_refresh(
            &self.capture,
            debug.network.rendered.get(),
            debug.network.last_sequence.get(),
        );
        debug.network.apply_network(refresh.update);
        debug.network.rendered.set(refresh.rendered_rows);
        debug.network.last_sequence.set(refresh.last_sequence);
    }

    /// One pump tick: the seam's events into the chrome, then the debug view.
    fn tick(&self) {
        if self.shell.borrow_mut().pump() {
            self.refresh_chrome();
        }
        // The capture store changes off the seam's load events, so a `pump()`
        // that returned false does not mean the store is unchanged; the refresh
        // is incremental, so an idle tick over an open view is one sequence
        // comparison.
        self.refresh_debug_view();
        // The spinner turns on THIS tick rather than on a repaint: a repaint
        // happens when the seam reports an event, and the load a spinner is for is
        // the one that reports nothing for seconds. Whether it is showing at all
        // was decided by the last paint, from the core's rule.
        self.chrome.spin();
    }

    /// `WM_DPICHANGED`: this window was dragged onto a monitor with a different
    /// scale (or the user changed the scale of the one it is on).
    ///
    /// `app.manifest` declares `PerMonitorV2`, so Windows re-scales NOTHING for
    /// this process: the whole response is ours. Honour the rect Windows
    /// SUGGESTS, recreate the font at the new size (a `CreateFontW` height is
    /// fixed at creation), push it to every control, delete the old one, and
    /// re-run the layout from the new metrics. Without this the window is
    /// correct only on the monitor it opened on, which on a laptop-plus-external
    /// desk is most of the time.
    fn dpi_changed(&self, window: HWND, wparam: WPARAM, lparam: LPARAM) {
        // The new scale rides on the message: the low word is the X-axis DPI
        // (the high word is the Y-axis one, which is the same on every Windows
        // display). Taking it from here rather than re-reading the window means
        // the layout below cannot race the move.
        self.chrome
            .dpi
            .set(u32::try_from(wparam.0 & 0xffff).unwrap_or(0));
        // `lParam` is a RECT: the same window, sized and positioned for the new
        // scale. Windows asks that it be honoured exactly, and a window that
        // does not is left straddling the monitor boundary.
        let suggested = unsafe { *(lparam.0 as *const windows::Win32::Foundation::RECT) };
        unsafe {
            let _ = SetWindowPos(
                window,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.rescale_font(self.chrome.metrics());
        self.chrome.relayout();
    }

    /// Recreate the UI font at `metrics`' height and give it to every control.
    ///
    /// The height of an `HFONT` is fixed when it is created, so a DPI change
    /// needs a NEW one pushed with `WM_SETFONT` ([`set_font`]) rather than an
    /// adjustment. The old one is deleted AFTER every control has taken the
    /// replacement — the same `DeleteObject` discipline
    /// [`Theme::release`](crate::win32::Theme::release) applies to its brushes,
    /// because a font leaked on every monitor drag is a real GDI leak.
    fn rescale_font(&self, metrics: Metrics) {
        let replacement = ui_font(metrics.font_height);
        if replacement.is_invalid() {
            return;
        }
        let previous = HFONT(self.font.get() as *mut _);
        self.font.set(replacement.0 as isize);
        for control in self.chrome.controls() {
            set_font(control, replacement);
        }
        // The debug view, when it is open, wears the same chrome font.
        if let Some(debug) = self.debug.borrow().as_ref() {
            for control in debug.controls() {
                set_font(control, replacement);
            }
            relayout_debug_window_of(debug);
        }
        release_font(previous);
    }

    /// A control's colours at paint time. Win32 asks the PARENT for them, which
    /// is why every coloured surface is answered here rather than set on the
    /// control.
    fn control_colors(&self, hdc: HDC, control: HWND) -> HBRUSH {
        let theme = self.theme.borrow();
        let chrome = &self.chrome;
        unsafe {
            if control == chrome.error_banner {
                // The banner is the severity's FILL with white text -- the same
                // split the GTK `APP_CSS` rules make.
                SetTextColor(hdc, COLORREF(0x00ff_ffff));
                SetBkColor(hdc, chrome.error_color.get());
                return chrome.error_brush.get();
            }
            if control == chrome.trust {
                SetTextColor(hdc, chrome.trust_color.get());
            } else if control == chrome.invalid_badge {
                SetTextColor(hdc, colorref(INVALID_ENTRY_COLOR));
            } else if control == chrome.url_edit {
                SetTextColor(
                    hdc,
                    if chrome.url_invalid.get() {
                        colorref(INVALID_ENTRY_COLOR)
                    } else {
                        theme.field_text
                    },
                );
                SetBkColor(hdc, theme.field_back);
                return theme.field;
            } else {
                SetTextColor(hdc, theme.text);
            }
            SetBkColor(hdc, theme.back);
        }
        theme.background
    }

    /// Re-read the OS colour scheme and repaint (`WM_SETTINGCHANGE`).
    fn follow_os_color_scheme(&self) {
        let scheme = windows_renderer::os_color_scheme();
        if scheme == self.scheme.get() {
            return;
        }
        self.scheme.set(scheme);
        let replacement = Theme::of(scheme);
        let previous = self.theme.replace(replacement);
        previous.release();
        apply_title_bar_theme(self.chrome.window, scheme);
        unsafe {
            let _ =
                windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.chrome.window), None, true);
        }
    }

    /// Open (or raise) the debug view.
    fn open_debug_view(self: &Rc<Self>) {
        if let Some(debug) = self.debug.borrow().as_ref() {
            unsafe {
                let _ = ShowWindow(debug.window, SW_SHOW);
                let _ = SetForegroundWindow(debug.window);
            }
            return;
        }
        let debug = build_debug_window(self);
        *self.debug.borrow_mut() = Some(debug);
        // Paint what was captured so far BEFORE presenting, so the window never
        // opens visibly empty when there are already entries.
        self.refresh_debug_view();
        if let Some(debug) = self.debug.borrow().as_ref() {
            unsafe {
                let _ = ShowWindow(debug.window, SW_SHOWNOACTIVATE);
            }
        }
    }

    /// Which of the two focus contexts the shared resolution distinguishes is
    /// live, REPORTED as an input to it.
    ///
    /// Escape means "stop the load" over the page and "revert my edit" in the URL
    /// bar, and the core decides which; this edge only answers the one question
    /// it is asked. Everything that is not the URL bar -- the page, a toolbar
    /// button, the window itself -- is [`Focus::Page`], so no widget tree is
    /// classified here.
    fn focus_context(&self) -> Focus {
        if unsafe { GetFocus() } == self.chrome.url_edit {
            Focus::UrlBar
        } else {
            Focus::Page
        }
    }

    /// A ⋮ menu item was chosen: dispatch on the core item's STABLE id, never the
    /// display label.
    fn menu_item_chosen(self: &Rc<Self>, index: usize) {
        let Some(chosen) = self.menu_items.get(index) else {
            return;
        };
        if chosen.id == MENU_ITEM_DEBUG {
            self.open_debug_view();
        }
    }
}

/// Follow the OS colour scheme on the title bar too, which is drawn by the
/// system rather than by this window (`docs/adr/0009`). Best-effort: an older
/// Windows 10 without the attribute keeps its default rather than failing.
fn apply_title_bar_theme(window: HWND, scheme: OsColorScheme) {
    let dark = windows::core::BOOL::from(scheme.prefer_dark());
    unsafe {
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::from_ref(&dark).cast(),
            u32::try_from(std::mem::size_of::<windows::core::BOOL>()).unwrap_or(4),
        );
    }
}

/// The controller behind a window handle, if it has one.
fn controller_of(window: HWND) -> Option<Rc<Controller>> {
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if raw == 0 {
        return None;
    }
    // BORROWED: the `BrowserWindow` owns the `Rc`; this clone must not consume
    // the stored one.
    let controller = unsafe { Rc::from_raw(raw as *const Controller) };
    let cloned = Rc::clone(&controller);
    std::mem::forget(controller);
    Some(cloned)
}

/// The browser window's procedure.
unsafe extern "system" fn wndproc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(controller) = controller_of(window) else {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    };
    match message {
        WM_SIZE => {
            controller.chrome.relayout();
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == PUMP_TIMER => {
            controller.tick();
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
            let brush =
                controller.control_colors(HDC(wparam.0 as *mut _), HWND(lparam.0 as *mut _));
            LRESULT(brush.0 as isize)
        }
        WM_SETTINGCHANGE => {
            controller.follow_os_color_scheme();
            LRESULT(0)
        }
        WM_DPICHANGED => {
            controller.dpi_changed(window, wparam, lparam);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xffff;
            handle_command(&controller, id);
            LRESULT(0)
        }
        // The mouse's side buttons ("4 and 5"), over this window's OWN chrome.
        // What they MEAN is the shared resolution's; this reads WHICH button off
        // the message.
        WM_XBUTTONDOWN => {
            let claimed = perform_pointer_button(&controller, shortcut_pointer_button(wparam.0));
            // TRUE is what an X button message expects from a window that took
            // it.
            LRESULT(isize::from(claimed))
        }
        // The matching release is swallowed so `DefWindowProc` does not ALSO turn
        // this one click into a `WM_APPCOMMAND` below, which would navigate
        // twice.
        WM_XBUTTONUP => LRESULT(1),
        // The other route the same buttons take: a click that landed on a CHILD
        // window (WebView2's page window is one, and lives in another process)
        // reaches this window as an app command instead. Also what a keyboard's
        // dedicated Back/Forward keys send.
        WM_APPCOMMAND => {
            if perform_pointer_button(&controller, app_command_pointer_button(lparam.0)) {
                LRESULT(1)
            } else {
                unsafe { DefWindowProcW(window, message, wparam, lparam) }
            }
        }
        // An action the page's own keystroke resolved to, performed HERE rather
        // than inside WebView2's synchronous callback.
        WM_WERUST_CHROME_ACTION => {
            if let Some(action) = posted_chrome_action(wparam) {
                perform_chrome_action(&controller, action);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(window), PUMP_TIMER);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Every chrome action drives the SHARED shell through the seam and then
/// repaints; none of them touches the webview directly.
fn handle_command(controller: &Rc<Controller>, id: usize) {
    match id {
        ID_BACK => controller.shell.borrow_mut().go_back(),
        ID_FORWARD => controller.shell.borrow_mut().go_forward(),
        ID_RELOAD_STOP => {
            // The ONE reload/stop control: what it DOES is the MODE's own
            // `ChromeAction` (the same closed vocabulary Ctrl+R and Escape
            // resolve into), performed by the SAME `perform_chrome_action` the
            // keyboard and the mouse side buttons go through. So this handler
            // decides nothing, and cancelling a load from the toolbar and
            // cancelling it with Escape are one path that cannot drift apart.
            perform_chrome_action(controller, controller.reload_stop_action());
            return;
        }
        ID_URL_ENTER => {
            // Enter in the URL bar: navigate through the shell, which owns the
            // front-door rule (a bare `.eth` name, a scheme-less host, an invalid
            // entry that must NOT navigate).
            let typed = window_text(controller.chrome.url_edit);
            let _ = controller.shell.borrow_mut().navigate(&typed);
        }
        ID_MENU_BUTTON => show_browser_menu(controller),
        ID_DEBUG_CLEAR => {
            controller.capture.clear();
            controller.refresh_debug_view();
            return;
        }
        _ if id >= ID_MENU_BASE => {
            controller.menu_item_chosen(id - ID_MENU_BASE);
            return;
        }
        _ => return,
    }
    controller.refresh_chrome();
}

/// PERFORM a resolved [`ChromeAction`]: this edge's half of the shortcut layer,
/// shared by the keyboard, the mouse's side buttons and the page's accelerator
/// keys.
///
/// Every action is performed the SAME way the equivalent toolbar control performs
/// it: through the shared [`BrowserShell`] (and therefore the `Renderer` seam),
/// gated on the SAME `ChromeState` capability flags the buttons take their
/// enabled state from, then a chrome repaint. Nothing here re-decides what the
/// action was for; the `match` is exhaustive over [`ChromeAction`], so an action
/// added to the shared vocabulary stops this edge compiling until it is handled.
fn perform_chrome_action(controller: &Rc<Controller>, action: ChromeAction) {
    let chrome = &controller.chrome;
    match action {
        ChromeAction::FocusUrlBar => {
            // Focus AND select, so the next keystroke replaces the address; no
            // shell call, so no repaint is owed.
            unsafe {
                let _ = SetFocus(Some(chrome.url_edit));
                SendMessageW(
                    chrome.url_edit,
                    EM_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(-1)),
                );
            }
            return;
        }
        ChromeAction::Reload => {
            let _ = controller.shell.borrow_mut().reload();
        }
        ChromeAction::GoBack => {
            // The SAME capability flag the Back button's enabled state reads: a
            // shortcut must not be able to drive a history move the on-screen
            // control refuses.
            let can_go_back = controller.shell.borrow().chrome().can_go_back;
            if !can_go_back {
                return;
            }
            controller.shell.borrow_mut().go_back();
        }
        ChromeAction::GoForward => {
            let can_go_forward = controller.shell.borrow().chrome().can_go_forward;
            if !can_go_forward {
                return;
            }
            controller.shell.borrow_mut().go_forward();
        }
        ChromeAction::Stop => controller.shell.borrow_mut().stop(),
        ChromeAction::RevertUrlBar => {
            // Revert the edit and restore the CURRENT page's URL: the bar goes
            // back to the shell's `url_text`, the same one fact `Chrome::apply`
            // paints it from, so Escape can never leave the bar showing something
            // the chrome does not believe.
            let url = controller.shell.borrow().chrome().url_text.clone();
            set_text(chrome.url_edit, &url);
            let end = url.encode_utf16().count();
            unsafe {
                SendMessageW(
                    chrome.url_edit,
                    EM_SETSEL,
                    Some(WPARAM(end)),
                    Some(LPARAM(end as isize)),
                );
            }
            return;
        }
        ChromeAction::OpenWebInspector => {
            // The PLATFORM's own devtools, never a werust re-implementation.
            controller.dev_tools.open();
            return;
        }
    }
    controller.refresh_chrome();
}

/// Post a resolved action to this window's message loop, carried as its slot in
/// the core's [`ChromeAction::ALL`] (never a Win32 list of its own, so a new
/// action cannot be silently dropped here).
fn post_chrome_action(window: HWND, action: ChromeAction) {
    let Some(slot) = ChromeAction::ALL
        .iter()
        .position(|candidate| *candidate == action)
    else {
        return;
    };
    unsafe {
        let _ = PostMessageW(
            Some(window),
            WM_WERUST_CHROME_ACTION,
            WPARAM(slot),
            LPARAM(0),
        );
    }
}

/// The action a [`WM_WERUST_CHROME_ACTION`] carries, or [`None`] for a slot that
/// names no action.
fn posted_chrome_action(wparam: WPARAM) -> Option<ChromeAction> {
    ChromeAction::ALL.get(wparam.0).copied()
}

/// Ask the shared resolution what a POINTER button means and perform it.
///
/// The mouse's two paths (a raw `WM_XBUTTONDOWN` over this window's own chrome,
/// and the `WM_APPCOMMAND` a child window's click arrives as) differ only in how
/// the BUTTON is read off the message; both end here, so neither can drift into
/// deciding what a button means.
fn perform_pointer_button(
    controller: &Rc<Controller>,
    button: Option<shortcuts::PointerButton>,
) -> bool {
    let Some(action) = button.and_then(shortcuts::resolve_pointer_button) else {
        return false;
    };
    perform_chrome_action(controller, action);
    true
}

/// Give the shortcut layer FIRST look at a keyboard message this thread is about
/// to dispatch; `true` when werust CLAIMED it.
///
/// The message loop, rather than a window procedure, is this edge's equivalent of
/// the GTK shell's CAPTURE-phase controller, and for the same reason: a browser's
/// own chords have to beat the URL bar's text editing (Win32 delivers a key to
/// the FOCUSED control, so an `EDIT` would otherwise eat Escape) and a focused
/// toolbar button, which never forwards a key to its parent at all. A claimed
/// message is dropped before `TranslateMessage`, so no `WM_CHAR` is synthesised
/// and the URL bar cannot beep at a chord the chrome just consumed. Anything the
/// resolution does not claim is dispatched untouched.
///
/// Only this window and its children are filtered: the debug view is a separate
/// top-level window with no browsing controls, and a key pressed there must not
/// drive the browser.
fn filter_shortcut(controller: &Rc<Controller>, message: &MSG) -> bool {
    if message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN {
        // A chord holding Alt (the history chords) arrives as WM_SYSKEYDOWN, not
        // WM_KEYDOWN: the same key press under a different message name.
        return false;
    }
    let window = controller.chrome.window;
    let ours = message.hwnd == window || unsafe { IsChild(window, message.hwnd) }.as_bool();
    if !ours {
        return false;
    }
    // `GetKeyState` (not `GetAsyncKeyState`): it reports the modifier state as of
    // the message being processed, which is the state this key press was made
    // with.
    let Some(action) = shortcut_action(
        (message.wParam.0 & 0xffff) as u16,
        |virtual_key| unsafe { GetKeyState(i32::from(virtual_key)) },
        controller.focus_context(),
    ) else {
        return false;
    };
    perform_chrome_action(controller, action);
    true
}

/// A key pressed while the PAGE had the focus, delivered by WebView2 because the
/// page window belongs to the browser process and its keystrokes never reach this
/// thread's message loop at all.
///
/// The same translation and the same resolution as every other input; only two
/// things differ, both forced by the platform:
///
/// * the focus is the PAGE by definition of this event, so that is what is
///   reported, and
/// * `GetAsyncKeyState` reads the modifiers, because this thread's message queue
///   never saw the key press that `GetKeyState` answers from.
///
/// The action is POSTED, never performed here: the handler runs with the browser
/// process blocked (see [`WM_WERUST_CHROME_ACTION`]).
fn claim_accelerator_key(controller: &Rc<Controller>, virtual_key: u16) -> bool {
    let Some(action) = shortcut_action(
        virtual_key,
        |virtual_key| unsafe { GetAsyncKeyState(i32::from(virtual_key)) },
        Focus::Page,
    ) else {
        return false;
    };
    post_chrome_action(controller.chrome.window, action);
    true
}

/// The ⋮ button: pop the core-derived menu up under it.
fn show_browser_menu(controller: &Rc<Controller>) {
    let chrome = &controller.chrome;
    let mut rect = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = GetWindowRect(chrome.menu_button, &mut rect);
        let _ = TrackPopupMenu(
            chrome.menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON,
            rect.left,
            rect.bottom,
            Some(0),
            chrome.window,
            None,
        );
    }
}

/// The URL bar's subclass: Win32's EDIT control swallows Enter (and dings), so
/// the keypress is turned into the same `WM_COMMAND` a button click sends.
///
/// Enter is ALL it handles. The web inspector used to be caught here too, by this
/// edge's own F12 branch; it is now a row in the shared shortcut table like every
/// other chord, reached through [`filter_shortcut`] before this control ever sees
/// the key.
unsafe extern "system" fn url_edit_proc(
    control: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match message {
        WM_KEYDOWN if wparam.0 as u16 == VK_RETURN.0 => {
            post_to_parent(control, ID_URL_ENTER);
            LRESULT(0)
        }
        // Swallow the Enter CHARACTER too, or the control beeps at a keystroke it
        // has already acted on.
        WM_CHAR if wparam.0 == 0x0d => LRESULT(0),
        _ => unsafe { DefSubclassProc(control, message, wparam, lparam) },
    }
}

/// Send one of this window's own commands to the parent, exactly as a control
/// click would.
fn post_to_parent(control: HWND, command: usize) {
    let parent = unsafe { GetParent(control) }.unwrap_or_default();
    unsafe {
        let _ = PostMessageW(
            Some(parent),
            WM_COMMAND,
            WPARAM(command),
            LPARAM(control.0 as isize),
        );
    }
}

/// The debug view's procedure: the CLEAR button, the tab swap, and the per-row
/// colours the shared carrier chose.
unsafe extern "system" fn debug_wndproc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(controller) = controller_of(window) else {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    };
    match message {
        WM_COMMAND => {
            handle_command(&controller, wparam.0 & 0xffff);
            LRESULT(0)
        }
        WM_SIZE => {
            relayout_debug_window(&controller);
            LRESULT(0)
        }
        // The debug view is its OWN top-level window and can be dragged to a
        // differently scaled monitor by itself, so it answers the same message
        // the browser window does. The FONT is the browser window's (one chrome
        // font per process), so this half is the suggested rect plus a relayout.
        WM_DPICHANGED => {
            let suggested = unsafe { *(lparam.0 as *const windows::Win32::Foundation::RECT) };
            unsafe {
                let _ = SetWindowPos(
                    window,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            relayout_debug_window(&controller);
            LRESULT(0)
        }
        WM_NOTIFY => {
            let header = unsafe { &*(lparam.0 as *const NMHDR) };
            if header.code == NM_CUSTOMDRAW {
                return debug_custom_draw(&controller, lparam);
            }
            if header.code == TCN_SELCHANGE {
                if let Some(debug) = controller.debug.borrow().as_ref() {
                    let selected = current_tab(debug.tabs);
                    debug.selected.set(selected);
                    show(debug.console.list, selected == TAB_CONSOLE);
                    show(debug.network.list, selected == TAB_NETWORK);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Closing the debug view drops the slot, so the next Debug activation
            // opens a fresh one (the same lifecycle the AppKit and GTK views have).
            unsafe {
                let _ = DestroyWindow(window);
            }
            *controller.debug.borrow_mut() = None;
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// `NM_CUSTOMDRAW`, the only way a Win32 list view paints one row (or one cell)
/// in its own colour. The colour itself is the CARRIER's: this answers with what
/// the row was built from.
fn debug_custom_draw(controller: &Rc<Controller>, lparam: LPARAM) -> LRESULT {
    const CDDS_PREPAINT: u32 = 0x0000_0001;
    const CDDS_ITEM: u32 = 0x0001_0000;
    const CDDS_SUBITEM: u32 = 0x0002_0000;
    const CDDS_ITEMPREPAINT: u32 = CDDS_ITEM | CDDS_PREPAINT;
    const CDRF_DODEFAULT: isize = 0x0000_0000;
    const CDRF_NEWFONT: isize = 0x0000_0002;
    const CDRF_NOTIFYITEMDRAW: isize = 0x0000_0020;
    const CDRF_NOTIFYSUBITEMDRAW: isize = 0x0000_0020;

    let draw = unsafe { &mut *(lparam.0 as *mut NMLVCUSTOMDRAW) };
    let debug = controller.debug.borrow();
    let Some(debug) = debug.as_ref() else {
        return LRESULT(CDRF_DODEFAULT);
    };
    let list = draw.nmcd.hdr.hwndFrom;
    let Some(tab) = debug.tab_of(list) else {
        return LRESULT(CDRF_DODEFAULT);
    };
    let stage = draw.nmcd.dwDrawStage;
    let row = draw.nmcd.dwItemSpec;
    if stage.0 == CDDS_PREPAINT {
        return LRESULT(CDRF_NOTIFYITEMDRAW);
    }
    if stage.0 == CDDS_ITEMPREPAINT {
        if list == debug.network.list {
            // Only the TRUST column is coloured in a network row, so the row's
            // draw is deferred to its cells.
            return LRESULT(CDRF_NOTIFYSUBITEMDRAW);
        }
        if let Some(color) = tab.color_of(row) {
            draw.clrText = color;
        }
        return LRESULT(CDRF_NEWFONT);
    }
    if stage.0 == CDDS_ITEMPREPAINT | CDDS_SUBITEM {
        if draw.iSubItem == NETWORK_TRUST_COLUMN {
            if let Some(color) = tab.color_of(row) {
                draw.clrText = color;
            }
        }
        return LRESULT(CDRF_NEWFONT);
    }
    LRESULT(CDRF_DODEFAULT)
}

/// Re-frame the debug view's strips after a resize (or a scale change).
fn relayout_debug_window(controller: &Rc<Controller>) {
    let debug = controller.debug.borrow();
    let Some(debug) = debug.as_ref() else {
        return;
    };
    relayout_debug_window_of(debug);
}

/// The debug view's layout itself, in the pixels of the display IT is on.
///
/// It is a separate top-level window, so it takes its own `GetDpiForWindow`
/// reading rather than the browser window's: the two can sit on monitors with
/// different scales.
fn relayout_debug_window_of(debug: &DebugWindow) {
    let metrics = Metrics::at(window_dpi(debug.window));
    let rect = client_rect(debug.window);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    place(
        debug.title,
        metrics.margin,
        metrics.margin,
        metrics.debug_title_width,
        metrics.debug_title_height,
    );
    place(
        debug.clear,
        width - metrics.margin - metrics.debug_button_width,
        metrics.margin,
        metrics.debug_button_width,
        metrics.debug_button_height,
    );
    place(
        debug.tabs,
        metrics.margin,
        metrics.debug_tabs_top,
        width - 2 * metrics.margin,
        height - metrics.debug_tabs_top - metrics.margin,
    );
    let list_top = metrics.debug_tabs_top + metrics.debug_tab_strip;
    let list_height = height - list_top - metrics.margin - metrics.scale(4);
    for list in [debug.console.list, debug.network.list] {
        place(
            list,
            metrics.margin + metrics.scale(4),
            list_top,
            width - 2 * metrics.margin - metrics.scale(8),
            list_height,
        );
    }
}

/// Register a window class once per process (a second registration of the same
/// class fails harmlessly, and this shell may open more than one window).
fn register_class(name: PCWSTR, proc: WNDPROC) {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: name,
            lpfnWndProc: proc,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&class);
    }
}

/// Create one child control of `parent`.
fn control(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    id: usize,
    font: isize,
) -> HWND {
    let caption = wide(text);
    let child = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(caption.as_ptr()),
            style | WS_CHILD,
            0,
            0,
            10,
            10,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(GetModuleHandleW(None).unwrap_or_default().into()),
            None,
        )
    }
    .unwrap_or_default();
    set_font(child, windows::Win32::Graphics::Gdi::HFONT(font as *mut _));
    child
}

/// Build the ⋮ menu from the core's items: an `Info` item is a DISABLED entry
/// (the `werust <version>` line), an `Action` item a live one, dispatched by its
/// index back to its stable id.
///
/// A FUTURE core menu item therefore needs no change here at all unless it is an
/// action with new behaviour -- the "structured to grow" property, expressed in
/// code, exactly as the GTK popover and the AppKit menu express it.
fn build_browser_menu(items: &[MenuItemPaint]) -> HMENU {
    let menu = unsafe { CreatePopupMenu() }.unwrap_or_default();
    for (index, item) in items.iter().enumerate() {
        let label = wide(&item.label);
        let flags = if item.activatable {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        unsafe {
            let _ = AppendMenuW(menu, flags, ID_MENU_BASE + index, PCWSTR(label.as_ptr()));
        }
    }
    menu
}

/// Build the debug view: a tab control over a CONSOLE and a NETWORK list, plus a
/// CLEAR button that empties the SHARED store.
fn build_debug_window(controller: &Rc<Controller>) -> DebugWindow {
    register_class(DEBUG_CLASS, Some(debug_wndproc));
    let font = controller.font.get();
    // Opened at the BROWSER window's scale (there is no HWND to ask about the
    // debug view's own monitor until it exists); if Windows puts it on a
    // differently scaled one it says so with `WM_DPICHANGED`, which this window
    // answers.
    let metrics = controller.chrome.metrics();
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            DEBUG_CLASS,
            w!("werust Debug"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            metrics.debug_width,
            metrics.debug_height,
            None,
            None,
            Some(GetModuleHandleW(None).unwrap_or_default().into()),
            None,
        )
    }
    .unwrap_or_default();
    // The SAME controller answers both windows' messages.
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, Rc::as_ptr(controller) as isize);
    }

    let title = control(
        window,
        w!("STATIC"),
        "Console + Network capture",
        WS_VISIBLE,
        0,
        font,
    );
    let clear = control(
        window,
        w!("BUTTON"),
        "Clear",
        WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        ID_DEBUG_CLEAR,
        font,
    );
    let tabs = control(
        window,
        w!("SysTabControl32"),
        "",
        WS_VISIBLE | WS_CLIPSIBLINGS,
        0,
        font,
    );
    add_tab(tabs, TAB_CONSOLE, "Console");
    add_tab(tabs, TAB_NETWORK, "Network");

    // READ-ONLY by construction: a report-mode list of labels, never an input.
    let list_style = WS_VISIBLE
        | WS_BORDER
        | WINDOW_STYLE(LVS_REPORT)
        | WINDOW_STYLE(LVS_SINGLESEL)
        | WINDOW_STYLE(LVS_NOSORTHEADER);
    let console = DebugTab::new(control(
        window,
        w!("SysListView32"),
        "",
        list_style,
        0,
        font,
    ));
    console.add_column(0, "Message", metrics.debug_width - metrics.scale(80));
    let network = DebugTab::new(control(
        window,
        w!("SysListView32"),
        "",
        list_style,
        0,
        font,
    ));
    for (index, (title, width)) in [
        ("Method", 60),
        ("Status", 60),
        ("MIME", 140),
        ("Size", 80),
        ("Trust", 180),
        ("URL", 380),
    ]
    .into_iter()
    .enumerate()
    {
        network.add_column(
            i32::try_from(index).unwrap_or(0),
            title,
            metrics.scale(width),
        );
    }
    // The CONSOLE tab is the one showing first; both lists share one rectangle.
    show(network.list, false);

    let debug = DebugWindow {
        window,
        title,
        tabs,
        clear,
        console,
        network,
        selected: Cell::new(TAB_CONSOLE),
    };
    relayout_debug_window_of(&debug);
    debug
}

/// The Windows browser window: the product surface, over the shared shell.
///
/// Construction is separate from the message loop on purpose: the CI smoke
/// (`examples/window_smoke.rs`) builds a real window, pumps it by hand and
/// asserts what the real widgets show, which is the only way this file gets
/// EXECUTED anywhere before a human opens it.
pub struct BrowserWindow {
    controller: Rc<Controller>,
}

impl BrowserWindow {
    /// Build the window over an already-wired shell and capture store.
    ///
    /// `shell` must already own the backend with its trust hooks installed; this
    /// function only paints and forwards. `dev_tools` is the handle onto the
    /// platform's own DevTools, taken from the backend before it was boxed.
    pub fn open(
        shell: Rc<RefCell<BrowserShell>>,
        capture: DebugCapture,
        dev_tools: windows_renderer::DevTools,
        placement: Placement,
    ) -> Result<Self, RendererError> {
        // The list view, the tab control and the progress bar are common
        // controls: without this they simply do not create.
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: u32::try_from(std::mem::size_of::<INITCOMMONCONTROLSEX>()).unwrap_or(8),
                dwICC: ICC_WIN95_CLASSES
                    | ICC_BAR_CLASSES
                    | ICC_LISTVIEW_CLASSES
                    | ICC_TAB_CLASSES
                    | ICC_PROGRESS_CLASS,
            };
            let _ = InitCommonControlsEx(&controls);
        }
        register_class(WINDOW_CLASS, Some(wndproc));

        let (x, y) = match placement {
            Placement::OnScreen => (CW_USEDEFAULT, CW_USEDEFAULT),
            // FAR off-screen: a CI run shows nothing and steals no focus.
            Placement::OffScreen => (-32_000, -32_000),
        };
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                WINDOW_CLASS,
                w!("werust"),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                x,
                y,
                DEFAULT_WIDTH,
                DEFAULT_HEIGHT,
                None,
                None,
                Some(GetModuleHandleW(None).unwrap_or_default().into()),
                None,
            )
        }
        .map_err(|e| RendererError::Backend(format!("CreateWindowExW(werust): {e}")))?;

        // Only now that the window EXISTS can Windows say which monitor it is on
        // and what that monitor's scale is (`GetDpiForWindow` takes an HWND), so
        // the size above is the 96-DPI design size and this is where it becomes
        // real pixels. Without it a 200% display opens a half-size window, which
        // is the same defect as the half-size chrome, one level up.
        let dpi = window_dpi(window);
        let metrics = Metrics::at(dpi);
        unsafe {
            let _ = SetWindowPos(
                window,
                None,
                0,
                0,
                metrics.default_width,
                metrics.default_height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }

        let font = ui_font(metrics.font_height);
        let font_raw = font.0 as isize;

        // The live page: the seam hands over an opaque pointer to the backend's
        // container window, which this shell RE-PARENTS in without knowing it is
        // a WebView2 host.
        let handle = shell.borrow().view_handle();
        let page = HWND(handle.0.cast());
        unsafe {
            let _ = SetParent(page, Some(window));
            // A container created as a top-level window becomes a CHILD here; the
            // style has to say so or Win32 will keep treating it as a popup.
            SetWindowLongPtrW(
                page,
                GWL_STYLE,
                (WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN).0 as isize,
            );
            let _ = ShowWindow(page, SW_SHOWNOACTIVATE);
        }

        let back = control(window, w!("BUTTON"), "◀", WS_VISIBLE, ID_BACK, font_raw);
        let forward = control(window, w!("BUTTON"), "▶", WS_VISIBLE, ID_FORWARD, font_raw);
        // The ONE reload/stop control. It is built with NO caption: its glyph is
        // the shared derivation's (`ChromePaint::reload_stop_label`), and the
        // `refresh_chrome` below paints it before the window is ever shown — the
        // same way the trust badge gets its first text.
        let reload_stop = control(
            window,
            w!("BUTTON"),
            "",
            WS_VISIBLE,
            ID_RELOAD_STOP,
            font_raw,
        );
        // The LOADING SPINNER: one glyph in a fixed slot, hidden until the core
        // says a load is worth reporting (like the invalid-entry badge, which is
        // the other control this chrome shows conditionally).
        let spinner = control(
            window,
            w!("STATIC"),
            "",
            WINDOW_STYLE(SS_CENTER),
            0,
            font_raw,
        );
        let menu_button = control(
            window,
            w!("BUTTON"),
            "⋮",
            WS_VISIBLE,
            ID_MENU_BUTTON,
            font_raw,
        );
        let url_edit = control(
            window,
            w!("EDIT"),
            "",
            WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            ID_URL_EDIT,
            font_raw,
        );
        // Win32's EDIT swallows Enter; the subclass turns it into the same
        // command a button click sends.
        unsafe {
            let _ = SetWindowSubclass(url_edit, Some(url_edit_proc), ID_URL_EDIT, 0);
        }
        // The progress bar is created AFTER the URL bar, so it sits above it in
        // the z-order: it is laid out INSIDE the bar's rectangle and must not be
        // clipped away by it.
        let progress = control(
            window,
            w!("msctls_progress32"),
            "",
            WS_CLIPSIBLINGS,
            0,
            font_raw,
        );
        unsafe {
            // VISUAL STYLES WOULD SWALLOW THE SHARED COLOUR, so this ONE control
            // opts out of them. `PBM_SETBARCOLOR` "has no effect" once visual
            // styles are enabled (Microsoft's documented remark on that
            // message), and since `windows-release-packaging-leg` embedded the
            // comctl32 v6 manifest they ARE enabled — so without this line the
            // strip would quietly become the theme's colour instead of the
            // palette's blue, on the one edge where nothing can notice: the
            // shared palette is asserted against the GTK stylesheet, not against
            // pixels a runner cannot see. Passing empty strings is the
            // documented way to detach a control from the theme.
            let _ = SetWindowTheme(progress, w!(""), w!(""));
            SendMessageW(progress, PBM_SETRANGE32, Some(WPARAM(0)), Some(LPARAM(100)));
            // The URL bar's progress fill, in the SHARED palette's colour — the
            // same blue the GTK edge's `entry > progress` rule paints.
            SendMessageW(
                progress,
                PBM_SETBARCOLOR,
                Some(WPARAM(0)),
                Some(LPARAM(colorref(LOAD_PROGRESS_COLOR).0 as isize)),
            );
        }
        let invalid_badge = control(window, w!("STATIC"), "", WINDOW_STYLE(0), 0, font_raw);
        let trust = control(window, w!("STATIC"), "", WS_VISIBLE, 0, font_raw);
        // The error banner sits directly under the toolbar and ABOVE the page, so
        // a failed load's reason is unmissable rather than buried in the footer.
        let error_banner = control(
            window,
            w!("STATIC"),
            "",
            WINDOW_STYLE(SS_LEFTNOWORDWRAP),
            0,
            font_raw,
        );
        let status = control(window, w!("STATIC"), "", WS_VISIBLE, 0, font_raw);

        // ONE tooltip control for the window, carrying the trust EXPLANATION and
        // the URL bar's in-flight progress sentence.
        let tooltip = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("tooltips_class32"),
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP) | WINDOW_STYLE(TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(window),
                None,
                Some(GetModuleHandleW(None).unwrap_or_default().into()),
                None,
            )
        }
        .unwrap_or_default();

        let items = menu_items();
        let menu = build_browser_menu(&items);

        let chrome = Chrome {
            window,
            back,
            forward,
            reload_stop,
            spinner,
            menu_button,
            url_edit,
            progress,
            invalid_badge,
            trust,
            error_banner,
            status,
            page,
            menu,
            tooltip,
            banner_visible: Cell::new(false),
            badge_visible: Cell::new(false),
            trust_color: Cell::new(COLORREF(0)),
            error_color: Cell::new(COLORREF(0)),
            error_brush: Cell::new(HBRUSH::default()),
            url_invalid: Cell::new(false),
            spinner_frame: Cell::new(0),
            // Every rectangle this chrome draws is scaled from here, and
            // `WM_DPICHANGED` replaces it.
            dpi: Cell::new(dpi.raw()),
        };
        chrome.add_tip(trust);
        chrome.add_tip(url_edit);
        // The collapsed control's ACCESSIBLE NAME ("Reload this page" / "Stop
        // loading this page") is the core's, and hover is this edge's surface for
        // it — so the control has to be a tool of the window's one tooltip before
        // the first paint can give it one.
        chrome.add_tip(reload_stop);

        // `docs/adr/0009`: read the OS setting once here and re-read it on
        // `WM_SETTINGCHANGE`. The reader is the engine crate's ONE registry read,
        // mapped by the shared `OsColorScheme` rule; this window never guesses.
        let scheme = windows_renderer::os_color_scheme();
        let controller = Rc::new(Controller {
            shell,
            capture,
            chrome,
            menu_items: items,
            dev_tools,
            debug: RefCell::new(None),
            scheme: Cell::new(scheme),
            theme: RefCell::new(Theme::of(scheme)),
            font: Cell::new(font_raw),
        });
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, Rc::as_ptr(&controller) as isize);
        }
        apply_title_bar_theme(window, scheme);

        controller.chrome.relayout();
        controller.refresh_chrome();
        unsafe {
            let _ = ShowWindow(
                window,
                match placement {
                    Placement::OnScreen => SW_SHOW,
                    Placement::OffScreen => SW_SHOWNOACTIVATE,
                },
            );
        }
        Ok(Self { controller })
    }

    /// Start the 50ms chrome pump on the window's message loop (the product path;
    /// the smoke pumps by hand instead).
    pub fn start_pump(&self) {
        unsafe {
            SetTimer(
                Some(self.controller.chrome.window),
                PUMP_TIMER,
                PUMP_INTERVAL_MS,
                None,
            );
        }
    }

    /// Run ONE pump tick by hand (the CI smoke's entry point).
    pub fn tick(&self) {
        self.controller.tick();
    }

    /// Repaint the chrome from the shell's current state, exactly as the pump
    /// does when the seam reports an event.
    ///
    /// The smoke's sampling point: a pump tick only repaints when the seam had
    /// something to say, so a driver that wants to read the widgets for the state
    /// the shell is in RIGHT NOW asks for the paint rather than hoping one
    /// happened.
    pub fn refresh_chrome(&self) {
        self.controller.refresh_chrome();
    }

    /// Turn the Win32 message loop until it is empty, so WebView2's events (which
    /// are all delivered through it) actually arrive.
    ///
    /// The shortcut layer gets FIRST look at every message here, exactly as it
    /// does in the product's loop, so the CI smoke drives the SAME path a user
    /// does.
    pub fn pump_messages(&self) {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if self.filter_shortcut(&message) {
                    continue;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    /// Give the shortcut layer first look at one message the loop is about to
    /// dispatch; `true` when it CLAIMED it (see [`filter_shortcut`]).
    #[must_use]
    pub fn filter_shortcut(&self, message: &MSG) -> bool {
        filter_shortcut(&self.controller, message)
    }

    /// Answer the keys the PAGE swallows, through the engine's accelerator hook.
    ///
    /// Installed after the window exists, on the handle the shell took from the
    /// backend before boxing it behind the seam -- the same move the devtools
    /// affordance makes. Without it the conventional chords would work only while
    /// the CHROME has the focus, because WebView2 hosts the page in another
    /// process (see [`claim_accelerator_key`]).
    pub fn install_accelerator_keys(&self, keys: &windows_renderer::AcceleratorKeys) {
        // The handler holds the controller, which (through the shell) holds the
        // backend that holds the handler: a cycle, and a deliberate one. This
        // window lives for the process's lifetime -- its `Rc` is already handed
        // to Win32 as a raw pointer in `GWLP_USERDATA` and never reclaimed -- so
        // a `Weak` here would buy nothing but a shortcut that could silently stop
        // working.
        let controller = Rc::clone(&self.controller);
        keys.on_key(move |virtual_key| claim_accelerator_key(&controller, virtual_key));
    }

    /// Answer ONE page-focused key exactly as the engine's hook does; `true` when
    /// werust claimed it.
    ///
    /// The CI smoke's entry into that path: no runner can press a key over a real
    /// page, but the translation, the POST and the performer on the other side of
    /// it are all real when driven from here.
    #[must_use]
    pub fn claim_accelerator_key(&self, virtual_key: u16) -> bool {
        claim_accelerator_key(&self.controller, virtual_key)
    }

    /// Give the page the keyboard focus, so Windows routes scroll / click /
    /// typing to the live page.
    pub fn focus_page(&self) {
        self.controller.shell.borrow_mut().focus_page(true);
    }

    /// Open the debug view, exactly as the ⋮ menu's Debug entry does.
    pub fn open_debug_view(&self) {
        self.controller.open_debug_view();
    }

    /// Close the debug view, as its own close button does (the CI smoke drives
    /// the same path a user does, so the slot-clearing is exercised).
    pub fn close_debug_view(&self) {
        let window = self
            .controller
            .debug
            .borrow()
            .as_ref()
            .map(|debug| debug.window);
        if let Some(window) = window {
            unsafe {
                let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            self.pump_messages();
        }
    }

    /// Open the platform's OWN devtools over the live page
    /// (`OpenDevToolsWindow`), never a werust re-implementation. `false` when
    /// there is no page yet or the runtime refused.
    pub fn open_dev_tools(&self) -> bool {
        self.controller.dev_tools.open()
    }

    /// The window itself (the smoke closes it; the product hands it to Win32).
    #[must_use]
    pub fn window(&self) -> HWND {
        self.controller.chrome.window
    }

    /// The scale of the display this window is on, as Windows reports it
    /// (`GetDpiForWindow`, per-MONITOR).
    #[must_use]
    pub fn dpi(&self) -> u32 {
        self.controller.chrome.dpi.get()
    }

    /// The metrics every rectangle in this window was laid out from — the ONE
    /// seam, at this display's scale.
    ///
    /// The smoke compares the REAL widgets against these, which is the only
    /// run-time check of the DPI work available anywhere: a CI runner has no DPI,
    /// so at 96 this proves the layout is COMPUTED from the seam, and on a
    /// human's scaled display the same assertions prove it SCALES.
    #[must_use]
    pub fn metrics(&self) -> Metrics {
        self.controller.chrome.metrics()
    }

    /// The URL bar (the smoke measures its rectangle).
    #[must_use]
    pub fn url_bar(&self) -> HWND {
        self.controller.chrome.url_edit
    }

    /// The trust indicator (likewise).
    #[must_use]
    pub fn trust(&self) -> HWND {
        self.controller.chrome.trust
    }

    /// The ONE reload/stop control (the smoke measures its rectangle).
    #[must_use]
    pub fn reload_stop(&self) -> HWND {
        self.controller.chrome.reload_stop
    }

    /// One control's rectangle in the WINDOW's client coordinates: the same
    /// space the layout placed it in.
    #[must_use]
    pub fn control_rect(&self, control: HWND) -> windows::Win32::Foundation::RECT {
        control_rect(self.controller.chrome.window, control)
    }

    /// The page window's rectangle in the window's client coordinates, so the
    /// smoke can check that the page starts exactly one scaled toolbar down.
    #[must_use]
    pub fn page_client_rect(&self) -> windows::Win32::Foundation::RECT {
        control_rect(self.controller.chrome.window, self.controller.chrome.page)
    }

    /// Close the window (and stop the pump).
    pub fn close(&self) {
        unsafe {
            let _ = DestroyWindow(self.controller.chrome.window);
        }
    }

    /// What the URL bar currently SHOWS.
    #[must_use]
    pub fn url_text(&self) -> String {
        window_text(self.controller.chrome.url_edit)
    }

    /// What the trust indicator currently SHOWS.
    #[must_use]
    pub fn trust_text(&self) -> String {
        window_text(self.controller.chrome.trust)
    }

    /// The trust indicator's EXPLANATION, as its tooltip holds it
    /// (`docs/adr/0006`: the badge is self-explaining on hover).
    #[must_use]
    pub fn trust_detail(&self) -> Option<String> {
        self.controller.chrome.tip_of(self.controller.chrome.trust)
    }

    /// What the status line currently SHOWS.
    #[must_use]
    pub fn status_text(&self) -> String {
        window_text(self.controller.chrome.status)
    }

    /// The error banner's text when it is VISIBLE, `None` when it is hidden.
    #[must_use]
    pub fn error_banner(&self) -> Option<String> {
        self.controller.chrome.visible_banner_text()
    }

    /// Whether the invalid-entry badge is showing.
    #[must_use]
    pub fn invalid_badge_visible(&self) -> bool {
        is_visible(self.controller.chrome.invalid_badge)
    }

    /// Whether the URL bar's progress strip is showing.
    #[must_use]
    pub fn progress_visible(&self) -> bool {
        is_visible(self.controller.chrome.progress)
    }

    /// The GLYPH the ONE reload/stop control is currently wearing: `⟳` on a
    /// settled page, `✕` while a load is in flight. Which of the two is the
    /// core's call, so the smoke compares this against the core's own label
    /// rather than against a literal.
    #[must_use]
    pub fn reload_stop_label(&self) -> String {
        window_text(self.controller.chrome.reload_stop)
    }

    /// That control's ACCESSIBLE NAME, as its tooltip holds it ("Reload this
    /// page" / "Stop loading this page", the core's wording).
    #[must_use]
    pub fn reload_stop_description(&self) -> Option<String> {
        self.controller
            .chrome
            .tip_of(self.controller.chrome.reload_stop)
    }

    /// Whether the LOADING SPINNER is showing.
    #[must_use]
    pub fn spinner_visible(&self) -> bool {
        is_visible(self.controller.chrome.spinner)
    }

    /// Activate the ONE reload/stop control, exactly as clicking it does: the
    /// same `WM_COMMAND` Win32 sends, through the same handler. This is how the
    /// smoke cancels an in-flight load FROM THE TOOLBAR rather than by calling
    /// the shell behind the widget's back.
    pub fn activate_reload_stop(&self) {
        handle_command(&self.controller, ID_RELOAD_STOP);
    }

    /// The page window's rectangle, so the smoke can prove that in-flight
    /// progress does NOT displace the page while a failure banner does.
    #[must_use]
    pub fn page_rect(&self) -> windows::Win32::Foundation::RECT {
        let mut rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            let _ = GetWindowRect(self.controller.chrome.page, &mut rect);
        }
        rect
    }

    /// The ⋮ menu's item titles, in order, as Win32 holds them.
    #[must_use]
    pub fn menu_titles(&self) -> Vec<String> {
        let menu = self.controller.chrome.menu;
        let count = unsafe { GetMenuItemCount(Some(menu)) }.max(0);
        (0..count)
            .map(|index| {
                let mut buffer = vec![0u16; 256];
                let written = unsafe {
                    GetMenuStringW(
                        menu,
                        u32::try_from(index).unwrap_or(0),
                        Some(&mut buffer),
                        MF_BYPOSITION,
                    )
                };
                String::from_utf16_lossy(&buffer[..written.max(0) as usize])
            })
            .collect()
    }

    /// The row counts of the open debug view's two tabs, or `None` when it is
    /// closed.
    #[must_use]
    pub fn debug_row_counts(&self) -> Option<(usize, usize)> {
        self.controller
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
            .menu_items
            .iter()
            .position(|item| item.id == id)
        else {
            return false;
        };
        if !self.controller.menu_items[index].activatable {
            return false;
        }
        // The same command Win32 posts when the item is clicked.
        handle_command(&self.controller, ID_MENU_BASE + index);
        true
    }
}

/// Build the whole Windows shell over the WebView2 backend and RUN it: the
/// product entry point.
///
/// The construction order mirrors the GTK and AppKit shells exactly, because the
/// constraints are the same shared ones: the trust hooks are installed on the
/// backend BEFORE it is boxed behind the seam (and, on Windows, before the first
/// navigation creates the environment, since the SET of custom scheme names is
/// fixed there), the redirect sink and the capture store are handed to BOTH the
/// backend and the shell so each side sees the same one, and the window is then a
/// painter over the result.
///
/// ADR-0010 (`target="_blank"` / `window.open` navigates in place) needs no call
/// here: the backend's own `add_NewWindowRequested` hook routes it through the
/// shared `renderer::new_window_action`, so this window neither opens a second
/// window nor re-decides the rule.
pub fn run(url: &str) -> Result<(), RendererError> {
    // The DURABLE profile: a browser must not inherit the engine's development
    // `%TEMP%` default, which loses cookies, storage and cache.
    let mut backend = match crate::profile::user_data_folder() {
        Some(folder) => windows_renderer::Webview2Renderer::with_user_data_folder(folder)?,
        None => windows_renderer::Webview2Renderer::new()?,
    };
    // Trust hook 1: the native EIP-1193 provider over the script-message bridge.
    backend.install_provider();
    // Trust hook 2: native `ipfs://` resolution through the hash-verified core
    // path. It hands back the `_redirects` 3xx sink the shell drains on its pump.
    let redirects = backend.install_ipfs();
    // The debug CAPTURE POINTS, on the same store the debug view renders.
    let capture = DebugCapture::new();
    install_debug_capture(&mut backend, capture.clone());
    // Taken BEFORE boxing: the chrome's devtools affordance is the platform's own
    // `OpenDevToolsWindow`, and the backend is unreachable once it is behind the
    // seam.
    let dev_tools = backend.dev_tools();
    // Likewise the hook onto the keys the PAGE has the focus for: WebView2 hosts
    // the page in its own process, so a chord pressed over it never reaches this
    // thread's message loop.
    let accelerator_keys = backend.accelerator_keys();

    let shell = Rc::new(RefCell::new(
        BrowserShell::new(Box::new(backend))
            .with_redirect_sink(redirects)
            .with_debug_capture(capture.clone()),
    ));

    let window = BrowserWindow::open(shell.clone(), capture, dev_tools, Placement::OnScreen)?;
    // The CONVENTIONAL BROWSER SHORTCUTS' page-focused half: Ctrl+L, Ctrl+R / F5,
    // Alt+Left / Alt+Right, Escape and F12 pressed over the page (task
    // `shortcuts-and-mouse-history-buttons-on-the-windows-edge`). The chrome-focused
    // half is `filter_shortcut`, in the loop below.
    window.install_accelerator_keys(&accelerator_keys);

    // Navigate through the seam and focus the live view, so Windows routes
    // scroll/click/focus/keyboard input to the page.
    shell.borrow_mut().navigate(url)?;
    window.focus_page();
    window.tick();
    window.start_pump();

    // The one message loop: WebView2 delivers EVERY event through it, and the
    // shortcut layer takes its look BEFORE `TranslateMessage`, which is this
    // edge's equivalent of the GTK shell's capture-phase key controller.
    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            if window.filter_shortcut(&message) {
                continue;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
