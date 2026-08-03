# Conventional browser shortcuts: what landed, and how to check it by hand

Task `shortcut-resolution-in-core-and-the-gtk-edge`, spec `chrome-conventional-controls`. The judgement calls behind the design are in `DECISIONS.md` beside this file.

## Where things live

- **The decision** (what a chord MEANS): `crates/werust-core/src/shortcuts.rs`. A pure `resolve_chord(chord, focus, primary) -> Option<ChromeAction>` plus `resolve_pointer_button(button)`. Toolkit-free, display-free, and pinned by a table test that drives BOTH accelerator conventions, so the macOS `Cmd` branch is already exercised on the Linux gate.
- **The translation and the doing** (the GTK edge): `crates/werust/src/main.rs`. `shortcut_key` / `shortcut_modifiers` / `shortcut_pointer_button` map `gdk` events into that vocabulary, `shortcut_focus` reports which of the two focus contexts is live, and `perform_chrome_action` performs the result through the same `BrowserShell` the toolbar buttons drive.
- **The guard** that the edge decides nothing: `crates/werust-core/tests/shortcut_edge_wiring_shape.rs`.
- **The parity row**: `conventional-shortcuts` in `docs/platform-capability-matrix.toml`.

## The shortcut set (Linux/GTK)

| input | action |
|---|---|
| Ctrl+L | focus the URL bar and select its contents |
| Ctrl+R, F5 | reload |
| Alt+Left / Alt+Right | history back / forward (only when that move is available) |
| Escape, page focused | stop the in-flight load |
| Escape, URL bar focused | revert the edit, restore the current page's URL |
| F12 | open the WebKitGTK Web Inspector (debug builds; unchanged) |
| mouse Back / Forward side buttons | history back / forward |

Anything not in this table is propagated untouched, so ordinary typing, page keys and GTK4's interactive-debugger chords (Ctrl+Shift+I / Ctrl+Shift+D) behave exactly as before.

## Manual check (needs a display; no CI runner in this repo presses a key)

Run a debug build: `cargo run -p werust`.

1. **Ctrl+L**: the URL bar takes focus and its whole text is selected; typing replaces the address.
2. **Ctrl+R** and **F5**: the page reloads (watch the URL-bar progress and the status line).
3. Navigate to a second page, then **Alt+Left**: it goes back; **Alt+Right**: forward. At the start of history Alt+Left does nothing (the same capability flag that greys the Back button).
4. **Escape with the page focused**, during a slow load: the load stops (status settles, progress clears), exactly as the Stop button does.
5. **Escape with the URL bar focused**, after typing rubbish into it: the bar snaps back to the current page's URL and nothing navigates.
6. **F12**: the Web Inspector opens over the page. **Ctrl+Shift+I** still opens GTK's interactive debugger, not the inspector.
7. **Mouse side buttons**: press the rear thumb button on a page with history: it goes back; the forward one goes forward. This works with the pointer over the page as well as over the chrome.

Two honest limits: none of the above is measured by a runner (it needs a display and real input hardware for step 7), and the claim "the page cannot swallow these chords" rests on the controllers being in the capture phase, which step 4 is the practical check of.
