//! The T1 text-shaping stage: real Latin/LTR shaping with `parley`.
//!
//! This is the shaping half of conformance tier **T1** (`docs/conformance-tiers.md`,
//! `CONTEXT.md`; task `t1-core-css-stylo-and-latin-shaping-parley`). It replaces the
//! T0 fixed monospace metric (`CHAR_WIDTH` * character count) with REAL shaping:
//! [`parley`] (over fontique/harfrust/skrifa, the pure-Rust stack's shaping arm)
//! turns a styled string into shaped glyphs with real, proportional advances and
//! real font metrics (ascent / descent / line height), so layout measures and
//! line-breaks the way a browser does — `Hello` is not five equal cells.
//!
//! # Deterministic by construction: one bundled font
//!
//! Shaping against the SYSTEM font set would make every advance (and therefore
//! wrapping and the render transcript) depend on whatever fonts the host happens to
//! have — non-reproducible across dev machines and CI. So the [`Shaper`] registers
//! exactly ONE bundled font ([`assets/DejaVuSans.ttf`], Bitstream Vera licence,
//! freely redistributable) into parley's `FontContext` and shapes against it only.
//! Bold and italic are SYNTHESISED by parley from that single regular face
//! (emboldening / skew), so one face covers the T1 emphasis set. See the spike
//! README, decision D2.
//!
//! # Scope: Latin/LTR only
//!
//! Complex-script and bidi shaping are explicitly T2, so parley's `complex-scripts`
//! feature is left off and this module shapes left-to-right runs. That is the T1
//! bar (`docs/conformance-tiers.md`: "real shaping for Latin/LTR text … complex
//! scripts, bidi … is T2").
//!
//! # What layout consumes
//!
//! [`Shaper::measure`] shapes one styled word/run and returns its [`ShapedRun`]:
//! the advance width in px and the font metrics of the line it would sit on. Layout
//! ([`crate::layout`]) uses the advance for inline positioning + wrapping and the
//! metrics for the block line height, so the whole flow is driven by real font
//! geometry rather than the fixed T0 cell.

use std::sync::OnceLock;

use parley::{
    FontContext, FontFamily, FontStyle, FontWeight, Layout, LayoutContext, LineHeight,
    PositionedLayoutItem, StyleProperty,
};

use crate::css::ComputedStyle;

/// The bundled deterministic shaping font (Bitstream Vera licence — freely
/// redistributable; see `assets/LICENSE-DejaVu.txt`). Embedded so shaping needs no
/// system fonts and is byte-identical in every environment (decision D2).
const BUNDLED_FONT: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// The measured result of shaping one styled run: its advance + line metrics.
///
/// `advance` is the shaped inline width in px (proportional — real glyph advances,
/// not a monospace cell). The metric fields describe the line box the run sits on:
/// `ascent` above the baseline, `descent` below, and `line_height` the block
/// advance a line of this run occupies. Layout uses `advance` for inline flow /
/// wrapping and `line_height` for block stacking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedRun {
    /// The shaped inline advance width in px.
    pub advance: f32,
    /// Typographic ascent (px above the baseline).
    pub ascent: f32,
    /// Typographic descent (px below the baseline).
    pub descent: f32,
    /// The block line height in px (the vertical advance one line occupies).
    pub line_height: f32,
    /// The number of shaped glyphs (a real shaper emits one per cluster, not one
    /// per `char`); carried so tests can assert shaping actually ran.
    pub glyphs: usize,
}

/// Register the bundled font into `font_ctx` and return its family name.
///
/// The font MUST be registered into every `Shaper`'s own `FontContext` (each has
/// its own font collection), so this registers on every call; only the resolved
/// family NAME is cached (it is stable across registrations), so the string is
/// resolved once and cloned thereafter.
fn register_bundled_font(font_ctx: &mut FontContext) -> String {
    static NAME: OnceLock<String> = OnceLock::new();
    let registered = font_ctx
        .collection
        .register_fonts(BUNDLED_FONT.to_vec().into(), None);
    let family_id = registered
        .first()
        .expect("the bundled font registers at least one family")
        .0;
    let name = font_ctx
        .collection
        .family_name(family_id)
        .expect("the registered family has a name")
        .to_string();
    NAME.get_or_init(|| name.clone()).clone()
}

/// The real T1 shaper: a parley shaping context seeded with the bundled font.
///
/// Construct with [`Shaper::new`]. It owns the parley `FontContext` (with exactly
/// the bundled font registered) and a reusable `LayoutContext`; [`measure`](Shaper::measure)
/// shapes one styled run against them. It is `!Sync` (parley's contexts are single
/// threaded), so layout holds one per pass.
pub struct Shaper {
    font_ctx: FontContext,
    layout_ctx: LayoutContext<[u8; 4]>,
    family: String,
}

impl Shaper {
    /// Create a shaper with the bundled font registered.
    #[must_use]
    pub fn new() -> Self {
        let mut font_ctx = FontContext::new();
        let family = register_bundled_font(&mut font_ctx);
        Shaper {
            font_ctx,
            layout_ctx: LayoutContext::new(),
            family,
        }
    }

    /// Shape `text` in `style` and return its measured [`ShapedRun`].
    ///
    /// The run is shaped with the style's resolved `font-size`, weight, and slant
    /// (bold / italic synthesised from the single bundled face), against the
    /// bundled family. The returned advance is the real proportional width and the
    /// metrics are the real font metrics for that size — what layout flows with.
    #[must_use]
    pub fn measure(&mut self, text: &str, style: &ComputedStyle) -> ShapedRun {
        let layout = self.build_layout(text, style);
        let mut advance = 0.0f32;
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        let mut line_height = 0.0f32;
        let mut glyphs = 0usize;
        for line in layout.lines() {
            let m = line.metrics();
            // A single measured run occupies one line; take that line's metrics.
            ascent = ascent.max(m.ascent);
            descent = descent.max(m.descent);
            line_height = line_height.max(m.line_height);
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    advance += glyph_run.advance();
                    glyphs += glyph_run.glyphs().count();
                }
            }
        }
        ShapedRun {
            advance,
            ascent,
            descent,
            line_height,
            glyphs,
        }
    }

    /// Build a single-run parley [`Layout`] for `text` in `style` (no wrapping —
    /// the caller measures one run and does its own line-breaking).
    fn build_layout(&mut self, text: &str, style: &ComputedStyle) -> Layout<[u8; 4]> {
        let mut builder = self
            .layout_ctx
            .ranged_builder(&mut self.font_ctx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(style.font_size));
        builder.push_default(StyleProperty::LineHeight(resolve_line_height(style)));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(&self.family)));
        if style.bold {
            builder.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
        }
        if style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        let mut layout: Layout<[u8; 4]> = builder.build(text);
        // No wrap width: shape the whole run onto one line; layout wraps by word.
        layout.break_all_lines(None);
        layout
    }
}

impl Default for Shaper {
    fn default() -> Self {
        Shaper::new()
    }
}

/// Resolve the cascade's `line-height` into a parley [`LineHeight`].
///
/// The cascade stores `line_height` either as an absolute px value or as `normal`
/// (the `0.0` sentinel). An absolute value maps to [`LineHeight::Absolute`]; the
/// `normal` keyword maps to a font-size-relative 1.2 (the conventional CSS
/// `normal` approximation) so the line box tracks the font size.
fn resolve_line_height(style: &ComputedStyle) -> LineHeight {
    if style.line_height > 0.0 {
        LineHeight::Absolute(style.line_height)
    } else {
        LineHeight::FontSizeRelative(1.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::ComputedStyle;

    fn style(font_size: f32) -> ComputedStyle {
        ComputedStyle {
            font_size,
            ..ComputedStyle::initial()
        }
    }

    #[test]
    fn shapes_latin_text_to_proportional_advances() {
        let mut shaper = Shaper::new();
        // Real proportional shaping: an `i`-heavy word is NARROWER than a
        // `W`-heavy word of the same character count. A monospace metric could not
        // tell them apart; a real shaper does.
        let narrow = shaper.measure("iiii", &style(16.0));
        let wide = shaper.measure("WWWW", &style(16.0));
        assert!(narrow.glyphs == 4 && wide.glyphs == 4, "one glyph per char");
        assert!(
            narrow.advance < wide.advance,
            "proportional shaping: iiii ({}) narrower than WWWW ({})",
            narrow.advance,
            wide.advance
        );
    }

    #[test]
    fn advance_scales_with_font_size() {
        let mut shaper = Shaper::new();
        let small = shaper.measure("Hello", &style(16.0));
        let big = shaper.measure("Hello", &style(32.0));
        assert!(
            big.advance > small.advance * 1.8,
            "doubling font-size roughly doubles advance ({} vs {})",
            small.advance,
            big.advance
        );
        assert!(big.ascent > small.ascent, "metrics scale with size too");
    }

    #[test]
    fn real_font_metrics_drive_line_height() {
        let mut shaper = Shaper::new();
        let run = shaper.measure("Hello", &style(16.0));
        // Real metrics: ascent + descent are non-trivial and the line height covers
        // them. This is what layout stacks lines by, not the fixed T0 LINE_HEIGHT.
        assert!(run.ascent > 0.0 && run.descent > 0.0);
        assert!(run.line_height >= run.ascent + run.descent);
        // A `normal` line-height at 16px is around 18-20px for DejaVu Sans.
        assert!(
            run.line_height > 16.0 && run.line_height < 24.0,
            "plausible normal line height at 16px: {}",
            run.line_height
        );
    }

    #[test]
    fn explicit_line_height_is_honoured() {
        let mut shaper = Shaper::new();
        let mut tall = style(16.0);
        tall.line_height = 40.0;
        let run = shaper.measure("Hello", &tall);
        assert!(
            (run.line_height - 40.0).abs() < 1.0,
            "explicit line-height:40px honoured: {}",
            run.line_height
        );
    }

    #[test]
    fn bold_and_italic_shape_without_a_separate_face() {
        // One bundled regular face; parley synthesises bold + italic, so an
        // emphasised run still shapes (non-zero advance, glyphs present) rather
        // than falling back to tofu/empty.
        let mut shaper = Shaper::new();
        let mut bold = style(16.0);
        bold.bold = true;
        let mut italic = style(16.0);
        italic.italic = true;
        for s in [&bold, &italic] {
            let run = shaper.measure("Emphasis", s);
            assert!(run.advance > 0.0 && run.glyphs == 8);
        }
    }

    #[test]
    fn empty_text_measures_to_zero_advance() {
        let mut shaper = Shaper::new();
        let run = shaper.measure("", &style(16.0));
        assert_eq!(run.advance, 0.0);
        assert_eq!(run.glyphs, 0);
    }
}
