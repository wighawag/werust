//! Collapsed reload/stop control + loading spinner wiring shape guard (task
//! `reload-stop-collapse-and-loading-spinner-core-and-gtk`, spec
//! `chrome-conventional-controls`).
//!
//! WHAT LANDED: werust's separate Reload and Stop buttons are ONE control, and a
//! loading spinner joins the URL bar's progress fraction. Both are DERIVED in the
//! toolkit-free core (`reload_stop_control`, `load_spinner_visible`, pure
//! functions of the same `ChromeState::is_loading` fact the progress bar reads)
//! and exported through BOTH carriers — the plain-Rust `desktop-paint` snapshot
//! the AppKit and Win32 painters read, and `werust_core::chrome_json`, which the
//! Kotlin and Swift edges decode — so the four sibling edge tasks read a value
//! where they would otherwise mint a conditional. The GTK desktop edge is the
//! painter that proves it end to end.
//!
//! WHY A SOURCE-SHAPE GUARD: the derivation itself is unit-tested where it lives
//! (display-free, over every `ChromeState` shape) and both carriers are asserted
//! to agree with it verbatim. What none of that can see is whether the EDGE
//! actually reads them: a painter that kept `stop.set_sensitive(is_loading())`
//! beside the new value, or grew its own `if loading` for the spinner, would
//! compile and pass every unit test while being exactly the per-edge twin
//! `CONTEXT.md`'s one-derivation rule exists to delete. The GTK window itself
//! needs a display, which this repo's `verify` gate does not have, so its widget
//! assertions are `#[ignore]`d in `crates/werust/src/main.rs` and the SHAPE is
//! pinned here, in the same spirit as the sibling guards
//! `shortcut_edge_wiring_shape.rs` and `mobile_chrome_presentation_shape.rs`.
//!
//! Acceptance criteria mapped to assertions below:
//! - The control mode and the spinner are derived in the core from the loading
//!   fact, and BOTH carriers export them
//!   (`the_control_mode_and_the_spinner_are_derived_once_and_carried_by_both_carriers`).
//! - The GTK painter shows ONE control, reading only the derived values, and the
//!   old enable-one-of-a-pair rule is gone
//!   (`the_gtk_painter_shows_one_control_and_derives_neither_it_nor_the_spinner`).
//! - Back and forward are untouched (desktop keeps them, per the spec)
//!   (`the_gtk_back_and_forward_buttons_are_untouched`).
//! - Cancelling an in-flight load survives the collapse, from the toolbar AND
//!   from the keyboard
//!   (`cancel_survives_the_collapse_on_the_toolbar_and_on_the_keyboard`).

use std::path::{Path, PathBuf};

use renderer::LoadState;
use werust_core::shortcuts::{self, Chord, ChromeAction, Focus, Key, Modifiers, PrimaryModifier};
use werust_core::{
    chrome_json, load_progress_visible, load_spinner_visible, reload_stop_control, ChromeState,
    LoadStep, ReloadStopControl,
};

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

/// The PRODUCTION half of the desktop shell: everything before its test module,
/// so an assertion about what the edge does cannot be satisfied (or tripped) by
/// test code. The same device the sibling shortcut guard uses.
fn desktop_production(shell: &str) -> &str {
    let tests = shell
        .find("#[cfg(test)]")
        .expect("the desktop shell must have a test module");
    &shell[..tests]
}

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice.
fn between<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
    let start = source
        .find(from)
        .unwrap_or_else(|| panic!("the source must contain `{from}`"));
    let end = source[start..]
        .find(to)
        .unwrap_or_else(|| panic!("the source must contain `{to}` after `{from}`"));
    &source[start..start + end]
}

#[test]
fn the_control_mode_and_the_spinner_are_derived_once_and_carried_by_both_carriers() {
    // One derivation, two carriers, chosen per edge by what can cross the
    // boundary (`CONTEXT.md`, "chrome presentation / painter"). A carrier that
    // skipped these facts is an edge that cannot follow — and a mobile edge
    // running its own `when`/`switch` is the twin this repo already deleted.
    let loading = ChromeState {
        load_state: LoadState::Started,
        load_step: LoadStep::FetchingContent,
        ..ChromeState::default()
    };
    let settled = ChromeState {
        load_state: LoadState::Finished,
        ..ChromeState::default()
    };
    assert_eq!(reload_stop_control(&loading), ReloadStopControl::Stop);
    assert_eq!(reload_stop_control(&settled), ReloadStopControl::Reload);
    assert!(load_spinner_visible(&loading) && !load_spinner_visible(&settled));

    // Carrier 1, the JSON the Kotlin and Swift edges decode each refresh.
    for state in [&loading, &settled] {
        let doc: serde_json::Value =
            serde_json::from_str(&chrome_json(state)).expect("the chrome JSON is valid JSON");
        for field in [
            "reloadStopControl",
            "reloadStopControlLabel",
            "reloadStopControlDescription",
            "loadSpinnerVisible",
        ] {
            assert!(
                !doc[field].is_null(),
                "the chrome JSON must carry `{field}`, or neither mobile edge can follow"
            );
        }
        assert_eq!(
            doc["reloadStopControl"],
            serde_json::json!(reload_stop_control(state).wire_name())
        );
        assert_eq!(
            doc["loadSpinnerVisible"],
            serde_json::json!(load_spinner_visible(state))
        );
    }

    // Carrier 2, the plain-Rust snapshot the AppKit and Win32 painters read. Its
    // agreement with the core is asserted field-by-field in that crate's own
    // tests; what this guard adds is that the fields EXIST there at all, since a
    // sibling task builds against them.
    let paint = source("crates/desktop-paint/src/lib.rs");
    for field in [
        "pub reload_stop_control: ReloadStopControl",
        "pub reload_stop_label: &'static str",
        "pub reload_stop_description: &'static str",
        "pub spinner_visible: bool",
    ] {
        assert!(
            paint.contains(field),
            "the desktop paint snapshot must carry `{field}`"
        );
    }
    assert!(
        paint.contains("reload_stop_control(state)")
            && paint.contains("load_spinner_visible(state)"),
        "the snapshot must CALL the core's rules; it is a carrier, not a second derivation"
    );
}

#[test]
fn the_gtk_painter_shows_one_control_and_derives_neither_it_nor_the_spinner() {
    // The GTK edge is the painter that proves the collapse end to end: ONE
    // toolbar control whose mode is the core's, plus the spinner, and NOTHING
    // deciding either here.
    let shell = desktop_shell();
    let production = desktop_production(&shell);

    // The toolbar builds one control where it built two.
    assert!(
        production.contains("toolbar.append(&reload_stop);")
            && production.contains("toolbar.append(&spinner);"),
        "the toolbar must carry the ONE reload/stop control and the spinner"
    );
    for gone in ["toolbar.append(&reload);", "toolbar.append(&stop);"] {
        assert!(
            !production.contains(gone),
            "`{gone}` is the pre-collapse pair; the two controls are one now"
        );
    }

    // The painter reads the derivation and only looks this toolkit's ICON up.
    let painter = between(
        production,
        "fn refresh(&self, state: &ChromeState) {",
        "\n/// The app stylesheet",
    );
    assert!(
        painter.contains("let control = reload_stop_control(state);")
            && painter.contains("self.reload_stop.set_icon_name(reload_stop_icon(control));")
            && painter.contains("control.description()"),
        "the painter must take the mode, its icon and its description from the core: {painter:?}"
    );
    assert!(
        painter.contains("let spinning = load_spinner_visible(state);"),
        "the spinner's visibility must be the core's rule: {painter:?}"
    );

    // …and it decides NEITHER. The pre-collapse rule was a pair of
    // `set_sensitive` calls on the loading fact; nothing in the painter may
    // branch on that fact for the control or the spinner again.
    assert!(
        !painter.contains("state.is_loading()"),
        "the painter must not re-derive the control mode or the spinner from the raw \
         loading fact; that decision is `reload_stop_control` / `load_spinner_visible`: \
         {painter:?}"
    );
}

#[test]
fn the_gtk_back_and_forward_buttons_are_untouched() {
    // Desktop KEEPS its history buttons (spec `chrome-conventional-controls`
    // story 14, and the removal was explicitly rejected by the human); only the
    // MOBILE edges drop them, in their own tasks. Their sensitivity is still the
    // core's capability flags, which this task does not touch.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    assert!(
        production.contains("toolbar.append(&back);")
            && production.contains("toolbar.append(&forward);"),
        "the desktop toolbar keeps back and forward"
    );
    assert!(
        production.contains("self.back.set_sensitive(state.can_go_back);")
            && production.contains("self.forward.set_sensitive(state.can_go_forward);"),
        "the history buttons still read the core's capability flags"
    );
}

#[test]
fn cancel_survives_the_collapse_on_the_toolbar_and_on_the_keyboard() {
    // The separate Stop button was the documented cancel affordance, so the
    // collapse must not cost the user the cancel — on either route.

    // The toolbar route: the control's mode carries the ACTION it performs (the
    // same closed vocabulary the keyboard resolves into), and the GTK handler
    // performs it through the ONE performer, so the button and the chord cannot
    // drift apart.
    assert_eq!(ReloadStopControl::Stop.action(), ChromeAction::Stop);
    assert_eq!(ReloadStopControl::Reload.action(), ChromeAction::Reload);
    let production = desktop_shell();
    let production = desktop_production(&production);
    let handler = between(
        production,
        "reload_stop.connect_clicked({",
        "trust_pin_button.connect_clicked({",
    );
    assert!(
        handler.contains("reload_stop_control(shell.borrow().chrome()).action()")
            && handler.contains("perform_chrome_action("),
        "the toolbar control must perform the mode's own action through the shared \
         performer: {handler:?}"
    );
    assert!(
        !handler.contains(".stop()") && !handler.contains(".reload()"),
        "the handler must not decide for itself which of the two this click is: {handler:?}"
    );

    // The keyboard route (spec story 5), unchanged by the collapse: Escape with
    // the PAGE focused still resolves to Stop, and the edge still has an arm for
    // it.
    assert_eq!(
        shortcuts::resolve_chord(
            Chord::new(Key::Escape, Modifiers::NONE),
            Focus::Page,
            PrimaryModifier::Control,
        ),
        Some(ChromeAction::Stop),
        "Escape with the page focused must still cancel an in-flight load"
    );
    let performer = between(
        production,
        "fn perform_chrome_action(",
        "\n/// Builds the startup",
    );
    assert!(
        performer.contains("ChromeAction::Stop =>")
            && performer.contains("shell.borrow_mut().stop();"),
        "the edge must still perform a cancel: {performer:?}"
    );
}

#[test]
fn the_spinner_never_changes_what_the_url_bar_progress_bar_does() {
    // Story 9: the URL-bar progress bar is the liked, fine-grained signal and is
    // UNCHANGED. The spinner is a SECOND presentation of the same load, on the
    // same visibility rule, so the two surfaces can never contradict each other —
    // and the GTK painter still paints the bar from the same fraction it always
    // did.
    for state in [
        ChromeState::default(),
        ChromeState {
            load_state: LoadState::Started,
            load_step: LoadStep::Idle,
            ..ChromeState::default()
        },
        ChromeState {
            load_state: LoadState::Idle,
            load_step: LoadStep::ResolvingName,
            ..ChromeState::default()
        },
        ChromeState {
            load_state: LoadState::Finished,
            ..ChromeState::default()
        },
    ] {
        assert_eq!(
            load_spinner_visible(&state),
            load_progress_visible(&state),
            "the spinner and the URL-bar bar report the SAME load: {state:?}"
        );
    }
    let production = desktop_shell();
    let production = desktop_production(&production);
    assert!(
        production.contains(".set_progress_fraction(load_progress_fraction(state));"),
        "the URL bar's own progress fraction must be untouched by this task"
    );
}
