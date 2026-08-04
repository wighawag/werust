---
title: "iOS has no edge-swipe back: `allowsBackForwardNavigationGestures` is never set, so WKWebView's default (off) stands"
slug: enable-the-ios-back-forward-swipe-gesture
spec: chrome-conventional-controls
blockedBy: []
covers: [13]
needsAnswers: true
---

## What to build

Measured 2026-08-01 while tasking `chrome-conventional-controls`: `allowsBackForwardNavigationGestures` appears NOWHERE in this repo. `WKWebView` defaults it to `false`, so the iOS shell has **no edge-swipe history navigation at all** — the gesture every other iOS browser has, and the one iOS users reach for first.

Enable it, and assert that it stays enabled.

**This is small but load-bearing.** It is a real gap on its own (an iOS user today can only navigate history through toolbar buttons), and it is the PREREQUISITE for the sibling task that removes those buttons. Removing them while the gesture is off would leave iOS with no way to go back whatsoever, which nobody on this project could discover by using it: there is no Mac here, so CI is the only evidence this platform ever gets (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`). That is exactly why the assertion matters more than the one-line change.

Note the asymmetry the spec accepts: the gesture gives BACK and forward via swipe, so unlike Android (whose system Back has no forward equivalent) iOS keeps both directions after its buttons go.

## Acceptance criteria

- [ ] `allowsBackForwardNavigationGestures` is enabled on the shell's `WKWebView`.
- [ ] A test asserts it is enabled, so a future refactor cannot silently return to the default and strand history navigation.
- [ ] Swipe-driven navigation reports through the SAME load-lifecycle path as a button-driven one, so the chrome (URL bar, trust posture, history capability flags) stays correct after a swipe rather than going stale.
- [ ] The assertion is one a CI runner can make without a human at a Mac.
- [ ] Tests network-isolated; mirror the repo's existing test style.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: turn on iOS edge-swipe history navigation, and assert it stays on.
>
> The iOS shell controller builds a `WKWebView` and a toolbar of buttons. `allowsBackForwardNavigationGestures` is never set anywhere in the repo, and WKWebView's default is `false`, so the swipe gesture iOS users expect simply does not exist in werust today. Set it, and pin it.
>
> The assertion is the point, not the flag. There is no Mac on this project, so nobody can discover a regression by using the app (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), and the sibling task `ios-chrome-collapse-reload-stop-and-drop-history-buttons` REMOVES the toolbar buttons on the strength of this gesture existing. If the flag silently reverts, that edge loses history navigation entirely with no human able to notice. Write the assertion so a CI runner catches it, in the style the iOS crate's existing checks use.
>
> Also verify the chrome stays correct after a swipe: a gesture-driven navigation must drive the same load-lifecycle signals a button-driven one does, so the URL bar, trust posture and history capability flags do not go stale. A gesture that navigates the WebView without informing the core would be a subtler version of the same bug.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): re-confirm the flag is still absent, since a sibling task may have set it meanwhile.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record, in particular anything you discover about how WKWebView reports gesture-driven navigations differently from programmatic ones.

---

### Claiming this task

```sh
dorfl claim enable-the-ios-back-forward-swipe-gesture --arbiter origin
git fetch origin && git switch -c work/enable-the-ios-back-forward-swipe-gesture origin/main
git mv work/tasks/ready/enable-the-ios-back-forward-swipe-gesture.md work/tasks/done/enable-the-ios-back-forward-swipe-gesture.md
```

## Requeue 2026-08-04

Gate 2 BLOCKED the previous attempt with two findings. Your committed work on this branch is kept and is being CONTINUED: build on it, do not restart.

FIX 1 (main-frame guard, correctness + security). The new decidePolicyFor hook in crates/werust-ios/App/Sources/WKWebViewShellController.swift (~:681-702) reports ANY .backForward navigation into the core, including SUBFRAME ones. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues a .backForward policy decision per child frame carrying that FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates forward history and pushes the iframe url as the current entry. The URL bar then shows a subresource address the user is not on (attacker-controlled for a hostile iframe). Guard on navigationAction.targetFrame?.isMainFrame == true. The idiom is ALREADY used in this same file for the _blank case (~:650), so follow it. Pin the guard in back_forward_gesture_wiring_shape.rs.

FIX 2 (acceptance criterion 3 is not met: the chrome DOES go stale after a swipe). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does). BrowserShell::go_back DOES clear last_error and invalid_entry explicitly. Repro: load a, navigate to b which FAILS (error banner shown), swipe back to a; the failed page's banner keeps showing over a page that loaded fine. Make a gesture-driven history move reach the same chrome state a button-driven one does. The existing test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back over-claims parity because it only exercises an ERROR-FREE history: extend it to cover the failed-load case so the parity claim is real.

Constraints that still bind: do NOT weaken or edit crates/werust-core/tests/mobile_chrome_presentation_shape.rs. Do not re-select the toolchain (rust-toolchain.toml is pinned). Keep user-facing chrome strings coming from the ONE core derivation. Conventional-commit subjects.

## Gate-3 conductor verdict (drive-tasks)

APPROVE, on the SECOND attempt. Gate 2 blocked the first attempt with two findings; both are fixed and pinned.

- `allowsBackForwardNavigationGestures` enabled: `WKWebViewShellController.swift:276`. MET.
- Assertion that it stays enabled, makeable by CI with no human at a Mac: `the_shell_enables_the_back_forward_swipe_gesture_on_its_webview` reads the Swift source. MET.
- Gesture navigation reports through the SAME load-lifecycle path, chrome does not go stale: `go_back`/`go_forward` and the reported gesture move now share ONE `BrowserShell::enter_history_entry()`, which applies the per-entry chrome reset (`last_error`, `invalid_entry`). MET.
- Tests network-isolated, repo test style. MET.

Gate-2 block 1 (MAIN-FRAME guard) FIXED: `navigationAction.targetFrame?.isMainFrame == true` at `WKWebViewShellController.swift:700`, pinned by `only_a_main_frame_history_navigation_is_reported_as_the_page_the_user_is_on`, which asserts the guard's presence in the Swift source. Without it a hostile iframe's `.backForward` decision could push a subresource URL into the URL bar and truncate history.

Gate-2 block 2 (stale error banner) FIXED: the parity test no longer over-claims. `the_chrome_after_a_swipe_back_off_a_failed_load_matches_the_button_back` covers the failed-load case, and `the_chrome_after_a_swipe_back_off_a_rejected_url_entry_matches_the_button_back` covers the orthogonal invalid-entry axis.

Guard check: `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` NOT touched. `rust-toolchain.toml` NOT touched.

CI VERIFIED on main (the Linux gate never compiles Swift, so this is the real evidence): `mobile-ios` SUCCESS, plus `macos-renderer`, `windows-renderer` and `verify` all SUCCESS on commit `feat(enable-the-ios-back-forward-swipe-gesture)`.

Six non-blocking Gate-2 nits are in `work/notes/observations/review-nits-enable-the-ios-back-forward-swipe-gesture-2026-08-04.md`, plus two new agent-filed observations (`ios-stop-does-not-stop-the-wkwebview`, `ios-webview-initiated-navigation-keeps-the-previous-postures`).
