# Conventional browser shortcuts on the macOS edge: what landed, and what proves it

Task `shortcuts-and-mouse-history-buttons-on-the-macos-edge`, spec `chrome-conventional-controls`. The judgement calls behind the design are in `DECISIONS.md` beside this file; what a chord MEANS is not among them, because that is decided once in `werust_core::shortcuts` (`docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/`).

## Where things live

- **The decision** (what an input MEANS): `crates/werust-core/src/shortcuts.rs`. Changed by this task in exactly two respects, both for the whole product rather than for this edge, and both on the Meta convention only: the history rows now yield to text editing on the platforms where the history chord IS a text-editing binding, which is macOS (`DECISIONS.md`, decision 8), and the Mac's bracket history chords `Cmd+[` / `Cmd+]` are bound, in either focus, as the mitigation for that yielding (decision 9). The Ctrl platforms resolve exactly as they did.
- **The translation** (`NSEvent` -> the shared vocabulary): `crates/werust-macos/src/input.rs`. Deliberately NOT target-gated, so the Ubuntu `verify` gate compiles and unit-tests it against the real core.
- **The interception and the doing**: `crates/werust-macos/src/window.rs`. `ShortcutWindow` (an `NSWindow` subclass) overrides `sendEvent:`; `WindowController::shortcut_focus` reports which of the two focus contexts is live; `WindowController::perform_chrome_action` performs the result through the same `BrowserShell` calls the toolbar buttons drive.
- **The guard** that this edge decides nothing: `crates/werust-macos/tests/macos_shortcut_shape.rs`.
- **The real key presses**: `crates/werust-macos/examples/window_smoke.rs`, run by `.github/workflows/macos-renderer.yml` on `macos-14`.
- **The parity row**: `conventional-shortcuts` in `docs/platform-capability-matrix.toml` (macOS is now `implemented`).

## The shortcut set (macOS)

| input | action |
|---|---|
| Cmd+L | focus the URL bar and select its contents |
| Cmd+R, F5 | reload |
| Cmd+Left / Cmd+Right, page focused | history back / forward (only when that move is available) |
| Cmd+Left / Cmd+Right, URL bar focused | NOT werust's: the field editor moves the caret to the start / end of the line, as it does in every Mac app |
| Cmd+`[` / Cmd+`]`, either focus | history back / forward: the Mac's other history chords, which no text field claims and which therefore never have to yield (`DECISIONS.md`, decision 9) |
| Escape, page focused | stop the in-flight load |
| Escape, URL bar focused | revert the edit, restore the current page's URL |
| mouse Back / Forward side buttons | history back / forward |

Ctrl+L, Ctrl+R and Option+Arrow are deliberately NOT shortcuts here: they are the Ctrl platform's spellings, and on a Mac they are text-editing bindings. That distinctness is asserted, not assumed.

**F12 is the deliberate omission.** The chord still RESOLVES to `OpenWebInspector` — the shared resolution is capability-agnostic by settled design — and this edge simply has no handler, because macOS reaches no web inspector at all: `docs/platform-capability-matrix.toml` records `web-inspector` as `stubbed` here, owned by `macos-web-inspector-safari-devtools`, and neither `crates/macos-renderer` nor `crates/werust-macos` touches `WKPreferences` or `isInspectable`. When that task lands, wiring the arm is a one-line follow-on and needs no change to the resolution.

Anything not in the table above is forwarded to AppKit untouched, so ordinary typing, page keys and the menu bar's own Cmd+Q behave exactly as before.

## What CI proved

**On every Ubuntu `verify` run (`cargo test`), against the REAL `werust-core`:**

1. The whole translation table: `NSEvent` key codes (Escape, F5, F12, the arrows) and `charactersIgnoringModifiers` letters into `shortcuts::Key`; `NSEventModifierFlags` bits into `shortcuts::Modifiers`; AppKit `buttonNumber` 3/4 into `PointerButton::Back`/`Forward`.
2. **The Cmd branch of the shared resolution, reached through THIS edge's real translation** — Cmd+L, Cmd+R, F5, Cmd+Left, Cmd+Right, Escape in both focus contexts — and its DISTINCTNESS from the Ctrl branch (Ctrl+L, Ctrl+R, Option+Arrow resolve to nothing on a Mac, while the same translated chord resolves on the Ctrl convention).
   Including the two rules this edge's reviews added: Cmd+Left / Cmd+Right resolve to history with the page focused and to NOTHING while the URL bar is edited, so the caret move survives (pinned for both platforms in both focus states in the core's own table, `history_follows_each_platforms_own_convention_from_the_same_one_branch`, and through this edge's translation in `the_mac_history_chord_is_left_to_the_field_editor_while_the_url_bar_is_edited`); and Cmd+`[` / Cmd+`]` resolve to history in EITHER focus on the Mac convention and to nothing at all on the Ctrl one (`the_mac_spells_history_a_second_way_that_no_text_field_ever_claims`, and this edge's `the_bracket_history_chords_reach_history_from_either_focus`).
3. That AppKit's own `Function` / `NumericPad` / Caps Lock bits, which a Mac really sends with every arrow and function key, are dropped in translation and therefore cannot make a chord unmatchable against the core's EXACT modifier comparison.
4. That the mouse side buttons resolve to history through the core.
5. Source-shape (`macos_shortcut_shape.rs`): the AppKit layer names no key and no key code; every `ChromeAction` has an arm; the web-inspector arm is EMPTY and says why; focus is reported and never branched on outside the reporter; history goes through the existing seam and its capability flags; no werust chord is installed as an AppKit key equivalent; the shared resolution grew no platform or capability branch.

**On the `macos-14` leg (`cargo run -p werust-macos --example window_smoke`), on a real AppKit window:**

6. AppKit's own `NSEventModifierFlags` values equal the plain-bit constants the Linux-side table is written against.
7. **Cmd+L, pressed as a real `NSEvent` through the real window's `sendEvent:`, focuses the URL bar** — and Ctrl+L, pressed the same way, does not (the negative control).
8. Escape in the URL bar restores the URL the chrome believes and navigates nowhere; Escape with the page focused leaves a half-typed URL alone instead (the discriminating check for focus being a real input rather than a constant).
9. Cmd+R really re-fetches the page, watched at the fixture's own retrieval counter rather than at a settled load state.
10. The side buttons are claimed by the chrome and an ordinary (middle) button is not.
11. F12 opens nothing and disturbs nothing on this edge.

**Locally, from Linux, without an SDK** (`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh`): the whole AppKit half type-checks against `aarch64-apple-darwin`, including the `NSWindow` subclass, the `sendEvent:` override and the smoke's event synthesis.

## What still awaits a Mac with a human in front of it

These are true of every macOS surface in this repo (`docs/adr/0011` Amendment 1, and the no-Mac finding), and the shortcut layer adds three of its own:

- **That the interception really beats a page that binds the key.** The smoke's fixture does not fight for Escape. The claim rests on `sendEvent:` running before the view hierarchy and the first responder, which is documented AppKit behaviour, not something this repo measured. A page that binds Escape is the practical check.
- **That the interception really beats the URL bar's field editor.** Same shape: the smoke drives the field editor, but not a case where AppKit's own `cancelOperation:` would otherwise win.
- **That Cmd+Left really reaches the field editor and moves the caret** when werust declines it. What CI proves is that werust RESOLVES nothing for it while the URL bar is focused, so `sendEvent:` forwards the event to `NSWindow` untouched; that AppKit then applies its standard `moveToLeftEndOfLine:` binding is documented AppKit behaviour this repo cannot press (manual step 3 below).
- **That Cmd+`[` / Cmd+`]` really navigate on a real window.** The smoke does not press them: its window has ONE page in its history, so "it really went back" is not an assertion that leg can make, and the chords add no new AppKit mechanism to check (they are character keys on the identical path Cmd+L is pressed through there). The Ubuntu gate proves they resolve through this edge's real translation in both focus contexts; manual step 3 is where a human presses them.
- **The page-text-field limit itself.** Cmd+Left inside a page's own text field navigates history and loses the edit, which is a KNOWN, ACCEPTED limit rather than something awaiting proof (`DECISIONS.md`, decision 8). It is listed here so nobody at a Mac reports it as a discovery.
- **`NSEvent.buttonNumber()` itself.** AppKit's synthetic-mouse constructor carries no button number, so the smoke drives the side buttons from the number onwards (`BrowserWindow::press_side_button`); reading the number off a real event is the one unexercised token. See `DECISIONS.md`, decision 7.

## Manual verification (needs a Mac; nothing in this repo can do it)

Build and run: `cargo run -p werust-macos` on a Mac.

1. **Cmd+L**: the URL bar takes focus and its whole text is selected; typing replaces the address. **Ctrl+L** does not do this (it is the Ctrl platform's chord).
2. **Cmd+R** and **F5**: the page reloads (watch the URL-bar progress and the status line).
3. Navigate to a second page, then, **with the page focused**, **Cmd+Left**: it goes back; **Cmd+Right**: forward. At the start of history Cmd+Left does nothing (the same capability flag that greys the Back button). **Option+Left** does nothing: it is not the Mac history chord.
   Then click into the **URL bar**, type an address, and press **Cmd+Left** and **Cmd+Right** there: the CARET must move to the start and the end of the line, exactly as in any Mac text field, and the browser must NOT navigate or lose the edit. That is the half werust deliberately does not claim (`DECISIONS.md`, decision 8).
   **KNOWN, ACCEPTED LIMIT, do NOT report it as a bug:** a text field inside the PAGE does not behave that way. Type into a search box or a form field on a page and press Cmd+Left, and werust navigates back and the edit is lost. The chrome gets the first look at every key and cannot see what the page has focused (`DECISIONS.md`, decision 8 records why, and why fixing it is an event-order question for the whole product rather than a macOS detail).
   Finally, **Cmd+`[`** and **Cmd+`]`**: these must go back and forward from EVERYWHERE, including while you are typing in the URL bar and while you are typing in a page text field. They are the always-safe history spelling and the mitigation for the limit above (decision 9).
4. **Escape with the page focused**, during a slow load: the load stops, exactly as the Stop button does. Try it on a page that binds Escape itself (a full-screen or modal-heavy site): werust must still stop the load.
5. **Escape with the URL bar focused**, after typing rubbish into it: the bar snaps back to the current page's URL and nothing navigates. AppKit's own field-editor cancel must not get there first.
6. **F12**: nothing happens, and no window opens. That is correct on this platform until `macos-web-inspector-safari-devtools` lands.
7. **Mouse side buttons**: on a page with history, the rear thumb button goes back and the forward one goes forward, with the pointer over the page as well as over the chrome.
8. **Cmd+Q still quits**, and ordinary typing in the URL bar (including Cmd+A / Cmd+C / Cmd+V) is untouched.
