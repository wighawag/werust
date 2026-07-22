//! The tree-construction subset runner (`html/syntax/parsing/`).
//!
//! Runs the html5lib-derived tree-construction tests against the native T1 parse
//! path: each `#data` fragment is parsed by [`Html5everParser`](crate::Html5everParser)
//! (the native T1 parser behind the [`Parser`](crate::Parser) seam), the resulting
//! render [`Dom`](crate::tree::Dom) serialized in the html5lib `#document` format,
//! and compared to the case's expected tree.
//!
//! # The `.dat` format
//!
//! Each `.dat` file holds one or more test cases separated by blank lines, each of
//! the form:
//!
//! ```text
//! #data
//! <the input HTML, possibly multi-line>
//! #errors
//! <expected parse errors — IGNORED here (werust asserts the tree, not the
//!  error list; error recovery is exercised by the parser task's own tests)>
//! #document
//! | <html>
//! |   <head>
//! |   <body>
//! |     <p>
//! |       "text"
//! ```
//!
//! The `#document` block is the html5lib serialization: one node per line, two
//! spaces of indentation per depth after the leading `| `, elements as `<tag>`,
//! attributes as `name="value"` lines nested one level under their element (in
//! attribute order), and text as `"…"`.
//!
//! # Normalising for werust's static render tree
//!
//! werust's [`Dom`](crate::tree::Dom) is a static, script-free render tree: it
//! drops the doctype and comments (documented in
//! [`Html5everParser`](crate::html5ever_parser)). So the EXPECTED `#document` is
//! normalised the same way before comparison — `<!DOCTYPE …>` and `<!-- … -->`
//! lines are removed — otherwise every doctype-bearing case would report a false
//! regression for a drop werust makes ON PURPOSE. This keeps the comparison honest
//! about element / text / attribute structure (the parse fidelity the bar
//! measures) without penalising the render tree's deliberate, documented shape.

use std::path::Path;

use crate::html5ever_parser::Html5everParser;
use crate::parser::Parser;
use crate::tree::{Dom, Node};

use super::{read_fixture, MeterReport};

/// The single WPT area this subset covers.
pub const AREA: &str = "html/syntax/parsing";

/// One parsed tree-construction case: the input and the expected (normalised)
/// serialized tree.
struct Case {
    /// The source `.dat` file's stem + the case's index within it (its id).
    name: String,
    /// The `#data` input HTML.
    data: String,
    /// The expected `#document` serialization, already normalised for werust's
    /// static tree (doctype + comment lines removed), trailing newline trimmed.
    expected: String,
}

/// Run the tree-construction subset in `dir` (every `*.dat` file) against the
/// native T1 parse path and report the pass-rate.
///
/// # Panics
///
/// Panics if `dir` cannot be read or contains no `.dat` files (a broken checkout,
/// not a runtime condition — the fixtures are committed).
#[must_use]
pub fn run(dir: &Path) -> MeterReport {
    let mut report = MeterReport::default();
    report.note_area(AREA);
    let parser = Html5everParser::new();

    let mut dat_files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read tree-construction dir {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "dat"))
        .collect();
    dat_files.sort();
    assert!(
        !dat_files.is_empty(),
        "no .dat tree-construction fixtures under {}",
        dir.display()
    );

    for path in &dat_files {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("tests");
        for case in parse_dat(stem, &read_fixture(path)) {
            let parsed = parser.parse(&case.data);
            let actual = serialize_document(&parsed.dom);
            if actual == case.expected {
                report.record_pass();
            } else {
                report.record_fail(
                    &case.name,
                    format!(
                        "tree mismatch for {:?}\n    expected:\n{}\n    actual:\n{}",
                        case.data,
                        indent(&case.expected),
                        indent(&actual),
                    ),
                );
            }
        }
    }
    report
}

/// Indent a multi-line serialization block for a legible failure message.
fn indent(block: &str) -> String {
    block
        .lines()
        .map(|l| format!("      {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a `.dat` file body into its cases.
///
/// The format uses `#data` / `#errors` / `#document` section markers. `#data` runs
/// until `#errors`; `#errors` (ignored) runs until `#document`; `#document` runs
/// until a blank line at column 0 that precedes the next `#data` (or EOF). We split
/// on the `#data` marker and parse each chunk's three sections.
fn parse_dat(stem: &str, body: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    // Each case starts at a line that is exactly `#data`.
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    let mut index = 0;
    while i < lines.len() {
        if lines[i] != "#data" {
            i += 1;
            continue;
        }
        i += 1;
        // #data body: up to the `#errors` marker.
        let mut data_lines = Vec::new();
        while i < lines.len() && lines[i] != "#errors" {
            data_lines.push(lines[i]);
            i += 1;
        }
        // Skip the #errors section (up to #document) — we assert the tree only.
        while i < lines.len() && lines[i] != "#document" {
            i += 1;
        }
        // #document body: the serialized-tree lines (start with `| `), up to a
        // blank line or the next `#data`.
        let mut doc_lines = Vec::new();
        if i < lines.len() && lines[i] == "#document" {
            i += 1;
            while i < lines.len() && lines[i] != "#data" && !lines[i].trim().is_empty() {
                doc_lines.push(lines[i]);
                i += 1;
            }
        }
        let data = data_lines.join("\n");
        let expected = normalise_expected(&doc_lines);
        cases.push(Case {
            name: format!("{stem}:{index}"),
            data,
            expected,
        });
        index += 1;
    }
    cases
}

/// Normalise an expected `#document` block for comparison against werust's static
/// render tree: strip the leading `| ` markers and drop the doctype + comment lines
/// (werust drops those on purpose). The html5lib depth convention already matches
/// [`serialize_document`]: the top-level `<html>` (and a doctype, when present) sits
/// at depth 0, so no re-dedent is needed once the markers are removed.
fn normalise_expected(doc_lines: &[&str]) -> String {
    let mut out = Vec::new();
    for line in doc_lines {
        // Strip the leading `| ` marker; the rest is `<indent><node>`.
        let Some(rest) = line.strip_prefix("| ") else {
            continue;
        };
        let trimmed = rest.trim_start();
        // Drop nodes a static render tree does not keep.
        if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<!--") {
            continue;
        }
        out.push(rest.to_string());
    }
    out.join("\n")
}

/// Serialize a werust [`Dom`] in the html5lib `#document` format (without the
/// leading `| ` markers — [`normalise_expected`] strips those from the expected
/// side so both are compared marker-free), roots at depth 0.
fn serialize_document(dom: &Dom) -> String {
    let mut out = String::new();
    for node in &dom.roots {
        serialize_node(node, 0, &mut out);
    }
    out.trim_end_matches('\n').to_string()
}

/// Serialize one node at `depth` (2 spaces per level), appending to `out`.
fn serialize_node(node: &Node, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    match node {
        Node::Element(e) => {
            out.push_str(&format!("{pad}<{}>\n", e.tag));
            // Attributes: one per line, one level deeper, in source order (html5lib
            // sorts them; the pinned fixtures are authored to match werust's source
            // order, which for these single-attribute cases is identical).
            for (name, value) in &e.attrs {
                out.push_str(&format!(
                    "{}{}=\"{}\"\n",
                    "  ".repeat(depth + 1),
                    name,
                    value
                ));
            }
            for child in &e.children {
                serialize_node(child, depth + 1, out);
            }
        }
        Node::Text(text) => {
            out.push_str(&format!("{pad}\"{text}\"\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_a_simple_tree_in_html5lib_shape() {
        let parsed = Html5everParser::new().parse("<p>hi</p>");
        let out = serialize_document(&parsed.dom);
        assert_eq!(
            out, "<html>\n  <head>\n  <body>\n    <p>\n      \"hi\"",
            "got:\n{out}"
        );
    }

    #[test]
    fn serializes_attributes_one_level_under_the_element() {
        let parsed = Html5everParser::new().parse("<a href=\"/x\">l</a>");
        let out = serialize_document(&parsed.dom);
        assert!(out.contains("<a>\n"), "got:\n{out}");
        assert!(out.contains("      href=\"/x\"\n"), "got:\n{out}");
    }

    #[test]
    fn normalise_strips_markers_and_drops_doctype() {
        let doc = ["| <!DOCTYPE html>", "| <html>", "|   <head>", "|   <body>"];
        let out = normalise_expected(&doc);
        assert_eq!(out, "<html>\n  <head>\n  <body>");
    }

    #[test]
    fn parse_dat_splits_multiple_cases() {
        let body = "#data\n<p>a</p>\n#errors\n#document\n| <html>\n|   <head>\n|   <body>\n|     <p>\n|       \"a\"\n\n#data\n<b>b</b>\n#errors\n#document\n| <html>\n|   <head>\n|   <body>\n|     <b>\n|       \"b\"\n";
        let cases = parse_dat("t", body);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].data, "<p>a</p>");
        assert_eq!(cases[1].data, "<b>b</b>");
        assert!(cases[0].expected.contains("<p>"));
    }
}
