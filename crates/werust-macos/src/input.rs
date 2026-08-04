//! The AppKit INPUT TRANSLATION: `NSEvent` key codes, modifier flags and mouse
//! button numbers, turned into the toolkit-free vocabulary
//! [`werust_core::shortcuts`] speaks.
//!
//! TRANSLATION ONLY, and that is the whole point. What a chord MEANS was decided
//! once, in the core (`resolve_chord` / `resolve_pointer_button`, task
//! `shortcut-resolution-in-core-and-the-gtk-edge`, spec
//! `chrome-conventional-controls`); this module says only WHICH key or button a
//! native macOS event carries, and [`window`](crate::window) performs whatever
//! comes back. An edge that decided what a chord means is the per-edge drift
//! `CONTEXT.md`'s ONE-derivation rule (its "shortcut resolution" entry) exists to
//! prevent.
//!
//! # Why this half is NOT `#[cfg(target_os = "macos")]`
//!
//! Nobody on this project has a Mac
//! (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`),
//! so CI is the only evidence this edge will ever get, and the Ubuntu `verify`
//! gate is the cheapest and most-run leg there is. So the translation TABLE takes
//! plain numbers -- a `u16` virtual key code, the `NSEvent` modifier bits as a
//! `u64`, an `isize` button number -- rather than an `NSEvent`, exactly as
//! [`crate::paint`] takes a `ChromeState` rather than an `NSTextField`. The whole
//! Cmd mapping is therefore unit-tested by an ordinary `cargo test` against the
//! REAL core, and [`window`](crate::window) is left with one line per event: read
//! the numbers off the `NSEvent` and ask this module.
//!
//! The AppKit half -- WHICH events are intercepted, and where -- is in
//! [`window`](crate::window), and it is exercised on the `macos-14` leg by
//! `examples/window_smoke.rs`, which synthesises real `NSEvent`s and sends them
//! through the real window.
//!
//! # Named keys come from the KEY CODE, letters from the CHARACTERS
//!
//! macOS hands an edge both a hardware virtual key code (`keyCode`, a physical
//! position) and the character the active layout produces
//! (`charactersIgnoringModifiers`). This module reads the key code for the keys
//! that HAVE a fixed physical position (Escape, F5, F12, the arrows) and the
//! character for letters, whose code moves with the layout. The trade, and the
//! rest of this edge's judgement calls, are recorded in
//! `docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md`.

use werust_core::shortcuts::{self, Chord, ChromeAction, Focus, Modifiers, PointerButton};

/// Escape's macOS virtual key code (`kVK_Escape`).
///
/// These are the Carbon `kVK_*` constants every macOS keyboard reports for a
/// PHYSICAL key position, unchanged by the active layout, which is why the named
/// keys are matched on them rather than on the characters they produce.
pub const KEY_CODE_ESCAPE: u16 = 0x35;
/// F5's macOS virtual key code (`kVK_F5`).
pub const KEY_CODE_F5: u16 = 0x60;
/// F12's macOS virtual key code (`kVK_F12`).
pub const KEY_CODE_F12: u16 = 0x6F;
/// The Left arrow's macOS virtual key code (`kVK_LeftArrow`).
pub const KEY_CODE_ARROW_LEFT: u16 = 0x7B;
/// The Right arrow's macOS virtual key code (`kVK_RightArrow`).
pub const KEY_CODE_ARROW_RIGHT: u16 = 0x7C;

/// `NSEventModifierFlagShift`.
///
/// The four modifier bits a werust shortcut can use, spelled as plain bits so
/// this module needs no AppKit and the Ubuntu gate can drive it. They are the
/// documented `NSEventModifierFlags` values; [`crate::window`] passes
/// `NSEvent::modifierFlags()` straight through, and the macOS leg checks the two
/// spellings agree by building its smoke events out of the real
/// `NSEventModifierFlags`.
pub const MODIFIER_FLAG_SHIFT: u64 = 1 << 17;
/// `NSEventModifierFlagControl`.
pub const MODIFIER_FLAG_CONTROL: u64 = 1 << 18;
/// `NSEventModifierFlagOption` (the Alt/Option key).
pub const MODIFIER_FLAG_OPTION: u64 = 1 << 19;
/// `NSEventModifierFlagCommand` (Cmd, the Mac's primary accelerator).
pub const MODIFIER_FLAG_COMMAND: u64 = 1 << 20;

/// AppKit's `buttonNumber` for the rear side button, engraved Back ("button 4").
///
/// AppKit numbers the buttons 0 = left, 1 = right, 2 = middle, so the two side
/// buttons a browser honours arrive as 3 and 4 on an
/// `NSEventTypeOtherMouseDown`. The core's vocabulary is NAMED rather than
/// numbered precisely because every toolkit numbers them differently (GDK 8/9,
/// Win32 `XBUTTON1`/`XBUTTON2`).
pub const BUTTON_NUMBER_BACK: isize = 3;
/// AppKit's `buttonNumber` for the forward side button ("button 5").
pub const BUTTON_NUMBER_FORWARD: isize = 4;

/// Which KEY a macOS key event carries, or [`None`] for a key the shared
/// vocabulary has no name for (which the edge must then pass on untouched).
///
/// `characters` is `NSEvent::charactersIgnoringModifiers()`: the character the
/// active layout produces for the key with the modifiers taken off, so a Cmd+L
/// press arrives as `"l"`. Only its FIRST character is looked at; a dead key or a
/// bare modifier reports nothing at all.
#[must_use]
pub fn key(key_code: u16, characters: Option<&str>) -> Option<shortcuts::Key> {
    match key_code {
        KEY_CODE_ESCAPE => Some(shortcuts::Key::Escape),
        KEY_CODE_F5 => Some(shortcuts::Key::F5),
        KEY_CODE_F12 => Some(shortcuts::Key::F12),
        KEY_CODE_ARROW_LEFT => Some(shortcuts::Key::ArrowLeft),
        KEY_CODE_ARROW_RIGHT => Some(shortcuts::Key::ArrowRight),
        _ => characters
            .and_then(|text| text.chars().next())
            .map(shortcuts::Key::Character),
    }
}

/// Which MODIFIERS a macOS event's `modifierFlags` carry.
///
/// Only the four a shortcut can use are carried across. Everything else AppKit
/// reports is dropped here, and on macOS that is load-bearing rather than tidy:
/// `NSEventModifierFlagFunction` is set on EVERY arrow and function key, and
/// `NSEventModifierFlagNumericPad` on every arrow, so a translation that passed
/// the raw flags on would make Cmd+Left and F12 unmatchable against the core's
/// EXACT modifier comparison. Caps Lock is dropped for the same reason the GTK
/// edge drops it: a lock key being on must never stop a chord firing.
///
/// Option is reported as the core's `alt` and Command as its `meta`, the W3C UI
/// Events spelling the shared vocabulary uses.
#[must_use]
pub fn modifiers(flags: u64) -> Modifiers {
    Modifiers {
        control: flags & MODIFIER_FLAG_CONTROL != 0,
        alt: flags & MODIFIER_FLAG_OPTION != 0,
        shift: flags & MODIFIER_FLAG_SHIFT != 0,
        meta: flags & MODIFIER_FLAG_COMMAND != 0,
    }
}

/// The whole translation, in the shared vocabulary's own word: the [`Chord`] a
/// macOS key event carries, or [`None`] when it carries none this edge can spell.
///
/// Deliberately SEPARATE from [`action`], and public, because the split is what
/// makes the Cmd mapping checkable without a Mac: a chord is a fact about the
/// native event, so it can be produced on any host and then resolved on EITHER
/// accelerator convention. The unit tests below drive exactly that -- this edge's
/// real translation, resolved on [`shortcuts::PrimaryModifier::Meta`] -- on the
/// Ubuntu gate.
#[must_use]
pub fn chord(key_code: u16, characters: Option<&str>, flags: u64) -> Option<Chord> {
    Some(Chord::new(key(key_code, characters)?, modifiers(flags)))
}

/// What a macOS key press MEANS: this edge's native event, translated and handed
/// to the SHARED resolution.
///
/// Exactly [`chord`] composed with [`shortcuts::resolve_chord`] under the
/// platform's own accelerator convention, and nothing else. The convention is the
/// CORE's call ([`shortcuts::PrimaryModifier::for_target`], which answers Meta on
/// a Mac), so the Cmd-versus-Ctrl split is not restated here -- this edge selects
/// its primary modifier, it does not add a second branch.
#[must_use]
pub fn action(
    key_code: u16,
    characters: Option<&str>,
    flags: u64,
    focus: Focus,
) -> Option<ChromeAction> {
    shortcuts::resolve_chord(
        chord(key_code, characters, flags)?,
        focus,
        shortcuts::PrimaryModifier::for_target(),
    )
}

/// Which SIDE BUTTON an `NSEventTypeOtherMouseDown` carries, or [`None`] for the
/// ordinary buttons the page keeps (middle-click included).
#[must_use]
pub fn pointer_button(button_number: isize) -> Option<PointerButton> {
    match button_number {
        BUTTON_NUMBER_BACK => Some(PointerButton::Back),
        BUTTON_NUMBER_FORWARD => Some(PointerButton::Forward),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arrow and function keys arrive with AppKit's Function bit set (and the
    /// arrows with NumericPad too) whatever else is held. Every arrow/function
    /// case below carries them, so the tests exercise the real flag soup a Mac
    /// delivers rather than a tidied-up one.
    const FUNCTION_BITS: u64 = (1 << 23) | (1 << 21);
    /// AppKit's Caps Lock bit, which must never break a chord.
    const CAPS_LOCK_BIT: u64 = 1 << 16;

    /// THIS EDGE's real translation, resolved on the MAC convention: the exact
    /// composition [`action`] runs on a Mac, driven from any host because the core
    /// keeps the Cmd-versus-Ctrl split as a PARAMETER rather than a `cfg!`.
    ///
    /// This is what lets the Cmd branch be exercised through a real edge on the
    /// Linux gate, which matters more here than anywhere else in the repo: nobody
    /// on this project has a Mac, so "someone will notice" is not available as a
    /// verification strategy.
    fn mac_action(
        key_code: u16,
        characters: Option<&str>,
        flags: u64,
        focus: Focus,
    ) -> Option<ChromeAction> {
        shortcuts::resolve_chord(
            chord(key_code, characters, flags)?,
            focus,
            shortcuts::PrimaryModifier::Meta,
        )
    }

    #[test]
    fn the_mac_accelerator_is_cmd_where_a_mac_user_expects_cmd() {
        // THE criterion for this edge (story 4): every shortcut the shared
        // resolution defines, reached through this edge's own NSEvent
        // translation, with Cmd where a Mac user expects Cmd.
        let table = [
            // Story 1: reach the URL bar without the mouse.
            (
                (0, Some("l"), MODIFIER_FLAG_COMMAND),
                Focus::Page,
                Some(ChromeAction::FocusUrlBar),
            ),
            // Story 2: reload, both ways.
            (
                (0, Some("r"), MODIFIER_FLAG_COMMAND),
                Focus::Page,
                Some(ChromeAction::Reload),
            ),
            (
                (KEY_CODE_F5, Some("\u{f708}"), FUNCTION_BITS),
                Focus::Page,
                Some(ChromeAction::Reload),
            ),
            // Story 3: history from the keyboard, on the MAC's chord (Safari and
            // Chrome use Cmd+Arrow, not Option+Arrow).
            (
                (
                    KEY_CODE_ARROW_LEFT,
                    Some("\u{f702}"),
                    MODIFIER_FLAG_COMMAND | FUNCTION_BITS,
                ),
                Focus::Page,
                Some(ChromeAction::GoBack),
            ),
            (
                (
                    KEY_CODE_ARROW_RIGHT,
                    Some("\u{f703}"),
                    MODIFIER_FLAG_COMMAND | FUNCTION_BITS,
                ),
                Focus::Page,
                Some(ChromeAction::GoForward),
            ),
            // Story 5 / 6: Escape, by the focus this edge reports.
            (
                (KEY_CODE_ESCAPE, Some("\u{1b}"), 0),
                Focus::Page,
                Some(ChromeAction::Stop),
            ),
            (
                (KEY_CODE_ESCAPE, Some("\u{1b}"), 0),
                Focus::UrlBar,
                Some(ChromeAction::RevertUrlBar),
            ),
            // Story 15: F12 still RESOLVES here. macOS reaches no web inspector
            // at all, so this edge has no HANDLER for the action -- which is the
            // capability-agnostic rule working, not a gap. See the module docs on
            // `crate::window`'s performer.
            (
                (KEY_CODE_F12, Some("\u{f70f}"), FUNCTION_BITS),
                Focus::Page,
                Some(ChromeAction::OpenWebInspector),
            ),
        ];
        for ((key_code, characters, flags), focus, expected) in table {
            assert_eq!(
                mac_action(key_code, characters, flags, focus),
                expected,
                "key code {key_code:#x} + flags {flags:#x} with {focus:?} focused"
            );
        }
    }

    #[test]
    fn the_cmd_branch_is_distinct_from_the_ctrl_branch_on_this_edge() {
        // Acceptance: the Cmd branch's DISTINCTNESS from the Ctrl branch is
        // ASSERTED, not merely used. A Mac user's Ctrl+L must NOT focus the URL
        // bar (in a Cocoa text field Ctrl+L is a text-editing binding), and
        // Option+Arrow must NOT navigate history (it is word-wise caret movement).
        // If this edge ever asked for the wrong `PrimaryModifier`, these are what
        // go red -- on the Ubuntu gate, with no Mac in sight.
        for (key_code, characters, flags, what) in [
            (0, Some("l"), MODIFIER_FLAG_CONTROL, "Ctrl+L"),
            (0, Some("r"), MODIFIER_FLAG_CONTROL, "Ctrl+R"),
            (
                KEY_CODE_ARROW_LEFT,
                Some("\u{f702}"),
                MODIFIER_FLAG_OPTION | FUNCTION_BITS,
                "Option+Left",
            ),
            (
                KEY_CODE_ARROW_RIGHT,
                Some("\u{f703}"),
                MODIFIER_FLAG_OPTION | FUNCTION_BITS,
                "Option+Right",
            ),
        ] {
            assert_eq!(
                mac_action(key_code, characters, flags, Focus::Page),
                None,
                "{what} is the CTRL platform's chord, not a Mac's"
            );
        }
        // …and the same inputs DO resolve on the Ctrl convention, so the two
        // branches are genuinely different rather than this edge simply refusing
        // everything.
        let ctrl_l = chord(0, Some("l"), MODIFIER_FLAG_CONTROL).expect("Ctrl+L is a chord");
        assert_eq!(
            shortcuts::resolve_chord(ctrl_l, Focus::Page, shortcuts::PrimaryModifier::Control),
            Some(ChromeAction::FocusUrlBar),
            "the same translated chord is the URL-bar shortcut on a Ctrl platform"
        );
    }

    #[test]
    fn the_platform_convention_is_the_cores_call_and_this_edge_adds_nothing() {
        // The edge must not restate "a Mac is the Cmd platform", and must not
        // wrap the resolution in a decision of its own: `action` is EXACTLY the
        // translation resolved under `PrimaryModifier::for_target()`. On a Mac
        // that answers Meta (and the window smoke presses Cmd+L for real on the
        // macos-14 leg); on this gate it answers Control, and the Cmd tests above
        // drive the same composition with `Meta` explicitly.
        let primary = shortcuts::PrimaryModifier::for_target();
        for (key_code, characters, flags) in [
            (0, Some("l"), MODIFIER_FLAG_COMMAND),
            (0, Some("l"), MODIFIER_FLAG_CONTROL),
            (KEY_CODE_ESCAPE, Some("\u{1b}"), 0),
            (KEY_CODE_F12, Some("\u{f70f}"), FUNCTION_BITS),
            (
                KEY_CODE_ARROW_LEFT,
                Some("\u{f702}"),
                MODIFIER_FLAG_COMMAND | FUNCTION_BITS,
            ),
        ] {
            for focus in [Focus::Page, Focus::UrlBar] {
                assert_eq!(
                    action(key_code, characters, flags, focus),
                    chord(key_code, characters, flags)
                        .and_then(|chord| shortcuts::resolve_chord(chord, focus, primary)),
                    "key code {key_code:#x} + flags {flags:#x}: the edge must add no decision"
                );
            }
        }
    }

    #[test]
    fn escape_is_reported_by_focus_and_never_decided_here() {
        // Acceptance: Escape stops the load with the page focused and reverts the
        // URL bar's edit with the bar focused, on the focus THIS edge reports.
        // The split lives in the core, so it holds on both conventions.
        for primary in [
            shortcuts::PrimaryModifier::Meta,
            shortcuts::PrimaryModifier::Control,
        ] {
            let escape = chord(KEY_CODE_ESCAPE, Some("\u{1b}"), 0).expect("Escape is a chord");
            assert_eq!(
                shortcuts::resolve_chord(escape, Focus::Page, primary),
                Some(ChromeAction::Stop),
                "Escape with the page focused stops the load ({primary:?})"
            );
            assert_eq!(
                shortcuts::resolve_chord(escape, Focus::UrlBar, primary),
                Some(ChromeAction::RevertUrlBar),
                "Escape in the URL bar reverts the edit ({primary:?})"
            );
        }
    }

    #[test]
    fn a_chord_survives_the_flags_a_mac_really_sends() {
        // F5 and F12 arrive with AppKit's Function bit set, and the arrows with
        // Function + NumericPad, so a translation that forwarded the raw flags
        // would make every one of them unmatchable against the core's EXACT
        // modifier comparison. They are dropped in translation, exactly as the
        // GTK edge drops Caps/Num Lock -- and Caps Lock is dropped here too.
        assert_eq!(
            mac_action(
                0,
                Some("L"),
                MODIFIER_FLAG_COMMAND | CAPS_LOCK_BIT,
                Focus::Page
            ),
            Some(ChromeAction::FocusUrlBar),
            "Caps Lock must not stop Cmd+L firing, and a capital L is still L"
        );
        assert_eq!(
            mac_action(
                KEY_CODE_ARROW_LEFT,
                Some("\u{f702}"),
                MODIFIER_FLAG_COMMAND | FUNCTION_BITS | CAPS_LOCK_BIT,
                Focus::Page
            ),
            Some(ChromeAction::GoBack),
            "the arrows' own Function/NumericPad bits must not disqualify Cmd+Left"
        );
    }

    #[test]
    fn a_real_modifier_still_disqualifies_a_chord() {
        // The negative half: matching is EXACT on the four modifiers that survive
        // translation, so Cmd+Shift+L is not Cmd+L and a modified F12 is not the
        // plain-F12 row. Everything werust does not claim must reach the page and
        // the URL bar untouched -- ordinary typing above all.
        for (key_code, characters, flags, what) in [
            (
                0,
                Some("l"),
                MODIFIER_FLAG_COMMAND | MODIFIER_FLAG_SHIFT,
                "Cmd+Shift+L",
            ),
            (
                KEY_CODE_F12,
                Some("\u{f70f}"),
                MODIFIER_FLAG_COMMAND | FUNCTION_BITS,
                "Cmd+F12",
            ),
            (0, Some("a"), 0, "a bare letter (ordinary typing)"),
            (0, Some("l"), 0, "an unmodified L"),
            (
                KEY_CODE_ARROW_LEFT,
                Some("\u{f702}"),
                FUNCTION_BITS,
                "a bare Left arrow (caret movement)",
            ),
            (
                0,
                Some("q"),
                MODIFIER_FLAG_COMMAND,
                "Cmd+Q, the platform's own Quit key equivalent",
            ),
        ] {
            assert_eq!(
                mac_action(key_code, characters, flags, Focus::Page),
                None,
                "{what} is not a werust shortcut and must reach its usual handler"
            );
        }
    }

    #[test]
    fn a_named_key_is_read_off_the_key_code_not_off_the_layouts_character() {
        // The named keys have a fixed PHYSICAL position, so they are matched on
        // the virtual key code and cannot be broken by a layout (or by AppKit's
        // private-use function-key characters).
        for (key_code, expected) in [
            (KEY_CODE_ESCAPE, shortcuts::Key::Escape),
            (KEY_CODE_F5, shortcuts::Key::F5),
            (KEY_CODE_F12, shortcuts::Key::F12),
            (KEY_CODE_ARROW_LEFT, shortcuts::Key::ArrowLeft),
            (KEY_CODE_ARROW_RIGHT, shortcuts::Key::ArrowRight),
        ] {
            assert_eq!(key(key_code, None), Some(expected));
            assert_eq!(
                key(key_code, Some("\u{f702}")),
                Some(expected),
                "the key code wins over whatever character the layout reports"
            );
        }
        // A letter, by contrast, comes from the layout's own character (the KNOWN,
        // accepted limit this seam inherits: letter chords resolve under a Latin
        // layout), and a key that produces none is simply not expressible.
        assert_eq!(key(0, Some("l")), Some(shortcuts::Key::Character('l')));
        assert_eq!(key(0, None), None);
        assert_eq!(key(0, Some("")), None);
    }

    #[test]
    fn the_modifier_bits_are_the_documented_appkit_ones() {
        // The flag constants are the numbers AppKit really sends
        // (`NSEventModifierFlags`), which is what lets this half be driven from a
        // Linux runner at all. The macOS leg checks the same mapping against the
        // real `NSEventModifierFlags` by building its smoke events out of them.
        assert_eq!(
            modifiers(MODIFIER_FLAG_CONTROL),
            Modifiers {
                control: true,
                ..Modifiers::NONE
            }
        );
        assert_eq!(
            modifiers(MODIFIER_FLAG_OPTION),
            Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
            "AppKit's Option is the core's `alt`"
        );
        assert_eq!(
            modifiers(MODIFIER_FLAG_COMMAND),
            Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
            "AppKit's Command is the core's `meta`"
        );
        assert_eq!(
            modifiers(MODIFIER_FLAG_SHIFT),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        );
        // Everything else AppKit can set (Caps Lock, NumericPad, Help, Function)
        // is dropped.
        assert_eq!(
            modifiers(FUNCTION_BITS | CAPS_LOCK_BIT | (1 << 22)),
            Modifiers::NONE
        );
    }

    #[test]
    fn the_side_buttons_are_translated_by_number_and_nothing_else() {
        // Story 7: the extra buttons on the mouse the user already owns. AppKit
        // numbers them 3 and 4; what they DO is the core's call.
        assert_eq!(
            pointer_button(BUTTON_NUMBER_BACK),
            Some(PointerButton::Back)
        );
        assert_eq!(
            pointer_button(BUTTON_NUMBER_FORWARD),
            Some(PointerButton::Forward)
        );
        for ordinary in [0, 1, 2, 5, -1] {
            assert_eq!(
                pointer_button(ordinary),
                None,
                "button {ordinary} is the page's, not the chrome's"
            );
        }
        // …and the core turns those two into history, which is what makes "mouse
        // buttons 4 and 5 navigate history" true on this edge.
        assert_eq!(
            pointer_button(BUTTON_NUMBER_BACK).and_then(shortcuts::resolve_pointer_button),
            Some(ChromeAction::GoBack)
        );
        assert_eq!(
            pointer_button(BUTTON_NUMBER_FORWARD).and_then(shortcuts::resolve_pointer_button),
            Some(ChromeAction::GoForward)
        );
    }
}
