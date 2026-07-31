---
title: "Windows: give the trust indicator a surface the user can open, and put the TOFU bless action in it"
slug: windows-trust-surface-bless-affordance
blockedBy: []
covers: []
---

## What to build

Close one of the two capabilities the `windows` parity column had to mark `stubbed`: `mutable-name-tofu-bless` in `docs/platform-capability-matrix.toml`.

Every other edge lets the user RECORD (bless) a mutable name's current CID as the version they trust, from an explicit action reached off the trust indicator: the GTK badge is a `MenuButton` opening a `Popover` carrying the posture explanation, the core's TOFU detail line and the bless button; iOS and Android put the same section into the explanation alert a badge TAP already opened. The Win32 window has no such surface at all — its trust indicator is a plain `STATIC` label with a tooltip (`crates/werust-windows/src/chrome.rs`), which the user cannot click. So on Windows a mutable name can be WARNED about (the change warning is pure derivation and reaches the window through the shared paint snapshot) but never blessed, and the SSH-host-key model `docs/adr/0006` describes is half-present.

Everything the surface must SHOW and DECIDE already exists and must not be re-decided here. The pin store, the wire form, the wording of the action and the decision of WHETHER it is offered are the shared core (`werust_core::pins` + `trust_pin_action_visible` / `trust_pin_action_label` / `trust_pin_detail`), and all three values are already fields on `desktop_paint::ChromePaint`, the snapshot this window already reads every refresh. The action itself is the one shared `BrowserShell::bless_current_name`. What is missing is purely the Win32 surface and the wire.

**The surface is a DESIGN choice this task must make and RECORD**, not one the parity column made for it. Win32 offers several idiomatic answers — a click on the badge opening a `TrackPopupMenu`, a small owner-drawn popup window (the closest analogue of the GTK popover, and the only one that can show a wrapped multi-line explanation plus a button), or turning the badge into a real `BUTTON` with a dropdown. Pick one, say why, and say what it costs: the explanation sentence is long (it is the same ~240-character text the tooltip carries), the TOFU detail line names the name, the day it was trusted and both CIDs, and the window ships without a comctl32 v6 manifest (`docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §4), so whatever is used must draw acceptably in the classic style.

Constraints that matter more than the wiring:

- **The Win32 layer decides nothing.** The label, the visibility and the detail text are read from the paint snapshot; `crates/werust-windows/tests/windows_window_shape.rs` exists to keep the Win32 half a painter and will red the gate if a rule or a literal appears there. Keep it that way (and extend it, in its existing style, to cover the new surface).
- **Do not add an item to the ⋮ menu.** Its items are the shared `werust_core::menu::BrowserMenu`; adding one there changes every platform's menu. The action belongs behind the trust indicator, which is where every other edge puts it and where the core's visibility rule is scoped.
- **Blessing writes a file.** It is an I/O-bearing action on the message-loop thread; keep it off any path that must stay instant, in the spirit of the Android `driveCore` treatment, and make sure the chrome repaints from the shell afterwards rather than from a locally-remembered answer.
- **The pin stays ADVISORY.** It must never steer a load and never upgrade a posture: an unblessed name must remain byte-for-byte the pre-TOFU chrome on Windows.
- **macOS has the SAME gap** (`macos-trust-surface-bless-affordance`, still open) on a different toolkit. The shared half is already shared; whichever of the two lands second must reuse the core's rule and the `ChromePaint` fields rather than forking anything, and must not change the other edge's behaviour by accident.

## Acceptance criteria

- [ ] On Windows, the trust indicator opens a surface the user can reach with the mouse, carrying the posture EXPLANATION and, exactly when the core says the action is visible, the TOFU detail line plus an action titled with the core's label.
- [ ] Activating it drives `BrowserShell::bless_current_name`, the pin is persisted in the shared store, and a later resolution of that name to a different CID raises the existing change warning on Windows.
- [ ] Nothing is re-derived at the edge: the label, the visibility and the detail text come from the shared paint snapshot, asserted by an extension to `crates/werust-windows/tests/windows_window_shape.rs` in its existing style (the gate must red if the Win32 layer starts deciding any of them).
- [ ] The `windows-renderer` CI leg's `window_smoke` drives the surface on a real window: with a blessable state it opens, holds the core's label and detail, and the action reaches the shell; with a non-blessable state the action is absent.
- [ ] An unblessed name's chrome is unchanged (the pin steers no load and upgrades no posture), covered by a test.
- [ ] The `mutable-name-tofu-bless` row's `windows` cell in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented` in the same change, naming what proves it; the parity guard stays green with no weakening.
- [ ] The surface choice (and any wrinkle the missing visual-styles manifest forces) is recorded durably and linked from the done record.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Blocked by

- None — the Windows engine and window have both landed (`windows-webview2-renderer-backend`, `windows-win32-window-and-chrome`), and the core's TOFU rules and pin store shipped with `ipns-tofu-pin-and-warn-on-change`.

## Prompt

> Goal: give the Windows shell a trust surface with the TOFU bless action in it, and flip the `mutable-name-tofu-bless` row's `windows` cell in `docs/platform-capability-matrix.toml` from `stubbed` to `implemented`. Today the Win32 trust indicator is a plain `STATIC` with a tooltip and no click target (`crates/werust-windows/src/chrome.rs`), so a mutable name can be warned about on Windows but never blessed — the desktop GTK badge is a `MenuButton` opening a `Popover` with the explanation, the core's TOFU detail line and the bless button, and both mobile edges put the same section in the badge-tap alert. Everything you need to SHOW already reaches the window: `trust_pin_action_visible` / `trust_pin_action_label` / `trust_pin_detail` are fields on the shared `desktop_paint::ChromePaint` this window paints from every refresh, and the action is the one shared `BrowserShell::bless_current_name`. Choose the Win32 surface deliberately (a `TrackPopupMenu` off the badge, a small owner-drawn popup window, a `BUTTON` with a dropdown), RECORD the choice and why — remember the explanation is a long sentence and the window ships without a comctl32 v6 manifest, so it draws classic-styled. Do NOT add an item to the ⋮ menu: those items are the shared `BrowserMenu` and would change every platform. Keep the Win32 layer a painter (extend `crates/werust-windows/tests/windows_window_shape.rs` so the gate reds if it starts deciding the label or the visibility), keep the pin ADVISORY (it steers no load and upgrades no posture, `docs/adr/0006`), and remember blessing writes a file. Prove it on the `windows-renderer` leg by driving the surface in `window_smoke`. macOS has the identical gap (`macos-trust-surface-bless-affordance`): reuse the shared rule, do not fork it, and do not change the AppKit edge by accident.
