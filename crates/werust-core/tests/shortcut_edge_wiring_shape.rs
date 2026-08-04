//! Shortcut edge-wiring shape guard (task
//! `shortcut-resolution-in-core-and-the-gtk-edge`, spec
//! `chrome-conventional-controls`).
//!
//! WHAT LANDED: the conventional browser shortcuts, decided ONCE in
//! `werust_core::shortcuts` (a pure `(key, modifiers, focus) -> ChromeAction`
//! table plus the mouse's back/forward side buttons) and proven end to end on the
//! GTK desktop edge, which TRANSLATES its `gdk` events into that vocabulary and
//! PERFORMS what comes back.
//!
//! WHY A SOURCE-SHAPE GUARD: the resolution itself is unit-tested where it lives
//! (display-free, both accelerator conventions), and the `gdk` translation is
//! unit-tested in the desktop binary. What neither can prove is the property the
//! seam exists for: that the EDGE contains no decision about what a chord MEANS.
//! An edge that grows one branch ("Escape here means stop") compiles, passes
//! every unit test, and is exactly the drift `CONTEXT.md`'s ONE-derivation rule
//! records the repo paying for twice. So this test PARSES the desktop shell and
//! asserts that shape, exactly as the sibling `debug_view_desktop_wiring_shape.rs`
//! does for the debug view. It lives in `werust-core` for the same reason: the
//! guards ride the one shared crate's `cargo test`, and the sibling
//! `platform_capability_parity.rs` covers the parity row
//! (`conventional-shortcuts`).
//!
//! Acceptance criteria mapped to assertions below:
//! 1. The GTK edge translates native key events into the shared resolution and
//!    performs the result; no chord's MEANING is decided in the edge
//!    (`the_gtk_edge_translates_into_the_shared_resolution_and_decides_nothing`,
//!    `the_edge_names_no_key_meaning_outside_its_translation`).
//! 2. Every action in the shared vocabulary has an edge handler
//!    (`the_edge_handles_every_action_the_shared_vocabulary_defines`).
//! 3. History goes through the EXISTING seam methods and the EXISTING
//!    `ChromeState` capability flags, which are unchanged
//!    (`history_rides_the_existing_seam_and_its_capability_flags`).
//! 4. Mouse buttons 4 and 5 navigate history on this edge
//!    (`the_mouse_side_buttons_ride_the_same_resolution_and_the_same_performer`).
//! 5. The old per-edge F12 predicate is GONE, folded into the table
//!    (`the_f12_predicate_is_gone_and_the_web_inspector_is_a_table_row`).
//! 6. Desktop-scoped + parity-tracked: the matrix row's desktop cell is
//!    implemented and any sibling edge that has not landed yet is a tracked stub
//!    (`the_desktop_cell_is_implemented_and_the_sibling_edges_are_tracked`).
//!
//! AMENDMENT (task `shortcuts-and-mouse-history-buttons-on-the-windows-edge`):
//! the Windows cell moved from `stubbed` to `implemented` when that edge landed
//! its own translation (`crates/werust-windows/src/shortcuts.rs`, guarded by
//! `crates/werust-windows/tests/windows_window_shape.rs`). This file keeps
//! asserting the property that matters here -- the row exists, desktop is real,
//! and an edge that has NOT landed still names its task -- rather than freezing
//! the sibling cells, so the macOS task can flip its own cell the same way.

use std::path::{Path, PathBuf};

use werust_core::shortcuts::ChromeAction;

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
/// test code.
fn desktop_production(shell: &str) -> &str {
    let tests = shell
        .find("#[cfg(test)]")
        .expect("the desktop shell must have a test module");
    &shell[..tests]
}

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice (the discipline the
/// sibling debug-view guard records).
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
fn the_gtk_edge_translates_into_the_shared_resolution_and_decides_nothing() {
    // Criterion 1: the edge's whole key path is translate -> ask the core ->
    // perform. `shortcut_action` maps the `gdk` keyval + modifier flags into the
    // shared vocabulary and calls the SHARED resolution; the key controller
    // performs whatever comes back and PROPAGATES anything unclaimed.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    assert!(
        production.contains("shortcuts::resolve_chord("),
        "the edge must ask the shared resolution what a chord means"
    );

    let translation = between(
        production,
        "fn shortcut_action(",
        "fn shortcut_pointer_button(",
    );
    assert!(
        translation.contains("shortcut_key(keyval)") && translation.contains("shortcut_modifiers("),
        "the edge must TRANSLATE the native keyval + modifiers: {translation:?}"
    );
    assert!(
        translation.contains("PrimaryModifier::for_target()"),
        "the accelerator convention is the core's call, not a per-edge constant: {translation:?}"
    );

    // The controller performs and propagates; it does not interpret.
    let controller = between(
        production,
        "let key_controller = gtk4::EventControllerKey::new();",
        "window.add_controller(key_controller);",
    );
    assert!(
        controller.contains("shortcut_action(keyval, modifiers, focus)")
            && controller.contains("perform_chrome_action("),
        "the key controller must resolve then perform: {controller:?}"
    );
    assert!(
        controller.contains("glib::Propagation::Proceed"),
        "a chord the core does not claim must reach the page/URL bar untouched: {controller:?}"
    );

    // Focus is REPORTED as an input, never branched on at the edge: the edge
    // asks one question ("is the URL bar focused?") and hands the answer over.
    assert!(
        controller.contains("shortcut_focus(&url_entry)"),
        "the edge must report focus into the resolution: {controller:?}"
    );
}

#[test]
fn the_edge_names_no_key_meaning_outside_its_translation() {
    // Criterion 1, the teeth: the ONLY place the edge may name a specific key is
    // its translation function. If `Key::Escape` (or any other key) appeared in
    // the performer or the controller, the edge would be deciding what that key
    // means, which is precisely the per-edge drift this seam removes.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    let translation = between(production, "fn shortcut_key(", "fn shortcut_modifiers(");

    for key in [
        "shortcuts::Key::Escape",
        "shortcuts::Key::F5",
        "shortcuts::Key::F12",
        "shortcuts::Key::ArrowLeft",
        "shortcuts::Key::ArrowRight",
        "shortcuts::Key::Character",
    ] {
        assert_eq!(
            production.matches(key).count(),
            translation.matches(key).count(),
            "`{key}` may only be named inside `shortcut_key` (translation), \
             never where the edge acts on it"
        );
    }

    // Likewise the raw GDK keyvals: naming one outside the translation would be
    // a native-event branch, i.e. a second, edge-local shortcut table.
    assert_eq!(
        production.matches("gdk::Key::").count(),
        translation.matches("gdk::Key::").count(),
        "the edge may only look at raw GDK keyvals while TRANSLATING them"
    );
}

#[test]
fn the_edge_handles_every_action_the_shared_vocabulary_defines() {
    // Criterion 2: one performer, with an arm for every action the core can
    // resolve. Driven off `ChromeAction::ALL` rather than a hand-copied list, so
    // a future action added to the shared vocabulary reds here until this edge
    // (which HAS every capability today) handles it.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    let performer = between(
        production,
        "fn perform_chrome_action(",
        "\n/// Builds the startup",
    );
    for action in ChromeAction::ALL {
        assert!(
            performer.contains(&format!("ChromeAction::{action:?}")),
            "the GTK edge must handle {action:?}: {performer:?}"
        );
    }
}

#[test]
fn history_rides_the_existing_seam_and_its_capability_flags() {
    // Criterion 3: a shortcut performs history EXACTLY as the toolbar button
    // does, via `BrowserShell::go_back` / `go_forward` (the existing `Renderer`
    // seam methods) gated on the existing `ChromeState` capability flags, so a
    // chord can never drive a move the on-screen control refuses.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    let performer = between(
        production,
        "fn perform_chrome_action(",
        "\n/// Builds the startup",
    );
    for expected in [
        "chrome().can_go_back",
        "go_back()",
        "chrome().can_go_forward",
        "go_forward()",
    ] {
        assert!(
            performer.contains(expected),
            "the history actions must go through `{expected}`: {performer:?}"
        );
    }

    // …and that seam is UNCHANGED by this task: the Android hardware Back button
    // rides the same methods, so they must still be the seam's own.
    let seam = source("crates/renderer/src/lib.rs");
    assert!(
        seam.contains("fn go_back(&mut self)") && seam.contains("fn go_forward(&mut self)"),
        "the Renderer seam's history methods must be unchanged"
    );
    let core = source("crates/werust-core/src/lib.rs");
    assert!(
        core.contains("pub can_go_back: bool") && core.contains("pub can_go_forward: bool"),
        "the ChromeState capability flags must be unchanged"
    );
    // The shortcut layer added NO seam method: it is input plumbing over the
    // controls that already existed.
    assert!(
        !seam.contains("shortcut") && !seam.contains("Chord"),
        "the shortcut layer must not have leaked into the Renderer seam"
    );
}

#[test]
fn the_mouse_side_buttons_ride_the_same_resolution_and_the_same_performer() {
    // Criterion 4: mouse buttons 4 and 5 navigate history, through the SAME
    // resolution and the SAME performer the keyboard uses (the same
    // input-to-action plumbing), and the edge knows only the BUTTON NUMBER.
    let shell = desktop_shell();
    let production = desktop_production(&shell);
    assert!(
        production.contains("const GDK_BUTTON_BACK: u32 = 8;")
            && production.contains("const GDK_BUTTON_FORWARD: u32 = 9;"),
        "the side buttons arrive as GDK buttons 8 and 9"
    );

    let gesture = between(
        production,
        "let mouse_controller = gtk4::GestureClick::new();",
        "window.add_controller(mouse_controller);",
    );
    assert!(
        gesture.contains("shortcut_pointer_button(gesture.current_button())")
            && gesture.contains("shortcuts::resolve_pointer_button(button)")
            && gesture.contains("perform_chrome_action("),
        "the mouse path must translate, ask the core, then perform: {gesture:?}"
    );
    // No history call of its own: the button path must not shortcut past the
    // shared vocabulary into the shell.
    assert!(
        !gesture.contains("go_back()") && !gesture.contains("go_forward()"),
        "the mouse path must not decide that a button means history: {gesture:?}"
    );
}

#[test]
fn the_f12_predicate_is_gone_and_the_web_inspector_is_a_table_row() {
    // Criterion 5: the ONE binding werust used to have was an edge-local
    // predicate (`should_open_web_inspector`). It is folded INTO the shared
    // table, not left beside it, and its behaviour survives: the edge still
    // opens the inspector, but only when the core resolves that action.
    let shell = desktop_shell();
    assert!(
        !shell.contains("should_open_web_inspector"),
        "the edge-local F12 predicate must be gone, folded into the shared table"
    );
    let production = desktop_production(&shell);
    assert!(
        production.contains("ChromeAction::OpenWebInspector")
            && production.contains("inspector.show()"),
        "the edge must still open the WebKit inspector when the core resolves it"
    );
}

#[test]
fn the_desktop_cell_is_implemented_and_the_sibling_edges_are_tracked() {
    // Criterion 6: this task is DESKTOP-scoped, and the parity matrix says so
    // out loud (enforced by `platform_capability_parity.rs`): the desktop cell is
    // implemented, and the two other desktop edges are stubs pointing at their
    // own sibling tasks rather than silent gaps.
    let matrix = source("docs/platform-capability-matrix.toml");
    let row = between(
        &matrix,
        "name = \"conventional-shortcuts\"",
        "\n[[capability]]",
    );
    assert!(
        row.contains("desktop = { state = \"implemented\" }"),
        "the GTK edge's shortcuts must be marked implemented: {row:?}"
    );
    for (platform, task) in [
        (
            "macos",
            "shortcuts-and-mouse-history-buttons-on-the-macos-edge",
        ),
        (
            "windows",
            "shortcuts-and-mouse-history-buttons-on-the-windows-edge",
        ),
    ] {
        // Either the edge has LANDED (its cell is implemented, as the Windows one
        // now is) or its gap is TRACKED on the sibling task -- never a silent
        // absence, which is the whole point of the row. Which of the two it is
        // stays the parity guard's business.
        assert!(
            row.contains(&format!("{platform} = {{ state = \"implemented\" }}"))
                || row.contains(&format!(
                    "{platform} = {{ state = \"stubbed\", task = \"{task}\" }}"
                )),
            "the {platform} edge must be implemented or a TRACKED stub on its sibling task: \
             {row:?}"
        );
    }
}
