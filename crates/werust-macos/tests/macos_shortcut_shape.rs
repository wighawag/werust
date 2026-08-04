//! macOS SHORTCUT wiring shape guard (task
//! `shortcuts-and-mouse-history-buttons-on-the-macos-edge`, spec
//! `chrome-conventional-controls`).
//!
//! WHAT LANDED: the AppKit edge's half of the shortcut layer. `src/input.rs`
//! translates `NSEvent` key codes, modifier flags and side-button numbers into
//! `werust_core::shortcuts`'s toolkit-neutral vocabulary; the `ShortcutWindow`
//! subclass in `src/window.rs` gives that translation the FIRST look at every
//! event (`sendEvent:`, the AppKit analogue of the GTK edge's capture phase),
//! reports which of the two focus contexts is live, and performs the returned
//! `ChromeAction` through the same `BrowserShell` calls the toolbar buttons use.
//!
//! WHY A SOURCE-SHAPE GUARD, AND WHY IT MATTERS MORE HERE: the AppKit half is
//! `#[cfg(target_os = "macos")]`, so the Ubuntu `verify` gate never compiles it,
//! and nobody on this project has a Mac
//! (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`),
//! so nobody will ever notice a wrong chord by using it. This file therefore
//! PARSES the AppKit source, exactly as the sibling `macos_window_shape.rs` does
//! for the chrome, and asserts the properties compilation would not have proven
//! anyway -- above all the one the seam exists for: that this edge contains NO
//! decision about what an input MEANS.
//!
//! WHAT IS COVERED ELSEWHERE, so this file does not duplicate it: the translation
//! TABLE (including the Cmd branch, its distinctness from the Ctrl branch, and
//! the flags a Mac really sends) is unit-tested against the REAL core in
//! `src/input.rs`, which is deliberately NOT target-gated; the chord table itself
//! lives in `crates/werust-core/src/shortcuts.rs` with its own table test; and
//! `examples/window_smoke.rs` presses real `NSEvent`s through the real window on
//! the `macos-14` leg.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. Every shortcut the shared resolution defines works here, on the platform's
//!    Cmd modifier, and the edge decides nothing
//!    (`the_macos_edge_translates_into_the_shared_resolution_and_decides_nothing`,
//!    `the_edge_names_no_key_meaning_outside_its_translation`).
//! 2. The web inspector is deliberately NOT delivered, and its absence is
//!    EXPLICIT (`the_web_inspector_is_explicitly_unhandled_because_macos_has_none`).
//! 3. The Cmd branch is genuinely exercised, by assertions a runner can make
//!    without a Mac (`the_cmd_branch_is_exercised_where_a_runner_can_see_it`,
//!    `the_macos_leg_presses_the_chords_on_a_real_window`).
//! 4. Escape behaves per focus, on focus REPORTED by this edge
//!    (`focus_is_reported_by_this_edge_and_branched_on_only_by_the_core`).
//! 5. Mouse buttons 4 and 5 navigate history
//!    (`the_side_buttons_ride_the_same_resolution_and_the_same_performer`).
//! 6. History goes through the existing seam and capability flags, unchanged
//!    (`history_rides_the_existing_seam_and_its_capability_flags`).
//! 7. Parity-tracked: the matrix row's macOS cell is implemented
//!    (`the_macos_cell_is_implemented_and_the_windows_sibling_is_still_tracked`).

use std::path::{Path, PathBuf};

use werust_core::shortcuts::ChromeAction;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-macos`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn window() -> String {
    source("crates/werust-macos/src/window.rs")
}

/// The TRANSLATION half, the one that is not target-gated.
fn input() -> String {
    source("crates/werust-macos/src/input.rs")
}

/// The PRODUCTION half of a source file: everything before its test module, so an
/// assertion about what the edge does cannot be satisfied (or tripped) by test
/// code.
fn production(text: &str) -> &str {
    match text.find("#[cfg(test)]") {
        Some(tests) => &text[..tests],
        None => text,
    }
}

/// `source` with every comment line dropped, so a "does this file mention X"
/// assertion is about the CODE and not about prose. These files legitimately
/// DISCUSS the rules they must not re-implement.
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

#[test]
fn the_macos_edge_translates_into_the_shared_resolution_and_decides_nothing() {
    // Criterion 1: the whole key path is translate -> ask the core -> perform.
    // `input::action` maps the NSEvent's numbers into the shared vocabulary and
    // calls the SHARED resolution under the platform's own accelerator
    // convention, which is the CORE's call, not a constant restated here.
    let input = input();
    let translation = production(&input);
    assert!(
        translation.contains("shortcuts::resolve_chord("),
        "the edge must ask the shared resolution what a chord means"
    );
    assert!(
        window().contains("shortcuts::resolve_pointer_button"),
        "…and what a side button means (the pointer path's own resolution call)"
    );
    assert!(
        translation.contains("shortcuts::PrimaryModifier::for_target()"),
        "the accelerator convention is the core's call, not a per-edge constant"
    );
    // The Cmd-versus-Ctrl split must not be re-minted here: this edge SELECTS a
    // primary modifier, it does not add a second branch.
    assert!(
        !code_only(translation).contains("PrimaryModifier::Meta")
            && !code_only(translation).contains("PrimaryModifier::Control"),
        "the edge must not name a convention of its own: `for_target()` is the one branch"
    );

    // The window INTERCEPTS and PERFORMS; it interprets nothing. The interception
    // is an `NSWindow` subclass's `sendEvent:`, the AppKit analogue of the GTK
    // edge's capture phase, and anything unclaimed is forwarded to `super`.
    let window = window();
    let send_event = between(
        &window,
        "fn send_event(&self, event: &NSEvent)",
        "\n        }",
    );
    assert!(
        send_event.contains("self.claim(event)") && send_event.contains("super(self), sendEvent:"),
        "an event werust does not claim must reach AppKit untouched: {send_event:?}"
    );
    let claim = between(
        &window,
        "fn claim(&self, event: &NSEvent) -> bool {",
        "\n    /// The KEY half",
    );
    assert!(
        claim.contains("NSEventType::KeyDown") && claim.contains("NSEventType::OtherMouseDown"),
        "the window must dispatch on the KIND of native event only: {claim:?}"
    );
    assert!(
        claim.contains("self.claim_key(") && claim.contains("self.claim_pointer_button("),
        "both input paths must go through the shared translation: {claim:?}"
    );
    let claim_key = between(&window, "fn claim_key(", "fn claim_pointer_button(");
    assert!(
        claim_key.contains("input::action(") && claim_key.contains("controller.shortcut_focus()"),
        "the key path must translate and REPORT focus, never branch on it: {claim_key:?}"
    );
    assert!(
        claim_key.contains("perform_chrome_action("),
        "the key path must perform what the core returned: {claim_key:?}"
    );
}

#[test]
fn the_edge_names_no_key_meaning_outside_its_translation() {
    // Criterion 1, the teeth: the ONLY place this edge may name a key is its
    // translation table. A key named in the window (`if key == Escape { stop }`)
    // would be this edge deciding what that key means, which is precisely the
    // per-edge drift `CONTEXT.md`'s ONE-derivation rule exists to prevent -- and
    // the drift nobody here could ever notice by using the product.
    let window = code_only(&window());
    for named in [
        "shortcuts::Key::",
        "Key::Escape",
        "Key::F5",
        "Key::F12",
        "Key::ArrowLeft",
        "Key::ArrowRight",
        "KEY_CODE_",
        "MODIFIER_FLAG_",
        "BUTTON_NUMBER_",
    ] {
        assert!(
            !window.contains(named),
            "`{named}` is named in the AppKit layer; keys and buttons may only be \
             spelled in `src/input.rs`, which TRANSLATES them"
        );
    }
    // The window reads the NSEvent's numbers and hands them straight over; it
    // must not compare one against anything.
    let claim = between(&window, "fn claim_key(", "fn claim_pointer_button(");
    assert!(
        !claim.contains("=="),
        "the key path must compare nothing: it translates and performs: {claim:?}"
    );

    // …and no werust chord may be minted a SECOND time as an AppKit key
    // equivalent. AppKit resolves a menu item's key equivalent itself, so a
    // `keyEquivalent` of "l" or "r" would be an edge-local shortcut table racing
    // the shared resolution for the same chord. The only key equivalent this
    // window installs is the platform's own Quit.
    assert!(
        window.contains("keyEquivalent"),
        "the app menu still installs the platform's Quit key equivalent"
    );
    let quit = between(&window, "fn install_main_menu(", "\n}\n");
    assert!(
        quit.contains("terminate:") && quit.contains("ns(\"q\")"),
        "the ONE key equivalent may be AppKit's own Quit: {quit:?}"
    );
    for chord in ["ns(\"l\")", "ns(\"r\")", "ns(\"L\")", "ns(\"R\")"] {
        assert!(
            !window.contains(chord),
            "`{chord}` is installed as an AppKit key equivalent; werust's chords are \
             resolved ONCE, in the core, never raced by a menu"
        );
    }
}

#[test]
fn the_edge_handles_every_action_the_shared_vocabulary_defines() {
    // One performer, with an arm for every action the core can resolve. Driven
    // off `ChromeAction::ALL` rather than a hand-copied list, so an action added
    // to the shared vocabulary reds here until this edge handles it or declines
    // it explicitly.
    let window = window();
    let performer = between(
        &window,
        "fn perform_chrome_action(&self, action: ChromeAction) {",
        "\n    /// Open (or raise) the debug view.",
    );
    for action in ChromeAction::ALL {
        assert!(
            performer.contains(&format!("ChromeAction::{action:?}")),
            "the macOS edge must have an arm for {action:?}: {performer:?}"
        );
    }
}

#[test]
fn the_web_inspector_is_explicitly_unhandled_because_macos_has_none() {
    // Criterion 2, and the one place this edge deliberately differs from its
    // siblings: macOS reaches no web inspector at all, so the action has an arm
    // but NO handler. The absence must be EXPLICIT rather than silent -- an empty
    // arm with the reason and the owning task written beside it -- and it must not
    // have been expressed by teaching the SHARED resolution about platforms.
    let window = window();
    let performer = between(
        &window,
        "fn perform_chrome_action(&self, action: ChromeAction) {",
        "\n    /// Open (or raise) the debug view.",
    );
    let arm = between(
        performer,
        "ChromeAction::OpenWebInspector => {",
        "\n            }",
    );
    assert!(
        arm.contains("macos-web-inspector-safari-devtools")
            && arm.contains("platform-capability-matrix"),
        "the empty arm must SAY why it is empty and who owns the gap: {arm:?}"
    );
    assert!(
        !code_only(arm).contains(';'),
        "the arm must genuinely do nothing: there is no inspector on this platform: {arm:?}"
    );
    // Nothing in this crate reaches for an inspector, so the absence is real and
    // not merely unwired at the shortcut.
    let window_code = code_only(&window);
    for api in [
        "isInspectable",
        "setInspectable",
        "WKPreferences",
        "inspector",
    ] {
        assert!(
            !window_code.contains(api),
            "`{api}` appears in the macOS window; the web inspector is owned by \
             `macos-web-inspector-safari-devtools`, not by this task"
        );
    }
    // And the SHARED resolution stayed capability-agnostic: expressing this
    // absence there would fork the resolution per platform and re-mint exactly
    // the per-edge decision the seam exists to delete. `resolve_chord` still
    // takes (chord, focus, primary) and nothing else, and its body knows about no
    // platform at all.
    let core = source("crates/werust-core/src/shortcuts.rs");
    assert!(
        core.contains(
            "pub fn resolve_chord(chord: Chord, focus: Focus, primary: PrimaryModifier) -> Option<ChromeAction>"
        ),
        "the shared resolution must not have grown a capability parameter"
    );
    let resolution = code_only(between(
        &core,
        "pub fn resolve_chord(",
        "\n/// What a mouse",
    ));
    for forked in ["target_os", "cfg!", "capability"] {
        assert!(
            !resolution.contains(forked),
            "the shared resolution must not branch on `{forked}`: {resolution:?}"
        );
    }
    // The matrix still records the gap where absences belong.
    let matrix = source("docs/platform-capability-matrix.toml");
    let row = between(&matrix, "name = \"web-inspector\"", "\n[[capability]]");
    assert!(
        row.contains(
            "macos = { state = \"stubbed\", task = \"macos-web-inspector-safari-devtools\" }"
        ),
        "the capability matrix must still track macOS's missing web inspector: {row:?}"
    );
}

#[test]
fn focus_is_reported_by_this_edge_and_branched_on_only_by_the_core() {
    // Criterion 4: Escape means two things, and which one is the CORE's call on
    // the focus this edge reports. The edge answers exactly one question ("is the
    // URL bar being typed in?"), which on AppKit means asking the CONTROL about
    // its field editor, because the first responder while typing is the editor
    // and not the field.
    let window = window();
    let reporter = between(
        &window,
        "fn shortcut_focus(&self) -> Focus {",
        "\n    /// PERFORM a resolved",
    );
    assert!(
        reporter.contains("currentEditor()") && reporter.contains("firstResponder()"),
        "focus must be read off AppKit's field editor / first responder: {reporter:?}"
    );
    assert!(
        reporter.contains("Focus::UrlBar") && reporter.contains("Focus::Page"),
        "the reporter must answer with the core's two-valued Focus: {reporter:?}"
    );
    // …and NOWHERE else in the window may look at a focus value, or this edge
    // would be growing its own Escape branch.
    let window_code = code_only(&window);
    for value in ["Focus::UrlBar", "Focus::Page"] {
        assert_eq!(
            window_code.matches(value).count(),
            code_only(reporter).matches(value).count(),
            "`{value}` may only be named where focus is REPORTED, never where the edge acts"
        );
    }
    // The core is what splits Escape, and it still does.
    let core = source("crates/werust-core/src/shortcuts.rs");
    assert!(
        core.contains("Focus::Page => ChromeAction::Stop")
            && core.contains("Focus::UrlBar => ChromeAction::RevertUrlBar"),
        "the focus-dependent Escape must still be resolved in the core"
    );
}

#[test]
fn history_rides_the_existing_seam_and_its_capability_flags() {
    // Criterion 6: a shortcut performs history EXACTLY as the toolbar button
    // does, via `BrowserShell::go_back` / `go_forward` (the existing `Renderer`
    // seam methods) gated on the existing `ChromeState` capability flags, so a
    // chord or a side button can never drive a move the on-screen control
    // refuses.
    let window = window();
    let performer = between(
        &window,
        "fn perform_chrome_action(&self, action: ChromeAction) {",
        "\n    /// Open (or raise) the debug view.",
    );
    for expected in [
        "chrome().can_go_back",
        "go_back()",
        "chrome().can_go_forward",
        "go_forward()",
        "shell.borrow_mut().reload()",
        "shell.borrow_mut().stop()",
    ] {
        assert!(
            performer.contains(expected),
            "the performer must go through `{expected}`: {performer:?}"
        );
    }

    // …and that seam is UNCHANGED by this task: the Android hardware Back button
    // and the GTK edge ride the same methods.
    let seam = source("crates/renderer/src/lib.rs");
    assert!(
        seam.contains("fn go_back(&mut self)") && seam.contains("fn go_forward(&mut self)"),
        "the Renderer seam's history methods must be unchanged"
    );
    assert!(
        !seam.contains("shortcut") && !seam.contains("Chord"),
        "the shortcut layer must not have leaked into the Renderer seam"
    );
    let core = source("crates/werust-core/src/lib.rs");
    assert!(
        core.contains("pub can_go_back: bool") && core.contains("pub can_go_forward: bool"),
        "the ChromeState capability flags must be unchanged"
    );
}

#[test]
fn the_side_buttons_ride_the_same_resolution_and_the_same_performer() {
    // Criterion 5: mouse buttons 4 and 5 navigate history, through the SAME
    // resolution and the SAME performer the keyboard uses, and the edge knows
    // only the BUTTON NUMBER (AppKit's 3 and 4 -- the core's vocabulary is named,
    // never numbered, because every toolkit numbers them differently).
    let input = input();
    assert!(
        input.contains("pub const BUTTON_NUMBER_BACK: isize = 3;")
            && input.contains("pub const BUTTON_NUMBER_FORWARD: isize = 4;"),
        "the side buttons arrive as AppKit buttonNumber 3 and 4"
    );
    let window = window();
    let pointer = between(
        &window,
        "fn claim_pointer_button(&self, button_number: isize) -> bool {",
        "\n}",
    );
    assert!(
        pointer.contains("input::pointer_button(button_number)")
            && pointer.contains("shortcuts::resolve_pointer_button")
            && pointer.contains("perform_chrome_action("),
        "the mouse path must translate, ask the core, then perform: {pointer:?}"
    );
    assert!(
        !pointer.contains("go_back()") && !pointer.contains("go_forward()"),
        "the mouse path must not decide that a button means history: {pointer:?}"
    );
}

#[test]
fn the_cmd_branch_is_exercised_where_a_runner_can_see_it() {
    // Criterion 3, and the reason this edge's translation is NOT target-gated:
    // there is no Mac on this project, so the Cmd mapping must be checkable on
    // the ordinary Ubuntu gate rather than left to "someone will notice".
    let lib = source("crates/werust-macos/src/lib.rs");
    assert!(
        !lib.contains("#[cfg(target_os = \"macos\")]\npub mod input;"),
        "`input` must NOT be target-gated: it is the half the Ubuntu gate proves"
    );
    assert!(
        lib.contains("pub mod input;"),
        "the translation half must be part of the crate"
    );

    // The unit tests beside it drive THIS edge's real translation on the Mac
    // convention, and assert the Ctrl branch is genuinely different.
    let input = input();
    let tests = &input[input
        .find("#[cfg(test)]")
        .expect("the translation half must carry unit tests")..];
    assert!(
        tests.contains("shortcuts::PrimaryModifier::Meta"),
        "the tests must drive the CMD branch of the shared resolution"
    );
    assert!(
        tests.contains("fn the_cmd_branch_is_distinct_from_the_ctrl_branch_on_this_edge"),
        "the Cmd branch's DISTINCTNESS from the Ctrl branch must be asserted, not assumed"
    );
    assert!(
        tests.contains("MODIFIER_FLAG_COMMAND") && tests.contains("MODIFIER_FLAG_CONTROL"),
        "the tests must drive real AppKit modifier bits, not abstract modifiers"
    );

    // The type-check harness (the fast local loop that keeps CI from being the
    // first place a typo is found) must cover the new module too.
    let harness =
        source("docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh");
    assert!(
        harness.contains("crates/werust-macos/src/input.rs"),
        "the local macOS type-check harness must cover the shortcut translation too"
    );
}

#[test]
fn the_macos_leg_presses_the_chords_on_a_real_window() {
    // Criterion 3, the other half: the ONLY execution this AppKit code ever gets
    // is the `macos-14` leg, so the window smoke must PRESS the chords as real
    // `NSEvent`s through the real window rather than assert on the table it
    // already trusts -- and it must carry the negative control that stops a pass
    // from being free.
    let smoke = source("crates/werust-macos/examples/window_smoke.rs");
    for driven in [
        "window.press_key(",
        "NSEventModifierFlags::Command",
        "NSEventModifierFlags::Control",
        "window.press_side_button(",
        "window.reported_focus()",
    ] {
        assert!(
            smoke.contains(driven),
            "the smoke must drive `{driven}` on the real window"
        );
    }
    // The AppKit constants and the Linux-side table must be checked against each
    // other on the one machine that has both.
    assert!(
        smoke.contains("input::MODIFIER_FLAG_COMMAND"),
        "the smoke must check the translation table's bits against AppKit's own flags"
    );
    // The event synthesis lives in the window (the AppKit layer), not in the
    // example, so the smoke drives the same `sendEvent:` a user's key press does.
    let window = window();
    assert!(
        window.contains("pub fn press_key(") && window.contains("self.window.sendEvent(&event)"),
        "a pressed key must reach the window through `sendEvent:`, as a real one does"
    );
    // The leg still runs the smoke.
    let workflow = source(".github/workflows/macos-renderer.yml");
    assert!(
        workflow.contains("--example window_smoke"),
        "the macOS job must RUN the window smoke"
    );
}

#[test]
fn the_macos_cell_is_implemented_beside_its_desktop_siblings() {
    // Criterion 7 (enforced end to end by
    // `crates/werust-core/tests/platform_capability_parity.rs`): this edge is no
    // longer a stub. Its Windows sibling
    // (`shortcuts-and-mouse-history-buttons-on-the-windows-edge`) landed while
    // this task was in flight, so all three DESKTOP cells now read implemented
    // and the mobile ones stay explicitly `n-a` with their reason, rather than
    // any edge being a silent gap.
    let matrix = source("docs/platform-capability-matrix.toml");
    let row = between(
        &matrix,
        "name = \"conventional-shortcuts\"",
        "\n[[capability]]",
    );
    for implemented in ["desktop", "macos", "windows"] {
        assert!(
            row.contains(&format!("{implemented} = {{ state = \"implemented\" }}")),
            "the {implemented} edge's shortcuts must be marked implemented: {row:?}"
        );
    }
    for mobile in ["ios", "android"] {
        assert!(
            row.contains(&format!("{mobile} = {{ state = \"n-a\", reason = ")),
            "the {mobile} edge must stay an explicit, REASONED n-a: {row:?}"
        );
    }
}
