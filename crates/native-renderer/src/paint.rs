//! The T0 software text stage: paint positioned runs into an in-memory surface.
//!
//! This is the "software text" tier of T0 (`docs/conformance-tiers.md`): no GPU,
//! no real font backend — a deterministic software raster. Rather than hinting a
//! real font (that fidelity is T1's parley/cosmic-text), T0 paints each glyph as a
//! fixed monospace cell into an RGBA [`Surface`], so the render is exact,
//! reproducible, and assertable in a headless test. The surface is the native
//! backend's paint output; a real windowing layer blits it, but the seam and the
//! tests only need the pixels.
//!
//! A [`Surface`] also records a plain-text transcript of what was painted (the
//! runs in flow order), which is the human-legible render assertion the T0 render
//! tests check against — the software equivalent of a golden image, without a font
//! dependency.

use crate::css::Color;
use crate::layout::{LayoutResult, TextRun, CHAR_WIDTH, LINE_HEIGHT};

/// An in-memory RGBA8 raster surface — the native backend's paint target.
///
/// Pixels are row-major RGBA (4 bytes each). T0 paints each character as a solid
/// filled cell in the run's resolved colour (a stand-in for a real glyph; real
/// shaping is T1), which is enough to prove the pipeline produces positioned,
/// coloured, styled output end-to-end. The [`text`](Surface::text) transcript is
/// the legible companion the render tests assert on.
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
/// [`TextRun`] is painted as a row of filled monospace cells in its resolved
/// colour, with an extra underline row when the style calls for it (bold widens
/// the cell fill; italic is recorded in the transcript). This is the T0 software
/// text output.
#[must_use]
pub fn paint(layout: &LayoutResult) -> Surface {
    let width = layout.width.ceil().max(1.0) as u32;
    let height = layout.height.ceil().max(LINE_HEIGHT) as u32;
    let mut surface = Surface::new(width, height);

    for run in &layout.runs {
        paint_run(&mut surface, run);
    }
    surface
}

/// Paint one text run: its cells, its underline, and its transcript line.
fn paint_run(surface: &mut Surface, run: &TextRun) {
    let cell_h = LINE_HEIGHT * 0.75;
    let mut x = run.x;
    for ch in run.text.chars() {
        if !ch.is_whitespace() {
            // A bold glyph fills its whole cell; a normal glyph leaves a 1px gutter
            // so weight is visible in the raster.
            let inset = if run.style.bold { 0.0 } else { 1.0 };
            surface.fill_rect(
                x + inset,
                run.y + inset,
                CHAR_WIDTH - inset,
                cell_h - inset,
                run.style.color,
            );
        }
        x += CHAR_WIDTH;
    }
    if run.style.underline {
        let underline_y = run.y + cell_h;
        let w = run.text.chars().count() as f32 * CHAR_WIDTH;
        surface.fill_rect(run.x, underline_y, w, 1.0, run.style.color);
    }
    surface.text.push(transcribe(run));
}

/// Build the transcript line for a run, annotating its active styles so the
/// legible render assertion captures colour/weight/decoration, not just position.
fn transcribe(run: &TextRun) -> String {
    let mut marks = Vec::new();
    if run.style.bold {
        marks.push("b");
    }
    if run.style.italic {
        marks.push("i");
    }
    if run.style.underline {
        marks.push("u");
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
    use crate::layout::layout;
    use crate::tokenizer::{SubsetTokenizer, Tokenizer};
    use crate::tree::{AllowlistTreeBuilder, TreeBuilder};

    fn paint_html(html: &str, width: f32) -> Surface {
        let tokens = SubsetTokenizer::new().tokenize(html);
        let dom = AllowlistTreeBuilder::new().build(&tokens);
        paint(&layout(&dom, &Stylesheet::default(), width))
    }

    #[test]
    fn paints_text_into_a_surface() {
        let surface = paint_html("<p>hi</p>", 400.0);
        assert_eq!(surface.width, 400);
        assert!(surface.height >= LINE_HEIGHT as u32);
        // Some non-white pixel was painted (the text cells).
        let painted = (0..surface.height)
            .any(|y| (0..surface.width).any(|x| surface.pixel(x, y) != Some([255, 255, 255, 255])));
        assert!(painted, "text was rasterized onto the surface");
    }

    #[test]
    fn transcript_records_flow_order_and_styles() {
        let surface = paint_html("<h1>Title</h1><p>a <em>b</em></p>", 400.0);
        let transcript = surface.transcript();
        // Heading is bold; the em fragment is italic; order is document order.
        assert!(transcript.contains("Title[b]"));
        assert!(transcript.contains("b[i]"));
        let title_at = transcript.find("Title").unwrap();
        let b_at = transcript.find("b[i]").unwrap();
        assert!(title_at < b_at, "flow order preserved in the transcript");
    }

    #[test]
    fn colored_text_paints_its_color() {
        let surface = paint_html(r#"<p style="color:#ff0000">x</p>"#, 400.0);
        // At least one pixel is pure red.
        let has_red = (0..surface.height)
            .any(|y| (0..surface.width).any(|x| surface.pixel(x, y) == Some([255, 0, 0, 255])));
        assert!(has_red, "the run painted in its cascaded color");
    }

    #[test]
    fn underlined_link_paints_an_underline_row() {
        let surface = paint_html("<p><a>link</a></p>", 400.0);
        assert!(surface.transcript().contains("link[u]"));
    }
}
