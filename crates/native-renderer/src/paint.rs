//! The T1 software text stage: paint positioned shaped runs into a surface.
//!
//! Like T0 this is a deterministic software raster (no GPU), but it paints the
//! **shaped** runs layout produced: each run carries its real proportional advance
//! and line height from the [`Shaper`](crate::shape::Shaper), so a run is drawn as
//! a filled band the width of its shaped advance (a stand-in for the real glyph
//! outlines — rasterising outlines is a later, vello/wgpu concern) in the run's
//! resolved colour, plus its underline and its transcript line.
//!
//! A [`Surface`] also records a plain-text transcript of what was painted (the
//! runs in flow order, with style + colour marks), which is the human-legible,
//! font-independent render assertion the T1 render tests check against — the
//! software equivalent of a golden image without a brittle pixel dependency.

use crate::css::Color;
use crate::layout::{LayoutResult, TextRun, FALLBACK_LINE_HEIGHT};

/// An in-memory RGBA8 raster surface — the native backend's paint target.
///
/// Pixels are row-major RGBA (4 bytes each). T1 paints each run as a filled band
/// the width of its shaped advance in the run's resolved colour (a stand-in for
/// real glyph outlines; outline rasterisation is a later vello/wgpu concern),
/// which is enough to prove the pipeline produces positioned, shaped, coloured,
/// styled output end to end. The [`text`](Surface::text) transcript is the legible
/// companion the render tests assert on.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// Surface width in px.
    pub width: u32,
    /// Surface height in px.
    pub height: u32,
    /// Row-major RGBA8 pixels (`width * height * 4` bytes).
    pub pixels: Vec<u8>,
    /// The painted text runs in flow order (the legible render transcript).
    pub text: Vec<String>,
}

impl Surface {
    /// Create a white, opaque surface of the given size.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let pixels = vec![255u8; (width as usize) * (height as usize) * 4];
        Surface {
            width,
            height,
            pixels,
            text: Vec::new(),
        }
    }

    /// The RGBA of the pixel at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ])
    }

    /// The painted transcript as a single newline-joined string.
    #[must_use]
    pub fn transcript(&self) -> String {
        self.text.join("\n")
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i] = color.r;
        self.pixels[i + 1] = color.g;
        self.pixels[i + 2] = color.b;
        self.pixels[i + 3] = 255;
    }

    /// Fill a rectangle with `color`, clipped to the surface bounds.
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let x0 = x.max(0.0) as u32;
        let y0 = y.max(0.0) as u32;
        let x1 = ((x + w).max(0.0) as u32).min(self.width);
        let y1 = ((y + h).max(0.0) as u32).min(self.height);
        for py in y0..y1 {
            for px in x0..x1 {
                self.set_pixel(px, py, color);
            }
        }
    }
}

/// Paint a laid-out document into a fresh [`Surface`].
///
/// The surface is sized to the layout's viewport width and content height. Each
/// [`TextRun`] is painted as a filled band the width of its shaped advance in its
/// resolved colour, with an extra underline row when the style calls for it. Any
/// run's `background-color` fills its band first. This is the T1 software text
/// output.
#[must_use]
pub fn paint(layout: &LayoutResult) -> Surface {
    let width = layout.width.ceil().max(1.0) as u32;
    let height = layout.height.ceil().max(FALLBACK_LINE_HEIGHT) as u32;
    let mut surface = Surface::new(width, height);

    for run in &layout.runs {
        paint_run(&mut surface, run);
    }
    surface
}

/// Paint one text run: its background, its glyph band, its underline, and its
/// transcript line.
fn paint_run(surface: &mut Surface, run: &TextRun) {
    let line_h = if run.line_height > 0.0 {
        run.line_height
    } else {
        FALLBACK_LINE_HEIGHT
    };
    // The glyph band is the ~75% of the line height text occupies above the
    // descender; the underline sits just below it.
    let glyph_h = line_h * 0.75;

    // Background colour (if any) fills the whole run band first.
    if let Some(bg) = run.style.background_color {
        surface.fill_rect(run.x, run.y, run.advance, line_h, bg);
    }

    // The glyph band: a filled band the width of the shaped advance (a stand-in
    // for glyph outlines). Leading whitespace in the run text is not part of the
    // ink, so inset by the space the leading char(s) occupy is approximated by
    // trimming — the advance already includes the leading space, so start after it.
    let leading_ws = run.text.len() - run.text.trim_start().len();
    let ink_x = if leading_ws > 0 {
        // Approximate the leading-space width as a fraction of the advance.
        run.x + run.advance * (leading_ws as f32 / run.text.chars().count().max(1) as f32)
    } else {
        run.x
    };
    let ink_w = (run.x + run.advance - ink_x).max(0.0);
    // A bold band fills its whole height; a normal band leaves a 1px gutter so
    // weight is visible in the raster.
    let inset = if run.style.bold { 0.0 } else { 1.0 };
    surface.fill_rect(
        ink_x + inset,
        run.y + inset,
        (ink_w - inset).max(0.0),
        glyph_h - inset,
        run.style.color,
    );

    if run.style.underline {
        surface.fill_rect(ink_x, run.y + glyph_h, ink_w, 1.0, run.style.color);
    }
    surface.text.push(transcribe(run));
}

/// Build the transcript line for a run, annotating its active styles + non-default
/// colour so the legible render assertion captures colour/weight/decoration, not
/// just position. The colour mark closes the T0 gap where the transcript did not
/// assert colour (task forward-pointer note): a colour-cascade regression now turns
/// the transcript red.
fn transcribe(run: &TextRun) -> String {
    let mut marks = Vec::new();
    if run.style.bold {
        marks.push("b".to_string());
    }
    if run.style.italic {
        marks.push("i".to_string());
    }
    if run.style.underline {
        marks.push("u".to_string());
    }
    // Record a non-black colour explicitly so a colour regression is visible.
    if run.style.color != Color::BLACK {
        marks.push(format!(
            "#{:02x}{:02x}{:02x}",
            run.style.color.r, run.style.color.g, run.style.color.b
        ));
    }
    let text = run.text.trim();
    if marks.is_empty() {
        text.to_string()
    } else {
        format!("{text}[{}]", marks.join(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Stylesheet;
    use crate::html5ever_parser::Html5everParser;
    use crate::layout::layout;
    use crate::parser::Parser;
    use crate::shape::Shaper;

    fn paint_html(html: &str, width: f32) -> Surface {
        let parsed = Html5everParser::new().parse(html);
        let sheet = Stylesheet::parse(&parsed.author_css);
        let mut shaper = Shaper::new();
        paint(&layout(&parsed.dom, &sheet, width, &mut shaper))
    }

    #[test]
    fn paints_text_into_a_surface() {
        let surface = paint_html("<p>hi</p>", 400.0);
        assert_eq!(surface.width, 400);
        assert!(surface.height >= FALLBACK_LINE_HEIGHT as u32);
        let painted = (0..surface.height)
            .any(|y| (0..surface.width).any(|x| surface.pixel(x, y) != Some([255, 255, 255, 255])));
        assert!(painted, "text was rasterized onto the surface");
    }

    #[test]
    fn transcript_records_flow_order_and_styles() {
        let surface = paint_html("<h1>Title</h1><p>a <em>b</em></p>", 400.0);
        let transcript = surface.transcript();
        assert!(transcript.contains("Title[b]"));
        assert!(transcript.contains("b[i]"));
        let title_at = transcript.find("Title").unwrap();
        let b_at = transcript.find("b[i]").unwrap();
        assert!(title_at < b_at, "flow order preserved in the transcript");
    }

    #[test]
    fn transcript_records_cascaded_color() {
        // The colour gap the forward-pointer flagged: colour is now in the
        // transcript, so a colour-cascade regression turns it red.
        let surface = paint_html(r#"<p style="color:#ff0000">x</p>"#, 400.0);
        assert!(
            surface.transcript().contains("x[#ff0000]"),
            "colour recorded: {}",
            surface.transcript()
        );
    }

    #[test]
    fn colored_text_paints_its_color() {
        let surface = paint_html(r#"<p style="color:#ff0000">x</p>"#, 400.0);
        let has_red = (0..surface.height)
            .any(|y| (0..surface.width).any(|x| surface.pixel(x, y) == Some([255, 0, 0, 255])));
        assert!(has_red, "the run painted in its cascaded color");
    }

    #[test]
    fn background_color_fills_the_run_band() {
        let surface = paint_html(r#"<p style="background-color:#00ff00">x</p>"#, 400.0);
        let has_green = (0..surface.height)
            .any(|y| (0..surface.width).any(|x| surface.pixel(x, y) == Some([0, 255, 0, 255])));
        assert!(has_green, "the run painted its background colour");
    }

    #[test]
    fn underlined_link_paints_an_underline_row() {
        let surface = paint_html("<p><a>link</a></p>", 400.0);
        // <a> is underlined AND blue (UA sheet), so it carries both marks.
        assert!(
            surface.transcript().contains("link[u#0000ee]"),
            "{}",
            surface.transcript()
        );
    }
}
