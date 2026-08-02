---
title: "The chrome should behave like a browser: conventional keyboard shortcuts, a loading spinner, and history buttons only where the platform lacks its own gesture"
slug: chrome-conventional-controls
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/` items. TASKED 2026-08-01 into nine tasks; the Implementation and Testing detail moved into those tasks, which own what to build.

## Problem Statement

werust's chrome works but does not behave like a browser, and two specific gaps show it.

**It ignores the keyboard conventions every browser shares.** The entire application currently binds exactly one key, F12 for the web inspector. There is no Ctrl+L, so reaching the URL bar means moving a hand to the mouse; no Ctrl+R; no Alt+Left. A user's twenty years of muscle memory does nothing here, which makes a technically-capable browser feel unfinished within ten seconds of use.

**Loading feedback is one signal where browsers give two.** The URL-bar progress bar is good and stays. But there is no spinner, so a load that stalls before any progress is reported is indistinguishable from an idle browser.

Separately, the history buttons are in the wrong place on mobile: Android has a hardware/gesture Back already wired to page history, and iOS has the WebKit edge-swipe, so on both the toolbar buttons duplicate an affordance the platform already provides and spend scarce toolbar width doing it. On desktop they are conventional and stay.

## Solution

The chrome adopts the conventions a user already knows.

- **Keyboard**: the shortcuts a browser is expected to have, resolved in ONE place so every edge agrees and the Cmd-versus-Ctrl difference is handled once rather than per edge.
- **Loading**: a spinner joins the existing URL-bar progress bar, both driven by the same `is_loading` truth, with reload and stop collapsing into the single button browsers use.
- **History buttons**: kept on desktop, removed from both mobile edges, where the platform's own gesture is the affordance.

The trust indicator's visual language is deliberately NOT part of this spec; it turned out to be a trust-model decision rather than a chrome-polish one and now lives in `work/specs/proposed/trust-indicator-and-details-panel.md` and `docs/adr/0012`.

## User Stories

1. As a keyboard user, I want Ctrl+L to focus the URL bar and select its contents, so that I can type a new address without touching the mouse.
2. As a keyboard user, I want Ctrl+R and F5 to reload, so that refreshing works the way it does in every other browser.
3. As a keyboard user, I want Alt+Left and Alt+Right to go back and forward, so that history navigation does not require the toolbar.
4. As a user on a Mac, I want Cmd+L and Cmd+R rather than Ctrl, so that the shortcuts match the platform I am on.
5. As a user with a load in flight, I want Escape to stop it when the page has focus, so that I can abandon a hanging page from the keyboard.
6. As a user typing in the URL bar, I want Escape to revert my edit and restore the current page's URL, so that Escape does the same thing it does in other browsers rather than one thing everywhere.
7. As a mouse user, I want the back and forward mouse buttons (4 and 5) to navigate history, so that the hardware I already own works.
8. As a user waiting on a page, I want a spinner while it loads, so that I can tell werust is working even before progress is measurable.
9. As a user, I want the existing URL-bar progress bar kept, so that I keep the fine-grained signal I already have.
10. As a user, I want the reload button to become a stop button while a page is loading, so that the control matches what the browser is doing and I am not hunting for a separate Stop.
11. As a mobile user, I want the toolbar free of back and forward buttons, so that the URL bar gets the width instead of controls that duplicate my phone's own Back gesture.
12. As an Android user, I want the hardware Back button to keep navigating page history, so that removing the toolbar buttons costs me nothing.
13. As an iOS user, I want the edge-swipe gesture to navigate history, so that removing the toolbar buttons costs me nothing there either.
14. As a desktop user, I want the back and forward buttons kept, so that the chrome still matches desktop browser convention.
15. As a user who has learned werust's F12 web inspector, I want it to keep working exactly as it does, so that a new shortcut layer does not break the one binding that already exists.

## Out of Scope

- **Find-in-page and bookmarks**, and therefore Ctrl+F and Ctrl+D. Binding a shortcut to a feature that does not exist is worse than leaving the key free.
- **Tabs**, and any shortcut that implies them (Ctrl+T, Ctrl+W, Ctrl+Tab). `docs/adr/0010` records that `_blank` and `window.open` navigate in place until tabs exist.
- **The trust indicator's icon and its details panel.** Moved to `trust-indicator-and-details-panel` + `docs/adr/0012`. (A separate PRIVACY indicator was analysed and deferred there; do not add one here.)
- **Removing the desktop back/forward buttons.** Explicitly rejected by the human: desktop keeps them, matching every desktop browser.
- **Hardware-keyboard shortcuts on the mobile edges.** Confirmed at tasking: no story asks for them, and the shortcut tasks cover the three desktop edges only.

## Further Notes

**Two things were MEASURED at tasking time and changed the work; both are now carried by the tasks.**

`allowsBackForwardNavigationGestures` is set nowhere in the repo, and `WKWebView` defaults it to false, so iOS has no edge-swipe back today. Story 13 was therefore a gap to FIX, not a precondition to verify, and it became its own task that the iOS button-removal task is blocked on. Removing those buttons first would have left iOS with no history navigation at all, which nobody here could have discovered by using it (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`).

This spec originally required the shortcut layer to land before ANY history button was removed. At tasking the human relaxed that for the MOBILE edges deliberately: that ordering exists to guarantee a keyboard fallback, and mobile has no keyboard. Android's system Back is already wired to history, and iOS's affordance is the swipe, so the mobile removals are blocked on their own real prerequisites instead. The desktop buttons are not being removed at all, so the original constraint has no remaining subject.
