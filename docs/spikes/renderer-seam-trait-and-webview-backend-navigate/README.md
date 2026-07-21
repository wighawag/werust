# Spike: Renderer seam + WebKitGTK backend that navigates and shows a page

Durable evidence for task `renderer-seam-trait-and-webview-backend-navigate`.

## What was built

- The `Renderer` seam (trait) in `crates/renderer` — the wide, hot-swappable rendering-backend interface: navigate/reload/stop, live view handle, input/scroll/focus forwarding, load-lifecycle state + events, the script-message bridge, and the request-interception / custom-scheme hook (the trust hooks). Prior-work draft; extended here.
- The first backend, `crates/webview-renderer`, binding WebKitGTK (`webkit6` over GTK4) behind that trait. It splits into a GTK-free `LoadLifecycle` state machine (seam-testable headlessly) and `WebViewRenderer`, which wires a real `webkit6::WebView`'s load signals into that lifecycle and shows the page.
- The `werust` binary opens a GTK window, embeds the live view via the seam's opaque `ViewHandle` (reconstructed as a generic `gtk4::Widget` — no WebKitGTK type crosses the seam), and navigates through `dyn Renderer`.

## Reproducing the "navigate to https and show the page" evidence

On a Linux desktop session (a display is required to actually SHOW the window; the seam-contract unit tests need no display):

```sh
cargo run -p webview-renderer --example navigate_and_show -- https://example.com/
```

The example drives the backend ONLY through the `dyn Renderer` seam: it navigates, embeds the live view, and drains `LoadEvent`s off the seam on the GTK loop, printing the lifecycle transitions and quitting once the load settles.

Observed output (WebKitGTK 2.52 / GTK4 4.18 on Linux), with the window showing the rendered example.com page:

```
SEAM started: https://example.com/
SEAM committed: https://example.com/
SEAM finished: https://example.com/
SEAM load reached Finished — page shown via the seam.
```

`Started -> Committed -> Finished` are driven by WebKitGTK's real `load-changed` signals feeding the seam's `LoadLifecycle`; reaching `Finished` with the window showing the page is the acceptance evidence that a real page is rendered by the system webview behind the seam.

## Decisions

- **`Renderer::current_url` returns `Option<String>` (owned), not `Option<&str>`.** The prior-work trait draft borrowed the URL out. A real event-driven backend (the WebKitGTK webview) must keep its load state behind interior mutability (`RefCell`) so its load-lifecycle signals, which fire on the GTK main loop, can update it; it therefore cannot lend a borrow out of the `RefCell`. Returning the URL owned keeps the seam implementable by such a backend. Alternative considered: keeping `&str` and forcing every backend to store the URL inline outside interior mutability — rejected because it makes the seam un-implementable by exactly the class of backend the seam exists for (signal-driven system webviews). This touches the `renderer` crate's trait and its `FakeBackend` test, and the `webview-renderer` backend/tests. Also documented at the choice site (the trait method's doc comment in `crates/renderer/src/lib.rs`).
- **New concept `webview-renderer` (backend crate).** Named from the `CONTEXT.md` glossary term *webview-now / native-later* and *system-webview backend*; it is a peer of the existing `native-renderer` backend crate (both depend on `renderer`), so it reuses the established backend-crate shape rather than forking a new concept.

## Notes

- Pinned to `gtk4 = "=0.10.0"` / `webkit6 = "=0.5.0"` because the newer `0.11` / `0.6` gtk4-rs line requires rustc >= 1.92 and this repo's toolchain is 1.91.x. The `0.5`/`0.10` line binds the same WebKitGTK 6.0 / GTK4 and covers the host's WebKit 2.52 (via the `v2_50` feature; 2.52 is backward compatible).
- Screenshot capture was not included as a checked-in artifact: this host's window is a native Wayland surface and no Wayland screenshot tool (`grim`) is installed, while the X11 tools present (`import`) cannot see Wayland-native windows. The `navigate_and_show` example's `Finished` line is the reproducible, tool-independent proof instead.
