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

/// The whole Win32 half, as one text: the three modules the Ubuntu gate cannot
/// compile. Asserting over the set (rather than over one file) means a rule that
/// leaks into a neighbouring module is still caught.
fn win32_half() -> String {
    [
        "crates/werust-windows/src/window.rs",
        "crates/werust-windows/src/chrome.rs",
        "crates/werust-windows/src/debugview.rs",
        "crates/werust-windows/src/win32.rs",
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
        ("Reload", "pub reload: HWND"),
        ("Stop", "pub stop: HWND"),
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

    // Every control drives the SHARED shell, never the webview.
    for (action, drives) in [
        ("ID_BACK", "shell.borrow_mut().go_back()"),
        ("ID_FORWARD", "shell.borrow_mut().go_forward()"),
        ("ID_RELOAD", "shell.borrow_mut().reload()"),
        ("ID_STOP", "shell.borrow_mut().stop()"),
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
        layout.contains("let page_top = TOOLBAR_HEIGHT + banner_height;"),
        "only the error banner may change the page's geometry: {layout:?}"
    );
    let banner_height = between(
        &chrome,
        "let banner_height = if self.banner_visible.get()",
        ";",
    );
    assert!(
        banner_height.contains("BANNER_HEIGHT"),
        "the banner takes its strip only when it is visible"
    );
    // And the banner is visible only when the core says a load FAILED.
    assert!(
        paint().contains("error_visible: error_banner_visible(state)"),
        "the banner's visibility is the core's failure rule, not a Windows choice"
    );
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
