# The iOS edge-swipe gesture: the decisions this task baked in

Task `enable-the-ios-back-forward-swipe-gesture`, spec `chrome-conventional-controls` (story 13). The one-line part of this task is `webView.allowsBackForwardNavigationGestures = true`. Everything below is the part that is not one line: what WebKit does and does NOT tell the edge about a gesture-driven navigation, and what werust decided to do about it. The sibling task `ios-chrome-collapse-reload-stop-and-drop-history-buttons` removes the on-screen `◀`/`▶` buttons on the strength of this gesture, so it inherits every decision here.

**Measured 2026-08-04, before building:** the flag was still absent from the whole repo (only the tracking notes mentioned it), so the task's premise had not drifted.

**Evidence caveat, stated once and applying to all of it:** there is no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so nothing here was observed on a device or a simulator. The WebKit behaviours below come from Apple's `WKNavigationDelegate` / `WKNavigationAction` documentation and the shape of the SDK, and the werust half of each decision is asserted headlessly in the pure-Rust gate (`crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs` plus the `IosHandle::on_history_navigated` unit tests). Where a behaviour could not be established from the documentation, it is named as an open question rather than assumed.

## 1. A gesture navigation is REPORTED into the core, never intercepted and re-driven

**Chosen:** the navigation delegate implements `decidePolicyFor` and, when `navigationAction.navigationType == .backForward` **and the navigation targets the MAIN FRAME**, reports the target URL into the core (`core.onHistoryNavigated(target)`) and then always `decisionHandler(.allow)`s. WebKit performs the navigation; the core is told it happened.

**Why:** the swipe is handled entirely inside WebKit. It calls none of the shell's actions, so unless the edge reports it the core never learns the user moved, and the URL bar, the trust posture and the Back/Forward capability flags all keep describing the document the user just swiped AWAY from. That is the "subtler version of the same bug" the task names.

**Alternative considered and rejected:** cancel the `.backForward` navigation (`decisionHandler(.cancel)`) and drive `core.goBack()` instead, which would make the gesture literally the same code path as the `◀` button and need no new signal at all. Rejected twice over: cancelling an INTERACTIVE gesture snaps the swipe animation back under the user's finger, and the core's Back is performed as a fresh `WKWebView.load`, so the page would be re-fetched and re-laid-out instead of restored, losing scroll position. A gesture that feels broken is worse than no gesture. The guard asserts the hook never contains `.cancel`, `core.goBack()` or a `.load(`, so a later "simplification" back to this shape reds the gate.

**Why `decidePolicyFor` and not `didCommit`:** it is the only callback that NAMES the navigation `.backForward` (a commit cannot tell a swipe from a link click to the same URL), and it is the EARLIEST — it fires before the target document's bytes are resolved, which is what lets decision 4's posture reset happen without clobbering the verification the `ipfs` scheme handler performs for the NEW page moments later.

**Touches:** the iOS C-ABI surface (a new `werust_ios_on_history_navigated` export + its header declaration + the `WerustCore.swift` binding), and one new `BrowserShell` method in the shared core (decision 6). Android needs no twin, because its system Back is INTERCEPTED into `core.goBack()` and its WebView therefore never drives its own back-forward list.

## 2. A gesture navigation is a history MOVE, matched against the ADJACENT entries

**Chosen:** `IosHandle::on_history_navigated(url)` moves the session-history cursor onto the entry the user swiped to: back if the PREVIOUS entry is that URL, forward if the NEXT one is, and otherwise it falls back to pushing. Back is checked first, so an ambiguous history (`[a, b, a]` standing on `b`, swiping to `a`) resolves as a step back.

**Why a move and not a push:** the existing webview-initiated signal, `on_url_changed` (the KVO on `webView.url`, which SPA `pushState` navigations arrive on), PUSHES. Reported that way, a swipe back from `b` to `a` would leave the history `[a, b, a]`: Forward would read false while the user can plainly swipe forward, and every swipe would leak another entry. A swipe RE-ENTERS an entry the session already has; that difference is the whole reason this is a separate signal rather than a second caller of `on_url_changed`.

**Why matched by URL rather than by a direction the edge reports:** `WKNavigationAction` says a navigation is `.backForward` without saying which WAY it went. The direction is recoverable from `webView.backForwardList` (is the target in `backList` or `forwardList`?), but that would put browsing logic in Swift, which is exactly what this codebase keeps out of the OS edge, and it would be unassertable in the gate. Matching adjacent entries in Rust is testable on Ubuntu and needs no SDK. The ambiguous case needs the same URL on both sides of the cursor, and either reading leaves the bar on the same address.

**Why a non-adjacent target is FOLLOWED (pushed) rather than ignored:** WebKit's back-forward list and the core's session history are two stacks that genuinely drift, because a core-driven history move is performed as a fresh `WKWebView.load`, which APPENDS to WebKit's list instead of moving its cursor. So a gesture can land on a URL that is neither of the core's neighbours. For a browser whose thesis is an honest address and an honest trust posture, showing an address the user is not on is the worst available outcome, so the bar follows.

**Not adopted:** making the core's history a mirror of `WKBackForwardList` (or driving history through `webView.goBack()`), which would delete the drift at its root. That is a change to the whole iOS `Renderer` backend, to which the desktop/Android history semantics are the shared reference, and it is exactly the "hides an unresolved design decision" shape this task is too small to carry. Recorded here as the real fix if the drift ever bites.

## 3. The move emits NO `Started` and queues NO pending load

**Chosen:** `on_history_navigated` emits only `LoadEvent::UrlChanged` and never sets `pending_load`. The load state is moved by the ordinary `didCommit` / `didFinish` that follow.

**Why no pending load:** WebKit is already performing this navigation. A pending load would be drained by the shell's next pump and turned into a `WKWebView.load` on top of it, i.e. a double navigation (and, since that load appends to WebKit's list, more drift). Pinned by an assertion that the gesture queues nothing.

**Why no `Started`:** a swipe onto a SAME-DOCUMENT history entry (an SPA `pushState` entry) fires no `didCommit` and no `didFinish` at all, so a `Started` there would never be settled and the chrome would show a spinner and an enabled Stop button forever. Skipping it costs the progress line on a gesture-driven cross-document load, which is the cheaper loss: a back navigation is usually served from WebKit's page cache and settles immediately.

**Open question (unmeasurable here):** whether WebKit calls `decidePolicyFor` at all for a SAME-DOCUMENT back/forward. If it does, the entry is handled exactly as above; if it does not, that swipe keeps arriving on the KVO `url` observer and pushes, as it does today. Either way nothing regresses, which is why the shape above was chosen over one that depends on the answer.

**Consequence, and where it is paid:** emitting no `Started` is also what made the first attempt at this task get the CHROME wrong, because `Started` is the one lifecycle arm that clears the error banner. That is fixed properly in decision 6 rather than by emitting a load event that did not happen.

## 4. Entering a document by gesture RESETS the per-load trust axes

**Chosen:** the move resets `TrustPosture` to `UnverifiedOrigin` and clears the ENS / mutable-name axes, the same reset a fresh `begin()` performs.

**Why:** the target is a DIFFERENT document. Swiping back from a hash-verified `ipfs://` page onto a plain served one would otherwise leave the badge claiming `content-verified` for bytes nobody verified — the overclaim `docs/adr/0006` exists to forbid, on the one platform where no human here can see it happen.

**The cost, accepted:** when WebKit restores a verified page from its page cache, no scheme task re-runs, so nothing re-marks it and the badge understates the page's trust. werust may show LESS trust than a page has; it may never show more. (The ENS axes are re-derived on every pump by `refresh_chrome` for a known ENS entry, so the name half recovers on its own.)

**Deliberately NOT generalised:** `on_url_changed` still keeps the current posture, because it is contracted as a SAME-DOCUMENT signal where that is correct. That a cross-document LINK click also arrives on it, and so keeps the previous page's posture, is a pre-existing gap with the same root cause and a different fix; it is captured at `work/notes/observations/ios-webview-initiated-navigation-keeps-the-previous-postures-2026-08-04.md` rather than smuggled into this task.

## 5. No new row in the platform-capability matrix

**Chosen:** `docs/platform-capability-matrix.toml` is left untouched.

**Why:** the matrix already speaks about this gesture, deliberately, from OUTSIDE its rows: the `system-back-navigates-history` row marks iOS and macOS `n-a` and says in both cells that the WKWebView swipe is "a distinct affordance with its own enablement (`allowsBackForwardNavigationGestures`)", tracked in the observation notes rather than folded into that row. Adding a `back-forward-gesture` capability now would force a cell for all five platforms, and three of those cells are other people's open questions (the macOS two-finger swipe in `work/notes/observations/macos-swipe-back-gesture-not-enabled-2026-07-31.md`, the WebView2 touch swipe in `windows-back-affordances-not-bound-2026-07-31.md`, and whatever GTK's answer is). Answering three platforms' questions as a side effect of turning on one flag is precisely the wrong layer for that decision.

**What a reviewer should push back on:** if the matrix is meant to be the one index of "which platform has which affordance", this gesture belongs in it and the row should be minted by a task that can answer all five cells. This note is here so that stays a visible choice rather than an omission.

## 6. The chrome reset a history move makes is SHARED, through one new `BrowserShell` method

*Added by the 2026-08-04 requeue: Gate 2 found that acceptance criterion 3 was not actually met.*

**The bug it fixes.** `on_history_navigated` emits only `LoadEvent::UrlChanged` (decision 3), and in `BrowserShell::pump` the `UrlChanged` / `Committed` / `Finished` arms never touch `chrome.last_error` — only `Started` does. `BrowserShell::go_back` clears `last_error` and `invalid_entry` explicitly, so the two paths disagreed exactly where it hurts: load `a`, navigate to `b`, which FAILS (the red banner is up), swipe back to `a`, and the dead page's banner keeps showing over a page that loaded fine. On a phone there is no chrome control that dismisses it. The same held for the invalid-entry badge and its PINNED typed text (`fail_invalid_entry` sets `url_override`), so a rejected URL-bar entry survived the swipe too.

**Chosen:** one new public method on the shared shell, `BrowserShell::note_history_navigated()`, and a private `enter_history_entry()` that `go_back`, `go_forward` and it all call. It is `go_back` **with the navigation already done**: it drives no backend navigation, it only applies the per-entry chrome reset (fresh redirect chain, drop the pinned name/typed text, clear the resolving step, clear `last_error`, clear `invalid_entry`, refresh). The iOS `CoreSession::on_history_navigated` calls it when — and only when — the backend actually MOVED, which is why `IosHandle::on_history_navigated` now returns a bool: a repeat report of the entry already shown must not silently dismiss THAT entry's own error banner.

**Why the shared core and not the iOS crate:** `last_error` / `invalid_entry` / `url_override` are private `BrowserShell` state, and rightly so. The alternative was to make the reset reachable per-field, which is a wider and worse surface than one method whose contract is "a history move happened".

**Alternative considered and rejected — emit `LoadEvent::Started`:** it would clear the banner through the existing pump arm and need no new API at all, but it CLAIMS a load began. A swipe onto a same-document entry starts no load and fires no `didCommit`/`didFinish`, so the chrome would spin forever (decision 3), and the debug console's load-lifecycle view would show a load that never existed. A lie on the seam to reuse a side effect.

**Alternative considered and rejected — a new `LoadEvent::HistoryMoved` variant:** the honest version of the above, and the shape to reach for if a SECOND backend ever moves its own history. Rejected as too wide for this task: `LoadEvent` is the cross-backend seam, so every backend and every consumer (GTK, macOS, Windows, native, Android) would grow a variant only iOS can emit, for a difference that is entirely about chrome bookkeeping and not about the load lifecycle at all. Recorded here as the migration if that changes.

**Alternative considered and rejected — re-drive `shell.go_back()` and drain the pending load:** it reuses the exact button path, but it re-arms the redirect back-skip (which can issue a FURTHER backend Back, i.e. a real navigation fighting WebKit's gesture) and it races the Swift edge for `take_pending_load`. `note_history_navigated` instead ENDS any in-flight back-skip, on the same rule `go_forward` uses: the user has taken the history over themselves.

**Touches:** `BrowserShell`'s public surface (one method), the iOS `CoreSession` + `IosHandle` (the bool), and nothing else — `go_back`/`go_forward` keep their exact behaviour, now expressed through the shared helper so the gesture path and the button path cannot drift apart again. Pinned by `a_platform_driven_history_move_resets_the_chrome_a_button_back_resets` (core) and the three `the_chrome_after_a_swipe_back_*_matches_the_button_back` parity tests (iOS), which now cover a settled load, a FAILED one and a REJECTED entry — the first attempt's single error-free case is what let the claim be false.

## 7. Only a MAIN-FRAME back-forward navigation is reported

*Added by the 2026-08-04 requeue: Gate 2 found this as a correctness + security defect.*

**The bug it fixes.** WebKit issues a policy decision PER FRAME. A back navigation onto a page carrying iframes — or an iframe of the current page calling `history.back()` — therefore delivers `.backForward` decisions whose `request.url` is the SUBFRAME's. The first attempt reported every one of them. Since a subframe url is neither neighbour of the current entry, the core took decision 2's DRIFT branch: it truncated the forward history and pushed the subresource as the current entry, so the URL bar showed an address the user was not on — for a hostile iframe, one the attacker chose. That is precisely the overclaim a browser whose thesis is an honest address cannot make.

**Chosen:** `navigationAction.targetFrame?.isMainFrame == true`, checked BEFORE the report. The idiom is already used in this same file for the `_blank` case (`targetFrame == nil`), so the edge now reads one way about frames.

**Why the guard is in Swift and not in the core:** the core has no frame model at all (nothing on the `Renderer` seam carries one), and inventing one to defend against a signal the edge should never have sent would be the wrong layer. The edge's job is to report the page the user is on; only the edge knows which frame that is.

**Why `targetFrame` and not `sourceFrame`:** the target frame is the one being navigated, which is the question being asked. `sourceFrame` is whoever initiated it, and a main-frame navigation initiated BY a subframe (an iframe calling `top.history.back()`) is a real main-frame move that must still be reported.

**Pinned by** `only_a_main_frame_history_navigation_is_reported_as_the_page_the_user_is_on`, which asserts both the guard and its ORDER relative to the report — a guard placed after the call would be no guard at all.
