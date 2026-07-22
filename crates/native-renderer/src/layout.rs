//! The T1 layout stage: block / inline normal flow over shaped text.
//!
//! Layout turns the styled tree into positioned boxes (`docs/conformance-tiers.md`
//! T1: correct static block/inline layout of real documents). It is normal flow
//! only — no floats, flex, grid, or tables (that is T2) — but unlike T0 it flows
//! with **real shaped text**: the [`Shaper`](crate::shape::Shaper) measures each
//! styled word to its real proportional advance and the block line height comes
//! from real font metrics, so `Hello` is not five equal cells and a line of 24px
//! text is taller than a line of 16px text.
//!
//! The model is the classic block/inline one:
//!
//! * A **block** box stacks its block children vertically, each starting on a new
//!   line, separated by the child's `margin-bottom` and offset by the parent's
//!   left padding/margin.
//! * A run of **inline** content (text and inline elements) inside a block flows
//!   left-to-right, each word measured by the shaper, and wraps to a new line when
//!   it would overflow the available width; `<br>` forces a line break.
//! * Each laid-out word becomes a positioned [`TextRun`] carrying its resolved
//!   [`ComputedStyle`] and its shaped advance, so paint draws it correctly.
//!
//! The output is a flat list of positioned [`TextRun`]s plus the total content
//! [`height`](LayoutResult::height).

use crate::css::{cascade, ComputedStyle, Display, Stylesheet};
use crate::shape::Shaper;
use crate::tree::{Dom, Element, Node};

/// A fallback line height in px when a run has no shaped metrics (e.g. an empty
/// block); real lines use the shaper's measured line height.
pub const FALLBACK_LINE_HEIGHT: f32 = 18.0;

/// A positioned run of text produced by layout, ready to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The text of the run (a single line, no embedded newlines).
    pub text: String,
    /// The x position of the run's left edge in px.
    pub x: f32,
    /// The y position of the run's top edge in px.
    pub y: f32,
    /// The shaped advance width of the run in px (proportional, from the shaper).
    pub advance: f32,
    /// The line height of the run in px (from the shaper's font metrics).
    pub line_height: f32,
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
/// style computed from its parent's, with the ancestor path for combinator
/// matching) as layout descends, and each word is shaped by `shaper`, so this is
/// the single pass that ties cascade -> shaping -> flow together for T1.
#[must_use]
pub fn layout(
    dom: &Dom,
    sheet: &Stylesheet,
    viewport_width: f32,
    shaper: &mut Shaper,
) -> LayoutResult {
    let mut ctx = LayoutCtx {
        runs: Vec::new(),
        cursor_y: 0.0,
        viewport_width,
        sheet,
        shaper,
    };
    let root_style = ComputedStyle::initial();
    ctx.layout_block_children(&dom.roots, &root_style, &[], 0.0, viewport_width);
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
    shaper: &'a mut Shaper,
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
    /// `ancestors` is the element ancestor path (nearest-first) of `nodes`' parent,
    /// used by the cascade for combinator matching.
    fn layout_block_children(
        &mut self,
        nodes: &[Node],
        parent_style: &ComputedStyle,
        ancestors: &[&Element],
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
                    let style = cascade(element, parent_style, ancestors, self.sheet);
                    match style.display {
                        Display::None => {}
                        Display::Inline => {
                            if element.tag == "br" {
                                pending.push(Inline::Break);
                            } else {
                                self.collect_inline(element, &style, ancestors, &mut pending);
                            }
                        }
                        Display::Block => {
                            self.flush_inline(&mut pending, x, width);
                            self.layout_block(element, &style, ancestors, x, width);
                        }
                    }
                }
            }
        }
        self.flush_inline(&mut pending, x, width);
    }

    /// Lay out a single block element: indent by its left margin+padding, stack its
    /// children, then advance by its own bottom margin.
    fn layout_block(
        &mut self,
        element: &Element,
        style: &ComputedStyle,
        ancestors: &[&Element],
        x: f32,
        width: f32,
    ) {
        // Top padding pushes the content down; left margin+padding indent it.
        self.cursor_y += style.padding.top + style.margin.top;
        let inner_x = x + style.margin.left + style.padding.left;
        let inner_width = (width
            - style.margin.left
            - style.margin.right
            - style.padding.left
            - style.padding.right)
            .max(0.0);

        let mut child_ancestors: Vec<&Element> = Vec::with_capacity(ancestors.len() + 1);
        child_ancestors.push(element);
        child_ancestors.extend_from_slice(ancestors);

        self.layout_block_children(
            &element.children,
            style,
            &child_ancestors,
            inner_x,
            inner_width,
        );
        self.cursor_y += style.padding.bottom + style.margin.bottom;
    }

    /// Gather an inline element's content (recursively) into the pending run.
    fn collect_inline(
        &mut self,
        element: &Element,
        style: &ComputedStyle,
        ancestors: &[&Element],
        pending: &mut Vec<Inline>,
    ) {
        let mut child_ancestors: Vec<&Element> = Vec::with_capacity(ancestors.len() + 1);
        child_ancestors.push(element);
        child_ancestors.extend_from_slice(ancestors);

        for child in &element.children {
            match child {
                Node::Text(text) => push_words(pending, text, style),
                Node::Element(child_el) => {
                    let child_style = cascade(child_el, style, &child_ancestors, self.sheet);
                    match child_style.display {
                        Display::None => {}
                        Display::Block => {
                            self.flush_inline(pending, 0.0, self.viewport_width);
                            self.layout_block(
                                child_el,
                                &child_style,
                                &child_ancestors,
                                0.0,
                                self.viewport_width,
                            );
                        }
                        Display::Inline => {
                            if child_el.tag == "br" {
                                pending.push(Inline::Break);
                            } else {
                                self.collect_inline(
                                    child_el,
                                    &child_style,
                                    &child_ancestors,
                                    pending,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Break the pending inline fragments into lines and emit positioned runs,
    /// shaping each word for its real advance and the line's real height.
    fn flush_inline(&mut self, pending: &mut Vec<Inline>, x: f32, width: f32) {
        if pending.is_empty() {
            return;
        }
        let mut cursor_x = x;
        let mut line_has_content = false;
        let mut line_height = 0.0f32;
        let max_x = x + width;
        // The space advance in the current style, measured lazily per word style.

        for item in pending.drain(..) {
            match item {
                Inline::Break => {
                    self.cursor_y += line_height.max(FALLBACK_LINE_HEIGHT);
                    cursor_x = x;
                    line_has_content = false;
                    line_height = 0.0;
                }
                Inline::Word { text, style } => {
                    let shaped = self.shaper.measure(&text, &style);
                    let space = if line_has_content {
                        self.shaper.measure(" ", &style).advance
                    } else {
                        0.0
                    };
                    // Wrap if this word would overflow and the line already has
                    // content (never leave a line empty to wrap one long word).
                    if line_has_content && cursor_x + space + shaped.advance > max_x {
                        self.cursor_y += line_height.max(FALLBACK_LINE_HEIGHT);
                        cursor_x = x;
                        line_has_content = false;
                        line_height = 0.0;
                    }
                    let (run_text, leading) = if line_has_content {
                        (format!(" {text}"), space)
                    } else {
                        (text, 0.0)
                    };
                    let advance = leading + shaped.advance;
                    line_height = line_height.max(shaped.line_height);
                    self.runs.push(TextRun {
                        text: run_text,
                        x: cursor_x,
                        y: self.cursor_y,
                        advance,
                        line_height: shaped.line_height,
                        style,
                    });
                    cursor_x += advance;
                    line_has_content = true;
                }
            }
        }
        self.cursor_y += line_height.max(FALLBACK_LINE_HEIGHT);
    }
}

/// Split `text` into whitespace-delimited words and push them onto `pending`.
///
/// Collapsing runs of whitespace to single word boundaries is the T1 white-space
/// model (`white-space: normal`): leading/trailing/interior whitespace collapses,
/// and word boundaries become the single spaces line-breaking reinserts.
fn push_words(pending: &mut Vec<Inline>, text: &str, style: &ComputedStyle) {
    for word in text.split_whitespace() {
        pending.push(Inline::Word {
            text: word.to_string(),
            style: style.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Stylesheet;
    use crate::html5ever_parser::Html5everParser;
    use crate::parser::Parser;
    use crate::shape::Shaper;

    fn layout_html(html: &str, width: f32) -> LayoutResult {
        let parsed = Html5everParser::new().parse(html);
        let sheet = Stylesheet::parse(&parsed.author_css);
        let mut shaper = Shaper::new();
        layout(&parsed.dom, &sheet, width, &mut shaper)
    }

    #[test]
    fn stacks_block_paragraphs_vertically() {
        let result = layout_html("<p>one</p><p>two</p>", 400.0);
        let ys: Vec<f32> = result.runs.iter().map(|r| r.y).collect();
        assert_eq!(result.runs.len(), 2);
        assert!(ys[1] > ys[0], "second paragraph below the first");
        assert!(result.height > 2.0 * FALLBACK_LINE_HEIGHT);
    }

    #[test]
    fn inline_content_shares_one_line_with_shaped_advances() {
        let result = layout_html("<p>a <strong>b</strong> c</p>", 400.0);
        let ys: Vec<f32> = result.runs.iter().map(|r| r.y).collect();
        assert!(ys.iter().all(|&y| y == ys[0]), "one inline line");
        assert!(result
            .runs
            .iter()
            .any(|r| r.text.contains('b') && r.style.bold));
        // Real shaping: every run has a positive proportional advance.
        assert!(result.runs.iter().all(|r| r.advance > 0.0));
    }

    #[test]
    fn proportional_widths_differ_from_a_monospace_cell() {
        // `iiii` is narrower than `MMMM` — impossible under a fixed monospace cell,
        // proof that real shaping drives layout width.
        let narrow = layout_html("<p>iiii</p>", 400.0);
        let wide = layout_html("<p>MMMM</p>", 400.0);
        let nadv = narrow.runs[0].advance;
        let wadv = wide.runs[0].advance;
        assert!(nadv < wadv, "iiii ({nadv}) narrower than MMMM ({wadv})");
    }

    #[test]
    fn larger_font_size_makes_a_taller_line() {
        // An h1 (32px UA) line is taller than a body p (16px) line: real font
        // metrics, not a fixed LINE_HEIGHT.
        let doc = layout_html("<h1>Big</h1><p>small</p>", 400.0);
        let h1 = doc.runs.iter().find(|r| r.text.contains("Big")).unwrap();
        let p = doc.runs.iter().find(|r| r.text.contains("small")).unwrap();
        assert!(
            h1.line_height > p.line_height,
            "h1 line ({}) taller than p line ({})",
            h1.line_height,
            p.line_height
        );
    }

    #[test]
    fn long_line_wraps_within_the_viewport() {
        let result = layout_html("<p>word word word word word word</p>", 80.0);
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
        assert!(result.runs[0].text.contains("shown"));
    }

    #[test]
    fn padding_indents_and_offsets_block_content() {
        let plain = layout_html("<div><p>x</p></div>", 400.0);
        let padded = layout_html(r#"<div style="padding: 20px"><p>x</p></div>"#, 400.0);
        let plain_run = &plain.runs[0];
        let padded_run = &padded.runs[0];
        assert!(padded_run.x > plain_run.x, "left padding indents content");
        assert!(
            padded_run.y > plain_run.y,
            "top padding pushes content down"
        );
    }
}
