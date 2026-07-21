//! The T0 render pipeline: source HTML → painted [`Surface`], end to end.
//!
//! This ties the four T0 stages into the one call the backend makes per load
//! (`docs/conformance-tiers.md` T0): tokenize → allowlist tree → cascade (small
//! property set) → block/inline flow → software text. Each stage sits behind its
//! own seam ([`Tokenizer`], [`TreeBuilder`]) or module (cascade, layout, paint),
//! so this function is just the wiring; swapping the T0 front-end for T1's
//! html5ever means swapping the [`Tokenizer`]/[`TreeBuilder`] passed in, not
//! rewriting the pipeline.
//!
//! Author CSS is a wrinkle worth noting: the allowlist tree builder DROPS
//! `<style>` (it is not on the v0 element allowlist), so the stylesheet cannot be
//! recovered from the tree. The pipeline therefore extracts author CSS straight
//! from the token stream (the text inside `<style>…</style>`) before tree building,
//! and feeds it to the cascade. This keeps `<style>` out of the rendered box tree
//! (it must not paint) while still honouring the author rules it carried.

use crate::css::Stylesheet;
use crate::layout::{layout, LayoutResult};
use crate::paint::{paint, Surface};
use crate::tokenizer::{Token, Tokenizer};
use crate::tree::{Dom, TreeBuilder};

/// The default T0 viewport width in px, used when a caller does not specify one.
pub const DEFAULT_VIEWPORT_WIDTH: f32 = 800.0;

/// The intermediate + final products of one render, kept for inspection/testing.
///
/// A render pass is deterministic: the same source and viewport width always yield
/// the same [`Dom`], [`LayoutResult`], and [`Surface`]. Carrying the intermediates
/// (not just the final surface) is what lets the render tests assert at each stage
/// — tree shape, positioned runs, painted pixels — as well as end to end.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// The allowlist DOM the tree builder produced.
    pub dom: Dom,
    /// The positioned text runs layout produced.
    pub layout: LayoutResult,
    /// The painted software-text surface.
    pub surface: Surface,
}

/// Render `source` HTML into a [`RenderOutput`] using the given T0 front-end.
///
/// `tokenizer` + `tree_builder` are the `Tokenizer | TreeBuilder` seam pair; at T0
/// they are the naive subset pair, at T1 they become html5ever. `viewport_width`
/// is the content width block boxes fill and inline content wraps within.
#[must_use]
pub fn render_with(
    tokenizer: &dyn Tokenizer,
    tree_builder: &dyn TreeBuilder,
    source: &str,
    viewport_width: f32,
) -> RenderOutput {
    let tokens = tokenizer.tokenize(source);
    let css = extract_author_css(&tokens);
    let sheet = Stylesheet::parse(&css);
    let dom = tree_builder.build(&tokens);
    let layout = layout(&dom, &sheet, viewport_width);
    let surface = paint(&layout);
    RenderOutput {
        dom,
        layout,
        surface,
    }
}

/// Extract the concatenated author CSS from the token stream.
///
/// Walks the flat tokens and gathers the text between each `<style>` start tag and
/// its matching `</style>` end tag. `<style>` is not on the element allowlist, so
/// this is the only place its contents are read; the tree builder drops the tag
/// itself so it never paints.
fn extract_author_css(tokens: &[Token]) -> String {
    let mut css = String::new();
    let mut in_style = false;
    for token in tokens {
        match token {
            Token::StartTag { name, .. } if name == "style" => in_style = true,
            Token::EndTag { name } if name == "style" => in_style = false,
            Token::Text(text) if in_style => css.push_str(text),
            _ => {}
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Color;
    use crate::tokenizer::SubsetTokenizer;
    use crate::tree::AllowlistTreeBuilder;

    fn render(html: &str) -> RenderOutput {
        render_with(
            &SubsetTokenizer::new(),
            &AllowlistTreeBuilder::new(),
            html,
            DEFAULT_VIEWPORT_WIDTH,
        )
    }

    #[test]
    fn renders_subset_document_end_to_end() {
        // A whole tiny v0-subset document: tokenize → tree → cascade → flow →
        // software text, in one pass.
        let out =
            render("<html><body><h1>Hello</h1><p>world <strong>bold</strong></p></body></html>");
        let transcript = out.surface.transcript();
        assert!(transcript.contains("Hello[b]"));
        assert!(transcript.contains("world"));
        assert!(transcript.contains("bold[b]"));
        assert!(!out.layout.runs.is_empty());
        assert!(out.surface.height > 0);
    }

    #[test]
    fn author_style_block_reaches_the_cascade() {
        // The `<style>` block is dropped from the tree but its rule still applies.
        let out = render(
            r#"<html><head><style>p{color:#ff0000}</style></head><body><p>x</p></body></html>"#,
        );
        // No `<style>` node survives in the DOM (it is off the allowlist).
        assert!(!contains_tag(&out.dom, "style"));
        // But the paragraph was painted red by the author rule.
        let run = out
            .layout
            .runs
            .iter()
            .find(|r| r.text.contains('x'))
            .unwrap();
        assert_eq!(run.style.color, Color { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn head_is_not_painted() {
        // `head` is display:none in the UA sheet, so its text never becomes a run.
        let out = render("<html><head>metadata</head><body><p>body</p></body></html>");
        assert!(out.surface.transcript().contains("body"));
        assert!(!out.surface.transcript().contains("metadata"));
    }

    fn contains_tag(dom: &Dom, tag: &str) -> bool {
        fn walk(node: &crate::tree::Node, tag: &str) -> bool {
            match node {
                crate::tree::Node::Element(e) => {
                    e.tag == tag || e.children.iter().any(|c| walk(c, tag))
                }
                crate::tree::Node::Text(_) => false,
            }
        }
        dom.roots.iter().any(|n| walk(n, tag))
    }
}
