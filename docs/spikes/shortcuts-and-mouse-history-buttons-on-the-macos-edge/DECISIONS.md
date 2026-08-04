# The macOS shortcut edge: the decisions this task baked in

Task `shortcuts-and-mouse-history-buttons-on-the-macos-edge`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind `crates/werust-macos/src/input.rs` and the shortcut half of `crates/werust-macos/src/window.rs`.

The DECISION about what a chord means is not among them, and could not be: it lives once, in `crates/werust-core/src/shortcuts.rs`, and its own judgement calls are recorded in `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`. Everything below is about how AppKit's native input reaches that resolution, and where AppKit's own machinery would otherwise compete with it.

## 1. The interception point is an `NSWindow` subclass's `sendEvent:`

**Chosen:** the browser window is `WerustMacBrowserWindow` (`ShortcutWindow` in Rust), an `NSWindow` subclass whose `sendEvent:` override offers every event to the shared resolution FIRST and forwards anything unclaimed to `super`.

**Why:** a browser's own chords have to beat the focused page and the URL bar, which is exactly what the GTK edge's `PropagationPhase::Capture` buys there (decision 6 in the sibling file). On AppKit the equivalent stage is `NSWindow.sendEvent:`, which runs before the event reaches the view hierarchy's `performKeyEquivalent:`, before the first responder's `keyDown:`, and therefore before both the `WKWebView` (a page can bind Escape) and the URL bar's field editor (whose own Escape is AppKit's `cancelOperation:`). Forwarding everything unclaimed keeps ordinary typing, page keys and the menu bar's key equivalents behaving exactly as they did.

**Alternatives considered:** (a) `NSEvent.addLocalMonitorForEventsMatchingMask:handler:`, which is the other documented pre-dispatch hook. Rejected on scope: a local monitor is APPLICATION-wide, so it would also claim Cmd+R while the DEBUG window is key, and the fix would be to compare `event.window` inside the monitor, i.e. to re-derive "is this the browser window?" that a window subclass answers by construction. It would also add a `block2` dependency for the handler block. (b) `performKeyEquivalent:` on the content view, which AppKit only calls for command-modified keys, so Escape, F5, F12 and the bare arrows would never arrive. (c) `keyDown:` on the content view, which is the BUBBLE phase wearing another name: the page would get the key first.

**Cost, recorded honestly:** the same trade the GTK edge recorded. A page can no longer see the chords werust claims (Cmd+L, Cmd+R, F5, Escape, F12, and Cmd+`[` / Cmd+`]`), which is the conventional browser trade and why the claimed set is kept to the spec's list.

Cmd+Arrow is the one chord that is NOT claimed unconditionally, and that is not a property of this interception point: it is macOS's own move-to-beginning/end-of-line binding, so claiming it while a text field has the keyboard would eat the caret move and the user's edit, which no Mac browser does. Because a chord means what the core says it means, the fix is the core's and not a special case here: the resolution returns nothing for Cmd+Arrow while the URL bar has focus, `claim` forwards the event to `NSWindow` untouched, and AppKit moves the caret.

That protection reaches the URL BAR's field editor and nothing else. A text field INSIDE the page still loses Cmd+Left / Cmd+Right to history navigation, because this interception point cannot see what the page has focused (decision 8 records the limit, why it is not fixable here, and the always-safe Cmd+`[` / Cmd+`]` spelling that mitigates it, decision 9).

**Touches:** the Windows sibling task will face the same question (its analogue is the message loop / `WM_KEYDOWN` before `TranslateAccelerator`), and the answer there should be reasoned from the same "must beat the focused page" property rather than copied.

## 2. AppKit's own key-equivalent handling is NOT allowed to race the resolution

**Chosen:** no werust chord is installed as an AppKit key equivalent. The `⋮` menu's items are built with `keyEquivalent: ""` (unchanged), and the ONE key equivalent this window installs is the platform's own Quit (`terminate:`, `q`) on the app menu.

**Why this is a decision and not an omission:** AppKit resolves menu key equivalents itself, inside the same dispatch this task hooks. A menu item carrying `l` or `r` would mean the chord had TWO owners — the core's `resolve_chord` and an AppKit `NSMenuItem` — and which one won would depend on where in `sendEvent:` the interception sat. That is the per-edge decision the seam exists to delete, wearing a macOS costume. It is also the tempting thing to do next: adding "Reload ⌘R" to the menu bar is a natural macOS polish request, and doing it by giving the item a key equivalent would silently fork the shortcut layer. The right way, when someone wants it, is a menu item whose ACTION performs the same `ChromeAction` (or a `keyEquivalent` set from the core, if the core ever grows a "how is this chord spelled" accessor) — never a second table.

The one collision that DOES exist today is benign and deliberate: Cmd+Q is AppKit's, the resolution claims nothing for it, and `crates/werust-macos/src/input.rs`'s tests pin that (`a_real_modifier_still_disqualifies_a_chord` includes Cmd+Q) so a future chord cannot quietly swallow Quit.

**Asserted, not just written down:** `crates/werust-macos/tests/macos_shortcut_shape.rs`, `the_edge_names_no_key_meaning_outside_its_translation`.

## 3. Named keys are read off the VIRTUAL KEY CODE, letters off the CHARACTERS

**Chosen:** Escape, F5, F12 and the arrows are matched on `NSEvent.keyCode` (the Carbon `kVK_*` constants); everything else falls through to the first character of `charactersIgnoringModifiers`.

**Why the split:** a virtual key code is a PHYSICAL position, so it is layout-independent — which is right for keys that are the same key on every keyboard, and wrong for letters, whose position moves with the layout. Using `charactersIgnoringModifiers` for letters also keeps this edge's behaviour identical to the GTK edge's (`keyval.to_unicode()`), including its KNOWN, ACCEPTED limit: letter chords resolve under a Latin layout only (recorded in `work/notes/observations/review-nits-shortcut-resolution-in-core-and-the-gtk-edge-2026-08-04.md`). Fixing that limit unilaterally here would re-fork the vocabulary, so it is deliberately inherited rather than "improved".

**Alternative considered:** matching the named keys on AppKit's private-use function-key characters (`NSF5FunctionKey` = `U+F708`, `NSLeftArrowFunctionKey` = `U+F702`, and `U+001B` for Escape), which is the other documented route. Rejected: it makes the named keys depend on the same layout-produced string as the letters for no gain, and a layout that reports nothing for a function key would silently kill the shortcut. The key code cannot be empty.

## 4. The translation half is NOT `#[cfg(target_os = "macos")]`

**Chosen:** `crates/werust-macos/src/input.rs` takes plain numbers (`u16` key code, the modifier bits as `u64`, an `isize` button number) and is compiled and unit-tested on the ordinary Ubuntu `verify` gate, against the REAL core. Only `window.rs` — the AppKit layer that reads those numbers off an `NSEvent` — is target-gated.

**Why:** this is the same line `crates/desktop-paint` draws for display, drawn again for input, and it is load-bearing HERE more than anywhere else in the repo: nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so a wrong Cmd mapping would never be noticed by using the product. Keeping the table on the host-independent side means the whole Cmd branch — and its DISTINCTNESS from the Ctrl branch — is checked by `cargo test` on every gate run, not once a month on a `macos-14` runner.

**Consequence, and the reason `chord()` is public:** `input::action` uses `PrimaryModifier::for_target()`, which answers `Control` on the gate. So the translation is split at exactly the point where the platform convention enters: `chord(key_code, characters, flags)` is a pure fact about the native event, and the tests resolve it under `PrimaryModifier::Meta` explicitly. That is what makes "Cmd+L focuses the URL bar, Ctrl+L does not" an assertion a Linux runner can make about THIS edge's real translation.

**Alternative considered:** letting this edge name `PrimaryModifier::Meta` directly (which would make `action` itself testable on the gate). Rejected: "a Mac is the Cmd platform" is knowledge the core already owns and exposes as `for_target()`, and restating it in an edge is the beginning of a second convention table. The shape guard asserts the edge names neither variant.

## 5. Focus is reported off the FIELD EDITOR, not off the first responder alone

**Chosen:** `Focus::UrlBar` when `url_field.currentEditor()` is non-nil, or when the field itself is the window's first responder; `Focus::Page` otherwise.

**Why:** AppKit hands an edited `NSTextField`'s keyboard to a shared FIELD EDITOR (an `NSText` the window lends out), so while the user is typing the first responder is the editor, NOT the field — a naive `firstResponder == url_field` check would report `Page` for exactly the case Escape's second meaning exists for. Asking the CONTROL (`currentEditor`) is the documented way to ask "is this control being edited". The first-responder comparison is kept as the second half because a field can be first responder for the instant before its editor is installed. This is the AppKit twin of the GTK edge's problem (a `GtkEntry` delegates to an internal `GtkText`, so `has_focus` is false while typing), and it is recorded for the same reason: it is the non-obvious part.

Everything that is not the URL bar — the page, a toolbar button, the menu — is `Page`, which is the whole of the two-valued question the core asks (decision 4 in the sibling file).

## 6. The window holds the controller, and the controller no longer holds the window

**Chosen:** `Chrome` lost its `window` field; `BrowserWindow` holds the `NSWindow` and the `ShortcutWindow` holds the controller.

**Why:** the interception needs to reach the controller from the window (`sendEvent:` must be able to perform an action), and the controller already reached the window through `Chrome`. Left as it was, that is a retain cycle between two Objective-C objects that Rust's `Retained` cannot break, i.e. a real leak of a window and its whole widget tree. Moving the accessor's `Retained<NSWindow>` up to `BrowserWindow` — which already owned the controller — makes the reference one-way instead. No behaviour changed; `BrowserWindow::window()` returns the same object it always did.

## 7. The side buttons are driven one step shallower by the CI smoke, and that is stated

**Chosen:** `ShortcutWindow::claim` splits into `claim_key` (driven end to end from a synthesised `NSEvent`) and `claim_pointer_button` (driven from the button NUMBER by `BrowserWindow::press_side_button`).

**Why:** AppKit's synthetic-mouse constructor (`+mouseEventWithType:location:modifierFlags:timestamp:windowNumber:context:eventNumber:clickCount:pressure:`) takes no `buttonNumber`, so a synthesised `otherMouseDown` cannot carry the one field that matters. Driving the resolution from the number keeps EVERYTHING after `NSEvent.buttonNumber()` on the production path — translation, resolution, performer — and leaves exactly one unexercised token, which is named in `README.md` beside this file rather than quietly implied to be covered.

**Alternative considered:** building the event through `CGEvent` (`kCGMouseEventButtonNumber`) and `+[NSEvent eventWithCGEvent:]`, which CAN carry a button number. Rejected for this task: it adds a Core Graphics dependency and a second event-synthesis path to a leg nobody here can debug, to cover one accessor. Worth revisiting if the side buttons ever grow behaviour of their own.

## 8. The Mac history chord yields to text editing, and that rule lives in the CORE

**Chosen:** `crates/werust-core/src/shortcuts.rs` gained `PrimaryModifier::history_chord_is_a_text_editing_binding` (false for `Control`, true for `Meta`), and the `ArrowLeft` / `ArrowRight` rows resolve only when the chord is not a text-editing binding on that platform OR the page has focus. On a Mac, Cmd+Left / Cmd+Right therefore navigate history with the page focused and resolve to NOTHING while the URL bar is being edited, so this edge's `sendEvent:` forwards them to AppKit and the field editor moves the caret. Alt+Left / Alt+Right on Linux and Windows are UNCHANGED: they navigate from either focus.

**KNOWN, ACCEPTED LIMIT, stated plainly because the first version of this record over-claimed it:** what this protects is the URL BAR's field editor, and ONLY that. A text field INSIDE the page (a search box, a login form, a comment field) still loses Cmd+Left / Cmd+Right to history navigation, and a user typing there who presses Cmd+Left gets a navigation rather than a caret move. Three properties make that not fixable at this seam, none of them an oversight:

1. `Focus` is TWO-VALUED in the shared core (page / URL bar) by settled design (decision 4 in the sibling file): the edge answers one question rather than classifying a widget tree.
2. Even a third `Focus` value would be unimplementable at the decision point. `sendEvent:` must answer SYNCHRONOUSLY, and the `WKWebView` is opaque to its host: the app cannot know what the web content has focused without asking it ASYNCHRONOUSLY (a JS evaluation), by which time the key has already been dispatched or dropped. `shortcut_focus` therefore reports `Focus::Page` for everything inside the web view, correctly, because that is all it can honestly know.
3. werust's model gives the CHROME the first look at every event (decision 1, the AppKit analogue of the GTK capture phase). Real browsers avoid this class of collision the other way round: they let the PAGE consume the key first and claim it only if the page did not. Changing that is a whole-product architecture decision affecting all three desktop edges, not a macOS detail, and it is not this task's to make.

So the limit is ACCEPTED and recorded rather than covered. Its mitigation is decision 9: Cmd+`[` / Cmd+`]`, the Mac's other history chords, which no text field claims anywhere and which therefore reach history from inside a page text field too. If the page-focus collision is ever judged worth fixing properly, the fix is the event-order question in point 3, and it belongs in a spec of its own.

**Why it could not be fixed in this edge:** the whole point of the seam is that a chord means one thing, decided once (`werust_core::shortcuts`). "Except on macOS, where the edge drops Cmd+Arrow while the URL bar has focus" would be a second table wearing an AppKit costume, i.e. exactly the per-edge decision the seam deletes. The collision is not an AppKit implementation detail either: it is a PLATFORM CONVENTION (Cmd+Arrow is move-to-line-start/end in every Cocoa text field), which is the same kind of fact `PrimaryModifier::history` already carries, so it belongs beside it.

**Why it is gated on the platform and not on focus alone:** gating the history rows on `Focus::Page` unconditionally would have been one line shorter and would have REGRESSED the two sibling edges. Alt+Arrow is nobody's text-editing binding, and Firefox and Chrome on Linux/Windows navigate history from the URL bar happily; taking that away to fix a Mac problem would be paying for macOS's key-binding history with the other two platforms' behaviour. The condition is therefore "does this platform's history chord collide with text editing", which is the fact that actually differs.

**Refines** decision 3 in `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md` (which chose Cmd+Arrow as the Mac history chord) rather than reversing it: the chord is unchanged, its focus sensitivity is new.

**Alternatives considered:** (a) claim Cmd+Arrow unconditionally, as the first attempt did. Rejected: a user editing an address who presses Cmd+Left would get a navigation and lose the edit. (b) let the edge inspect the field editor and decline the key itself. Rejected as above: a second decision site. (c) make the core capability- or widget-aware. Rejected: the resolution stays capability-agnostic (module docs), and "is a text field being edited" is already answered by `Focus`.

**Touches:** every edge that consumes `resolve_chord`. GTK (`crates/werust/src/main.rs`) and Windows (`crates/werust-windows/src/shortcuts.rs`) are unaffected by construction, and their tests were re-run to confirm it; neither encoded an assumption that history is focus-independent (both assert their history rows with `Focus::Page`).

**Asserted, not just written down:** `crates/werust-core/src/shortcuts.rs`, `history_follows_each_platforms_own_convention_from_the_same_one_branch` (all four platform/focus combinations, in both directions), and through this edge's own translation in `crates/werust-macos/src/input.rs`, `the_mac_history_chord_is_left_to_the_field_editor_while_the_url_bar_is_edited`.

**Superseded in one respect:** the first version of this decision left Cmd+`[` / Cmd+`]` out as an unasked-for chord. The page-text-field limit above is what changed that: they are no longer a nicety but the only history spelling that is safe everywhere on this platform. See decision 9.

## 9. Cmd+`[` / Cmd+`]` are bound too, because they are the spelling that never has to yield

**Chosen:** the shared table resolves `Cmd+[` to `GoBack` and `Cmd+]` to `GoForward` on the Meta convention ONLY, in BOTH focus contexts, via `PrimaryModifier::history_is_also_spelled_with_brackets` beside the two platform facts already there.

**Why, and why now:** it is the honest mitigation for decision 8's limit. Cmd+Arrow must yield to text being edited, and inside the page werust cannot tell whether text IS being edited, so a Mac user needs a history chord that never has to yield. Safari, Chrome and Firefox all bind exactly this pair, no Cocoa text field claims it, and the mobile/desktop `Key` vocabulary already carries arbitrary characters (`Key::Character`), so nothing had to be added to it. On this edge the chords need no new translation at all: a bracket is a character key, so it arrives through the same `charactersIgnoringModifiers` path a letter does, inherits the same layout limit letters have, and reaches the core's new rows through the translation that already existed.

**Meta only, deliberately:** `Ctrl+[` is the Ctrl platforms' ESC (`^[`), and neither Firefox nor Chrome binds it to history on Linux or Windows. Claiming it there would be INVENTING a shortcut rather than following a convention, which is the opposite of what the shared table is for, and it would take `Ctrl+[` away from anything that uses it. The Ctrl platforms therefore see no change whatever from this decision: the third platform fact answers false for them, exactly as the second one does.

**Alternatives considered:** (a) leave them out, as the first version of decision 8 did. Rejected once the page-text-field limit was accepted: it would leave the platform with NO always-safe history chord and a record whose only advice was "use the mouse". (b) bind them on every platform for uniformity. Rejected as above: a shortcut nobody's browser has is a werust invention, and uniformity across platforms is not what this table promises. It promises ONE decision site, and the per-platform conventions are inputs to it. (c) add a `Key::LeftBracket` / `Key::RightBracket` to the vocabulary. Rejected: `Key::Character` already spells them, and a named variant would be a second way to say the same key.

**Refines** decision 3 in `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`, which left these chords "not adopted" but explicitly open for this task to add ("the macOS edge task can add one row here if it wants it, which is the point of the table being shared"). That entry now carries an amendment pointing here.

**Touches:** every edge that consumes `resolve_chord`, in the same additive way decision 8 does. GTK and Windows are unaffected by construction (both are `Control`), and the Windows translation cannot even produce a bracket: `shortcut_key` maps `VK_A..=VK_Z` and nothing else.

**Asserted, not just written down:** `crates/werust-core/src/shortcuts.rs`, `the_mac_spells_history_a_second_way_that_no_text_field_ever_claims` (both directions, both focus contexts, the Ctrl platform's non-claim, and the exact-match negatives that keep Safari's own Cmd+Shift+`[` alone), and through this edge's real translation in `crates/werust-macos/src/input.rs`, `the_bracket_history_chords_reach_history_from_either_focus`.

**NOT pressed on a real Mac, and that is stated rather than implied:** the `macos-14` smoke does not press them. It has one page in its history, so "Cmd+`[` really went back" is not an assertion that leg can make today, and the chords introduce no new AppKit mechanism to check: they are character keys travelling the identical `sendEvent:` -> `charactersIgnoringModifiers` -> `input::chord` path Cmd+L is pressed through there. `README.md` beside this file lists them with the rest of what awaits a human at a Mac.

## What was measured, and what was not

`README.md` beside this file states, step by step, what the Ubuntu gate proves, what the `macos-14` leg proves, and what still awaits a human at a Mac.
