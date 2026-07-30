//! The toolbar, the error surface and the status line: the widgets, and nothing
//! else.
//!
//! Every string, fraction, colour and enabled-flag assigned here comes from
//! [`ChromePaint`], the shared `desktop-paint` carrier of the `werust-core`
//! derivation. This file evaluates no rule; [`Chrome::apply`] is a straight-line
//! assignment block, which is what keeps a stale badge from surviving a
//! transition.
//!
//! **Only a FAILURE moves the page.** [`Chrome::relayout`] gives the page window
//! everything between the fixed toolbar and the fixed status line, minus the
//! error banner's strip when (and only when) the banner is up. In-flight progress
//! is laid out INSIDE the URL bar's rectangle, so a navigation never resizes the
//! page (task `loading-progress-in-the-url-bar-not-a-banner`).

use std::cell::Cell;

use windows::core::PWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH};
use windows::Win32::UI::Controls::{
    TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_GETTEXTW, TTM_UPDATETIPTEXTW, TTTOOLINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, HMENU};

use crate::paint::ChromePaint;
use crate::win32::{
    client_rect, colorref, enable, is_visible, place, redraw, set_text, show, wide, window_text,
};

/// The toolbar strip's height.
pub const TOOLBAR_HEIGHT: i32 = 40;
/// The error banner's height (a failure is the one state allowed to take it).
pub const BANNER_HEIGHT: i32 = 44;
/// The status line's height.
pub const STATUS_HEIGHT: i32 = 22;
/// The gap around chrome items.
pub const MARGIN: i32 = 8;
/// A nav button's width.
pub const BUTTON_WIDTH: i32 = 36;
/// The trust indicator's width (it carries a whole phrase, not a glyph).
pub const TRUST_WIDTH: i32 = 210;
/// The invalid-entry badge's width.
pub const BADGE_WIDTH: i32 = 110;
/// The URL bar's progress strip: a few pixels along its bottom edge, INSIDE the
/// bar, so it takes no height from the page.
pub const PROGRESS_HEIGHT: i32 = 3;

/// `PBM_SETPOS`, the progress bar's position message.
const PBM_SETPOS: u32 = 0x0400 + 2;
/// `PBM_SETRANGE32`, its 32-bit range message.
pub const PBM_SETRANGE32: u32 = 0x0400 + 6;
/// `PBM_SETBARCOLOR`, which paints the fill in the SHARED palette's colour
/// rather than the system accent.
pub const PBM_SETBARCOLOR: u32 = 0x0400 + 9;

/// The `TTTOOLINFOW` size a comctl32 **5.82** accepts (`TTTOOLINFO_V2_SIZE`):
/// every field up to and including `lParam`, WITHOUT the `lpReserved` that
/// version 6 added.
///
/// This is load-bearing, and it was MEASURED rather than reasoned: run
/// [30588443862](https://github.com/wighawag/werust/actions/runs/30588443862)
/// passed 24 of 26 window checks and failed exactly the two that read the trust
/// EXPLANATION back off the tooltip, because `size_of::<TTTOOLINFOW>()` is the
/// version-6 size and a tooltip control rejects a `cbSize` it does not
/// recognise -- so `TTM_ADDTOOL` silently added no tool at all.
///
/// werust ships without an application manifest today, so the process links
/// comctl32 **5.82** (`docs/spikes/windows-win32-window-and-chrome/DECISIONS.md`
/// §4: the v6 manifest is a packaging concern, deferred to
/// `windows-release-packaging-leg`). The V2 size is accepted by BOTH versions, so
/// this stays correct after that manifest lands.
const TOOL_INFO_V2_SIZE: usize =
    std::mem::size_of::<TTTOOLINFOW>() - std::mem::size_of::<*mut std::ffi::c_void>();

/// The widgets the pump repaints from [`ChromePaint`], plus the window they live
/// in.
///
/// Grouped so ONE [`apply`](Chrome::apply) keeps every surface in step with the
/// shell's state — the same shape the GTK and AppKit edges have, for the same
/// reason: a half-applied chrome is how a stale badge survives a transition.
pub struct Chrome {
    /// The top-level window.
    pub window: HWND,
    pub back: HWND,
    pub forward: HWND,
    pub reload: HWND,
    pub stop: HWND,
    pub menu_button: HWND,
    pub url_edit: HWND,
    /// The load-progress bar, laid out INSIDE the URL bar (never its own row).
    pub progress: HWND,
    pub invalid_badge: HWND,
    pub trust: HWND,
    pub error_banner: HWND,
    pub status: HWND,
    /// The backend's container window, re-parented in through the seam's
    /// `ViewHandle`.
    pub page: HWND,
    /// The ⋮ menu, built once from the core's `BrowserMenu`.
    pub menu: HMENU,
    /// The one tooltip control: it carries the trust indicator's EXPLANATION and
    /// the URL bar's in-flight progress sentence.
    pub tooltip: HWND,
    /// Whether the error banner is currently taking its strip. Tracked because a
    /// change in it is the ONE thing that re-lays-out the page area.
    pub banner_visible: Cell<bool>,
    /// Whether the invalid-entry badge is taking its toolbar slot.
    pub badge_visible: Cell<bool>,
    /// The colours the last paint chose. Win32 asks for a control's colours at
    /// PAINT time (`WM_CTLCOLORSTATIC`) rather than storing them on the control,
    /// so the window proc reads them back from here.
    pub trust_color: Cell<COLORREF>,
    pub error_color: Cell<COLORREF>,
    pub error_brush: Cell<HBRUSH>,
    /// Whether the URL bar's text is currently the INVALID one (painted in the
    /// carrier's invalid colour, while the typed text is KEPT for the user to
    /// fix).
    pub url_invalid: Cell<bool>,
}

impl Chrome {
    /// Paint one [`ChromePaint`] into the widgets.
    ///
    /// Straight-line assignment: no rule is evaluated here, and every value is a
    /// field of the snapshot the shared carrier derived from the core.
    pub fn apply(&self, paint: &ChromePaint) {
        // Only overwrite the URL bar when it does not already hold this text, so
        // the caret does not jump while the user is mid-edit.
        if window_text(self.url_edit) != paint.url_text {
            set_text(self.url_edit, &paint.url_text);
        }
        // The INVALID-entry surface (field finding D): the badge appears and the
        // typed text is rendered invalid, while the text itself is KEPT.
        if self.url_invalid.get() != paint.invalid_entry {
            self.url_invalid.set(paint.invalid_entry);
            redraw(self.url_edit);
        }
        set_text(self.invalid_badge, paint.invalid_badge_text);
        show(self.invalid_badge, paint.invalid_entry);

        enable(self.back, paint.can_go_back);
        enable(self.forward, paint.can_go_forward);
        // Stop is meaningful only while a load is in flight; Reload only once it
        // has settled.
        enable(self.stop, paint.is_loading);
        enable(self.reload, !paint.is_loading);

        set_text(self.status, &paint.status_text);

        // The trust indicator: the shared badge text, its EXPLANATION as the
        // tooltip, and the colour of the class the core chose. Exactly one state
        // is painted, so no stale colour can survive a transition.
        set_text(self.trust, paint.trust_text);
        self.trust_color.set(colorref(paint.trust_color));
        self.set_tip(self.trust, paint.trust_detail);
        redraw(self.trust);

        // The URL bar's own progress bar: it advances with the real pipeline
        // phase and disappears once the load settles. It changes NO geometry, so
        // a navigation never resizes the page.
        let percent = (paint.progress_fraction * 100.0).round().clamp(0.0, 100.0) as usize;
        unsafe {
            SendMessageW(
                self.progress,
                PBM_SETPOS,
                Some(WPARAM(percent)),
                Some(LPARAM(0)),
            );
        }
        show(self.progress, paint.progress_visible);
        // `None` clears it, so a stale phase never lingers on hover.
        self.set_tip(
            self.url_edit,
            paint.progress_tooltip.as_deref().unwrap_or(""),
        );

        // The PROMINENT error banner: shown ONLY on a failed load, carrying the
        // accurate, protocol-named reason across the top of the view.
        if paint.error_visible {
            set_text(self.error_banner, &paint.error_text);
            self.set_banner_fill(colorref(paint.error_color));
            redraw(self.error_banner);
        }
        show(self.error_banner, paint.error_visible);

        // A change in either optional surface changes the geometry, so the strips
        // are re-laid-out; nothing else in a repaint moves a window.
        if self.banner_visible.get() != paint.error_visible
            || self.badge_visible.get() != paint.invalid_entry
        {
            self.banner_visible.set(paint.error_visible);
            self.badge_visible.set(paint.invalid_entry);
            self.relayout();
        }
    }

    /// Keep ONE brush for the banner's current severity fill, replacing it only
    /// when the severity's colour actually changed (a brush created per repaint
    /// is a GDI leak at 20 repaints a second).
    fn set_banner_fill(&self, color: COLORREF) {
        if self.error_color.get() == color && !self.error_brush.get().is_invalid() {
            return;
        }
        let previous = self.error_brush.get();
        self.error_color.set(color);
        self.error_brush.set(unsafe { CreateSolidBrush(color) });
        if !previous.is_invalid() {
            unsafe {
                let _ = DeleteObject(previous.into());
            }
        }
    }

    /// Recompute every rectangle from the client area: fixed strips top and
    /// bottom, the page window taking everything between. Called on open, on
    /// every resize, and whenever the banner or badge appears/disappears.
    pub fn relayout(&self) {
        let rect = client_rect(self.window);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let banner_height = if self.banner_visible.get() {
            BANNER_HEIGHT
        } else {
            0
        };

        place(
            self.error_banner,
            MARGIN,
            TOOLBAR_HEIGHT + 6,
            width - 2 * MARGIN,
            banner_height - 12,
        );
        // ONLY the error banner may change the page's geometry.
        let page_top = TOOLBAR_HEIGHT + banner_height;
        let page_height = height - page_top - STATUS_HEIGHT;
        place(self.page, 0, page_top, width, page_height);
        place(
            self.status,
            MARGIN,
            height - STATUS_HEIGHT + 3,
            width - 2 * MARGIN,
            STATUS_HEIGHT - 6,
        );

        // The toolbar's own row, left to right: the nav controls, then the URL
        // bar taking the slack, then (optionally) the invalid badge, the trust
        // indicator and the ⋮ menu pinned to the right.
        let row_y = 6;
        let row_height = TOOLBAR_HEIGHT - 12;
        let mut x = MARGIN;
        for control in [self.back, self.forward, self.reload, self.stop] {
            place(control, x, row_y, BUTTON_WIDTH, row_height);
            x += BUTTON_WIDTH + 2;
        }
        let badge_width = if self.badge_visible.get() {
            BADGE_WIDTH + 6
        } else {
            0
        };
        let right = width - MARGIN - BUTTON_WIDTH - TRUST_WIDTH - badge_width - 12;
        let url_width = (right - x - 6).max(60);
        place(self.url_edit, x, row_y, url_width, row_height);
        // INSIDE the URL bar, along its bottom edge: the progress strip takes no
        // height of its own and therefore cannot move the page.
        place(
            self.progress,
            x + 2,
            row_y + row_height - PROGRESS_HEIGHT - 2,
            url_width - 4,
            PROGRESS_HEIGHT,
        );
        let mut x = x + url_width + 6;
        if self.badge_visible.get() {
            place(self.invalid_badge, x, row_y, BADGE_WIDTH, row_height);
            x += BADGE_WIDTH + 6;
        }
        place(self.trust, x, row_y, TRUST_WIDTH, row_height);
        x += TRUST_WIDTH + 6;
        place(self.menu_button, x, row_y, BUTTON_WIDTH, row_height);
    }

    /// Register a control with the tooltip control, so it can carry text later.
    pub fn add_tip(&self, control: HWND) {
        if self.tooltip.is_invalid() {
            return;
        }
        let mut empty = wide("");
        let mut info = self.tool_info(control, PWSTR(empty.as_mut_ptr()));
        unsafe {
            SendMessageW(
                self.tooltip,
                TTM_ADDTOOLW,
                Some(WPARAM(0)),
                Some(LPARAM(std::ptr::from_mut(&mut info) as isize)),
            );
        }
    }

    /// Set (or clear) one control's tooltip text.
    fn set_tip(&self, control: HWND, text: &str) {
        if self.tooltip.is_invalid() {
            return;
        }
        let mut buffer = wide(text);
        let mut info = self.tool_info(control, PWSTR(buffer.as_mut_ptr()));
        unsafe {
            SendMessageW(
                self.tooltip,
                TTM_UPDATETIPTEXTW,
                Some(WPARAM(0)),
                Some(LPARAM(std::ptr::from_mut(&mut info) as isize)),
            );
        }
    }

    /// Read one control's tooltip text back, exactly as a hover would show it.
    ///
    /// This is how the CI smoke asserts that the trust EXPLANATION really reached
    /// a WIDGET rather than merely a struct field — the one thing a Windows
    /// runner adds over the Ubuntu gate.
    #[must_use]
    pub fn tip_of(&self, control: HWND) -> Option<String> {
        if self.tooltip.is_invalid() {
            return None;
        }
        let mut buffer = vec![0u16; 512];
        let mut info = self.tool_info(control, PWSTR(buffer.as_mut_ptr()));
        unsafe {
            SendMessageW(
                self.tooltip,
                TTM_GETTEXTW,
                Some(WPARAM(buffer.len() - 1)),
                Some(LPARAM(std::ptr::from_mut(&mut info) as isize)),
            );
        }
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(0);
        (end > 0).then(|| String::from_utf16_lossy(&buffer[..end]))
    }

    /// The `TTTOOLINFOW` naming one control as a tool of this window's tooltip.
    fn tool_info(&self, control: HWND, text: PWSTR) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: u32::try_from(TOOL_INFO_V2_SIZE).unwrap_or(0),
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: self.window,
            uId: control.0 as usize,
            lpszText: text,
            ..Default::default()
        }
    }

    /// What the error banner SHOWS when it is visible, `None` when it is hidden —
    /// so a caller cannot mistake a stale string for a shown banner. (Named for
    /// the WIDGET, not for the core rule `error_banner_text`, which decides the
    /// wording and lives in `werust-core` where it belongs.)
    #[must_use]
    pub fn visible_banner_text(&self) -> Option<String> {
        is_visible(self.error_banner).then(|| window_text(self.error_banner))
    }
}
