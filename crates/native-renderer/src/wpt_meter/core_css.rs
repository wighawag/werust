//! The core-CSS subset runner (the five T1 areas).
//!
//! Runs pinned computed-value cases for `css/CSS2/normal-flow/`, `css/css-box/`,
//! `css/css-color/`, `css/css-fonts/`, and `css/css-text/` against the native
//! cascade surface ([`Stylesheet::parse`](crate::css::Stylesheet) +
//! [`cascade`](crate::css::cascade) + [`ComputedStyle`](crate::css::ComputedStyle)),
//! and checks each case's assertion. Complex-script / bidi areas are NOT part of the
//! pinned set (excluded from the T1 bar).
//!
//! # The case format
//!
//! One `#test` block per case (see `tests/fixtures/t1-wpt/core-css/cases.txt` for
//! the authoritative description and the property vocabulary):
//!
//! ```text
//! #test  <name>
//! #area  css/css-color
//! #css   p { color: #ff0000 }
//! #html  <p>x</p>
//! #path  html body p
//! #expect color = #ff0000
//! ```
//!
//! `#path` is a tag chain from a render-tree root to the element under test; append
//! `[n]` (0-based) to a step to pick the n-th matching sibling. Each `#expect` line
//! is `<property> = <value>`; a case passes only if EVERY assertion holds.

use std::path::Path;

use crate::css::{cascade, ComputedStyle, Display, Stylesheet};
use crate::html5ever_parser::Html5everParser;
use crate::parser::Parser;
use crate::tree::{Element, Node};

use super::{read_fixture, MeterReport};

/// Run the pinned core-CSS cases in `path` against the native cascade and report
/// the pass-rate.
///
/// # Panics
///
/// Panics if `path` cannot be read or parses to zero cases (a broken checkout).
#[must_use]
pub fn run(path: &Path) -> MeterReport {
    let body = read_fixture(path);
    let cases = parse_cases(&body);
    assert!(!cases.is_empty(), "no core-CSS cases in {}", path.display());

    let mut report = MeterReport::default();
    for case in &cases {
        report.note_area(&case.area);
        match evaluate(case) {
            Ok(()) => report.record_pass(),
            Err(reason) => report.record_fail(&case.name, reason),
        }
    }
    report
}

/// One parsed computed-value case.
struct Case {
    name: String,
    area: String,
    css: String,
    html: String,
    path: Vec<PathStep>,
    expects: Vec<Expect>,
}

/// A single step of a `#path`: a tag and an index among same-tag siblings.
struct PathStep {
    tag: String,
    index: usize,
}

/// One `<property> = <value>` assertion.
struct Expect {
    property: String,
    value: String,
}

/// Evaluate a case: parse its fragment + CSS, cascade down its `#path`, and check
/// every `#expect`. Returns `Ok(())` if all hold, else the first mismatch.
fn evaluate(case: &Case) -> Result<(), String> {
    let parsed = Html5everParser::new().parse(&case.html);
    let sheet = Stylesheet::parse(&case.css);

    let Some(style) = cascade_to_path(&parsed.dom.roots, &case.path, &sheet) else {
        return Err(format!(
            "path {:?} not found in the parsed tree",
            path_debug(&case.path)
        ));
    };

    for expect in &case.expects {
        check(&style, &expect.property, &expect.value)?;
    }
    Ok(())
}

/// Walk `#path` from the roots, running the cascade top-down (each element's style
/// from its parent, with the ancestor path for combinator matching — mirroring how
/// [`layout`](crate::layout::layout) descends), and return the target's style.
fn cascade_to_path(roots: &[Node], path: &[PathStep], sheet: &Stylesheet) -> Option<ComputedStyle> {
    let mut siblings = roots;
    let mut parent_style = ComputedStyle::initial();
    let mut ancestors: Vec<&Element> = Vec::new();
    let mut result = None;

    for step in path {
        let element = nth_element(siblings, &step.tag, step.index)?;
        let style = cascade(element, &parent_style, &ancestors, sheet);
        ancestors.insert(0, element); // nearest-first, as `cascade` expects.
        parent_style = style.clone();
        siblings = &element.children;
        result = Some(style);
    }
    result
}

/// Find the `index`-th child element with tag `tag` among `siblings`.
fn nth_element<'a>(siblings: &'a [Node], tag: &str, index: usize) -> Option<&'a Element> {
    siblings
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) if e.tag == tag => Some(e),
            _ => None,
        })
        .nth(index)
}

/// Check one assertion against a computed style. Returns `Ok(())` on match, else a
/// legible expected-vs-actual reason.
fn check(style: &ComputedStyle, property: &str, expected: &str) -> Result<(), String> {
    let actual = actual_value(style, property)
        .ok_or_else(|| format!("unknown meter property `{property}`"))?;
    if values_equal(property, &actual, expected) {
        Ok(())
    } else {
        Err(format!("{property}: expected `{expected}`, got `{actual}`"))
    }
}

/// Render the native cascade's value for `property` as the case vocabulary's string
/// form, or `None` if the property name is not in the vocabulary.
fn actual_value(style: &ComputedStyle, property: &str) -> Option<String> {
    let hex = |c: crate::css::Color| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    Some(match property {
        "color" => hex(style.color),
        "background-color" => match style.background_color {
            Some(c) => hex(c),
            None => "none".to_string(),
        },
        "display" => match style.display {
            Display::Block => "block",
            Display::Inline => "inline",
            Display::None => "none",
        }
        .to_string(),
        "font-size" => fmt_px(style.font_size),
        // The USED line-height in px against this element's own font-size (a unitless
        // multiplier resolves here); `normal` has no cascade px, reported as `normal`.
        "line-height" => match style.line_height.resolve(style.font_size) {
            Some(px) => fmt_px(px),
            None => "normal".to_string(),
        },
        "font-weight" => bool_word(style.bold, "bold", "normal"),
        "font-style" => bool_word(style.italic, "italic", "normal"),
        "text-decoration" => bool_word(style.underline, "underline", "none"),
        "margin-top" => fmt_px(style.margin.top),
        "margin-right" => fmt_px(style.margin.right),
        "margin-bottom" => fmt_px(style.margin.bottom),
        "margin-left" => fmt_px(style.margin.left),
        "padding-top" => fmt_px(style.padding.top),
        "padding-right" => fmt_px(style.padding.right),
        "padding-bottom" => fmt_px(style.padding.bottom),
        "padding-left" => fmt_px(style.padding.left),
        "font-family" => {
            if style.font_family.is_empty() {
                "default".to_string()
            } else {
                style.font_family.join(",")
            }
        }
        _ => return None,
    })
}

/// Compare an actual value to an expected one. Numeric (px) properties compare with
/// a small tolerance so shaped/rounded metrics do not cause spurious failures;
/// everything else is an exact string match (colours are already canonical hex).
fn values_equal(property: &str, actual: &str, expected: &str) -> bool {
    const NUMERIC: &[&str] = &[
        "font-size",
        "line-height",
        "margin-top",
        "margin-right",
        "margin-bottom",
        "margin-left",
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
    ];
    if NUMERIC.contains(&property) {
        match (actual.parse::<f32>(), expected.parse::<f32>()) {
            (Ok(a), Ok(e)) => (a - e).abs() < 0.01,
            _ => actual == expected,
        }
    } else {
        actual.eq_ignore_ascii_case(expected)
    }
}

/// Format a px value without a trailing `.0` for whole numbers (so `24.0` prints
/// `24`, matching the case files).
fn fmt_px(v: f32) -> String {
    if (v.fract()).abs() < 1e-6 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn bool_word(flag: bool, yes: &str, no: &str) -> String {
    if flag { yes } else { no }.to_string()
}

fn path_debug(path: &[PathStep]) -> Vec<String> {
    path.iter()
        .map(|s| format!("{}[{}]", s.tag, s.index))
        .collect()
}

/// Parse the `cases.txt` body into cases. Lines outside a `#test` block (blank
/// lines, `#`-prefixed comments that are not section markers) are ignored.
fn parse_cases(body: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut current: Option<Case> = None;

    for raw in body.lines() {
        let line = raw.trim_end();
        if let Some((marker, rest)) = split_marker(line) {
            let rest = rest.trim();
            match marker {
                "test" => {
                    if let Some(case) = current.take() {
                        cases.push(case);
                    }
                    current = Some(Case {
                        name: rest.to_string(),
                        area: String::new(),
                        css: String::new(),
                        html: String::new(),
                        path: Vec::new(),
                        expects: Vec::new(),
                    });
                }
                "area" => {
                    if let Some(c) = current.as_mut() {
                        c.area = rest.to_string();
                    }
                }
                "css" => {
                    if let Some(c) = current.as_mut() {
                        c.css = rest.to_string();
                    }
                }
                "html" => {
                    if let Some(c) = current.as_mut() {
                        c.html = rest.to_string();
                    }
                }
                "path" => {
                    if let Some(c) = current.as_mut() {
                        c.path = parse_path(rest);
                    }
                }
                "expect" => {
                    if let Some(c) = current.as_mut() {
                        if let Some(e) = parse_expect(rest) {
                            c.expects.push(e);
                        }
                    }
                }
                _ => {}
            }
        }
        // Any other line (comment / blank) is ignored: cases carry their content on
        // the marker lines themselves.
    }
    if let Some(case) = current.take() {
        cases.push(case);
    }
    // Drop malformed cases (no path or no assertions) so a typo cannot silently
    // pad the denominator.
    cases.retain(|c| !c.path.is_empty() && !c.expects.is_empty() && !c.area.is_empty());
    cases
}

/// Split a line into `(marker, rest)` if it starts with a known `#<marker>` token.
fn split_marker(line: &str) -> Option<(&str, &str)> {
    const MARKERS: &[&str] = &["test", "area", "css", "html", "path", "expect"];
    let rest = line.strip_prefix('#')?;
    for marker in MARKERS {
        if rest == *marker {
            return Some((marker, ""));
        }
        if let Some(after) = rest.strip_prefix(marker) {
            if after.starts_with(char::is_whitespace) {
                return Some((marker, after));
            }
        }
    }
    None
}

/// Parse a `#path` value (`html body p[1]`) into steps.
fn parse_path(text: &str) -> Vec<PathStep> {
    text.split_whitespace()
        .map(|token| {
            if let Some((tag, idx)) = token.split_once('[') {
                let index = idx.trim_end_matches(']').parse().unwrap_or(0);
                PathStep {
                    tag: tag.to_ascii_lowercase(),
                    index,
                }
            } else {
                PathStep {
                    tag: token.to_ascii_lowercase(),
                    index: 0,
                }
            }
        })
        .collect()
}

/// Parse a `#expect` value (`color = #ff0000`) into an assertion.
fn parse_expect(text: &str) -> Option<Expect> {
    let (property, value) = text.split_once('=')?;
    Some(Expect {
        property: property.trim().to_ascii_lowercase(),
        value: value.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_case() {
        let body = "#test t\n#area css/css-color\n#css p { color: #ff0000 }\n#html <p>x</p>\n#path html body p\n#expect color = #ff0000\n";
        let cases = parse_cases(body);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].area, "css/css-color");
        assert_eq!(cases[0].path.len(), 3);
        assert_eq!(cases[0].expects[0].property, "color");
    }

    #[test]
    fn drops_a_case_missing_its_path_or_assertion() {
        let body = "#test broken\n#area css/css-box\n#css p {}\n#html <p>x</p>\n";
        assert!(parse_cases(body).is_empty());
    }

    #[test]
    fn evaluates_a_color_case_against_the_native_cascade() {
        let case = Case {
            name: "c".into(),
            area: "css/css-color".into(),
            css: "p { color: #ff0000 }".into(),
            html: "<p>x</p>".into(),
            path: parse_path("html body p"),
            expects: vec![Expect {
                property: "color".into(),
                value: "#ff0000".into(),
            }],
        };
        assert!(evaluate(&case).is_ok());
    }

    #[test]
    fn a_wrong_expectation_fails_with_a_legible_reason() {
        let case = Case {
            name: "c".into(),
            area: "css/css-color".into(),
            css: "p { color: #ff0000 }".into(),
            html: "<p>x</p>".into(),
            path: parse_path("html body p"),
            expects: vec![Expect {
                property: "color".into(),
                value: "#00ff00".into(),
            }],
        };
        let err = evaluate(&case).unwrap_err();
        assert!(err.contains("expected `#00ff00`"), "reason: {err}");
    }

    #[test]
    fn font_size_em_resolves_against_the_parent() {
        let case = Case {
            name: "c".into(),
            area: "css/css-fonts".into(),
            css: "body { font-size: 20px } h1 { font-size: 2em }".into(),
            html: "<h1>x</h1>".into(),
            path: parse_path("html body h1"),
            expects: vec![Expect {
                property: "font-size".into(),
                value: "40".into(),
            }],
        };
        assert!(evaluate(&case).is_ok(), "{:?}", evaluate(&case));
    }
}
