//! T1 server-web floor — two committed real-page golden fixtures.
//!
//! This is the objective regression guard for the conformance ladder's **T1
//! server-web floor** (`docs/conformance-tiers.md` T1; user story 15 of the ship
//! spec, task `t1-server-web-floor-article-and-blog`). Where the T0 floor pins
//! authored *subset* fragments, T1 pins two INDEPENDENTLY-authored REAL static
//! pages so the tier is not tuned to one exemplar:
//!
//! 1. `article` — a `motherfuckingwebsite.com`-class minimal semantic-HTML
//!    article/doc page (a full `<!doctype>`, `<header>`, headings, paragraphs, a
//!    list, links, inline emphasis, and a core-CSS stylesheet).
//! 2. `blog-post` — a second, independently-authored static-site-generator (Hugo-
//!    class) blog post (post metadata, a `<blockquote>`, an ordered list, a nested
//!    site header) — a different author's structure, so the tier is proven on two.
//!
//! Both render through the **native T1 path** (html5ever parse behind the
//! `Parser` seam + the core-CSS cascade + parley Latin/LTR shaping), driven THROUGH
//! the [`Renderer`] seam exactly as the browser shell would. Each fixture's painted
//! software-text transcript — flow order + style marks (`[b]`/`[i]`/`[u]`) + a
//! non-black `#rrggbb` colour mark, so a colour-cascade regression turns it red —
//! is asserted **byte-equal** to its committed `<name>.golden.txt`. Any regression
//! in parse / cascade / shaping / layout / paint makes a golden mismatch and the
//! `verify` gate (`cargo test`) goes red.
//!
//! The fixtures are captured local snapshots (see `SOURCE.md`): the tests are
//! isolated from the live network — no fetch happens, the pages are rendered from
//! the committed bytes through a `data:text/html,…` URL. Shaping is reproducible
//! because it is pinned to the crate's one bundled font (`assets/DejaVuSans.ttf`);
//! the goldens are stable ONLY against that font.
//!
//! When an INTENDED render change shifts the goldens, regenerate with the ignored
//! helper [`regenerate_goldens`] (see the fixtures `README.md`) and review the diff
//! — a golden change is a rendering change.

use std::path::{Path, PathBuf};

use native_renderer::css::Color;
use native_renderer::{NativeRenderer, RenderOutput};
use renderer::{LoadState, Renderer};

/// The viewport width the goldens are pinned at, in px. Fixed so inline wrapping
/// (and therefore the transcript) is stable and reproducible across runs.
const FIXTURE_VIEWPORT_WIDTH: f32 = 800.0;

/// The committed fixture names (each has a `<name>.html` + `<name>.golden.txt`).
///
/// Two INDEPENDENTLY-authored real pages, per the T1 checklist: a minimal
/// semantic-HTML article/doc page and a static-site-generator blog post.
const FIXTURES: &[&str] = &["article", "blog-post"];

/// Absolute path to the fixtures directory (committed beside this test).
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t1-server-floor")
}

/// Read the captured page snapshot `<name>.html`.
fn read_fixture_html(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.html"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// Build a `data:text/html,…` URL for `html`, percent-encoding exactly the bytes
/// the backend's decoder treats specially (`%`, `+`) plus spaces — so the captured
/// snapshot reaches the native path through the seam byte-for-byte intact and NO
/// network fetch is involved.
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

/// Render a fixture through the native T1 path, driven THROUGH the [`Renderer`]
/// seam (as the shell does), returning the whole [`RenderOutput`] for inspection.
fn render_fixture(name: &str) -> RenderOutput {
    let html = read_fixture_html(name);
    let mut backend = NativeRenderer::with_viewport_width(FIXTURE_VIEWPORT_WIDTH);
    {
        let seam: &mut dyn Renderer = &mut backend;
        seam.navigate(&data_url(&html))
            .expect("the captured real page is navigable via the native T1 path");
        assert_eq!(
            seam.load_state(),
            LoadState::Finished,
            "fixture {name} finished loading"
        );
    }
    backend.last_render().expect("a render happened").clone()
}

/// The painted software-text transcript for a fixture (the golden reference form).
fn render_fixture_transcript(name: &str) -> String {
    render_fixture(name).surface.transcript()
}

/// The committed golden path for `name`.
fn golden_path(name: &str) -> PathBuf {
    fixtures_dir().join(format!("{name}.golden.txt"))
}

#[test]
fn renders_each_real_page_at_golden_parity() {
    for name in FIXTURES {
        let actual = render_fixture_transcript(name);
        let path = golden_path(name);
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {} ({e}). Regenerate with: \
                 cargo test -p native-renderer --test t1_server_floor_goldens -- \
                 --ignored regenerate_goldens",
                path.display()
            )
        });
        assert_eq!(
            actual,
            expected.trim_end_matches('\n'),
            "fixture {name} drifted from its committed golden ({}). If this render \
             change is intended, regenerate the goldens and review the diff.",
            path.display()
        );
    }
}

#[test]
fn each_page_renders_via_the_native_t1_path_with_shaped_text() {
    // Beyond byte-equality: prove each page actually went through the T1 native
    // path — real shaped runs with positive proportional advances, real font line
    // heights, and cascaded colour on the surface (not just in the transcript).
    for name in FIXTURES {
        let out = render_fixture(name);
        assert!(
            !out.layout.runs.is_empty(),
            "{name}: the page produced runs"
        );
        // Real Latin/LTR shaping: every run has a positive, proportional advance
        // and a real (positive) line height from the bundled font's metrics.
        assert!(
            out.layout.runs.iter().all(|r| r.advance > 0.0),
            "{name}: every run has a positive shaped advance"
        );
        assert!(
            out.layout.runs.iter().all(|r| r.line_height > 0.0),
            "{name}: every run has a real font line height"
        );
        // The <h1> line is larger than a body line — real per-font-size metrics.
        let max_line = out
            .layout
            .runs
            .iter()
            .map(|r| r.line_height)
            .fold(0.0_f32, f32::max);
        let min_line = out
            .layout
            .runs
            .iter()
            .map(|r| r.line_height)
            .fold(f32::MAX, f32::min);
        assert!(
            max_line > min_line,
            "{name}: heading/body lines differ in height (real shaping metrics)"
        );
        // Real pixels were painted at a positive size.
        assert!(
            out.surface.width > 0 && out.surface.height > 0,
            "{name}: painted a sized surface"
        );
    }
}

#[test]
fn head_title_is_not_painted() {
    // Each fixture has a <title> in <head> (display:none in the UA sheet); it must
    // never appear in the painted transcript — proof the real tree was cascaded.
    let article = render_fixture_transcript("article");
    assert!(
        !article.contains("Motherfucking"),
        "article <title> not painted: {article}"
    );
    let blog = render_fixture_transcript("blog-post");
    assert!(
        !blog.contains("werust log &middot;") && !blog.contains("Shipping a Rust renderer &"),
        "blog <title> not painted: {blog}"
    );
}

#[test]
fn colour_cascade_reaches_the_surface() {
    // A colour-cascade regression must turn a golden red, so prove real cascaded
    // colours are painted (author rules over the core-CSS colour property set).
    // The article's `.tip` note is green (#008000).
    let article = render_fixture("article");
    let tip = article
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("shaped"))
        .expect("the article .tip run");
    assert_eq!(
        tip.style.color,
        Color { r: 0, g: 128, b: 0 },
        "the .tip author colour cascaded onto its run"
    );
    let tip_green = (0..article.surface.height).any(|y| {
        (0..article.surface.width).any(|x| article.surface.pixel(x, y) == Some([0, 128, 0, 255]))
    });
    assert!(tip_green, "the .tip note painted in its cascaded green");

    // The blog post's <h1> is #111111 (author rule over the UA sheet).
    let blog = render_fixture("blog-post");
    let heading = blog
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("Shipping"))
        .expect("the blog h1 run");
    assert_eq!(
        heading.style.color,
        Color {
            r: 0x11,
            g: 0x11,
            b: 0x11
        },
        "the blog h1 author colour cascaded"
    );
    assert!(heading.style.bold, "the blog h1 is bold (UA sheet)");
}

#[test]
fn fixtures_stay_within_the_t1_static_scope() {
    // T1 is real static documents: NO floats/flex/grid/tables (T2) and NO
    // JavaScript (T3). Guard that the pinned pages never quietly drift into a
    // higher tier's constructs — a fixture that did would make this floor claim
    // more than T1 defines.
    for name in FIXTURES {
        let html = read_fixture_html(name).to_ascii_lowercase();
        // No T2 layout constructs (tables/floats/flex/grid).
        for banned in [
            "<table",
            "<script",
            "float:",
            "display:flex",
            "display: flex",
            "display:grid",
            "display: grid",
            "display:table",
            "display: table",
        ] {
            assert!(
                !html.contains(banned),
                "fixture {name} uses out-of-T1-scope construct `{banned}`"
            );
        }
    }
}

/// Regenerate the committed goldens from the current render output.
///
/// This is NOT part of the gate (it is `#[ignore]`d): it is the maintainer helper
/// that rewrites `<name>.golden.txt` after an INTENDED render change. Run it, then
/// review the diff before committing — a golden change is a rendering change. See
/// the fixtures `README.md`.
///
/// ```sh
/// cargo test -p native-renderer --test t1_server_floor_goldens -- \
///     --ignored regenerate_goldens
/// ```
#[test]
#[ignore = "maintainer helper: rewrites committed goldens; run explicitly"]
fn regenerate_goldens() {
    for name in FIXTURES {
        let transcript = render_fixture_transcript(name);
        let path = golden_path(name);
        std::fs::write(&path, format!("{transcript}\n"))
            .unwrap_or_else(|e| panic!("write golden {}: {e}", path.display()));
        eprintln!("wrote {}", path.display());
    }
}
