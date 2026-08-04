//! Android collapsed reload/stop control + spinner + dropped history buttons
//! wiring shape guard (task
//! `android-chrome-collapse-reload-stop-and-drop-history-buttons`, spec
//! `chrome-conventional-controls`, stories 8-12).
//!
//! WHAT LANDED: the Android toolbar lost its on-screen `◀` and `▶` buttons (the
//! SYSTEM Back already navigates page history, and forward has no gesture
//! equivalent — accepted by the spec, which values the width more), and its
//! separate Reload and Stop buttons became the ONE control the core derives, with
//! a loading spinner beside it. Every value the edge paints is read off the
//! chrome JSON it already decodes each refresh (`reloadStopControlLabel`,
//! `reloadStopControlDescription`, `loadSpinnerVisible`); the freed width goes to
//! the weighted URL bar, which needs no code at all.
//!
//! WHY A SOURCE-SHAPE GUARD: the painter is Kotlin inside a live Android
//! `Activity`, and this repo's `verify` gate is pure Rust (`cargo fmt && clippy
//! && build && test`, no Android SDK, and the Gradle/Kotlin build is not in the
//! gate either), so nothing else in the gate can see this wiring at all. The
//! DERIVATION is unit-tested where it lives (`werust_core::reload_stop_control` /
//! `load_spinner_visible`, over every `ChromeState` shape) and the carrier is
//! asserted to agree with it verbatim; what neither can see is whether this EDGE
//! reads them, or whether a Kotlin `when` came back to decide the mode. That twin
//! is the exact failure this repo has already deleted once
//! (`mobile-chrome-presentation-from-one-derivation`, `docs/adr/0011`), so it is
//! guarded here in the same spirit as the sibling guards
//! `system_back_wiring_shape.rs` and
//! `crates/werust-core/tests/collapsed_reload_stop_control_shape.rs`.
//!
//! Acceptance criteria mapped to assertions below:
//! - The toolbar no longer shows on-screen back or forward buttons
//!   (`the_android_toolbar_has_no_on_screen_back_or_forward_buttons`).
//! - The system Back still navigates page history — ASSERTED, not assumed, since
//!   it not doing so was a field-reported bug once already
//!   (`the_system_back_affordance_survives_the_removal_of_the_on_screen_buttons`).
//! - Reload and Stop are ONE control whose mode is the core's, and cancelling an
//!   in-flight load is still possible
//!   (`the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight`,
//!   `the_toolbar_carries_one_reload_stop_control_and_the_spinner`,
//!   `the_control_performs_the_modes_own_action_through_the_core`).
//! - A spinner shows while loading, its visibility read from the chrome JSON
//!   (`the_control_and_the_spinner_are_painted_from_the_carried_derivation`).
//! - No Kotlin conditional decides the control mode or the spinner's visibility
//!   (`no_kotlin_conditional_decides_the_control_mode_or_the_spinner`).
//! - `can_go_back` / `can_go_forward` and the history seam are unchanged; only
//!   the painter changes
//!   (`the_history_capability_is_untouched_by_the_button_removal`).
//! - The mobile presentation guard's field lists are NOT touched here (this is
//!   the MIGRATE step; `register-the-new-chrome-fields-in-the-mobile-presentation-guard`
//!   owns the registration once BOTH mobile edges consume the fields)
//!   (`the_mobile_presentation_guard_field_lists_are_not_registered_here`).

use std::path::{Path, PathBuf};

use renderer::LoadState;
use werust_core::{reload_stop_control, ReloadStopControl};
use werust_mobile::CoreSession;

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-android/rust`, so the root is three levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const KOTLIN_PAINTER: &str =
    "crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt";
const KOTLIN_BINDING: &str =
    "crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt";

/// The DERIVED chrome-JSON fields this edge starts consuming here. They are
/// deliberately NOT registered in the mobile presentation guard's `DERIVED_FIELDS`
/// yet (see `the_mobile_presentation_guard_field_lists_are_not_registered_here`);
/// this list is what THIS edge's own guard demands in the meantime.
const CONSUMED_DERIVED_FIELDS: &[&str] = &[
    "reloadStopControlLabel",
    "reloadStopControlDescription",
    "loadSpinnerVisible",
];

/// The Kotlin source with COMMENTS removed and every STRING LITERAL collected,
/// so an assertion can tell wiring from documentation.
///
/// Both halves are load-bearing here. The negative assertions ("no `◀` any more",
/// "nothing reads the raw loading fact") must not trip on the KDoc that EXPLAINS
/// the removal — documentation naming a deleted affordance is documentation, and
/// this file is comment-heavy by house style. The literal half is what proves the
/// control's glyph is the core's `reloadStopControlLabel` rather than a `⟳`
/// hand-written here.
///
/// The same scanner shape the sibling guards use (`//` line comments, `/* */`
/// blocks, `"` strings with backslash escapes, `"""` raw strings); a `//` inside
/// a string is not a comment, which is why this is a scanner and not a regex.
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
        // Push the whole character, not one byte: this source carries `◀`, `⟳`
        // and friends, and slicing mid-char would panic.
        let ch = source[i..].chars().next().expect("a char at a boundary");
        code.push(ch);
        i += ch.len_utf8();
    }
    (code, literals)
}

/// The scanned code with every run of whitespace collapsed to one space, so an
/// assertion about a STATEMENT is immune to where the house style happens to wrap
/// it. (`loadingSpinner.visibility = …` is wrapped across two lines exactly as
/// its `loadingProgress` sibling is.)
fn statements(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `code` contains `target = value` as a complete assignment: the value
/// must END there, so assigning a LONGER field of the same prefix does not count.
///
/// That boundary is the whole point: `chrome.reloadStopControl` is a prefix of
/// `chrome.reloadStopControlLabel` AND of `chrome.reloadStopControlDescription`,
/// so a plain `contains` could not tell the mode's wire name from the glyph the
/// button actually wears. Pinned by
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

/// Settle a load the way the Kotlin edge's `WebViewClient` callbacks do, so the
/// core reaches the state a real page arrival leaves it in. No network: the
/// signals are simulated, exactly as the sibling guards do it.
fn settle(session: &mut CoreSession) {
    let url = session
        .take_pending_load()
        .expect("a pending load for the WebView");
    session.on_page_committed(&url);
    session.on_page_finished(&url);
}

#[test]
fn the_one_control_reloads_a_settled_page_and_stops_a_load_in_flight() {
    // The BEHAVIOUR behind the collapse, headless: ONE control that reloads a
    // settled page and cancels a load in flight, driven through the single core
    // entry point the Kotlin click handler calls
    // (`CoreSession::activate_reload_stop_control`). The mode is the core's
    // (`reload_stop_control`), so this asserts the entry point performs exactly
    // what the mode names — the edge chooses nothing.
    //
    // Network-isolated: nothing leaves the process; the WebView's load signals
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
    // `can_go_forward` and the history seam are exactly as they were — the system
    // Back rides on them (and so do the desktop shortcuts), so a regression here
    // would take the ONLY remaining Android back affordance with it.
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
        "the forward CAPABILITY survives even though Android now paints no forward \
         affordance (spec `chrome-conventional-controls`, story 11: the width is worth more)"
    );
}

#[test]
fn the_android_toolbar_has_no_on_screen_back_or_forward_buttons() {
    // Acceptance 1: the two buttons are GONE from the toolbar — the fields, the
    // glyphs, the enablement lines and the forward core call with them. The
    // platform's own Back is the back affordance now (see the sibling test), and
    // forward has no gesture equivalent, which the spec accepts.
    let (code, literals) = scan(&source(KOTLIN_PAINTER));
    for gone in [
        "backButton",
        "forwardButton",
        "reloadButton",
        "stopButton",
        "core.goForward()",
        "chrome.canGoForward",
    ] {
        assert!(
            !code.contains(gone),
            "`{gone}` is part of the pre-collapse toolbar; the Android edge no longer paints it"
        );
    }
    for glyph in ["◀", "▶"] {
        assert!(
            !literals.iter().any(|literal| literal.contains(glyph)),
            "the `{glyph}` history-button glyph must be gone from the Android edge"
        );
    }
}

#[test]
fn the_system_back_affordance_survives_the_removal_of_the_on_screen_buttons() {
    // Acceptance 2, ASSERTED rather than assumed: dropping the on-screen `◀` is
    // only safe because the SYSTEM Back button already navigates page history,
    // and system Back NOT doing that was a FIELD-REPORTED bug once already
    // (v0.2.5, task `android-hardware-back-button-navigates-history`). After this
    // task it is the ONLY back affordance Android has, so its wiring is now
    // load-bearing on its own rather than a second view of the button's.
    //
    // The full guard (the dispatcher registration, the brace-bounded handler
    // body, the deprecated-API exclusion, and the headless `can_go_back` drive)
    // is `system_back_wiring_shape.rs`; what THIS task must pin is that its three
    // load-bearing lines survived the button removal.
    let (code, _) = scan(&source(KOTLIN_PAINTER));
    assert!(
        code.contains("onBackPressedDispatcher.addCallback(this, systemBackCallback)"),
        "the system-Back callback must still be registered on the dispatcher"
    );
    assert!(
        assigns(&code, "systemBackCallback.isEnabled", "chrome.canGoBack"),
        "the system-Back callback's enablement must still be the core's `canGoBack` — it is \
         the ONLY back affordance left, so losing this line loses Back entirely"
    );
    assert!(
        code.contains("driveCore { core.goBack() }"),
        "system Back must still drive the core's `go_back` off the UI thread"
    );

    // And the fact it reads is still the core's, at the two boundaries that
    // decide navigate-vs-exit.
    let mut s = CoreSession::new();
    assert!(
        !s.chrome().can_go_back,
        "no history: the callback is disabled and Back exits, as a browser does"
    );
    assert!(s.navigate("https://a.example/"));
    settle(&mut s);
    assert!(s.navigate("https://b.example/"));
    settle(&mut s);
    assert!(
        s.chrome().can_go_back,
        "history: the callback is enabled and Back navigates it"
    );
    s.go_back();
    settle(&mut s);
    assert_eq!(
        s.chrome().url_text,
        "https://a.example/",
        "and it really goes back a page"
    );
}

#[test]
fn the_toolbar_carries_one_reload_stop_control_and_the_spinner() {
    // Acceptance 3/4: ONE control where there were two, plus the spinner, in the
    // order the shared decision record fixes (the two loading surfaces stay
    // together, immediately before the URL bar that takes the freed width):
    // docs/spikes/reload-stop-collapse-and-loading-spinner-core-and-gtk/DECISIONS.md.
    let (code, _) = scan(&source(KOTLIN_PAINTER));
    let flat = statements(&code);
    let order = [
        "toolbar.addView(reloadStopButton)",
        "toolbar.addView(loadingSpinner)",
        "toolbar.addView(urlBar)",
    ];
    let mut at = 0usize;
    for view in order {
        let found = flat[at..]
            .find(view)
            .unwrap_or_else(|| panic!("the toolbar must add `{view}` (after the ones before it)"));
        at += found + view.len();
    }
    // The URL bar keeps the weight, so the width the two dropped buttons and the
    // second control freed goes to it with no arithmetic anywhere.
    assert!(
        flat.contains("layoutParams = LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)"),
        "the URL bar must stay the WEIGHTED member of the toolbar row, so it absorbs the \
         freed width"
    );
}

#[test]
fn the_control_and_the_spinner_are_painted_from_the_carried_derivation() {
    // Acceptance 3/4: every value the control and the spinner wear is READ off
    // the chrome JSON the edge already decodes each refresh — the glyph, the
    // accessible name, and the spinner's visibility. The binding must decode
    // them, and the painter must assign them.
    let (binding, binding_literals) = scan(&source(KOTLIN_BINDING));
    for field in CONSUMED_DERIVED_FIELDS {
        assert!(
            binding_literals.iter().any(|literal| literal == field),
            "the Kotlin binding must decode the carrier's `{field}` (the JSON key)"
        );
        assert!(
            binding.contains(field),
            "the Kotlin binding must bind `{field}` to a property the painter can read"
        );
    }

    let (painter, _) = scan(&source(KOTLIN_PAINTER));
    assert!(
        assigns(
            &painter,
            "reloadStopButton.text",
            "chrome.reloadStopControlLabel"
        ),
        "the control's glyph must be the core's `reload_stop_control(state).label()`, carried \
         as `reloadStopControlLabel`"
    );
    assert!(
        assigns(
            &painter,
            "reloadStopButton.contentDescription",
            "chrome.reloadStopControlDescription"
        ),
        "the control's ACCESSIBLE NAME must be the core's description, in the platform's \
         accessible-name slot (Android has no hover to hang a tooltip on)"
    );
    assert!(
        assigns(
            &painter,
            "loadingSpinner.visibility",
            "if (chrome.loadSpinnerVisible) View.VISIBLE else View.INVISIBLE"
        ),
        "the spinner's visibility must be the carried `loadSpinnerVisible` and nothing else; \
         INVISIBLE (never GONE) keeps its slot allocated, so starting a load cannot shove the \
         URL bar sideways"
    );
    // The FIRST paint is the core's too, so the control cannot start out wearing
    // a mode the very next refresh disagrees with.
    assert!(
        painter.contains("compactNavButton(initialChrome.reloadStopControlLabel)"),
        "the control's first paint must take its glyph from the core's own initial chrome, \
         never a Kotlin literal that happens to match"
    );
}

#[test]
fn no_kotlin_conditional_decides_the_control_mode_or_the_spinner() {
    // Acceptance 5, THE property of this task. The Kotlin/Swift chrome twins
    // existed, drifted, and were deleted (`docs/adr/0011`); a `when` deciding
    // reload-vs-stop, or an `if (chrome.loading)` deciding the spinner, would be
    // the twin coming back. The strongest expression of that: the painter does
    // not touch the RAW loading fact AT ALL any more (it used to, twice — the
    // pair of `isEnabled` lines), and neither edge file spells either mode's
    // glyph or wire name.
    let (painter, painter_literals) = scan(&source(KOTLIN_PAINTER));
    assert!(
        !painter.contains("chrome.loading"),
        "the painter must not re-derive the control mode or the spinner from the raw loading \
         fact; those decisions are `reload_stop_control` / `load_spinner_visible`, carried as \
         fields"
    );
    for wire in ["chrome.reloadStopControl ", "chrome.reloadStopControl)"] {
        assert!(
            !painter.contains(wire),
            "the painter must not branch on the mode's WIRE NAME (`{wire}`); it assigns the \
             values the mode carries"
        );
    }
    // The glyphs are the core's `ReloadStopControl::label()` on every edge. A
    // literal here is a twin in the making even while it agrees, which is exactly
    // how iOS ended up with a build-time badge string that was never refreshed.
    for (path, literals) in [
        (KOTLIN_PAINTER, &painter_literals),
        (KOTLIN_BINDING, &scan(&source(KOTLIN_BINDING)).1),
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
    // the core (`ReloadStopControl::action()`), exactly as the GTK edge performs
    // it through the one `perform_chrome_action`. So the click handler decides
    // nothing: there is no Kotlin arm mapping a mode back onto a session method,
    // and cancel is reachable from the same one control.
    //
    // It goes through `driveCore` like every other session-driving action, so the
    // ANR fix (task `android-anr-main-thread-diagnose-and-unblock`) is not
    // regressed by the collapse.
    let (painter, _) = scan(&source(KOTLIN_PAINTER));
    assert!(
        painter.contains("driveCore { core.activateReloadStopControl() }"),
        "the ONE control must activate through the core's ONE entry point, off the UI thread"
    );
    for decided_here in ["core.reload()", "core.stop()"] {
        assert!(
            !painter.contains(decided_here),
            "`{decided_here}` at the edge means the edge decided which of the two modes this \
             click was; that decision is the core's `reload_stop_control(...).action()`"
        );
    }
}

#[test]
fn the_mobile_presentation_guard_field_lists_are_not_registered_here() {
    // Acceptance 6, the SEQUENCING trap. `mobile_chrome_presentation_shape.rs`
    // demands that BOTH mobile bindings decode and BOTH painters paint every
    // field in `DERIVED_FIELDS`. This task is the MIGRATE step for ONE of the two
    // edges, so registering the new fields here would red the gate until the iOS
    // task lands. The CONTRACT step is
    // `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, which is
    // blocked on both edges; this assertion is what keeps a well-meaning
    // registration from creeping in early.
    //
    // It reads the LITERAL half of the scan, and that is the whole difference
    // between a real guard and a vacuous one: a field name can only ever appear
    // in that guard AS a string literal (a `FACT_FIELDS` / `DERIVED_FIELDS`
    // entry, or an FFI signature string), so asserting its absence from the
    // literal-STRIPPED code view would be an assertion that can never fail. The
    // positive control below pins that this check really does see a registered
    // field; it straddles the file (a `DERIVED_FIELDS` entry near the top, the
    // binding signature it demands near the bottom) because the scanned file is
    // RUST and `scan` is written for Kotlin, so a construct it does not model
    // (a byte-char literal such as `b'\"'`) could in principle shift the split
    // partway through.
    let guard = source("crates/werust-core/tests/mobile_chrome_presentation_shape.rs");
    let (_, literals) = scan(&guard);
    for control in ["loadProgressVisible", "fun loadProgressVisible()"] {
        assert!(
            literals.iter().any(|literal| literal == control),
            "POSITIVE CONTROL: `{control}` IS registered in the mobile presentation guard \
             (a `DERIVED_FIELDS` entry, and the binding signature it demands), so this check \
             must SEE it — otherwise the assertion below could never fail and the \
             MIGRATE/CONTRACT sequencing would be unguarded. The two controls straddle the \
             whole file, so a scan that lost its place partway through is caught too."
        );
    }
    for field in CONSUMED_DERIVED_FIELDS {
        assert!(
            !literals.iter().any(|literal| literal.contains(field)),
            "`{field}` must NOT be registered in the mobile presentation guard yet: the guard \
             requires BOTH mobile edges to consume a field, and the iOS edge \
             (`ios-chrome-collapse-reload-stop-and-drop-history-buttons`) has not landed. The \
             fan-in task `register-the-new-chrome-fields-in-the-mobile-presentation-guard` \
             owns this registration."
        );
    }
}

#[test]
fn the_assignment_check_is_not_satisfied_by_a_longer_field_name() {
    // The guard ON the guard, part one: `chrome.reloadStopControl` is a PREFIX of
    // both fields this edge paints, so a substring check would read "the button
    // wears the mode's wire name" as "the button wears the glyph" and green-light
    // the branch-on-the-wire-name this task exists to avoid.
    let code = "        reloadStopButton.text = chrome.reloadStopControlLabel\n";
    assert!(
        assigns(
            code,
            "reloadStopButton.text",
            "chrome.reloadStopControlLabel"
        ),
        "an indented whole-statement assignment is matched"
    );
    assert!(
        !assigns(code, "reloadStopButton.text", "chrome.reloadStopControl"),
        "assigning the LABEL must not read as assigning the mode's wire name"
    );
    assert!(
        !assigns(
            code,
            "reloadStopButton.contentDescription",
            "chrome.reloadStopControlLabel"
        ),
        "a different SLOT is a different assignment"
    );
    // And a WRAPPED statement is the same statement: the house style wraps the
    // spinner's visibility line exactly as it wraps its `loadingProgress` sibling.
    let wrapped = "        loadingSpinner.visibility =\n            if (chrome.loadSpinnerVisible) View.VISIBLE else View.INVISIBLE\n";
    assert!(
        assigns(
            wrapped,
            "loadingSpinner.visibility",
            "if (chrome.loadSpinnerVisible) View.VISIBLE else View.INVISIBLE"
        ),
        "a statement wrapped across lines is still that statement"
    );
}

#[test]
fn the_scanner_reads_literals_and_code_apart() {
    // The guard ON the guard, part two: every negative assertion above rests on
    // `scan` telling literals, code and comments apart. A scanner that treated a
    // KDoc line as code would make "the `◀` button is gone" fail on the very
    // comment that EXPLAINS why it went; one that treated `https://` as a comment
    // would make the literal checks vacuous.
    let fixture = "\
// a comment naming the removed \u{25c0} button
/* and a block one with \"a quoted phrase\" */
val url = \"https://example.com/\" // trailing comment
val glyph = \"\u{27f3}\"
val raw = \"\"\"a raw \" one\"\"\"
val identifier = loadingSpinner
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
        code.contains("val identifier = loadingSpinner"),
        "code outside comments and literals is kept: {code:?}"
    );
    assert!(
        !code.contains("example.com"),
        "literal CONTENT is not part of the code view: {code:?}"
    );
}
