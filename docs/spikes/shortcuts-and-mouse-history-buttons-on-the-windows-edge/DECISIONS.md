# The Windows shortcut edge: the decisions this task baked in

Task `shortcuts-and-mouse-history-buttons-on-the-windows-edge`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind `crates/werust-windows/src/shortcuts.rs`, the wiring in `crates/werust-windows/src/window.rs` and the one engine addition in `crates/windows-renderer/src/backend.rs`.

Nothing here re-decides what a chord MEANS: that is `werust_core::shortcuts`, and its own decisions are recorded in `docs/spikes/shortcut-resolution-in-core-and-the-gtk-edge/DECISIONS.md`. Every entry below is about how a Win32 input REACHES that resolution, which is the only thing this edge was free to choose.

## 1. Two Win32 messages that are NOT one-to-one with the abstract vocabulary

**`WM_SYSKEYDOWN` is the same key press as `WM_KEYDOWN`.** Windows delivers a key held with **Alt** as a SYSTEM key down, and the history chords (Alt+Left / Alt+Right) are exactly that. Both messages carry the same virtual-key code, so the translation treats them identically and the edge simply listens to both, in both delivery paths (the message-loop filter, and the WebView2 accelerator hook's `COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN`). A filter that listened only to `WM_KEYDOWN` would have silently shipped a browser with no keyboard history navigation, which is why it is called out here rather than left as a line of code.

**`WM_APPCOMMAND` is translated to the same `PointerButton`s as `WM_XBUTTONDOWN`,** and this is the one place this edge's word for an input is WIDER than the physical button. `APPCOMMAND_BROWSER_BACKWARD` / `_FORWARD` arrive from two different places: `DefWindowProc` synthesises them when an X-button click on a CHILD window goes unhandled (which is the only way a click over WebView2's page window can reach werust at all -- that window belongs to the browser process), and a keyboard with dedicated Back/Forward keys sends them directly.

**Considered and rejected:** teaching the shared vocabulary a new input (an `AppCommand`, or a `PointerButton::BrowserBack`). It would be a Win32-shaped word in a toolkit-free module, for zero behavioural difference: the resolved ACTION is `GoBack`/`GoForward` either way, which is also the right answer for the keyboard's browser keys. **Also rejected:** handling `WM_APPCOMMAND` only, and dropping `WM_XBUTTONDOWN`. It would leave the chrome-area click depending on `DefWindowProc`'s synthesis of an event this window can read directly, and the smoke could no longer post the real message.

**Consequence, paid deliberately:** `WM_XBUTTONUP` is SWALLOWED (`LRESULT(1)`) rather than passed to `DefWindowProc`. Otherwise one click would navigate twice: once from this window's `WM_XBUTTONDOWN` handler and once from the `WM_APPCOMMAND` `DefWindowProc` generates from the release.

**Touches:** the macOS sibling task only as a warning that a native edge may need more than one message per input; nothing in the core.

## 2. The keyboard is hooked in the MESSAGE LOOP, not in a window procedure

**Chosen:** `filter_shortcut` is called on every message before `TranslateMessage`, in BOTH loops this shell has (the product's `GetMessageW` loop and the smoke's `pump_messages`). It filters only messages for this window and its children.

**Why:** Win32 delivers a key to the FOCUSED control, and the shell's focus can be an `EDIT` (which eats Escape and every arrow key), a `BUTTON` (which forwards nothing to its parent) or the window itself. A `WM_KEYDOWN` arm in the window procedure would therefore catch a shortcut only in the one case where nothing else has the focus. The loop is this edge's equivalent of the GTK shell's CAPTURE-phase controller (decision 6 of the core task's DECISIONS), and it earns the same trade: a browser's own chords beat the focused widget, and everything unclaimed is dispatched untouched.

**A second, real benefit:** a claimed message never reaches `TranslateMessage`, so no `WM_CHAR` is synthesised for it. That is what keeps the URL bar from beeping at the control character a claimed chord would otherwise produce (`Ctrl+L` is `0x0C`), without a per-character list of which chords the chrome consumes -- which would have been a second, drifting copy of the shortcut table.

**Considered and rejected:** `TranslateAccelerator` over an `HACCEL`. It is the classic Win32 idiom, but the accelerator table would be a SECOND place that says which chord exists, in Win32's own encoding, next to the shared resolution -- exactly the per-edge table this seam deletes. The filter asks the core instead.

## 3. The page's keys come through the ENGINE, because the page is in another process

**Chosen:** `crates/windows-renderer` grows one hook, `AcceleratorKeys` (`Webview2Renderer::accelerator_keys()`, wired to `ICoreWebView2Controller::add_AcceleratorKeyPressed` at realisation). It carries a virtual-key code out and a yes/no back. The WINDOW translates, resolves and performs; the engine learns nothing about chords.

**Why an engine change at all:** WebView2 hosts the page in the browser process, so a key pressed over the page is delivered to a window this process does not own and NEVER appears in this thread's message loop. Microsoft documents `AcceleratorKeyPressed` as the way an app sees those keys; there is no other. Without it the conventional shortcuts would work only while the chrome had the focus -- and the shell focuses the PAGE at startup, so that is the normal case, i.e. the acceptance criterion would have been false in the one state the user is usually in.

**Why it does not widen the `Renderer` seam:** it is a concrete method on the Windows backend, taken as a handle BEFORE the backend is boxed -- exactly the shape `DevTools` already has, and for the same reason (the window that answers does not exist yet). The seam trait, and `ChromeState`'s capability flags, are untouched by this task.

**Why a claimed key is POSTED, never performed inline:** Microsoft documents the windowed-mode handler as running synchronously with the browser process BLOCKED, so a COM call back into WebView2 from inside it fails with `RPC_E_CANTCALLOUT_ININPUTSYNCCALL`. Every chrome action ends in such a call. So the handler marks the key handled and posts `WM_WERUST_CHROME_ACTION`, carrying the action's slot in the CORE's `ChromeAction::ALL` (never a Win32 list of its own), and the window performs it a moment later on its own loop. This mirrors Microsoft's own sample, which performs the action asynchronously.

**A claimed key is marked `Handled(TRUE)`,** so WebView2's OWN built-in accelerators (its Ctrl+R, F12, Alt+Arrow) do not act as well: werust's chrome is the one that reloads, and it does it through the shell so the URL bar, the trust badge and the status line stay true. A key werust does NOT claim is left entirely alone, so WebView2 behaves exactly as it did before this task.

**Touches:** `crates/windows-renderer` (one new public handle) and the Windows CI leg, which builds both.

## 4. The virtual-key codes are spelled out, and cross-checked at compile time

**Chosen:** the translation is a NON-`cfg`-gated module holding plain `u16` constants (`VK_ESCAPE = 0x1b`, …) rather than the `windows` crate's `VK_*`; `window.rs` then asserts each one against the SDK's own value in a `const` block that only compiles on Windows.

**Why:** the same reason `crate::dpi` reproduces `MulDiv`'s arithmetic instead of calling it -- a module that used the `windows` crate could only be compiled, let alone tested, on Windows, and this repo's `verify` gate is Ubuntu. The cost is a transcription risk (a mistyped code is a shortcut that silently never fires), and the `const` cross-check removes it for free on the one host where both spellings exist.

**Sub-decision, a letter comes from its virtual-key code, not from the active layout.** `winuser.h` gives `VK_A`-`VK_Z` the ASCII capitals' values, so `Ctrl+L` translates to `Character('l')` with no `MapVirtualKeyW` call and no layout lookup. This keeps the translation pure, and it is also the closest Windows equivalent of the GTK edge's keyval-to-character step. The known Latin-layout limit recorded in `work/notes/observations/review-nits-shortcut-resolution-in-core-and-the-gtk-edge-2026-08-04.md` is INHERITED here, deliberately unfixed: fixing it in one edge would re-fork the vocabulary the seam exists to share.

**Sub-decision, `GetKeyState` in the message-loop filter and `GetAsyncKeyState` in the accelerator hook.** The first is synchronised with the message being processed, which is the state that key press was made with; but this thread's queue never saw the page's key press at all, so the accelerator path has to read the CURRENT physical state instead. The pure translation takes the reader as a parameter, so both paths run the same code and the tests drive it with a fake keyboard. Reading either as `!= 0` rather than `< 0` would report a toggled-on Caps/Num Lock as a held modifier and kill every chord; that is asserted, not just avoided.

## 5. Escape's focus is answered with ONE question

**Chosen:** `Controller::focus_context()` is `GetFocus() == url_edit`, and everything else -- the page, a toolbar button, the window -- is `Focus::Page`. The page-focused accelerator path reports `Focus::Page` unconditionally, because that is what the event MEANS (WebView2 raises it only while the page has the focus).

**Why:** the shared resolution's `Focus` has exactly two values on purpose, so an edge answers one question instead of classifying its widget tree. Anything cleverer here would be this edge deciding what Escape means.

## 6. The old edge-local F12 branch is GONE

The URL bar's subclass used to catch F12 itself and post an `ID_DEV_TOOLS` command. That was this edge's own one-key shortcut table, and it is now a row in the shared one like every other chord: the filter claims F12, the core resolves `OpenWebInspector`, and the performer opens Edge's own DevTools. The `ID_DEV_TOOLS` command id is deleted with it, so there is no second path left to drift. `BrowserWindow::open_dev_tools()` (the smoke's entry point) still calls the same handle.
