//! The T1 core-CSS cascade, built on the mature Rust CSS stack.
//!
//! At **T1** (`docs/conformance-tiers.md`, `CONTEXT.md`; task
//! `t1-core-css-stylo-and-latin-shaping-parley`) the cascade grows from the T0
//! handful of properties to the **core CSS set** a hand-written or lightly
//! templated static page uses: box-model (`margin`/`padding` + their per-side
//! longhands), colour (`color`, `background-color`), typography (`font-size`,
//! `font-weight`, `font-style`, `font-family`, `line-height`, `text-decoration`),
//! and the normal-flow `display`. It stays a REAL cascade — UA sheet, then author
//! rules by (specificity, source order), then inline `style="…"`, with inheritance
//! of inherited properties — now over the wider set. No floats/flex/grid/tables
//! (that is T2); no JS (that is T3).
//!
//! # Standing on the stylo stack's parser
//!
//! Stylesheets and values are tokenised with [`cssparser`], the exact CSS
//! tokenizer/parser Servo's stylo is built on (its colour parsing too:
//! [`cssparser::color`]), rather than the T0 hand-rolled string splitting — so real
//! stylesheets (comments, quoted strings, functional `rgb()`, `!important`,
//! whitespace quirks) parse robustly. Selector MATCHING is a focused matcher over
//! the T1 core selector set (type / `.class` / `#id` / `*`, descendant and child
//! combinators, grouping) with correct CSS specificity, rather than the full
//! `selectors` crate `Element` trait: werust's [`Dom`](crate::tree::Dom) is a plain
//! owned static tree with no parent/sibling pointers, and the full `selectors`
//! matcher wants a navigable, interior-mutable DOM (the object-graph friction the
//! thesis parks at T1). The rationale + the rejected `Stylist`/`selectors::Element`
//! alternative are recorded in
//! `docs/spikes/t1-core-css-stylo-and-latin-shaping-parley/README.md` (decision D1).
//!
//! # The T0 subset allowlist still lives here
//!
//! [`SUPPORTED_PROPERTIES`] / [`is_supported_property`] / [`is_supported_selector`]
//! are UNCHANGED: they define the fixed **T0** subset the server-floor drift guard
//! (`tests/t0_server_floor_goldens.rs`) checks its committed fixtures against. They
//! are deliberately narrower than what the T1 cascade now accepts — a T0 fixture
//! must stay inside the documented v0 subset even though the engine can render more.

use cssparser::color::{parse_hash_color, parse_named_color};
use cssparser::{ParseError, Parser, ParserInput, Token};

use crate::tree::Element;

/// The fixed **T0** CSS property allowlist (`docs/conformance-tiers.md` T0).
///
/// These name the T0 subset the server-floor drift guard checks committed fixtures
/// against; they are intentionally narrower than the T1 cascade below. See the
/// module docs.
pub const SUPPORTED_PROPERTIES: &[&str] = &[
    "display",
    "color",
    "font-weight",
    "font-style",
    "text-decoration",
    "text-decoration-line",
    "margin-bottom",
];

/// Whether `name` is on the **T0** CSS property allowlist (case-insensitively).
#[must_use]
pub fn is_supported_property(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    SUPPORTED_PROPERTIES.contains(&name.as_str())
}

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

    /// Opaque white.
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
    };

    /// Parse a CSS colour with the stylo stack's colour parser: `#rgb` / `#rrggbb`,
    /// the full CSS named-colour table, and functional `rgb()` / `rgba()`. Returns
    /// `None` for a value outside that (alpha is flattened to opaque — the T1 paint
    /// surface is opaque).
    #[must_use]
    pub fn parse(value: &str) -> Option<Color> {
        let v = value.trim();
        if let Some(hex) = v.strip_prefix('#') {
            let (r, g, b, _a) = parse_hash_color(hex.as_bytes()).ok()?;
            return Some(Color { r, g, b });
        }
        if let Some(c) = parse_rgb_function(v) {
            return Some(c);
        }
        let (r, g, b) = parse_named_color(&v.to_ascii_lowercase()).ok()?;
        Some(Color { r, g, b })
    }
}

/// Parse an `rgb(…)` / `rgba(…)` colour with cssparser's tokenizer. Returns `None`
/// if `value` is not such a function or its arguments are malformed. Alpha is
/// accepted and dropped (the paint surface is opaque).
fn parse_rgb_function(value: &str) -> Option<Color> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let name = match parser.next().ok()? {
        Token::Function(name) => name.clone(),
        _ => return None,
    };
    if !name.eq_ignore_ascii_case("rgb") && !name.eq_ignore_ascii_case("rgba") {
        return None;
    }
    let channels: Result<Vec<u8>, ParseError<()>> = parser.parse_nested_block(|p| {
        let mut out = Vec::new();
        while out.len() < 4 {
            match p.next() {
                Ok(Token::Number { value, .. }) => out.push(value.clamp(0.0, 255.0) as u8),
                Ok(Token::Percentage { unit_value, .. }) => {
                    out.push((unit_value.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
                Ok(Token::Comma) => {}
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Ok(out)
    });
    let ch = channels.ok()?;
    if ch.len() >= 3 {
        Some(Color {
            r: ch[0],
            g: ch[1],
            b: ch[2],
        })
    } else {
        None
    }
}

/// The `display` outer type the T1 cascade resolves.
///
/// Only the normal-flow outer types are modelled: `block`, `inline`,
/// `inline-block` (treated as inline for flow at T1), and `none`. Floats / flex /
/// grid / table displays are T2 and fall back to `block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// A block-level box (participates in block flow).
    Block,
    /// An inline-level box (participates in inline flow).
    Inline,
    /// Not rendered (and generates no box).
    None,
}

/// Box edge offsets in px (`margin` / `padding`), resolved per side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    /// Top edge in px.
    pub top: f32,
    /// Right edge in px.
    pub right: f32,
    /// Bottom edge in px.
    pub bottom: f32,
    /// Left edge in px.
    pub left: f32,
}

/// The initial `font-size` in px (the CSS default medium, 16px).
pub const INITIAL_FONT_SIZE: f32 = 16.0;

/// A computed `line-height`, carried through inheritance in the form CSS mandates.
///
/// The three forms inherit differently, which is the whole reason this is an enum
/// rather than a resolved `f32` (see the bug fixed in
/// `work/notes/observations/t1-unitless-line-height-inherits-as-absolute-px.md`):
///
/// - `Normal` — the `normal` keyword; the used px is font-size-relative per element.
/// - `Absolute(px)` — a unit-bearing value (`24px`, `1.5em`): a FIXED px, inherited
///   as that same px so a differently-sized child does NOT rescale it.
/// - `Multiplier(n)` — a UNITLESS number (`1.5`): inherited as the multiplier itself,
///   so each descendant recomputes `n * its own font-size` at use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// The `normal` keyword: a font-size-relative used value resolved at shaping.
    Normal,
    /// A fixed px value (from a unit-bearing `line-height`), inherited unchanged.
    Absolute(f32),
    /// A unitless multiplier, resolved against each element's own font-size at use.
    Multiplier(f32),
}

impl LineHeight {
    /// The used line-height in px against `font_size`, or `None` for `Normal` (whose
    /// used value is a shaper-side font-size-relative default, not a cascade px).
    #[must_use]
    pub fn resolve(self, font_size: f32) -> Option<f32> {
        match self {
            LineHeight::Normal => None,
            LineHeight::Absolute(px) => Some(px),
            LineHeight::Multiplier(n) => Some(n * font_size),
        }
    }
}

/// The fully-resolved style of one element after the T1 cascade.
///
/// Layout + shaping + paint read exactly these fields. Inherited properties
/// (`color`, `font-*`, `line-height`, `text-decoration`) flow parent -> child in
/// [`cascade`]; non-inherited ones (`display`, `margin`, `padding`,
/// `background-color`) reset to their initial value on each element.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// Outer display type.
    pub display: Display,
    /// Text colour (inherited).
    pub color: Color,
    /// Background colour, if any (not inherited).
    pub background_color: Option<Color>,
    /// Whether text is bold (inherited).
    pub bold: bool,
    /// Whether text is italic (inherited).
    pub italic: bool,
    /// Whether text is underlined (inherited).
    pub underline: bool,
    /// Font size in px (inherited); the basis for `em` lengths.
    pub font_size: f32,
    /// The resolved font-family list (inherited); an empty list means the default.
    pub font_family: Vec<String>,
    /// Line height (inherited). A unitless value inherits as a `Multiplier` and is
    /// re-resolved per element's own font-size; a unit-bearing value inherits as a
    /// fixed `Absolute` px; unset stays `Normal`.
    pub line_height: LineHeight,
    /// Margin box edges in px (not inherited).
    pub margin: Edges,
    /// Padding box edges in px (not inherited).
    pub padding: Edges,
}

impl ComputedStyle {
    /// The initial (root) style: an inherited baseline before any UA/author rule.
    #[must_use]
    pub fn initial() -> Self {
        ComputedStyle {
            display: Display::Inline,
            color: Color::BLACK,
            background_color: None,
            bold: false,
            italic: false,
            underline: false,
            font_size: INITIAL_FONT_SIZE,
            font_family: Vec::new(),
            line_height: LineHeight::Normal,
            margin: Edges::default(),
            padding: Edges::default(),
        }
    }

    /// The bottom margin in px — the block-flow separation layout uses. Kept as a
    /// convenience accessor so layout/paint read one field where T0 had a scalar.
    #[must_use]
    pub fn margin_bottom(&self) -> f32 {
        self.margin.bottom
    }
}

/// One parsed CSS declaration in the T1 core property set.
#[derive(Debug, Clone, PartialEq)]
enum Declaration {
    Display(Display),
    Color(Color),
    BackgroundColor(Color),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    /// A font size as a length, resolved against the parent size at cascade time.
    FontSize(Length),
    FontFamily(Vec<String>),
    /// A line height in its declared form (see [`LineHeightDecl`]): `normal`, a
    /// unit-bearing `<length>` resolved to a fixed px against the element's own
    /// font-size at apply time, or a UNITLESS multiplier carried unresolved so each
    /// descendant recomputes it against its own font-size.
    LineHeight(LineHeightDecl),
    Margin([Option<Length>; 4]),
    Padding([Option<Length>; 4]),
}

/// A declared `line-height` before font-size is known.
///
/// A unit-bearing `<length>` still needs the element's own font-size to resolve an
/// `em` to a fixed px, so it is carried as a [`Length`] and resolved at apply time
/// into [`LineHeight::Absolute`]. A UNITLESS number becomes [`LineHeight::Multiplier`]
/// directly — it must NOT be collapsed to px, so it inherits as the multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LineHeightDecl {
    /// The `normal` keyword.
    Normal,
    /// A unit-bearing `<length>` (`24px`, `1.5em`), resolved to a fixed px at apply.
    Length(Length),
    /// A unitless number, kept as the multiplier.
    Multiplier(f32),
}

/// A CSS length the cascade resolves to px: absolute `px` or font-relative `em`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Length {
    Px(f32),
    Em(f32),
}

impl Length {
    /// Resolve to px against a font size basis (for `em`).
    fn resolve(self, font_basis: f32) -> f32 {
        match self {
            Length::Px(v) => v,
            Length::Em(v) => v * font_basis,
        }
    }
}

/// A compound selector: an optional type name plus any `.class` / `#id` filters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Compound {
    tag: Option<String>,
    classes: Vec<String>,
    ids: Vec<String>,
}

impl Compound {
    fn matches(&self, element: &Element) -> bool {
        if let Some(tag) = &self.tag {
            if element.tag != *tag {
                return false;
            }
        }
        for id in &self.ids {
            if element.attr("id") != Some(id.as_str()) {
                return false;
            }
        }
        for class in &self.classes {
            let has = element
                .attr("class")
                .map(|c| c.split_whitespace().any(|w| w == class))
                .unwrap_or(false);
            if !has {
                return false;
            }
        }
        true
    }

    /// Specificity contribution: (#ids, #classes, #types).
    fn specificity(&self) -> (u32, u32, u32) {
        (
            self.ids.len() as u32,
            self.classes.len() as u32,
            u32::from(self.tag.is_some()),
        )
    }
}

/// A combinator between two compound selectors in a complex selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    /// ` ` — a descendant (matches any ancestor).
    Descendant,
    /// `>` — a child (matches the immediate parent).
    Child,
}

/// A complex selector: a key compound plus a chain of ancestor compounds joined by
/// combinators, matched right-to-left against the element and its ancestor path.
#[derive(Debug, Clone, PartialEq)]
struct Selector {
    /// The rightmost (key) compound — matched against the element itself.
    key: Compound,
    /// Ancestor compounds, nearest-first, each with the combinator to its right.
    ancestors: Vec<(Combinator, Compound)>,
}

impl Selector {
    /// Whether this selector matches `element` given its `ancestors` (nearest-first:
    /// `ancestors[0]` is the element's parent, `ancestors[1]` its grandparent, …).
    fn matches(&self, element: &Element, ancestors: &[&Element]) -> bool {
        if !self.key.matches(element) {
            return false;
        }
        let mut path = ancestors.iter();
        for (combinator, compound) in &self.ancestors {
            match combinator {
                Combinator::Child => {
                    let Some(parent) = path.next() else {
                        return false;
                    };
                    if !compound.matches(parent) {
                        return false;
                    }
                }
                Combinator::Descendant => loop {
                    let Some(candidate) = path.next() else {
                        return false;
                    };
                    if compound.matches(candidate) {
                        break;
                    }
                },
            }
        }
        true
    }

    /// The CSS specificity as a single comparable key (a,b,c packed).
    fn specificity(&self) -> u32 {
        let (mut a, mut b, mut c) = self.key.specificity();
        for (_, compound) in &self.ancestors {
            let (ea, eb, ec) = compound.specificity();
            a += ea;
            b += eb;
            c += ec;
        }
        (a << 16) | (b << 8) | c
    }
}

/// One author rule: a selector plus its declarations, tagged with source order.
#[derive(Debug, Clone)]
struct Rule {
    selector: Selector,
    declarations: Vec<Declaration>,
    order: usize,
}

/// A parsed T1 stylesheet: the author rules from all `<style>` blocks.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

impl Stylesheet {
    /// Parse author CSS text (the concatenated `<style>` block contents) into a T1
    /// stylesheet using cssparser for value tokenising. Unsupported selectors and
    /// properties are skipped; the parse never fails.
    #[must_use]
    pub fn parse(css: &str) -> Self {
        let mut rules = Vec::new();
        let mut order = 0;
        for (prelude, body) in split_rules(css) {
            let declarations = parse_declarations(&body);
            if declarations.is_empty() {
                continue;
            }
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
        }
        Stylesheet { rules }
    }
}

/// Split a stylesheet into `(prelude, body)` pairs at top-level `{ … }`, skipping
/// at-rules (`@media` etc. are T2/beyond). A brace-depth scan keeps nested blocks
/// (e.g. inside `@media`) from being mistaken for rules.
fn split_rules(css: &str) -> Vec<(String, String)> {
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut prelude_start = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let prelude = css[prelude_start..i].trim().to_string();
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let body = css[i + 1..j.saturating_sub(1)].to_string();
                if !prelude.starts_with('@') {
                    rules.push((prelude, body));
                }
                i = j;
                prelude_start = i;
            }
            b'}' => {
                i += 1;
                prelude_start = i;
            }
            _ => i += 1,
        }
    }
    rules
}

/// The **T0** subset selector check (unchanged): a single type / `.class` / `#id`
/// or `*`. Used ONLY by the T0 server-floor drift guard, not the T1 matcher.
#[must_use]
pub fn is_supported_selector(text: &str) -> bool {
    let text = text.trim();
    if text == "*" {
        return true;
    }
    let ident = text
        .strip_prefix('.')
        .or_else(|| text.strip_prefix('#'))
        .unwrap_or(text);
    !ident.is_empty()
        && ident
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse a complex T1 selector: compounds joined by descendant (` `) or child
/// (`>`) combinators. Returns `None` if any compound is malformed or an
/// unsupported construct (pseudo-classes, attribute selectors, sibling
/// combinators) appears — those are skipped rather than mis-matched.
fn parse_selector(text: &str) -> Option<Selector> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.contains(':') || text.contains('[') || text.contains('~') || text.contains('+') {
        return None;
    }
    // Normalise `>` into a spaced token so splitting on whitespace sees it.
    let spaced = text.replace('>', " > ");
    // Parse into an ordered list of compounds and the combinator PRECEDING each
    // (the combinator that links a compound to the one on its LEFT).
    let mut compounds: Vec<(Combinator, Compound)> = Vec::new();
    let mut pending = Combinator::Descendant;
    for token in spaced.split_whitespace() {
        if token == ">" {
            pending = Combinator::Child;
            continue;
        }
        let compound = parse_compound(token)?;
        compounds.push((pending, compound));
        pending = Combinator::Descendant;
    }
    // The rightmost compound is the key (its own leading combinator is the one
    // linking it to its nearest ancestor). Pop it, then walk leftwards: each
    // ancestor entry pairs the compound with the combinator that linked it to the
    // compound on its RIGHT (i.e. the leading combinator of the entry to its right).
    let (key_combinator, key) = compounds.pop()?;
    let mut ancestors: Vec<(Combinator, Compound)> = Vec::new();
    let mut right_combinator = key_combinator;
    while let Some((leading, compound)) = compounds.pop() {
        ancestors.push((right_combinator, compound));
        right_combinator = leading;
    }
    Some(Selector { key, ancestors })
}

/// Parse one compound selector (`p.lead#main` / `.note` / `#id` / `*` / `div`).
fn parse_compound(text: &str) -> Option<Compound> {
    if text == "*" {
        return Some(Compound::default());
    }
    let mut compound = Compound::default();
    let mut chars = text.chars().peekable();
    if matches!(chars.peek(), Some(c) if is_ident_char(*c)) {
        let ident = take_ident(&mut chars);
        if ident.is_empty() {
            return None;
        }
        compound.tag = Some(ident.to_ascii_lowercase());
    }
    while let Some(&c) = chars.peek() {
        match c {
            '.' => {
                chars.next();
                let ident = take_ident(&mut chars);
                if ident.is_empty() {
                    return None;
                }
                compound.classes.push(ident);
            }
            '#' => {
                chars.next();
                let ident = take_ident(&mut chars);
                if ident.is_empty() {
                    return None;
                }
                compound.ids.push(ident);
            }
            _ => return None,
        }
    }
    Some(compound)
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn take_ident(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident_char(c) {
            out.push(c);
            chars.next();
        } else {
            break;
        }
    }
    out
}

/// Parse a declaration block body (`prop: value; …`) into the T1 property set.
fn parse_declarations(body: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for decl in body.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        let value = strip_important(value.trim());
        for parsed in parse_declaration(prop.trim(), value) {
            declarations.push(parsed);
        }
    }
    declarations
}

/// Strip a trailing `!important` (its priority is honoured implicitly: an
/// important author declaration still cascades in source order at T1, enough for
/// the core set — full origin/priority interplay is beyond T1 scope).
fn strip_important(value: &str) -> &str {
    let lower = value.to_ascii_lowercase();
    if lower.ends_with("!important") {
        value[..value.len() - "!important".len()].trim()
    } else {
        value
    }
}

/// Parse one `property: value` pair into zero or more [`Declaration`]s (a
/// shorthand like `margin` expands to the four sides).
fn parse_declaration(prop: &str, value: &str) -> Vec<Declaration> {
    let v = value.trim();
    let vl = v.to_ascii_lowercase();
    match prop.to_ascii_lowercase().as_str() {
        "display" => match vl.as_str() {
            "block" => vec![Declaration::Display(Display::Block)],
            "inline" | "inline-block" => vec![Declaration::Display(Display::Inline)],
            "none" => vec![Declaration::Display(Display::None)],
            _ => vec![Declaration::Display(Display::Block)],
        },
        "color" => Color::parse(v)
            .map(Declaration::Color)
            .into_iter()
            .collect(),
        "background-color" | "background" => Color::parse(v)
            .map(Declaration::BackgroundColor)
            .into_iter()
            .collect(),
        "font-weight" => match vl.as_str() {
            "bold" | "bolder" | "600" | "700" | "800" | "900" => vec![Declaration::Bold(true)],
            "normal" | "lighter" | "100" | "200" | "300" | "400" | "500" => {
                vec![Declaration::Bold(false)]
            }
            _ => vec![],
        },
        "font-style" => match vl.as_str() {
            "italic" | "oblique" => vec![Declaration::Italic(true)],
            "normal" => vec![Declaration::Italic(false)],
            _ => vec![],
        },
        "text-decoration" | "text-decoration-line" => {
            if vl.split_whitespace().any(|w| w == "underline") {
                vec![Declaration::Underline(true)]
            } else if vl.split_whitespace().any(|w| w == "none") {
                vec![Declaration::Underline(false)]
            } else {
                vec![]
            }
        }
        "font-size" => parse_font_size(&vl)
            .map(Declaration::FontSize)
            .into_iter()
            .collect(),
        "font-family" => {
            let families = parse_font_family(v);
            if families.is_empty() {
                vec![]
            } else {
                vec![Declaration::FontFamily(families)]
            }
        }
        "line-height" => {
            if vl == "normal" {
                vec![Declaration::LineHeight(LineHeightDecl::Normal)]
            } else if let Ok(number) = vl.parse::<f32>() {
                // A UNITLESS line-height is the MULTIPLIER (checked before
                // `parse_length`, which would treat a bare number as px). It is kept
                // unresolved so it inherits as the multiplier, not a fixed px.
                vec![Declaration::LineHeight(LineHeightDecl::Multiplier(number))]
            } else if let Some(len) = parse_length(&vl) {
                vec![Declaration::LineHeight(LineHeightDecl::Length(len))]
            } else {
                vec![]
            }
        }
        "margin" => parse_box_shorthand(&vl)
            .map(Declaration::Margin)
            .into_iter()
            .collect(),
        "padding" => parse_box_shorthand(&vl)
            .map(Declaration::Padding)
            .into_iter()
            .collect(),
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => parse_length(&vl)
            .map(|len| edge_longhand(prop, len, true))
            .into_iter()
            .collect(),
        "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => parse_length(&vl)
            .map(|len| edge_longhand(prop, len, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}

/// Build a single-side margin/padding declaration for a `*-top/right/bottom/left`
/// longhand (the other three sides stay `None` = untouched).
fn edge_longhand(prop: &str, len: Length, is_margin: bool) -> Declaration {
    let mut sides: [Option<Length>; 4] = [None; 4];
    let idx = match prop.rsplit('-').next().unwrap_or("") {
        "top" => 0,
        "right" => 1,
        "bottom" => 2,
        "left" => 3,
        _ => 0,
    };
    sides[idx] = Some(len);
    if is_margin {
        Declaration::Margin(sides)
    } else {
        Declaration::Padding(sides)
    }
}

/// Parse a `margin`/`padding` shorthand (1–4 CSS lengths) into per-side values.
fn parse_box_shorthand(value: &str) -> Option<[Option<Length>; 4]> {
    let parts: Vec<Length> = value.split_whitespace().filter_map(parse_length).collect();
    let (t, r, b, l) = match parts.as_slice() {
        [a] => (*a, *a, *a, *a),
        [v, h] => (*v, *h, *v, *h),
        [t, h, b] => (*t, *h, *b, *h),
        [t, r, b, l] => (*t, *r, *b, *l),
        _ => return None,
    };
    Some([Some(t), Some(r), Some(b), Some(l)])
}

/// Parse a `font-size` value: a length, a percentage, or an absolute keyword.
fn parse_font_size(value: &str) -> Option<Length> {
    match value {
        "medium" => Some(Length::Px(16.0)),
        "small" => Some(Length::Px(13.0)),
        "large" => Some(Length::Px(18.0)),
        "x-small" => Some(Length::Px(10.0)),
        "x-large" => Some(Length::Px(24.0)),
        "xx-large" => Some(Length::Px(32.0)),
        "smaller" => Some(Length::Em(0.83)),
        "larger" => Some(Length::Em(1.2)),
        _ => {
            if let Some(pct) = value.strip_suffix('%') {
                pct.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|p| Length::Em(p / 100.0))
            } else {
                parse_length(value)
            }
        }
    }
}

/// Parse a `font-family` list into ordered family names (quotes stripped).
fn parse_font_family(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|name| name.trim().trim_matches(['"', '\'']).to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Parse a CSS length: `<n>px`, `<n>em`, `<n>rem`, or a bare number (px). Returns
/// `None` for unsupported units.
fn parse_length(value: &str) -> Option<Length> {
    let v = value.trim();
    if let Some(px) = v.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(Length::Px);
    }
    if let Some(em) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")) {
        return em.trim().parse::<f32>().ok().map(Length::Em);
    }
    v.parse::<f32>().ok().map(Length::Px)
}

/// The user-agent default declarations for an element by tag name.
///
/// A small UA stylesheet covering the common semantic elements a real static page
/// uses: block-level flow for structural + sectioning elements, heading scale +
/// margins, list/paragraph margins, and inline emphasis defaults. Elements not
/// listed default to `display: inline` (the initial value).
fn ua_declarations(tag: &str) -> Vec<Declaration> {
    use Declaration::*;
    let block = || Display(self::Display::Block);
    let margin_bottom = |px: f32| Margin([None, None, Some(Length::Px(px)), None]);
    match tag {
        "head" | "title" | "meta" | "link" | "script" | "style" => {
            vec![Display(self::Display::None)]
        }
        "html" | "body" | "div" | "article" | "section" | "header" | "footer" | "nav" | "main"
        | "aside" | "figure" | "figcaption" | "address" | "hgroup" => vec![block()],
        "p" => vec![block(), margin_bottom(16.0)],
        "ul" | "ol" => vec![block(), margin_bottom(16.0)],
        "li" => vec![block()],
        "blockquote" => vec![block(), margin_bottom(16.0)],
        "pre" => vec![block(), margin_bottom(16.0)],
        "hr" => vec![block(), margin_bottom(8.0)],
        "h1" => vec![
            block(),
            Bold(true),
            FontSize(Length::Px(32.0)),
            margin_bottom(21.0),
        ],
        "h2" => vec![
            block(),
            Bold(true),
            FontSize(Length::Px(24.0)),
            margin_bottom(19.0),
        ],
        "h3" => vec![
            block(),
            Bold(true),
            FontSize(Length::Px(19.0)),
            margin_bottom(16.0),
        ],
        "h4" | "h5" | "h6" => vec![block(), Bold(true), margin_bottom(16.0)],
        "strong" | "b" => vec![Bold(true)],
        "em" | "i" => vec![Italic(true)],
        "a" => vec![Underline(true), Color(self::Color { r: 0, g: 0, b: 238 })],
        _ => vec![],
    }
}

/// Apply a list of declarations onto a style, in order (later wins). `parent_size`
/// is the parent font size (the basis for `em` `font-size`); other `em` lengths
/// resolve against the element's own (already-applied) `font_size`.
fn apply(style: &mut ComputedStyle, declarations: &[Declaration], parent_size: f32) {
    for decl in declarations {
        match decl {
            Declaration::Display(d) => style.display = *d,
            Declaration::Color(c) => style.color = *c,
            Declaration::BackgroundColor(c) => style.background_color = Some(*c),
            Declaration::Bold(b) => style.bold = *b,
            Declaration::Italic(i) => style.italic = *i,
            Declaration::Underline(u) => style.underline = *u,
            Declaration::FontSize(len) => {
                style.font_size = len.resolve(parent_size);
            }
            Declaration::FontFamily(families) => style.font_family = families.clone(),
            Declaration::LineHeight(decl) => {
                style.line_height = match decl {
                    // `normal` and a unit-bearing `<length>` resolve to their
                    // inheriting form now; a UNITLESS number stays the multiplier so
                    // each descendant recomputes it against its own font-size.
                    LineHeightDecl::Normal => LineHeight::Normal,
                    LineHeightDecl::Length(l) => LineHeight::Absolute(l.resolve(style.font_size)),
                    LineHeightDecl::Multiplier(n) => LineHeight::Multiplier(*n),
                };
            }
            Declaration::Margin(sides) => apply_edges(&mut style.margin, sides, style.font_size),
            Declaration::Padding(sides) => apply_edges(&mut style.padding, sides, style.font_size),
        }
    }
}

/// Apply per-side length overrides onto an [`Edges`], leaving `None` sides intact.
fn apply_edges(edges: &mut Edges, sides: &[Option<Length>; 4], font_size: f32) {
    if let Some(t) = sides[0] {
        edges.top = t.resolve(font_size);
    }
    if let Some(r) = sides[1] {
        edges.right = r.resolve(font_size);
    }
    if let Some(b) = sides[2] {
        edges.bottom = b.resolve(font_size);
    }
    if let Some(l) = sides[3] {
        edges.left = l.resolve(font_size);
    }
}

/// Run the T1 cascade for one element, given its parent's computed style and its
/// ancestor path (nearest-first) for combinator matching.
#[must_use]
pub fn cascade(
    element: &Element,
    parent: &ComputedStyle,
    ancestors: &[&Element],
    sheet: &Stylesheet,
) -> ComputedStyle {
    let parent_size = parent.font_size;
    let mut style = ComputedStyle {
        display: Display::Inline,
        color: parent.color,
        background_color: None,
        bold: parent.bold,
        italic: parent.italic,
        underline: parent.underline,
        font_size: parent.font_size,
        font_family: parent.font_family.clone(),
        line_height: parent.line_height,
        margin: Edges::default(),
        padding: Edges::default(),
    };

    apply(&mut style, &ua_declarations(&element.tag), parent_size);

    let mut matched: Vec<&Rule> = sheet
        .rules
        .iter()
        .filter(|r| r.selector.matches(element, ancestors))
        .collect();
    matched.sort_by_key(|r| (r.selector.specificity(), r.order));
    for rule in matched {
        apply(&mut style, &rule.declarations, parent_size);
    }

    if let Some(inline) = element.attr("style") {
        apply(&mut style, &parse_declarations(inline), parent_size);
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

    fn root(element: &Element, sheet: &Stylesheet) -> ComputedStyle {
        cascade(element, &ComputedStyle::initial(), &[], sheet)
    }

    #[test]
    fn ua_sheet_makes_p_a_block_with_margin() {
        let style = root(&el("p"), &Stylesheet::default());
        assert_eq!(style.display, Display::Block);
        assert!(style.margin_bottom() > 0.0);
    }

    #[test]
    fn ua_sheet_gives_headings_a_larger_font_size() {
        let sheet = Stylesheet::default();
        let h1 = root(&el("h1"), &sheet);
        let p = root(&el("p"), &sheet);
        assert!(h1.bold);
        assert!(h1.font_size > p.font_size, "h1 larger than body text");
    }

    #[test]
    fn ua_sheet_keeps_semantic_containers_in_block_flow() {
        let sheet = Stylesheet::default();
        for tag in ["article", "section", "header", "footer", "nav", "main"] {
            assert_eq!(
                root(&el(tag), &sheet).display,
                Display::Block,
                "{tag} block"
            );
        }
    }

    #[test]
    fn inherited_color_and_font_flow_to_children() {
        let mut parent = ComputedStyle::initial();
        parent.color = Color {
            r: 10,
            g: 20,
            b: 30,
        };
        parent.font_size = 20.0;
        let child = cascade(&el("span"), &parent, &[], &Stylesheet::default());
        assert_eq!(
            child.color,
            Color {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert_eq!(child.font_size, 20.0, "font-size inherited");
    }

    #[test]
    fn non_inherited_margin_and_background_do_not_flow() {
        let mut parent = ComputedStyle::initial();
        parent.margin.bottom = 40.0;
        parent.background_color = Some(Color::WHITE);
        let child = cascade(&el("span"), &parent, &[], &Stylesheet::default());
        assert_eq!(child.margin.bottom, 0.0);
        assert_eq!(child.background_color, None);
    }

    #[test]
    fn author_rule_overrides_ua_and_specificity_wins() {
        let sheet =
            Stylesheet::parse("p { color: red } .special { color: green } #x { color: blue }");
        assert_eq!(root(&el("p"), &sheet).color, Color::parse("red").unwrap());
        assert_eq!(
            root(&el_attr("p", "class", "special"), &sheet).color,
            Color::parse("green").unwrap()
        );
        assert_eq!(
            root(&el_attr("p", "id", "x"), &sheet).color,
            Color::parse("blue").unwrap()
        );
    }

    #[test]
    fn descendant_and_child_combinators_match_via_the_ancestor_path() {
        let sheet = Stylesheet::parse("article p { color: green } article > h1 { color: blue }");
        let article = el("article");
        let div = el("div");
        let p = cascade(
            &el("p"),
            &ComputedStyle::initial(),
            &[&div, &article],
            &sheet,
        );
        assert_eq!(p.color, Color::parse("green").unwrap());
        let h1 = cascade(&el("h1"), &ComputedStyle::initial(), &[&article], &sheet);
        assert_eq!(h1.color, Color::parse("blue").unwrap());
        let h1_deep = cascade(
            &el("h1"),
            &ComputedStyle::initial(),
            &[&div, &article],
            &sheet,
        );
        assert_ne!(h1_deep.color, Color::parse("blue").unwrap());
    }

    #[test]
    fn inline_style_beats_author_rules() {
        let sheet = Stylesheet::parse("#x { color: green }");
        let mut element = el_attr("p", "id", "x");
        element.attrs.push(("style".into(), "color: red".into()));
        assert_eq!(root(&element, &sheet).color, Color::parse("red").unwrap());
    }

    #[test]
    fn display_none_is_parsed_and_applied() {
        let sheet = Stylesheet::parse(".hidden { display: none }");
        assert_eq!(
            root(&el_attr("div", "class", "hidden"), &sheet).display,
            Display::None
        );
    }

    #[test]
    fn box_model_shorthand_and_longhands_resolve_per_side() {
        let sheet = Stylesheet::parse(
            "div { margin: 10px 20px 30px 40px; padding: 5px } .p { padding-left: 8px }",
        );
        let div = root(&el("div"), &sheet);
        assert_eq!(
            (
                div.margin.top,
                div.margin.right,
                div.margin.bottom,
                div.margin.left
            ),
            (10.0, 20.0, 30.0, 40.0)
        );
        assert_eq!(div.padding.top, 5.0);
        let p = root(&el_attr("div", "class", "p"), &sheet);
        assert_eq!(p.padding.left, 8.0, "longhand overrides one side");
    }

    #[test]
    fn font_size_em_is_relative_to_the_parent() {
        let sheet = Stylesheet::parse(".big { font-size: 2em }");
        let mut parent = ComputedStyle::initial();
        parent.font_size = 20.0;
        let child = cascade(&el_attr("span", "class", "big"), &parent, &[], &sheet);
        assert_eq!(child.font_size, 40.0, "2em of a 20px parent = 40px");
    }

    #[test]
    fn line_height_honours_px_unitless_and_normal() {
        // A unit-bearing value is a FIXED px; a unitless value is the MULTIPLIER;
        // unset stays `normal`.
        let px = root(
            &el_attr("p", "style", "line-height: 24px"),
            &Stylesheet::default(),
        );
        assert_eq!(px.line_height, LineHeight::Absolute(24.0));
        let unitless = root(
            &el_attr("p", "style", "font-size: 20px; line-height: 1.5"),
            &Stylesheet::default(),
        );
        assert_eq!(unitless.line_height, LineHeight::Multiplier(1.5));
        assert_eq!(
            unitless.line_height.resolve(unitless.font_size),
            Some(30.0),
            "1.5 * its own 20px = 30px"
        );
        let normal = root(&el("p"), &Stylesheet::default());
        assert_eq!(normal.line_height, LineHeight::Normal);
    }

    #[test]
    fn unitless_line_height_inherits_as_a_multiplier_not_a_fixed_px() {
        // The orphaned-cascade defect: `body { font-size: 20px; line-height: 1.5 }`
        // sets the multiplier on the body, but a child at a DIFFERENT font-size must
        // recompute `1.5 * its own font-size`, not inherit the body's resolved 30px.
        let sheet = Stylesheet::parse(
            "body { font-size: 20px; line-height: 1.5 } small { font-size: 10px }",
        );
        let body = root(&el("body"), &sheet);
        assert_eq!(body.line_height, LineHeight::Multiplier(1.5));
        assert_eq!(body.line_height.resolve(body.font_size), Some(30.0));
        // The child inherits the MULTIPLIER (not the absolute 30px) and re-resolves.
        let small = cascade(&el("small"), &body, &[&el("body")], &sheet);
        assert_eq!(small.font_size, 10.0);
        assert_eq!(small.line_height, LineHeight::Multiplier(1.5));
        assert_eq!(
            small.line_height.resolve(small.font_size),
            Some(15.0),
            "1.5 * child's own 10px = 15px, NOT the body's 30px"
        );
    }

    #[test]
    fn unit_bearing_line_height_inherits_as_a_fixed_px_across_font_sizes() {
        // A unit-bearing `line-height` is a fixed px and does NOT rescale per child:
        // `24px` set on the body stays 24px on a differently-sized child.
        let sheet = Stylesheet::parse(
            "body { font-size: 20px; line-height: 24px } small { font-size: 10px }",
        );
        let body = root(&el("body"), &sheet);
        assert_eq!(body.line_height, LineHeight::Absolute(24.0));
        let small = cascade(&el("small"), &body, &[&el("body")], &sheet);
        assert_eq!(small.font_size, 10.0);
        assert_eq!(
            small.line_height,
            LineHeight::Absolute(24.0),
            "a fixed px line-height is not rescaled by the child's font-size"
        );
        assert_eq!(small.line_height.resolve(small.font_size), Some(24.0));
    }

    #[test]
    fn parses_hex_named_and_rgb_colors_via_cssparser() {
        assert_eq!(Color::parse("#f00"), Some(Color { r: 255, g: 0, b: 0 }));
        assert_eq!(Color::parse("#00ff00"), Some(Color { r: 0, g: 255, b: 0 }));
        assert_eq!(Color::parse("blue"), Some(Color { r: 0, g: 0, b: 255 }));
        // The full css-color-4 keyword table cssparser ships, not the T0 handful.
        assert_eq!(
            Color::parse("rebeccapurple"),
            Some(Color {
                r: 102,
                g: 51,
                b: 153
            })
        );
        assert_eq!(
            Color::parse("rgb(10, 20, 30)"),
            Some(Color {
                r: 10,
                g: 20,
                b: 30
            })
        );
        assert_eq!(
            Color::parse("rgba(255, 0, 0, 0.5)"),
            Some(Color { r: 255, g: 0, b: 0 }),
            "alpha dropped to opaque"
        );
        assert_eq!(Color::parse("nonsense"), None);
    }

    #[test]
    fn background_color_is_cascaded() {
        let sheet = Stylesheet::parse("body { background-color: #eef }");
        assert_eq!(
            root(&el("body"), &sheet).background_color,
            Some(Color {
                r: 238,
                g: 238,
                b: 255
            })
        );
    }

    #[test]
    fn important_is_stripped_and_the_declaration_still_applies() {
        let sheet = Stylesheet::parse("p { color: red !important }");
        assert_eq!(root(&el("p"), &sheet).color, Color::parse("red").unwrap());
    }

    #[test]
    fn robust_parse_survives_comments_and_at_rules() {
        // cssparser-grade robustness: a comment and an @media block do not derail
        // the following real rule.
        let sheet = Stylesheet::parse(
            "/* a comment */ @media screen { p { color: blue } } p { color: green }",
        );
        // The top-level `p` wins (the @media block is skipped at T1).
        assert_eq!(root(&el("p"), &sheet).color, Color::parse("green").unwrap());
    }

    #[test]
    fn font_family_list_is_parsed_and_inherited() {
        let sheet = Stylesheet::parse(r#"body { font-family: "Some Serif", serif }"#);
        let body = root(&el("body"), &sheet);
        assert_eq!(
            body.font_family,
            vec!["Some Serif".to_string(), "serif".to_string()]
        );
        let child = cascade(&el("p"), &body, &[], &sheet);
        assert_eq!(child.font_family, body.font_family, "font-family inherited");
    }

    #[test]
    fn t0_subset_helpers_are_unchanged() {
        // The T0 drift-guard surface stays narrow even though the T1 cascade is wide.
        assert!(is_supported_property("color"));
        assert!(!is_supported_property("padding"));
        assert!(is_supported_selector("p"));
        assert!(!is_supported_selector("div p"));
        assert!(!is_supported_selector("a:hover"));
    }
}
