//! The Win32 edge's half of the shortcut layer: native input TRANSLATED into
//! the toolkit-free vocabulary [`werust_core::shortcuts`] already decides in.
//!
//! What a chord MEANS was decided ONCE, for every edge, by task
//! `shortcut-resolution-in-core-and-the-gtk-edge` (spec
//! `chrome-conventional-controls`), and this module contains no part of that
//! decision. It answers only the questions Win32 alone can answer: which KEY is
//! virtual-key code `0x25`, which MODIFIERS does `GetKeyState` say are held, and
//! which mouse SIDE BUTTON is `XBUTTON1`. Everything after that is
//! [`shortcuts::resolve_chord`] / [`shortcuts::resolve_pointer_button`], and the
//! performing is [`crate::window`]'s.
//!
//! # Why it is a module of its own, and NOT `#[cfg(windows)]`
//!
//! The same reason [`crate::dpi`] and [`crate::profile`] are not: the translation
//! is PURE arithmetic over integers Win32 hands out, so it compiles and is
//! unit-tested on the Ubuntu `verify` gate rather than being discovered wrong on
//! a Windows desktop. The Win32 half ([`crate::window`]) is then one line per
//! message: take what the message carries, ask here, perform what comes back.
//!
//! Being pure has one cost, paid deliberately: the virtual-key codes are spelled
//! here as plain `u16` constants rather than taken from the `windows` crate's
//! `VK_*` (which only exists on Windows), exactly as [`crate::dpi`] spells out
//! `MulDiv`'s arithmetic rather than calling it. They are the values
//! `winuser.h` documents, and the Win32 half asserts each one against the SDK's
//! own `VK_*` at COMPILE time, so a transcription slip cannot survive a Windows
//! build (`crate::window`'s `VIRTUAL_KEY_CODES_MATCH_THE_SDK`).
//!
//! # The messages that are not one-to-one
//!
//! Two Win32 messages do not map straight onto the abstract vocabulary, and both
//! are recorded in
//! `docs/spikes/shortcuts-and-mouse-history-buttons-on-the-windows-edge/DECISIONS.md`:
//!
//! * A chord holding **Alt** (the history chords, Alt+Left / Alt+Right) arrives
//!   as `WM_SYSKEYDOWN`, not `WM_KEYDOWN`. Both carry the same virtual-key code,
//!   so both are translated here identically; the edge simply has to listen to
//!   both messages.
//! * `WM_APPCOMMAND` carries `APPCOMMAND_BROWSER_BACKWARD` /
//!   `APPCOMMAND_BROWSER_FORWARD`, which a mouse's side buttons reach this window
//!   through when the click happened over a CHILD window (Win32's
//!   `DefWindowProc` turns an unhandled `XBUTTON` click into an app command and
//!   sends it up the parent chain) -- and which a keyboard's dedicated
//!   browser keys also send. So it is translated to the same
//!   [`PointerButton`]s, which is the one place this edge's word for an input is
//!   wider than the button engraved on a mouse.

use werust_core::shortcuts::{self, Chord, ChromeAction, Focus, Modifiers, PointerButton};

/// `VK_SHIFT` (`winuser.h`).
pub const VK_SHIFT: u16 = 0x10;
/// `VK_CONTROL`.
pub const VK_CONTROL: u16 = 0x11;
/// `VK_MENU`: Win32's name for the **Alt** key.
pub const VK_MENU: u16 = 0x12;
/// `VK_ESCAPE`.
pub const VK_ESCAPE: u16 = 0x1b;
/// `VK_LEFT`.
pub const VK_LEFT: u16 = 0x25;
/// `VK_RIGHT`.
pub const VK_RIGHT: u16 = 0x27;
/// `VK_LWIN`: the left Windows key, the UI Events `meta` on this platform.
pub const VK_LWIN: u16 = 0x5b;
/// `VK_RWIN`: the right Windows key.
pub const VK_RWIN: u16 = 0x5c;
/// `VK_F5`.
pub const VK_F5: u16 = 0x74;
/// `VK_F12`.
pub const VK_F12: u16 = 0x7b;

/// The first letter key. `winuser.h` documents `0x41`-`0x5a` as the A-Z keys,
/// with the same values as the ASCII capitals, which is the whole of the letter
/// translation below.
const VK_A: u16 = 0x41;
/// The last letter key.
const VK_Z: u16 = 0x5a;

/// `XBUTTON1`: the rear-most side button, in the HIGH word of a `WM_XBUTTON*`
/// `wParam`.
pub const XBUTTON1: u16 = 0x0001;
/// `XBUTTON2`: the forward side button.
pub const XBUTTON2: u16 = 0x0002;

/// `APPCOMMAND_BROWSER_BACKWARD`.
pub const APPCOMMAND_BROWSER_BACKWARD: u16 = 1;
/// `APPCOMMAND_BROWSER_FORWARD`.
pub const APPCOMMAND_BROWSER_FORWARD: u16 = 2;
/// `FAPPCOMMAND_MASK`: the device bits `WM_APPCOMMAND` packs beside the command
/// itself, which `GET_APPCOMMAND_LPARAM` masks off.
const FAPPCOMMAND_MASK: u16 = 0xf000;

/// Whether a key `GetKeyState` was asked about is HELD DOWN.
///
/// Win32 packs two different facts into that `i16`: the HIGH bit says the key is
/// down, the LOW bit says a TOGGLE key is currently on. Reading it as `!= 0`
/// would therefore report Caps Lock (or Num Lock) as a held modifier and break
/// every chord while a lock key happens to be on -- the exact behaviour the
/// shared vocabulary keeps impossible by having no lock modifiers at all.
#[must_use]
pub const fn is_down(key_state: i16) -> bool {
    key_state < 0
}

/// Translate a Win32 virtual-key code into the toolkit-free
/// [`shortcuts::Key`] vocabulary, or [`None`] for a key that vocabulary has no
/// name for.
///
/// TRANSLATION ONLY: it says WHICH key was pressed, never what it means. The
/// letter keys are carried across as the character their virtual-key code IS
/// (`winuser.h` gives `VK_A`-`VK_Z` the ASCII capitals' values), lower-cased
/// because the core compares letters case-insensitively; every other key
/// resolves to nothing and is passed on to whatever the focus is.
#[must_use]
pub fn shortcut_key(virtual_key: u16) -> Option<shortcuts::Key> {
    match virtual_key {
        VK_ESCAPE => Some(shortcuts::Key::Escape),
        VK_F5 => Some(shortcuts::Key::F5),
        VK_F12 => Some(shortcuts::Key::F12),
        VK_LEFT => Some(shortcuts::Key::ArrowLeft),
        VK_RIGHT => Some(shortcuts::Key::ArrowRight),
        VK_A..=VK_Z => char::from_u32(u32::from(virtual_key))
            .map(|letter| shortcuts::Key::Character(letter.to_ascii_lowercase())),
        _ => None,
    }
}

/// Translate the keyboard's current state into the toolkit-free [`Modifiers`].
///
/// `key_state` is `GetKeyState`, passed in rather than called here so the whole
/// translation stays pure and testable off Windows. Only the four modifiers a
/// shortcut can use are read: the lock keys are never asked about, which is what
/// keeps a chord firing while Caps Lock is on.
///
/// Windows reports the Windows key as `VK_LWIN` / `VK_RWIN` and the UI Events
/// vocabulary calls that position `meta`, so a Super chord is REPORTED (and
/// therefore refused by the resolution) rather than silently read as unmodified.
#[must_use]
pub fn shortcut_modifiers(key_state: impl Fn(u16) -> i16) -> Modifiers {
    Modifiers {
        control: is_down(key_state(VK_CONTROL)),
        alt: is_down(key_state(VK_MENU)),
        shift: is_down(key_state(VK_SHIFT)),
        meta: is_down(key_state(VK_LWIN)) || is_down(key_state(VK_RWIN)),
    }
}

/// What a Win32 key press MEANS: this edge's native event, translated and handed
/// to the SHARED resolution.
///
/// The whole of this edge's key handling, and deliberately a pure function of
/// (virtual key, keyboard state, focus) so it is pinned on the Ubuntu gate. The
/// decision itself is [`shortcuts::resolve_chord`]'s; the platform's accelerator
/// convention is the core's call too ([`shortcuts::PrimaryModifier::for_target`],
/// Control on Windows), so the Cmd-versus-Ctrl split is not restated here.
///
/// `focus` is REPORTED by the edge (see [`crate::window`]'s `focus_context`),
/// because Escape means one thing over the page and another in the URL bar and
/// the core -- not this module -- knows which.
#[must_use]
pub fn shortcut_action(
    virtual_key: u16,
    key_state: impl Fn(u16) -> i16,
    focus: Focus,
) -> Option<ChromeAction> {
    let key = shortcut_key(virtual_key)?;
    shortcuts::resolve_chord(
        Chord::new(key, shortcut_modifiers(key_state)),
        focus,
        shortcuts::PrimaryModifier::for_target(),
    )
}

/// Translate a `WM_XBUTTONDOWN` `wParam` into the toolkit-free
/// [`PointerButton`], or [`None`] for anything that is not a side button.
///
/// The pressed X button rides in the HIGH word (the low word holds the modifier
/// and button flags), which is the extraction this function exists to do once.
#[must_use]
pub fn shortcut_pointer_button(wparam: usize) -> Option<PointerButton> {
    match (wparam >> 16) as u16 {
        XBUTTON1 => Some(PointerButton::Back),
        XBUTTON2 => Some(PointerButton::Forward),
        _ => None,
    }
}

/// Translate a `WM_APPCOMMAND` `lParam` into the toolkit-free [`PointerButton`],
/// or [`None`] for the many app commands werust claims nothing about.
///
/// The command is the HIGH word with the device bits masked off, which is what
/// `GET_APPCOMMAND_LPARAM` does.
///
/// This is the one place this edge's translation is WIDER than the name suggests
/// (recorded in the spike's `DECISIONS.md`): the browser-backward/forward app
/// command is how a mouse side button reaches a top-level window when the click
/// landed on a CHILD window -- including WebView2's page window, which belongs to
/// another process and whose raw `WM_XBUTTONDOWN` this window can never see -- and
/// it is also what a keyboard's dedicated Back/Forward keys send. The resulting
/// ACTION is the same either way, so it is translated to the same button rather
/// than teaching the shared vocabulary a Win32-shaped word.
#[must_use]
pub fn app_command_pointer_button(lparam: isize) -> Option<PointerButton> {
    match ((lparam >> 16) as u16) & !FAPPCOMMAND_MASK {
        APPCOMMAND_BROWSER_BACKWARD => Some(PointerButton::Back),
        APPCOMMAND_BROWSER_FORWARD => Some(PointerButton::Forward),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `GetKeyState` for a keyboard with `held` down and nothing else: the high
    /// bit set is Win32's "this key is down".
    fn keyboard(held: &[u16]) -> impl Fn(u16) -> i16 + '_ {
        move |vk| {
            if held.contains(&vk) {
                -128
            } else {
                0
            }
        }
    }

    /// A keyboard with nothing held, but Caps Lock ON: Win32 reports the toggle
    /// in the LOW bit of the SAME value.
    fn caps_locked(held: &[u16]) -> impl Fn(u16) -> i16 + '_ {
        move |vk| {
            let down = if held.contains(&vk) { -128i16 } else { 0 };
            down | 1
        }
    }

    #[test]
    fn every_conventional_chord_the_core_defines_is_reachable_from_a_win32_key_press() {
        // Acceptance: every shortcut the shared resolution defines works on this
        // edge -- focus-and-select the URL bar, reload (both chords), history back
        // and forward, stop, and the web inspector -- reached from the virtual-key
        // codes and modifier state Win32 actually delivers. The MEANINGS are the
        // core's; this pins the TRANSLATION that gets there.
        let table = [
            (
                b'L' as u16,
                &[VK_CONTROL][..],
                Focus::Page,
                Some(ChromeAction::FocusUrlBar),
            ),
            (
                b'R' as u16,
                &[VK_CONTROL][..],
                Focus::Page,
                Some(ChromeAction::Reload),
            ),
            (VK_F5, &[][..], Focus::Page, Some(ChromeAction::Reload)),
            (
                VK_LEFT,
                &[VK_MENU][..],
                Focus::Page,
                Some(ChromeAction::GoBack),
            ),
            (
                VK_RIGHT,
                &[VK_MENU][..],
                Focus::Page,
                Some(ChromeAction::GoForward),
            ),
            (VK_ESCAPE, &[][..], Focus::Page, Some(ChromeAction::Stop)),
            (
                VK_ESCAPE,
                &[][..],
                Focus::UrlBar,
                Some(ChromeAction::RevertUrlBar),
            ),
            (
                VK_F12,
                &[][..],
                Focus::Page,
                Some(ChromeAction::OpenWebInspector),
            ),
        ];
        for (virtual_key, held, focus, expected) in table {
            assert_eq!(
                shortcut_action(virtual_key, keyboard(held), focus),
                expected,
                "virtual key {virtual_key:#04x} with {held:?} held and {focus:?} focused"
            );
        }
    }

    #[test]
    fn escape_is_reported_by_focus_and_never_decided_here() {
        // Acceptance: Escape behaves per FOCUS -- stop the load with the page
        // focused, revert the edit with the URL bar focused -- and this edge
        // reports focus rather than branching on it. The two answers differ only
        // because the argument does.
        assert_eq!(
            shortcut_action(VK_ESCAPE, keyboard(&[]), Focus::Page),
            Some(ChromeAction::Stop)
        );
        assert_eq!(
            shortcut_action(VK_ESCAPE, keyboard(&[]), Focus::UrlBar),
            Some(ChromeAction::RevertUrlBar)
        );
    }

    #[test]
    fn a_key_press_werust_claims_nothing_about_resolves_to_nothing() {
        // The negative half, and the property that keeps typing working: an
        // unclaimed key must resolve to nothing so the edge passes the message on
        // to the URL bar and the page. Ordinary letters, the arrow keys without
        // the history modifier (caret movement in the URL bar!), and a chord under
        // the wrong modifier all fall through.
        for (virtual_key, held) in [
            (b'A' as u16, &[][..]),
            (b'L' as u16, &[][..]),
            (b'L' as u16, &[VK_MENU][..]),
            (b'L' as u16, &[VK_LWIN][..]),
            (VK_LEFT, &[][..]),
            (VK_RIGHT, &[][..]),
            (VK_F5, &[VK_CONTROL][..]),
            (VK_F12, &[VK_SHIFT][..]),
            // The keys the vocabulary has no name for at all.
            (0x09, &[][..]), // VK_TAB
            (0x0d, &[][..]), // VK_RETURN, which the URL bar's own subclass owns
            (0x20, &[][..]), // VK_SPACE
        ] {
            assert_eq!(
                shortcut_action(virtual_key, keyboard(held), Focus::UrlBar),
                None,
                "virtual key {virtual_key:#04x} with {held:?} held is not a werust shortcut"
            );
        }
    }

    #[test]
    fn a_lock_key_being_on_cannot_break_a_chord() {
        // `GetKeyState` packs the TOGGLE state of a lock key into the LOW bit of
        // the same value whose HIGH bit means "held down". Reading it as `!= 0`
        // would make Caps Lock look like a held modifier and kill every chord --
        // including the F12 binding, which has always fired regardless. The
        // shared vocabulary has no lock modifiers precisely so this is decided
        // once, here, in translation.
        assert!(!is_down(1), "a toggled-on lock key is not a held key");
        assert!(is_down(-128), "the high bit is what 'down' means");
        assert_eq!(
            shortcut_modifiers(caps_locked(&[])),
            Modifiers::NONE,
            "Caps Lock is not a modifier the core ever hears about"
        );
        assert_eq!(
            shortcut_action(VK_F12, caps_locked(&[]), Focus::Page),
            Some(ChromeAction::OpenWebInspector)
        );
        assert_eq!(
            shortcut_action(b'L' as u16, caps_locked(&[VK_CONTROL]), Focus::Page),
            Some(ChromeAction::FocusUrlBar)
        );
    }

    #[test]
    fn the_windows_key_is_reported_as_meta_rather_than_as_nothing() {
        // Win32 has no `meta`: it has VK_LWIN and VK_RWIN. Dropping them would
        // make Super+L look like a bare L to the resolution, i.e. the edge would
        // silently claim a chord the core refuses. Both sides of the keyboard
        // report the same modifier.
        for windows_key in [VK_LWIN, VK_RWIN] {
            assert_eq!(
                shortcut_modifiers(keyboard(&[windows_key])),
                Modifiers {
                    meta: true,
                    ..Modifiers::NONE
                }
            );
        }
        assert_eq!(
            shortcut_modifiers(keyboard(&[VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN])),
            Modifiers {
                control: true,
                alt: true,
                shift: true,
                meta: true,
            }
        );
    }

    #[test]
    fn a_letter_key_is_translated_from_its_virtual_key_code() {
        // `winuser.h` gives VK_A..VK_Z the ASCII capitals' values, so the letter
        // IS the code; the core compares case-insensitively, and this edge hands
        // it the lower-case letter rather than making that a per-edge decision.
        assert_eq!(
            shortcut_key(b'L' as u16),
            Some(shortcuts::Key::Character('l'))
        );
        assert_eq!(
            shortcut_key(b'A' as u16),
            Some(shortcuts::Key::Character('a'))
        );
        assert_eq!(
            shortcut_key(b'Z' as u16),
            Some(shortcuts::Key::Character('z'))
        );
        // Just outside the letter range: the digits below and the Windows key
        // above are not characters this vocabulary carries.
        assert_eq!(shortcut_key(VK_A - 1), None);
        assert_eq!(shortcut_key(VK_Z + 1), None);
    }

    #[test]
    fn the_named_keys_are_the_ones_the_shortcut_table_uses() {
        // The rest of the vocabulary, pinned against the virtual-key codes
        // `winuser.h` documents (the numbers are also checked against the SDK's
        // own `VK_*` at compile time on Windows).
        for (virtual_key, key) in [
            (VK_ESCAPE, shortcuts::Key::Escape),
            (VK_F5, shortcuts::Key::F5),
            (VK_F12, shortcuts::Key::F12),
            (VK_LEFT, shortcuts::Key::ArrowLeft),
            (VK_RIGHT, shortcuts::Key::ArrowRight),
        ] {
            assert_eq!(shortcut_key(virtual_key), Some(key));
        }
    }

    #[test]
    fn the_mouse_side_buttons_are_translated_from_the_message_they_arrive_in() {
        // Acceptance: mouse buttons 4 and 5 navigate history. Win32 calls them
        // XBUTTON1 and XBUTTON2 and packs the pressed one into the HIGH word of
        // `WM_XBUTTONDOWN`'s wParam, beside the modifier flags in the low word --
        // so the extraction has to ignore the low word, and a plain button (no X
        // button at all) must claim nothing.
        assert_eq!(
            shortcut_pointer_button(usize::from(XBUTTON1) << 16),
            Some(PointerButton::Back)
        );
        assert_eq!(
            shortcut_pointer_button(usize::from(XBUTTON2) << 16),
            Some(PointerButton::Forward)
        );
        // MK_CONTROL | MK_LBUTTON in the low word must not disturb it.
        assert_eq!(
            shortcut_pointer_button((usize::from(XBUTTON1) << 16) | 0x0009),
            Some(PointerButton::Back)
        );
        assert_eq!(shortcut_pointer_button(0x0009), None);
    }

    #[test]
    fn the_browser_app_command_is_translated_to_the_same_side_buttons() {
        // The other route the side buttons take: a click that landed on a CHILD
        // window (WebView2's page window is one, and lives in another process)
        // reaches this window as `WM_APPCOMMAND` instead. The command sits in the
        // HIGH word of lParam WITH the device bits, which must be masked off --
        // `FAPPCOMMAND_MOUSE` (0x8000) is precisely what a mouse-sourced command
        // carries, so a translation that forgot the mask would claim nothing at
        // all from a mouse.
        const FAPPCOMMAND_MOUSE: isize = 0x8000;
        const FAPPCOMMAND_KEY: isize = 0x0000;
        for device in [FAPPCOMMAND_MOUSE, FAPPCOMMAND_KEY] {
            assert_eq!(
                app_command_pointer_button(
                    ((isize::from(APPCOMMAND_BROWSER_BACKWARD as i16) | device) << 16) | 0x1234
                ),
                Some(PointerButton::Back)
            );
            assert_eq!(
                app_command_pointer_button(
                    ((isize::from(APPCOMMAND_BROWSER_FORWARD as i16) | device) << 16) | 0x1234
                ),
                Some(PointerButton::Forward)
            );
        }
        // APPCOMMAND_VOLUME_UP (10) and the rest belong to whoever else wants
        // them.
        assert_eq!(
            app_command_pointer_button((10 | FAPPCOMMAND_MOUSE) << 16),
            None
        );
    }

    #[test]
    fn the_side_buttons_resolve_through_the_core_not_through_this_edge() {
        // The buttons are translated HERE and decided THERE: this edge never
        // names history. Asserted end to end so the two halves cannot drift apart
        // (the core's own test pins the meanings).
        for (wparam, expected) in [
            (usize::from(XBUTTON1) << 16, ChromeAction::GoBack),
            (usize::from(XBUTTON2) << 16, ChromeAction::GoForward),
        ] {
            let button = shortcut_pointer_button(wparam).expect("a side button");
            assert_eq!(shortcuts::resolve_pointer_button(button), Some(expected));
        }
    }
}
