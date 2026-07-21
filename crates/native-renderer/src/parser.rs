//! The `Parser` seam: the whole HTML front-end that produces the render [`Dom`].
//!
//! This is the swap point the conformance ladder calls the `Tokenizer | TreeBuilder`
//! seam (`docs/conformance-tiers.md`, `CONTEXT.md`). Everything downstream
//! (cascade, layout, paint) consumes a [`Dom`], so the ONLY thing a tier swaps at
//! the front-end is which [`Parser`] turns source HTML into that [`Dom`]:
//!
//! * **T0** — [`SubsetParser`]: the naive [`SubsetTokenizer`](crate::tokenizer::SubsetTokenizer)
//!   paired with the [`AllowlistTreeBuilder`](crate::tree::AllowlistTreeBuilder).
//!   A fixed v0 subset, no error recovery — the ladder's floor.
//! * **T1** — [`Html5everParser`]: a real WHATWG-algorithm parser (html5ever)
//!   bound behind the SAME seam, so REAL documents parse into a [`Dom`] correctly
//!   (task `t1-whatwg-parser-html5ever-behind-tokenizer-seam`).
//!
//! # Why a `Parser` seam and not the two-trait pair directly
//!
//! At T0 the front-end factors cleanly into a [`Tokenizer`](crate::tokenizer::Tokenizer)
//! (source → flat [`Token`](crate::tokenizer::Token)s) and a
//! [`TreeBuilder`](crate::tree::TreeBuilder) (tokens → [`Dom`]), and those two
//! traits stay the T0 implementation. But a real WHATWG parser (html5ever) does
//! NOT factor at that boundary: its tokenizer and its tree constructor share
//! state (insertion modes, the adoption agency, foster parenting, raw-text state
//! driven by the open-element stack), so you cannot feed it a pre-tokenized
//! [`Token`] stream. The real, load-bearing seam invariant was never "there are
//! exactly two traits" — it is **"everything downstream consumes the [`Dom`]"**.
//! [`Parser`] names that invariant: `source → `[`ParsedDocument`]. The T0 pair
//! composes into a [`SubsetParser`] behind it unchanged; html5ever slots in as a
//! second implementation, and cascade/layout/paint never learn which ran. The
//! full rationale + the rejected alternatives are recorded in
//! `docs/spikes/t1-whatwg-parser-html5ever-behind-tokenizer-seam/README.md`.
//!
//! # Author CSS travels with the tree
//!
//! A [`Parser`] returns both the [`Dom`] and the document's author CSS (the
//! concatenated text of its `<style>` elements) as a [`ParsedDocument`]. The two
//! parsers gather it differently for a real reason:
//!
//! * The T0 [`AllowlistTreeBuilder`](crate::tree::AllowlistTreeBuilder) DROPS
//!   `<style>` (it is off the v0 element allowlist), so the subset parser recovers
//!   the CSS from the token stream before tree building (as the T0 pipeline always
//!   did).
//! * html5ever KEEPS `<style>` in the tree (under `<head>`), so the T1 parser
//!   walks the produced [`Dom`] for `<style>` text instead.
//!
//! Either way the downstream pipeline receives one [`ParsedDocument`] and does not
//! care which parser produced it. `<style>` never paints on either path: the UA
//! sheet sets `head { display: none }` and `<style>` lives under `<head>`.

use crate::tokenizer::{Token, Tokenizer};
use crate::tree::{Dom, Element, Node, TreeBuilder};

/// The product of one parse: the render [`Dom`] plus the document's author CSS.
///
/// Author CSS is the concatenated text of the document's `<style>` elements. It
/// rides alongside the tree because the two [`Parser`]s recover it from different
/// places (token stream at T0, tree walk at T1) but the downstream pipeline wants
/// it uniformly, next to the [`Dom`] it styles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedDocument {
    /// The document tree everything downstream (cascade, layout, paint) consumes.
    pub dom: Dom,
    /// The concatenated author CSS from the document's `<style>` elements.
    pub author_css: String,
}

/// The HTML front-end seam: turn source HTML into a [`ParsedDocument`].
///
/// This is the single swap point between conformance tiers at the front-end. T0
/// uses the [`SubsetParser`]; T1 uses [`Html5everParser`]. Everything downstream
/// consumes the [`ParsedDocument`], so swapping the parser does not touch cascade,
/// layout, or paint.
pub trait Parser {
    /// Parse `source` HTML into a [`ParsedDocument`].
    fn parse(&self, source: &str) -> ParsedDocument;
}

/// The T0 subset parser: the naive tokenizer + allowlist tree builder composed.
///
/// This packages the existing T0 front-end pair behind the [`Parser`] seam
/// unchanged: it tokenizes with a [`Tokenizer`], extracts author CSS from the
/// token stream (because the allowlist tree builder drops `<style>`), then builds
/// the allowlist [`Dom`] with a [`TreeBuilder`]. It is generic over the pair so
/// the exact same composition serves the unit tests and the backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubsetParser<T, B> {
    tokenizer: T,
    tree_builder: B,
}

impl<T: Tokenizer, B: TreeBuilder> SubsetParser<T, B> {
    /// Compose a subset parser from a tokenizer and a tree builder.
    pub fn new(tokenizer: T, tree_builder: B) -> Self {
        SubsetParser {
            tokenizer,
            tree_builder,
        }
    }
}

impl<T: Tokenizer, B: TreeBuilder> Parser for SubsetParser<T, B> {
    fn parse(&self, source: &str) -> ParsedDocument {
        let tokens = self.tokenizer.tokenize(source);
        let author_css = author_css_from_tokens(&tokens);
        let dom = self.tree_builder.build(&tokens);
        ParsedDocument { dom, author_css }
    }
}

/// Extract the concatenated author CSS from a T0 token stream.
///
/// Gathers the text between each `<style>` start tag and its matching `</style>`
/// end tag. At T0 the allowlist tree builder drops `<style>`, so the token stream
/// is the only place its contents survive.
fn author_css_from_tokens(tokens: &[Token]) -> String {
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

/// Extract the concatenated author CSS from a parsed [`Dom`] by walking every
/// `<style>` element and concatenating its text children (the T1 path, where the
/// parser keeps `<style>` in the tree).
pub(crate) fn author_css_from_dom(dom: &Dom) -> String {
    let mut css = String::new();
    for node in &dom.roots {
        collect_style_text(node, &mut css);
    }
    css
}

fn collect_style_text(node: &Node, css: &mut String) {
    match node {
        Node::Element(element) => {
            if element.tag == "style" {
                push_text(element, css);
            } else {
                for child in &element.children {
                    collect_style_text(child, css);
                }
            }
        }
        Node::Text(_) => {}
    }
}

fn push_text(element: &Element, css: &mut String) {
    for child in &element.children {
        match child {
            Node::Text(text) => css.push_str(text),
            Node::Element(inner) => push_text(inner, css),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::SubsetTokenizer;
    use crate::tree::{AllowlistTreeBuilder, Element, Node};

    fn subset() -> SubsetParser<SubsetTokenizer, AllowlistTreeBuilder> {
        SubsetParser::new(SubsetTokenizer::new(), AllowlistTreeBuilder::new())
    }

    #[test]
    fn subset_parser_builds_the_allowlist_tree() {
        let parsed = subset().parse("<div><p>hi</p></div>");
        let Node::Element(div) = &parsed.dom.roots[0] else {
            panic!("expected div");
        };
        assert_eq!(div.tag, "div");
    }

    #[test]
    fn subset_parser_recovers_author_css_from_the_token_stream() {
        // `<style>` is off the T0 allowlist, so it is dropped from the tree; the
        // CSS must still be recovered (from the tokens) as author CSS.
        let parsed = subset()
            .parse("<html><head><style>p{color:red}</style></head><body><p>x</p></body></html>");
        assert_eq!(parsed.author_css, "p{color:red}");
        // And the `<style>` node did not survive into the T0 tree.
        assert!(!contains_tag(&parsed.dom, "style"));
    }

    #[test]
    fn author_css_from_dom_concatenates_style_element_text() {
        // The T1 recovery path: `<style>` is IN the tree; its text is gathered.
        let dom = Dom {
            roots: vec![Node::Element(Element {
                tag: "head".into(),
                attrs: vec![],
                children: vec![Node::Element(Element {
                    tag: "style".into(),
                    attrs: vec![],
                    children: vec![Node::Text("p{color:blue}".into())],
                })],
            })],
        };
        assert_eq!(author_css_from_dom(&dom), "p{color:blue}");
    }

    fn contains_tag(dom: &Dom, tag: &str) -> bool {
        fn walk(node: &Node, tag: &str) -> bool {
            match node {
                Node::Element(e) => e.tag == tag || e.children.iter().any(|c| walk(c, tag)),
                Node::Text(_) => false,
            }
        }
        dom.roots.iter().any(|n| walk(n, tag))
    }
}
