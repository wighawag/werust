//! The chrome's ONE DPI seam: its metrics as DESIGNED (at 96 DPI), and the
//! arithmetic that turns them into pixels for the display the window is
//! actually on.
//!
//! A pure rule with pure tests, so the one thing this crate computes about
//! GEOMETRY is checked on the Ubuntu `verify` gate rather than discovered on a
//! high-DPI Windows desktop months later — exactly the shape
//! [`profile`](crate::profile) has, and for exactly the same reason.
//!
//! # Why this module exists
//!
//! `crates/werust-windows/app.manifest` declares `PerMonitorV2`. That is a
//! PROMISE to Windows: do not bitmap-scale this process, it scales itself. Until
//! this module the window did not keep it — every rectangle was a raw 96-DPI
//! pixel and the UI font was a hard-coded `-15` — so on a 150%/200% display the
//! chrome drew at 66%/50% of its intended size while WebView2, which does its own
//! DPI handling, drew the PAGE correctly. A correct page in a doll's-house
//! chrome, found on real hardware
//! (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`,
//! defect 1).
//!
//! Removing the manifest would "fix" the SIZE by making Windows bitmap-scale the
//! process, at the cost of a blurry chrome AND a blurry page — the outcome the
//! manifest exists to prevent. So the window keeps the promise instead, and this
//! is where it keeps it.
//!
//! # One seam, not dozens of call sites
//!
//! Every pixel the Win32 half draws comes from a [`Metrics`] field or from
//! [`Metrics::scale`]. There are dozens of sites, and a MISSED one is a subtly
//! misaligned control rather than an obvious failure, so the guard in
//! `tests/windows_window_shape.rs` scans the layouts for any integer literal that
//! did not go through here.
//!
//! # Why the arithmetic is spelled out rather than calling `MulDiv`
//!
//! `MulDiv` is a Win32 function, so a module that called it could only be
//! compiled — let alone tested — on Windows, which is precisely what this seam
//! exists to avoid. [`Dpi::scale`] therefore reproduces `MulDiv`'s contract
//! (multiply in a wider type, then divide rounding half AWAY from zero) in plain
//! Rust, and the tests below pin that rounding, including the half-way cases
//! where a truncating division would differ.

/// The DPI every metric in this module is DESIGNED at, and Windows' own
/// baseline: `USER_DEFAULT_SCREEN_DPI`.
pub const BASELINE_DPI: u32 = 96;

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
/// The toolbar row's inset from the strip's top and bottom edges.
pub const ROW_INSET: i32 = 6;
/// The narrowest the URL bar may be squeezed to.
pub const MIN_URL_WIDTH: i32 = 60;

/// The UI font's height, as a POSITIVE number of pixels (`CreateFontW` takes it
/// negated, which is Win32's way of saying "character height, not cell height").
pub const FONT_HEIGHT: i32 = 15;

/// The browser window's initial size.
pub const DEFAULT_WIDTH: i32 = 1024;
/// The browser window's initial size.
pub const DEFAULT_HEIGHT: i32 = 768;
/// The debug view's initial size.
pub const DEBUG_WIDTH: i32 = 940;
/// The debug view's initial size.
pub const DEBUG_HEIGHT: i32 = 480;
/// The debug view's title label.
pub const DEBUG_TITLE_WIDTH: i32 = 300;
/// The debug view's title label.
pub const DEBUG_TITLE_HEIGHT: i32 = 20;
/// The debug view's CLEAR button.
pub const DEBUG_BUTTON_WIDTH: i32 = 90;
/// The debug view's CLEAR button.
pub const DEBUG_BUTTON_HEIGHT: i32 = 26;
/// Where the debug view's tab control starts, below its title row.
pub const DEBUG_TABS_TOP: i32 = 40;
/// The height the tab control's own strip takes before its page begins.
pub const DEBUG_TAB_STRIP: i32 = 28;

/// One display's scale, as Windows reports it for a WINDOW (per-MONITOR, never
/// the process's system DPI).
///
/// Construct it from `GetDpiForWindow` at the Win32 edge; everything downstream
/// of that call is this type and is testable anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpi(u32);

impl Dpi {
    /// The 96-DPI baseline: an unscaled display, where every design metric is
    /// itself.
    pub const BASELINE: Self = Self(BASELINE_DPI);

    /// One reading. `0` — what `GetDpiForWindow` returns for an invalid window —
    /// falls back to the baseline rather than collapsing every metric to nothing.
    #[must_use]
    pub fn new(dpi: u32) -> Self {
        if dpi == 0 {
            Self::BASELINE
        } else {
            Self(dpi)
        }
    }

    /// The reading itself.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }

    /// A 96-DPI design metric in this display's pixels: `MulDiv(value, dpi, 96)`.
    ///
    /// Multiplies in `i64` (so a large metric at 300% cannot overflow the way
    /// `value * dpi` in `i32` would) and rounds half AWAY from zero, which is
    /// what `MulDiv` does and what keeps a 3-pixel strip from vanishing.
    #[must_use]
    pub fn scale(self, value: i32) -> i32 {
        let numerator = i64::from(value) * i64::from(self.0);
        let denominator = i64::from(BASELINE_DPI);
        let half = denominator / 2;
        let rounded = if numerator >= 0 {
            (numerator + half) / denominator
        } else {
            (numerator - half) / denominator
        };
        i32::try_from(rounded).unwrap_or(if rounded < 0 { i32::MIN } else { i32::MAX })
    }
}

impl Default for Dpi {
    fn default() -> Self {
        Self::BASELINE
    }
}

/// Every chrome metric, in the pixels of one display.
///
/// Built once per layout pass from the window's current [`Dpi`], so a rectangle
/// cannot be computed from a stale scale, and so a DPI CHANGE is nothing more
/// than building a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// The scale these metrics were computed at.
    pub dpi: Dpi,
    /// The gap around chrome items.
    pub margin: i32,
    /// The toolbar strip's height.
    pub toolbar_height: i32,
    /// The error banner's height.
    pub banner_height: i32,
    /// The status line's height.
    pub status_height: i32,
    /// The toolbar row's top edge inside the strip.
    pub row_y: i32,
    /// The height of a control on the toolbar row.
    pub row_height: i32,
    /// A nav button's width.
    pub button_width: i32,
    /// The trust indicator's width.
    pub trust_width: i32,
    /// The invalid-entry badge's width.
    pub badge_width: i32,
    /// The URL bar's progress strip.
    pub progress_height: i32,
    /// The narrowest the URL bar may be squeezed to.
    pub min_url_width: i32,
    /// The UI font's height, positive (see [`FONT_HEIGHT`]).
    pub font_height: i32,
    /// The browser window's initial width.
    pub default_width: i32,
    /// The browser window's initial height.
    pub default_height: i32,
    /// The debug view's initial width.
    pub debug_width: i32,
    /// The debug view's initial height.
    pub debug_height: i32,
    /// The debug view's title label.
    pub debug_title_width: i32,
    /// The debug view's title label.
    pub debug_title_height: i32,
    /// The debug view's CLEAR button.
    pub debug_button_width: i32,
    /// The debug view's CLEAR button.
    pub debug_button_height: i32,
    /// Where the debug view's tab control starts.
    pub debug_tabs_top: i32,
    /// The tab control's own strip height.
    pub debug_tab_strip: i32,
}

impl Metrics {
    /// The chrome's metrics at one display's scale.
    #[must_use]
    pub fn at(dpi: Dpi) -> Self {
        Self {
            dpi,
            margin: dpi.scale(MARGIN),
            toolbar_height: dpi.scale(TOOLBAR_HEIGHT),
            banner_height: dpi.scale(BANNER_HEIGHT),
            status_height: dpi.scale(STATUS_HEIGHT),
            row_y: dpi.scale(ROW_INSET),
            row_height: dpi.scale(TOOLBAR_HEIGHT - 2 * ROW_INSET),
            button_width: dpi.scale(BUTTON_WIDTH),
            trust_width: dpi.scale(TRUST_WIDTH),
            badge_width: dpi.scale(BADGE_WIDTH),
            progress_height: dpi.scale(PROGRESS_HEIGHT),
            min_url_width: dpi.scale(MIN_URL_WIDTH),
            font_height: dpi.scale(FONT_HEIGHT),
            default_width: dpi.scale(DEFAULT_WIDTH),
            default_height: dpi.scale(DEFAULT_HEIGHT),
            debug_width: dpi.scale(DEBUG_WIDTH),
            debug_height: dpi.scale(DEBUG_HEIGHT),
            debug_title_width: dpi.scale(DEBUG_TITLE_WIDTH),
            debug_title_height: dpi.scale(DEBUG_TITLE_HEIGHT),
            debug_button_width: dpi.scale(DEBUG_BUTTON_WIDTH),
            debug_button_height: dpi.scale(DEBUG_BUTTON_HEIGHT),
            debug_tabs_top: dpi.scale(DEBUG_TABS_TOP),
            debug_tab_strip: dpi.scale(DEBUG_TAB_STRIP),
        }
    }

    /// The chrome's metrics for one raw `GetDpiForWindow` reading.
    #[must_use]
    pub fn for_dpi(dpi: u32) -> Self {
        Self::at(Dpi::new(dpi))
    }

    /// One incidental measurement — a gap, a column width, an inset — at this
    /// display's scale.
    ///
    /// The named fields above are the metrics the chrome DESIGNS with; this is
    /// how the small local numbers beside them (`+ 6` between two controls, a
    /// list column) reach the same seam instead of staying raw pixels.
    #[must_use]
    pub fn scale(&self, value: i32) -> i32 {
        self.dpi.scale(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_design_metric_scales_by_the_displays_dpi_against_the_96_baseline() {
        // THE acceptance criterion this module exists for, in the task's own
        // spelling: `scale(8, 144) == 12`.
        assert_eq!(Dpi::new(144).scale(MARGIN), 12);
        // 100%: an unscaled display leaves every metric exactly as designed.
        assert_eq!(Dpi::BASELINE.scale(MARGIN), MARGIN);
        assert_eq!(Dpi::BASELINE.scale(TOOLBAR_HEIGHT), TOOLBAR_HEIGHT);
        // 200%: doubled, which is the display the chrome was drawing at half
        // size on.
        assert_eq!(Dpi::new(192).scale(TOOLBAR_HEIGHT), 80);
        assert_eq!(Dpi::new(192).scale(FONT_HEIGHT), 30);
        assert_eq!(Dpi::new(192).scale(DEFAULT_WIDTH), 2048);
        // 125% and 175%, the other two scales Windows offers by default.
        assert_eq!(Dpi::new(120).scale(MARGIN), 10);
        assert_eq!(Dpi::new(168).scale(BUTTON_WIDTH), 63);
    }

    #[test]
    fn the_rounding_is_mul_divs_and_a_thin_strip_never_vanishes() {
        // `MulDiv` rounds half AWAY from zero; a truncating division would
        // differ on exactly these values, and this is the difference between a
        // 3-pixel progress strip and a 2-pixel one at 150%.
        // 4.5 -> 5, and 3.75 -> 4.
        assert_eq!(Dpi::new(144).scale(PROGRESS_HEIGHT), 5);
        assert_eq!(Dpi::new(120).scale(PROGRESS_HEIGHT), 4);
        // 1.5 -> 2.
        assert_eq!(Dpi::new(144).scale(1), 2);
        // And it is symmetric about zero, so a negative offset (an inset
        // subtracted from a right edge) rounds the same way its positive twin
        // does rather than drifting a pixel.
        assert_eq!(Dpi::new(144).scale(-1), -2);
        assert_eq!(
            Dpi::new(144).scale(-PROGRESS_HEIGHT),
            -Dpi::new(144).scale(PROGRESS_HEIGHT)
        );
    }

    #[test]
    fn an_unreadable_dpi_falls_back_to_the_baseline_rather_than_collapsing() {
        // `GetDpiForWindow` answers 0 for an invalid window. Scaling by 0 would
        // reduce every rectangle to nothing — a window with no chrome at all —
        // so the seam declines and draws the design metrics instead.
        assert_eq!(Dpi::new(0), Dpi::BASELINE);
        assert_eq!(Metrics::for_dpi(0), Metrics::at(Dpi::BASELINE));
    }

    #[test]
    fn every_metric_is_the_design_metric_at_100_percent_and_doubles_at_200() {
        // The whole table, so a field added later without a scale is caught here
        // rather than by eye on a HiDPI display.
        let baseline = Metrics::at(Dpi::BASELINE);
        assert_eq!(baseline.margin, MARGIN);
        assert_eq!(baseline.toolbar_height, TOOLBAR_HEIGHT);
        assert_eq!(baseline.banner_height, BANNER_HEIGHT);
        assert_eq!(baseline.status_height, STATUS_HEIGHT);
        assert_eq!(baseline.button_width, BUTTON_WIDTH);
        assert_eq!(baseline.trust_width, TRUST_WIDTH);
        assert_eq!(baseline.badge_width, BADGE_WIDTH);
        assert_eq!(baseline.progress_height, PROGRESS_HEIGHT);
        assert_eq!(baseline.font_height, FONT_HEIGHT);
        assert_eq!(baseline.default_width, DEFAULT_WIDTH);
        assert_eq!(baseline.default_height, DEFAULT_HEIGHT);
        assert_eq!(baseline.debug_width, DEBUG_WIDTH);
        assert_eq!(baseline.debug_height, DEBUG_HEIGHT);
        assert_eq!(baseline.row_y, ROW_INSET);
        assert_eq!(baseline.row_height, TOOLBAR_HEIGHT - 2 * ROW_INSET);

        let doubled = Metrics::at(Dpi::new(2 * BASELINE_DPI));
        assert_eq!(doubled.margin, 2 * baseline.margin);
        assert_eq!(doubled.toolbar_height, 2 * baseline.toolbar_height);
        assert_eq!(doubled.trust_width, 2 * baseline.trust_width);
        assert_eq!(doubled.font_height, 2 * baseline.font_height);
        assert_eq!(doubled.default_height, 2 * baseline.default_height);
        assert_eq!(doubled.debug_tab_strip, 2 * baseline.debug_tab_strip);
    }

    #[test]
    fn an_incidental_gap_goes_through_the_same_seam_as_a_named_metric() {
        // The `+ 6` between two toolbar controls is as much a pixel as the
        // toolbar's height; it must not stay raw just because it has no name.
        let metrics = Metrics::at(Dpi::new(144));
        assert_eq!(metrics.scale(6), 9);
        assert_eq!(metrics.scale(6), metrics.dpi.scale(6));
        assert_eq!(metrics.scale(0), 0);
    }

    #[test]
    fn a_large_metric_at_a_large_scale_neither_overflows_nor_wraps() {
        // The multiplication happens in i64: `i32::MAX * 384` would wrap in i32,
        // and a wrapped width is a rectangle Win32 would place at random.
        assert_eq!(Dpi::new(384).scale(i32::MAX), i32::MAX);
        assert_eq!(Dpi::new(384).scale(i32::MIN), i32::MIN);
        assert!(Dpi::new(384).scale(DEFAULT_WIDTH) > 0);
    }
}
