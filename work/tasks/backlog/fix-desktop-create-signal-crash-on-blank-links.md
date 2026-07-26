---
title: "FIX CRASH: desktop connect_create returns the existing view, aborting WebKitGTK (WindowFeatures optional) on a target=_blank click; route new-window via decide-policy instead"
slug: fix-desktop-create-signal-crash-on-blank-links
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

CRASH REGRESSION (v0.2.5, human, DESKTOP, reproducible): on `ronan.eth` -> click "portfolio" (loads) -> click an external `target="_blank"` link (e.g. "stratagems") -> werust ABORTS with:

```
/usr/lib/gcc/.../optional:482: ... std::_Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...:
  Assertion 'this->_M_is_engaged()' failed.
fish: Job 1, './werust' terminated by signal SIGABRT (Abort)
```

ROOT CAUSE (confirmed): the `blank-and-window-open-links-navigate-in-place` fix (task `blank-and-window-open-links-navigate-in-place`, `docs/adr/0010`) wired `WebView::connect_create` in `crates/webview-renderer/src/backend.rs` (`install_new_window_in_place`) and, after loading the target URI in place, RETURNS THE EXISTING VIEW (`view_for_create.clone().upcast::<gtk4::Widget>()`). WebKitGTK's `create` signal contract (webkitgtk.org signal.WebView.create) is: the handler must return "a newly allocated WebKitWebView widget or NULL to propagate the event further." Returning the EXISTING view is neither a new related view nor NULL, so WebKit proceeds to apply the new window's `WindowFeatures` to the returned view, dereferences an EMPTY `std::optional<WebCore::WindowFeatures>`, and ABORTS. Every external `ronan.eth` link is `target="_blank"`, so clicking any of them crashes the browser.

The webkit6-0.4.0 Rust binding types `connect_create` as `Fn(&Self, &NavigationAction) -> gtk::Widget` (a NON-nullable Widget return), which is WHY the author returned a view rather than NULL - there is no ergonomic way to return NULL through that typed binding. So the fix is NOT to keep using `create`; route the new-window request EARLIER, through `decide-policy`, which cannot crash and needs no view return.

THE FIX (route via `decide-policy`, remove the crashing `create` handler):
- Replace the `connect_create` handler with a `connect_decide_policy` handler (webkit6 `WebView::connect_decide_policy`, `Fn(&Self, &PolicyDecision, PolicyDecisionType) -> bool`). When `decision_type == PolicyDecisionType::NewWindowAction` (WebKitGTK fires this for a `_blank`/`window.open` request BEFORE `create`):
  - downcast the `PolicyDecision` to `NavigationPolicyDecision`, read its `NavigationAction`'s request URI (the `_blank`/`window.open` target),
  - apply the SAME shared `renderer::new_window_action` rule; on `NavigateInPlace { url }`, `self.view.load_uri(&url)` (in-place) + `life.begin(&url)` (so the bar follows), exactly as before,
  - call `decision.ignore()` on the policy decision (so WebKit does NOT proceed to create a new window / fire `create`), and return `true` (handled).
  - For any other `decision_type` (NavigationAction, Response), return `false` (let WebKit's default handling proceed) - do NOT change normal in-page navigation or resource loading.
- REMOVE the `connect_create` registration entirely (it is the crash). `install_new_window_in_place` becomes the decide-policy wiring; keep the same public method name + call site (`crates/werust/src/main.rs:435`) or rename + update the call site, whichever is cleaner - record it.
- Everything else stays: the in-place load goes through the SAME `load_uri` (so an `ipfs://`/ENS `_blank` target is still hash-verified by `install_ipfs`, an unsupported scheme still refused - no trust bypass), no second window is created, and the shell's URL bar follows the new URL. ADR-0010's decision (in-place until tabs exist) is unchanged; only the MECHANISM moves from `create` to `decide-policy`.

VERIFY IT ACTUALLY FIXES THE CRASH: the crash is a runtime WebKitGTK abort (not caught by `cargo test`), so the diagnosis + the manual reproduction (ronan.eth -> portfolio -> a `_blank` link -> no crash, loads in place) MUST be recorded in a `docs/spikes/<slug>/` note, and the strongest automatable guard added at the seam (see criteria).

## Acceptance criteria

- [ ] The reproducible crash is GONE: on desktop, clicking a `target="_blank"` link (e.g. an external link on ronan.eth after navigating to portfolio) loads the target IN THE CURRENT view without aborting - no `WindowFeatures` / `_M_is_engaged` SIGABRT.
- [ ] The crashing `connect_create` handler is REMOVED; the new-window request is routed via `connect_decide_policy` on `PolicyDecisionType::NewWindowAction` (which returns `bool`, never a view, so it cannot hit the WindowFeatures abort).
- [ ] A `_blank`/`window.open` target still loads IN THE CURRENT view (no second window), through the SAME `load_uri` path, so an `ipfs://`/ENS target is still hash-verified and an unsupported scheme refused (no trust bypass); the URL bar follows the new URL.
- [ ] Normal in-page navigation and resource loading are unchanged (only `NewWindowAction` decisions are intercepted; `NavigationAction`/`Response` decisions fall through to default).
- [ ] The fix + a manual desktop reproduction (before: crash; after: loads in place) is recorded in `docs/spikes/<slug>/`. The strongest automatable guard rides `cargo test`: the shared `renderer::new_window_action` routing rule stays unit-tested, and any seam-level assertion of the in-place-load routing that is testable without a live WebView is added.
- [ ] iOS/Android `_blank` handling is UNCHANGED (they use their own native hooks, not `create`; this crash is desktop-WebKitGTK-specific) - confirm no regression to them.

## Blocked by

- None. (Highest priority - a reproducible crash in the shipped v0.2.5 desktop build. Supersedes the `create`-based mechanism from `blank-and-window-open-links-navigate-in-place`.)

## Prompt

> Goal: FIX a reproducible desktop CRASH. On ronan.eth, click portfolio then any external `target="_blank"` link -> werust SIGABRTs: `WebCore::WindowFeatures ... _M_is_engaged() failed`. Cause: the `blank-and-window-open-links-navigate-in-place` fix wired `WebView::connect_create` in `crates/webview-renderer/src/backend.rs` (`install_new_window_in_place`) and returns the EXISTING view; WebKitGTK's `create` contract requires a NEW view or NULL, so returning the existing view makes it apply an empty `WindowFeatures` optional and abort. The webkit6 binding types `connect_create` as returning a non-nullable `gtk::Widget`, so you cannot return NULL through it.
>
> FIX: remove the `connect_create` handler and route the new-window request via `connect_decide_policy` instead (returns `bool`, no view). On `PolicyDecisionType::NewWindowAction`: downcast the `PolicyDecision` to `NavigationPolicyDecision`, read its navigation action's request URI, apply the shared `renderer::new_window_action` rule, on `NavigateInPlace{url}` do `self.view.load_uri(&url)` + `life.begin(&url)`, call `decision.ignore()`, return `true`. Other decision types return `false` (default handling). Keep the in-place load through the SAME `load_uri` (ipfs:// still hash-verified, unsupported refused - no trust bypass), no second window, bar follows the URL. iOS/Android unchanged (native hooks, not `create`).
>
> Done = the crash is gone (record the manual ronan.eth -> portfolio -> _blank repro, before/after, in `docs/spikes/<slug>/`), connect_create removed, decide-policy NewWindowAction routing in place, normal navigation unchanged, the shared new_window_action rule still unit-tested. FIRST re-check `install_new_window_in_place` still returns the existing view from `connect_create`.
