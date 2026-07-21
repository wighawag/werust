//! The T1 parser: html5ever bound behind the [`Parser`] seam.
//!
//! This is the parse half of conformance tier **T1** (`docs/conformance-tiers.md`,
//! `CONTEXT.md`; task `t1-whatwg-parser-html5ever-behind-tokenizer-seam`): a real
//! WHATWG-algorithm HTML parser replacing the T0 subset tokenizer at the
//! `Tokenizer | TreeBuilder` seam, so REAL documents (not just the v0 allowlist)
//! parse correctly into a [`Dom`].
//!
//! # How it binds
//!
//! html5ever's tokenizer and tree constructor are fused (they share the WHATWG
//! insertion-mode / open-element state), so it cannot consume the T0 [`Token`]
//! stream. It is therefore bound at the whole-front-end [`Parser`] seam, not the
//! two-trait pair: [`Html5everParser::parse`] runs html5ever over the source into
//! the standard `RcDom` (the `Rc<Node>` tree html5ever's own suite uses), then
//! converts that into werust's own owned [`Dom`] — the SAME [`Dom`] the T0 parser
//! produces, so cascade / layout / paint downstream are untouched by the swap.
//!
//! # What the conversion keeps (and drops)
//!
//! werust's [`Dom`] is a static, script-free render tree: elements (tag +
//! attributes + children) and text. The conversion therefore keeps element and
//! text nodes and drops what a static render does not consume — the `Document`
//! wrapper (its element children become the tree roots), doctype, comments, and
//! processing instructions. Tag names and attribute names are lowercased to match
//! the rest of the pipeline (the T0 tokenizer already lowercased them; the cascade
//! and the `<style>`/`<br>` checks compare against lowercase names). Unlike the T0
//! allowlist builder this drops NOTHING by element name: a real `<article>`,
//! `<section>`, `<table>`, or `<nav>` is kept in the tree — which is the whole
//! point of T1.

use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use crate::parser::{author_css_from_dom, ParsedDocument, Parser};
use crate::tree::{Dom, Element, Node};

/// The T1 parser: a real WHATWG HTML parser (html5ever) behind the [`Parser`]
/// seam.
///
/// Construct with [`Html5everParser::new`]. [`parse`](Html5everParser::parse)
/// runs the full WHATWG parse algorithm and yields werust's [`Dom`] plus the
/// document's author CSS (recovered from the `<style>` elements html5ever keeps in
/// the tree). It is a drop-in replacement for the T0
/// [`SubsetParser`](crate::parser::SubsetParser) at the front-end seam.
#[derive(Debug, Default, Clone, Copy)]
pub struct Html5everParser;

impl Html5everParser {
    /// Create the T1 html5ever parser.
    #[must_use]
    pub fn new() -> Self {
        Html5everParser
    }
}

impl Parser for Html5everParser {
    fn parse(&self, source: &str) -> ParsedDocument {
        // Run the real WHATWG parse into the standard Rc-backed DOM. `.one` feeds
        // the whole source and finishes; parse errors are recovered per the spec
        // (that error recovery is exactly what T1 buys over the T0 subset), so
        // this never fails on real-world input.
        let rc_dom: RcDom = parse_document(RcDom::default(), ParseOpts::default())
            .from_utf8()
            .read_from(&mut source.as_bytes())
            .expect("reading from an in-memory slice cannot fail");

        let dom = dom_from_rcdom(&rc_dom);
        let author_css = author_css_from_dom(&dom);
        ParsedDocument { dom, author_css }
    }
}

/// Convert the standard `RcDom` into werust's owned [`Dom`].
///
/// The `RcDom` root is the `Document` node; its element/text children become the
/// tree roots (typically a single `<html>`). The `Document` wrapper, doctype,
/// comments, and processing instructions are dropped: a static render tree does
/// not consume them.
fn dom_from_rcdom(rc_dom: &RcDom) -> Dom {
    let mut roots = Vec::new();
    for child in rc_dom.document.children.borrow().iter() {
        if let Some(node) = convert_node(child) {
            roots.push(node);
        }
    }
    Dom { roots }
}

/// Convert one `RcDom` handle into a werust [`Node`], or `None` for a node kind a
/// static render tree drops (doctype, comment, PI, and the `Document` wrapper —
/// which is never reached here because its children are lifted directly).
fn convert_node(handle: &Handle) -> Option<Node> {
    match &handle.data {
        NodeData::Text { contents } => Some(Node::Text(contents.borrow().to_string())),
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref().to_ascii_lowercase();
            let attrs = attrs
                .borrow()
                .iter()
                .map(|a| {
                    (
                        a.name.local.as_ref().to_ascii_lowercase(),
                        a.value.to_string(),
                    )
                })
                .collect();
            let children = handle
                .children
                .borrow()
                .iter()
                .filter_map(convert_node)
                .collect();
            Some(Node::Element(Element {
                tag,
                attrs,
                children,
            }))
        }
        // Document wrapper, doctype, comments, and processing instructions carry
        // nothing a static render consumes.
        NodeData::Document
        | NodeData::Doctype { .. }
        | NodeData::Comment { .. }
        | NodeData::ProcessingInstruction { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Node;

    /// Find the first element with `tag` anywhere in the tree.
    fn find<'a>(nodes: &'a [Node], tag: &str) -> Option<&'a Element> {
        for node in nodes {
            if let Node::Element(e) = node {
                if e.tag == tag {
                    return Some(e);
                }
                if let Some(found) = find(&e.children, tag) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The concatenated text of an element's descendants.
    fn text_of(element: &Element) -> String {
        let mut out = String::new();
        fn walk(node: &Node, out: &mut String) {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => {
                    for c in &e.children {
                        walk(c, out);
                    }
                }
            }
        }
        for c in &element.children {
            walk(c, &mut out);
        }
        out
    }

    #[test]
    fn parses_a_minimal_document_into_html_head_body() {
        // A real WHATWG parse builds the full html>head/body scaffold even from a
        // bare fragment — the tree the subset builder never produced.
        let parsed = Html5everParser::new().parse("<p>hi</p>");
        let html = find(&parsed.dom.roots, "html").expect("an <html> root");
        assert!(find(&html.children, "head").is_some(), "an implicit <head>");
        let body = find(&html.children, "body").expect("an implicit <body>");
        let p = find(&body.children, "p").expect("<p> lands in <body>");
        assert_eq!(text_of(p), "hi");
    }

    #[test]
    fn keeps_elements_off_the_v0_allowlist() {
        // The point of T1: real elements the T0 allowlist dropped are kept.
        let parsed = Html5everParser::new()
            .parse("<body><article><h1>T</h1><table><tr><td>c</td></tr></table></article></body>");
        assert!(find(&parsed.dom.roots, "article").is_some());
        let table = find(&parsed.dom.roots, "table").expect("<table> is kept at T1");
        // html5ever inserts the implied <tbody> per the WHATWG algorithm.
        assert!(find(&table.children, "tbody").is_some(), "implied <tbody>");
        assert!(find(&table.children, "td").is_some());
    }

    #[test]
    fn recovers_from_misnested_tags_the_whatwg_way() {
        // Error recovery is what T1 buys over the naive T0 builder: mis-nested
        // <b>/<i> are handled by the adoption agency, not dropped or mis-stacked.
        let parsed = Html5everParser::new().parse("<p><b>bold <i>both</b> italic</i></p>");
        // The document still parses into a well-formed tree with the text present.
        let body = find(&parsed.dom.roots, "body").expect("<body>");
        let flat = text_of(body);
        assert!(flat.contains("bold"));
        assert!(flat.contains("both"));
        assert!(flat.contains("italic"));
    }

    #[test]
    fn lowercases_tag_and_attribute_names() {
        let parsed = Html5everParser::new().parse("<DIV CLASS=\"x\">t</DIV>");
        let div = find(&parsed.dom.roots, "div").expect("<div> lowercased");
        assert_eq!(div.attr("class"), Some("x"));
    }

    #[test]
    fn extracts_author_css_from_the_style_element() {
        // html5ever keeps <style> in the tree; the parser recovers its CSS.
        let parsed = Html5everParser::new()
            .parse("<html><head><style>p{color:red}</style></head><body><p>x</p></body></html>");
        assert_eq!(parsed.author_css, "p{color:red}");
    }

    #[test]
    fn decodes_named_and_numeric_entities() {
        // A real parser has the full named-entity table (T0 had a tiny fixed set).
        let parsed = Html5everParser::new().parse("<p>a &amp; b &copy; c &#233;</p>");
        let p = find(&parsed.dom.roots, "p").expect("<p>");
        let text = text_of(p);
        assert!(text.contains("a & b"));
        assert!(text.contains('\u{00a9}'), "&copy; decoded: {text}");
        assert!(text.contains('\u{00e9}'), "&#233; decoded: {text}");
    }

    #[test]
    fn drops_comments_and_doctype_from_the_render_tree() {
        let parsed = Html5everParser::new()
            .parse("<!doctype html><!-- c --><html><body><p>x</p></body></html>");
        // No comment text leaked into the tree; the tree is rooted at <html>.
        assert!(find(&parsed.dom.roots, "html").is_some());
        let body = find(&parsed.dom.roots, "body").expect("<body>");
        assert_eq!(text_of(body), "x");
    }
}
