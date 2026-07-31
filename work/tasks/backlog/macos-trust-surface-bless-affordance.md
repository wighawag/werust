---
title: "macOS: give the AppKit trust badge a surface the user can bless a mutable name from"
slug: macos-trust-surface-bless-affordance
blockedBy: []
covers: []
---

## What to build

Close the ONE cell the `macos` column had to mark `stubbed` when trust-on-first-use landed: `mutable-name-tofu-bless` in `docs/platform-capability-matrix.toml`. The WARNING half of TOFU already reaches macOS for free, because it is pure derivation carried on the shared `desktop-paint` snapshot (`trust_text` / `trust_detail` / `trust_color` / `error_*`): a blessed name that now points to different content already paints the `trust-name-changed` badge in the banner's red and raises the high-contrast banner on the AppKit window, with no macOS change at all. What macOS has NO affordance for is the BLESS itself.

The blocker is the widget, not the platform: the AppKit trust indicator is a plain `NSTextField` label (`crates/werust-macos/src/window.rs`), and a label has no click target. Every other edge already had a surface behind the badge and only had to add a line and a button to it: GTK a `Popover`, Android an `AlertDialog`, iOS a `UIAlertController`. macOS needs that surface built: turn the badge into something clickable (an `NSButton`, or a click gesture recogniser on the field) and open a small panel/popover carrying, in this order, the core's `trust_indicator_detail`, the core's `trust_pin_detail`, and (shown exactly when the core's `trust_pin_action_visible` is true) a button whose title is the core's `trust_pin_action_label`, activating `BrowserShell::bless_current_name`.

Take EVERY string and EVERY visibility decision from the shared derivation; all three values are already on `desktop_paint::ChromePaint` (`trust_pin_action_visible`, `trust_pin_action_label`, `trust_pin_detail`), put there by the TOFU task precisely so this edge has nothing to decide. Minting a macOS wording, or deciding locally whether the action is offered, is the drift `docs/adr/0011` exists to prevent and `crates/werust-macos/tests/macos_window_shape.rs` exists to catch.

Windows is in the same position for the same reason and is NOT covered here: the `windows` column does not exist in the matrix yet (`windows-parity-column-and-stub-tasks`). If the Win32 chrome grows the same affordance, the shared half belongs in `crates/desktop-paint`, not in either window.

## Acceptance criteria

- [ ] The macOS trust badge is CLICKABLE and opens a surface showing the posture explanation, the mutable name's current CID and blessed state, and (only when the core says the action is offered) the bless action.
- [ ] Activating it calls `BrowserShell::bless_current_name` and the chrome repaints, so the badge and banner reflect the new pin immediately.
- [ ] Every string and the action's visibility come from `ChromePaint`'s `trust_pin_*` fields; the AppKit layer decides nothing and mints no wording.
- [ ] The Ubuntu `verify` gate guards the wiring through the existing source-shape test (`crates/werust-macos/tests/macos_window_shape.rs`), since the AppKit half is never compiled there.
- [ ] The macos-14 CI leg still builds/tests/runs the window smoke green; if the smoke can read the surface back out of the real widget (as it already does for the badge text and tooltip), it does.
- [ ] The `mutable-name-tofu-bless` row's `macos` cell in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented` in the same change, naming what proves it, and the parity guard stays green with no weakening.

## Blocked by

- None. The TOFU core, the pin store and the `ChromePaint` fields all landed with `ipns-tofu-pin-and-warn-on-change`; only the AppKit affordance is missing.

## Prompt

> Goal: let a macOS user BLESS a mutable name's current CID from the trust badge, and flip the `mutable-name-tofu-bless` row's `macos` cell in `docs/platform-capability-matrix.toml` from `stubbed` to `implemented`. The trust-on-first-use WARNING already works on macOS (it is pure derivation on the shared `desktop-paint` snapshot); what is missing is the affordance, because the AppKit trust indicator is a plain `NSTextField` with no click target while GTK, Android and iOS each already had a surface behind the badge. Make the badge clickable and open a small panel/popover with, in order, `ChromePaint::trust_detail`, `ChromePaint::trust_pin_detail`, and (shown exactly when `ChromePaint::trust_pin_action_visible` is true) a button titled `ChromePaint::trust_pin_action_label` that calls `BrowserShell::bless_current_name` and repaints. Take every string and the visibility from those fields: the AppKit layer must decide nothing and mint no wording (`docs/adr/0011`). Guard the wiring in `crates/werust-macos/tests/macos_window_shape.rs` so the Ubuntu gate covers code it cannot compile, keep the macos-14 leg green, and extend the window smoke to read the surface back out of the real widget if it can. Do not touch the Win32 window: its column does not exist yet (`windows-parity-column-and-stub-tasks`), and anything shared belongs in `crates/desktop-paint`.
