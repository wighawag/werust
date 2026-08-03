//! Desktop debug-view wiring shape guard (task
//! `debug-view-console-network-tabs-desktop`, spec
//! `in-app-debug-menu-console-and-network`).
//!
//! WHAT LANDED: the DESKTOP tabbed debug view. The browser menu's Debug entry
//! (the hook `general-browser-menu-with-version-and-debug-entry` left) now opens
//! a real debug-view window over the shared capture store: a `Notebook` of a
//! CONSOLE tab (level-coloured rows: level + message + source:line) and a
//! NETWORK tab (method / status / mime / size / trust / url rows), a CLEAR
//! action driving the store's `clear()`, live updates on the EXISTING chrome
//! pump cadence (no new timer, no busy loop), all READ-ONLY. The native F12
//! WebKit inspector is untouched and coexists as the deep devtools.
//!
//! WHY A SOURCE-SHAPE GUARD: the render-from-store MAPPING is unit-tested where
//! it lives (the pure `console_row_text` / `network_*` / `network_trust_*`
//! functions in `crates/werust/src/main.rs`), but the WIDGET tree itself needs a
//! display, so no automated test in this repo's pure-Rust `verify` gate can open
//! the real window. What compilation alone cannot prove is that the view really
//! renders the SHARED store (not a desktop-local copy), really reuses the trust
//! indicator's vocabulary (not a minted label), and really updates on the
//! existing pump. So this test PARSES the desktop shell and asserts that shape,
//! exactly as the sibling `browser_menu_edge_wiring_shape.rs` does for the menu
//! and `debug_capture_edge_wiring_shape.rs` for the capture points. It lives in
//! `werust-core` for the same reason they do: the guards ride the one shared
//! crate's `cargo test`, and the sibling `platform_capability_parity.rs` guard
//! covers the parity-matrix row (`debug-view-console-network`).
//!
//! Acceptance criteria mapped to assertions below:
//! 1. The Debug entry opens a tabbed (Console + Network) view rendering the core
//!    capture store
//!    (`the_debug_entry_opens_a_tabbed_console_and_network_view_over_the_shared_store`).
//! 2. The Network tab renders the per-request trust posture with the trust
//!    indicator's SAME vocabulary
//!    (`the_network_tab_reuses_the_trust_indicators_vocabulary_never_a_new_label`).
//! 3. Clear drives the store's `clear()`, and the view updates on the existing
//!    pump cadence with no busy loop
//!    (`clear_empties_the_shared_store_and_the_view_updates_on_the_existing_pump`).
//! 4. The view is READ-ONLY (no typeable input in the debug view) and the F12
//!    inspector is unaffected
//!    (`the_view_is_read_only_and_the_f12_inspector_is_untouched`).
//! 5. Desktop-scoped + parity-tracked: the matrix row's desktop cell is
//!    implemented (the mobile cells have since been implemented by
//!    `debug-view-console-network-tabs-mobile`, guarded by the sibling
//!    `debug_view_mobile_wiring_shape.rs`)
//!    (`the_desktop_cell_is_implemented_and_the_mobile_cells_are_the_sibling_guards_job`).
//! 6. Tests cover the mapping where testable (the `main.rs` unit tests); the
//!    window itself carries recorded manual steps at
//!    `docs/spikes/debug-view-console-network-tabs-desktop/README.md`.

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

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice (the same discipline the
/// sibling menu guard records).
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
fn the_debug_entry_opens_a_tabbed_console_and_network_view_over_the_shared_store() {
    // Criterion 1: the menu's Debug entry calls the named open-debug-view hook,
    // which builds a window with a GTK `Notebook` of exactly the CONSOLE and
    // NETWORK tabs. The placeholder the menu task landed is GONE: the hook is
    // filled with the real view.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("fn open_debug_view(")
            && desktop.contains("open_debug_view(&window, &debug_view)"),
        "the Debug entry must route to the named open_debug_view hook"
    );
    assert!(
        !desktop.contains("debug_view_placeholder_message"),
        "the menu task's placeholder must be gone: the hook now opens the REAL view"
    );
    assert!(
        desktop.contains("Notebook::new()"),
        "the debug view must be a tabbed GTK Notebook"
    );
    assert!(
        desktop.contains("\"Console\"") && desktop.contains("\"Network\""),
        "the debug view must have a Console tab and a Network tab"
    );

    // It renders the SHARED capture store: the view reads `console()` /
    // `network()` off a `DebugCapture` handle, and the shell hands the menu the
    // SAME store the capture points feed (`install_debug_capture` and
    // `with_debug_capture` clone the one `Arc` handle), never a desktop-local
    // copy.
    let view = between(&desktop, "struct DebugView {", "fn build_menu_button(");
    assert!(
        view.contains("self.capture.console()") && view.contains("self.capture.network()"),
        "the debug view must render the shared DebugCapture store: {view:?}"
    );
    assert!(
        desktop.contains("install_debug_capture") && desktop.contains("with_debug_capture"),
        "the store the view renders must be the one the capture points feed"
    );
}

#[test]
fn the_network_tab_reuses_the_trust_indicators_vocabulary_never_a_new_label() {
    // Criterion 2: the Network tab's per-request trust column renders the core's
    // wire names (`content-verified` / `unverified-origin` / …) and colours them
    // with the SAME `trust-*` CSS classes the chrome trust indicator toggles. No
    // new trust label is minted for the debug view (ADR-0006).
    //
    // The rules themselves MOVED into `werust_core::debug` (task
    // `macos-appkit-window-and-chrome`), so the desktop shell now CONSUMES them
    // rather than defining them, and the exact glyph + wire-name + class mapping
    // is pinned by the core unit test
    // `the_network_trust_column_speaks_the_chrome_trust_indicators_exact_vocabulary`
    // — one derivation, painted by both desktop views.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("werust_core::debug::{"),
        "the debug view's row rules must come from the shared core module"
    );
    for shared in ["network_trust_label", "network_trust_css_class"] {
        assert!(
            desktop.contains(shared),
            "the Network tab must render the core's `{shared}`, not a minted label"
        );
    }
    // And it must not have kept a private copy of the rule it now imports: the
    // glyph/class literals belong to the core, and the only `trust-*` names left
    // in this edge are the stylesheet rules that colour them.
    assert!(
        !desktop.contains("fn network_trust_label(")
            && !desktop.contains("fn network_trust_css_class("),
        "the desktop shell must not keep a second copy of the trust-column rules"
    );
    for class in [
        "trust-verified",
        "trust-name-trusted-rpc",
        "trust-mutable-name",
        "trust-unverified",
    ] {
        assert!(
            desktop.contains(&format!(".{class} {{")),
            "the edge must STYLE the trust indicator's `{class}` class its Network tab reuses"
        );
    }
}

#[test]
fn clear_empties_the_shared_store_and_the_view_updates_on_the_existing_pump() {
    // Criterion 3: the Clear action drives the SHARED store's `clear()` (both
    // buffers), and the open view is refreshed inside the EXISTING chrome pump
    // timeout (`timeout_add_local` over the shell's `pump()`), so a live capture
    // shows up on the same cadence as the rest of the chrome. No second timer and
    // no busy loop is added for the debug view.
    let desktop = desktop_shell();
    assert!(
        desktop.contains("Button::with_label(\"Clear\")"),
        "the debug view must have a Clear action"
    );
    assert!(
        desktop.contains("view.capture.clear();"),
        "Clear must drive the shared store's clear()"
    );
    let pump = between(
        &desktop,
        "glib::timeout_add_local(Duration::from_millis(50)",
        "window.present();",
    );
    assert!(
        pump.contains("view.borrow_mut().refresh();"),
        "the open debug view must refresh on the EXISTING pump cadence: {pump:?}"
    );
    assert_eq!(
        desktop.matches("timeout_add_local").count(),
        1,
        "no NEW timer may be added for the debug view: the one existing pump drives it"
    );

    // The incremental refresh must anchor on the store's MONOTONIC per-entry
    // SEQUENCE, never the store's length: a ring buffer AT its cap never
    // changes length (every push pairs with a `pop_front` eviction), so a
    // length-anchored refresh silently freezes exactly in the long-session
    // case the ring buffer exists for (the Gate-2 defect; the fix is recorded
    // in DECISIONS.md Decision 2).
    let view = between(&desktop, "struct DebugView {", "fn build_menu_button(");
    assert!(
        view.contains("tail_plan(") && view.contains("::sequence"),
        "the refresh must anchor on the store's monotonic entry sequence so \
         eviction at the cap cannot freeze the view: {view:?}"
    );
}

#[test]
fn the_view_is_read_only_and_the_f12_inspector_is_untouched() {
    // Criterion 4: the debug view is READ-ONLY (it renders the store; a typeable
    // REPL is the native inspector's job). Bounded to the debug-view code, it
    // must construct no input widget (`Entry`, `TextView`, `SearchEntry`). The
    // patterns name the CONSTRUCTIONS, not the bare `Entry::` prefix, so the
    // store's own `ConsoleEntry` / `NetworkEntry` types (which the refresh
    // maps over) do not trip it.
    let desktop = desktop_shell();
    let view = between(&desktop, "struct DebugView {", "fn build_menu_button(");
    for input in [
        "Entry::new",
        "Entry::builder",
        "TextView::",
        "SearchEntry::",
    ] {
        assert!(
            !view.contains(input),
            "the debug view must be READ-ONLY, but it builds a `{input}`: {view:?}"
        );
    }

    // The F12 native WebKit inspector coexists untouched: the shell still
    // performs the web-inspector action and calls `show` on the live view, and
    // the debug view is a SEPARATE window, not a replacement. (The F12 DECISION
    // itself moved into the shared `werust_core::shortcuts` table by task
    // `shortcut-resolution-in-core-and-the-gtk-edge`, where its
    // F12 / not-Ctrl+Shift+I assertions now live; this edge translates and
    // performs. The check is the same strength: the wiring must still be here.)
    assert!(
        desktop.contains("ChromeAction::OpenWebInspector") && desktop.contains("inspector.show()"),
        "the F12 WebKit inspector wiring must be untouched"
    );
    assert!(
        desktop.contains("Window::builder()") && desktop.contains(".transient_for(parent)"),
        "the debug view must be a separate window transient for the browser window"
    );
}

#[test]
fn the_desktop_cell_is_implemented_and_the_mobile_cells_are_the_sibling_guards_job() {
    // Criterion 5: this task was DESKTOP-scoped. The parity matrix row
    // (`debug-view-console-network`, enforced by `platform_capability_parity.rs`)
    // marks the desktop cell implemented. The mobile cells were stubbed onto
    // `debug-view-console-network-tabs-mobile` when this landed; that task has
    // since filled them, so the mobile half of the row (and the mobile hooks,
    // whose honest placeholder is GONE) is now asserted by the sibling
    // `debug_view_mobile_wiring_shape.rs`.
    let matrix = source("docs/platform-capability-matrix.toml");
    // The row is the LAST capability in the matrix, so slice to the end of the
    // file rather than to a following `[[capability]]` header.
    let row_start = matrix
        .find("name = \"debug-view-console-network\"")
        .expect("the matrix must carry the debug-view-console-network row");
    let row = &matrix[row_start..];
    assert!(
        row.contains("desktop = { state = \"implemented\" }"),
        "the desktop debug view must be marked implemented: {row:?}"
    );
}
