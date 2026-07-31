//! Web-storage edge wiring shape guard (task
//! `android-enable-dom-storage-and-guard-web-platform-parity`, finding
//! `work/notes/findings/android-localstorage-is-null-dom-storage-never-enabled-2026-07-31.md`).
//!
//! WHAT LANDED: the Android OS edge now sets `settings.domStorageEnabled = true`.
//! Before it, `window.localStorage` on Android was **`null`** — not a `Storage`
//! object and not a `SecurityError` throw, so not one of the two answers the web
//! platform allows — because Android's `WebSettings` default is `false` and this
//! edge never set it. The other four edges (WebKitGTK, WKWebView on iOS and
//! macOS, WebView2) have DOM storage ON by default and touch no storage setting
//! at all, which is exactly why the gap was Android-only.
//!
//! WHY A SOURCE-SHAPE GUARD: the Android edge is Kotlin, which this repo's
//! pure-Rust `verify` gate (`cargo fmt && clippy && build && test`, no Android
//! SDK) never compiles, and there is NO CI emulator leg — so the on-device
//! instrumented probe (`crates/werust-android/app/src/androidTest/.../
//! WebStorageTest.kt`) is a hand-run measurement, not a gate. This test is the
//! half that DOES run on every push: it parses the Kotlin edge and pins that DOM
//! storage is still enabled, so a later refactor of that settings block cannot
//! silently return `window.localStorage` to `null`. Same pattern, same reason as
//! the sibling `debug_view_mobile_wiring_shape.rs` and
//! `browser_menu_edge_wiring_shape.rs`.
//!
//! It lives in `werust-core` for the sibling guards' reason: it spans the
//! Android edge, the Android Rust core's origin map AND the parity matrix, and
//! `werust-core` is the one crate every edge sits over.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. The Android edge enables DOM storage, in the `WebView` configuration block,
//!    with a comment saying why the `WebView` default is wrong for a BROWSER and
//!    why per-CID origin mapping makes it safe
//!    (`the_android_edge_enables_dom_storage_where_the_webview_is_configured`).
//! 2. The safety premise is REAL, not asserted: the Android origin map still
//!    gives each CID its own subdomain, so storage stays partitioned per content
//!    address (`each_cid_still_gets_its_own_origin_so_storage_stays_partitioned`).
//! 3. The fix did NOT touch the origin map's opaque-origin work: the `null` ruled
//!    that cause out (an opaque origin throws `SecurityError`)
//!    (`the_origin_map_was_not_touched_chasing_the_wrong_cause`).
//! 4. The parity matrix carries the `web-storage` row — its FIRST web-platform
//!    row — with an explicit cell for all five platforms and the ceiling this
//!    incident exposed stated in its description
//!    (`the_matrix_carries_the_first_web_platform_row_with_an_explicit_cell_everywhere`).
//! 5. The on-device instrumented probe exists and asserts the round-trip, and it
//!    is honestly marked as NOT running in CI
//!    (`the_on_device_probe_asserts_the_round_trip_and_says_it_does_not_run_in_ci`).
//! 6. The `WebSettings` audit is written down and CHANGED NOTHING: every audited
//!    setting appears in the audit note, and none of them is set by the edge
//!    (`the_websettings_audit_is_recorded_and_changed_nothing`).

use std::path::{Path, PathBuf};

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

fn android_origin_map() -> String {
    source("crates/werust-android/rust/src/origin_map.rs")
}

fn on_device_probe() -> String {
    source("crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/WebStorageTest.kt")
}

fn websettings_audit() -> String {
    source(
        "docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/WEBSETTINGS-AUDIT.md",
    )
}

/// The `WebSettings` defaults the task audited and deliberately left ALONE: they
/// are user-visible UX judgements for a human, not conformance bugs. The audit
/// note must name each one; the edge must set none of them.
const AUDITED_BUT_UNCHANGED: [&str; 7] = [
    "builtInZoomControls",
    "displayZoomControls",
    "setSupportZoom",
    "useWideViewPort",
    "loadWithOverviewMode",
    "mediaPlaybackRequiresUserGesture",
    "textZoom",
];

/// The slice of `source` from the start of `from` up to (not including) `to`.
/// Both markers must be present and in order, so a rename cannot silently make a
/// bounded assertion vacuous by matching an empty slice (the discipline the
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

/// The Android edge's `WebView` configuration block: everything between the
/// `WebView(this).apply {` that builds it and the provider script built right
/// after it.
fn webview_configuration_block(activity: &str) -> &str {
    between(
        activity,
        "webView = WebView(this).apply {",
        "providerScript = buildProviderScript()",
    )
}

#[test]
fn the_android_edge_enables_dom_storage_where_the_webview_is_configured() {
    // Criterion 1: the ONE line that fixes the bug, in the block that configures
    // the browsing `WebView` (beside `javaScriptEnabled`), never somewhere a
    // later reader would not look.
    let activity = android_activity();
    let block = webview_configuration_block(&activity);
    assert!(
        block.contains("settings.domStorageEnabled = true"),
        "the Android WebView configuration block must enable DOM storage, or \
         `window.localStorage` returns `null` again: {block:?}"
    );

    // It carries the WHY its neighbours carry: the `WebView` default is built for
    // an app EMBEDDING a view, not for a browser...
    assert!(
        block.contains("domStorageEnabled")
            && block.contains("browser")
            && block.contains("default"),
        "the DOM-storage line must say WHY the WebView default is wrong for a browser: {block:?}"
    );
    // ...and it is safe HERE because each CID gets its own origin (the premise
    // criterion 2 checks is actually true).
    assert!(
        block.contains("origin_map.rs"),
        "the DOM-storage comment must name `origin_map.rs` as why enabling storage is \
         safe (each CID gets its own subdomain, so storage is partitioned per content \
         address): {block:?}"
    );
}

#[test]
fn each_cid_still_gets_its_own_origin_so_storage_stays_partitioned() {
    // Criterion 2: web storage is partitioned by ORIGIN, so enabling it is only
    // safe while two different sites cannot land on the SAME origin. On Android
    // they cannot: the origin map puts each CID in its own HOST LABEL
    // (`https://<cid>.ipfs.werust.invalid`), never a shared host with a per-CID
    // PATH — which would put every site on one origin and make this a real
    // cross-site storage leak. This is the safety premise the Kotlin comment
    // claims, asserted against the code rather than trusted.
    let origin_map = android_origin_map();
    assert!(
        origin_map.contains("pub const INTERNAL_IPFS_HOST_SUFFIX: &str = \".ipfs.werust.invalid\""),
        "the internal origin must still be a per-CID SUBDOMAIN suffix"
    );
    assert!(
        origin_map.contains("{INTERNAL_SCHEME}://{}{INTERNAL_IPFS_HOST_SUFFIX}{tail}"),
        "`to_webview_url` must still build `<cid>` as the HOST LABEL (a shared host with a \
         per-CID path would put every site on ONE origin and make web storage cross-site)"
    );
}

#[test]
fn the_origin_map_was_not_touched_chasing_the_wrong_cause() {
    // Criterion 3: Android is the one platform where `ipfs://` is origin-MAPPED,
    // so an opaque origin is the obvious suspect for a storage failure — but an
    // opaque origin throws `SecurityError`, it does not return `null`. The `null`
    // is the fingerprint that ruled it out, and the origin map is working. This
    // pins that the mapping the SPA-nav fix landed is still exactly what it was:
    // `ipfs://` in, internal https origin out, both directions.
    let origin_map = android_origin_map();
    assert!(
        origin_map.contains("pub fn to_webview_url(url: &str) -> String")
            && origin_map.contains("pub fn from_webview_url(url: &str) -> String"),
        "the origin map's two directions must still be there (the storage fix does not touch it)"
    );
    let activity = android_activity();
    assert!(
        activity.contains("core.toWebViewUrl("),
        "the edge must still map `ipfs://` onto the internal origin (the tuple origin the \
         SPA-nav fix landed, and the origin storage is now partitioned by)"
    );
}

#[test]
fn the_matrix_carries_the_first_web_platform_row_with_an_explicit_cell_everywhere() {
    // Criterion 4: the parity matrix's FIRST WEB-PLATFORM row. Every row before
    // it is a werust FEATURE (trust indicator, ENS resolution, debug view), so
    // the question "does the web platform itself behave the same on all five
    // edges?" was unasked — which is why the guard that exists to stop a
    // capability shipping on one platform could not fire on this bug. The row
    // must say that, and give an explicit cell for all five platforms.
    let matrix = source("docs/platform-capability-matrix.toml");
    let row_start = matrix
        .find("name = \"web-storage\"")
        .expect("the matrix must carry the `web-storage` capability row");
    let row = &matrix[row_start..];
    // Bound the row at the next capability header if there is one, so the cell
    // assertions cannot pass on a LATER row's cells.
    let row = match row.find("\n[[capability]]") {
        Some(end) => &row[..end],
        None => row,
    };
    for platform in ["desktop", "macos", "windows", "ios", "android"] {
        assert!(
            row.contains(&format!("{platform} = {{ state = ")),
            "the web-storage row must give an explicit cell for `{platform}`: {row:?}"
        );
    }
    assert!(
        row.contains("web-platform"),
        "the web-storage row must name itself the matrix's first WEB-PLATFORM row \
         (the ceiling this incident exposed): {row:?}"
    );
    // The general validity of every cell (and that a `stubbed` cell names a task
    // that really exists) is the sibling `platform_capability_parity.rs` guard's
    // job; this test only pins that the row is there and complete.
}

#[test]
fn the_on_device_probe_asserts_the_round_trip_and_says_it_does_not_run_in_ci() {
    // Criterion 5: the instrumented half. It asserts what a device can prove and
    // a source parse cannot — that the REAL System WebView hands back a `Storage`
    // object that round-trips — and it is honest that no CI leg runs it.
    let probe = on_device_probe();
    assert!(
        probe.contains("domStorageEnabled = true"),
        "the on-device probe must exercise a WebView with DOM storage enabled"
    );
    assert!(
        probe.contains("[object Storage]"),
        "the probe must assert `window.localStorage` is a real `Storage` object \
         (the platform's answer, not merely non-null)"
    );
    assert!(
        probe.contains("localStorage") && probe.contains("sessionStorage"),
        "the probe must cover both web-storage areas"
    );
    assert!(
        probe.contains("indexedDB"),
        "the probe must MEASURE IndexedDB too: a localStorage fix that leaves IndexedDB \
         broken is half a fix"
    );
    assert!(
        probe.contains("connectedDebugAndroidTest"),
        "the probe must record the command that runs it (there is no CI emulator leg)"
    );
    assert!(
        probe.contains("does not run in CI") || probe.contains("NOT run in CI"),
        "the probe must say plainly that it does not run in CI"
    );
}

#[test]
fn the_websettings_audit_is_recorded_and_changed_nothing() {
    // Criterion 6: the root cause is general — Android's `WebView` defaults are
    // tuned for an embedded view and werust is a browser — so the task's durable
    // deliverable beside the fix is the LIST of the other browser-wrong defaults,
    // triageable in one pass. Each is a user-visible UX decision for a human, so
    // this change sets NONE of them.
    let audit = websettings_audit();
    let activity = android_activity();
    for setting in AUDITED_BUT_UNCHANGED {
        assert!(
            audit.contains(setting),
            "the WebSettings audit must list `{setting}` (with what it would change and a \
             recommendation), so a human can triage the list in one pass"
        );
        assert!(
            !activity.contains(setting),
            "`{setting}` is a UX judgement this task deliberately did NOT make: it must not \
             be set by the Android edge. Changing it is a human's call — update the audit \
             note at docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/\
             WEBSETTINGS-AUDIT.md when one is made."
        );
    }
}
