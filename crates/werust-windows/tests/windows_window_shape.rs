//! Windows WINDOW shape guard (task `windows-win32-window-and-chrome`,
//! `docs/adr/0011-webview2-for-windows.md`'s Windows split, sub-task 3).
//!
//! WHY A SOURCE-SHAPE GUARD: the window's Win32 half is `#[cfg(windows)]`, and
//! this repo's `verify` gate runs on Ubuntu with no Windows SDK, so `cargo build`
//! NEVER compiles it. That is the same position `crates/windows-renderer` and
//! `crates/werust-macos` are in, and the repo's answer is the same: a plain
//! `cargo test` that PARSES the source it cannot compile and asserts the
//! properties compilation would not have proven anyway --
//!
//! * that every chrome surface exists at all,
//! * that the Win32 layer PAINTS the shared derivation instead of re-deriving it
//!   (the failure that already cost this project the Kotlin and Swift twins), and
//!   that it consumes the SHARED carrier rather than a Windows copy of it,
//! * that the ⋮ menu is the core's `BrowserMenu` and devtools are the platform's
//!   own `OpenDevToolsWindow`,
//! * that ADR-0009 / ADR-0010 / the URL-bar-progress rule are FOLLOWED, not
//!   re-decided,
//! * that the shell passes a DURABLE profile rather than inheriting the engine's
//!   development `%TEMP%` default, and
//! * that the CI leg really exercises the new surface.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. Every surface is present over the WebView2 backend
//!    (`the_window_carries_every_chrome_surface_over_the_webview2_backend`).
//! 2. Every surface reads the SHARED derivation, through the SHARED carrier
//!    (`the_win32_layer_paints_and_never_derives`,
//!    `the_paint_carrier_is_shared_with_the_appkit_window_not_copied`,
//!    `the_class_names_and_labels_are_never_restated_in_the_window`).
//! 3. The ⋮ menu is the core's, and devtools are `OpenDevToolsWindow`
//!    (`the_menu_is_the_cores_browser_menu_and_devtools_are_the_platforms_own`).
//! 4. ADR-0009, ADR-0010 and the URL-bar-progress rule are honoured
//!    (`the_os_colour_scheme_is_followed_from_one_reader_never_forced`,
//!    `new_windows_navigate_in_place_via_the_shared_rule`,
//!    `progress_lives_in_the_url_bar_and_only_a_failure_moves_the_page`).
//! 5. The profile is durable
//!    (`the_shell_passes_a_durable_profile_not_the_engines_temp_default`).
//! 6. What CI proves versus what awaits a Windows box is written down, and the
//!    CI leg exercises this crate
//!    (`the_ci_leg_builds_tests_and_runs_the_window`,
//!    `the_verification_honesty_is_recorded`).
//! 7. The gate stays green (this file is a plain `cargo test`).
//!
//! AMENDMENT (task `shortcuts-and-mouse-history-buttons-on-the-windows-edge`):
//! the conventional browser shortcuts reached this edge. What each chord and
//! side button MEANS was decided once, for every edge, in
//! `werust_core::shortcuts`; this edge contributes TRANSLATION (virtual-key codes
//! and `GetKeyState` bits into that vocabulary, in the pure
//! `crates/werust-windows/src/shortcuts.rs` the Ubuntu gate unit-tests) and
//! EXECUTION. The property neither unit test can prove -- that the EDGE decides
//! nothing, and that the delivery paths really go through the shared resolution
//! -- is guarded here, exactly as the sibling
//! `crates/werust-core/tests/shortcut_edge_wiring_shape.rs` guards the GTK edge
//! (`the_win32_edge_translates_into_the_shared_resolution_and_decides_nothing`,
//! `the_edge_names_no_key_meaning_outside_its_translation`,
//! `the_edge_handles_every_action_the_shared_vocabulary_defines`,
//! `history_rides_the_existing_seam_and_its_capability_flags`,
//! `the_side_buttons_ride_the_same_resolution_and_the_same_performer`,
//! `the_page_focused_keys_arrive_through_the_engines_accelerator_hook`,
//! `the_shortcut_paths_are_exercised_on_the_windows_leg`).
//!
//! AMENDMENT (task `reload-stop-collapse-and-spinner-on-the-windows-chrome`):
//! the separate Reload and Stop controls are ONE control, and a loading spinner
//! joined the toolbar. Both the control's MODE and the spinner's VISIBILITY are
//! read off the shared `desktop-paint` snapshot (`reload_stop_control` /
//! `load_spinner_visible` in `werust-core`), so this edge assigns values where it
//! used to enable one of a pair on the raw loading fact. A local conditional for
//! either would be exactly the per-edge twin the one-derivation rule forbids, and
//! it would compile and pass every unit test, so it is pinned here
//! (`the_chrome_carries_one_reload_stop_control_and_a_spinner_it_does_not_derive`,
//! `the_collapsed_control_performs_the_modes_own_action_and_history_is_untouched`).
//!
//! AMENDMENT (task `windows-smoke-mouse-back-check-runs-after-a-failed-load`,
//! 2026-08-04): the mouse-back check above turned the whole leg RED on `main`,
//! not because the edge was wrong but because the smoke asked for history it had
//! never created -- it ran straight after the TAMPERED load, which fails CLOSED
//! (no response is set, built-in error pages are off), so WebView2 commits no
//! document and therefore adds no back entry, and the reloads before it replace
//! the current entry rather than adding one. A SEQUENCE is invisible to the
//! compiler and to every Linux unit test, and the Windows leg can only report
//! FAIL after the fact, so the section's precondition -- that it establishes its
//! own two verified entries, and bounds the wait on the failure path -- is pinned
//! here (`the_mouse_back_check_establishes_the_history_it_asks_for`).
//!
//! AMENDMENT (task `windows-gui-subsystem-no-console-window`, 2026-07-31): the
//! first run on REAL hardware found the binary linking as a CONSOLE-subsystem
//! app, so Windows opened a console window beside the browser
//! (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`).
//! That is a property of the BINARY, not of any code path a runner can execute
//! -- the Windows leg would report it green forever -- so it is pinned HERE, by
//! reading `main.rs`, together with the surfaces that keep a startup FAILURE
//! legible once there is no console to print to
//! (`the_binary_links_as_a_gui_app_and_a_startup_failure_stays_legible`).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-windows`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn exists(relative: &str) -> bool {
    repo_root().join(relative).exists()
}

/// The whole Win32 half, as one text: the modules the Ubuntu gate cannot
/// compile. Asserting over the set (rather than over one file) means a rule that
/// leaks into a neighbouring module is still caught.
fn win32_half() -> String {
    [
        "crates/werust-windows/src/window.rs",
        "crates/werust-windows/src/chrome.rs",
        "crates/werust-windows/src/debugview.rs",
        "crates/werust-windows/src/win32.rs",
        "crates/werust-windows/src/startup.rs",
    ]
    .iter()
    .map(|path| source(path))
    .collect::<Vec<_>>()
    .join("\n")
}

fn window() -> String {
    source("crates/werust-windows/src/window.rs")
}

fn chrome() -> String {
    source("crates/werust-windows/src/chrome.rs")
}

/// The DPI seam: the chrome's 96-DPI design metrics and the arithmetic that
/// turns them into pixels for the display the window is on.
fn dpi_seam() -> String {
    source("crates/werust-windows/src/dpi.rs")
}

/// The SHORTCUT seam: the pure Win32-to-`werust_core::shortcuts` translation.
fn shortcut_translation() -> String {
    source("crates/werust-windows/src/shortcuts.rs")
}

/// Every integer literal in `body` that does NOT go through the DPI seam.
///
/// A raw pixel that survives in a layout is exactly this task's defect (a
/// control that is subtly misaligned rather than obviously broken), so the
/// guard scans for them instead of listing the ones it happens to remember.
/// Literals inside `metrics.scale(…)` are the seam's own arguments and are
/// removed first; a digit run glued to an identifier (`i32`, `x2`) is not a
/// pixel.
fn unscaled_literals(body: &str) -> Vec<String> {
    let mut stripped = String::new();
    let mut rest = body;
    while let Some(start) = rest.find("metrics.scale(") {
        stripped.push_str(&rest[..start]);
        let after = &rest[start + "metrics.scale(".len()..];
        let end = after.find(')').unwrap_or(0);
        rest = &after[end..];
    }
    stripped.push_str(rest);

    let bytes: Vec<char> = stripped.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let glued = start > 0 && (bytes[start - 1].is_alphabetic() || bytes[start - 1] == '_');
        if !glued {
            found.push(bytes[start..index].iter().collect::<String>());
        }
    }
    found
}

/// The SHARED carrier both native desktop windows paint from.
fn paint() -> String {
    source("crates/desktop-paint/src/lib.rs")
}

/// `source` with every comment line dropped, so a "does this file mention X"
/// assertion is about the CODE and not about prose. The window's docs
/// legitimately DISCUSS the rules it must not re-implement.
fn code_only(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            // `#` catches a TOML comment too, which is why this helper is also
            // applied to a manifest.
            !(trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with('#'))
        })
        .collect::<Vec<_>>()
        .join("\n")
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
fn the_window_carries_every_chrome_surface_over_the_webview2_backend() {
    // Criterion 1: a real top-level window with every surface the task names,
    // hosting the Windows backend's container window through the SEAM (never a
    // WebView2 of its own).
    let window = window();
    assert!(
        window.contains("CreateWindowExW(") && window.contains("WS_OVERLAPPEDWINDOW"),
        "the shell must open a real top-level Win32 window"
    );
    // The page arrives through the seam's opaque handle, so this window has no
    // WebView2 dependency at all.
    assert!(
        window.contains("shell.borrow().view_handle()") && window.contains("SetParent(page"),
        "the page must be embedded through the seam's ViewHandle, by re-parenting"
    );
    let manifest = code_only(&source("crates/werust-windows/Cargo.toml"));
    assert!(
        !manifest.contains("webview2-com"),
        "the window must not reach past the seam to WebView2: that is the engine's job"
    );

    // Every surface, by the widget that carries it.
    let chrome = chrome();
    for (surface, needle) in [
        ("the URL bar", "pub url_edit: HWND"),
        ("Back", "pub back: HWND"),
        ("Forward", "pub forward: HWND"),
        ("the ONE reload/stop control", "pub reload_stop: HWND"),
        ("the loading spinner", "pub spinner: HWND"),
        ("the trust indicator", "pub trust: HWND"),
        ("the invalid-entry badge", "pub invalid_badge: HWND"),
        ("the ⋮ menu", "pub menu_button: HWND"),
        ("the error banner", "pub error_banner: HWND"),
        ("the load-progress indicator", "pub progress: HWND"),
        ("the status line", "pub status: HWND"),
        ("the trust EXPLANATION's tooltip", "pub tooltip: HWND"),
    ] {
        assert!(
            chrome.contains(needle),
            "{surface} must be part of the window's chrome"
        );
    }

    // The tabbed debug view: a Console and a Network tab over the SHARED store.
    assert!(
        window.contains("SysTabControl32")
            && window.contains("\"Console\"")
            && window.contains("\"Network\""),
        "the debug view must be a tabbed Console + Network view"
    );
    assert!(
        window.contains("controller.capture.clear()"),
        "the debug view's Clear must empty the SHARED capture store"
    );
    // READ-ONLY by construction: the debug view builds list views of labels,
    // never an editable control. (`EDIT` is the URL bar, built in
    // `BrowserWindow::open`, not in the debug window.)
    let debug_view = between(
        &window,
        "fn build_debug_window(",
        "/// The Windows browser window",
    );
    for input in ["EDIT", "RichEdit", "ES_AUTOHSCROLL"] {
        assert!(
            !debug_view.contains(input),
            "the debug view must be READ-ONLY, but it builds a `{input}`"
        );
    }

    // Every control drives the SHARED shell, never the webview. (The ONE
    // reload/stop control drives it through the shared performer instead, which
    // `the_collapsed_control_performs_the_modes_own_action_and_history_is_untouched`
    // pins.)
    for (action, drives) in [
        ("ID_BACK", "shell.borrow_mut().go_back()"),
        ("ID_FORWARD", "shell.borrow_mut().go_forward()"),
        ("ID_URL_ENTER", "shell.borrow_mut().navigate("),
    ] {
        assert!(
            window.contains(action) && window.contains(drives),
            "`{action}` must drive the shared shell (`{drives}`)"
        );
    }

    // The chrome is pumped on ONE timer, the same 50ms cadence the GTK and
    // AppKit shells use, and the debug view rides it rather than adding a second.
    assert!(
        window.contains("SetTimer(") && window.contains("PUMP_INTERVAL_MS"),
        "the chrome must be pumped on the window's message loop"
    );
    assert_eq!(
        window.matches("SetTimer(").count(),
        1,
        "no second timer may be added: the one pump drives the chrome AND the debug view"
    );
    let tick = between(&window, "fn tick(&self) {", "\n    }\n");
    assert!(
        tick.contains("shell.borrow_mut().pump()") && tick.contains("self.refresh_debug_view()"),
        "one tick must fold the seam's events into the chrome and catch the debug view up: {tick:?}"
    );
}

#[test]
fn the_win32_layer_paints_and_never_derives() {
    // Criterion 2, the heart of this task: the Win32 half assigns values the
    // shared carrier (and through it `werust-core`) decided. It must not call the
    // chrome derivation itself -- not because calling it would be wrong, but
    // because the carrier is the half the Ubuntu gate can COMPILE and TEST, and
    // every rule that leaks past it into Win32-land leaves the gate's reach.
    let win32 = code_only(&win32_half());
    for rule in [
        "status_line(",
        "trust_indicator(",
        "trust_indicator_detail(",
        "trust_indicator_css_class(",
        "error_banner_visible(",
        "error_banner_text(",
        "error_banner_css_class(",
        "invalid_entry_badge_visible(",
        "invalid_entry_badge_text(",
        "load_progress_visible(",
        "load_progress_fraction(",
        "load_progress_hint(",
        "load_progress_tooltip(",
        "reload_stop_control(",
        "load_spinner_visible(",
        "console_row_text(",
        "network_trust_label(",
        "network_status_text(",
    ] {
        assert!(
            !win32.contains(rule),
            "the Win32 layer must read `{rule}`'s result from the shared carrier, not call it \
             (that is the half the Ubuntu gate compiles and tests)"
        );
    }
    // And the SHARED carrier must be the one that DOES call them, from the core.
    let paint = paint();
    for rule in [
        "status_line(state)",
        "trust_indicator(state)",
        "trust_indicator_detail(state)",
        "trust_indicator_css_class(state)",
        "error_banner_visible(state)",
        "error_banner_text(state)",
        "error_banner_css_class(state)",
        "invalid_entry_badge_visible(state)",
        "invalid_entry_badge_text(state)",
        "load_progress_visible(state)",
        "load_progress_fraction(state)",
        "reload_stop_control(state)",
        "load_spinner_visible(state)",
    ] {
        assert!(
            paint.contains(rule),
            "the shared carrier must derive `{rule}` from the core"
        );
    }

    // The window paints a snapshot, field by field: it reads `ChromePaint`.
    assert!(
        chrome().contains("pub fn apply(&self, paint: &ChromePaint)"),
        "the window must paint a derived snapshot"
    );
    // Including the trust EXPLANATION, which is the surface that shipped
    // desktop-only for months because three edges each hand-wrote the chrome.
    assert!(
        chrome().contains("self.set_tip(self.trust, paint.trust_detail)"),
        "the trust badge must carry the core's EXPLANATION"
    );
}

#[test]
fn the_paint_carrier_is_shared_with_the_appkit_window_not_copied() {
    // Criterion 2's other half, and the reason this task extracted a crate rather
    // than copying a module: the CARRIER (and the palette inside it) is ONE
    // implementation, consumed by both native desktop windows. A Windows copy
    // would have been the fourth hand-maintained chrome in this repo -- the
    // specific failure `docs/adr/0011`'s Consequences section warns about.
    assert!(
        exists("crates/desktop-paint/src/lib.rs"),
        "the shared painter half must exist as its own crate"
    );
    for (edge, manifest) in [
        ("werust-windows", "crates/werust-windows/Cargo.toml"),
        ("werust-macos", "crates/werust-macos/Cargo.toml"),
    ] {
        assert!(
            source(manifest).contains("desktop-paint = { path = \"../desktop-paint\" }"),
            "{edge} must CONSUME the shared painter half"
        );
    }
    assert!(
        !exists("crates/werust-macos/src/paint.rs"),
        "the extraction must leave no copy behind in the macOS crate"
    );
    // The palette lives ONCE. A second table anywhere is a second source of
    // truth for "the same green on both desktops".
    let palette_holders: Vec<&str> = [
        "crates/desktop-paint/src/lib.rs",
        "crates/werust-windows/src/window.rs",
        "crates/werust-windows/src/chrome.rs",
        "crates/werust-windows/src/win32.rs",
    ]
    .into_iter()
    .filter(|path| source(path).contains("CLASS_COLORS"))
    .collect();
    assert_eq!(
        palette_holders,
        vec!["crates/desktop-paint/src/lib.rs"],
        "the palette must exist in exactly one place"
    );
    // Both windows name it the same thing, so `paint` still means one thing.
    for lib in [
        "crates/werust-windows/src/lib.rs",
        "crates/werust-macos/src/lib.rs",
    ] {
        assert!(
            source(lib).contains("pub use desktop_paint as paint;"),
            "{lib} must expose the shared carrier under the name both windows already use"
        );
    }
}

#[test]
fn the_class_names_and_labels_are_never_restated_in_the_window() {
    // The other half of criterion 2: no state class name, badge label or banner
    // wording is written in the Win32 layer. Those are the strings that drifted
    // between the desktop, Kotlin and Swift copies; here they can only arrive
    // through the shared carrier.
    let win32 = code_only(&win32_half());
    for restated in [
        "trust-loading",
        "trust-verified",
        "trust-name-trusted-rpc",
        "trust-mutable-name",
        "trust-unverified",
        "error-banner",
        "debug-console-",
        "content-verified",
        "unverified origin",
        "invalid URL",
        "failed to load",
        "loading…",
    ] {
        assert!(
            !win32.contains(restated),
            "`{restated}` is restated in the Win32 layer; it must come from the shared derivation"
        );
    }
    // No hex colour is written here either: the palette is the shared carrier's,
    // and this edge only CONVERTS it (Win32's COLORREF byte order is reversed,
    // which is exactly the transcription that must happen once).
    let converters = code_only(&source("crates/werust-windows/src/win32.rs"));
    assert!(
        converters.contains("pub fn colorref(rgb: Rgb) -> COLORREF"),
        "the edge converts the shared palette's colours; it does not restate them"
    );
}

#[test]
fn the_menu_is_the_cores_browser_menu_and_devtools_are_the_platforms_own() {
    // Criterion 3: the ⋮ menu is built from the core's `BrowserMenu` (so a new
    // core item appears here with no Windows change), an item is dispatched by
    // its STABLE id, and devtools are Edge's own `OpenDevToolsWindow` rather than
    // anything werust drew.
    let paint = paint();
    assert!(
        paint.contains("BrowserMenu::new()") && paint.contains("MenuItemKind::Action"),
        "the menu items must come from the core's BrowserMenu"
    );
    let window = window();
    assert!(
        window.contains("fn build_browser_menu(") && window.contains("menu_items()"),
        "the HMENU must be built from the core's item list"
    );
    assert!(
        window.contains("chosen.id == MENU_ITEM_DEBUG"),
        "a chosen item must be dispatched by its STABLE id, never its label"
    );
    // Devtools: the platform's own window, reached through the engine.
    assert!(
        window.contains("dev_tools.open()") && window.contains("fn open_dev_tools"),
        "the shell must offer the platform's own devtools"
    );
    let engine = source("crates/windows-renderer/src/backend.rs");
    assert!(
        engine.contains("OpenDevToolsWindow()"),
        "devtools must be WebView2's own OpenDevToolsWindow, not a werust re-implementation"
    );
    assert!(
        engine.contains("SetAreDevToolsEnabled(cfg!(debug_assertions))"),
        "devtools must be gated on a debug build, as every other platform's row is"
    );
}

#[test]
fn the_os_colour_scheme_is_followed_from_one_reader_never_forced() {
    // Criterion 4, `docs/adr/0009`: FOLLOW the OS, never force dark. Unlike
    // AppKit, Win32 propagates nothing, so the chrome must READ the setting --
    // through the ENGINE crate's ONE registry read, mapped by the SHARED
    // `OsColorScheme` rule -- and re-read it when it changes.
    let window = window();
    assert!(
        window.contains("windows_renderer::os_color_scheme()"),
        "the chrome must read the OS setting through the engine's ONE reader"
    );
    assert!(
        window.contains("WM_SETTINGCHANGE") && window.contains("fn follow_os_color_scheme"),
        "the chrome must re-read the OS setting when Windows says it changed"
    );
    // The window must not mint a second reader.
    let win32 = code_only(&win32_half());
    for forked in ["AppsUseLightTheme", "RegGetValueW", "Personalize"] {
        assert!(
            !win32.contains(forked),
            "`{forked}` is read in the window; the platform read belongs to the ONE reader in the \
             engine crate"
        );
    }
    // And the light/dark DECISION is the shared rule's, not a local guess: only
    // `prefer_dark()` may select dark.
    let helpers = source("crates/werust-windows/src/win32.rs");
    let theme = between(
        &helpers,
        "pub fn of(scheme: OsColorScheme) -> Self {",
        "\n    }\n",
    );
    assert!(
        theme.contains("scheme.prefer_dark()"),
        "the theme must follow the shared OsColorScheme rule: {theme:?}"
    );
    assert!(
        !theme.contains("OsColorScheme::NoPreference =>") && !theme.contains("== OsColorScheme::"),
        "the edge must not re-decide what NoPreference means: {theme:?}"
    );
    // The engine's own half of the ADR is untouched by this task.
    assert!(
        source("crates/windows-renderer/src/backend.rs")
            .contains("COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO"),
        "the engine must still let WebView2 follow the OS natively"
    );
}

#[test]
fn new_windows_navigate_in_place_via_the_shared_rule() {
    // Criterion 4, `docs/adr/0010`: a `target="_blank"` / `window.open` navigates
    // IN PLACE until tabs exist. That rule is the ENGINE's `add_NewWindowRequested`
    // hook over the SHARED `renderer::new_window_action`; this window must neither
    // open a second window for it nor re-decide it.
    let win32 = code_only(&win32_half());
    assert!(
        !win32.contains("new_window_action") && !win32.contains("NewWindowRequested"),
        "the new-window rule is the engine's, over the shared decision; the window must not fork it"
    );
    let backend = source("crates/windows-renderer/src/backend.rs");
    assert!(
        backend.contains("new_window_action(Some(uri.as_str()))")
            && backend.contains("add_NewWindowRequested"),
        "the engine must still route new windows through the shared rule"
    );
    // Exactly TWO window classes are registered here: the browser window and the
    // debug view, which hosts no page.
    let window = window();
    assert_eq!(
        window.matches("register_class(").count(),
        3,
        "one helper and its two call sites: the browser window and the debug view, no more"
    );
}

#[test]
fn progress_lives_in_the_url_bar_and_only_a_failure_moves_the_page() {
    // Criterion 4, the loading rule (`loading-progress-in-the-url-bar-not-a-banner`):
    // in-flight progress must be visible WITHOUT displacing the page; only a
    // FAILURE may take a banner.
    let chrome = chrome();
    let layout = between(&chrome, "pub fn relayout(&self) {", "\n    }\n");
    // The progress bar is laid out inside the URL bar's own rectangle, in the
    // fixed-height toolbar.
    assert!(
        layout.contains("place(\n            self.progress,") && layout.contains("url_width"),
        "the progress strip must be laid out INSIDE the URL bar's own rectangle: {layout:?}"
    );
    // The page's height depends on the BANNER only -- never on progress.
    assert!(
        layout.contains("let page_top = metrics.toolbar_height + banner_height;"),
        "only the error banner may change the page's geometry: {layout:?}"
    );
    let banner_height = between(
        &chrome,
        "let banner_height = if self.banner_visible.get()",
        ";",
    );
    assert!(
        banner_height.contains("metrics.banner_height"),
        "the banner takes its strip only when it is visible"
    );
    // And the banner is visible only when the core says a load FAILED.
    assert!(
        paint().contains("error_visible: error_banner_visible(state)"),
        "the banner's visibility is the core's failure rule, not a Windows choice"
    );
}

#[test]
fn the_chrome_carries_one_reload_stop_control_and_a_spinner_it_does_not_derive() {
    // The collapse (spec `chrome-conventional-controls`, story 10): the two
    // controls that were enabled on the negation of each other's condition are
    // ONE control whose MODE the core derives, and the spinner (story 8) joins
    // the URL bar's progress fraction (story 9, untouched).
    let chrome = chrome();
    assert!(
        chrome.contains("pub reload_stop: HWND") && chrome.contains("pub spinner: HWND"),
        "the toolbar must carry the ONE reload/stop control and the spinner"
    );
    for gone in ["pub reload: HWND", "pub stop: HWND"] {
        assert!(
            !chrome.contains(gone),
            "`{gone}` is the pre-collapse pair; the two controls are one now"
        );
    }

    // The paint is a straight assignment from the SNAPSHOT's fields: this edge
    // reads the mode, its accessible description and the spinner's visibility,
    // and computes none of them.
    let apply = between(
        &chrome,
        "pub fn apply(&self, paint: &ChromePaint) {",
        "\n    }\n",
    );
    for assignment in [
        "set_text(self.reload_stop, paint.reload_stop_label)",
        "self.set_tip(self.reload_stop, paint.reload_stop_description)",
        "show(self.spinner, paint.spinner_visible)",
    ] {
        assert!(
            apply.contains(assignment),
            "the paint must assign `{assignment}` from the shared snapshot: {apply:?}"
        );
    }
    // …and the pre-collapse rule -- enable one of a pair on the raw loading fact
    // -- is GONE, along with any other use of that fact in the paint. Keeping it
    // beside the derived value is the drift the one-derivation rule forbids.
    for gone in ["enable(self.stop", "enable(self.reload", "paint.is_loading"] {
        assert!(
            !apply.contains(gone),
            "`{gone}` is this edge deciding the control's mode for itself: {apply:?}"
        );
    }

    // The spinner's SLOT is laid out from the DPI seam like every other
    // rectangle, and permanently: only its VISIBILITY follows the derivation, so
    // a load starting can never shove the URL bar sideways.
    let layout = between(&chrome, "pub fn relayout(&self) {", "\n    }\n");
    assert!(
        layout.contains("self.reload_stop") && layout.contains("metrics.spinner_width"),
        "the collapsed control and the spinner must be laid out from the seam: {layout:?}"
    );

    // Win32 has no spinner control, so this edge ANIMATES one -- on the chrome
    // pump that already exists. A second timer is what the Android ANR guard and
    // this crate's one-pump rule forbid.
    let window = window();
    assert!(
        window.contains("self.chrome.spin();"),
        "the spinner must be advanced from the existing pump tick"
    );
    assert_eq!(
        window.matches("SetTimer(").count(),
        1,
        "the spinner must not add a timer of its own"
    );
    assert!(
        exists("docs/spikes/reload-stop-collapse-and-spinner-on-the-windows-chrome/DECISIONS.md"),
        "how a Win32 control renders a spinner, and where it sits, is a recorded decision"
    );
}

#[test]
fn the_collapsed_control_performs_the_modes_own_action_and_history_is_untouched() {
    // What the ONE control DOES is the MODE's own `ChromeAction` -- the same
    // closed vocabulary Ctrl+R and Escape resolve into -- performed by the SAME
    // performer, so the toolbar cancel and the keyboard cancel are one path.
    let window = window();
    assert!(
        window.contains("const ID_RELOAD_STOP: usize"),
        "the collapsed control needs ONE command id"
    );
    for gone in ["const ID_RELOAD:", "const ID_STOP:"] {
        assert!(
            !window.contains(gone),
            "`{gone}` is the pre-collapse pair's command id"
        );
    }
    let command = between(&window, "fn handle_command(", "/// PERFORM a resolved");
    assert!(
        command.contains("ID_RELOAD_STOP =>") && command.contains("perform_chrome_action("),
        "a click must go through the shared performer: {command:?}"
    );
    for decided in ["shell.borrow_mut().stop()", "shell.borrow_mut().reload()"] {
        assert!(
            !command.contains(decided),
            "`{decided}` is this handler deciding which of the two modes the click \
             is: {command:?}"
        );
    }
    // And the MODE it performs is read off the shared snapshot, never recomputed.
    let action = between(&window, "fn reload_stop_action(", "\n    }\n");
    assert!(
        action.contains("ChromePaint::of(") && action.contains("reload_stop_control.action()"),
        "the click's action must be the snapshot's own mode: {action:?}"
    );

    // Back and forward are UNTOUCHED: desktop keeps its history buttons (spec
    // story 14; only the mobile edges drop them, in their own tasks).
    let chrome = chrome();
    assert!(
        chrome.contains("pub back: HWND") && chrome.contains("pub forward: HWND"),
        "the desktop toolbar keeps back and forward"
    );
    assert!(
        chrome.contains("enable(self.back, paint.can_go_back)")
            && chrome.contains("enable(self.forward, paint.can_go_forward)"),
        "the history buttons still read the core's capability flags"
    );

    // The interactive half belongs on the `windows-latest` leg: only a real
    // window can show that the real control re-labels itself and really cancels.
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    for driven in [
        "window.reload_stop_label()",
        "window.reload_stop_description()",
        "window.spinner_visible()",
        "activate_reload_stop",
        "werust_windows::shortcuts::VK_ESCAPE",
    ] {
        assert!(
            smoke.contains(driven),
            "the smoke must drive `{driven}` against the real window"
        );
    }
}

#[test]
fn the_shell_passes_a_durable_profile_not_the_engines_temp_default() {
    // Criterion 5 (planted on this task at Gate 3 of the engine task): the engine
    // defaults its WebView2 user-data folder to `%TEMP%\werust-webview2`, which is
    // right for a CI-only engine and WRONG for a browser -- a temp profile loses
    // cookies, storage and cache. The SHELL must pass its own durable path.
    let window = window();
    assert!(
        window.contains("crate::profile::user_data_folder()")
            && window.contains("Webview2Renderer::with_user_data_folder(folder)"),
        "the shell must pass its own durable user-data folder"
    );
    let profile = source("crates/werust-windows/src/profile.rs");
    assert!(
        profile.contains("pub const LOCAL_APP_DATA_ENV: &str = \"LOCALAPPDATA\""),
        "the durable profile must live under %LOCALAPPDATA%"
    );
    let win32 = code_only(&win32_half());
    for temp in ["temp_dir()", "default_user_data_folder()"] {
        assert!(
            !win32.contains(temp),
            "the window must not fall back to the engine's development `{temp}` profile"
        );
    }
    // And the smoke MEASURES it rather than asserting it in prose.
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    assert!(
        smoke.contains("werust_windows::profile::user_data_folder()")
            && smoke.contains("!folder.starts_with(std::env::temp_dir())"),
        "the smoke must check the real profile folder on the real machine"
    );
}

#[test]
fn the_ci_leg_builds_tests_and_runs_the_window() {
    // Criterion 6: the new surface is actually EXERCISED on Windows, not merely
    // added to the repo. The existing `windows-latest` leg must build it, test it,
    // run the window smoke, and re-run when this crate changes.
    let workflow = source(".github/workflows/windows-renderer.yml");
    assert!(
        workflow.contains("-p werust-windows"),
        "the Windows job must build the window crate"
    );
    assert!(
        workflow.contains("--example window_smoke"),
        "the Windows job must RUN the window smoke, not just compile it"
    );
    assert!(
        workflow.contains("crates/werust-windows/**"),
        "the job's path filters must include the window crate"
    );
    assert!(
        workflow.contains("crates/desktop-paint/**"),
        "and the shared painter half it now depends on"
    );
    // The smoke must assert on the real WIDGETS (the only thing a Windows runner
    // adds), and must be able to FAIL.
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    for widget in [
        "window.url_text()",
        "window.trust_text()",
        "window.trust_detail()",
        "window.status_text()",
        "window.error_banner()",
        "window.menu_titles()",
        "window.debug_row_counts()",
        "window.page_rect()",
    ] {
        assert!(
            smoke.contains(widget),
            "the smoke must read back what the real widget `{widget}` holds"
        );
    }
    assert!(
        smoke.contains("NEGATIVE CONTROL") || smoke.contains("negative control"),
        "the smoke must carry a negative control, or a pass means nothing"
    );
    assert!(
        smoke.contains("window_smoke: FAIL"),
        "the smoke must be able to report failure"
    );
    // Offline: a smoke that needs a gateway is not a CI smoke.
    for networked in ["https://", "http://"] {
        assert!(
            !smoke.contains(&format!("navigate(\"{networked}")),
            "the smoke must stay offline"
        );
    }
}

#[test]
fn the_binary_links_as_a_gui_app_and_a_startup_failure_stays_legible() {
    // The amendment above: `werust-windows.exe` must not bring a console window
    // with it. The attribute is the ONLY thing that decides that, and it is
    // invisible to every runtime test, so it is pinned by reading the source.
    let main = source("crates/werust-windows/src/main.rs");
    assert!(
        main.contains("#![cfg_attr(windows, windows_subsystem = \"windows\")]"),
        "the binary must link as a GUI app, or Windows allocates a console beside the window"
    );
    // `cfg`-gated, and the non-Windows arm that keeps this crate in the Ubuntu
    // gate is untouched: it still COMPILES everywhere and still refuses LOUDLY.
    assert!(
        !main.contains("#![windows_subsystem"),
        "the attribute must be cfg-gated, so nothing about the non-Windows build changes"
    );
    assert!(
        main.contains("#[cfg(not(windows))]") && main.contains("only runs on Windows"),
        "the non-Windows refusal arm must keep working"
    );

    // The other half of the change, and the risky one: under the windows
    // subsystem there is no console, so a deleted message is a window that never
    // appears with NO explanation -- and this shell has a pre-specified honest
    // failure to report (no WebView2 Runtime). Both surfaces must be present.
    assert!(
        main.contains("eprintln!(\"werust: {e}\")"),
        "the failure text must not be deleted: a terminal launch still prints it"
    );
    assert!(
        main.contains("startup::attach_parent_console()")
            && main.contains("startup::report_startup_failure("),
        "a startup failure must reach the user on whichever surface launched werust"
    );
    let startup = source("crates/werust-windows/src/startup.rs");
    assert!(
        startup.contains("AttachConsole(ATTACH_PARENT_PROCESS)"),
        "a terminal-launched run must borrow the terminal's console rather than spawn one"
    );
    assert!(
        startup.contains("MessageBoxW("),
        "a double-clicked run has no console, so its failure must take a message box"
    );
    // No console is ever CREATED: `AllocConsole` would re-open the very window
    // this task closed.
    let win32 = code_only(&win32_half());
    assert!(
        !win32.contains("AllocConsole"),
        "werust must never allocate a console of its own"
    );
    assert!(
        exists("docs/spikes/windows-gui-subsystem-no-console-window/DECISIONS.md"),
        "how a startup failure is surfaced is a recorded decision, not a silent one"
    );
}

#[test]
fn the_chrome_scales_from_one_dpi_seam_and_follows_a_dpi_change() {
    // The defect this test exists for (task `windows-chrome-must-scale-with-the-display-dpi`,
    // defect 1 of `work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`):
    // `app.manifest` declares `PerMonitorV2`, which PROMISES Windows that this
    // process scales ITSELF, and the Win32 half laid every rectangle out in raw
    // 96-DPI pixels -- so on a 150%/200% display the chrome drew at 66%/50% of
    // its intended size while WebView2 drew the page correctly. A CI runner has
    // no DPI and cannot see any of that; what it CAN pin, and what this asserts,
    // is that every metric comes from ONE seam whose arithmetic the Ubuntu gate
    // unit-tests, and that a DPI CHANGE is handled.

    // The promise itself stays: reverting the manifest would restore the SIZE by
    // making Windows bitmap-scale the process, at the cost of a blurry chrome AND
    // a blurry page.
    let manifest = source("crates/werust-windows/app.manifest");
    assert!(
        manifest.contains("PerMonitorV2"),
        "the manifest must keep promising per-monitor-v2 awareness; the WINDOW is the half that \
         has to keep the promise"
    );

    // The seam is HOST-INDEPENDENT, so its arithmetic compiles and is tested on
    // the Ubuntu gate rather than only on a Windows box -- the same shape
    // `profile.rs` has, and the reason this crate is a workspace member at all.
    let lib = source("crates/werust-windows/src/lib.rs");
    assert!(
        lib.contains("pub mod dpi;"),
        "the DPI seam must be a module of this crate"
    );
    assert!(
        !lib.contains("#[cfg(windows)]\npub mod dpi;"),
        "the DPI seam must NOT be cfg-gated: the Ubuntu gate is where its arithmetic is tested"
    );
    let seam = dpi_seam();
    assert!(
        seam.contains("#[cfg(test)]"),
        "the scaling arithmetic must carry its own unit tests on the Ubuntu gate"
    );

    // The design metrics live ONCE, at the seam, in their 96-DPI form. A copy
    // left behind in the Win32 half is a metric that silently stops scaling.
    for (name, value) in [
        ("TOOLBAR_HEIGHT", 40),
        ("BANNER_HEIGHT", 44),
        ("STATUS_HEIGHT", 22),
        ("MARGIN", 8),
        ("BUTTON_WIDTH", 36),
        ("TRUST_WIDTH", 210),
        ("BADGE_WIDTH", 110),
        ("PROGRESS_HEIGHT", 3),
        ("SPINNER_WIDTH", 20),
        ("FONT_HEIGHT", 15),
        ("DEFAULT_WIDTH", 1024),
        ("DEFAULT_HEIGHT", 768),
        ("DEBUG_WIDTH", 940),
        ("DEBUG_HEIGHT", 480),
    ] {
        assert!(
            seam.contains(&format!("pub const {name}: i32 = {value};")),
            "`{name}` must be a 96-DPI design metric at the seam"
        );
    }
    for source_file in [
        "crates/werust-windows/src/chrome.rs",
        "crates/werust-windows/src/window.rs",
    ] {
        let text = code_only(&source(source_file));
        assert!(
            !text.contains("const MARGIN") && !text.contains("const DEFAULT_WIDTH"),
            "{source_file} must consume the seam's metrics, not restate them"
        );
    }

    // The DPI itself is read ONCE, per window, through the platform call the
    // task names -- `GetDpiForWindow`, which is per-MONITOR, not the process's
    // system DPI.
    let helpers = source("crates/werust-windows/src/win32.rs");
    assert!(
        helpers.contains("GetDpiForWindow("),
        "the window's scale must come from GetDpiForWindow (per-monitor), through one helper"
    );
    assert_eq!(
        code_only(&win32_half()).matches("GetDpiForWindow(").count(),
        1,
        "exactly ONE call site: a second GetDpiForWindow is a second seam"
    );

    // The LAYOUT: every rectangle is the seam's, and no raw pixel survives.
    let chrome = chrome();
    let layout = between(&chrome, "pub fn relayout(&self) {", "\n    }\n");
    assert!(
        layout.contains("let metrics = self.metrics();"),
        "the layout must be computed from the DPI seam: {layout:?}"
    );
    for field in [
        "metrics.toolbar_height",
        "metrics.banner_height",
        "metrics.status_height",
        "metrics.margin",
        "metrics.button_width",
        "metrics.trust_width",
        "metrics.badge_width",
        "metrics.progress_height",
        "metrics.row_height",
    ] {
        assert!(
            layout.contains(field),
            "the layout must take `{field}` from the seam: {layout:?}"
        );
    }
    // `0` (an origin) and `2` (the two-margin multiplier) are not pixels; every
    // other literal in a layout is one, and must have gone through the seam.
    let leftovers: Vec<String> = unscaled_literals(layout)
        .into_iter()
        .filter(|literal| literal != "0" && literal != "2")
        .collect();
    assert!(
        leftovers.is_empty(),
        "the chrome layout still carries raw 96-DPI pixels {leftovers:?}: every metric must go \
         through the seam"
    );
    let window = window();
    let debug_layout = between(
        &window,
        "fn relayout_debug_window(",
        "\n/// Register a window class",
    );
    let leftovers: Vec<String> = unscaled_literals(debug_layout)
        .into_iter()
        .filter(|literal| literal != "0" && literal != "2")
        .collect();
    assert!(
        leftovers.is_empty(),
        "the debug view's layout still carries raw 96-DPI pixels {leftovers:?}"
    );

    // The FONT: `CreateFontW` fixes a height at creation, so the seam has to be
    // asked for it -- and the old `-15` may not survive anywhere.
    assert!(
        helpers.contains("pub fn ui_font(height: i32) -> HFONT"),
        "the UI font must be created at the height the seam computed"
    );
    let win32 = code_only(&win32_half());
    assert!(
        !win32.contains("-15"),
        "the hard-coded 96-DPI font height must be gone"
    );
    assert!(
        window.contains("ui_font(metrics.font_height)"),
        "the window must create its font at the scaled height"
    );

    // The INITIAL window size: a 200% display must not open a half-size window.
    assert!(
        window.contains("metrics.default_width") && window.contains("metrics.default_height"),
        "the initial window size must be DPI-scaled"
    );
    assert!(
        window.contains("metrics.debug_width") && window.contains("metrics.debug_height"),
        "the debug view's initial size must be DPI-scaled too"
    );

    // A DPI CHANGE: dragging between monitors of different scale. The suggested
    // rect Windows sends is honoured, the font is recreated and pushed to every
    // control, the OLD font is deleted (the crate's brush-cleanup pattern), and
    // the layout re-runs.
    assert!(
        window.contains("WM_DPICHANGED") && window.contains("fn dpi_changed"),
        "the window must handle WM_DPICHANGED, or it is correct only on the monitor it opened on"
    );
    let changed = between(&window, "fn dpi_changed(", "\n    }\n");
    assert!(
        changed.contains("SetWindowPos("),
        "the suggested rect Windows sends must be honoured: {changed:?}"
    );
    assert!(
        changed.contains("self.rescale_font(") && changed.contains("relayout()"),
        "a DPI change must recreate the font and re-run the layout: {changed:?}"
    );
    let rescale = between(&window, "fn rescale_font(", "\n    }\n");
    assert!(
        rescale.contains("ui_font(") && rescale.contains("set_font("),
        "the new font must be pushed to every control with WM_SETFONT: {rescale:?}"
    );
    assert!(
        rescale.contains("release_font("),
        "the OLD HFONT must be deleted rather than leaked on every DPI change: {rescale:?}"
    );
    assert!(
        helpers.contains("pub fn release_font(") && helpers.contains("DeleteObject(font.into())"),
        "the font's cleanup must follow the same DeleteObject path the theme's brushes use"
    );
    // Every control the window owns takes the new font: a missed one keeps the
    // old size and is the visible half of this defect.
    assert!(
        chrome.contains("pub fn controls(&self)"),
        "the chrome must name every control it owns, so a font push cannot miss one"
    );

    // The window SMOKE measures the real widgets against the seam, which is the
    // only run-time check available anywhere (and, on a 96-DPI runner, still
    // proves the layout is COMPUTED from the seam rather than from constants).
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    assert!(
        smoke.contains("window.dpi()") && smoke.contains("Metrics::at("),
        "the smoke must compare the real layout against the seam's metrics for the runner's DPI"
    );
    for measured in [
        "window.page_client_rect()",
        "window.control_rect(",
        "metrics.toolbar_height",
        "metrics.row_height",
        "metrics.trust_width",
    ] {
        assert!(
            smoke.contains(measured),
            "the smoke must measure `{measured}` off the real window"
        );
    }

    // And the honesty: a runner cannot close this one, so the manual steps are
    // written down at the task's stable spike path.
    let readme = source("docs/spikes/windows-chrome-must-scale-with-the-display-dpi/README.md");
    for step in ["100%", "150%", "200%", "cross-monitor drag"] {
        assert!(
            readme.contains(step),
            "the spike README must record the manual step `{step}`"
        );
    }
    assert!(
        readme.contains("CI cannot"),
        "the README must say plainly that CI cannot verify this"
    );
}

#[test]
fn the_verification_honesty_is_recorded() {
    // Criterion 6 (ADR-0011 Amendment 1): the visible behaviour cannot be checked
    // from the development machine, so the manual steps AND the
    // proved-versus-awaiting split are written down at the task's stable spike
    // path.
    let readme = source("docs/spikes/windows-win32-window-and-chrome/README.md");
    for section in [
        "What CI proved",
        "What still awaits real Windows hardware",
        "Manual verification",
    ] {
        assert!(
            readme.contains(section),
            "the spike README must state `{section}`"
        );
    }
    assert!(
        exists("docs/spikes/windows-win32-window-and-chrome/DECISIONS.md"),
        "the judgement calls this task made must be recorded beside it"
    );
    // The local type-check harness the engine task left behind must cover this
    // window too (it is the fast loop that keeps CI from being the first place a
    // typo is found).
    let harness =
        source("docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh");
    assert!(
        harness.contains("werust-windows"),
        "the local Windows type-check harness must cover the window crate too"
    );
}

#[test]
fn the_win32_edge_translates_into_the_shared_resolution_and_decides_nothing() {
    // The heart of the shortcut half: this edge's whole key path is translate ->
    // ask the core -> perform. The TRANSLATION is a pure module (so the Ubuntu
    // gate tests it), it calls the SHARED resolution, and it takes the
    // accelerator convention from the core rather than restating "Windows is a
    // Ctrl platform".
    let translation = shortcut_translation();
    assert!(
        translation.contains("shortcuts::resolve_chord(")
            && translation.contains("shortcuts::PrimaryModifier::for_target()"),
        "the edge must ask the SHARED resolution what a chord means, on the core's own convention"
    );
    // Pure by construction: the module may not reach for the `windows` crate at
    // all, which is what keeps it inside the Ubuntu gate.
    assert!(
        !code_only(&translation).contains("windows::"),
        "the translation must stay host-independent: it is the half the Ubuntu gate tests"
    );
    let lib = source("crates/werust-windows/src/lib.rs");
    assert!(
        lib.contains("pub mod shortcuts;") && !lib.contains("#[cfg(windows)]\npub mod shortcuts;"),
        "the shortcut translation must NOT be cfg-gated, like the DPI and profile seams"
    );
    assert!(
        translation.contains("#[cfg(test)]"),
        "the translation must carry its own unit tests on the Ubuntu gate"
    );
    // The virtual-key codes are spelled here as plain `u16` (the price of being
    // pure), so the Win32 half must pin them against the SDK's own `VK_*`.
    let window = window();
    let pinned = between(
        &window,
        "const _VIRTUAL_KEY_CODES_MATCH_THE_SDK: () = {",
        "\n};",
    );
    for code in [
        "VK_SHIFT",
        "VK_CONTROL",
        "VK_MENU",
        "VK_ESCAPE",
        "VK_LEFT",
        "VK_RIGHT",
        "VK_LWIN",
        "VK_RWIN",
        "VK_F5",
        "VK_F12",
    ] {
        assert!(
            pinned.contains(&format!("crate::shortcuts::{code} == {code}.0")),
            "`{code}` must be checked against the SDK's own value at compile time: {pinned:?}"
        );
    }
}

#[test]
fn the_edge_names_no_key_meaning_outside_its_translation() {
    // The teeth: the ONLY place this edge may name a specific KEY is its
    // translation function. A `Key::Escape` in the window would be this edge
    // deciding what Escape means, which is precisely the per-edge drift the
    // shared resolution exists to prevent.
    let translation = shortcut_translation();
    // The PRODUCTION half only, and cut BEFORE `code_only`: that helper drops
    // `#`-led lines (it is applied to manifests too), which would eat the
    // `#[cfg(test)]` marker itself.
    let production = code_only(
        translation
            .split("#[cfg(test)]")
            .next()
            .expect("the translation must have a production half"),
    );
    let translated = between(
        &translation,
        "pub fn shortcut_key(",
        "/// Translate the keyboard",
    );
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
            translated.matches(key).count(),
            "`{key}` may only be named while TRANSLATING, never where the edge acts on it"
        );
    }
    // And the Win32 half names no key at all: it hands the virtual-key code
    // straight to the translation.
    let win32 = code_only(&win32_half());
    assert!(
        !win32.contains("shortcuts::Key::"),
        "the Win32 half must not name a key: it only carries the code it was given"
    );
    // …outside the compile-time cross-check, which is the ONE place the SDK's
    // own `VK_*` may be named (it compares them with the pure module's, it does
    // not act on a key).
    let acting = win32.replace(
        between(
            &win32,
            "const _VIRTUAL_KEY_CODES_MATCH_THE_SDK: () = {",
            "\n};",
        ),
        "",
    );
    for claimed in ["VK_ESCAPE.0", "VK_F5.0", "VK_LEFT.0", "VK_RIGHT.0"] {
        assert!(
            !acting.contains(&format!("== {claimed}")),
            "`{claimed}` must not be branched on in the Win32 half: that is a second shortcut table"
        );
    }
    // The old edge-local F12 branch is GONE, folded into the shared table, and
    // the inspector is now opened by the RESOLVED action.
    assert!(
        !win32.contains("ID_DEV_TOOLS"),
        "the URL bar's own F12 command must be gone: F12 is a row in the shared table now"
    );
}

#[test]
fn the_edge_handles_every_action_the_shared_vocabulary_defines() {
    // One performer, with an arm for every action the core can resolve. Driven
    // off `ChromeAction::ALL` rather than a hand-copied list, so an action added
    // to the shared vocabulary reds here until this edge handles it.
    let window = window();
    let performer = between(
        &window,
        "fn perform_chrome_action(",
        "/// Post a resolved action",
    );
    for action in werust_core::shortcuts::ChromeAction::ALL {
        assert!(
            performer.contains(&format!("ChromeAction::{action:?}")),
            "the Win32 edge must handle {action:?}: {performer:?}"
        );
    }
    // Focus is REPORTED, as an input to the resolution, and answered with ONE
    // question rather than by classifying the widget tree.
    let focus = between(&window, "fn focus_context(&self) -> Focus {", "\n    }\n");
    assert!(
        focus.contains("GetFocus()")
            && focus.contains("Focus::UrlBar")
            && focus.contains("Focus::Page"),
        "the edge must REPORT which of the two focus contexts is live: {focus:?}"
    );
    assert!(
        window.contains("controller.focus_context()"),
        "the reported focus must be handed to the resolution, not branched on at the edge"
    );
    // The web inspector is still the PLATFORM's own devtools, reached only when
    // the core resolves that action.
    assert!(
        performer.contains("dev_tools.open()"),
        "the inspector action must open Edge's own DevTools: {performer:?}"
    );
}

#[test]
fn history_rides_the_existing_seam_and_its_capability_flags() {
    // A shortcut performs history EXACTLY as the toolbar button does: through
    // `BrowserShell::go_back` / `go_forward` (the existing `Renderer` seam
    // methods) gated on the existing `ChromeState` flags, so a chord or a side
    // button can never drive a move the on-screen control refuses.
    let window = window();
    let performer = between(
        &window,
        "fn perform_chrome_action(",
        "/// Post a resolved action",
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
    // The seam itself is UNCHANGED by this edge: no shortcut vocabulary leaked
    // into it.
    let seam = source("crates/renderer/src/lib.rs");
    assert!(
        seam.contains("fn go_back(&mut self)") && seam.contains("fn go_forward(&mut self)"),
        "the Renderer seam's history methods must be unchanged"
    );
    assert!(
        !seam.contains("shortcut") && !seam.contains("Chord"),
        "the shortcut layer must not have leaked into the Renderer seam"
    );
}

#[test]
fn the_side_buttons_ride_the_same_resolution_and_the_same_performer() {
    // Mouse buttons 4 and 5 navigate history, through the SAME resolution and the
    // SAME performer the keyboard uses. The edge knows only which BUTTON the
    // message named -- and it has two messages to read it from, because a click
    // over the page lands on a window this process does not own.
    let window = window();
    assert!(
        window.contains("WM_XBUTTONDOWN") && window.contains("shortcut_pointer_button(wparam.0)"),
        "a side-button click over the chrome must be translated, not interpreted"
    );
    assert!(
        window.contains("WM_APPCOMMAND") && window.contains("app_command_pointer_button(lparam.0)"),
        "a side-button click over a CHILD window arrives as an app command and must be honoured"
    );
    assert!(
        window.contains("WM_XBUTTONUP => LRESULT(1)"),
        "the release must be swallowed, or DefWindowProc turns one click into a second navigation"
    );
    let pointer = between(&window, "fn perform_pointer_button(", "\n}\n");
    assert!(
        pointer.contains("shortcuts::resolve_pointer_button")
            && pointer.contains("perform_chrome_action(controller, action)"),
        "the mouse path must ask the core, then perform: {pointer:?}"
    );
    assert!(
        !pointer.contains("go_back()") && !pointer.contains("go_forward()"),
        "the mouse path must not decide that a button means history: {pointer:?}"
    );
}

#[test]
fn the_page_focused_keys_arrive_through_the_engines_accelerator_hook() {
    // The platform fact this edge is shaped by: WebView2 hosts the page in ANOTHER
    // PROCESS, so a key pressed over the page never reaches this thread's message
    // loop and `add_AcceleratorKeyPressed` is the documented way to see it. The
    // hook carries a virtual-key code and a yes/no -- no chord vocabulary crosses
    // into the engine.
    let engine = source("crates/windows-renderer/src/backend.rs");
    assert!(
        engine.contains("add_AcceleratorKeyPressed("),
        "the engine must forward the keys the page swallows"
    );
    let hook = between(
        &engine,
        "fn wire_accelerator_keys(",
        "/// Install the document-start scripts",
    );
    assert!(
        hook.contains("COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN")
            && hook.contains("COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN"),
        "an Alt chord arrives as a SYSTEM key down, so both kinds must be forwarded: {hook:?}"
    );
    assert!(
        hook.contains("args.SetHandled(true)"),
        "a claimed key must be marked handled, or WebView2's own accelerator acts too: {hook:?}"
    );
    assert!(
        !code_only(&engine).contains("shortcuts::") && !code_only(&engine).contains("ChromeAction"),
        "the engine must learn nothing about chords: it carries a virtual-key code"
    );

    // And the WINDOW answers it with the same translation, reporting the page as
    // the focus (which is what the event means) and POSTING the action rather
    // than performing it inside a callback that blocks the browser process.
    let window = window();
    let claim = between(&window, "fn claim_accelerator_key(", "\n}\n");
    assert!(
        claim.contains("shortcut_action(") && claim.contains("Focus::Page"),
        "the page-focused path must use the same translation: {claim:?}"
    );
    assert!(
        claim.contains("post_chrome_action("),
        "a claimed key must be POSTED: the callback runs with the browser process blocked: {claim:?}"
    );
    assert!(
        window.contains("WM_WERUST_CHROME_ACTION"),
        "the posted action needs a message of its own"
    );
    assert!(
        window.contains("ChromeAction::ALL"),
        "the posted action must be carried as its slot in the CORE's action list"
    );

    // The chrome-focused half: a message-loop pre-filter, this edge's equivalent
    // of the GTK shell's capture phase. Anything unclaimed is dispatched
    // untouched.
    let filter = between(&window, "fn filter_shortcut(", "\n}\n");
    assert!(
        filter.contains("WM_KEYDOWN") && filter.contains("WM_SYSKEYDOWN"),
        "an Alt chord arrives as WM_SYSKEYDOWN and must be filtered too: {filter:?}"
    );
    assert!(
        filter.contains("GetKeyState") && filter.contains("controller.focus_context()"),
        "the filter must translate the modifier state and report focus: {filter:?}"
    );
    assert!(
        window.contains("if self.filter_shortcut(&message) {")
            && window.contains("if window.filter_shortcut(&message) {"),
        "both message loops (the smoke's pump and the product's) must give the layer first look"
    );
}

#[test]
fn the_shortcut_paths_are_exercised_on_the_windows_leg() {
    // The interactive half belongs on the `windows-latest` leg, because it is the
    // only place a real window pumps real messages. The smoke posts the messages
    // Windows itself posts and reads the result off the shell and the widgets.
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    for driven in [
        "post_key(&window, werust_windows::shortcuts::VK_F5)",
        "window.claim_accelerator_key(werust_windows::shortcuts::VK_F5)",
        "post_x_button(&window, werust_windows::shortcuts::XBUTTON1)",
    ] {
        assert!(
            smoke.contains(driven),
            "the smoke must drive `{driven}` against the real window"
        );
    }
    assert!(
        smoke.contains("!window.claim_accelerator_key(0x09)"),
        "the smoke must also check that an UNCLAIMED key is left to the page"
    );
    // And the honesty: what a runner cannot press is written down at the task's
    // stable spike path.
    let readme =
        source("docs/spikes/shortcuts-and-mouse-history-buttons-on-the-windows-edge/README.md");
    for section in [
        "What CI proves",
        "What still awaits real Windows hardware",
        "Manual verification",
    ] {
        assert!(
            readme.contains(section),
            "the spike README must state `{section}`"
        );
    }
    assert!(
        exists("docs/spikes/shortcuts-and-mouse-history-buttons-on-the-windows-edge/DECISIONS.md"),
        "the judgement calls this task made must be recorded beside it"
    );
}

#[test]
fn the_mouse_back_check_establishes_the_history_it_asks_for() {
    // The amendment above. A back check is only evidence if there is somewhere to
    // go back TO, and the two ways this smoke can reach the section with an empty
    // session list are both silent: a FAILED load (the tampered control) commits
    // nothing, and a RELOAD (the F5 checks) replaces the current entry. So the
    // section must create its OWN two verified entries, whatever precedes it,
    // rather than inheriting a history from the checks above.
    let smoke = source("crates/werust-windows/examples/window_smoke.rs");
    let section = between(&smoke, "the mouse's back side button", "// The COLLAPSE");
    assert!(
        section.matches("load_and_settle(").count() >= 2,
        "the section must perform TWO successful loads of its own before it asks whether \
         there is history: {section:?}"
    );
    assert!(
        section.contains("second_cid") && section.contains("honest_cid"),
        "the two loads must be DIFFERENT pages, or `back` has nothing to move between \
         and the URL bar cannot tell a move from a no-op: {section:?}"
    );
    // And the check itself is NOT weakened: the precondition is still asserted (a
    // section that quietly tolerated `can_go_back == false` would go green while
    // proving nothing), and the side button is still what drives the move.
    assert!(
        section.contains("chrome().can_go_back")
            && section.contains("post_x_button(&window, werust_windows::shortcuts::XBUTTON1)"),
        "the section must still assert the precondition and still drive the real button: \
         {section:?}"
    );
    // The FAILURE path is BOUNDED. When the action correctly refuses (which is
    // what a regression here looks like: the product declining a history move the
    // Back button would decline too), the wait can only end by timing out, so the
    // budget is what a regression COSTS in CI.
    let waited = between(section, "post_x_button(", "\"mouse button 4");
    let seconds: u32 = waited
        .split("wait_until(&window, ")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|seconds| seconds.trim().parse().ok())
        .unwrap_or_else(|| {
            panic!("the back wait must be a `wait_until(&window, <seconds>, …)`: {waited:?}")
        });
    assert!(
        seconds <= 10,
        "a back navigation to an already-fetched, in-memory page lands in well under a \
         second, so a {seconds}-second budget only buys a SLOW failure"
    );
    // The confirmed diagnosis and the judgement calls are recorded at the task's
    // stable spike path, beside the sibling edges' own.
    assert!(
        exists("docs/spikes/windows-smoke-mouse-back-check-runs-after-a-failed-load/README.md"),
        "the diagnosis this fix rests on must be written down where the next reader looks"
    );
}
