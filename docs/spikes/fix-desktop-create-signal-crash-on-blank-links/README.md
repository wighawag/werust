# Desktop SIGABRT on a `target="_blank"` click: diagnosis, fix, and the recorded before/after repro

The shipped v0.2.5 desktop build ABORTED (`SIGABRT`) whenever a page made a new-window request. It is fixed by answering WebKitGTK's `create` signal with a real **NULL** widget through the RAW glib signal API instead of returning the existing view through the typed `connect_create` binding. This note records the crash, why the obvious fix does not work, the mechanism actually chosen, and the measured before/after on the real `ronan.eth` fixture.

Task: `fix-desktop-create-signal-crash-on-blank-links`. Supersedes the desktop mechanism recorded by `blank-and-window-open-links-navigate-in-place` / `docs/adr/0010` (that task's in-place DECISION stands; only the desktop MECHANISM changed).

## The crash

Reproduction (human, v0.2.5, desktop): open `ronan.eth`, click "portfolio", then click any external link (they are all `target="_blank"`). werust aborts with:

```
/usr/lib/gcc/x86_64-linux-gnu/14/../../../../include/c++/14/optional:482:
  const _Tp &std::_Optional_base_impl<WebCore::WindowFeatures, std::_Optional_base<WebCore::WindowFeatures>>::_M_get() const
  [_Tp = WebCore::WindowFeatures, _Dp = std::_Optional_base<WebCore::WindowFeatures>]:
  Assertion 'this->_M_is_engaged()' failed.
```

Root cause: WebKitGTK's `create` signal contract is "return a NEWLY ALLOCATED `WebKitWebView`, or NULL to propagate the event further". The first version of `WebViewRenderer::install_new_window_in_place` returned the **EXISTING** view (`view.clone().upcast::<gtk4::Widget>()`). That answer is neither, so WebKit proceeded to apply the pending window's `WindowFeatures` to the returned view and dereferenced an EMPTY `std::optional<WebCore::WindowFeatures>`, aborting the process. It fires on BOTH triggers (a `target="_blank"` link click and a `window.open(url)` call), not just `_blank`.

Why the existing view was returned in the first place: the typed webkit6 binding is generated as `connect_create<F: Fn(&Self, &NavigationAction) -> gtk::Widget>` (`webkit6-0.5.0/src/auto/web_view.rs:1690`) — a **non-nullable** `gtk::Widget` return. There is no way to answer NULL through it.

## Why not `decide-policy` (the mechanism the task first prescribed)

Routing the new-window request via `connect_decide_policy` on `PolicyDecisionType::NewWindowAction` cannot crash (it returns `bool`, never a view), but it only covers HALF the capability:

- A `target="_blank"` link click goes through the FrameLoader named-target path, which calls `PolicyChecker::checkNewWindowPolicy` and DOES emit a `NewWindowAction` decision.
- `window.open(url)` does NOT: `LocalDOMWindow::open` -> `WebCore::createWindow` -> `Chrome::createWindow` -> `webkitWebViewCreateNewPage` emits `create` directly, never passing through `checkNewWindowPolicy`. Measured with a scratch GTK harness on WebKitGTK 2.52.3 / webkit6 0.5.0: with only a decide-policy handler installed, a `window.open` click produced NO decision at all and the view stayed put (a dead link), with and without `javascript_can_open_windows_automatically`.

So decide-policy-only would have silently REGRESSED `window.open` on desktop, contradicting `docs/adr/0010`, the `blank-window-open-navigates-in-place` capability row, and the `window.open`-framed seam tests. The task was requeued (`## Requeue 2026-07-26` in the task body) with the corrected mechanism below.

## The fix (mechanism)

`WebViewRenderer::install_new_window_in_place` (`crates/webview-renderer/src/backend.rs`) now wires `create` through the RAW glib signal API:

```rust
self.view.connect_local("create", false, move |args| {
    let target = args
        .get(1)
        .and_then(|arg| arg.get::<webkit6::NavigationAction>().ok())
        .and_then(|mut action| action.request().and_then(|req| req.uri()));
    if let NewWindowAction::NavigateInPlace { url } = new_window_action(target.as_deref()) {
        life.borrow_mut().begin(&url);
        view_for_create.load_uri(&url);
    }
    Some(None::<gtk4::Widget>.to_value())
});
```

The raw signal API has no nullability restriction, so the handler answers a real NULL `GtkWidget`: WebKitGTK creates no second view and applies no `WindowFeatures` to anything, so there is no deref and no abort. ONE hook still covers BOTH triggers, so the ADR-0010 decision and the capability row stand unchanged.

Everything else is untouched: the in-place load goes through the SAME `load_uri` the seam's `navigate` drives, so an `ipfs://`/ENS `_blank` target is still hash-verified by `install_ipfs` and an unsupported scheme is still refused (the hook is a ROUTER, not a trust bypass), the shared `renderer::new_window_action` rule still makes the routing decision, and `life.begin(&url)` still makes the URL bar follow. iOS/Android are unchanged: they use their own native hooks (`WKUIDelegate.webView(_:createWebViewWith:for:windowFeatures:)`, `WebChromeClient.onCreateWindow`), not WebKitGTK's `create`.

## Measured before/after (the real `ronan.eth` fixture)

Driven through a REAL `WebViewRenderer` on a live GTK loop: navigate to `https://ronan.eth.limo/portfolio`, let the SPA hydrate, then click the first `a[target="_blank"]` (which is `https://conquest.game/`).

| | desktop `create` handler | result |
| --- | --- | --- |
| BEFORE (v0.2.5, `HEAD~`) | typed `connect_create`, returns the EXISTING view | `BEFORE-CLICK uri=Some("https://ronan.eth.limo/portfolio/")` then `Assertion 'this->_M_is_engaged()' failed.` -> **process aborted (signal 6, SIGABRT)** |
| AFTER (this fix) | raw `connect_local("create", …)`, returns NULL | `BEFORE-CLICK uri=Some("https://ronan.eth.limo/portfolio/")`, `AFTER-CLICK uri=Some("https://conquest.game/")` -> **loaded IN PLACE, no crash, no second window** |

## The automatable guard

`crates/webview-renderer/src/lib.rs` -> `real_webview_new_window_requests_load_in_place_without_aborting` (`#[ignore]`d, needs a display). It serves a fixture page from an in-memory custom scheme carrying BOTH triggers, drives each through a real WebKitGTK view on a running GTK loop, and asserts the SAME view ended up on each target. It is a real red/green signal for this crash: before the fix it aborts the test binary with the exact `_M_is_engaged()` assertion; after it passes.

Run it with:

```
cargo test -p webview-renderer -- --ignored --test-threads=1 real_webview_new_window_requests_load_in_place_without_aborting
```

(Filter to this one test: the `--ignored` set as a whole cannot run in a single process because each test constructs a `WebViewRenderer` and GTK refuses to initialize twice — pre-existing, see `work/notes/observations/ignored-gtk-tests-cannot-share-one-test-process-2026-07-26.md`.)

The display-free half stays where it was: the shared routing rule `renderer::new_window_action` is pinned by `a_new_window_request_navigates_the_current_view_in_place` / `a_new_window_request_with_no_target_opens_no_window_and_loads_nothing` (`crates/renderer`), and the handler-body shape by `a_new_window_request_navigates_the_existing_view_in_place_no_second_view` (`crates/webview-renderer`).

## Manual verification

1. `cargo run -p werust -- https://ronan.eth.limo/`
2. Click "portfolio", then any external link (all `target="_blank"`). EXPECT: the CURRENT window navigates to the target. It must NOT abort, and no second window opens.
3. Trigger a `window.open(url)` (e.g. from the web inspector console, F12). EXPECT: the same in-place navigation, no abort.
4. Point a `_blank` link at an `ipfs://<cid>` (or a `.eth` that resolves to IPFS). EXPECT: it loads in place AND the trust indicator shows content-verified / name-via-trusted-RPC; an unsupported scheme still fails closed.
