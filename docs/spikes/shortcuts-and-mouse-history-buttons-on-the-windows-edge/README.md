# The conventional shortcuts on the Windows edge: what is proved, and what a human still has to press

Task `shortcuts-and-mouse-history-buttons-on-the-windows-edge`, spec `chrome-conventional-controls`. The judgement calls are in [`DECISIONS.md`](DECISIONS.md) beside this file; the shared resolution every edge inherits is `crates/werust-core/src/shortcuts.rs` and its own decisions are in `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`.

## The shape, in one paragraph

What a chord or a mouse side button MEANS was decided once, in the toolkit-free core, by the GTK tracer-bullet task. This edge adds two things and no third: a TRANSLATION (`crates/werust-windows/src/shortcuts.rs`, pure, unit-tested on the Ubuntu gate, turning Win32 virtual-key codes, `GetKeyState` bits and `XBUTTON1`/`XBUTTON2` into the shared vocabulary) and an EXECUTION (`perform_chrome_action` in `crates/werust-windows/src/window.rs`, driving the same `BrowserShell` the toolbar buttons drive, gated on the same `ChromeState` capability flags). The edge reports focus; it never decides what Escape means.

Where Windows differs from GTK is only in HOW an input reaches the edge, and that is entirely because WebView2 hosts the page in a separate process:

| input | how it reaches this edge |
|---|---|
| a key with the CHROME focused (URL bar, a toolbar button, the window) | the message-loop pre-filter `filter_shortcut`, before `TranslateMessage` |
| a key with the PAGE focused | the engine's `add_AcceleratorKeyPressed` hook -> `claim_accelerator_key` -> a posted `WM_WERUST_CHROME_ACTION` |
| a mouse side button over the CHROME | `WM_XBUTTONDOWN` in the window procedure |
| a mouse side button over the PAGE (or any child window) | `WM_APPCOMMAND` (`APPCOMMAND_BROWSER_BACKWARD` / `_FORWARD`) bubbled up by `DefWindowProc` |

## What CI proves

The Ubuntu `verify` gate (pure Rust, no Windows SDK):

* the WHOLE translation, as unit tests beside it: every chord in the shared table reached from the virtual-key codes and modifier state Win32 delivers, Escape differing only by the reported focus, the unclaimed keys resolving to nothing (so typing and caret movement survive), the Windows key reported as `meta`, a lock key never breaking a chord, and both mouse routes (`WM_XBUTTONDOWN`'s high word, `WM_APPCOMMAND`'s masked command) landing on the same `PointerButton`s;
* the SHAPE of the Win32 half it cannot compile, in `crates/werust-windows/tests/windows_window_shape.rs`: that the edge asks the shared resolution, names no key outside the translation, handles every `ChromeAction::ALL` entry, drives history through the existing seam and capability flags, swallows `WM_XBUTTONUP` so one click cannot navigate twice, and that the engine hook carries a virtual-key code and no chord vocabulary.

The `windows-latest` leg (`.github/workflows/windows-renderer.yml`), through `examples/window_smoke.rs`, on a REAL window:

* a real `WM_KEYDOWN` for F5, posted into the real message loop, reloads the page THROUGH THE SHELL (proved by a fresh console row from the fixture's own `console.log`, not by a flag);
* the PAGE-focused path end to end minus the keystroke itself: `claim_accelerator_key(VK_F5)` translates, claims, posts `WM_WERUST_CHROME_ACTION`, and the window performs it on its own loop;
* an unclaimed key (`VK_TAB`) is refused, so the page keeps everything werust says nothing about;
* a real `WM_XBUTTONDOWN` carrying `XBUTTON1` navigates history back, after two loads, and the URL bar shows the earlier page again.

The compile-time cross-check `_VIRTUAL_KEY_CODES_MATCH_THE_SDK` in `window.rs` pins every virtual-key code the pure module spells out against the SDK's own `VK_*`, so the price of keeping the translation host-independent cannot become a silently dead shortcut.

## What still awaits real Windows hardware

Nothing in CI presses a key or clicks a mouse: every check above POSTS a message or calls a function. Three claims are therefore still unmeasured, and all three are about the platform's delivery, not about werust's logic:

1. **A modified chord.** `GetKeyState` answers from the thread's message queue, and a posted message carries no modifier state, so Ctrl+L, Ctrl+R and Alt+Left/Right are exercised as translation (on the Ubuntu gate) but never as real key presses. A human has to press them.
2. **The page-focused delivery.** That WebView2 really raises `AcceleratorKeyPressed` for these keys, that marking them handled really stops its own built-in accelerators (its Ctrl+R, F12 and Alt+Arrow), and that the posted action really runs after the callback returns.
3. **The side buttons over the PAGE.** This is the weakest claim in the whole task: `WM_APPCOMMAND` reaching this window depends on the WebView2 page window's own handling of an X-button click, in a process werust does not own. Over the CHROME the buttons are proved (the smoke posts the real message); over the page they are expected, not measured.

## Manual verification

On a real Windows desktop, with `werust-windows.exe` on any page:

1. **Ctrl+L** focuses the URL bar AND selects it (typing replaces the address). Press it with the page focused and again with the bar already focused.
2. **Ctrl+R** and **F5** reload; the URL bar's progress strip appears and the status line moves, i.e. the load went through werust's shell rather than through WebView2's own reload.
3. **Alt+Left** / **Alt+Right** move through history, and do NOTHING at the ends (the Back/Forward buttons are greyed there: a shortcut may never beat the on-screen control).
4. **Escape** over the page cancels an in-flight load (start a slow one first); **Escape** in the URL bar, after typing something else, restores the current page's URL and leaves the caret in the bar.
5. **F12** opens Edge's DevTools over the page (debug build only, as every other edge's inspector row is), from BOTH focuses.
6. **Mouse buttons 4 and 5** navigate history: click them over the toolbar AND over the page. If the page-area click does nothing, that is the `WM_APPCOMMAND` claim above failing, and it belongs in `work/notes/observations/` rather than in a silent workaround.
7. **Ordinary typing still works.** Type an address, use the arrow keys and Home/End inside the URL bar, and press Enter: no chord may swallow a key werust does not claim, and the bar must never beep.
