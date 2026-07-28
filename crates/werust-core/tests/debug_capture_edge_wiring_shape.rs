//! Console + network CAPTURE-POINT edge-wiring shape guard (task
//! `debug-console-network-capture-per-platform`, spec
//! `in-app-debug-menu-console-and-network`).
//!
//! WHAT LANDED: real console + network capture on all three platforms, feeding
//! the ONE shared bounded store in `werust-core`'s `debug` module (from the
//! blockedBy task `debug-capture-store-console-and-network-in-core`), always-on
//! for Phase 1.
//!
//! WHY A SOURCE-SHAPE GUARD: the mapping halves are unit-tested where they live
//! (`werust_core::debug`'s shim/parse/entry tests, `webview-renderer`'s
//! resource-load mapping, and the two mobile cores' FFI tests). What is NOT
//! otherwise assertable is that the OS EDGES actually CALL those capture points
//! at the right platform hook — and two of the three edges are Kotlin and Swift,
//! which this repo's pure-Rust `verify` gate (`cargo fmt && clippy && build &&
//! test`, no Android SDK, no Xcode) never compiles at all. So this test PARSES
//! the edges and asserts that shape, exactly as its sibling
//! `browser_menu_edge_wiring_shape.rs` does for the browser menu.
//!
//! It lives in `werust-core` (not one edge's crate) because it spans all THREE
//! edges, and `werust-core` is the one crate every edge sits over — the same
//! reason `platform_capability_parity.rs` and `browser_menu_edge_wiring_shape.rs`
//! live here.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. Console captured on all three, with level/message/source/line
//!    (`every_platform_captures_the_console_at_its_own_platform_hook`).
//! 2. Network captured at each platform's reachable points
//!    (`every_platform_captures_network_at_the_points_it_can_reach`).
//! 3. Always-on, bounded, and OFF the UI thread — the Android ANR fix respected
//!    (`android_capture_never_goes_through_the_session_lock`,
//!    `capture_is_always_on_with_no_edge_side_gate`).
//! 4. iOS coverage limits recorded honestly
//!    (`the_ios_coverage_limits_are_recorded_not_silently_partial`).
//! 5. Verification/trust unchanged: capture is READ-ONLY observation
//!    (`capture_is_read_only_and_never_decides_what_is_served`).
//! 6. Parity-tracked (the sibling `platform_capability_parity.rs` enforces the
//!    matrix row) + tests where testable + recorded manual steps
//!    (`the_capture_points_carry_recorded_manual_steps`).

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn desktop_backend() -> String {
    source("crates/webview-renderer/src/backend.rs")
}

fn desktop_shell() -> String {
    source("crates/werust/src/main.rs")
}

fn android_activity() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt")
}

fn android_binding() -> String {
    source("crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt")
}

fn android_core() -> String {
    source("crates/werust-android/rust/src/lib.rs")
}

fn ios_controller() -> String {
    source("crates/werust-ios/App/Sources/WKWebViewShellController.swift")
}

fn ios_binding() -> String {
    source("crates/werust-ios/App/Sources/WerustCore.swift")
}

fn ios_core() -> String {
    source("crates/werust-ios/rust/src/lib.rs")
}

#[test]
fn every_platform_captures_the_console_at_its_own_platform_hook() {
    // Criterion 1. The MECHANISM differs per platform ON PURPOSE (recorded in
    // DECISIONS.md): desktop and iOS have no native console callback, so both
    // inject the ONE shared core shim; Android has the real
    // `WebChromeClient.onConsoleMessage`, which is strictly better.
    let desktop = desktop_backend();
    assert!(
        desktop.contains("console_shim()") && desktop.contains("CAPTURE_BRIDGE"),
        "desktop must inject the SHARED core console shim over the capture channel"
    );
    assert!(
        desktop_shell().contains("install_debug_capture"),
        "the desktop shell must actually install the capture points on the backend"
    );

    let android = android_activity();
    assert!(
        android.contains("override fun onConsoleMessage(message: ConsoleMessage): Boolean"),
        "Android must capture at its REAL native console callback, not a shim"
    );
    assert!(
        android.contains("message.messageLevel().name")
            && android.contains("message.message()")
            && android.contains("message.sourceId()")
            && android.contains("message.lineNumber()"),
        "the Android console entry must carry level/message/source/line"
    );
    assert!(
        !android.contains("console_shim") && !android_binding().contains("console_shim"),
        "Android must NOT inject a console shim: it has the native callback"
    );

    let ios = ios_controller();
    assert!(
        ios.contains("DebugCaptureHandler(core: core)")
            && ios.contains("name: Self.captureChannel"),
        "iOS must register the capture script-message channel"
    );
    assert!(
        ios_core().contains("console_shim()"),
        "the iOS core must inject the SHARED core console shim (WKWebView has no \
         console callback)"
    );
}

#[test]
fn every_platform_captures_network_at_the_points_it_can_reach() {
    // Criterion 2: each platform's reach is different, and each uses the widest
    // point it has.
    assert!(
        desktop_backend().contains("connect_resource_load_started"),
        "desktop must capture at the resource-load signals, which see https too \
         (the ipfs:// scheme handler never does)"
    );

    let android = android_activity();
    // `shouldInterceptRequest` already sees EVERY request, so BOTH branches record.
    assert!(
        android.contains("core.captureNetwork(method, url, 0, \"\", 0L, false, mainFrame)"),
        "Android must record the PASSED-THROUGH (return-null) requests too"
    );
    assert!(
        android.contains("resolution.mimeType,")
            && android.matches("core.captureNetwork").count() >= 3,
        "Android must record the intercepted and the failed branches as well"
    );

    let ios = ios_controller();
    assert!(
        ios.matches("core.captureNetwork(").count() >= 3,
        "iOS must capture at every point it can reach: the two scheme handlers and \
         the main-frame navigation"
    );
    assert!(
        ios_core().contains("network_shim()"),
        "iOS must also inject the best-effort fetch/XHR shim (its only page-wide \
         network reach)"
    );
    assert!(
        !desktop_backend().contains("network_shim()"),
        "desktop must NOT inject the fetch/XHR shim: its resource-load signals \
         already see every resource, so it would double-record a subset"
    );
}

#[test]
fn the_main_frame_reconciliation_uses_the_one_shared_core_predicate() {
    // Criterion 5 (verification/trust honest) + the store's DECISIONS.md Decision
    // 4, as corrected by this task's Decision 5. The MAIN-DOCUMENT row takes the
    // LOAD's own two-axis posture so the Network tab cannot contradict the trust
    // indicator — which only works if each edge agrees on WHICH row that is.
    //
    // There is exactly ONE main-frame predicate in the codebase
    // (`RedirectSink::is_main_frame`, re-exported as `BrowserShell::is_main_frame`),
    // driven by the top-level URL the shell reports on every navigation and
    // normalised through `frame_key`. Android is the one platform with a NATIVE
    // answer (`isForMainFrame`) and uses it. Desktop and iOS have none, and must
    // ASK the core rather than compare URL strings: the naive compares are all
    // wrong (the chrome's url_text is the pinned ENS DISPLAY name, WebKit
    // re-reports `ipfs://<cid>` authority-less, and a redirected main document
    // keeps its pre-redirect URL).
    assert!(
        desktop_backend().contains("is_main_frame(&self.url)"),
        "the desktop capture must ask the SHARED core main-frame predicate"
    );
    assert!(
        !desktop_backend().contains("life.current_url() == Some(self.url"),
        "no local URL compare may stand in for the shared predicate on desktop"
    );

    assert!(
        ios_core().contains("self.shell.is_main_frame(url)"),
        "the iOS core must reconcile the main-document row with the SHARED core \
         predicate, since a WKURLSchemeTask carries no main-frame flag"
    );
    assert!(
        !ios_controller().contains("core.chrome().url == url.absoluteString"),
        "Swift must NOT decide main-frame by comparing against the chrome's \
         DISPLAYED url: on an ENS load that is the pinned name (ronan.eth) while \
         the request is ipfs://<cid>/…, so it never fires on exactly the page the \
         reconciliation exists for"
    );

    assert!(
        android_activity().contains("val mainFrame = request.isForMainFrame"),
        "Android has the platform's OWN main-frame answer and must use it"
    );
}

#[test]
fn a_desktop_resource_is_recorded_once_from_finished_never_also_from_failed() {
    // Criterion 5 again, from the other side. WebKit emits a failed resource's
    // `failed` signal and then ALSO emits `finished` for it
    // (`webkitWebResourceFailed` ends by calling `webkitWebResourceFinished`), so
    // pushing from both recorded every failed load TWICE and the second row
    // claimed the success the first disproved — stamping a failed, possibly
    // hash-MISMATCHED `ipfs://` subresource `content-verified`.
    let desktop = desktop_backend();
    assert_eq!(
        desktop.matches(".record(").count(),
        1,
        "exactly ONE push site per resource: the finished handler"
    );
    let failed_handler = desktop
        .split_once("resource.connect_failed(")
        .expect("the desktop capture connects a failed handler")
        .1
        .split_once("resource.connect_finished(")
        .expect("the failed handler precedes the single finished push")
        .0;
    assert!(
        !failed_handler.contains(".record(") && failed_handler.contains("failed.set(true)"),
        "connect_failed must only FLAG the failure for the single finished push \
         to read, never push a row of its own"
    );
}

#[test]
fn android_capture_never_goes_through_the_session_lock() {
    // Criterion 3, THE ANR GUARD (spec user story 4). `onConsoleMessage` runs on
    // the Android UI THREAD, while `resolve_ipfs` can hold the session lock for
    // SECONDS on a worker thread during a CAR retrieval (`docs/adr/0008`). Pushing
    // a capture entry through the session boundary would block the UI thread
    // behind that retrieval — exactly the ANR the off-main-thread work fixed.
    //
    // The runtime proof is `werust-android-core`'s
    // `a_capture_push_never_waits_on_the_session_lock_so_the_ui_thread_cannot_anr`
    // (it holds the lock and captures from another thread). This pins the SHAPE
    // that makes it possible, so a later refactor cannot quietly route capture
    // back through `self.with(...)`.
    let core = android_core();
    for method in [
        "pub fn push_console_entry(&self, entry: werust_core::debug::ConsoleEntry) {\n        self.debug.push_console(entry);",
        "pub fn push_network_entry(&self, entry: werust_core::debug::NetworkEntry) {\n        self.debug.push_network(entry);",
        "pub fn clear_debug_capture(&self) {\n        self.debug.clear();",
    ] {
        assert!(
            core.contains(method),
            "an Android capture push must reach the CLONED DebugCapture handle \
             directly, never `self.with(...)` (the session lock): missing\n{method}"
        );
    }
    assert!(
        core.contains("werust_core::debug::debug_json(&self.debug)"),
        "the debug view's poll must also read off the session lock"
    );
}

#[test]
fn capture_is_always_on_with_no_edge_side_gate() {
    // Criterion 3 (always-on for Phase 1). No edge may add its own enable flag:
    // the ONE gate is the core store's `network_capture_enabled`, which is the
    // seam the Phase-2 task `debug-network-capture-toggle-config` drives. An
    // edge-local gate would fork that concept and leave the Phase-2 setting
    // half-effective.
    for (name, src) in [
        ("desktop backend", desktop_backend()),
        ("Android activity", android_activity()),
        ("Android binding", android_binding()),
        ("iOS controller", ios_controller()),
        ("iOS binding", ios_binding()),
    ] {
        assert!(
            !src.contains("set_network_capture_enabled")
                && !src.contains("setNetworkCaptureEnabled"),
            "{name} must not gate capture itself: the ONE flag is the core store's, \
             owned by the Phase-2 toggle task"
        );
    }
}

#[test]
fn capture_is_read_only_and_never_decides_what_is_served() {
    // Criterion 5: capturing must not alter the load path, the ipfs://
    // verification, or the trust posture — it REPORTS the posture per entry.
    //
    // The structural risk is an edge letting the capture call's RESULT influence
    // what it serves. Every capture call is therefore a statement returning
    // nothing: `captureNetwork`/`captureConsole` (Kotlin) and `captureNetwork`/
    // `captureScriptMessage` (Swift) are `Unit`/`Void`, so a branch cannot depend
    // on one.
    assert!(
        android_binding().contains(
            "    ) = nativeCaptureNetwork(handle, method, url, status, mime, size, verified, mainFrame)"
        ),
        "the Kotlin capture binding must return Unit, so no branch can depend on it"
    );
    assert!(
        !android_activity().contains("if (core.captureNetwork")
            && !android_activity().contains("when (core.capture"),
        "no Android branch may depend on a capture call"
    );
    assert!(
        !ios_controller().contains("if core.captureNetwork")
            && !ios_controller().contains("switch core.capture"),
        "no iOS branch may depend on a capture call"
    );

    // And no edge may mark verification from a capture point: the posture comes
    // from what the request ACTUALLY did, decided in the core.
    for (name, src) in [
        ("Android activity", android_activity()),
        ("iOS controller", ios_controller()),
    ] {
        assert!(
            !src.contains("mark_content_verified") && !src.contains("markContentVerified"),
            "{name} must not touch verification from a capture point"
        );
    }
}

#[test]
fn the_ios_coverage_limits_are_recorded_not_silently_partial() {
    // Criterion 4: iOS network coverage is honestly PARTIAL (WKWebView exposes no
    // per-resource callback), and the spec accepts that — but only if the limits
    // are RECORDED. Partial is acceptable; silence is not.
    let decisions = source("docs/spikes/debug-console-network-capture-per-platform/DECISIONS.md");
    assert!(
        decisions.contains("iOS"),
        "the decision record must speak to iOS"
    );
    for marker in ["fetch", "XHR", "subresource"] {
        assert!(
            decisions.contains(marker),
            "the iOS coverage limits must name what it can and cannot see: missing \
             `{marker}`"
        );
    }
    // The limits must ALSO be visible at the code site an engineer reads.
    assert!(
        ios_core().contains("browser-internal subresource loads"),
        "the iOS capture wiring must state its coverage limit in situ"
    );
}

#[test]
fn the_capture_points_carry_recorded_manual_steps() {
    // Criterion 6: the LIVE platform hooks (a real WebKitGTK page, a real device
    // WebView, a real simulator WKWebView) cannot be driven by the pure-Rust gate,
    // so they carry recorded manual verification steps — the same discipline the
    // browser-menu and new-window tasks used.
    let readme = source("docs/spikes/debug-console-network-capture-per-platform/README.md");
    for platform in ["Desktop", "Android", "iOS"] {
        assert!(
            readme.contains(platform),
            "the manual steps must cover {platform}"
        );
    }
    assert!(
        readme.contains("console") && readme.contains("Network"),
        "the manual steps must exercise BOTH tabs' capture"
    );
}
