//! Chrome CSS-class-set wiring shape guard (task
//! `export-the-chrome-css-class-set-from-core`).
//!
//! WHAT LANDED: the COMPLETE chrome CSS-class set is exported from `werust-core`
//! beside the rules that produce it (`CHROME_CSS_CLASS_SETS`, grouped into the
//! `TRUST_INDICATOR_CSS_CLASSES` and `ERROR_BANNER_CSS_CLASSES` families), and the
//! GTK painter ITERATES it instead of re-stating the class names in a literal
//! toggle list.
//!
//! WHY A SOURCE-SHAPE GUARD: the two teeth this task adds are real tests (the
//! core's exhaustiveness test in `crates/werust-core/src/lib.rs`, and the edge's
//! no-unstyled-class test in `crates/werust/src/main.rs`), but the first tooth
//! only BITES while the painter actually derives its toggle list from the
//! exported set: a painter that quietly went back to a literal list would keep a
//! green suite while a fifth posture left it stale — exactly the latent bug this
//! task closes. Asserting the toggle itself needs a display (`Chrome::refresh`
//! sets GTK widget classes), which the `verify` gate may not have, so this test
//! PARSES the desktop shell for that wiring, exactly as the sibling
//! `debug_view_desktop_wiring_shape.rs` and `browser_menu_edge_wiring_shape.rs`
//! do for the debug view and the menu.
//!
//! Acceptance criteria mapped to assertions below:
//! - The painter derives BOTH toggle families from the exported set rather than
//!   from hard-coded literals
//!   (`the_gtk_painter_toggles_from_the_exported_class_set_not_a_literal_list`).
//! - The layering holds: the stylesheet stays in the edge and core gains no
//!   notion of colour
//!   (`the_stylesheet_stays_in_the_edge_and_core_gains_no_styling_concept`).

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn desktop_shell() -> String {
    source("crates/werust/src/main.rs")
}

/// The slice of `text` between `start` and the following `end`, or the tail from
/// `start` when `end` does not follow it.
fn between(text: &str, start: &str, end: &str) -> String {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("the desktop shell must contain {start:?}"));
    let rest = &text[from..];
    let to = rest.find(end).unwrap_or(rest.len());
    rest[..to].to_string()
}

#[test]
fn the_gtk_painter_toggles_from_the_exported_class_set_not_a_literal_list() {
    let desktop = desktop_shell();
    // The painter IMPORTS the exported set from the core (one shared decision),
    // rather than restating the names.
    for exported in [
        "ERROR_BANNER_CSS_CLASSES",
        "TRUST_INDICATOR_CSS_CLASSES",
        "CHROME_CSS_CLASS_SETS",
    ] {
        assert!(
            desktop.contains(exported),
            "the desktop shell must consume the core's `{exported}`"
        );
    }

    // Inside `Chrome::refresh` — the paint path — both toggle loops iterate the
    // exported family, and NONE of the class names is written as a literal: a
    // literal list is the stale-class bug (a name the list omits is a class the
    // painter never clears, so a stale badge colour lingers across a transition).
    let refresh = between(
        &desktop,
        "fn refresh(&self, state: &ChromeState)",
        "/// The app stylesheet",
    );
    assert!(
        refresh.contains("for class in ERROR_BANNER_CSS_CLASSES"),
        "the error-banner toggle must iterate the exported set: {refresh:?}"
    );
    assert!(
        refresh.contains("for class in TRUST_INDICATOR_CSS_CLASSES"),
        "the trust-indicator toggle must iterate the exported set: {refresh:?}"
    );
    for class in [
        "trust-loading",
        "trust-verified",
        "trust-name-trusted-rpc",
        "trust-mutable-name",
        "trust-unverified",
        "error-banner",
        "error-banner-transient",
    ] {
        assert!(
            !refresh.contains(class),
            "`{class}` is hard-coded in the painter; it must come from the core's exported set"
        );
    }
}

#[test]
fn the_stylesheet_stays_in_the_edge_and_core_gains_no_styling_concept() {
    // The class NAME is a derivation (core); the STYLESHEET is painting (edge).
    // This task exports the SET, it does not move `APP_CSS` into the core, and it
    // gives the core no notion of colour.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("const APP_CSS: &str"),
        "the stylesheet stays in the edge that has a stylesheet"
    );
    let core = source("crates/werust-core/src/lib.rs");
    assert!(
        !core.contains("APP_CSS: &str"),
        "the core must not carry the edge's stylesheet (naming it in a doc comment is fine)"
    );
    for styling in ["color:", "background-color", "font-weight"] {
        assert!(
            !core.contains(styling),
            "the core must gain no notion of colour, but it mentions `{styling}`"
        );
    }
}
