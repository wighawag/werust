//! The T0 cascade: a real cascade over a small, fixed property set.
//!
//! T0's cascade is deliberately narrow (`docs/conformance-tiers.md`): a handful of
//! properties, a restricted selector set (type, `.class`, `#id`), user-agent
//! defaults for the allowlist elements, author rules from `<style>` blocks, and
//! `style="…"` inline declarations — resolved by specificity + source order into a
//! [`ComputedStyle`] per element. It is a REAL cascade (origin/specificity/order,
//! inheritance of inherited properties), just over a small property set — NOT the
//! full CSS engine, which is T1 (stylo).
//!
//! Supported properties (the fixed T0 set):
//! `display` (`block` | `inline` | `none`), `color`, `font-weight`
//! (`normal` | `bold`), `font-style` (`normal` | `italic`), `text-decoration`
//! (`none` | `underline`), and `margin-bottom` (a length in px). Everything else
//! in a declaration block is ignored. This is enough to render the v0 subset with
//! block/inline flow and styled inline text.

use crate::tree::Element;

/// An sRGB colour, 8 bits per channel, opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Color {
    /// Opaque black — the initial `color`.
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };

    /// Parse a CSS colour from the T0 subset: `#rgb`, `#rrggbb`, or a small set of
    /// named colours. Returns `None` for anything outside the subset.
    #[must_use]
    pub fn parse(value: &str) -> Option<Color> {
        let v = value.trim();
        if let Some(hex) = v.strip_prefix('#') {
            return Color::parse_hex(hex);
        }
        match v.to_ascii_lowercase().as_str() {
            "black" => Some(Color { r: 0, g: 0, b: 0 }),
            "white" => Some(Color {
                r: 255,
                g: 255,
                b: 255,
            }),
            "red" => Some(Color { r: 255, g: 0, b: 0 }),
            "green" => Some(Color { r: 0, g: 128, b: 0 }),
            "blue" => Some(Color { r: 0, g: 0, b: 255 }),
            "gray" | "grey" => Some(Color {
                r: 128,
                g: 128,
                b: 128,
            }),
            _ => None,
        }
    }

    fn parse_hex(hex: &str) -> Option<Color> {
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                Some(Color {
                    r: r * 17,
                    g: g * 17,
                    b: b * 17,
                })
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color { r, g, b })
            }
            _ => None,
        }
    }
}

/// The `display` outer type in the T0 subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// A block-level box (participates in block flow).
    Block,
    /// An inline-level box (participates in inline flow).
    Inline,
    /// Not rendered (and generates no box).
    None,
}

/// The fully-resolved style of one element after the cascade.
///
/// Only the fixed T0 property set is represented; layout + paint read exactly
/// these fields. Inherited properties (`color`, `font-*`, `text-decoration`) flow
/// from parent to child in [`cascade`]; non-inherited ones (`display`,
/// `margin-bottom`) reset to their initial value on each element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedStyle {
    /// Outer display type.
    pub display: Display,
    /// Text colour (inherited).
    pub color: Color,
    /// Whether text is bold (inherited).
    pub bold: bool,
    /// Whether text is italic (inherited).
    pub italic: bool,
    /// Whether text is underlined (inherited).
    pub underline: bool,
    /// Bottom margin in px (not inherited).
    pub margin_bottom: f32,
}

impl ComputedStyle {
    /// The initial (root) style: an inherited baseline before any UA/author rule.
    #[must_use]
    pub fn initial() -> Self {
        ComputedStyle {
            display: Display::Inline,
            color: Color::BLACK,
            bold: false,
            italic: false,
            underline: false,
            margin_bottom: 0.0,
        }
    }
}

/// One parsed CSS declaration in the T0 property set.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Declaration {
    Display(Display),
    Color(Color),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    MarginBottom(f32),
}

/// A restricted T0 selector: a type name, a `.class`, or an `#id`.
///
/// The T0 selector set is deliberately tiny (no combinators, no pseudo-classes) —
/// enough to target the allowlist by element, class, or id. Each selector carries
/// a fixed specificity used to resolve the cascade.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Selector {
    /// A type selector, e.g. `p` (specificity 1).
    Type(String),
    /// A class selector, e.g. `.note` (specificity 10).
    Class(String),
    /// An id selector, e.g. `#main` (specificity 100).
    Id(String),
    /// The universal selector `*` (specificity 0).
    Universal,
}

impl Selector {
    fn specificity(&self) -> u32 {
        match self {
            Selector::Id(_) => 100,
            Selector::Class(_) => 10,
            Selector::Type(_) => 1,
            Selector::Universal => 0,
        }
    }

    fn matches(&self, element: &Element) -> bool {
        match self {
            Selector::Universal => true,
            Selector::Type(name) => element.tag == *name,
            Selector::Id(id) => element.attr("id") == Some(id.as_str()),
            Selector::Class(class) => element
                .attr("class")
                .map(|c| c.split_whitespace().any(|w| w == class))
                .unwrap_or(false),
        }
    }
}

/// One author rule: a selector plus its declarations, tagged with source order.
#[derive(Debug, Clone)]
struct Rule {
    selector: Selector,
    declarations: Vec<Declaration>,
    order: usize,
}

/// A parsed T0 stylesheet: the author rules from all `<style>` blocks.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

impl Stylesheet {
    /// Parse author CSS text (the concatenated contents of the document's
    /// `<style>` blocks) into a T0 stylesheet. Unsupported selectors and
    /// properties are skipped; the parse never fails.
    #[must_use]
    pub fn parse(css: &str) -> Self {
        let mut rules = Vec::new();
        let mut order = 0;
        let mut rest = css;
        while let Some(open) = rest.find('{') {
            let prelude = rest[..open].trim();
            let Some(close) = rest[open + 1..].find('}') else {
                break;
            };
            let body = &rest[open + 1..open + 1 + close];
            let declarations = parse_declarations(body);
            for selector_text in prelude.split(',') {
                if let Some(selector) = parse_selector(selector_text.trim()) {
                    rules.push(Rule {
                        selector,
                        declarations: declarations.clone(),
                        order,
                    });
                    order += 1;
                }
            }
            rest = &rest[open + 1 + close + 1..];
        }
        Stylesheet { rules }
    }
}

/// Parse a single restricted selector.
fn parse_selector(text: &str) -> Option<Selector> {
    if text.is_empty() {
        return None;
    }
    if text == "*" {
        return Some(Selector::Universal);
    }
    if let Some(id) = text.strip_prefix('#') {
        return (!id.is_empty()).then(|| Selector::Id(id.to_string()));
    }
    if let Some(class) = text.strip_prefix('.') {
        return (!class.is_empty()).then(|| Selector::Class(class.to_string()));
    }
    // A bare type selector: only accept a plain identifier (no combinators).
    if text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Some(Selector::Type(text.to_ascii_lowercase()));
    }
    None
}

/// Parse a declaration block body (`prop: value; …`) into the T0 property set.
fn parse_declarations(body: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for decl in body.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        if let Some(parsed) = parse_declaration(prop.trim(), value.trim()) {
            declarations.push(parsed);
        }
    }
    declarations
}

/// Parse one `property: value` pair, or `None` if outside the T0 set.
fn parse_declaration(prop: &str, value: &str) -> Option<Declaration> {
    let v = value.trim().to_ascii_lowercase();
    match prop.to_ascii_lowercase().as_str() {
        "display" => match v.as_str() {
            "block" => Some(Declaration::Display(Display::Block)),
            "inline" => Some(Declaration::Display(Display::Inline)),
            "none" => Some(Declaration::Display(Display::None)),
            _ => None,
        },
        "color" => Color::parse(value).map(Declaration::Color),
        "font-weight" => match v.as_str() {
            "bold" | "bolder" | "700" | "800" | "900" => Some(Declaration::Bold(true)),
            "normal" | "400" => Some(Declaration::Bold(false)),
            _ => None,
        },
        "font-style" => match v.as_str() {
            "italic" | "oblique" => Some(Declaration::Italic(true)),
            "normal" => Some(Declaration::Italic(false)),
            _ => None,
        },
        "text-decoration" | "text-decoration-line" => match v.as_str() {
            "underline" => Some(Declaration::Underline(true)),
            "none" => Some(Declaration::Underline(false)),
            _ => None,
        },
        "margin-bottom" => parse_px(&v).map(Declaration::MarginBottom),
        _ => None,
    }
}

/// Parse a `<number>px` (or bare number) length in px.
fn parse_px(value: &str) -> Option<f32> {
    let v = value.trim().strip_suffix("px").unwrap_or(value.trim());
    v.trim().parse::<f32>().ok()
}

/// The user-agent default declarations for an allowlist element.
///
/// A small UA stylesheet: block-level elements display `block` and headings /
/// paragraphs / lists carry a bottom margin; `strong`/`b` are bold, `em`/`i`
/// italic, `a` underlined. This is the T0 UA sheet author rules cascade over.
fn ua_declarations(tag: &str) -> Vec<Declaration> {
    use Declaration::*;
    match tag {
        // `head` renders nothing; it is display:none in the UA sheet.
        "head" => vec![Display(self::Display::None)],
        "html" | "body" | "div" | "ul" | "ol" | "li" => vec![Display(self::Display::Block)],
        "p" => vec![Display(self::Display::Block), MarginBottom(16.0)],
        "h1" => vec![
            Display(self::Display::Block),
            Bold(true),
            MarginBottom(21.0),
        ],
        "h2" => vec![
            Display(self::Display::Block),
            Bold(true),
            MarginBottom(19.0),
        ],
        "h3" | "h4" | "h5" | "h6" => {
            vec![
                Display(self::Display::Block),
                Bold(true),
                MarginBottom(16.0),
            ]
        }
        "strong" | "b" => vec![Bold(true)],
        "em" | "i" => vec![Italic(true)],
        "a" => vec![Underline(true), Color(self::Color { r: 0, g: 0, b: 238 })],
        _ => vec![],
    }
}

/// Apply a list of declarations onto a style, in order (later wins).
fn apply(style: &mut ComputedStyle, declarations: &[Declaration]) {
    for decl in declarations {
        match *decl {
            Declaration::Display(d) => style.display = d,
            Declaration::Color(c) => style.color = c,
            Declaration::Bold(b) => style.bold = b,
            Declaration::Italic(i) => style.italic = i,
            Declaration::Underline(u) => style.underline = u,
            Declaration::MarginBottom(m) => style.margin_bottom = m,
        }
    }
}

/// Run the T0 cascade for one element, given its parent's computed style.
///
/// This is the real cascade over the small property set: it starts from the
/// inherited baseline (inherited properties flow from `parent`, non-inherited ones
/// reset to their initial value), layers the UA sheet, then the matching author
/// rules sorted by (specificity, source order), then the inline `style="…"`
/// declarations (which win over author rules). The result is the element's
/// [`ComputedStyle`].
#[must_use]
pub fn cascade(element: &Element, parent: &ComputedStyle, sheet: &Stylesheet) -> ComputedStyle {
    // Start from inherited values; reset non-inherited ones to their initial.
    let mut style = ComputedStyle {
        display: Display::Inline,
        color: parent.color,
        bold: parent.bold,
        italic: parent.italic,
        underline: parent.underline,
        margin_bottom: 0.0,
    };

    // 1. UA sheet.
    apply(&mut style, &ua_declarations(&element.tag));

    // 2. Author rules, sorted by specificity then source order.
    let mut matched: Vec<&Rule> = sheet
        .rules
        .iter()
        .filter(|r| r.selector.matches(element))
        .collect();
    matched.sort_by_key(|r| (r.selector.specificity(), r.order));
    for rule in matched {
        apply(&mut style, &rule.declarations);
    }

    // 3. Inline style="…" wins over author rules.
    if let Some(inline) = element.attr("style") {
        apply(&mut style, &parse_declarations(inline));
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Element;

    fn el(tag: &str) -> Element {
        Element {
            tag: tag.into(),
            attrs: vec![],
            children: vec![],
        }
    }

    fn el_attr(tag: &str, k: &str, v: &str) -> Element {
        Element {
            tag: tag.into(),
            attrs: vec![(k.into(), v.into())],
            children: vec![],
        }
    }

    #[test]
    fn ua_sheet_makes_p_a_block_with_margin() {
        let style = cascade(&el("p"), &ComputedStyle::initial(), &Stylesheet::default());
        assert_eq!(style.display, Display::Block);
        assert!(style.margin_bottom > 0.0);
    }

    #[test]
    fn ua_sheet_makes_strong_bold_and_em_italic() {
        let sheet = Stylesheet::default();
        assert!(cascade(&el("strong"), &ComputedStyle::initial(), &sheet).bold);
        assert!(cascade(&el("em"), &ComputedStyle::initial(), &sheet).italic);
        assert!(cascade(&el("a"), &ComputedStyle::initial(), &sheet).underline);
    }

    #[test]
    fn inherited_color_flows_to_children() {
        let mut parent = ComputedStyle::initial();
        parent.color = Color {
            r: 10,
            g: 20,
            b: 30,
        };
        let child = cascade(&el("span"), &parent, &Stylesheet::default());
        assert_eq!(
            child.color,
            Color {
                r: 10,
                g: 20,
                b: 30
            }
        );
    }

    #[test]
    fn non_inherited_margin_does_not_flow() {
        let mut parent = ComputedStyle::initial();
        parent.margin_bottom = 40.0;
        let child = cascade(&el("span"), &parent, &Stylesheet::default());
        assert_eq!(child.margin_bottom, 0.0);
    }

    #[test]
    fn author_rule_overrides_ua_and_specificity_wins() {
        let sheet =
            Stylesheet::parse("p { color: red } .special { color: green } #x { color: blue }");
        let plain = cascade(&el("p"), &ComputedStyle::initial(), &sheet);
        assert_eq!(plain.color, Color::parse("red").unwrap());

        let classed = cascade(
            &el_attr("p", "class", "special"),
            &ComputedStyle::initial(),
            &sheet,
        );
        // .special (specificity 10) beats p (1).
        assert_eq!(classed.color, Color::parse("green").unwrap());

        let ided = cascade(&el_attr("p", "id", "x"), &ComputedStyle::initial(), &sheet);
        // #x (100) beats both.
        assert_eq!(ided.color, Color::parse("blue").unwrap());
    }

    #[test]
    fn inline_style_beats_author_rules() {
        let sheet = Stylesheet::parse("#x { color: green }");
        let mut element = el_attr("p", "id", "x");
        element.attrs.push(("style".into(), "color: red".into()));
        let style = cascade(&element, &ComputedStyle::initial(), &sheet);
        assert_eq!(style.color, Color::parse("red").unwrap());
    }

    #[test]
    fn display_none_is_parsed_and_applied() {
        let sheet = Stylesheet::parse(".hidden { display: none }");
        let style = cascade(
            &el_attr("div", "class", "hidden"),
            &ComputedStyle::initial(),
            &sheet,
        );
        assert_eq!(style.display, Display::None);
    }

    #[test]
    fn parses_hex_and_named_colors() {
        assert_eq!(Color::parse("#f00"), Some(Color { r: 255, g: 0, b: 0 }));
        assert_eq!(Color::parse("#00ff00"), Some(Color { r: 0, g: 255, b: 0 }));
        assert_eq!(Color::parse("blue"), Some(Color { r: 0, g: 0, b: 255 }));
        assert_eq!(Color::parse("nonsense"), None);
    }
}
