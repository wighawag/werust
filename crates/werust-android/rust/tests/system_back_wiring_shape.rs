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
//! lockstep with the on-screen button, on the SAME off-UI-thread path; the
//! Gradle/Kotlin build is not in `verify` either, so nothing else in the gate
//! sees that wiring at all. So this test PARSES the Kotlin edge and asserts that
//! shape: the strongest automatable guard for runtime-only wiring, and the ONLY
//! gate-side cover it has, in the same spirit as the config-shape test
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
//!    regressed) (`the_system_back_drives_the_core_off_the_ui_thread_like_the_on_screen_button`):
//!    `handleOnBackPressed`'s OWN body (brace-matched by [`kotlin_block_body`],
//!    so it stops at the handler's closing brace) must contain
//!    `driveCore { core.goBack() }` and, once that dispatch is stripped, must
//!    contain no remaining `core.` call at all. Emptying the handler, replacing
//!    the dispatch with an inline UI-thread `core.goBack()`, or ADDING an inline
//!    core call beside the dispatch each turn that assertion RED (verified by
//!    mutation), and [`the_block_extractor_stops_at_the_matching_brace`] pins
//!    the extractor itself so the guard cannot silently go vacuous again.
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

/// The first occurrence of `pat` in `bytes` at or after `from`, as an absolute
/// index. Byte-wise (never slices the `str`) so the scan below can walk a source
/// that contains multi-byte characters (`◀`, `⛔`, and the like) without landing
/// mid-char.
fn find_from(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|i| from + i)
}

/// The BODY of a Kotlin declaration: the text between the braces of the block
/// that opens after `signature`, bounded at its MATCHING closing brace.
///
/// POSITION-bounded, deliberately. The first version of this guard ended a body
/// at the next member declaration picked by KIND, not position (`\n    private
/// fun ` was tried BEFORE `\n    override fun `), so for `handleOnBackPressed` it
/// matched the far-later `private fun driveCore` and swallowed the whole of
/// `onCreate`. The criterion-3 assertion below was then satisfied by the
/// ON-SCREEN button's `compactNavButton("◀") { driveCore { core.goBack() } }`
/// line and stayed GREEN even with an EMPTY handler, i.e. a vacuous guard. Brace
/// matching cannot drift that way: the body ends where the block ends. The
/// extractor's own regression guard is
/// [`the_block_extractor_stops_at_the_matching_brace`].
///
/// `//` line comments, `/* */` block comments and `"`/`"""` string literals are
/// skipped, so a brace inside one cannot unbalance the count.
fn kotlin_block_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let bytes = source.as_bytes();
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("the Kotlin source must declare `{signature}`"));
    let after = start + signature.len();
    let open =
        find_from(bytes, after, b"{").unwrap_or_else(|| panic!("`{signature}` must open a block"));

    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let tail = &bytes[i..];
        if tail.starts_with(b"//") {
            i = find_from(bytes, i, b"\n").unwrap_or(bytes.len());
            continue;
        }
        if tail.starts_with(b"/*") {
            i = find_from(bytes, i + 2, b"*/").map_or(bytes.len(), |e| e + 2);
            continue;
        }
        if tail.starts_with(b"\"\"\"") {
            i = find_from(bytes, i + 3, b"\"\"\"").map_or(bytes.len(), |e| e + 3);
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i += 1;
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    // `open` and `i` are both ASCII brace positions, so these are
                    // char boundaries.
                    return &source[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("`{signature}` opens a block that is never closed")
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
    let refresh = kotlin_block_body(&src, "private fun refreshChrome()");
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
    // POSITION-bounded (brace-matched) so this is the handler's OWN body and
    // nothing else. See `kotlin_block_body` and
    // `the_block_extractor_stops_at_the_matching_brace`. Emptying the handler, or
    // making it call the core inline, FAILS these asserts.
    let handler = kotlin_block_body(&src, "override fun handleOnBackPressed()");
    assert!(
        handler.contains("driveCore { core.goBack() }"),
        "`handleOnBackPressed` must drive the core through `driveCore` (off the UI thread), the \
         SAME path the on-screen `◀` button uses; its body is instead: {handler:?}"
    );
    // Every core call in the handler must be the dispatched one: strip the
    // `driveCore` dispatch and NOTHING that touches the core may remain, so an
    // added inline `core.<anything>()` on the UI thread (the ANR regression this
    // guards) fails here.
    let without_dispatch = handler.replace("driveCore { core.goBack() }", "");
    assert!(
        !without_dispatch.contains("core."),
        "the system Back must not call the core inline on the UI thread (that would regress the \
         ANR fix); the handler has a core call outside `driveCore`: {without_dispatch:?}"
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
fn the_block_extractor_stops_at_the_matching_brace() {
    // The guard ON the guard. `kotlin_block_body` is what makes the criterion-3
    // assertion above mean anything, so its bounding is pinned here on a fixture
    // shaped exactly like the trap that made the first version of this test
    // VACUOUS: a short `override fun` whose next member declaration is an
    // `override fun`, with a `private fun` (carrying a decoy `driveCore { ... }`
    // call) FURTHER DOWN. A kind-ordered terminator search reaches for the
    // `private fun` first and swallows everything between; a brace-matched one
    // stops at the handler's own closing brace.
    let fixture = "\
class Fixture {
    private val cb = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() {
            driveCore { core.goBack() }
        }
    }

    override fun onCreate() {
        val b = compactNavButton(\"◀\") { driveCore { core.goBack() } }
    }

    private fun driveCore(action: () -> Unit) {
    }
}
";
    let handler = kotlin_block_body(fixture, "override fun handleOnBackPressed()");
    assert!(
        handler.contains("driveCore { core.goBack() }"),
        "the extracted body must contain the handler's own dispatch"
    );
    assert!(
        !handler.contains("compactNavButton"),
        "the extracted body must STOP at the handler's matching brace; it must not run on into \
         `onCreate` and pick up the ON-SCREEN button's dispatch (the vacuity that made the first \
         version of this guard pass with an EMPTY handler); it extracted: {handler:?}"
    );

    // An EMPTY handler must extract as empty, which is what makes the
    // criterion-3 assertion FAIL when the handler stops driving the core.
    let emptied = fixture.replace("            driveCore { core.goBack() }\n", "");
    assert!(
        !kotlin_block_body(&emptied, "override fun handleOnBackPressed()").contains("core.goBack"),
        "an EMPTY handler must extract as an empty body (otherwise the criterion-3 guard is \
         vacuous)"
    );

    // Braces inside comments and string literals must not unbalance the count.
    let tricky = "\
    private fun sample() {
        // a brace in a comment: }
        /* and a block one: } */
        val s = \"a literal brace }\"
        val t = \"\"\"a raw one }\"\"\"
        val marker = 1
    }

    private fun after() {
        val outside = 2
    }
";
    let body = kotlin_block_body(tricky, "private fun sample()");
    assert!(
        body.contains("val marker = 1") && !body.contains("val outside = 2"),
        "braces inside comments/strings must not end the body early or late; extracted: {body:?}"
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
