//! The `TreeBuilder` seam and the T0 allowlist tree builder.
//!
//! Tree building is the SECOND half of the `Tokenizer | TreeBuilder` seam
//! (`docs/conformance-tiers.md`): it consumes the flat [`Token`](crate::tokenizer::Token)
//! stream a [`Tokenizer`](crate::tokenizer::Tokenizer) produced and assembles a
//! [`Dom`] tree. At T0 the [`AllowlistTreeBuilder`] admits ONLY the fixed v0
//! element allowlist and drops everything else; at T1 a real WHATWG tree
//! constructor (html5ever) is swapped in behind the same [`TreeBuilder`] trait
//! (task `t1-whatwg-parser-html5ever-behind-tokenizer-seam`), and the stages
//! downstream (cascade, layout, paint) keep consuming a [`Dom`] unchanged.
//!
//! The T0 builder is deliberately simple: a stack-based nesting of allowlisted
//! elements, void elements that never open a scope, an implicit `<body>` root, and
//! text nodes. It does NOT implement WHATWG insertion modes, foster parenting, or
//! error recovery — that fidelity is T1.

use crate::tokenizer::Token;

/// The fixed v0 element allowlist (`docs/conformance-tiers.md` T0).
///
/// These are the ONLY element names the T0 tree builder keeps; a start tag for
/// anything else is dropped (its text children are still kept, flattened into the
/// current parent), so an unknown wrapper never derails the subset render. This
/// list is the single source of truth for "which elements T0 renders".
pub const ELEMENT_ALLOWLIST: &[&str] = &[
    "html", "head", "body", "div", "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li",
    "span", "a", "strong", "em", "b", "i", "br",
];

/// The v0 void elements: allowlisted elements that never have children and never
/// open a nesting scope (only `br` at T0).
pub const VOID_ELEMENTS: &[&str] = &["br"];

/// Whether `name` is on the T0 element allowlist.
#[must_use]
pub fn is_allowed(name: &str) -> bool {
    ELEMENT_ALLOWLIST.contains(&name)
}

/// Whether `name` is a v0 void element (no children, no nesting scope).
#[must_use]
pub fn is_void(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

/// A node in the T0 [`Dom`]: either an element or a text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// An allowlisted element with its attributes and children.
    Element(Element),
    /// A run of text.
    Text(String),
}

/// An allowlisted element node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The lowercased tag name (guaranteed to be on [`ELEMENT_ALLOWLIST`]).
    pub tag: String,
    /// The element's attributes, `(name, value)` in source order.
    pub attrs: Vec<(String, String)>,
    /// The element's children in document order.
    pub children: Vec<Node>,
}

impl Element {
    /// The value of attribute `name`, if present.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The T0 document tree: an ordered forest of top-level [`Node`]s.
///
/// The tree builder produces this from the token stream; the cascade + layout
/// stages walk it. It is a plain owned tree (no parent pointers, no interior
/// mutability) because T0 is a static render with no scripting — the DOM
/// object-graph friction the experiment watches for is deliberately not paid here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dom {
    /// The top-level nodes in document order (typically a single `<html>` or an
    /// implicit `<body>`).
    pub roots: Vec<Node>,
}

/// The tree-building stage of the native render path — the second half of the
/// `Tokenizer | TreeBuilder` seam.
///
/// A backend swaps the HTML front-end by swapping the [`Tokenizer`](crate::tokenizer::Tokenizer)
/// and this [`TreeBuilder`] together: T0 uses the [`AllowlistTreeBuilder`]; T1
/// will use html5ever's tree constructor. Everything downstream consumes the
/// [`Dom`], so it is unaffected by the swap.
pub trait TreeBuilder {
    /// Build a [`Dom`] from the flat [`Token`] stream.
    fn build(&self, tokens: &[Token]) -> Dom;
}

/// The T0 allowlist tree builder.
///
/// A stack machine: an allowlisted, non-void start tag pushes a new element scope;
/// a matching end tag pops it; text is appended to the current scope. Non-allowed
/// tags are dropped (unwrapped — their text children flatten into the current
/// parent) so an unsupported wrapper does not lose the subset content it holds.
/// Unmatched end tags are ignored. This is intentionally NOT the WHATWG tree
/// construction algorithm; that is T1.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowlistTreeBuilder;

impl AllowlistTreeBuilder {
    /// Create a T0 allowlist tree builder.
    #[must_use]
    pub fn new() -> Self {
        AllowlistTreeBuilder
    }
}

impl TreeBuilder for AllowlistTreeBuilder {
    fn build(&self, tokens: &[Token]) -> Dom {
        // A stack of open elements. `roots` is the finished top level.
        let mut roots: Vec<Node> = Vec::new();
        let mut stack: Vec<Element> = Vec::new();

        for token in tokens {
            match token {
                Token::StartTag {
                    name,
                    attrs,
                    self_closing,
                } => {
                    if !is_allowed(name) {
                        // Drop the wrapper but keep scanning: its allowed children
                        // and text still land in the current scope.
                        continue;
                    }
                    let element = Element {
                        tag: name.clone(),
                        attrs: attrs.clone(),
                        children: Vec::new(),
                    };
                    if is_void(name) || *self_closing {
                        // Void / self-closed: never opens a scope.
                        append_node(&mut roots, &mut stack, Node::Element(element));
                    } else {
                        stack.push(element);
                    }
                }
                Token::EndTag { name } => {
                    if !is_allowed(name) || is_void(name) {
                        continue;
                    }
                    close_element(&mut roots, &mut stack, name);
                }
                Token::Text(text) => {
                    append_node(&mut roots, &mut stack, Node::Text(text.clone()));
                }
            }
        }

        // Close any still-open elements (well-formed subset input closes its own
        // tags, but T0 tolerates a missing end tag by auto-closing at EOF).
        while let Some(element) = stack.pop() {
            append_node(&mut roots, &mut stack, Node::Element(element));
        }

        Dom { roots }
    }
}

/// Append `node` to the current open element (top of `stack`), or to `roots` if
/// nothing is open.
fn append_node(roots: &mut Vec<Node>, stack: &mut [Element], node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

/// Close the nearest open element whose tag matches `name`, folding it (and any
/// elements still open inside it) into its parent. An end tag with no matching
/// open element is ignored.
fn close_element(roots: &mut Vec<Node>, stack: &mut Vec<Element>, name: &str) {
    // Find the nearest matching open element.
    let Some(index) = stack.iter().rposition(|e| e.tag == name) else {
        return;
    };
    // Pop everything above it too (auto-close mis-nested inner tags), from the top
    // down, so each closed element is folded into what is now its parent.
    while stack.len() > index {
        let element = stack.pop().expect("index in range");
        append_node(roots, stack, Node::Element(element));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::{SubsetTokenizer, Tokenizer};

    fn dom_of(html: &str) -> Dom {
        let tokens = SubsetTokenizer::new().tokenize(html);
        AllowlistTreeBuilder::new().build(&tokens)
    }

    #[test]
    fn builds_a_nested_element_tree() {
        let dom = dom_of("<div><p>hi</p></div>");
        let Node::Element(div) = &dom.roots[0] else {
            panic!("expected div element");
        };
        assert_eq!(div.tag, "div");
        let Node::Element(p) = &div.children[0] else {
            panic!("expected p element");
        };
        assert_eq!(p.tag, "p");
        assert_eq!(p.children, vec![Node::Text("hi".into())]);
    }

    #[test]
    fn drops_non_allowlisted_elements_but_keeps_their_text() {
        // `<script>` and `<table>` are not on the v0 allowlist; their text is kept
        // in place, the wrapper is dropped.
        let dom = dom_of("<p>a<script>keep</script>b</p>");
        let Node::Element(p) = &dom.roots[0] else {
            panic!("expected p");
        };
        assert_eq!(
            p.children,
            vec![
                Node::Text("a".into()),
                Node::Text("keep".into()),
                Node::Text("b".into()),
            ]
        );
    }

    #[test]
    fn void_element_never_opens_a_scope() {
        let dom = dom_of("<p>a<br>b</p>");
        let Node::Element(p) = &dom.roots[0] else {
            panic!("expected p");
        };
        assert_eq!(p.children.len(), 3);
        assert!(
            matches!(&p.children[1], Node::Element(e) if e.tag == "br" && e.children.is_empty())
        );
    }

    #[test]
    fn auto_closes_unclosed_elements_at_eof() {
        let dom = dom_of("<div><p>hi");
        let Node::Element(div) = &dom.roots[0] else {
            panic!("expected div");
        };
        let Node::Element(p) = &div.children[0] else {
            panic!("expected p");
        };
        assert_eq!(p.children, vec![Node::Text("hi".into())]);
    }

    #[test]
    fn ignores_an_unmatched_end_tag() {
        let dom = dom_of("</p><p>ok</p>");
        assert_eq!(dom.roots.len(), 1);
        assert!(matches!(&dom.roots[0], Node::Element(e) if e.tag == "p"));
    }

    #[test]
    fn allowlist_and_void_membership() {
        assert!(is_allowed("div"));
        assert!(is_allowed("strong"));
        assert!(!is_allowed("table"));
        assert!(!is_allowed("script"));
        assert!(is_void("br"));
        assert!(!is_void("div"));
    }
}
