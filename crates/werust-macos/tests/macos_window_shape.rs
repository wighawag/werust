//! macOS WINDOW shape guard (task `macos-appkit-window-and-chrome`,
//! `docs/adr/0011-webview2-for-windows.md`'s macOS split, sub-task 3).
//!
//! WHY A SOURCE-SHAPE GUARD: the window's AppKit half is
//! `#[cfg(target_os = "macos")]`, and this repo's `verify` gate runs on Ubuntu
//! with no Xcode and no SDK, so `cargo build` NEVER compiles it. That is the same
//! position `crates/macos-renderer` is in, and the repo's answer is the same: a
//! plain `cargo test` that PARSES the source it cannot compile and asserts the
//! properties compilation would not have proven anyway --
//!
//! * that every chrome surface exists at all,
//! * that the AppKit layer PAINTS the shared derivation instead of re-deriving
//!   it (the failure that already cost this project the Kotlin and Swift twins),
//! * that the ⋮ menu is the core's `BrowserMenu` rather than a macOS list,
//! * that ADR-0009 / ADR-0010 / the URL-bar-progress rule are FOLLOWED, not
//!   re-decided, and
//! * that the CI leg really exercises the new surface.
//!
//! Everything that is a display RULE lives in `werust-core` and is unit-tested
//! there; everything that ASSEMBLES a display value lives in `src/paint.rs` and
//! is unit-tested against the real core by this crate's own `cargo test`. This
//! guard covers the wiring between them.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. Every surface is present over the WKWebView backend
//!    (`the_window_carries_every_chrome_surface_over_the_wkwebview_backend`).
//! 2. Every surface reads the SHARED derivation; nothing is re-derived here
//!    (`the_appkit_layer_paints_and_never_derives`,
//!    `the_class_names_and_labels_are_never_restated_in_the_window`).
//! 3. The debug-view row helpers moved into the core and BOTH desktop views use
//!    them (`the_debug_row_rules_live_in_the_core_and_both_desktop_views_use_them`).
//! 4. The ⋮ menu comes from the core's `BrowserMenu`
//!    (`the_menu_is_the_cores_browser_menu_not_a_macos_list`).
//! 5. ADR-0009, ADR-0010 and the URL-bar-progress rule are honoured
//!    (`the_os_colour_scheme_is_followed_never_forced`,
//!    `new_windows_navigate_in_place_via_the_shared_rule`,
//!    `progress_lives_in_the_url_bar_and_only_a_failure_moves_the_page`).
//! 6. What CI proves versus what awaits a Mac is written down, and the CI leg
//!    exercises this crate
//!    (`the_ci_leg_builds_tests_and_runs_the_window`,
//!    `the_verification_honesty_is_recorded`).
//! 7. The gate stays green (this file is a plain `cargo test`).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-macos`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn exists(relative: &str) -> bool {
    repo_root().join(relative).exists()
}

fn window() -> String {
    source("crates/werust-macos/src/window.rs")
}

/// The window's host-independent half.
///
/// It landed HERE (`crates/werust-macos/src/paint.rs`) and was EXTRACTED to
/// `crates/desktop-paint` by task `windows-win32-window-and-chrome`, verbatim and
/// with its tests, so the Win32 window consumes the one carrier instead of
/// copying it (and the palette). `werust_macos::paint` still names it, so nothing
/// about this window changed; this guard follows it to where it now lives.
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
            !(trimmed.starts_with("//") || trimmed.starts_with("///"))
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
fn the_window_carries_every_chrome_surface_over_the_wkwebview_backend() {
    // Criterion 1: a real `NSWindow` with every surface the task names, hosting
    // the macOS backend's view through the SEAM (never a WKWebView of its own).
    let window = window();
    assert!(
        window.contains("NSWindow::initWithContentRect_styleMask_backing_defer"),
        "the shell must open a real NSWindow"
    );
    // The page view arrives through the seam's opaque handle, so this window has
    // no WebKit dependency at all.
    assert!(
        window.contains("shell.borrow().view_handle()")
            && window.contains("handle.0.cast::<NSView>()"),
        "the page view must be embedded through the seam's ViewHandle"
    );
    assert!(
        !code_only(&window).contains("WKWebView") && !window.contains("objc2_web_kit"),
        "the window must not reach past the seam to WebKit: that is the engine's job"
    );

    // Every surface, by the widget that carries it.
    for (surface, needle) in [
        ("the URL bar", "url_field: Retained<NSTextField>"),
        ("Back", "back: Retained<NSButton>"),
        ("Forward", "forward: Retained<NSButton>"),
        ("Reload", "reload: Retained<NSButton>"),
        ("Stop", "stop: Retained<NSButton>"),
        ("the trust indicator", "trust: Retained<NSTextField>"),
        (
            "the invalid-entry badge",
            "invalid_badge: Retained<NSTextField>",
        ),
        ("the ⋮ menu", "menu_button: Retained<NSButton>"),
        ("the error banner", "error_banner: Retained<NSTextField>"),
        (
            "the load-progress indicator",
            "progress: Retained<NSProgressIndicator>",
        ),
        ("the status line", "status: Retained<NSTextField>"),
    ] {
        assert!(
            window.contains(needle),
            "{surface} must be part of the window's chrome"
        );
    }

    // The tabbed debug view: a Console and a Network tab over the SHARED store.
    assert!(
        window.contains("NSTabView")
            && window.contains("\"Console\"")
            && window.contains("\"Network\""),
        "the debug view must be a tabbed Console + Network view"
    );
    assert!(
        window.contains("self.ivars().capture.clear()"),
        "the debug view's Clear must empty the SHARED capture store"
    );
    // READ-ONLY by construction: the debug view builds labels, never an editable
    // field. (`NSTextField::new` + `setEditable(true)` is the URL bar, and it is
    // built in `BrowserWindow::open`, not in the debug window.)
    let debug_view = between(&window, "impl DebugWindow {", "/// One CONSOLE row");
    for input in ["setEditable", "NSTextView", "NSSearchField"] {
        assert!(
            !debug_view.contains(input),
            "the debug view must be READ-ONLY, but it builds a `{input}`"
        );
    }

    // Every control drives the SHARED shell, never the webview.
    for (action, drives) in [
        ("goBack:", "shell.borrow_mut().go_back()"),
        ("goForward:", "shell.borrow_mut().go_forward()"),
        ("reloadPage:", "shell.borrow_mut().reload()"),
        ("stopLoading:", "shell.borrow_mut().stop()"),
        ("urlEntered:", "shell.borrow_mut().navigate("),
    ] {
        assert!(
            window.contains(action) && window.contains(drives),
            "`{action}` must drive the shared shell (`{drives}`)"
        );
    }

    // The chrome is pumped on ONE timer, the same 50ms cadence the GTK shell
    // uses, and the debug view rides it rather than adding a second one.
    assert!(
        window.contains("scheduledTimerWithTimeInterval_target_selector_userInfo_repeats"),
        "the chrome must be pumped on the run loop"
    );
    assert_eq!(
        window.matches("scheduledTimerWithTimeInterval").count(),
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
fn the_appkit_layer_paints_and_never_derives() {
    // Criterion 2, the heart of this task: the AppKit half assigns values that
    // `paint.rs` (and through it `werust-core`) decided. It must not call the
    // chrome derivation itself -- not because calling it would be wrong, but
    // because `paint.rs` is the half the Ubuntu gate can COMPILE and TEST, and
    // every rule that leaks past it into AppKit-land leaves the gate's reach.
    let window = code_only(&window());
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
        "console_row_text(",
        "network_trust_label(",
        "network_status_text(",
    ] {
        assert!(
            !window.contains(rule),
            "the AppKit layer must read `{rule}`'s result from `paint`, not call it \
             (that is the half the Ubuntu gate compiles and tests)"
        );
    }
    // And `paint.rs` must be the one that DOES call them, from the shared core.
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
            "`paint.rs` must derive `{rule}` from the shared core"
        );
    }
    assert!(
        paint.contains("use werust_core::{"),
        "`paint.rs` must consume the core's chrome derivation"
    );

    // The window paints a snapshot, field by field: it reads `ChromePaint`.
    assert!(
        window.contains("fn apply(&self, paint: &ChromePaint)"),
        "the window must paint a derived snapshot"
    );
}

#[test]
fn the_class_names_and_labels_are_never_restated_in_the_window() {
    // The other half of criterion 2: no state class name, badge label or banner
    // wording is written in the AppKit layer. Those are the strings that drifted
    // between the desktop, Kotlin and Swift copies; here they can only arrive
    // through `paint`.
    let window = code_only(&window());
    for restated in [
        "trust-loading",
        "trust-verified",
        "trust-name-trusted-rpc",
        "trust-mutable-name",
        "trust-unverified",
        "error-banner",
        "debug-console-",
        "verified",
        "unverified origin",
        "invalid URL",
        "failed to load",
        "loading…",
    ] {
        assert!(
            !window.contains(restated),
            "`{restated}` is restated in the AppKit layer; it must come from the core's derivation"
        );
    }
    // The palette (the one thing an edge legitimately owns) lives in `paint.rs`
    // and its coverage gate is driven by the core's aggregate over EVERY exported
    // family, never by a family list written out here: a hand-written list is
    // exhaustive over the CLASSES it names but not over the FAMILIES, so a sixth
    // family would join no gate and paint invisibly with a green suite (task
    // `one-derivation-close-the-aggregate-and-tooltip-gaps`).
    let paint = paint();
    assert!(
        paint.contains("pub const CLASS_COLORS"),
        "the edge's stylesheet belongs to the edge (now the SHARED native-widget \
         edge half, so macOS and Windows cannot disagree about a colour)"
    );
    let paint_code = code_only(&paint);
    let gate = between(
        &paint_code,
        "fn every_exported_class_has_a_colour()",
        "\n    }\n",
    );
    assert!(
        gate.contains("for family in CssClassFamily::ALL") && gate.contains("family.classes()"),
        "the no-unstyled-class guard must iterate the CORE's family aggregate: {gate:?}"
    );
    for hand_written in [
        "TRUST_INDICATOR_CSS_CLASSES",
        "ERROR_BANNER_CSS_CLASSES",
        "DEBUG_CONSOLE_CSS_CLASSES",
    ] {
        assert!(
            !gate.contains(hand_written),
            "`{hand_written}` is named in the palette gate; WHICH families are checked must come \
             from the core's aggregate"
        );
    }
}

#[test]
fn the_debug_row_rules_live_in_the_core_and_both_desktop_views_use_them() {
    // Criterion 3: the debug-view row helpers were EXTRACTED from the GTK edge
    // into `werust-core` (behaviour-preservingly, with their tests), and BOTH
    // desktop views now paint from that one derivation.
    let core = source("crates/werust-core/src/debug.rs");
    for rule in [
        "pub fn console_row_text(",
        "pub fn console_source_line(",
        "pub fn console_level_css_class(",
        "pub fn network_status_text(",
        "pub fn network_mime_text(",
        "pub fn network_size_text(",
        "pub fn network_trust_label(",
        "pub fn network_trust_css_class(",
        "pub fn tail_plan(",
        "pub const DEBUG_CONSOLE_CSS_CLASSES",
    ] {
        assert!(
            core.contains(rule),
            "the shared core must own the debug view's `{rule}`"
        );
    }

    // The GTK edge CONSUMES them and no longer defines them (a copy left behind
    // is exactly the drift this extraction exists to end).
    let gtk = source("crates/werust/src/main.rs");
    for defined in [
        "fn console_row_text(",
        "fn console_level_css_class(",
        "fn network_status_text(",
        "fn network_trust_label(",
        "fn tail_plan(",
    ] {
        assert!(
            !gtk.contains(defined),
            "the GTK edge must not keep its own `{defined}`: it moved to the core"
        );
    }
    assert!(
        gtk.contains("use werust_core::debug::{"),
        "the GTK edge must consume the core's row rules"
    );

    // The macOS view paints from the same ones (through the shared carrier).
    let paint = paint();
    for shared in [
        "console_row_text(entry)",
        "console_level_css_class(entry.level)",
        "network_status_text(entry.status)",
        "network_trust_label(entry.trust)",
        "network_trust_css_class(entry.trust)",
        "tail_plan(sequences",
    ] {
        assert!(
            paint.contains(shared),
            "the macOS debug view must paint from the core's `{shared}`"
        );
    }

    // The capture points that FEED it are the shared shims on the dedicated
    // capture channel, never a macOS-local console parser.
    assert!(
        paint.contains("console_shim()")
            && paint.contains("network_shim()")
            && paint.contains("route_capture_message")
            && paint.contains("CAPTURE_BRIDGE"),
        "the macOS capture points must be the shared ones"
    );
}

#[test]
fn the_menu_is_the_cores_browser_menu_not_a_macos_list() {
    // Criterion 4: both the ⋮ button's menu and the macOS MENU BAR are built from
    // the core's `BrowserMenu`, and an item is dispatched by its STABLE id.
    let paint = paint();
    assert!(
        paint.contains("BrowserMenu::new()") && paint.contains("MenuItemKind::Action"),
        "the menu items must come from the core's BrowserMenu"
    );
    let window = window();
    assert!(
        window.contains("fn build_browser_menu(") && window.contains("menu_items()"),
        "the NSMenu must be built from the core's item list"
    );
    assert!(
        window.contains("chosen.id == MENU_ITEM_DEBUG"),
        "a chosen item must be dispatched by its STABLE id, never its label"
    );
    // The macOS menu BAR is a real convention werust uses -- with the same core
    // content, so the two menus cannot disagree.
    assert!(
        window.contains("fn install_main_menu(") && window.contains("setMainMenu"),
        "macOS gets a real menu bar"
    );
    let main_menu = between(&window, "fn install_main_menu(", "\n}\n");
    assert!(
        main_menu.contains("build_browser_menu(mtm, &controller.ivars().menu_items)"),
        "the menu bar's content must be the SAME core menu, not a second list: {main_menu:?}"
    );
    assert!(
        main_menu.contains("terminate:"),
        "the app menu must carry the platform's own Quit"
    );
}

#[test]
fn the_os_colour_scheme_is_followed_never_forced() {
    // Criterion 5, `docs/adr/0009`: FOLLOW the OS, never force dark. On macOS
    // that is done by NOT acting -- AppKit propagates the effective appearance
    // into the chrome and into the WKWebView's web process -- so the check is an
    // ABSENCE: nothing here may set an appearance.
    let window = code_only(&window());
    for forcing in ["setAppearance", "NSAppearance", "DarkAqua"] {
        assert!(
            !window.contains(forcing),
            "the window must not touch `{forcing}`: ADR-0009 says follow the OS, never force"
        );
    }
    let paint = code_only(&paint());
    for forcing in ["setAppearance", "NSAppearance"] {
        assert!(!paint.contains(forcing), "nor may the paint half");
    }
}

#[test]
fn new_windows_navigate_in_place_via_the_shared_rule() {
    // Criterion 5, `docs/adr/0010`: a `target="_blank"` / `window.open` navigates
    // IN PLACE until tabs exist. That rule is the ENGINE's `WKUIDelegate` over
    // the SHARED `renderer::new_window_action`; this window must neither open a
    // second window for it nor re-decide it.
    let window = code_only(&window());
    assert!(
        !window.contains("new_window_action") && !window.contains("createWebViewWithConfiguration"),
        "the new-window rule is the engine's, over the shared decision; the window must not fork it"
    );
    // Exactly ONE browser window is ever constructed here (the other `NSWindow`
    // is the debug view, which is not a page host).
    let backend = source("crates/macos-renderer/src/backend.rs");
    assert!(
        backend.contains("renderer::new_window_action(target.as_deref())")
            && backend.contains("WKUIDelegate for NavigationBridge"),
        "the engine must still route new windows through the shared rule"
    );
}

#[test]
fn progress_lives_in_the_url_bar_and_only_a_failure_moves_the_page() {
    // Criterion 5, the loading rule (`loading-progress-in-the-url-bar-not-a-banner`):
    // in-flight progress must be visible WITHOUT displacing the page; only a
    // FAILURE may take a banner.
    let window = window();
    // The progress indicator is laid out inside the URL bar's own rectangle, in
    // the fixed-height toolbar.
    let layout = between(&window, "fn relayout(&self) {", "\n    }\n");
    assert!(
        layout.contains("self.progress.setFrame(") && layout.contains("url_width"),
        "the progress strip must be laid out INSIDE the URL bar's own rectangle: {layout:?}"
    );
    assert!(
        layout.contains("TOOLBAR_HEIGHT") && !layout.contains("progress_height_row"),
        "the toolbar's height must not depend on the progress indicator"
    );
    // The page area's height depends on the BANNER only -- never on progress.
    assert!(
        layout.contains("let page_top = TOOLBAR_HEIGHT + banner_height;"),
        "only the error banner may change the page view's geometry: {layout:?}"
    );
    let banner_height = between(
        &window,
        "let banner_height = if self.banner_visible.get()",
        ";",
    );
    assert!(
        banner_height.contains("BANNER_HEIGHT"),
        "the banner takes its strip only when it is visible"
    );
    // And the banner is visible only when the core says a load FAILED.
    let paint = paint();
    assert!(
        paint.contains("error_visible: error_banner_visible(state)"),
        "the banner's visibility is the core's failure rule, not a macOS choice"
    );
}

#[test]
fn the_ci_leg_builds_tests_and_runs_the_window() {
    // Criterion 6: the new surface is actually EXERCISED on a Mac, not merely
    // added to the repo. The existing `macos-14` leg must build it, test it, run
    // the window smoke, and re-run when this crate changes.
    let workflow = source(".github/workflows/macos-renderer.yml");
    assert!(
        workflow.contains("-p werust-macos"),
        "the macOS job must build the window crate"
    );
    assert!(
        workflow.contains("--example window_smoke"),
        "the macOS job must RUN the window smoke, not just compile it"
    );
    assert!(
        workflow.contains("crates/werust-macos/**"),
        "the job's path filters must include the window crate"
    );
    // The smoke must assert on the real WIDGETS (the only thing a Mac adds), and
    // must be able to FAIL.
    let smoke = source("crates/werust-macos/examples/window_smoke.rs");
    for widget in [
        "window.url_text()",
        "window.trust_text()",
        "window.status_text()",
        "window.error_banner()",
        "window.menu_titles()",
        "window.debug_row_counts()",
        "window.page_frame()",
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
    let readme = source("docs/spikes/macos-appkit-window-and-chrome/README.md");
    for section in [
        "What CI proved",
        "What still awaits a Mac",
        "Manual verification",
    ] {
        assert!(
            readme.contains(section),
            "the spike README must state `{section}`"
        );
    }
    assert!(
        exists("docs/spikes/macos-appkit-window-and-chrome/DECISIONS.md"),
        "the judgement calls this task made must be recorded beside it"
    );
    // The local type-check harness the engine task left behind must cover this
    // window too (it is the fast loop that keeps CI from being the first place a
    // typo is found).
    let harness =
        source("docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh");
    assert!(
        harness.contains("crates/werust-macos/src/window.rs"),
        "the local macOS type-check harness must cover the window crate too"
    );
}
