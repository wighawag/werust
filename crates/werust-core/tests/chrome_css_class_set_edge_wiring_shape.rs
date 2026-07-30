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
//!
//! EXTENDED by task `one-derivation-close-the-aggregate-and-tooltip-gaps`, which
//! closed the level ABOVE: each family was exhaustive over its CLASSES, but each
//! painter's coverage GATE hand-wrote which FAMILIES it checked, so a sixth
//! family would join neither gate and paint invisibly on both desktops with both
//! suites green. Both gates now iterate the core's `CssClassFamily::ALL`
//! (`both_coverage_gates_iterate_the_family_aggregate_not_a_hand_written_list`),
//! and the URL-bar progress SENTENCE is composed once in the core rather than
//! twice at the edges (`the_progress_tooltip_sentence_lives_only_in_the_core`).
//! Both are source-shape guards for the same reason as the toggle guard above:
//! the macOS half of the wiring cannot be exercised on this gate, and a painter
//! that drifted back to a literal list would keep a green suite.

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
    for exported in ["ERROR_BANNER_CSS_CLASSES", "TRUST_INDICATOR_CSS_CLASSES"] {
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

/// The NATIVE-WIDGET painter's host-independent half, which the Ubuntu gate DOES
/// compile (its palette gate lives there).
///
/// It landed as `crates/werust-macos/src/paint.rs` and was EXTRACTED to its own
/// crate by task `windows-win32-window-and-chrome`, so the Win32 window consumes
/// the one carrier instead of copying it. ONE path here now covers both native
/// desktop windows, which is the point of the extraction.
fn shared_paint() -> String {
    source("crates/desktop-paint/src/lib.rs")
}

#[test]
fn both_coverage_gates_iterate_the_family_aggregate_not_a_hand_written_list() {
    // Acceptance (task `one-derivation-close-the-aggregate-and-tooltip-gaps`):
    // each painter's no-unstyled-class gate must be driven by the CORE's
    // aggregate over every exported family, so a SIXTH family reds both gates
    // instead of joining neither. A gate that names its families itself is
    // exhaustive over the classes of the families it happens to know, which is
    // exactly the hole this task closes.
    for (edge, gate_fn, source_text) in [
        (
            "crates/werust/src/main.rs",
            "fn every_chrome_css_class_the_core_exports_has_a_rule_in_the_app_css()",
            desktop_shell(),
        ),
        (
            "crates/desktop-paint/src/lib.rs",
            "fn every_exported_class_has_a_colour()",
            shared_paint(),
        ),
    ] {
        let gate = between(&source_text, gate_fn, "\n    }\n");
        let code: String = gate
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            code.contains("for family in CssClassFamily::ALL") && code.contains("family.classes()"),
            "{edge}'s coverage gate must iterate the core's family aggregate: {code:?}"
        );
        for hand_written in [
            "TRUST_INDICATOR_CSS_CLASSES",
            "ERROR_BANNER_CSS_CLASSES",
            "DEBUG_CONSOLE_CSS_CLASSES",
            "CHROME_CSS_CLASS_SETS",
        ] {
            assert!(
                !code.contains(hand_written),
                "{edge}'s coverage gate names `{hand_written}` itself; WHICH families are checked \
                 must come from the core's aggregate"
            );
        }
    }
}

#[test]
fn the_progress_tooltip_sentence_lives_only_in_the_core() {
    // Acceptance (task `one-derivation-close-the-aggregate-and-tooltip-gaps`): the
    // URL bar's progress tooltip is a pure function of `ChromeState`, so it is
    // composed ONCE beside the other `load_progress_*` rules and both desktop
    // painters CALL it. It had been written out verbatim in both edges — two
    // copies of one sentence, which is exactly how the Kotlin and Swift twins
    // started to drift.
    let core = source("crates/werust-core/src/lib.rs");
    assert!(
        core.contains("pub fn load_progress_tooltip(state: &ChromeState, stop_label: &str)"),
        "the sentence is a core rule, beside the other `load_progress_*` rules"
    );
    for (edge, text) in [
        ("crates/werust/src/main.rs", desktop_shell()),
        ("crates/desktop-paint/src/lib.rs", shared_paint()),
    ] {
        assert!(
            text.contains("load_progress_tooltip(state, STOP_AFFORDANCE_LABEL)"),
            "{edge} must call the core's tooltip rule with the label its Stop control carries"
        );
        assert!(
            !text.contains("to cancel"),
            "{edge} still composes the progress sentence itself; it belongs to the core"
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
