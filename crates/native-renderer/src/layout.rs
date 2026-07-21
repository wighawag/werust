//! The T0 layout stage: block / inline normal flow over the computed styles.
//!
//! Layout turns the styled tree into positioned boxes (`docs/conformance-tiers.md`
//! T0: "block/inline flow"). It is normal flow only — no floats, flex, grid, or
//! tables (that is T2) — with a fixed monospace text metric so text advances and
//! line heights are deterministic and testable without a real font backend (real
//! shaping is T1).
//!
//! The model is small and classic:
//!
//! * A **block** box stacks its block children vertically, each on its own line
//!   run, and separates them by the child's `margin-bottom`.
//! * A run of **inline** content (text and inline elements) inside a block flows
//!   left-to-right and wraps to a new line when it exceeds the available width;
//!   `<br>` forces a line break.
//! * Each laid-out glyph run becomes a positioned [`TextRun`] carrying its
//!   resolved [`ComputedStyle`], so paint can draw it with the right colour /
//!   weight / decoration.
//!
//! The output is a flat list of positioned [`TextRun`]s plus the total content
//! [`height`](LayoutResult::height) — exactly what the software text stage paints.

use crate::css::{cascade, ComputedStyle, Display, Stylesheet};
use crate::tree::{Dom, Element, Node};

/// The fixed cell width of one character in px (a monospace T0 metric).
pub const CHAR_WIDTH: f32 = 8.0;

/// The fixed line height in px (a monospace T0 metric).
pub const LINE_HEIGHT: f32 = 16.0;

/// A positioned run of text produced by layout, ready to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The text of the run (a single line, no embedded newlines).
    pub text: String,
    /// The x position of the run's left edge in px.
    pub x: f32,
    /// The y position of the run's top edge in px.
    pub y: f32,
    /// The resolved style paint uses for colour / weight / decoration.
    pub style: ComputedStyle,
}

/// The result of laying out a document: positioned text runs + total height.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutResult {
    /// The positioned text runs in paint order.
    pub runs: Vec<TextRun>,
    /// The total content height in px.
    pub height: f32,
    /// The viewport width the layout was performed against, in px.
    pub width: f32,
}

/// Lay out a styled document into positioned text runs under normal flow.
///
/// `viewport_width` is the available content width in px; block boxes fill it and
/// inline content wraps within it. The cascade is run top-down (each element's
/// style computed from its parent's) as layout descends, so this is the single
/// pass that ties cascade → flow together for T0.
#[must_use]
pub fn layout(dom: &Dom, sheet: &Stylesheet, viewport_width: f32) -> LayoutResult {
    let mut ctx = LayoutCtx {
        runs: Vec::new(),
        cursor_y: 0.0,
        viewport_width,
        sheet,
    };
    let root_style = ComputedStyle::initial();
    // The document roots are laid out as the children of an anonymous block.
    ctx.layout_block_children(&dom.roots, &root_style, 0.0, viewport_width);
    LayoutResult {
        runs: ctx.runs,
        height: ctx.cursor_y,
        width: viewport_width,
    }
}

/// The mutable state threaded through a layout pass.
struct LayoutCtx<'a> {
    runs: Vec<TextRun>,
    cursor_y: f32,
    viewport_width: f32,
    sheet: &'a Stylesheet,
}

/// An inline fragment gathered before line-breaking: a word (or a forced break).
enum Inline {
    /// A whitespace-delimited word with its style.
    Word { text: String, style: ComputedStyle },
    /// A forced line break (`<br>`).
    Break,
}

impl LayoutCtx<'_> {
    /// Lay out a sequence of nodes as the block-flow children of a block box.
    ///
    /// Consecutive inline nodes are gathered into an anonymous line box and flowed
    /// together (so `a <strong>b</strong> c` shares a line); a block child flushes
    /// the pending inline run, then stacks below it.
    fn layout_block_children(
        &mut self,
        nodes: &[Node],
        parent_style: &ComputedStyle,
        x: f32,
        width: f32,
    ) {
        let mut pending: Vec<Inline> = Vec::new();

        for node in nodes {
            match node {
                Node::Text(text) => {
                    push_words(&mut pending, text, parent_style);
                }
                Node::Element(element) => {
                    let style = cascade(element, parent_style, self.sheet);
                    match style.display {
                        Display::None => {}
                        Display::Inline => {
                            if element.tag == "br" {
                                pending.push(Inline::Break);
                            } else {
                                self.collect_inline(element, &style, &mut pending);
                            }
                        }
                        Display::Block => {
                            // Flush the inline run that preceded this block child.
                            self.flush_inline(&mut pending, x, width);
                            self.layout_block(element, &style, x, width);
                        }
                    }
                }
            }
        }
        self.flush_inline(&mut pending, x, width);
    }

    /// Lay out a single block element: its own margin below, its children stacked.
    fn layout_block(&mut self, element: &Element, style: &ComputedStyle, x: f32, width: f32) {
        self.layout_block_children(&element.children, style, x, width);
        self.cursor_y += style.margin_bottom;
    }

    /// Gather an inline element's content (recursively) into the pending run.
    fn collect_inline(
        &mut self,
        element: &Element,
        style: &ComputedStyle,
        pending: &mut Vec<Inline>,
    ) {
        for child in &element.children {
            match child {
                Node::Text(text) => push_words(pending, text, style),
                Node::Element(child_el) => {
                    let child_style = cascade(child_el, style, self.sheet);
                    match child_style.display {
                        Display::None => {}
                        Display::Block => {
                            // A block inside inline content: flush, lay it out.
                            self.flush_inline(pending, 0.0, self.viewport_width);
                            self.layout_block(child_el, &child_style, 0.0, self.viewport_width);
                        }
                        Display::Inline => {
                            if child_el.tag == "br" {
                                pending.push(Inline::Break);
                            } else {
                                self.collect_inline(child_el, &child_style, pending);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Break the pending inline fragments into lines and emit positioned runs.
    fn flush_inline(&mut self, pending: &mut Vec<Inline>, x: f32, width: f32) {
        if pending.is_empty() {
            return;
        }
        let mut cursor_x = x;
        let mut line_has_content = false;
        let max_x = x + width;

        for item in pending.drain(..) {
            match item {
                Inline::Break => {
                    self.cursor_y += LINE_HEIGHT;
                    cursor_x = x;
                    line_has_content = false;
                }
                Inline::Word { text, style } => {
                    let word_width = text.chars().count() as f32 * CHAR_WIDTH;
                    // Wrap if this word would overflow and the line already has
                    // content (never leave a line empty just to wrap a long word).
                    if line_has_content && cursor_x + word_width > max_x {
                        self.cursor_y += LINE_HEIGHT;
                        cursor_x = x;
                        line_has_content = false;
                    }
                    // A leading space between words on the same line.
                    let text = if line_has_content {
                        format!(" {text}")
                    } else {
                        text
                    };
                    let run_width = text.chars().count() as f32 * CHAR_WIDTH;
                    self.runs.push(TextRun {
                        text,
                        x: cursor_x,
                        y: self.cursor_y,
                        style,
                    });
                    cursor_x += run_width;
                    line_has_content = true;
                }
            }
        }
        // The last (or only) line of this inline run occupies one line height.
        self.cursor_y += LINE_HEIGHT;
    }
}

/// Split `text` into whitespace-delimited words and push them onto `pending`.
///
/// Collapsing runs of whitespace to single word boundaries is the T0 white-space
/// model (`white-space: normal`): leading/trailing/interior whitespace collapses,
/// and word boundaries become the single spaces line-breaking reinserts.
fn push_words(pending: &mut Vec<Inline>, text: &str, style: &ComputedStyle) {
    for word in text.split_whitespace() {
        pending.push(Inline::Word {
            text: word.to_string(),
            style: *style,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Stylesheet;
    use crate::tokenizer::{SubsetTokenizer, Tokenizer};
    use crate::tree::{AllowlistTreeBuilder, TreeBuilder};

    fn layout_html(html: &str, width: f32) -> LayoutResult {
        let tokens = SubsetTokenizer::new().tokenize(html);
        let dom = AllowlistTreeBuilder::new().build(&tokens);
        layout(&dom, &Stylesheet::default(), width)
    }

    #[test]
    fn stacks_block_paragraphs_vertically() {
        let result = layout_html("<p>one</p><p>two</p>", 400.0);
        let ys: Vec<f32> = result.runs.iter().map(|r| r.y).collect();
        assert_eq!(result.runs.len(), 2);
        assert!(ys[1] > ys[0], "second paragraph is below the first");
        // Two lines + the first paragraph's bottom margin.
        assert!(result.height >= 2.0 * LINE_HEIGHT);
    }

    #[test]
    fn inline_content_shares_one_line() {
        let result = layout_html("<p>a <strong>b</strong> c</p>", 400.0);
        // All three fragments sit on the same y (one inline line).
        let ys: Vec<f32> = result.runs.iter().map(|r| r.y).collect();
        assert!(ys.iter().all(|&y| y == ys[0]));
        // The bold fragment carries bold style.
        assert!(result
            .runs
            .iter()
            .any(|r| r.text.contains('b') && r.style.bold));
    }

    #[test]
    fn long_line_wraps_within_the_viewport() {
        // Each word is 4 chars = 32px; a 100px viewport fits ~2 words per line.
        let result = layout_html("<p>word word word word</p>", 100.0);
        let distinct_ys: std::collections::BTreeSet<i32> =
            result.runs.iter().map(|r| r.y as i32).collect();
        assert!(distinct_ys.len() >= 2, "text wrapped to multiple lines");
    }

    #[test]
    fn br_forces_a_line_break() {
        let result = layout_html("<p>a<br>b</p>", 400.0);
        let ys: std::collections::BTreeSet<i32> = result.runs.iter().map(|r| r.y as i32).collect();
        assert_eq!(ys.len(), 2, "br split the text across two lines");
    }

    #[test]
    fn display_none_produces_no_runs() {
        let result = layout_html(r#"<p style="display:none">hidden</p><p>shown</p>"#, 400.0);
        assert_eq!(result.runs.len(), 1);
        assert_eq!(result.runs[0].text, "shown");
    }
}
