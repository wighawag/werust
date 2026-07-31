//! The `Tokenizer` seam and the T0 naive subset tokenizer.
//!
//! Tokenizing is the FIRST stage of the native render path and the first swap
//! point on the `Tokenizer | TreeBuilder` seam (`docs/conformance-tiers.md`): at
//! T0 a deliberately naive [`SubsetTokenizer`] scans the fixed v0 subset; at T1 a
//! real WHATWG tokenizer (html5ever) is swapped in behind the same [`Tokenizer`]
//! trait without touching the stages downstream (task
//! `t1-whatwg-parser-html5ever-behind-tokenizer-seam`).
//!
//! A [`Tokenizer`] turns a source string into a flat stream of [`Token`]s — open
//! tags (with their raw attributes), close tags, and text runs. It does NOT build
//! a tree or apply the allowlist; that is the [`TreeBuilder`](crate::tree)'s job.
//! Keeping the two apart is what makes the seam a seam: the tree builder consumes
//! [`Token`]s regardless of which tokenizer produced them.
//!
//! The T0 tokenizer is intentionally naive (it is NOT a WHATWG parser): it does
//! no error recovery, no character-reference decoding beyond a tiny fixed set, no
//! `<script>`/`<style>` raw-text state machine beyond a simple scan, and it
//! assumes reasonably well-formed subset input. That narrowness is the point of
//! T0 — the real parser is T1.

/// A single lexical token produced by a [`Tokenizer`].
///
/// The stream is flat: an element appears as a [`StartTag`](Token::StartTag)
/// followed later by its [`EndTag`](Token::EndTag) (void elements like `br` emit
/// only a self-closing start tag), with [`Text`](Token::Text) runs in between.
/// The [`TreeBuilder`](crate::tree) turns this flat stream into a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// An open tag: `<div class="x">`. Carries the lowercased tag name, its raw
    /// attributes (name, value) in source order, and whether it self-closed
    /// (`<br/>`).
    StartTag {
        /// The lowercased tag name (e.g. `"div"`).
        name: String,
        /// The attributes in source order, each `(name, value)` with the name
        /// lowercased and the value already entity-decoded.
        attrs: Vec<(String, String)>,
        /// Whether the tag closed itself (`<br/>`), a hint the tree builder uses
        /// alongside its own void-element knowledge.
        self_closing: bool,
    },
    /// A close tag: `</div>`. Carries the lowercased tag name.
    EndTag {
        /// The lowercased tag name (e.g. `"div"`).
        name: String,
    },
    /// A run of character data between tags, with entity references decoded.
    Text(String),
}

/// The tokenizing stage of the native render path — the first half of the
/// `Tokenizer | TreeBuilder` seam.
///
/// A backend swaps the whole HTML-front-end by swapping the [`Tokenizer`] (and
/// its paired [`TreeBuilder`](crate::tree)) behind this trait: T0 uses the naive
/// [`SubsetTokenizer`]; T1 will use html5ever. Everything downstream (cascade,
/// layout, paint) consumes the resulting tree, not the tokens, so it is unaffected
/// by the swap.
pub trait Tokenizer {
    /// Tokenize `source` into a flat [`Token`] stream.
    fn tokenize(&self, source: &str) -> Vec<Token>;
}

/// The T0 naive subset tokenizer.
///
/// Scans the fixed v0 subset with a small hand-written state machine: it splits
/// on `<`/`>`, lowercases tag names, parses quoted/unquoted attributes, decodes a
/// tiny fixed set of entities, and treats `<!-- … -->` comments and `<!doctype …>`
/// as skippable. It is deliberately NOT a WHATWG tokenizer — no error recovery,
/// no full named-entity table — because T0 is a fixed subset and the real parser
/// is T1.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubsetTokenizer;

impl SubsetTokenizer {
    /// Create a T0 subset tokenizer.
    #[must_use]
    pub fn new() -> Self {
        SubsetTokenizer
    }
}

impl Tokenizer for SubsetTokenizer {
    fn tokenize(&self, source: &str) -> Vec<Token> {
        let chars: Vec<char> = source.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        let n = chars.len();

        while i < n {
            if chars[i] == '<' {
                // A markup construct. Comment / doctype are skipped; otherwise it
                // is a start or end tag.
                if starts_with(&chars, i, "<!--") {
                    i = skip_comment(&chars, i);
                    continue;
                }
                if i + 1 < n && (chars[i + 1] == '!' || chars[i + 1] == '?') {
                    // `<!doctype …>` / processing-instruction-like: skip to `>`.
                    i = skip_to_gt(&chars, i);
                    continue;
                }
                if i + 1 < n && chars[i + 1] == '/' {
                    let (name, next) = read_end_tag(&chars, i);
                    if !name.is_empty() {
                        tokens.push(Token::EndTag { name });
                    }
                    i = next;
                    continue;
                }
                // A start tag (or a stray `<` that is not a tag start).
                if i + 1 < n && is_name_start(chars[i + 1]) {
                    let (token, next) = read_start_tag(&chars, i);
                    tokens.push(token);
                    i = next;
                    continue;
                }
                // A bare `<` that is not part of a tag: treat it as text.
                let (text, next) = read_text(&chars, i, true);
                push_text(&mut tokens, text);
                i = next;
            } else {
                let (text, next) = read_text(&chars, i, false);
                push_text(&mut tokens, text);
                i = next;
            }
        }

        tokens
    }
}

/// Push a text token, dropping runs that are entirely empty (never empty-text
/// tokens, which carry no information downstream).
fn push_text(tokens: &mut Vec<Token>, text: String) {
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
}

/// Whether `chars[at..]` starts with the ASCII `needle`.
fn starts_with(chars: &[char], at: usize, needle: &str) -> bool {
    let needle: Vec<char> = needle.chars().collect();
    if at + needle.len() > chars.len() {
        return false;
    }
    chars[at..at + needle.len()] == needle[..]
}

/// Skip a `<!-- … -->` comment, returning the index just past the closing `-->`.
fn skip_comment(chars: &[char], start: usize) -> usize {
    let mut i = start + 4; // past `<!--`
    while i < chars.len() {
        if starts_with(chars, i, "-->") {
            return i + 3;
        }
        i += 1;
    }
    chars.len()
}

/// Skip to just past the next `>`, returning that index (or end of input).
fn skip_to_gt(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '>' {
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

/// Read a text run starting at `start`. When `first_is_bare_lt` the leading `<`
/// is a stray non-tag `<` consumed as text; the run then continues until the next
/// `<`. Entities are decoded.
fn read_text(chars: &[char], start: usize, first_is_bare_lt: bool) -> (String, usize) {
    let mut i = start;
    let mut raw = String::new();
    if first_is_bare_lt {
        raw.push('<');
        i += 1;
    }
    while i < chars.len() && chars[i] != '<' {
        raw.push(chars[i]);
        i += 1;
    }
    (decode_entities(&raw), i)
}

/// Read an end tag `</name>` starting at the `<`. Returns the lowercased name and
/// the index just past the `>`.
fn read_end_tag(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 2; // past `</`
    let mut name = String::new();
    while i < chars.len() && is_name_char(chars[i]) {
        name.push(chars[i].to_ascii_lowercase());
        i += 1;
    }
    (name, skip_to_gt(chars, i))
}

/// Read a start tag `<name attrs...>` (or `<name .../>`) starting at the `<`.
/// Returns the [`Token::StartTag`] and the index just past the `>`.
fn read_start_tag(chars: &[char], start: usize) -> (Token, usize) {
    let mut i = start + 1; // past `<`
    let mut name = String::new();
    while i < chars.len() && is_name_char(chars[i]) {
        name.push(chars[i].to_ascii_lowercase());
        i += 1;
    }

    let mut attrs = Vec::new();
    let mut self_closing = false;

    loop {
        i = skip_whitespace(chars, i);
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            '>' => {
                i += 1;
                break;
            }
            '/' => {
                self_closing = true;
                i += 1;
            }
            _ => {
                let (attr_name, next) = read_attr_name(chars, i);
                i = next;
                if attr_name.is_empty() {
                    // Not progressing on a name: step past one char to avoid a
                    // stall on malformed input (T0 does not do WHATWG recovery).
                    i += 1;
                    continue;
                }
                i = skip_whitespace(chars, i);
                let value = if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    i = skip_whitespace(chars, i);
                    let (v, next) = read_attr_value(chars, i);
                    i = next;
                    v
                } else {
                    String::new()
                };
                attrs.push((attr_name, decode_entities(&value)));
            }
        }
    }

    (
        Token::StartTag {
            name,
            attrs,
            self_closing,
        },
        i,
    )
}

/// Read an attribute name (lowercased) starting at `at`.
fn read_attr_name(chars: &[char], at: usize) -> (String, usize) {
    let mut i = at;
    let mut name = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
            break;
        }
        name.push(c.to_ascii_lowercase());
        i += 1;
    }
    (name, i)
}

/// Read an attribute value (quoted or unquoted) starting at `at`. The returned
/// value is raw (entity decoding is applied by the caller).
fn read_attr_value(chars: &[char], at: usize) -> (String, usize) {
    let mut i = at;
    if i >= chars.len() {
        return (String::new(), i);
    }
    let quote = chars[i];
    if quote == '"' || quote == '\'' {
        i += 1;
        let mut value = String::new();
        while i < chars.len() && chars[i] != quote {
            value.push(chars[i]);
            i += 1;
        }
        if i < chars.len() {
            i += 1; // past the closing quote
        }
        (value, i)
    } else {
        let mut value = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || c == '>' {
                break;
            }
            value.push(c);
            i += 1;
        }
        (value, i)
    }
}

/// Skip ASCII/Unicode whitespace, returning the first non-whitespace index.
fn skip_whitespace(chars: &[char], at: usize) -> usize {
    let mut i = at;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Whether `c` can start a tag name.
fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// Whether `c` can continue a tag name.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Decode the tiny fixed entity set the T0 subset supports.
///
/// Only the handful of entities a subset fixture actually uses are decoded
/// (`&amp; &lt; &gt; &quot; &#39; &nbsp;` plus numeric `&#NN;` / `&#xHH;`); every
/// other `&…;` is left verbatim. A full named-entity table is a T1 concern.
fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            if let Some((decoded, next)) = decode_one_entity(&chars, i) {
                out.push_str(&decoded);
                i = next;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Try to decode a single entity starting at the `&`. Returns the decoded string
/// and the index just past the `;`, or `None` if it is not a recognized entity.
fn decode_one_entity(chars: &[char], amp: usize) -> Option<(String, usize)> {
    // Find the terminating `;` within a small window.
    let end = (amp + 1..chars.len().min(amp + 12)).find(|&j| chars[j] == ';')?;
    let name: String = chars[amp + 1..end].iter().collect();
    let decoded = match name.as_str() {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" | "#39" => "'".to_string(),
        "nbsp" => "\u{00a0}".to_string(),
        other => {
            let code = if let Some(hex) = other
                .strip_prefix("#x")
                .or_else(|| other.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                // Not `&#x…;`/`&#X…;`, so the only remaining numeric form is
                // decimal — anything without the `#` is a NAME this subset does
                // not know, and `?` returns `None` for it (the caller then emits
                // the entity verbatim).
                let dec = other.strip_prefix('#')?;
                dec.parse::<u32>().ok()?
            };
            char::from_u32(code)?.to_string()
        }
    };
    Some((decoded, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tokens: &[Token]) -> Vec<String> {
        tokens
            .iter()
            .map(|t| match t {
                Token::StartTag { name, .. } => format!("<{name}>"),
                Token::EndTag { name } => format!("</{name}>"),
                Token::Text(t) => format!("#{t}"),
            })
            .collect()
    }

    #[test]
    fn tokenizes_nested_tags_and_text() {
        let toks = SubsetTokenizer::new().tokenize("<p>hi <strong>there</strong></p>");
        assert_eq!(
            names(&toks),
            vec!["<p>", "#hi ", "<strong>", "#there", "</strong>", "</p>"]
        );
    }

    #[test]
    fn parses_attributes_quoted_and_unquoted() {
        let toks = SubsetTokenizer::new().tokenize(r#"<a href="x" title=hello>t</a>"#);
        match &toks[0] {
            Token::StartTag { name, attrs, .. } => {
                assert_eq!(name, "a");
                assert_eq!(
                    attrs,
                    &vec![
                        ("href".to_string(), "x".to_string()),
                        ("title".to_string(), "hello".to_string())
                    ]
                );
            }
            other => panic!("expected start tag, got {other:?}"),
        }
    }

    #[test]
    fn marks_self_closing_void_tag() {
        let toks = SubsetTokenizer::new().tokenize("a<br/>b");
        assert!(matches!(
            &toks[1],
            Token::StartTag {
                name,
                self_closing: true,
                ..
            } if name == "br"
        ));
    }

    #[test]
    fn skips_comments_and_doctype() {
        let toks = SubsetTokenizer::new().tokenize("<!doctype html><!-- hi --><p>x</p>");
        assert_eq!(names(&toks), vec!["<p>", "#x", "</p>"]);
    }

    #[test]
    fn decodes_the_small_entity_set() {
        let toks = SubsetTokenizer::new().tokenize("<p>a &amp; b &lt;c&gt; &#39;q&#39;</p>");
        assert_eq!(names(&toks), vec!["<p>", "#a & b <c> 'q'", "</p>"]);
    }

    #[test]
    fn decodes_numeric_entities_and_leaves_the_rest_verbatim() {
        // Pins every branch of `decode_one_entity`'s numeric tail: hex (both
        // `&#x…;` and `&#X…;`), decimal, an unrecognized NAME, and a `#` form
        // whose digits do not parse. Written before that block was rewritten
        // with `?` (task `pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main`,
        // clippy 1.97's `question_mark`), so the rewrite is provably a
        // simplification and not a behaviour change.
        let toks = SubsetTokenizer::new().tokenize("<p>&#x41;&#X42;&#67; &copy; &#zz;</p>");
        assert_eq!(names(&toks), vec!["<p>", "#ABC &copy; &#zz;", "</p>"]);
    }

    #[test]
    fn lowercases_tag_names() {
        let toks = SubsetTokenizer::new().tokenize("<DIV><P>x</P></DIV>");
        assert_eq!(names(&toks), vec!["<div>", "<p>", "#x", "</p>", "</div>"]);
    }
}
