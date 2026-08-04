# The iOS edge-swipe gesture: the decisions this task baked in

Task `enable-the-ios-back-forward-swipe-gesture`, spec `chrome-conventional-controls` (story 13). The one-line part of this task is `webView.allowsBackForwardNavigationGestures = true`. Everything below is the part that is not one line: what WebKit does and does NOT tell the edge about a gesture-driven navigation, and what werust decided to do about it. The sibling task `ios-chrome-collapse-reload-stop-and-drop-history-buttons` removes the on-screen `◀`/`▶` buttons on the strength of this gesture, so it inherits every decision here.

**Measured 2026-08-04, before building:** the flag was still absent from the whole repo (only the tracking notes mentioned it), so the task's premise had not drifted.

**Evidence caveat, stated once and applying to all of it:** there is no Mac on this project (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so nothing here was observed on a device or a simulator. The WebKit behaviours below come from Apple's `WKNavigationDelegate` / `WKNavigationAction` documentation and the shape of the SDK, and the werust half of each decision is asserted headlessly in the pure-Rust gate (`crates/werust-ios/rust/tests/back_forward_gesture_wiring_shape.rs` plus the `IosHandle::on_history_navigated` unit tests). Where a behaviour could not be established from the documentation, it is named as an open question rather than assumed.

## 1. A gesture navigation is REPORTED into the core, never intercepted and re-driven

**Chosen:** the navigation delegate implements `decidePolicyFor` and, when `navigationAction.navigationType == .backForward`, reports the target URL into the core (`core.onHistoryNavigated(target)`) and then always `decisionHandler(.allow)`s. WebKit performs the navigation; the core is told it happened.

**Why:** the swipe is handled entirely inside WebKit. It calls none of the shell's actions, so unless the edge reports it the core never learns the user moved, and the URL bar, the trust posture and the Back/Forward capability flags all keep describing the document the user just swiped AWAY from. That is the "subtler version of the same bug" the task names.

**Alternative considered and rejected:** cancel the `.backForward` navigation (`decisionHandler(.cancel)`) and drive `core.goBack()` instead, which would make the gesture literally the same code path as the `◀` button and need no new signal at all. Rejected twice over: cancelling an INTERACTIVE gesture snaps the swipe animation back under the user's finger, and the core's Back is performed as a fresh `WKWebView.load`, so the page would be re-fetched and re-laid-out instead of restored, losing scroll position. A gesture that feels broken is worse than no gesture. The guard asserts the hook never contains `.cancel`, `core.goBack()` or a `.load(`, so a later "simplification" back to this shape reds the gate.

**Why `decidePolicyFor` and not `didCommit`:** it is the only callback that NAMES the navigation `.backForward` (a commit cannot tell a swipe from a link click to the same URL), and it is the EARLIEST — it fires before the target document's bytes are resolved, which is what lets decision 4's posture reset happen without clobbering the verification the `ipfs` scheme handler performs for the NEW page moments later.

**Touches:** the iOS C-ABI surface (a new `werust_ios_on_history_navigated` export + its header declaration + the `WerustCore.swift` binding). Nothing outside the iOS edge: Android needs no twin, because its system Back is INTERCEPTED into `core.goBack()` and its WebView therefore never drives its own back-forward list.

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

## 4. Entering a document by gesture RESETS the per-load trust axes

**Chosen:** the move resets `TrustPosture` to `UnverifiedOrigin` and clears the ENS / mutable-name axes, the same reset a fresh `begin()` performs.

**Why:** the target is a DIFFERENT document. Swiping back from a hash-verified `ipfs://` page onto a plain served one would otherwise leave the badge claiming `content-verified` for bytes nobody verified — the overclaim `docs/adr/0006` exists to forbid, on the one platform where no human here can see it happen.

**The cost, accepted:** when WebKit restores a verified page from its page cache, no scheme task re-runs, so nothing re-marks it and the badge understates the page's trust. werust may show LESS trust than a page has; it may never show more. (The ENS axes are re-derived on every pump by `refresh_chrome` for a known ENS entry, so the name half recovers on its own.)

**Deliberately NOT generalised:** `on_url_changed` still keeps the current posture, because it is contracted as a SAME-DOCUMENT signal where that is correct. That a cross-document LINK click also arrives on it, and so keeps the previous page's posture, is a pre-existing gap with the same root cause and a different fix; it is captured at `work/notes/observations/ios-webview-initiated-navigation-keeps-the-previous-postures-2026-08-04.md` rather than smuggled into this task.

## 5. No new row in the platform-capability matrix

**Chosen:** `docs/platform-capability-matrix.toml` is left untouched.

**Why:** the matrix already speaks about this gesture, deliberately, from OUTSIDE its rows: the `system-back-navigates-history` row marks iOS and macOS `n-a` and says in both cells that the WKWebView swipe is "a distinct affordance with its own enablement (`allowsBackForwardNavigationGestures`)", tracked in the observation notes rather than folded into that row. Adding a `back-forward-gesture` capability now would force a cell for all five platforms, and three of those cells are other people's open questions (the macOS two-finger swipe in `work/notes/observations/macos-swipe-back-gesture-not-enabled-2026-07-31.md`, the WebView2 touch swipe in `windows-back-affordances-not-bound-2026-07-31.md`, and whatever GTK's answer is). Answering three platforms' questions as a side effect of turning on one flag is precisely the wrong layer for that decision.

**What a reviewer should push back on:** if the matrix is meant to be the one index of "which platform has which affordance", this gesture belongs in it and the row should be minted by a task that can answer all five cells. This note is here so that stays a visible choice rather than an omission.
