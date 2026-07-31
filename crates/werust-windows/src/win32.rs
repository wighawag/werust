//! The small Win32 vocabulary the chrome is written in: string conversion,
//! control accessors, and the OS-colour-scheme palette.
//!
//! It exists so [`chrome`](crate::chrome), [`debugview`](crate::debugview) and
//! [`window`](crate::window) read as "set this widget to that value" instead of
//! as `SendMessageW` ceremony. Nothing here decides anything about browsing,
//! trust or wording.
//!
//! It also holds the ONE platform call behind the DPI seam,
//! [`window_dpi`](crate::win32::window_dpi): the metrics themselves, and every
//! sum over them, are [`crate::dpi`]'s, where the Ubuntu gate can test them.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, InvalidateRect, ScreenToClient, ANSI_CHARSET,
    CLIP_DEFAULT_PRECIS, DEFAULT_PITCH, DEFAULT_QUALITY, FF_DONTCARE, FW_NORMAL, HBRUSH, HFONT,
    OUT_DEFAULT_PRECIS,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    MoveWindow, SendMessageW, SetWindowTextW, ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE, WM_SETFONT,
};

use renderer::OsColorScheme;

use crate::dpi::Dpi;
use crate::paint::Rgb;

/// A NUL-terminated UTF-16 buffer, the only string shape Win32 takes.
#[must_use]
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Convert one of the shared carrier's colours into a Win32 `COLORREF`
/// (`0x00BBGGRR` — Win32's byte order is the reverse of the hex the palette is
/// written in, which is exactly the kind of transcription this one function
/// exists to do once).
#[must_use]
pub fn colorref(rgb: Rgb) -> COLORREF {
    let channel = |v: f64| ((v * 255.0).round().clamp(0.0, 255.0) as u32) & 0xff;
    COLORREF(channel(rgb.red) | (channel(rgb.green) << 8) | (channel(rgb.blue) << 16))
}

/// Set a control's text.
pub fn set_text(control: HWND, text: &str) {
    let buffer = wide(text);
    unsafe {
        let _ = SetWindowTextW(control, PCWSTR(buffer.as_ptr()));
    }
}

/// Read a control's text back — what the widget actually HOLDS, which is what
/// the CI smoke asserts on.
#[must_use]
pub fn window_text(control: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(control);
        if length <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; (length as usize) + 1];
        let written = GetWindowTextW(control, &mut buffer);
        String::from_utf16_lossy(&buffer[..written.max(0) as usize])
    }
}

/// Show or hide a control, WITHOUT activating it (a repaint must never steal
/// focus from whatever the user is typing into).
pub fn show(control: HWND, visible: bool) {
    unsafe {
        let _ = ShowWindow(control, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
}

/// Whether a control is currently on screen.
#[must_use]
pub fn is_visible(control: HWND) -> bool {
    unsafe { IsWindowVisible(control).as_bool() }
}

/// Enable or grey a control.
pub fn enable(control: HWND, enabled: bool) {
    unsafe {
        let _ = EnableWindow(control, enabled);
    }
}

/// Move a control to a rectangle in its parent's client coordinates.
pub fn place(control: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let _ = MoveWindow(control, x, y, width.max(0), height.max(0), true);
    }
}

/// Ask for a control to be repainted (its colours are chosen at paint time, in
/// `WM_CTLCOLORSTATIC`, so a colour change is applied by invalidating).
pub fn redraw(control: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(control), None, true);
    }
}

/// A window's client rectangle, or a zero rectangle when it has none.
#[must_use]
pub fn client_rect(window: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut rect);
    }
    rect
}

/// One control's rectangle in its PARENT's client coordinates — the same space
/// [`place`] puts it in, which is what makes it comparable with the metrics the
/// layout was computed from.
#[must_use]
pub fn control_rect(parent: HWND, control: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(control, &mut rect);
        let mut top_left = POINT {
            x: rect.left,
            y: rect.top,
        };
        let mut bottom_right = POINT {
            x: rect.right,
            y: rect.bottom,
        };
        let _ = ScreenToClient(parent, &mut top_left);
        let _ = ScreenToClient(parent, &mut bottom_right);
        RECT {
            left: top_left.x,
            top: top_left.y,
            right: bottom_right.x,
            bottom: bottom_right.y,
        }
    }
}

/// The scale of the display this WINDOW is on, as Windows reports it.
///
/// Per-MONITOR (`GetDpiForWindow`), never the process's system DPI: the whole
/// point of the `PerMonitorV2` declaration in `app.manifest` is that a window
/// dragged onto a differently scaled monitor is re-scaled, and a system-DPI read
/// cannot express that. This is the ONE platform call behind
/// [`crate::dpi::Dpi`]; everything downstream of it is pure and testable on the
/// Ubuntu gate.
#[must_use]
pub fn window_dpi(window: HWND) -> Dpi {
    // 0 (an invalid window) is the seam's own fallback to the 96 baseline.
    Dpi::new(unsafe { GetDpiForWindow(window) })
}

/// Give a control the window's font (Win32 controls otherwise inherit the
/// 1980s system font).
pub fn set_font(control: HWND, font: HFONT) {
    unsafe {
        SendMessageW(
            control,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

/// The UI font: the system's own UI face, at the height the DPI seam computed
/// for this display ([`crate::dpi::Metrics::font_height`]).
///
/// `CreateFontW` fixes the height at CREATION, so this is not a value that can be
/// adjusted later: a DPI change means a NEW font, pushed to every control with
/// [`set_font`] and the old one handed to [`release_font`].
#[must_use]
pub fn ui_font(height: i32) -> HFONT {
    let face = wide("Segoe UI");
    unsafe {
        CreateFontW(
            // Negative: Win32's way of asking for a CHARACTER height rather than
            // a cell height.
            -height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            ANSI_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
            PCWSTR(face.as_ptr()),
        )
    }
}

/// Release a font this window created, which is the same `DeleteObject` path
/// [`Theme::release`] takes for its brushes.
///
/// A DPI change creates a NEW `HFONT` (the height cannot be changed in place), so
/// without this every drag between two differently scaled monitors would leak one
/// GDI object.
pub fn release_font(font: HFONT) {
    if font.is_invalid() {
        return;
    }
    unsafe {
        let _ = DeleteObject(font.into());
    }
}

/// The palette for the chrome's OWN surfaces at the current OS colour scheme
/// (`docs/adr/0009`).
///
/// Only [`OsColorScheme::Dark`] paints dark: `Light` AND `NoPreference` are both
/// light, because the shared rule says werust never guesses dark. This type
/// makes no decision of its own — it picks pixels for an answer the one shared
/// mapping already gave.
pub struct Theme {
    /// Whether the OS asked for dark.
    pub dark: bool,
    /// The window's background, also returned from `WM_CTLCOLORSTATIC`.
    pub background: HBRUSH,
    /// An editable field's background.
    pub field: HBRUSH,
    /// Ordinary chrome text.
    pub text: COLORREF,
    /// The colour behind that text (matched to `background`).
    pub back: COLORREF,
    /// An editable field's text.
    pub field_text: COLORREF,
    /// An editable field's background colour.
    pub field_back: COLORREF,
}

impl Theme {
    /// The theme for one OS reading.
    #[must_use]
    pub fn of(scheme: OsColorScheme) -> Self {
        let (back, field_back, text) = if scheme.prefer_dark() {
            (
                COLORREF(0x0020_2020),
                COLORREF(0x002b_2b2b),
                COLORREF(0x00f0_f0f0),
            )
        } else {
            (
                COLORREF(0x00f0_f0f0),
                COLORREF(0x00ff_ffff),
                COLORREF(0x0000_0000),
            )
        };
        Self {
            dark: scheme.prefer_dark(),
            background: unsafe { CreateSolidBrush(back) },
            field: unsafe { CreateSolidBrush(field_back) },
            text,
            back,
            field_text: text,
            field_back,
        }
    }

    /// Release the two brushes this theme owns (a theme is replaced whenever the
    /// OS setting changes, and a leaked GDI brush is a real leak).
    pub fn release(&self) {
        unsafe {
            let _ = DeleteObject(self.background.into());
            let _ = DeleteObject(self.field.into());
        }
    }
}
