---
title: "The macOS smoke's blur_url_bar does not end the field editor, so the page-focused Escape check fails and macos-renderer is RED on main"
slug: macos-smoke-blur-url-bar-does-not-end-the-field-editor
spec: chrome-conventional-controls
blockedBy: []
covers: [5]
needsAnswers: true
---

## What to build

`macos-renderer` is RED on `main`. The AppKit smoke fails exactly ONE check:

```
FAIL Escape with the PAGE focused stops the load instead of reverting the bar
window_smoke: FAIL (1 checks)
```

Every other check passes, including all the other new shortcut ones (Cmd+R re-fetches, both side buttons are claimed, an ordinary middle button stays the page's, F12 opens no inspector).

This is the DISCRIMINATING half of the focus input: the same key, the same window, a different reported focus must do something else. It is currently not discriminating, so the one guarantee that proves this edge reports focus correctly is unproven.

## CORRECTED diagnosis (the first one was WRONG; this supersedes it)

A build agent STOPPED on the original task and disproved its central premise. That analysis is ACCEPTED and is now the task. Its evidence:

`press_key -> sendEvent -> claim_key -> perform_chrome_action` is fully SYNCHRONOUS, and the smoke asserts `window.url_text() == typed` immediately after `press_key(Escape)` with no pump between. On the PAGE branch, `ChromeAction::Stop` (`crates/werust-macos/src/window.rs:1052-1055`) calls `shell.stop()` and then `refresh_chrome()`; `refresh_chrome` (`window.rs:926-932`) always calls `Chrome::apply`, which at `window.rs:385-388` overwrites the URL field with `paint.url_text` whenever it differs, and `ChromePaint::url_text` is verbatim `ChromeState::url_text` (`crates/desktop-paint/src/lib.rs:303`), i.e. the `believed` URL.

So **Stop itself rewrites the bar from `typed` to `believed` before the check reads it.** Two consequences:

1. The assertion is unreachable in BOTH focus states, because `ChromeAction::RevertUrlBar` (`window.rs:1056-1070`) also leaves the bar showing `believed`. The two branches produce IDENTICAL observable text, so the check never discriminated focus at all, even when it was written.
2. There is therefore NO evidence that `blur_url_bar` is broken. The single observed CI failure is fully explained by the Stop repaint alone.

The original theory (a surviving field editor makes `shortcut_focus` report `UrlBar`) is unproven and probably wrong. `shortcut_focus` appears CORRECT for a real user: a real click into the page moves the first responder and ends the field editor. Do not change production focus reporting.

## Acceptance criteria (RE-SCOPED — criterion 3 is now explicitly relaxed)

- [ ] `blur_url_bar` is hardened as hygiene: end the field-editor session explicitly (`NSWindow::endEditingFor(None)`) as well as moving the first responder, and HONOUR the `BOOL` from `makeFirstResponder` instead of discarding it.
- [ ] PRIMARY CHECK: assert `shortcut_focus()` reports `Focus::Page` directly after `blur_url_bar()`. This is the only assertion that actually tests focus REPORTING for this case, and it is unaffected by the repaint. Expose whatever minimal accessor the smoke needs.
- [ ] **AUTHORISED, and the point of this re-scope:** REPLACE the symptom assertion (`url_text() == typed`) with the EFFECT assertion the Windows smoke already uses (`crates/werust-windows/examples/window_smoke.rs:697-713`). With the PAGE focused, Escape must CANCEL an in-flight load (observed at the shell / the pinned fixture). With the BAR focused, Escape must revert the edit and must NOT cancel the load. This is strictly STRONGER and genuinely discriminating, so it does not weaken the safety net. The earlier "do not weaken the assertion" constraint is hereby relaxed FOR THIS ONE ASSERTION, and only in the direction of replacing it with the stronger effect-based pair.
- [ ] Do NOT change production repaint behaviour to make a bar edit survive a chrome repaint. That is a cross-edge, user-visible product decision (the GTK edge behaves identically, `crates/werust/src/main.rs:296-300` and `1089-1090`), nothing in this spec asks for it, and it is out of scope here.
- [ ] `macos-renderer` is GREEN on `main` afterwards. This is the only real evidence; the Ubuntu gate does not compile AppKit.
- [ ] Record, as a durable note, that the original check was never discriminating, so the focus half of the macOS shortcut layer was unguarded from the moment it landed until this task.

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
