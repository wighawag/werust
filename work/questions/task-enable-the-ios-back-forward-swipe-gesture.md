<!-- dorfl-sidecar: item=task:enable-the-ios-back-forward-swipe-gesture type=task slug=enable-the-ios-back-forward-swipe-gesture allAnswered=false -->

## Q1

**'task:enable-the-ios-back-forward-swipe-gesture' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new decidePolicyFor hook has no MAIN-FRAME guard: it reports ANY .backForward navigation, including SUBFRAME ones, into the core. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues .backForward policy decisions for each child frame carrying the FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates the forward history and PUSHES the iframe url as the current entry, so the URL bar shows a subresource address the user is not on (an attacker-controlled one for a hostile iframe) and the just-left entry is destroyed. This is the honest-address failure the task and DECISIONS.md invoke. The same file already uses navigationAction.targetFrame for the _blank case, so the idiom is present; add targetFrame?.isMainFrame == true (and pin it in back_forward_gesture_wiring_shape.rs) or record why subframe reports are safe. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:681-702 (no isMainFrame check) vs :650 (targetFrame used for _blank); drift branch backend.rs on_history_navigated)
> - A gesture move does not clear the error banner, so the chrome DOES go stale after a swipe (acceptance criterion 3). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does, plus the explicit verbs). BrowserShell::go_back clears last_error and invalid_entry explicitly. So: load a, navigate to b which FAILS (banner shown), swipe back to a, and the failed page's error banner keeps showing over a page that loaded fine, until some later explicit navigate/reload. The button-vs-swipe equality test only exercises an error-free history, so it over-claims parity. Either clear the error on a gesture move or state why the divergence is acceptable and pin it. (werust-core/src/lib.rs:2785 UrlChanged arm (no last_error reset) vs :2408-2425 go_back; test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:enable-the-ios-back-forward-swipe-gesture' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new decidePolicyFor hook has no MAIN-FRAME guard: it reports ANY .backForward navigation, including SUBFRAME ones, into the core. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues .backForward policy decisions for each child frame carrying the FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates the forward history and PUSHES the iframe url as the current entry, so the URL bar shows a subresource address the user is not on (an attacker-controlled one for a hostile iframe) and the just-left entry is destroyed. This is the honest-address failure the task and DECISIONS.md invoke. The same file already uses navigationAction.targetFrame for the _blank case, so the idiom is present; add targetFrame?.isMainFrame == true (and pin it in back_forward_gesture_wiring_shape.rs) or record why subframe reports are safe. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:681-702 (no isMainFrame check) vs :650 (targetFrame used for _blank); drift branch backend.rs on_history_navigated)
> - A gesture move does not clear the error banner, so the chrome DOES go stale after a swipe (acceptance criterion 3). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does, plus the explicit verbs). BrowserShell::go_back clears last_error and invalid_entry explicitly. So: load a, navigate to b which FAILS (banner shown), swipe back to a, and the failed page's error banner keeps showing over a page that loaded fine, until some later explicit navigate/reload. The button-vs-swipe equality test only exercises an error-free history, so it over-claims parity. Either clear the error on a gesture move or state why the divergence is acceptable and pin it. (werust-core/src/lib.rs:2785 UrlChanged arm (no last_error reset) vs :2408-2425 go_back; test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:enable-the-ios-back-forward-swipe-gesture' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new decidePolicyFor hook has no MAIN-FRAME guard: it reports ANY .backForward navigation, including SUBFRAME ones, into the core. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues .backForward policy decisions for each child frame carrying the FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates the forward history and PUSHES the iframe url as the current entry, so the URL bar shows a subresource address the user is not on (an attacker-controlled one for a hostile iframe) and the just-left entry is destroyed. This is the honest-address failure the task and DECISIONS.md invoke. The same file already uses navigationAction.targetFrame for the _blank case, so the idiom is present; add targetFrame?.isMainFrame == true (and pin it in back_forward_gesture_wiring_shape.rs) or record why subframe reports are safe. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:681-702 (no isMainFrame check) vs :650 (targetFrame used for _blank); drift branch backend.rs on_history_navigated)
> - A gesture move does not clear the error banner, so the chrome DOES go stale after a swipe (acceptance criterion 3). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does, plus the explicit verbs). BrowserShell::go_back clears last_error and invalid_entry explicitly. So: load a, navigate to b which FAILS (banner shown), swipe back to a, and the failed page's error banner keeps showing over a page that loaded fine, until some later explicit navigate/reload. The button-vs-swipe equality test only exercises an error-free history, so it over-claims parity. Either clear the error on a gesture move or state why the divergence is acceptable and pin it. (werust-core/src/lib.rs:2785 UrlChanged arm (no last_error reset) vs :2408-2425 go_back; test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:enable-the-ios-back-forward-swipe-gesture' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new decidePolicyFor hook has no MAIN-FRAME guard: it reports ANY .backForward navigation, including SUBFRAME ones, into the core. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues .backForward policy decisions for each child frame carrying the FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates the forward history and PUSHES the iframe url as the current entry, so the URL bar shows a subresource address the user is not on (an attacker-controlled one for a hostile iframe) and the just-left entry is destroyed. This is the honest-address failure the task and DECISIONS.md invoke. The same file already uses navigationAction.targetFrame for the _blank case, so the idiom is present; add targetFrame?.isMainFrame == true (and pin it in back_forward_gesture_wiring_shape.rs) or record why subframe reports are safe. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:681-702 (no isMainFrame check) vs :650 (targetFrame used for _blank); drift branch backend.rs on_history_navigated)
> - A gesture move does not clear the error banner, so the chrome DOES go stale after a swipe (acceptance criterion 3). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does, plus the explicit verbs). BrowserShell::go_back clears last_error and invalid_entry explicitly. So: load a, navigate to b which FAILS (banner shown), swipe back to a, and the failed page's error banner keeps showing over a page that loaded fine, until some later explicit navigate/reload. The button-vs-swipe equality test only exercises an error-free history, so it over-claims parity. Either clear the error on a gesture move or state why the divergence is acceptable and pin it. (werust-core/src/lib.rs:2785 UrlChanged arm (no last_error reset) vs :2408-2425 go_back; test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):

## Q5

**'task:enable-the-ios-back-forward-swipe-gesture' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new decidePolicyFor hook has no MAIN-FRAME guard: it reports ANY .backForward navigation, including SUBFRAME ones, into the core. On a back navigation to a page with iframes (or an iframe calling history.back()), WebKit issues .backForward policy decisions for each child frame carrying the FRAME's url; the core then sees a non-adjacent target, takes the drift branch, truncates the forward history and PUSHES the iframe url as the current entry, so the URL bar shows a subresource address the user is not on (an attacker-controlled one for a hostile iframe) and the just-left entry is destroyed. This is the honest-address failure the task and DECISIONS.md invoke. The same file already uses navigationAction.targetFrame for the _blank case, so the idiom is present; add targetFrame?.isMainFrame == true (and pin it in back_forward_gesture_wiring_shape.rs) or record why subframe reports are safe. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:681-702 (no isMainFrame check) vs :650 (targetFrame used for _blank); drift branch backend.rs on_history_navigated)
> - A gesture move does not clear the error banner, so the chrome DOES go stale after a swipe (acceptance criterion 3). on_history_navigated emits only LoadEvent::UrlChanged, and in BrowserShell the UrlChanged / Committed / Finished arms never reset chrome.last_error (only Started does, plus the explicit verbs). BrowserShell::go_back clears last_error and invalid_entry explicitly. So: load a, navigate to b which FAILS (banner shown), swipe back to a, and the failed page's error banner keeps showing over a page that loaded fine, until some later explicit navigate/reload. The button-vs-swipe equality test only exercises an error-free history, so it over-claims parity. Either clear the error on a gesture move or state why the divergence is acceptable and pin it. (werust-core/src/lib.rs:2785 UrlChanged arm (no last_error reset) vs :2408-2425 go_back; test the_chrome_after_a_swipe_back_matches_the_chrome_after_a_button_back)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q5 fields: id=q5 kind=stuck -->

**Your answer** (write below this line):
