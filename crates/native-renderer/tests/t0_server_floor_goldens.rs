//! T0 server-web floor — committed golden fixtures + the subset-doc-drift guard.
//!
//! This is the objective regression guard for the conformance ladder's **T0
//! server-web floor** (`docs/conformance-tiers.md` T0; user story 11 of the ship
//! spec, task `t0-server-web-floor-golden-fixtures`). There is no WPT bar at T0 —
//! a fixed private subset has no meaningful public pass-rate — so the guard is the
//! **golden-image suite + the subset-doc-drift guard**, both wired here under the
//! `verify` gate (`cargo test`).
//!
//! Two guarantees, both load-bearing:
//!
//! 1. **Golden-image guard** ([`renders_each_fixture_at_golden_parity`]): the
//!    native T0 path renders each committed `<name>.html` fragment (driven THROUGH
//!    the [`Renderer`] seam, exactly as the browser shell would) and its painted
//!    software-text transcript is asserted **byte-equal** to the committed
//!    `<name>.golden.txt` reference. Any regression in tokenize / cascade / layout
//!    / paint makes a golden mismatch, and the gate goes red.
//! 2. **Subset-doc-drift guard** ([`every_fixture_stays_within_the_v0_allowlist`]):
//!    every fixture is checked to use ONLY the documented v0 allowlist — elements on
//!    [`native_renderer::tree::ELEMENT_ALLOWLIST`], CSS properties on
//!    [`native_renderer::css::SUPPORTED_PROPERTIES`], and the T0 selector set — so a
//!    golden fixture can never quietly drift outside the subset T0 actually defines.
//!
//! The goldens are committed reference data. When an INTENDED change to the render
//! path shifts them, regenerate with the ignored helper
//! [`regenerate_goldens`] (see the fixtures `README.md`) and review the diff — a
//! golden change is a rendering change.

use std::path::{Path, PathBuf};

use native_renderer::css::{is_supported_property, is_supported_selector};
use native_renderer::tree::is_allowed;
use native_renderer::{NativeRenderer, SubsetTokenizer, Token, Tokenizer};
use renderer::{LoadState, Renderer};

/// The viewport width the goldens are pinned at, in px. Fixed so inline wrapping
/// (and therefore the transcript) is stable and reproducible across runs.
const FIXTURE_VIEWPORT_WIDTH: f32 = 800.0;

/// The committed fixture names (each has a `<name>.html` + `<name>.golden.txt`).
///
/// The set is chosen to exercise the breadth of the v0 subset: headings +
/// paragraphs + inline emphasis (`article`), ordered/unordered lists + nesting
/// (`lists`), cascade order + inline `style` + `display:none` + `<br>`
/// (`inline-styles`), and the heading scale + a `div` block + the universal
/// selector (`headings`).
const FIXTURES: &[&str] = &["article", "lists", "inline-styles", "headings"];

/// Absolute path to the fixtures directory (committed beside this test).
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t0-server-floor")
}

/// Read the authored fragment `<name>.html`.
fn read_fixture_html(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.html"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// Build a `data:text/html,…` URL for `html`, percent-encoding exactly the bytes
/// the T0 backend's decoder treats specially (`%`, `+`) plus spaces — so the
/// fixture reaches the native path through the seam byte-for-byte intact.
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

/// Render a fixture through the native T0 path, driven THROUGH the [`Renderer`]
/// seam (as the shell does), and return the painted software-text transcript.
fn render_fixture_transcript(name: &str) -> String {
    let html = read_fixture_html(name);
    let mut backend = NativeRenderer::with_viewport_width(FIXTURE_VIEWPORT_WIDTH);
    {
        let seam: &mut dyn Renderer = &mut backend;
        seam.navigate(&data_url(&html))
            .expect("the v0-subset fixture is navigable at T0");
        assert_eq!(
            seam.load_state(),
            LoadState::Finished,
            "fixture {name} finished loading"
        );
    }
    backend
        .last_render()
        .expect("a render happened")
        .surface
        .transcript()
}

/// The committed golden path for `name`.
fn golden_path(name: &str) -> PathBuf {
    fixtures_dir().join(format!("{name}.golden.txt"))
}

#[test]
fn renders_each_fixture_at_golden_parity() {
    for name in FIXTURES {
        let actual = render_fixture_transcript(name);
        let path = golden_path(name);
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {} ({e}). Regenerate with: \
                 cargo test -p native-renderer --test t0_server_floor_goldens -- \
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
fn every_fixture_stays_within_the_v0_allowlist() {
    for name in FIXTURES {
        let html = read_fixture_html(name);
        if let Err(reason) = check_subset(&html) {
            panic!("fixture {name} drifted outside the documented v0 allowlist: {reason}");
        }
    }
}

/// The subset-doc-drift guard: verify `html` uses ONLY the documented v0 allowlist
/// — elements, CSS properties, and selectors. Returns `Err(reason)` on the first
/// out-of-subset construct so a fixture can never quietly cover more than T0
/// defines.
///
/// It tokenizes with the same [`SubsetTokenizer`] the render path uses, so the
/// guard sees exactly the elements/attributes the renderer would, then checks:
/// every start-tag name is on [`is_allowed`] (plus `<style>`, see below), every
/// `<style>`-block selector is on [`is_supported_selector`], and every declared
/// property (in `<style>` blocks and inline `style="…"`) is on
/// [`is_supported_property`].
///
/// `<style>` is a deliberate exception to the element check: it is NOT on
/// [`ELEMENT_ALLOWLIST`](native_renderer::tree::ELEMENT_ALLOWLIST) because the tree
/// builder DROPS it from the box tree (it must not paint), but the T0 server-web
/// floor is explicitly "the v0 element allowlist … with `<style>` / inline
/// `style`" (`docs/conformance-tiers.md`): the pipeline reads author CSS out of
/// `<style>` before dropping the tag. So the guard admits `<style>` and instead
/// polices its CONTENTS (selectors + properties) against the subset.
fn check_subset(html: &str) -> Result<(), String> {
    let tokens = SubsetTokenizer::new().tokenize(html);

    let mut in_style = false;
    let mut style_css = String::new();
    for token in &tokens {
        match token {
            Token::StartTag { name, attrs, .. } => {
                if name == "style" {
                    in_style = true;
                } else if !is_allowed(name) {
                    return Err(format!(
                        "element <{name}> is not on the v0 element allowlist"
                    ));
                }
                // Inline `style="…"` declarations must stay within the property set.
                if let Some((_, value)) = attrs.iter().find(|(k, _)| k == "style") {
                    check_declarations(value)
                        .map_err(|p| format!("inline style on <{name}> uses property `{p}`"))?;
                }
            }
            Token::EndTag { name } => {
                if name == "style" {
                    in_style = false;
                }
            }
            Token::Text(text) if in_style => style_css.push_str(text),
            Token::Text(_) => {}
        }
    }

    check_stylesheet(&style_css)
}

/// Check every rule in a `<style>` block: its selectors and its declared
/// properties must all be on the T0 allowlist.
fn check_stylesheet(css: &str) -> Result<(), String> {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = rest[..open].trim();
        let Some(close) = rest[open + 1..].find('}') else {
            break;
        };
        let body = &rest[open + 1..open + 1 + close];
        for selector in prelude.split(',') {
            let selector = selector.trim();
            if !selector.is_empty() && !is_supported_selector(selector) {
                return Err(format!(
                    "selector `{selector}` is not in the v0 selector set (type/.class/#id/*)"
                ));
            }
        }
        check_declarations(body).map_err(|p| format!("rule `{prelude}` uses property `{p}`"))?;
        rest = &rest[open + 1 + close + 1..];
    }
    Ok(())
}

/// Check a declaration block body: every `property:` name must be on the T0
/// property allowlist. Returns `Err(property_name)` for the first that is not.
fn check_declarations(body: &str) -> Result<(), String> {
    for decl in body.split(';') {
        let Some((prop, _value)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim();
        if prop.is_empty() {
            continue;
        }
        if !is_supported_property(prop) {
            return Err(prop.to_string());
        }
    }
    Ok(())
}

#[test]
fn drift_guard_rejects_out_of_subset_constructs() {
    // The guard is only worth anything if it actually FAILS on drift. Prove it
    // rejects each way a fixture could leave the documented v0 subset.

    // An off-allowlist element.
    assert!(check_subset("<body><table><tr><td>x</td></tr></table></body>").is_err());
    // An unsupported CSS property in a <style> block.
    assert!(check_subset("<style>p { padding: 10px }</style><body><p>x</p></body>").is_err());
    // An unsupported selector (a descendant combinator) in a <style> block.
    assert!(check_subset("<style>div p { color: red }</style><body><p>x</p></body>").is_err());
    // An unsupported property in an inline style attribute.
    assert!(check_subset(r#"<body><p style="width:10px">x</p></body>"#).is_err());

    // A well-formed subset fragment passes (elements, a supported property, a
    // supported selector, and a supported inline style are all fine).
    assert!(check_subset(
        r#"<html><head><style>.lead { color: red }</style></head>
           <body><h1>Hi</h1><p class="lead" style="color:blue">x <em>y</em></p></body></html>"#
    )
    .is_ok());
}

/// Regenerate the committed goldens from the current render output.
///
/// This is NOT part of the gate (it is `#[ignore]`d): it is the maintainer helper
/// that rewrites `<name>.golden.txt` after an INTENDED render change. Run it, then
/// review the diff before committing — a golden change is a rendering change. See
/// the fixtures `README.md`.
///
/// ```sh
/// cargo test -p native-renderer --test t0_server_floor_goldens -- \
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
