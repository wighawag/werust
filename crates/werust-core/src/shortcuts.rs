//! The **shortcut resolution**: the ONE place that decides what a keyboard
//! chord (or a mouse back/forward button) MEANS, for every OS edge.
//!
//! werust used to bind exactly one key anywhere in the product (F12, for the web
//! inspector), and it was bound by DECIDING, inside the GTK binary, that a
//! particular `gdk::Key` + `gdk::ModifierType` pair meant "open the inspector".
//! That is the shape `CONTEXT.md`'s ONE-derivation rule (the "chrome
//! presentation / painter" entry) exists to prevent, and the repo has already
//! paid twice for the same class of drift: a rule re-implemented per edge drifts
//! per edge. So the DECISION lives here, in the toolkit-free core, and each edge
//! only TRANSLATES its native event into this vocabulary and PERFORMS what comes
//! back (spec `chrome-conventional-controls`, task
//! `shortcut-resolution-in-core-and-the-gtk-edge`).
//!
//! # The shape
//!
//! [`resolve_chord`] is a pure function of a [`Chord`] (a [`Key`] plus the held
//! [`Modifiers`]), the current [`Focus`], and the platform's
//! [`PrimaryModifier`], returning the [`ChromeAction`] to perform, or [`None`]
//! when werust claims no meaning for that input and the edge must let the page
//! and the URL bar have it. [`resolve_pointer_button`] is the same decision for
//! the mouse's extra buttons.
//!
//! Nothing here needs a display, a toolkit or an SDK, so the WHOLE shortcut set
//! is pinned by the table test below inside the pure-Rust `verify` gate, and the
//! Cmd-versus-Ctrl split is ONE branch rather than a re-decision on each of the
//! three desktop edges.
//!
//! # Focus is an INPUT, not an edge special case
//!
//! Escape means two different things depending on what has focus: stop the load
//! (page focused), revert the edit and restore the current URL (URL bar
//! focused). If focus were not part of the signature, every edge would grow its
//! own Escape branch, which is precisely the per-edge decision this module
//! deletes. So the edge REPORTS focus and this module decides.
//!
//! # Capability-agnostic, deliberately
//!
//! A chord resolves to an action regardless of whether the asking edge can
//! PERFORM it; an edge that lacks the underlying capability simply has no
//! handler for that action. macOS is the live case: it reaches no web inspector
//! at all (`docs/platform-capability-matrix.toml` records `web-inspector` as
//! `stubbed` there, owned by `macos-web-inspector-safari-devtools`), and
//! teaching THIS function about that would fork it per platform, re-minting the
//! per-edge branching it exists to remove. The rule is load-bearing for the
//! three sibling edge tasks and is recorded, with the rest of this module's
//! judgement calls, in
//! `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`.
//!
//! # Where the vocabulary comes from
//!
//! [`Key`] and [`Modifiers`] are spelled after the W3C UI Events `KeyboardEvent`
//! key names (`"Escape"`, `"ArrowLeft"`, `"F5"`, and `meta` for Cmd/Super),
//! because that vocabulary is already the neutral meeting point GTK, Win32 and
//! AppKit each map onto, and because a browser project that invented a fourth
//! naming scheme for keys would have to explain it forever. No toolkit enum
//! (`gdk::Key`, `VK_*`, `NSEvent` key codes) may cross into this module.

/// A KEY, named toolkit-neutrally: what the user pressed, independent of how any
/// one toolkit spells it.
///
/// Only the keys werust's shortcuts actually use are modelled. A key with no
/// shortcut has no reason to exist here, and an edge that cannot map a native
/// key onto one of these simply reports nothing and passes the event on.
///
/// Named after the W3C UI Events `KeyboardEvent.key` values, so an edge author
/// can look the mapping up rather than learn a werust-only spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character key, carried as the character it produces.
    ///
    /// Letters are compared case-INSENSITIVELY by [`resolve_chord`], so an edge
    /// may report whatever case its toolkit hands it (toolkits disagree, and
    /// Caps Lock changes it under the user's hand) without normalising, which
    /// would be a decision per edge.
    Character(char),
    /// The Escape key. The one FOCUS-dependent shortcut werust has.
    Escape,
    /// The F5 function key (the second, modifier-free reload chord).
    F5,
    /// The F12 function key (the web inspector, werust's original binding).
    F12,
    /// The Left arrow key (history back, with the platform's history modifier).
    ArrowLeft,
    /// The Right arrow key (history forward, likewise).
    ArrowRight,
}

/// The modifier keys held down when the key was pressed.
///
/// Deliberately the PHYSICAL modifiers, not an abstract "accelerator" flag: an
/// edge that folded Cmd and Ctrl into one flag would be deciding the
/// Cmd-versus-Ctrl convention itself, which is exactly the branch this module
/// keeps ([`PrimaryModifier`]).
///
/// Lock modifiers (Caps Lock, Num Lock) are deliberately ABSENT: they never
/// participate in a shortcut, so an edge drops them in translation and a chord
/// cannot be broken by a lock key being on (the behaviour the desktop F12
/// binding has always had).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// The Control key.
    pub control: bool,
    /// The Alt / Option key.
    pub alt: bool,
    /// The Shift key.
    pub shift: bool,
    /// The Meta key: Command on macOS, Super/Windows elsewhere (the UI Events
    /// `metaKey`).
    pub meta: bool,
}

impl Modifiers {
    /// No modifier held: the plain keypress (F5, F12, Escape).
    pub const NONE: Modifiers = Modifiers {
        control: false,
        alt: false,
        shift: false,
        meta: false,
    };

    /// Whether exactly `expected` is held, and nothing else.
    ///
    /// Shortcut matching is EXACT rather than "at least": Ctrl+Shift+I must not
    /// satisfy a Ctrl+I rule, or werust would silently swallow GTK4's own
    /// interactive-debugger chords (Ctrl+Shift+I / Ctrl+Shift+D) that the
    /// original F12 decision was chosen to leave alone.
    #[must_use]
    const fn is_exactly(self, expected: Modifiers) -> bool {
        self.control == expected.control
            && self.alt == expected.alt
            && self.shift == expected.shift
            && self.meta == expected.meta
    }

    /// Just this one modifier held.
    #[must_use]
    const fn only(modifier: PrimaryModifier) -> Modifiers {
        match modifier {
            PrimaryModifier::Control => Modifiers {
                control: true,
                ..Modifiers::NONE
            },
            PrimaryModifier::Meta => Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        }
    }
}

/// Which physical modifier a platform's users expect as the PRIMARY browser
/// accelerator: Control on Linux and Windows, Meta (Command) on macOS.
///
/// THIS is the Cmd-versus-Ctrl difference, and it exists ONCE, as an input to
/// [`resolve_chord`], instead of once per edge. An edge states which convention
/// its platform follows (usually via [`for_target`](PrimaryModifier::for_target))
/// and never restates what a chord means under it.
///
/// It is a keyboard CONVENTION, not a capability: it says how a platform's users
/// spell a shortcut, never what that platform can do (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryModifier {
    /// Ctrl-based shortcuts: the Linux (GTK) and Windows convention.
    Control,
    /// Cmd-based shortcuts: the macOS convention.
    Meta,
}

impl PrimaryModifier {
    /// The convention of the platform this build targets: [`Meta`](Self::Meta)
    /// on macOS, [`Control`](Self::Control) everywhere else.
    ///
    /// The knowledge that "a Mac is the Cmd platform" is the core's, so an edge
    /// does not restate it. It stays a PARAMETER of [`resolve_chord`] rather than
    /// being read inside it, so the table test can drive BOTH conventions from
    /// any host: the Cmd branch is pinned on the Linux gate, not only on a Mac.
    #[must_use]
    pub fn for_target() -> Self {
        if cfg!(target_os = "macos") {
            PrimaryModifier::Meta
        } else {
            PrimaryModifier::Control
        }
    }

    /// The modifier the platform uses for HISTORY navigation: Alt+Arrow on a
    /// Ctrl platform, Cmd+Arrow on a Mac.
    ///
    /// A second axis of the same platform split, and the reason this is a method
    /// rather than "swap Ctrl for Cmd everywhere": Alt+Left is the
    /// Linux/Windows history chord, while a Mac's is Cmd+Left (decision 3 in
    /// `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`).
    #[must_use]
    const fn history(self) -> Modifiers {
        match self {
            PrimaryModifier::Control => Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
            PrimaryModifier::Meta => Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        }
    }
}

/// Where the keyboard focus is when the key is pressed: an INPUT to the
/// resolution, because Escape means different things in the two places.
///
/// Deliberately two-valued. The URL bar is the only chrome widget whose keyboard
/// context changes what a chord means; everything else (the page, a toolbar
/// button, the menu) is reported as [`Page`](Focus::Page), so an edge answers
/// one question ("is the URL bar focused?") rather than classifying its whole
/// widget tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The page view (or anything that is not the URL bar) has the keyboard.
    #[default]
    Page,
    /// The URL bar has the keyboard: the user is typing an address.
    UrlBar,
}

/// A KEY CHORD: the key pressed plus the modifiers held with it.
///
/// The word the spec and the tasks use for "the thing an edge must not
/// interpret", so it is the word the code uses too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// The key pressed.
    pub key: Key,
    /// The modifiers held with it.
    pub modifiers: Modifiers,
}

impl Chord {
    /// The chord `key` pressed with `modifiers` held.
    #[must_use]
    pub const fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

/// The extra mouse buttons a browser is expected to honour: the side buttons
/// most mice engrave as Back and Forward (story 7, "buttons 4 and 5").
///
/// Named by the BUTTON, never by its native number: X11/Wayland deliver them as
/// buttons 8 and 9, Win32 as `XBUTTON1`/`XBUTTON2`, AppKit as `buttonNumber` 3
/// and 4. Mapping a native number onto one of these is translation (the edge's
/// job); deciding that they navigate history is a decision (this module's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// The rear-most side button, engraved Back on a typical mouse.
    Back,
    /// The forward side button.
    Forward,
}

/// What the chrome should DO in response to an input: the closed vocabulary
/// every edge implements handlers for.
///
/// Actions, not implementations: [`GoBack`](ChromeAction::GoBack) says "go back",
/// and the edge performs it through the existing [`Renderer`](renderer::Renderer)
/// seam (`BrowserShell::go_back`, gated on
/// [`ChromeState::can_go_back`](crate::ChromeState::can_go_back)) exactly as its
/// toolbar button does. Nothing here knows how any edge performs anything, and
/// an edge that cannot perform an action simply has no handler for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeAction {
    /// Focus the URL bar and SELECT its contents, so typing replaces the address
    /// (story 1).
    FocusUrlBar,
    /// Reload the current page (story 2).
    Reload,
    /// Go one step back in session history (story 3 / 7), if there is one.
    GoBack,
    /// Go one step forward in session history (story 3 / 7), if there is one.
    GoForward,
    /// Stop the in-flight load (story 5).
    Stop,
    /// Revert the URL bar's in-progress edit and restore the current page's URL
    /// (story 6): the other half of Escape.
    RevertUrlBar,
    /// Open the platform's web inspector over the current page (story 15,
    /// werust's original F12 binding). Not every edge has one; see the module
    /// docs on the capability-agnostic rule.
    OpenWebInspector,
}

impl ChromeAction {
    /// Every action an input can resolve to.
    ///
    /// The single source of truth for "which actions exist", so an edge-wiring
    /// guard (or a future edge author) can iterate the whole vocabulary instead
    /// of re-listing it in a literal that silently goes stale. Kept complete by
    /// the const check below.
    pub const ALL: [ChromeAction; 7] = [
        ChromeAction::FocusUrlBar,
        ChromeAction::Reload,
        ChromeAction::GoBack,
        ChromeAction::GoForward,
        ChromeAction::Stop,
        ChromeAction::RevertUrlBar,
        ChromeAction::OpenWebInspector,
    ];
}

/// Keeps [`ChromeAction::ALL`] EXHAUSTIVE, at compile time (the same device
/// [`LoadStep::ALL`](crate::LoadStep::ALL) uses: a total match whose every arm
/// hands back the action's OWN slot, so a new variant cannot reach a build
/// without being listed, once, in slot order).
const _CHROME_ACTION_ALL_IS_EVERY_ACTION_IN_SLOT_ORDER: () = {
    const fn listed(action: ChromeAction) -> ChromeAction {
        match action {
            ChromeAction::FocusUrlBar => ChromeAction::ALL[0],
            ChromeAction::Reload => ChromeAction::ALL[1],
            ChromeAction::GoBack => ChromeAction::ALL[2],
            ChromeAction::GoForward => ChromeAction::ALL[3],
            ChromeAction::Stop => ChromeAction::ALL[4],
            ChromeAction::RevertUrlBar => ChromeAction::ALL[5],
            ChromeAction::OpenWebInspector => ChromeAction::ALL[6],
        }
    }
    let mut i = 0;
    while i < ChromeAction::ALL.len() {
        assert!(
            listed(ChromeAction::ALL[i]) as u8 == ChromeAction::ALL[i] as u8,
            "ChromeAction::ALL must hold every action, once, in slot order"
        );
        i += 1;
    }
};

/// What `chord` MEANS, given where the focus is and which modifier the platform
/// uses as its primary accelerator, or [`None`] when werust claims no meaning
/// for it.
///
/// THE table. Every werust keyboard shortcut is a row here and nowhere else:
///
/// | chord (Ctrl platform / Mac) | focus | action |
/// |---|---|---|
/// | Ctrl+L / Cmd+L | any | [`FocusUrlBar`](ChromeAction::FocusUrlBar) |
/// | Ctrl+R / Cmd+R, or F5 | any | [`Reload`](ChromeAction::Reload) |
/// | Alt+Left / Cmd+Left | any | [`GoBack`](ChromeAction::GoBack) |
/// | Alt+Right / Cmd+Right | any | [`GoForward`](ChromeAction::GoForward) |
/// | Escape | page | [`Stop`](ChromeAction::Stop) |
/// | Escape | URL bar | [`RevertUrlBar`](ChromeAction::RevertUrlBar) |
/// | F12 | any | [`OpenWebInspector`](ChromeAction::OpenWebInspector) |
///
/// A [`None`] must be PASSED ON by the edge (the page and the URL bar keep every
/// key werust does not claim), which is also what leaves GTK4's interactive
/// debugger (Ctrl+Shift+I / Ctrl+Shift+D) and the platform's own text-editing
/// keys working: matching is EXACT on the modifier set, never "at least these".
#[must_use]
pub fn resolve_chord(chord: Chord, focus: Focus, primary: PrimaryModifier) -> Option<ChromeAction> {
    let modifiers = chord.modifiers;
    let primary_only = modifiers.is_exactly(Modifiers::only(primary));
    let unmodified = modifiers.is_exactly(Modifiers::NONE);
    let history = modifiers.is_exactly(primary.history());

    match chord.key {
        Key::Character(c) if primary_only && c.eq_ignore_ascii_case(&'l') => {
            Some(ChromeAction::FocusUrlBar)
        }
        Key::Character(c) if primary_only && c.eq_ignore_ascii_case(&'r') => {
            Some(ChromeAction::Reload)
        }
        Key::F5 if unmodified => Some(ChromeAction::Reload),
        Key::ArrowLeft if history => Some(ChromeAction::GoBack),
        Key::ArrowRight if history => Some(ChromeAction::GoForward),
        // The FOCUS-dependent one, and the reason focus is in this signature at
        // all: abandon a hanging page from the keyboard (story 5), or undo what
        // you were typing in the bar and get the current URL back (story 6).
        Key::Escape if unmodified => Some(match focus {
            Focus::Page => ChromeAction::Stop,
            Focus::UrlBar => ChromeAction::RevertUrlBar,
        }),
        // F12 ALONE. Any real modifier disqualifies it, which is what keeps the
        // web inspector distinct from GTK4's interactive debugger chords (the
        // original desktop decision, moved here intact).
        Key::F12 if unmodified => Some(ChromeAction::OpenWebInspector),
        _ => None,
    }
}

/// What a mouse `button` MEANS: the side buttons navigate session history
/// (story 7).
///
/// Separate from [`resolve_chord`] because the INPUT is different, but the same
/// rule and the same [`ChromeAction`] vocabulary: the edge says which button was
/// pressed, this module says what happens, and the edge performs it through the
/// same seam its toolbar buttons use. An [`Option`] for symmetry and for room to
/// grow (a middle-click / extra button werust claims no meaning for stays the
/// page's).
#[must_use]
pub fn resolve_pointer_button(button: PointerButton) -> Option<ChromeAction> {
    Some(match button {
        PointerButton::Back => ChromeAction::GoBack,
        PointerButton::Forward => ChromeAction::GoForward,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ctrl on a Linux/Windows keyboard, Cmd on a Mac: the two conventions the
    /// table is driven with below.
    const CTRL_PLATFORM: PrimaryModifier = PrimaryModifier::Control;
    const MAC_PLATFORM: PrimaryModifier = PrimaryModifier::Meta;

    const CONTROL: Modifiers = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    const ALT: Modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    const META: Modifiers = Modifiers {
        meta: true,
        ..Modifiers::NONE
    };

    #[test]
    fn the_table_pins_every_conventional_browser_chord_on_a_ctrl_platform() {
        // Acceptance: ONE display-free resolution maps (key, modifiers, focus) to
        // a chrome action, covering focus-and-select the URL bar, reload, history
        // back, history forward, stop, and the web inspector. This is the whole
        // table for a Ctrl platform (the GTK edge this task wires, and Windows),
        // asserted as data so a later edge inherits the SAME meanings.
        let table = [
            // Story 1: reach the URL bar without the mouse.
            (
                Chord::new(Key::Character('l'), CONTROL),
                Focus::Page,
                Some(ChromeAction::FocusUrlBar),
            ),
            // Story 2: reload the way every other browser does.
            (
                Chord::new(Key::Character('r'), CONTROL),
                Focus::Page,
                Some(ChromeAction::Reload),
            ),
            (
                Chord::new(Key::F5, Modifiers::NONE),
                Focus::Page,
                Some(ChromeAction::Reload),
            ),
            // Story 3: history from the keyboard.
            (
                Chord::new(Key::ArrowLeft, ALT),
                Focus::Page,
                Some(ChromeAction::GoBack),
            ),
            (
                Chord::new(Key::ArrowRight, ALT),
                Focus::Page,
                Some(ChromeAction::GoForward),
            ),
            // Story 5 / 6: Escape, by focus.
            (
                Chord::new(Key::Escape, Modifiers::NONE),
                Focus::Page,
                Some(ChromeAction::Stop),
            ),
            (
                Chord::new(Key::Escape, Modifiers::NONE),
                Focus::UrlBar,
                Some(ChromeAction::RevertUrlBar),
            ),
            // Story 15: the one binding that already existed, now a table row.
            (
                Chord::new(Key::F12, Modifiers::NONE),
                Focus::Page,
                Some(ChromeAction::OpenWebInspector),
            ),
        ];
        for (chord, focus, expected) in table {
            assert_eq!(
                resolve_chord(chord, focus, CTRL_PLATFORM),
                expected,
                "{chord:?} with {focus:?} focused"
            );
        }
    }

    #[test]
    fn a_chord_that_means_nothing_resolves_to_nothing_so_the_edge_passes_it_on() {
        // The negative half of the table: an unbound key, a bare letter (ordinary
        // typing), and the URL-bar chords under the WRONG modifier must all
        // resolve to nothing, so the edge lets the page and the URL bar keep
        // every key werust does not claim.
        for chord in [
            Chord::new(Key::Character('a'), Modifiers::NONE),
            Chord::new(Key::Character('l'), Modifiers::NONE),
            Chord::new(Key::Character('l'), ALT),
            Chord::new(Key::Character('r'), ALT),
            Chord::new(Key::ArrowLeft, Modifiers::NONE),
            Chord::new(Key::ArrowRight, Modifiers::NONE),
        ] {
            assert_eq!(
                resolve_chord(chord, Focus::Page, CTRL_PLATFORM),
                None,
                "{chord:?} is not a werust shortcut"
            );
        }
    }

    #[test]
    fn escape_is_focus_dependent_rather_than_an_each_edge_special_case() {
        // Acceptance: Escape resolves DIFFERENTLY by focus: stop the load with
        // the page focused (story 5), revert the edit and restore the current URL
        // with the URL bar focused (story 6). Focus is an INPUT to the
        // resolution, so no edge grows its own Escape branch, and the split holds
        // on BOTH keyboard conventions.
        for primary in [CTRL_PLATFORM, MAC_PLATFORM] {
            assert_eq!(
                resolve_chord(
                    Chord::new(Key::Escape, Modifiers::NONE),
                    Focus::Page,
                    primary
                ),
                Some(ChromeAction::Stop),
                "Escape with the page focused stops the load ({primary:?})"
            );
            assert_eq!(
                resolve_chord(
                    Chord::new(Key::Escape, Modifiers::NONE),
                    Focus::UrlBar,
                    primary
                ),
                Some(ChromeAction::RevertUrlBar),
                "Escape in the URL bar reverts the edit ({primary:?})"
            );
        }
    }

    #[test]
    fn the_cmd_versus_ctrl_difference_lives_here_once_not_in_any_edge() {
        // Acceptance: the Cmd-versus-Ctrl split is expressed ONCE, in this
        // resolution. A Mac user's Cmd+L focuses the URL bar and their Ctrl+L
        // does not; a Linux/Windows user's Ctrl+L does and their Cmd (Super) +L
        // does not. The GTK edge exercises the Ctrl side; the macOS edge task
        // exercises the Cmd side against THIS same function.
        assert_eq!(
            resolve_chord(
                Chord::new(Key::Character('l'), META),
                Focus::Page,
                MAC_PLATFORM
            ),
            Some(ChromeAction::FocusUrlBar)
        );
        assert_eq!(
            resolve_chord(
                Chord::new(Key::Character('r'), META),
                Focus::Page,
                MAC_PLATFORM
            ),
            Some(ChromeAction::Reload)
        );
        // …and the two conventions are genuinely DISTINCT, not "both always work".
        assert_eq!(
            resolve_chord(
                Chord::new(Key::Character('l'), CONTROL),
                Focus::Page,
                MAC_PLATFORM
            ),
            None,
            "Ctrl+L is not a Mac browser shortcut"
        );
        assert_eq!(
            resolve_chord(
                Chord::new(Key::Character('l'), META),
                Focus::Page,
                CTRL_PLATFORM
            ),
            None,
            "Super+L is not a Linux/Windows browser shortcut"
        );
    }

    #[test]
    fn history_follows_each_platforms_own_convention_from_the_same_one_branch() {
        // The history chords are the second half of the same platform split:
        // Alt+Arrow is the Ctrl-platform convention, Cmd+Arrow the Mac one
        // (recorded in docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md,
        // decision 3). Both live HERE, so no edge decides it.
        assert_eq!(
            resolve_chord(Chord::new(Key::ArrowLeft, META), Focus::Page, MAC_PLATFORM),
            Some(ChromeAction::GoBack)
        );
        assert_eq!(
            resolve_chord(Chord::new(Key::ArrowRight, META), Focus::Page, MAC_PLATFORM),
            Some(ChromeAction::GoForward)
        );
        assert_eq!(
            resolve_chord(Chord::new(Key::ArrowLeft, ALT), Focus::Page, MAC_PLATFORM),
            None,
            "Option+Left is not the Mac history chord"
        );
        assert_eq!(
            resolve_chord(Chord::new(Key::ArrowLeft, META), Focus::Page, CTRL_PLATFORM),
            None,
            "Super+Left is not the Linux/Windows history chord"
        );
    }

    #[test]
    fn f12_opens_the_web_inspector_and_the_gtk_debugger_chord_does_not() {
        // Acceptance (story 15, moved here from the desktop binary's own
        // `should_open_web_inspector` and NOT weakened): F12 with no modifiers is
        // the WEB inspector, and GTK4's interactive debugger chords
        // (Ctrl+Shift+I / Ctrl+Shift+D) are NOT, so the two surfaces stay distinct.
        assert_eq!(
            resolve_chord(
                Chord::new(Key::F12, Modifiers::NONE),
                Focus::Page,
                CTRL_PLATFORM
            ),
            Some(ChromeAction::OpenWebInspector),
            "F12 opens the web inspector"
        );

        let gtk_debugger = Modifiers {
            control: true,
            shift: true,
            ..Modifiers::NONE
        };
        for key in ['i', 'd'] {
            assert_eq!(
                resolve_chord(
                    Chord::new(Key::Character(key), gtk_debugger),
                    Focus::Page,
                    CTRL_PLATFORM
                ),
                None,
                "Ctrl+Shift+{key} is the GTK debugger, not the web inspector"
            );
        }

        // A MODIFIED F12 is not the plain-F12 shortcut either, so the
        // web-inspector key stays unambiguous and can never become a debugger
        // chord.
        for modifiers in [
            CONTROL,
            ALT,
            META,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        ] {
            assert_eq!(
                resolve_chord(Chord::new(Key::F12, modifiers), Focus::Page, CTRL_PLATFORM),
                None,
                "a modified F12 ({modifiers:?}) is not the web-inspector shortcut"
            );
        }
    }

    #[test]
    fn the_resolution_is_capability_agnostic_so_no_edge_forks_it() {
        // Load-bearing for the three sibling edge tasks: this maps a chord to an
        // ACTION, never to "can this platform do it". macOS has no web inspector
        // at all (`docs/platform-capability-matrix.toml` records `web-inspector`
        // as stubbed there, owned by `macos-web-inspector-safari-devtools`), and
        // the resolution still resolves F12 to it on the Mac convention: the edge
        // simply has no handler. A capability-aware resolution would fork per
        // platform and re-mint exactly the per-edge decision this seam deletes.
        assert_eq!(
            resolve_chord(
                Chord::new(Key::F12, Modifiers::NONE),
                Focus::Page,
                MAC_PLATFORM
            ),
            Some(ChromeAction::OpenWebInspector),
            "the chord resolves everywhere; PERFORMING it is the edge's business"
        );
    }

    #[test]
    fn a_letter_key_resolves_whatever_case_the_edge_reports() {
        // Toolkits differ on whether a chorded letter arrives lower- or
        // upper-case (and Caps Lock changes it under the user's hand), so the
        // resolution compares letters case-insensitively rather than making every
        // edge normalise, because a normalisation step per edge is a decision per
        // edge.
        assert_eq!(
            resolve_chord(
                Chord::new(Key::Character('L'), CONTROL),
                Focus::Page,
                CTRL_PLATFORM
            ),
            Some(ChromeAction::FocusUrlBar)
        );
    }

    #[test]
    fn the_mouse_back_and_forward_buttons_navigate_history() {
        // Story 7: the extra buttons on the mouse the user already owns. Same
        // input-to-action plumbing, same one place that decides what an input
        // MEANS. The edge only says WHICH button was pressed.
        assert_eq!(
            resolve_pointer_button(PointerButton::Back),
            Some(ChromeAction::GoBack)
        );
        assert_eq!(
            resolve_pointer_button(PointerButton::Forward),
            Some(ChromeAction::GoForward)
        );
    }

    #[test]
    fn every_chrome_action_is_reachable_from_some_input() {
        // The action vocabulary is not allowed to grow a variant nothing can
        // reach: every `ChromeAction::ALL` entry must be resolved by some chord
        // (on some convention/focus) or by a mouse button. A new action added
        // without a way to reach it reds here rather than shipping as dead
        // vocabulary for three edges to implement handlers for.
        let conventions = [PrimaryModifier::Control, PrimaryModifier::Meta];
        let keys = [
            Key::Character('l'),
            Key::Character('r'),
            Key::F5,
            Key::F12,
            Key::Escape,
            Key::ArrowLeft,
            Key::ArrowRight,
        ];
        let modifier_sets = [Modifiers::NONE, CONTROL, ALT, META];
        for action in ChromeAction::ALL {
            let reachable = conventions.iter().any(|&primary| {
                keys.iter().any(|&key| {
                    modifier_sets.iter().any(|&modifiers| {
                        [Focus::Page, Focus::UrlBar].iter().any(|&focus| {
                            resolve_chord(Chord::new(key, modifiers), focus, primary)
                                == Some(action)
                        })
                    })
                })
            }) || [PointerButton::Back, PointerButton::Forward]
                .iter()
                .any(|&button| resolve_pointer_button(button) == Some(action));
            assert!(reachable, "{action:?} is reachable from some input");
        }
    }

    #[test]
    fn the_platform_default_convention_is_the_cores_call_not_each_edges() {
        // An edge should not have to know that a Mac is the Cmd platform: the ONE
        // branch that knows lives here, and an edge that has no reason to
        // override simply asks for its target's convention.
        let expected = if cfg!(target_os = "macos") {
            PrimaryModifier::Meta
        } else {
            PrimaryModifier::Control
        };
        assert_eq!(PrimaryModifier::for_target(), expected);
    }
}
