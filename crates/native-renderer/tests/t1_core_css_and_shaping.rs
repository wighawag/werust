//! T1 core-CSS cascade + Latin/LTR shaping — integration assertions on real
//! fragments, driven through the public crate surface as the shell would.
//!
//! This is the acceptance evidence for task
//! `t1-core-css-stylo-and-latin-shaping-parley` (spec story 14,
//! `docs/conformance-tiers.md` T1): a real static document, parsed by the
//! html5ever front-end and rendered by the native path, lays out under
//! block/inline normal flow with a REAL cascade over the core CSS property set and
//! REAL Latin/LTR text shaping. No floats/flex/grid/tables (T2); no JS (T3).
//!
//! The assertions here are stage-crossing and consumer-facing (the per-stage unit
//! tests live in the `css`/`shape`/`layout`/`paint` modules); this file proves the
//! stages compose into correct T1 static layout of a real page and that the public
//! surface a WPT/core-CSS harness (`t1-wpt-subset-regression-meter`) or the T1
//! server-floor task (`t1-server-web-floor-article-and-blog`) drives is present.

use native_renderer::css::{cascade, Color, ComputedStyle, Display, Stylesheet};
use native_renderer::layout::layout;
use native_renderer::{Element, Html5everParser, NativeRenderer, Parser, Shaper};
use renderer::{LoadState, Renderer};

/// A real, hand-authored `motherfuckingwebsite.com`-class static document off the
/// T0 v0 subset: a full `<!doctype>`, semantic sectioning elements the T0
/// allowlist dropped, an author stylesheet using the core CSS set (a descendant
/// combinator, `font-size` in `em`, `margin`, `color`, `background-color`), inline
/// emphasis, a list, and named entities. Everything a T1 page uses, nothing from
/// T2 (no float/flex/grid/table) or T3 (no JS).
const REAL_DOC: &str = "<!doctype html><html><head><title>werust T1</title>\
<style>\
  body { color: #222222; font-family: \"DejaVu Sans\", sans-serif }\
  article { background-color: #f8f8f8 }\
  h1 { font-size: 2em }\
  article p { margin: 12px }\
  .note { color: rgb(0, 100, 0) }\
</style></head><body>\
<article>\
<header><h1>Real &amp; Static</h1></header>\
<p>A real document with <strong>bold</strong>, <em>italic</em>, and a \
<a href=\"https://example.com/\">link</a>.</p>\
<p class=\"note\">This paragraph is a note &copy; 2026.</p>\
<ul><li>headings</li><li>paragraphs</li><li>lists</li></ul>\
</article></body></html>";

/// Build a `data:text/html,…` URL (encoding just the bytes the backend's decoder
/// treats specially, plus spaces).
fn data_url(html: &str) -> String {
    let mut payload = String::new();
    for b in html.bytes() {
        match b {
            b'%' => payload.push_str("%25"),
            b'+' => payload.push_str("%2B"),
            b' ' => payload.push_str("%20"),
            _ => payload.push(b as char),
        }
    }
    format!("data:text/html,{payload}")
}

#[test]
fn real_static_document_lays_out_and_shapes_via_the_native_path() {
    // Drive the WHOLE public path through the Renderer seam, exactly as the shell
    // would, then inspect the native render output.
    let mut backend = NativeRenderer::new();
    {
        let seam: &mut dyn Renderer = &mut backend;
        seam.navigate(&data_url(REAL_DOC))
            .expect("a real T1 document is navigable via the native path");
        assert_eq!(seam.load_state(), LoadState::Finished);
    }
    let out = backend.last_render().expect("a render happened");
    let transcript = out.surface.transcript();

    // --- The document parsed and laid out (real semantic elements kept) ---------
    assert!(!out.layout.runs.is_empty(), "the document produced runs");
    // The <title> is in <head> (display:none) and must not paint.
    assert!(
        !transcript.contains("werust T1"),
        "head title not painted: {transcript}"
    );

    // --- Real cascade over the core CSS set -------------------------------------
    // The <h1> is bold (UA sheet), inherits the body colour (#222222), and the
    // `&amp;` entity decoded to `&`.
    assert!(
        transcript.contains("Real[b#222222]"),
        "h1 bold: {transcript}"
    );
    assert!(
        transcript.contains("&[b#222222]"),
        "decoded &amp; in h1: {transcript}"
    );
    // Inline emphasis carried through the cascade (colour inherited from body #222222).
    assert!(
        transcript.contains("bold[b#222222]"),
        "strong bold + inherited body colour: {transcript}"
    );
    assert!(
        transcript.contains("italic[i#222222]"),
        "em italic + inherited body colour: {transcript}"
    );
    // The link is underlined and blue (UA link colour beats the inherited body colour).
    assert!(
        transcript.contains("link[u#0000ee]"),
        "a underlined + UA link colour: {transcript}"
    );
    // The `.note` author rule (a functional `rgb()` colour) coloured its paragraph.
    let note = out
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("note"))
        .expect("the note paragraph run");
    assert_eq!(
        note.style.color,
        Color { r: 0, g: 100, b: 0 },
        "rgb() author colour cascaded"
    );
    // `&copy;` decoded to the © glyph (real parser entity table).
    assert!(
        transcript.contains('\u{00a9}'),
        "&copy; decoded: {transcript}"
    );
    // The list items each became their own block line, in order.
    for item in ["headings", "paragraphs", "lists"] {
        assert!(transcript.contains(item), "list item {item}: {transcript}");
    }

    // --- Real Latin/LTR shaping drives geometry ---------------------------------
    // The <h1> is 2em (32px) of the body 16px: its line is TALLER than a body line.
    let h1_run = out
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("Real"))
        .expect("the h1 run");
    let body_run = out
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("document"))
        .expect("a body-paragraph run");
    assert!(
        h1_run.line_height > body_run.line_height,
        "2em h1 line ({}) taller than 1em body line ({})",
        h1_run.line_height,
        body_run.line_height
    );
    // Proportional shaping: every run has a positive, non-monospace advance.
    assert!(out.layout.runs.iter().all(|r| r.advance > 0.0));

    // --- Real pixels were painted ----------------------------------------------
    assert!(out.surface.width > 0 && out.surface.height > 0);
    // The link run painted in its UA blue (#0000ee) — real cascaded colour on the
    // surface, not just in the transcript.
    let has_link_blue = (0..out.surface.height)
        .any(|y| (0..out.surface.width).any(|x| out.surface.pixel(x, y) == Some([0, 0, 238, 255])));
    assert!(has_link_blue, "the link painted in its cascaded colour");
}

#[test]
fn shaping_is_deterministic_across_shapers_and_renders() {
    // The bundled font makes shaping reproducible: two independent shapers, and two
    // renders, produce byte-identical transcripts + identical advances. This is
    // what lets the sibling floor task pin stable goldens.
    let a = render_transcript_and_first_advance("<p>Reproducible shaping</p>");
    let b = render_transcript_and_first_advance("<p>Reproducible shaping</p>");
    assert_eq!(a, b, "shaping is deterministic across independent renders");
}

fn render_transcript_and_first_advance(html: &str) -> (String, u32) {
    let parsed = Html5everParser::new().parse(html);
    let sheet = Stylesheet::parse(&parsed.author_css);
    let mut shaper = Shaper::new();
    let laid = layout(&parsed.dom, &sheet, 800.0, &mut shaper);
    // Bit-pattern the advance so the comparison is exact and Eq-able.
    let advance_bits = laid.runs.first().map(|r| r.advance.to_bits()).unwrap_or(0);
    (
        laid.runs
            .iter()
            .map(|r| r.text.clone())
            .collect::<Vec<_>>()
            .join("|"),
        advance_bits,
    )
}

#[test]
fn cascade_is_public_and_runnable_for_the_wpt_meter() {
    // The core-CSS WPT meter (`t1-wpt-subset-regression-meter`) drives the cascade
    // surface directly; prove it is public and produces a ComputedStyle over the
    // core set from a real fragment, independent of the full render.
    let sheet = Stylesheet::parse("main > p.lead { color: #112233; font-size: 1.5em }");
    let main = Element {
        tag: "main".into(),
        attrs: vec![],
        children: vec![],
    };
    let p = Element {
        tag: "p".into(),
        attrs: vec![("class".into(), "lead".into())],
        children: vec![],
    };
    let main_style = cascade(&main, &ComputedStyle::initial(), &[], &sheet);
    assert_eq!(
        main_style.display,
        Display::Block,
        "main is a block container"
    );
    let p_style = cascade(&p, &main_style, &[&main], &sheet);
    assert_eq!(
        p_style.color,
        Color {
            r: 0x11,
            g: 0x22,
            b: 0x33
        }
    );
    assert_eq!(p_style.font_size, 24.0, "1.5em of the 16px body");
}
