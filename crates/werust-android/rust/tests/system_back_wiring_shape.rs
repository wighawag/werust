//! System-Back wiring shape guard (task
//! `android-hardware-back-button-navigates-history`, spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`, field finding v0.2.5: "the
//! android back button do not navigate back in history like it should").
//!
//! THE PROBLEM: the Android hardware/system Back button used to EXIT the app
//! (the default `Activity` finish) even when there was page history, because the
//! Kotlin OS edge handled only the ON-SCREEN `◀` button. The fix registers an
//! `OnBackPressedCallback` on the Activity's `onBackPressedDispatcher` whose
//! `isEnabled` tracks the core's `canGoBack` and whose `handleOnBackPressed`
//! drives the SAME `driveCore { core.goBack() }` path the on-screen button uses.
//!
//! WHY A SOURCE-SHAPE GUARD: that wiring lives in Kotlin, inside a live Android
//! `Activity` — it cannot run in this repo's pure-Rust `verify` gate (`cargo fmt
//! && clippy && build && test`, no Android SDK). The BEHAVIOUR it depends on is
//! already pinned headlessly in the Rust core (`can_go_back` is the core's truth
//! — see `back_and_forward_reflect_navigation_state_through_the_core` in
//! `src/lib.rs`, plus `the_system_back_affordance_is_enabled_exactly_when_the_core_can_go_back`
//! below, which pins the fact the edge reads). What is NOT otherwise assertable
//! is that the EDGE really reads that fact for the SYSTEM Back affordance, in
//! lockstep with the on-screen button, on the SAME off-UI-thread path. So this
//! test PARSES the Kotlin edge and asserts that shape — the strongest automatable
//! guard for runtime-only wiring, in the same spirit as the config-shape test
//! `crates/werust-core/tests/release_plumbing_shape.rs`. The manual on-device
//! steps that cover the rest are recorded at
//! `docs/spikes/android-hardware-back-button-navigates-history/README.md`.
//!
//! Acceptance criteria mapped to assertions below:
//! 1./2. System Back navigates history when there IS history and falls through to
//!    the default (exit) when there is not — expressed as: the callback is
//!    registered on `onBackPressedDispatcher`, starts DISABLED, and its enabled
//!    state is the core's `canGoBack`
//!    (`the_system_back_callback_is_registered_on_the_dispatcher_and_starts_disabled`,
//!    `the_system_back_enabled_state_is_in_lockstep_with_can_go_back`).
//! 3. The SAME off-UI-thread path as the on-screen button (the ANR fix is not
//!    regressed) (`the_system_back_drives_the_core_off_the_ui_thread_like_the_on_screen_button`).
//! 4. Lockstep enablement, set where the on-screen button's is
//!    (`the_system_back_enabled_state_is_in_lockstep_with_can_go_back`).
//! 5. The non-deprecated API only — no `onBackPressed()` override
//!    (`the_deprecated_on_back_pressed_override_is_not_used`).
//! 6. Tracked per the parity guard: the capability row lives in
//!    `docs/platform-capability-matrix.toml` (enforced by
//!    `crates/werust-core/tests/platform_capability_parity.rs`).

use std::path::{Path, PathBuf};

use werust_mobile::CoreSession;

/// The Kotlin OS edge this guard parses. `CARGO_MANIFEST_DIR` is
/// `crates/werust-android/rust`, so the app module is its sibling.
fn browser_activity_source() -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The body of a `private fun <name>(...)` in the Kotlin source: everything from
/// its signature up to the next member declaration at class-member indentation.
/// Deliberately coarse — enough to assert "these two assignments are in the SAME
/// method" without pinning the method's exact contents.
fn kotlin_fun_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("BrowserActivity.kt must declare `{signature}`"));
    let rest = &source[start + signature.len()..];
    let end = rest
        .find("\n    private fun ")
        .or_else(|| rest.find("\n    fun "))
        .or_else(|| rest.find("\n    override fun "))
        .or_else(|| rest.find("\n    private inner class "))
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn the_system_back_affordance_is_enabled_exactly_when_the_core_can_go_back() {
    // Criteria 1/2, headless: the FACT the edge reads for the system-back
    // callback's `isEnabled` is the core's `canGoBack`, and that fact is false
    // exactly when there is nowhere back — at the very start, and again after
    // walking back to the first entry. That is what makes system Back navigate
    // history when it can and FALL THROUGH to the default (exit the Activity)
    // when it cannot. Network-isolated: no load ever leaves the process; the
    // Kotlin edge's `WebView` signals are simulated by `on_page_*`.
    let mut s = CoreSession::new();
    let settle = |s: &mut CoreSession| {
        let url = s
            .take_pending_load()
            .expect("a pending load for the WebView");
        s.on_page_committed(&url);
        s.on_page_finished(&url);
    };

    assert!(
        !s.chrome().can_go_back,
        "at the start there is no history: the system-back callback is disabled, so the \
         platform default Back exits the app"
    );

    assert!(s.navigate("https://a.example/"));
    settle(&mut s);
    assert!(
        !s.chrome().can_go_back,
        "one entry is still nowhere back: system Back still exits"
    );

    assert!(s.navigate("https://b.example/"));
    settle(&mut s);
    assert!(
        s.chrome().can_go_back,
        "two entries: the system-back callback is ENABLED and Back navigates history"
    );

    // The system Back drives the very same core action the on-screen `◀` does.
    s.go_back();
    settle(&mut s);
    assert_eq!(
        s.chrome().url_text,
        "https://a.example/",
        "Back went back a page"
    );
    assert!(
        !s.chrome().can_go_back,
        "back at the start of history: the callback disables again, so the NEXT system Back \
         falls through to the default and exits"
    );
}

#[test]
fn the_system_back_callback_is_registered_on_the_dispatcher_and_starts_disabled() {
    // Criteria 1/2/5: the system Back is handled through the NON-deprecated
    // `OnBackPressedDispatcher`, and the callback starts DISABLED so that before
    // any history exists (and whenever the core says there is nowhere back) the
    // platform default runs and Back EXITS the app, exactly as a browser does at
    // the start of history.
    let src = browser_activity_source();
    assert!(
        src.contains("androidx.activity.OnBackPressedCallback"),
        "the edge must use the AndroidX `OnBackPressedCallback` (the non-deprecated system-back API)"
    );
    assert!(
        src.contains("onBackPressedDispatcher.addCallback("),
        "the callback must be registered on the Activity's `onBackPressedDispatcher`"
    );
    assert!(
        src.contains("OnBackPressedCallback(false)"),
        "the callback must start DISABLED (no history yet), so system Back exits until the \
         core reports `canGoBack`"
    );
}

#[test]
fn the_system_back_enabled_state_is_in_lockstep_with_can_go_back() {
    // Criterion 4 (and the enabled half of 1/2): the SYSTEM Back affordance and
    // the ON-SCREEN `◀` button are enabled from the SAME core fact
    // (`chrome.canGoBack`) in the SAME place (`refreshChrome`), so the two Back
    // affordances can never disagree — and when `canGoBack` is false the callback
    // is disabled, which is what lets the default Back exit the app.
    let src = browser_activity_source();
    let refresh = kotlin_fun_body(&src, "private fun refreshChrome()");
    assert!(
        refresh.contains("backButton.isEnabled = chrome.canGoBack"),
        "the on-screen back button's enablement must be the core's `canGoBack`, in `refreshChrome`"
    );
    assert!(
        refresh.contains("systemBackCallback.isEnabled = chrome.canGoBack"),
        "the system-back callback's enablement must be set from the SAME `chrome.canGoBack`, in \
         the SAME `refreshChrome` — the two Back affordances stay in lockstep"
    );
}

#[test]
fn the_system_back_drives_the_core_off_the_ui_thread_like_the_on_screen_button() {
    // Criterion 3: system Back must go through the SAME `driveCore { core.goBack() }`
    // dispatch the on-screen button uses — the ANR fix (task
    // `android-anr-main-thread-diagnose-and-unblock`) moved the blocking core
    // actions off the UI thread, and a second Back entry point calling the core
    // inline would regress it.
    let src = browser_activity_source();
    let handler = kotlin_fun_body(&src, "override fun handleOnBackPressed()");
    assert!(
        handler.contains("driveCore { core.goBack() }"),
        "`handleOnBackPressed` must drive the core through `driveCore` (off the UI thread), the \
         SAME path the on-screen `◀` button uses"
    );
    assert!(
        !handler.contains("core.goBack()\n") && !handler.contains("core.goBack();"),
        "the system Back must not call the core inline on the UI thread (that would regress the \
         ANR fix)"
    );
    // The on-screen button's dispatch is the reference path: both must be the
    // same expression, so a future change to one is visibly a change to both.
    assert!(
        src.contains("compactNavButton(\"◀\") { driveCore { core.goBack() } }"),
        "the on-screen `◀` button must keep driving the core through `driveCore` (the reference \
         path the system Back mirrors)"
    );
}

#[test]
fn the_deprecated_on_back_pressed_override_is_not_used() {
    // Criterion 5: the deprecated `onBackPressed()` override (and the raw
    // KEYCODE_BACK route) must NOT be how system Back is handled — the
    // dispatcher is the one implementation that works across versions and
    // bridges to the Android 13+ predictive-back API.
    let src = browser_activity_source();
    assert!(
        !src.contains("override fun onBackPressed("),
        "system Back must be handled via `OnBackPressedDispatcher`, not the deprecated \
         `onBackPressed()` override"
    );
    assert!(
        !src.contains("KEYCODE_BACK"),
        "system Back must not be handled by raw key interception"
    );
}
