//! Mobile debug-view wiring shape guard (task
//! `debug-view-console-network-tabs-mobile`, spec
//! `in-app-debug-menu-console-and-network`).
//!
//! WHAT LANDED: the MOBILE tabbed debug view (Android + iOS). The browser
//! menu's Debug entry (the hook `general-browser-menu-with-version-and-debug-entry`
//! left, until now an honest "not built yet" placeholder) now opens a
//! FULL-SCREEN tabbed screen over the shared capture store: a CONSOLE tab
//! (level-distinguished rows: level + message + source:line) and a NETWORK tab
//! (method / status / mime / size / trust / url rows), a CLEAR action driving
//! the store's clear over the FFI, live updates on the EXISTING chrome-refresh
//! cadence (no new timer, no busy loop), all READ-ONLY. The native remote
//! inspector (chrome://inspect, Safari over USB) is untouched and coexists as
//! the deep devtools.
//!
//! WHY A SOURCE-SHAPE GUARD: the mobile edges are Kotlin and Swift, which this
//! repo's pure-Rust `verify` gate (`cargo fmt && clippy && build && test`, no
//! Android SDK, no Xcode) never compiles. What compilation alone could never
//! prove anyway is that the views really render the SHARED store over the FFI
//! (not an edge-local copy), really reuse the mobile trust indicator's
//! vocabulary (not a minted label), and really update on the existing cadence
//! rather than a tight main-thread poll (the Android ANR fix,
//! `android-anr-main-thread-diagnose-and-unblock`, must not be regressed). So
//! this test PARSES the two mobile edges and asserts that shape, exactly as the
//! sibling `debug_view_desktop_wiring_shape.rs` does for the desktop view and
//! `browser_menu_edge_wiring_shape.rs` does for the menu. The store half (the
//! FFI `debug_json` document and its round-trip, including the trust wire
//! names) is already unit-tested in both mobile Rust cores
//! (`debug_json_round_trips_console_and_network_entries_including_their_trust`).
//!
//! It lives in `werust-core` for the same reason the sibling guards do: it
//! spans BOTH mobile edges plus the parity matrix, and `werust-core` is the one
//! crate every edge sits over.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. On both platforms the Debug entry opens a FULL-SCREEN view with a Console
//!    tab and a Network tab, rendering the core capture store over the FFI
//!    (`the_debug_entry_opens_a_full_screen_console_and_network_view_over_the_shared_store`).
//! 2. The Network tab renders the per-request trust posture with the mobile
//!    trust indicator's SAME vocabulary (ADR-0006)
//!    (`the_network_tab_reuses_the_mobile_trust_indicators_vocabulary_never_a_new_label`).
//! 3. Clear drives the store's clear over the FFI, and the view updates on the
//!    existing chrome-refresh cadence with NO tight/busy main-thread poll
//!    (`clear_empties_the_store_and_the_view_updates_on_the_existing_cadence`).
//! 4. The view is READ-ONLY (no typeable input in the debug view)
//!    (`the_view_is_read_only`).
//! 5. Mobile-scoped + parity-tracked: the matrix row is implemented on all
//!    three, and the native remote inspector wiring is untouched
//!    (`the_parity_matrix_marks_all_three_debug_views_implemented`).
//! 6. Tests cover what is testable (this guard plus the FFI round-trip tests in
//!    the mobile Rust cores); the views themselves carry recorded manual
//!    device steps at
//!    `docs/spikes/debug-view-console-network-tabs-mobile/README.md`.

use std::path::{Path, PathBuf};

use renderer::TrustPosture;
use werust_core::{trust_indicator, ChromeState};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn android_activity() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt")
}

fn android_debug_view() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/DebugView.kt")
}

fn android_binding() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt")
}

fn ios_controller() -> String {
    source("crates/werust-ios/App/Sources/WKWebViewShellController.swift")
}

fn ios_binding() -> String {
    source("crates/werust-ios/App/Sources/WerustCore.swift")
}

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice (the same discipline the
/// sibling guards record).
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
fn the_debug_entry_opens_a_full_screen_console_and_network_view_over_the_shared_store() {
    // Criterion 1: the menu's Debug entry calls the named open-debug-view hook,
    // which now opens the REAL full-screen view on both platforms: the "is not
    // built yet" placeholder the menu task landed is GONE from both hooks.
    let android = android_activity();
    assert!(
        android.contains("private fun openDebugView()"),
        "Android must keep the named openDebugView hook"
    );
    assert!(
        !android.contains("is not built yet"),
        "the Android placeholder must be gone: the hook now opens the REAL view"
    );
    assert!(
        android.contains("debugView.open()"),
        "the Android hook must open the debug view"
    );
    // FULL-SCREEN: the view is laid out MATCH_PARENT over the whole browser
    // chrome (an overlay on the root container), not a corner panel.
    assert!(
        android.contains("debugView") && android.contains("FrameLayout"),
        "the Android debug view must overlay the browser chrome full-screen"
    );

    let ios = ios_controller();
    assert!(
        ios.contains("private func openDebugView()"),
        "iOS must keep the named openDebugView hook"
    );
    assert!(
        !ios.contains("is not built yet"),
        "the iOS placeholder must be gone: the hook now opens the REAL view"
    );
    assert!(
        ios.contains("DebugViewController(core: core)")
            && ios.contains("modalPresentationStyle = .fullScreen"),
        "the iOS hook must present the debug view FULL-SCREEN"
    );

    // BOTH views have a Console tab and a Network tab. The task allowed "a
    // TabLayout + a pager, or two toggled lists": both edges toggle ONE list
    // between the two tabs (Android two toggle buttons, iOS a
    // UISegmentedControl), so neither edge takes a new UI dependency for the
    // tab strip.
    let android_view = android_debug_view();
    assert!(
        android_view.contains("\"Console\"") && android_view.contains("\"Network\""),
        "the Android debug view must have a Console tab and a Network tab"
    );
    assert!(
        ios.contains("UISegmentedControl(items: [\"Console\", \"Network\"])"),
        "the iOS debug view must have a Console tab and a Network tab"
    );

    // BOTH views render the SHARED capture store over the FFI: the dedicated
    // `debug_json` document (the accessor the store task recorded, kept off the
    // chrome JSON so the per-keystroke chrome refresh stays lean), never an
    // edge-local copy. The bindings are the same accessors the capture task's
    // guard already pins to the shared store.
    assert!(
        android_view.contains("core.debugJson()"),
        "the Android debug view must render the shared store over the FFI"
    );
    assert!(
        android_binding().contains("fun debugJson(): String = nativeDebugJson(handle)"),
        "the Kotlin binding must read the debug document from the core over JNI"
    );
    assert!(
        ios.contains("core.debugJSON()"),
        "the iOS debug view must render the shared store over the FFI"
    );
    assert!(
        ios_binding().contains("werust_ios_debug_json(handle)"),
        "the Swift binding must read the debug document from the core over the C-ABI"
    );
}

#[test]
fn the_network_tab_reuses_the_mobile_trust_indicators_vocabulary_never_a_new_label() {
    // Criterion 2: the Network tab's per-request trust renders the core's wire
    // names (`content-verified` / `unverified-origin` / `name-via-trusted-rpc`
    // / `mutable-name`, which the debug JSON's `trust` field already carries)
    // with the SAME glyphs the trust indicator paints (the core's
    // `trust_indicator`, which both mobile edges now carry as `Chrome`'s
    // `trustIndicator` field). No new trust label is minted for the debug view
    // (ADR-0006; the desktop view's Decision 4, which this mirrors).
    for (name, view) in [("Android", android_debug_view()), ("iOS", ios_debug_view())] {
        assert!(
            view.contains("networkTrustLabel"),
            "{name} must map the posture onto a trust label through one named mapping"
        );
        for wire_name in [
            "content-verified",
            "unverified-origin",
            "name-via-trusted-rpc",
            "mutable-name",
        ] {
            assert!(
                view.contains(wire_name),
                "{name}'s Network tab must render the core's `{wire_name}` wire name, \
                 not a minted label"
            );
        }
        for glyph in ["✓", "◈", "◇", "⚠"] {
            assert!(
                view.contains(glyph),
                "{name}'s Network tab must reuse the trust indicator's `{glyph}` glyph"
            );
        }
    }

    // The glyphs really are the TRUST INDICATOR's own, checked against the rule
    // itself rather than against a copy of it: every glyph the Network tab reuses
    // must appear in what `werust_core::trust_indicator` produces over
    // `TrustPosture::ALL`, so the tab can never speak a vocabulary the indicator
    // does not own.
    //
    // This assertion used to read the glyphs out of each mobile BINDING's own
    // `trustIndicator()` twin. Those twins are gone (task
    // `mobile-chrome-presentation-from-one-derivation`): the badge is derived once
    // in the core and carried to both edges on the chrome JSON, so the core is
    // where the vocabulary now lives, and asserting against the derivation is
    // strictly stronger than asserting against a transcription of it.
    let painted: String = TrustPosture::ALL
        .iter()
        .map(|posture| {
            trust_indicator(&ChromeState {
                trust_posture: *posture,
                ..ChromeState::default()
            })
        })
        .collect();
    for glyph in ["✓", "◈", "◇", "⚠"] {
        assert!(
            painted.contains(glyph),
            "the shared trust indicator must paint the `{glyph}` glyph the Network tab reuses; \
             it paints: {painted:?}"
        );
    }
}

#[test]
fn clear_empties_the_store_and_the_view_updates_on_the_existing_cadence() {
    // Criterion 3: the Clear action drives the shared store's clear over the
    // FFI (both buffers), and the open view is refreshed from the EXISTING
    // refresh points: the shell's own chrome refresh (the mobile cadence is
    // event-driven, after each core action / page lifecycle signal) plus the
    // console capture event itself, which already runs on the UI/main thread.
    // NO new timer, NO Handler/postDelayed loop, NO busy poll is added for the
    // debug view; the Android ANR fix is respected (the store's debug_json
    // reads OFF the session lock precisely so this refresh can never block the
    // UI thread behind an in-flight `ipfs://` retrieval).
    let android_view = android_debug_view();
    assert!(
        android_view.contains("core.debugClear()"),
        "the Android Clear action must drive the shared store's clear over the FFI"
    );
    let android = android_activity();
    let chrome_refresh = between(
        &android,
        "private fun refreshChrome()",
        "private fun buildProviderScript()",
    );
    assert!(
        chrome_refresh.contains("debugView.refresh()"),
        "the open Android debug view must refresh on the EXISTING chrome-refresh point: {chrome_refresh:?}"
    );
    let console_capture = between(
        &android,
        "override fun onConsoleMessage",
        "override fun onCreateWindow",
    );
    assert!(
        console_capture.contains("debugView.refresh()"),
        "a captured console message (already on the UI thread) must refresh the open view \
         from its own event: {console_capture:?}"
    );
    for poll in [
        "postDelayed",
        "Handler(",
        "Timer(",
        "ScheduledExecutorService",
    ] {
        assert!(
            !android_view.contains(poll),
            "the Android debug view must not add a `{poll}` poll/loop: the refresh is event-driven"
        );
    }

    let ios = ios_controller();
    let ios_view = ios_debug_view();
    assert!(
        ios_view.contains("core.debugClear()"),
        "the iOS Clear action must drive the shared store's clear over the FFI"
    );
    let chrome_refresh = between(
        &ios,
        "private func refreshChrome()",
        "// --- the general browser menu",
    );
    assert!(
        chrome_refresh.contains("debugViewController?.refresh()"),
        "the open iOS debug view must refresh on the EXISTING chrome-refresh point: {chrome_refresh:?}"
    );
    let capture_handler = between(
        &ios,
        "final class DebugCaptureHandler",
        "final class ProviderBridgeHandler",
    );
    assert!(
        capture_handler.contains("onCapture?()"),
        "a captured shim message (already on the main thread) must refresh the open view \
         from its own event: {capture_handler:?}"
    );
    for poll in ["Timer.scheduledTimer", "Timer(", "DispatchSourceTimer"] {
        assert!(
            !ios.contains(poll),
            "the iOS edge must not add a `{poll}` poll/loop for the debug view: \
             the refresh is event-driven"
        );
    }
}

#[test]
fn the_view_is_read_only() {
    // Criterion 4: the debug view is READ-ONLY (it renders the store; a
    // typeable REPL is the native remote inspector's job, spec Out of Scope).
    // Neither view constructs an input widget.
    let android_view = android_debug_view();
    for input in ["EditText(", "AutoCompleteTextView(", "SearchView("] {
        assert!(
            !android_view.contains(input),
            "the Android debug view must be READ-ONLY, but it builds a `{input}`"
        );
    }
    let ios_view = ios_debug_view();
    for input in ["UITextField(", "UITextView(", "UISearchBar("] {
        assert!(
            !ios_view.contains(input),
            "the iOS debug view must be READ-ONLY, but it builds a `{input}`"
        );
    }
}

#[test]
fn the_parity_matrix_marks_all_three_debug_views_implemented() {
    // Criterion 5: this task is MOBILE-scoped and parity-tracked (ADR-0005).
    // The `debug-view-console-network` row is now implemented on all three
    // platforms (the desktop half landed in
    // `debug-view-console-network-tabs-desktop`, guarded by the sibling
    // `debug_view_desktop_wiring_shape.rs`), and the native remote inspector
    // (`web-inspector`) is untouched: the mobile debug-build gates for
    // chrome://inspect / Safari-over-USB are still wired exactly as the
    // inspector task left them.
    let matrix = source("docs/platform-capability-matrix.toml");
    // The row is the LAST capability in the matrix, so slice to the end of the
    // file rather than to a following `[[capability]]` header.
    let row_start = matrix
        .find("name = \"debug-view-console-network\"")
        .expect("the matrix must carry the debug-view-console-network row");
    let row = &matrix[row_start..];
    for platform in ["desktop", "ios", "android"] {
        assert!(
            row.contains(&format!("{platform} = {{ state = \"implemented\" }}")),
            "the {platform} debug view must be marked implemented: {row:?}"
        );
    }

    let android = android_activity();
    assert!(
        android.contains("WebView.setWebContentsDebuggingEnabled(true)")
            && android.contains("FLAG_DEBUGGABLE"),
        "the Android chrome://inspect remote inspector must be untouched (it coexists)"
    );
    let ios = ios_controller();
    assert!(
        ios.contains("webView.isInspectable = true") && ios.contains("#if DEBUG"),
        "the iOS Safari Web Inspector must be untouched (it coexists)"
    );
}

/// The iOS debug view is a slice of the shell controller file (kept there so
/// the task needs no Xcode project-file edit): from its declaration to the
/// next class.
fn ios_debug_view() -> String {
    between(
        &ios_controller(),
        "final class DebugViewController",
        "final class DebugCaptureHandler",
    )
    .to_string()
}
