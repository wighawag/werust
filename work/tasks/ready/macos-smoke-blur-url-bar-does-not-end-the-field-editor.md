---
title: "The macOS smoke's blur_url_bar does not end the field editor, so the page-focused Escape check fails and macos-renderer is RED on main"
slug: macos-smoke-blur-url-bar-does-not-end-the-field-editor
spec: chrome-conventional-controls
blockedBy: []
covers: [5]
---

## What to build

`macos-renderer` is RED on `main`. The AppKit smoke fails exactly ONE check:

```
FAIL Escape with the PAGE focused stops the load instead of reverting the bar
window_smoke: FAIL (1 checks)
```

Every other check passes, including all the other new shortcut ones (Cmd+R re-fetches, both side buttons are claimed, an ordinary middle button stays the page's, F12 opens no inspector).

This is the DISCRIMINATING half of the focus input: the same key, the same window, a different reported focus must do something else. It is currently not discriminating, so the one guarantee that proves this edge reports focus correctly is unproven.

## The diagnosis (already done; confirm it before fixing)

In `crates/werust-macos/examples/window_smoke.rs` (~:374-390) the check does `set_url_text(typed)`, then `blur_url_bar()`, then presses Escape, and expects the typed text to SURVIVE (page-focused Escape is Stop, not RevertUrlBar).

`WerustWindow::shortcut_focus` (`crates/werust-macos/src/window.rs:987`) reports `Focus::UrlBar` when EITHER `url_field.currentEditor()` is `Some` OR the field is the window's `firstResponder`.

`blur_url_bar` (`crates/werust-macos/src/window.rs:1492`) is only:

```rust
pub fn blur_url_bar(&self) {
    let _ = self.window.makeFirstResponder(None);
}
```

Two problems. It DISCARDS the `BOOL` result, so a refused resignation is silent. And `makeFirstResponder(nil)` does not reliably tear down the FIELD EDITOR, which is the condition `shortcut_focus` checks first: `set_url_text` immediately before it touches `field.currentEditor()`, so an editor is installed, and if it survives the blur then `shortcut_focus` returns `UrlBar`, Escape resolves to `RevertUrlBar`, the bar reverts, and the check fails exactly as observed.

Note this is almost certainly a SMOKE-HARNESS defect, not a product defect: when a real user clicks into the page, AppKit moves the first responder and ends the editor properly. Confirm that before changing any production behaviour.

## Acceptance criteria

- [ ] `blur_url_bar` really takes the keyboard away from the URL bar: end the field-editor session explicitly (`NSWindow::endEditingFor(None)` is the API that tears it down) as well as moving the first responder, and do NOT silently discard the result of `makeFirstResponder`.
- [ ] After `blur_url_bar()`, `shortcut_focus()` reports `Focus::Page`. Assert this directly in the smoke, so the next failure names the CAUSE (focus misreported) rather than only the symptom (the bar reverted).
- [ ] The failing check passes: page-focused Escape leaves a half-typed URL exactly where it was.
- [ ] The URL-bar-focused half still passes: Escape with the bar focused still reverts the edit and restores the current URL.
- [ ] `macos-renderer` is GREEN on `main` afterwards.
- [ ] If, and only if, the investigation shows `shortcut_focus` itself is wrong for a REAL user (not just for this harness), fix that instead and say so plainly in the done record. Do not change production focus reporting merely to make a harness call succeed.
- [ ] Do NOT weaken or delete the check. It is the only evidence this project ever gets that the macOS edge reports focus correctly, and there is no Mac here.

## Blocked by

- None.

## Prompt

> Goal: turn the `macos-renderer` leg green by making the smoke's `blur_url_bar` actually blur, without weakening the check it feeds.
>
> The Ubuntu acceptance gate does NOT compile the AppKit edge, so a green local gate proves nothing here: the `macos-renderer` CI leg on `main` is the only evidence. Expect to be judged on that leg.
>
> Read the diagnosis above and CONFIRM it first. The likely fix is one helper, but verify that `shortcut_focus` is right for a real user before you touch it: the bug is that the TEST cannot get the window into the page-focused state, not necessarily that the edge misreports it.
>
> Do not weaken the assertion. Nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so this check is the whole safety net for the focus half of the shortcut layer.
