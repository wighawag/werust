# Shortcut resolution: the decisions this task baked in

Task `shortcut-resolution-in-core-and-the-gtk-edge`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind `crates/werust-core/src/shortcuts.rs` and the GTK wiring in `crates/werust/src/main.rs`. Three sibling edge tasks (`shortcuts-and-mouse-history-buttons-on-the-macos-edge`, `shortcuts-and-mouse-history-buttons-on-the-windows-edge`, and the mobile chrome tasks that must NOT grow shortcuts) inherit them, so they are recorded rather than buried.

## 1. The abstract key vocabulary is spelled after W3C UI Events

**Chosen:** `shortcuts::Key` uses the `KeyboardEvent.key` names (`Escape`, `F5`, `F12`, `ArrowLeft`, `ArrowRight`, plus `Character(char)` for printable keys), and `shortcuts::Modifiers` uses `control` / `alt` / `shift` / `meta`, where `meta` is Command on macOS and Super/Windows elsewhere. No toolkit enum (`gdk::Key`, `VK_*`, AppKit key codes) may cross into the core.

**Why:** it must be expressible by GTK, Win32 and AppKit alike, and every later edge inherits it. UI Events is the one naming scheme all three already map onto in published tables, and werust is a browser: inventing a fourth spelling for keys would need explaining forever. It also keeps the vocabulary SMALL, only the keys a shortcut uses, so an edge that cannot express a key simply reports nothing and passes the event on.

**Alternatives considered:** (a) carry raw platform keycodes and let the core normalise, which drags one toolkit's enum into the toolkit-free crate and makes the core wrong for the other two; (b) an X11-keysym-shaped vocabulary, which is GTK's own dialect wearing a neutral hat; (c) a string key name, which loses exhaustiveness at the `match` and turns a typo into a silently-dead shortcut.

**Touches:** every edge task, since each one's translation function is written against this.

**Sub-decision, letters are matched case-insensitively.** Toolkits disagree about whether a chorded letter arrives as `l` or `L` (and Caps Lock changes it under the user's hand). `resolve_chord` compares letters with `eq_ignore_ascii_case`, so no edge has to normalise; a normalisation step per edge is a decision per edge.

**Sub-decision, lock modifiers do not exist in the vocabulary.** Caps/Num Lock never participate in a shortcut, so `Modifiers` has no bit for them and each edge drops them while translating. This preserves the F12 binding's original behaviour (a lock key being on must not stop F12 firing) as a property of the vocabulary rather than a mask inside one predicate.

## 2. The resolution is CAPABILITY-AGNOSTIC (settled by the task, recorded because three siblings depend on it)

**Chosen:** `resolve_chord` maps a chord to an action regardless of whether the asking edge can PERFORM it. An edge that lacks the capability simply has no handler for that action.

**Why:** macOS is the live case. It reaches no web inspector at all (`docs/platform-capability-matrix.toml` records `web-inspector` as `stubbed` there, owned by `macos-web-inspector-safari-devtools`), so a capability-aware resolution would have to fork per platform, re-minting exactly the per-edge branching this seam exists to delete. The absence stays visible where absences belong: the capability matrix and the edge's handler list.

**Touches:** all three desktop edge tasks. It is asserted, not just written down (`the_resolution_is_capability_agnostic_so_no_edge_forks_it`).

## 3. The Cmd-versus-Ctrl split is a PARAMETER, and it covers history too

**Chosen:** `resolve_chord` takes a `PrimaryModifier` (`Control` on Linux/Windows, `Meta` on macOS). `PrimaryModifier::for_target()` is the core's one `cfg!(target_os = "macos")` branch, which each edge calls rather than restating. The split covers TWO axes, not one:

| action | Ctrl platform | Mac |
|---|---|---|
| focus URL bar / reload | Ctrl+L, Ctrl+R | Cmd+L, Cmd+R |
| history back / forward | Alt+Left, Alt+Right | Cmd+Left, Cmd+Right |

**Why a parameter rather than reading `cfg!` inside `resolve_chord`:** the Cmd branch would then be unexercisable anywhere except a Mac, and this project has no Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`). As a parameter, the Linux `verify` gate drives BOTH conventions through the same table, so the macOS sibling inherits a branch that is already pinned.

**Why history follows the platform too:** story 4 asks for shortcuts that match the platform the user is on, and Alt/Option+Left is not the Mac history chord (Safari and Chrome use Cmd+Left / Cmd+`[`). Keeping Alt+Arrow everywhere would have been simpler but would have shipped a chord no Mac user presses. The alternative of a Mac-only extra row in the table was rejected for the same reason as any per-platform fork: it is the same split, so it belongs in the same one branch (`PrimaryModifier::history`).

**Not adopted:** Cmd+`[` / Cmd+`]` as a second Mac history chord. It is real muscle memory, but no story asks for it and it needs bracket keys in the vocabulary; the macOS edge task can add one row here if it wants it, which is the point of the table being shared.

> **AMENDED by `shortcuts-and-mouse-history-buttons-on-the-macos-edge`:** it did add them, for the Meta convention only, and not for the muscle memory: Cmd+Arrow had to become focus-sensitive there (it is macOS's own caret binding), and Cmd+`[` / Cmd+`]` are the history spelling that never has to yield. No key vocabulary was needed after all, since `Key::Character` already spells a bracket. Decisions 8 and 9 in `docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/DECISIONS.md`; the Ctrl platforms resolve exactly as this decision left them.

**Touches:** the macOS and Windows edge tasks.

## 4. Focus is an INPUT with exactly two values

**Chosen:** `Focus::{Page, UrlBar}`, reported by the edge on every key press. Everything that is not the URL bar (the page, a toolbar button, the menu) is `Page`.

**Why:** Escape means two things (stop the load / revert the edit), and without focus in the signature every edge would grow its own Escape branch, which is the drift the seam exists to prevent. Two values keep the edge's job to ONE question ("is the URL bar focused?") instead of classifying its whole widget tree; a third value would have to be answered by three edges before anything used it.

**Touches:** every edge task (each must report focus, not branch on it).

## 5. The mouse side buttons resolve through the same core vocabulary

**Chosen:** `PointerButton::{Back, Forward}` plus `resolve_pointer_button`, returning the same `ChromeAction`s. The edge translates its native button number; the core says what it does.

**Why:** it is the same input-to-action plumbing, and `if button == 8 { shell.go_back() }` in three edges is three decisions. The function is nearly tautological today, and that is fine: it is the place a later "middle-click opens in a new tab" lands without any edge re-deciding.

**Alternative considered:** naming the variants after the native numbers (`Extra1`/`Extra2`) to keep every hint of meaning out of the edge. Rejected: the buttons are physically engraved Back and Forward, so the name is the button's identity, not its behaviour.

**Edge-local fact, not a decision:** "buttons 4 and 5" is the user-facing name; X11 and Wayland/libinput both deliver them to GDK as buttons **8** and **9** (4 and 5 are the legacy scroll buttons, which GDK4 delivers as scroll events, so binding those would make the wheel navigate history). Win32 sees `XBUTTON1`/`XBUTTON2` and AppKit `buttonNumber` 3/4, which is why the core's vocabulary is named, not numbered.

## 6. The GTK controllers sit on the window, in the CAPTURE phase

**Chosen:** both the `EventControllerKey` and the `GestureClick` are added to the window with `PropagationPhase::Capture`, and anything the resolution does not claim is propagated untouched (`Propagation::Proceed`, and the gesture only claims its sequence for a side button).

**Why:** a browser's own chords have to beat the focused page (a page can bind Escape, and WebKitGTK would otherwise swallow it) and the URL bar's text-editing keys. In the default bubble phase the shortcut only fires when nothing under the focus wanted the key, which is not what a browser does. Capture + propagate-what-we-do-not-claim keeps ordinary typing, page keys and GTK4's own interactive-debugger chords (Ctrl+Shift+I / Ctrl+Shift+D) working exactly as before: the resolution returns `None` for them, so they are never consumed.

**Cost, recorded honestly:** a page can no longer see the chords werust claims (Ctrl+L, Ctrl+R, F5, Alt+Arrow, Escape, F12). That is the conventional browser trade and is why the claimed set is kept to the spec's list.

## 7. Escape in the URL bar restores the chrome's own `url_text`

**Chosen:** `ChromeAction::RevertUrlBar` is performed by setting the entry's text back to `BrowserShell::chrome().url_text`, the same one fact `Chrome::refresh` paints the bar from. Focus is left where it is.

**Why:** the bar can never end up showing something the chrome does not believe, and the revert needs no new core state. Leaving focus in the bar matches Chrome/Firefox (a second Escape returning focus to the page is a nicety no story asks for). The invalid-entry badge is deliberately untouched: it is a separate chrome axis owned by `navigate`, and clearing it from a key handler would be this edge deciding a chrome rule.

## 8. The parity matrix gains one row, and the mobile cells are `n-a`

**Chosen:** a `conventional-shortcuts` row: desktop `implemented`, macOS and Windows `stubbed` onto their sibling tasks, iOS and Android `n-a` with the reason.

**Why `n-a` rather than `stubbed`:** the spec puts hardware-keyboard shortcuts on the mobile edges explicitly out of scope ("Confirmed at tasking: no story asks for them, and the shortcut tasks cover the three desktop edges only"), and neither phone edge has a mouse with side buttons. A `stubbed` cell would have to name a follow-on task that nobody intends to write, which is the kind of fake gap that makes the matrix less trustworthy. Android's system Back keeps its own row (`system-back-navigates-history`).

## What was measured, and what was not

The whole resolution and the whole `gdk` translation are unit-tested display-free inside `verify`, and the edge's shape (translate, never decide) is guarded by `crates/werust-core/tests/shortcut_edge_wiring_shape.rs`. What NO automated test in this repo covers is that a real key press reaches the real window: that needs a display, so it is a recorded manual check (see `README.md` beside this file), exactly as the F12 binding's own acceptance was.
