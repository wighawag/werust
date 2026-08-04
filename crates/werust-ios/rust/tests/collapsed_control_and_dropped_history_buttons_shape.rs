//! iOS collapsed reload/stop control + spinner + dropped history buttons wiring
//! shape guard (task `ios-chrome-collapse-reload-stop-and-drop-history-buttons`,
//! spec `chrome-conventional-controls`, stories 8-11).
//!
//! WHAT LANDED: the iOS toolbar lost its `◀` and `▶` buttons (the WebKit
//! EDGE-SWIPE gesture is the iOS history affordance, and unlike Android it covers
//! BOTH directions), and its separate Reload and Stop buttons became the ONE
//! control the core derives, with a loading spinner beside it. Every value the
//! edge paints is read off the chrome JSON it already decodes each refresh
//! (`reloadStopControlLabel`, `reloadStopControlDescription`,
//! `loadSpinnerVisible`); the freed width goes to the URL field, which already
//! hugs weakly and therefore absorbs it with no arithmetic at all.
//!
//! WHY A SOURCE-SHAPE GUARD: the painter is Swift inside a live `UIViewController`,
//! and this repo's `verify` gate is pure Rust (`cargo fmt && clippy && build &&
//! test`, no Xcode and no simulator), so nothing else in the gate can see this
//! wiring. There is also no Mac on this project at all
//! (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`),
//! so the removal cannot rest on anyone LOOKING at the toolbar. The DERIVATION is
//! unit-tested where it lives (`werust_core::reload_stop_control` /
//! `load_spinner_visible`) and the carrier is asserted to agree with it verbatim;
//! what neither can see is whether this EDGE reads them, or whether a Swift
//! `switch` came back to decide the mode — the twin this repo has already deleted
//! once (`mobile-chrome-presentation-from-one-derivation`, `docs/adr/0011`), and
//! which this very edge shipped in its invalid-entry badge. Same spirit as the
//! sibling guards `back_forward_gesture_wiring_shape.rs` and
//! `crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs`.
//!
//! The one thing a runner CAN do beyond this file is compile the Swift and link
//! the core: the `mobile-ios` leg builds the Simulator `.app` and
//! `docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh` asserts the
//! bundle carries the C-ABI symbols, including the new
//! `werust_ios_activate_reload_stop_control` this collapse introduced.
//!
//! Acceptance criteria mapped to assertions below:
//! - The toolbar no longer shows back or forward buttons
//!   (`the_ios_toolbar_has_no_back_or_forward_buttons`).
//! - The edge-swipe gesture is verified ENABLED before the buttons are removed
//!   (`the_swipe_gesture_the_removal_rests_on_is_enabled_and_still_guarded`).
//! - Reload and Stop are ONE control whose mode comes from the chrome JSON, and
//!   cancelling an in-flight load is still possible
//!   (`the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight`,
//!   `the_toolbar_carries_one_reload_stop_control_and_the_spinner`,
//!   `the_control_performs_the_modes_own_action_through_the_core`).
//! - A spinner shows while loading, its visibility read from the chrome JSON
//!   (`the_control_and_the_spinner_are_painted_from_the_carried_derivation`).
//! - No Swift conditional decides the control mode or the spinner's visibility
//!   (`no_swift_conditional_decides_the_control_mode_or_the_spinner`).
//! - `can_go_back` / `can_go_forward` and the history seam are unchanged; only
//!   the painter changes
//!   (`the_history_capability_is_untouched_by_the_button_removal`).
//! - Every field this edge consumes is registered in the mobile presentation
//!   guard's `DERIVED_FIELDS`, so the CENTRAL guard (not only this per-edge one)
//!   demands it of BOTH edges
//!   (`the_mobile_presentation_guard_registers_the_fields_this_edge_consumes`).
//!   This assertion was the SEQUENCING hold in the MIGRATE step: it asserted the
//!   fields were NOT yet registered, so a well-meaning registration could not
//!   creep in before the CONTRACT step got its own review. That step
//!   (`register-the-new-chrome-fields-in-the-mobile-presentation-guard`) landed
//!   the registration and INVERTED this check, which keeps the coupling live in
//!   the other direction: a field this edge paints can no longer be quietly
//!   dropped from the central list to make a gate green.
//!
//! Every assertion runs on the Linux gate with no network, no Xcode and no
//! simulator. The decisions this bakes in are recorded in
//! `docs/spikes/ios-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md`.

use std::path::{Path, PathBuf};

use renderer::LoadState;
use werust_core::{reload_stop_control, ReloadStopControl};
use werust_mobile::CoreSession;

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-ios/rust`, so the root is three levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const SWIFT_BINDING: &str = "crates/werust-ios/App/Sources/WerustCore.swift";
const SWIFT_PAINTER: &str = "crates/werust-ios/App/Sources/WKWebViewShellController.swift";
/// The sibling guard that pins the gesture this task's removal RESTS on.
const GESTURE_GUARD: &str = "crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs";

/// The DERIVED chrome-JSON fields this edge starts consuming here. Registered in
/// the mobile presentation guard's `DERIVED_FIELDS` by the CONTRACT step
/// `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, once BOTH
/// mobile edges consumed them; this list is what THIS edge's own guard demands,
/// and what
/// [`the_mobile_presentation_guard_registers_the_fields_this_edge_consumes`]
/// holds the central guard to.
///
/// It is the SAME three the Android edge consumes, and it deliberately EXCLUDES
/// the mode's stable wire name `reloadStopControl`: a painter that must not branch
/// on the mode has nothing to do with it, and the only tempting consumer is the
/// `switch` this collapse exists to prevent. Both mobile edges answering that the
/// same way is what lets the fan-in task register one shape rather than inherit an
/// argument (`docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md`
/// section 4).
const CONSUMED_DERIVED_FIELDS: &[&str] = &[
    "reloadStopControlLabel",
    "reloadStopControlDescription",
    "loadSpinnerVisible",
];

/// The Swift source with COMMENTS removed and every STRING LITERAL collected, so
/// an assertion can tell wiring from documentation.
///
/// Both halves are load-bearing. The negative assertions ("no `◀` any more",
/// "nothing reads the raw loading fact") must not trip on the doc comments that
/// EXPLAIN the removal — documentation naming a deleted affordance is
/// documentation, and this file is comment-heavy by house style. The literal half
/// is what proves the control's glyph is the core's `reloadStopControlLabel`
/// rather than a `⟳` hand-written here.
///
/// The same scanner shape the sibling guards use (`//` line comments, `/* */`
/// blocks, `"` strings with backslash escapes, `"""` raw strings); a `//` inside a
/// string is not a comment, which is why this is a scanner and not a regex. It is
/// the fourth near-identical copy in this repo, filed as
/// `work/notes/observations/kotlin-source-scanner-duplicated-across-edge-guards-2026-08-04.md`
/// rather than collapsed mid-task; its own regression guard is
/// [`the_scanner_reads_literals_and_code_apart`].
fn scan(source: &str) -> (String, Vec<String>) {
    let bytes = source.as_bytes();
    let mut code = String::with_capacity(source.len());
    let mut literals = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let tail = &bytes[i..];
        if tail.starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if tail.starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if tail.starts_with(b"\"\"\"") {
            let start = i + 3;
            i = start;
            while i < bytes.len() && !bytes[i..].starts_with(b"\"\"\"") {
                i += 1;
            }
            literals.push(source[start..i.min(source.len())].to_string());
            i = (i + 3).min(bytes.len());
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            literals.push(source[start..i.min(source.len())].to_string());
            i = (i + 1).min(bytes.len());
            continue;
        }
        // Push the whole character, not one byte: this source carries `⟳`, `⋮`
        // and friends, and slicing mid-char would panic.
        let ch = source[i..].chars().next().expect("a char at a boundary");
        code.push(ch);
        i += ch.len_utf8();
    }
    (code, literals)
}

/// The scanned code with every run of whitespace collapsed to one space, so an
/// assertion about a STATEMENT is immune to where the house style happens to wrap
/// it (`swift-format` wraps the toolbar's `arrangedSubviews` list across lines).
fn statements(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `code` contains `target = value` as a complete assignment: the value
/// must END there, so assigning a LONGER field of the same prefix does not count.
///
/// That boundary is the whole point: `chrome.reloadStopControl` is a prefix of
/// `chrome.reloadStopControlLabel` AND of `chrome.reloadStopControlDescription`,
/// so a plain `contains` could not tell the mode's wire name from the glyph the
/// control actually wears. Pinned by
/// [`the_assignment_check_is_not_satisfied_by_a_longer_field_name`].
fn assigns(code: &str, target: &str, value: &str) -> bool {
    let statement = format!("{target} = {value}");
    let flat = statements(code);
    flat.match_indices(&statement).any(|(at, _)| {
        flat[at + statement.len()..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '_' && next != '.')
    })
}

/// Settle a load the way the Swift edge's `WKNavigationDelegate` callbacks do, so
/// the core reaches the state a real page arrival leaves it in. No network: the
/// signals are simulated, exactly as the sibling guards do it.
fn settle(session: &mut CoreSession) {
    let url = session
        .take_pending_load()
        .expect("a pending load to apply to the WKWebView");
    session.on_page_committed(&url);
    session.on_page_finished(&url);
}

#[test]
fn the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight() {
    // The BEHAVIOUR behind the collapse, headless: ONE control that reloads a
    // settled page and cancels a load in flight, driven through the single core
    // entry point the Swift tap handler calls
    // (`CoreSession::activate_reload_stop_control`). The mode is the core's
    // (`reload_stop_control`), so this asserts the entry point performs exactly
    // what the mode names — the edge chooses nothing.
    //
    // Network-isolated: nothing leaves the process; the WKWebView's load signals
    // are simulated by `on_page_*`.
    let mut s = CoreSession::new();

    assert!(s.navigate("https://example.com/"));
    settle(&mut s);
    assert_eq!(
        reload_stop_control(s.chrome()),
        ReloadStopControl::Reload,
        "a settled page: the ONE control is in its RELOAD mode"
    );

    s.activate_reload_stop_control();
    assert!(
        s.chrome().is_loading(),
        "activating the control in RELOAD mode must reload the page"
    );
    assert_eq!(
        reload_stop_control(s.chrome()),
        ReloadStopControl::Stop,
        "a load in flight: the SAME control is now in its STOP mode"
    );

    // Acceptance: cancelling an in-flight load is STILL possible after the
    // collapse — the separate Stop button was the documented cancel affordance,
    // and this is the control that replaced it.
    s.activate_reload_stop_control();
    assert_eq!(
        s.chrome().load_state,
        LoadState::Idle,
        "activating the control in STOP mode must cancel the in-flight load"
    );
    assert_eq!(
        reload_stop_control(s.chrome()),
        ReloadStopControl::Reload,
        "and the control falls back to RELOAD once the load is cancelled"
    );
}

#[test]
fn the_history_capability_is_untouched_by_the_button_removal() {
    // Acceptance: this task removes BUTTONS, not the capability. `can_go_back` /
    // `can_go_forward` and the history seam are exactly as they were — the
    // edge-swipe rides on them (and so do the desktop shortcuts), so a regression
    // here would take the ONLY history affordance iOS has left with it.
    //
    // Unlike Android, iOS keeps BOTH directions after its buttons go, because the
    // swipe covers back AND forward: this drives the pair the way the gesture
    // does.
    let mut s = CoreSession::new();
    assert!(!s.chrome().can_go_back && !s.chrome().can_go_forward);

    assert!(s.navigate("https://a.example/"));
    settle(&mut s);
    assert!(s.navigate("https://b.example/"));
    settle(&mut s);
    assert!(
        s.chrome().can_go_back,
        "two entries: there is somewhere back"
    );
    assert!(!s.chrome().can_go_forward);

    s.go_back();
    settle(&mut s);
    assert_eq!(s.chrome().url_text, "https://a.example/");
    assert!(
        s.chrome().can_go_forward,
        "the forward capability is live, and on iOS it still has an affordance: \
         the swipe navigates BOTH ways"
    );

    s.go_forward();
    settle(&mut s);
    assert_eq!(s.chrome().url_text, "https://b.example/");
    assert!(s.chrome().can_go_back && !s.chrome().can_go_forward);
}

#[test]
fn the_swipe_gesture_the_removal_rests_on_is_enabled_and_still_guarded() {
    // Acceptance 2, and the reason this task was BLOCKED on
    // `enable-the-ios-back-forward-swipe-gesture`: `WKWebView` defaults
    // `allowsBackForwardNavigationGestures` to FALSE, so removing the on-screen
    // buttons while it is unset would leave iOS with NO history navigation at all
    // — and with no Mac on this project, nobody would find out by using the app.
    //
    // Asserted HERE as well as in the sibling guard deliberately: this is the file
    // that documents WHY the buttons are gone, so the precondition it rests on
    // should red THIS suite too, not only a file a reader might not open. The
    // second half pins that the sibling guard still exists and still owns the
    // full gesture wiring, so this is a cross-check rather than a fork of it.
    let (painter, _) = scan(&source(SWIFT_PAINTER));
    assert!(
        painter.contains("webView.allowsBackForwardNavigationGestures = true"),
        "the edge-swipe gesture must be ENABLED: it is the ONLY history affordance \
         left on this edge now that the ◀/▶ buttons are gone, and WKWebView's \
         default is false"
    );
    assert!(
        !painter.contains("allowsBackForwardNavigationGestures = false"),
        "nothing may disable the gesture: iOS would be left with no way to move \
         through history at all"
    );

    let guard = source(GESTURE_GUARD);
    for owned in [
        "fn the_shell_enables_the_back_forward_swipe_gesture_on_its_webview",
        "fn a_gesture_driven_history_navigation_is_reported_into_the_core",
        "fn the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back",
    ] {
        assert!(
            guard.contains(owned),
            "the sibling guard {GESTURE_GUARD} must still carry `{owned}`: this \
             task removed the buttons on the strength of that gesture working AND \
             reporting into the core, so deleting its assertions would strand iOS \
             history navigation with nothing watching it"
        );
    }
}

#[test]
fn the_ios_toolbar_has_no_back_or_forward_buttons() {
    // Acceptance 1: the two buttons are GONE from the toolbar — the fields, the
    // glyphs, the enablement lines and the two core calls with them. The WebKit
    // edge-swipe is the history affordance now (see the sibling test above), and
    // it covers both directions, which is why iOS loses no capability the way
    // Android loses forward.
    let (code, literals) = scan(&source(SWIFT_PAINTER));
    for gone in [
        "backButton",
        "forwardButton",
        "reloadButton",
        "stopButton",
        "core.goBack()",
        "core.goForward()",
        "chrome.canGoBack",
        "chrome.canGoForward",
    ] {
        assert!(
            !code.contains(gone),
            "`{gone}` is part of the pre-collapse toolbar; the iOS edge no longer paints it"
        );
    }
    // The glyphs, including the variation selector the arrows carried (`◀︎`), so a
    // button re-added with the exact old title cannot slip past.
    for glyph in ["◀", "▶"] {
        assert!(
            !literals.iter().any(|literal| literal.contains(glyph)),
            "the `{glyph}` history-button glyph must be gone from the iOS painter"
        );
    }
}

#[test]
fn the_toolbar_carries_one_reload_stop_control_and_the_spinner() {
    // Acceptance 3/4: ONE control where there were two, plus the spinner, in the
    // order the shared decision record fixes (the two loading surfaces stay
    // together, immediately before the URL field that takes the freed width):
    // docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md.
    let (code, _) = scan(&source(SWIFT_PAINTER));
    let flat = statements(&code);
    let order = [
        "reloadStopButton,",
        "loadingSpinner,",
        "urlField,",
        "invalidBadge,",
        "menuButton,",
    ];
    let mut at = flat
        .find("UIStackView(arrangedSubviews: [")
        .expect("the toolbar must still be built from a stack view");
    for view in order {
        let found = flat[at..].find(view).unwrap_or_else(|| {
            panic!("the toolbar stack must arrange `{view}` (after the ones before it)")
        });
        at += found + view.len();
    }
    // The URL field keeps the weak hugging priority, so the width the two dropped
    // buttons and the second reload/stop button freed goes to it with no
    // arithmetic anywhere — the same "the bar absorbs it" rule the Android edge
    // gets from its layout weight.
    assert!(
        flat.contains("urlField.setContentHuggingPriority(.defaultLow, for: .horizontal)"),
        "the URL field must stay the LOW-hugging member of the toolbar row, so it \
         absorbs the freed width"
    );
    // And the spinner must keep a permanently allocated slot: `isHidden` on a
    // stack view's arranged subview REMOVES it from the row, so a load starting or
    // ending would shove the URL bar sideways under the user's finger — the
    // horizontal twin of the layout jump `loading-progress-in-the-url-bar-not-a-banner`
    // fixed vertically on this very screen.
    assert!(
        assigns(&flat, "loadingSpinner.hidesWhenStopped", "false"),
        "the spinner must keep its slot when stopped (`hidesWhenStopped = false`), \
         so no load state re-lays-out the toolbar row"
    );
    assert!(
        !flat.contains("loadingSpinner.isHidden"),
        "`isHidden` on an arranged subview REMOVES it from the stack view, which is \
         exactly the layout jump the reserved slot exists to avoid; the spinner is \
         shown and hidden by its ALPHA"
    );
}

#[test]
fn the_control_and_the_spinner_are_painted_from_the_carried_derivation() {
    // Acceptance 3/4: every value the control and the spinner wear is READ off the
    // chrome JSON the edge already decodes each refresh — the glyph, the accessible
    // name, and the spinner's visibility. The binding must decode them, and the
    // painter must assign them.
    let (binding, binding_literals) = scan(&source(SWIFT_BINDING));
    for field in CONSUMED_DERIVED_FIELDS {
        assert!(
            binding_literals.iter().any(|literal| literal == field),
            "the Swift binding must decode the carrier's `{field}` (the JSON key)"
        );
        assert!(
            binding.contains(field),
            "the Swift binding must bind `{field}` to a property the painter can read"
        );
    }

    let (painter, _) = scan(&source(SWIFT_PAINTER));
    assert!(
        painter.contains("reloadStopButton.setTitle(chrome.reloadStopControlLabel, for: .normal)"),
        "the control's glyph must be the core's `reload_stop_control(state).label()`, \
         carried as `reloadStopControlLabel` (a `UIButton` title is set, not assigned)"
    );
    assert!(
        assigns(
            &painter,
            "reloadStopButton.accessibilityLabel",
            "chrome.reloadStopControlDescription"
        ),
        "the control's ACCESSIBLE NAME must be the core's description, in the \
         platform's accessible-name slot (iOS has no hover to hang a tooltip on)"
    );
    // The spinner's visibility is the carried fact and nothing else. It travels
    // through one local (`spinnerVisible`), exactly as the progress line's
    // `progressVisible` does immediately below it, because a `UIActivityIndicatorView`
    // needs both an opacity and an animation state and they must not be allowed to
    // disagree about which fact they follow.
    assert!(
        assigns(&painter, "let spinnerVisible", "chrome.loadSpinnerVisible"),
        "the spinner must follow the carried `loadSpinnerVisible` and nothing else"
    );
    assert!(
        assigns(&painter, "loadingSpinner.alpha", "spinnerVisible ? 1 : 0"),
        "the spinner is shown and hidden by its ALPHA off that one fact, so its slot \
         stays allocated and the toolbar never re-lays-out mid-load"
    );
    assert!(
        statements(&painter).contains(
            "if spinnerVisible { loadingSpinner.startAnimating() } else { \
             loadingSpinner.stopAnimating() }"
        ),
        "the spinner must ANIMATE on the same one carried fact (and stop otherwise, \
         rather than spinning invisibly forever)"
    );
    // The FIRST paint is the core's too, so the control cannot start out wearing a
    // mode the very next refresh disagrees with — the failure mode this edge
    // actually shipped once, with a badge string set at build time.
    assert!(
        painter.contains(
            "reloadStopButton.setTitle(initialChrome.reloadStopControlLabel, for: .normal)"
        ),
        "the control's first paint must take its glyph from the core's own initial \
         chrome, never a Swift literal that happens to match"
    );
    assert!(
        assigns(
            &painter,
            "reloadStopButton.accessibilityLabel",
            "initialChrome.reloadStopControlDescription"
        ),
        "and its first accessible name likewise"
    );
}

#[test]
fn no_swift_conditional_decides_the_control_mode_or_the_spinner() {
    // Acceptance 5, THE property of this task. The Kotlin/Swift chrome twins
    // existed, drifted, and were deleted (`docs/adr/0011`); a `switch` deciding
    // reload-vs-stop, or an `if chrome.loading` deciding the spinner, would be the
    // twin coming back. The strongest expression of that: the painter does not
    // touch the RAW loading fact AT ALL any more (it used to, twice — the pair of
    // `isEnabled` lines), and neither edge file spells either mode's glyph or wire
    // name.
    let (painter, painter_literals) = scan(&source(SWIFT_PAINTER));
    assert!(
        !painter.contains("chrome.loading"),
        "the painter must not re-derive the control mode or the spinner from the raw \
         loading fact; those decisions are `reload_stop_control` / \
         `load_spinner_visible`, carried as fields"
    );
    for wire in ["chrome.reloadStopControl ", "chrome.reloadStopControl)"] {
        assert!(
            !painter.contains(wire),
            "the painter must not branch on the mode's WIRE NAME (`{wire}`); it \
             assigns the values the mode carries"
        );
    }
    // The glyphs are the core's `ReloadStopControl::label()` on every edge. A
    // literal here is a twin in the making even while it agrees, which is exactly
    // how this edge ended up with a build-time badge string that was never
    // refreshed.
    for (path, literals) in [
        (SWIFT_PAINTER, &painter_literals),
        (SWIFT_BINDING, &scan(&source(SWIFT_BINDING)).1),
    ] {
        for glyph in [
            werust_core::RELOAD_AFFORDANCE_LABEL,
            werust_core::STOP_AFFORDANCE_LABEL,
        ] {
            assert!(
                !literals.iter().any(|literal| literal.contains(glyph)),
                "{path} carries the `{glyph}` glyph as a literal; it is the core's \
                 `ReloadStopControl::label()`, carried on the chrome JSON"
            );
        }
    }
}

#[test]
fn the_control_performs_the_modes_own_action_through_the_core() {
    // Acceptance 3/8: what the control DOES is the mode's own action, resolved in
    // the core (`ReloadStopControl::action()`), exactly as the GTK edge performs it
    // through the one `perform_chrome_action`. So the tap handler decides nothing:
    // there is no Swift arm mapping a mode back onto a session method, and cancel
    // is reachable from that same one control.
    let (painter, _) = scan(&source(SWIFT_PAINTER));
    assert!(
        painter.contains("core.activateReloadStopControl()"),
        "the ONE control must activate through the core's ONE entry point"
    );
    assert!(
        painter.contains("#selector(onReloadStop)"),
        "the control's tap must be wired to that one handler"
    );
    for decided_here in ["core.reload()", "core.stop()"] {
        assert!(
            !painter.contains(decided_here),
            "`{decided_here}` at the edge means the edge decided which of the two \
             modes this tap was; that decision is the core's \
             `reload_stop_control(...).action()`"
        );
    }
    // And the binding really reaches the C-ABI export, so the handler is not a
    // no-op that compiles.
    let (binding, _) = scan(&source(SWIFT_BINDING));
    assert!(
        binding.contains("werust_ios_activate_reload_stop_control(handle)"),
        "the Swift binding must call the C-ABI export that performs the mode"
    );
    // The header the Swift edge imports must DECLARE it, or the app does not
    // build at all on the `mobile-ios` leg — the one CI evidence this edge gets.
    let header = source("crates/werust-ios/Sources/werust_mobile.h");
    assert!(
        header
            .contains("void werust_ios_activate_reload_stop_control(WerustCoreSession *session);"),
        "the bridging header must declare the new export in lock-step with the Rust \
         `extern \"C\"` fn"
    );
}

#[test]
fn the_mobile_presentation_guard_registers_the_fields_this_edge_consumes() {
    // Acceptance 6, the SEQUENCING trap, now on its far side.
    // `mobile_chrome_presentation_shape.rs` demands that BOTH mobile bindings
    // decode and BOTH painters paint every field in `DERIVED_FIELDS`. This edge was
    // the MIGRATE step for the SECOND of the two edges, so the fields only became
    // consumable-everywhere here; registering them was deliberately left to the
    // CONTRACT step, and this assertion ran the other way round to keep a
    // well-meaning registration from turning a deliberate hand-off into an
    // unreviewed one. That step
    // (`register-the-new-chrome-fields-in-the-mobile-presentation-guard`) has since
    // landed the registration and inverted it: what is worth guarding now is the
    // OPPOSITE regression, a field this edge paints being dropped from the central
    // list — the cheapest way to make that guard green after breaking an edge.
    //
    // It reads the LITERAL half of the scan, because a field name can only ever
    // appear in that guard AS a string literal (a `FACT_FIELDS` / `DERIVED_FIELDS`
    // entry, or an FFI signature string), and it demands an EXACT literal rather
    // than a substring: these field names also occur inside the guard's own prose,
    // and a comment is not a registration. The positive control below stays from
    // the pre-inversion shape, where it was what kept the check from being vacuous
    // (that exact mistake was caught in review of the Android twin,
    // `docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md`
    // section 7); it still earns its place by telling a MISSING entry apart from a
    // scan that lost its place, since the two controls straddle the file and the
    // scanned file is RUST while `scan` is written for Swift/Kotlin.
    let guard = source("crates/werust-core/tests/mobile_chrome_presentation_shape.rs");
    let (_, literals) = scan(&guard);
    for control in ["loadProgressVisible", "func loadProgressVisible()"] {
        assert!(
            literals.iter().any(|literal| literal == control),
            "POSITIVE CONTROL: `{control}` has been registered in the mobile \
             presentation guard since long before this edge (a `DERIVED_FIELDS` \
             entry, and the Swift binding signature it forbids), so this check must \
             SEE it — if it does not, the scan is broken and the assertion below is \
             reporting a missing entry it simply cannot read. The two controls \
             straddle the whole file, so a scan that lost its place partway through \
             is caught too."
        );
    }
    for field in CONSUMED_DERIVED_FIELDS {
        assert!(
            literals.iter().any(|literal| literal == field),
            "`{field}` must be registered in the mobile presentation guard's \
             `DERIVED_FIELDS`: this edge paints it, and the central guard is what \
             holds the OTHER edge to reading the same one derivation. Dropping the \
             entry would leave this field the only chrome fact crossing to mobile \
             without that protection."
        );
    }
}

#[test]
fn the_assignment_check_is_not_satisfied_by_a_longer_field_name() {
    // The guard ON the guard, part one: `chrome.reloadStopControl` is a PREFIX of
    // both fields this edge paints, so a substring check would read "the control
    // wears the mode's wire name" as "the control wears the glyph" and green-light
    // the branch-on-the-wire-name this task exists to avoid.
    let code =
        "        reloadStopButton.accessibilityLabel = chrome.reloadStopControlDescription\n";
    assert!(
        assigns(
            code,
            "reloadStopButton.accessibilityLabel",
            "chrome.reloadStopControlDescription"
        ),
        "an indented whole-statement assignment is matched"
    );
    assert!(
        !assigns(
            code,
            "reloadStopButton.accessibilityLabel",
            "chrome.reloadStopControl"
        ),
        "assigning the DESCRIPTION must not read as assigning the mode's wire name"
    );
    assert!(
        !assigns(
            code,
            "reloadStopButton.accessibilityValue",
            "chrome.reloadStopControlDescription"
        ),
        "a different SLOT is a different assignment"
    );
    // And a WRAPPED statement is the same statement: `swift-format` wraps a long
    // assignment across lines exactly as the house style does elsewhere in this
    // painter.
    let wrapped = "        loadingSpinner.alpha =\n            spinnerVisible ? 1 : 0\n";
    assert!(
        assigns(wrapped, "loadingSpinner.alpha", "spinnerVisible ? 1 : 0"),
        "a statement wrapped across lines is still that statement"
    );
}

#[test]
fn the_scanner_reads_literals_and_code_apart() {
    // The guard ON the guard, part two: every negative assertion above rests on
    // `scan` telling literals, code and comments apart. A scanner that treated a
    // doc comment as code would make "the `◀` button is gone" fail on the very
    // comment that EXPLAINS why it went; one that treated `https://` as a comment
    // would make the literal checks vacuous.
    let fixture = "\
// a comment naming the removed \u{25c0} button
/* and a block one with \"a quoted phrase\" */
let url = \"https://example.com/\" // trailing comment
let glyph = \"\u{27f3}\"
let raw = \"\"\"a raw \" one\"\"\"
let identifier = loadingSpinner
";
    let (code, literals) = scan(fixture);
    assert!(
        literals.contains(&"https://example.com/".to_string()),
        "a `//` inside a string is not a comment: {literals:?}"
    );
    assert!(
        literals.contains(&"\u{27f3}".to_string()),
        "a plain literal is collected, multi-byte glyph and all: {literals:?}"
    );
    assert!(
        literals.contains(&"a raw \" one".to_string()),
        "a raw literal is collected: {literals:?}"
    );
    assert!(
        !code.contains('\u{25c0}') && !literals.iter().any(|l| l.contains('\u{25c0}')),
        "a glyph named only in a COMMENT is documentation, not a painted affordance"
    );
    assert!(
        code.contains("let identifier = loadingSpinner"),
        "code outside comments and literals is kept: {code:?}"
    );
    assert!(
        !code.contains("example.com"),
        "literal CONTENT is not part of the code view: {code:?}"
    );
}
