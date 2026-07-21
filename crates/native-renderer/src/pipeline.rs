//! The T0 render pipeline: source HTML → painted [`Surface`], end to end.
//!
//! This ties the render stages into the one call the backend makes per load
//! (`docs/conformance-tiers.md`): parse → cascade (small property set) →
//! block/inline flow → software text. The front-end sits behind the
//! [`Parser`] seam and the rest behind modules (cascade, layout, paint), so this
//! function is just the wiring; swapping the T0 subset front-end for T1's
//! html5ever means passing a different [`Parser`], not rewriting the pipeline.
//!
//! Author CSS travels WITH the parse: a [`Parser`] returns the
//! [`ParsedDocument`]'s `author_css` (the concatenated text of the document's
//! `<style>` elements) alongside the [`Dom`], so the pipeline feeds it to the
//! cascade without caring HOW it was recovered (the T0 parser reads it from the
//! token stream because the allowlist builder drops `<style>`; the T1 parser walks
//! the tree html5ever keeps it in). `<style>` never paints either way: the UA
//! sheet sets `head { display: none }` and `<style>` lives under `<head>`.

use crate::css::Stylesheet;
use crate::layout::{layout, LayoutResult};
use crate::paint::{paint, Surface};
use crate::parser::Parser;
use crate::tree::Dom;

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

/// Render `source` HTML into a [`RenderOutput`] using the given front-end.
///
/// `parser` is the [`Parser`] seam: at T0 the naive subset parser, at T1
/// html5ever. `viewport_width` is the content width block boxes fill and inline
/// content wraps within.
#[must_use]
pub fn render_with(parser: &dyn Parser, source: &str, viewport_width: f32) -> RenderOutput {
    let parsed = parser.parse(source);
    let sheet = Stylesheet::parse(&parsed.author_css);
    let layout = layout(&parsed.dom, &sheet, viewport_width);
    let surface = paint(&layout);
    RenderOutput {
        dom: parsed.dom,
        layout,
        surface,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Color;
    use crate::html5ever_parser::Html5everParser;
    use crate::parser::SubsetParser;
    use crate::tokenizer::SubsetTokenizer;
    use crate::tree::AllowlistTreeBuilder;

    fn render(html: &str) -> RenderOutput {
        let parser = SubsetParser::new(SubsetTokenizer::new(), AllowlistTreeBuilder::new());
        render_with(&parser, html, DEFAULT_VIEWPORT_WIDTH)
    }

    fn render_t1(html: &str) -> RenderOutput {
        render_with(&Html5everParser::new(), html, DEFAULT_VIEWPORT_WIDTH)
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

    #[test]
    fn same_pipeline_renders_via_the_t1_parser_swap() {
        // The whole point of the seam: swapping the T0 subset parser for html5ever
        // is the ONLY change; cascade/layout/paint are untouched and still paint
        // the styled content. Same source, different `Parser`, real render.
        let out =
            render_t1("<html><body><h1>Hello</h1><p>world <strong>bold</strong></p></body></html>");
        let transcript = out.surface.transcript();
        assert!(
            transcript.contains("Hello[b]"),
            "heading bold: {transcript}"
        );
        assert!(transcript.contains("world"));
        assert!(transcript.contains("bold[b]"), "strong bold: {transcript}");
        assert!(!out.layout.runs.is_empty());
        assert!(out.surface.height > 0);
    }

    #[test]
    fn t1_parser_renders_a_real_document_off_the_v0_subset() {
        // A real document using semantic elements the T0 allowlist would DROP
        // (`<article>`, `<header>`, `<h2>` inside them), an author `<style>` block
        // html5ever keeps in the tree, and named entities beyond the T0 set. It
        // parses and paints via the native path end to end.
        let out = render_t1(
            "<!doctype html><html><head><title>Doc</title>\
             <style>.lead{color:#008000}</style></head><body>\
             <article><header><h1>Real &amp; Static</h1></header>\
             <p class=\"lead\">An <em>article</em> with <strong>real</strong> markup &copy; 2026.</p>\
             <ul><li>alpha</li><li>beta</li></ul></article></body></html>",
        );
        let transcript = out.surface.transcript();
        // Title lives in <head> (display:none) and must not paint.
        assert!(
            !transcript.contains("Doc"),
            "head title not painted: {transcript}"
        );
        // The <h1> is bold (UA sheet) and the `&amp;` entity is decoded to `&`
        // (layout emits one run per word, so `Real`/`&`/`Static` are separate).
        assert!(transcript.contains("Real[b]"), "h1 bold: {transcript}");
        assert!(
            transcript.contains("&[b]"),
            "decoded entity in h1: {transcript}"
        );
        assert!(transcript.contains("Static[b]"), "h1 bold: {transcript}");
        assert!(transcript.contains("article[i]"), "em italic: {transcript}");
        assert!(transcript.contains("real[b]"), "strong bold: {transcript}");
        assert!(
            transcript.contains('\u{00a9}'),
            "&copy; decoded: {transcript}"
        );
        assert!(transcript.contains("alpha") && transcript.contains("beta"));
        // The author `.lead` rule (kept via the html5ever tree) coloured the para.
        let lead = out
            .layout
            .runs
            .iter()
            .find(|r| r.text.contains("An"))
            .expect("the lead paragraph run");
        assert_eq!(lead.style.color, Color { r: 0, g: 128, b: 0 });
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
