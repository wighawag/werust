---
title: Gate-3 (conductor) verdict — renderer-seam-trait-and-webview-backend-navigate — APPROVE
date: 2026-07-21
kind: observation
reviewOf: renderer-seam-trait-and-webview-backend-navigate
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main in --merge mode, commit e195bda)

Conductor's own diff-vs-criteria review. `do` ran Gate-1 (acceptance) + Gate-2
(code review), both green; this is the third pass.

### Acceptance criteria — all met

- ✅ `Renderer` trait declares the FULL seam surface: navigate/reload/stop,
  load_state/current_url/poll_event (load-lifecycle), view_handle (live view),
  send_pointer/send_key/send_scroll/set_focus (input/scroll/focus forwarding),
  register_script_message_handler + inject_script (script-message bridge), and
  register_scheme_handler (custom-scheme / request-interception). The two trust
  hooks (EIP-1193 bridge, ipfs custom-scheme) are declared from day one.
- ✅ WebKitGTK backend (`webview-renderer` crate, `webkit6` over GTK4) implements
  the trait: `WebView::builder()` + `load_uri()` for navigate, load-signal wiring
  for lifecycle, and a `navigate_and_show` example. Host has webkitgtk-6.0 + gtk4.
- ✅ No WebKitGTK leaks past the seam: the seam crate `renderer` has NO gtk/webkit
  dep (the grep "hits" there are doc comments only). The `werust` binary uses gtk4
  ONLY as the product shell (Application/window/main loop) and reconstructs the
  opaque `ViewHandle` as a generic `gtk4::Widget` via `from_glib_none` — no
  `webkit6` type crosses the seam; rendering is driven through `Box<dyn Renderer>`.
- ✅ Seam-contract tests at the trait level (7 tests in webview-renderer:
  navigate transitions lifecycle, stop settles, reload re-navigates, trust hooks
  present, etc.) + 2 in the binary. All green.

### Triage of the 4 non-blocking Gate-2 nits

1. `current_url` returns owned `Option<String>` (not `&str`) — KEEP. Recorded in
   Decisions; enables a signal-driven RefCell-backed backend. Sound, reversible.
2. New `webview-renderer` peer crate of `native-renderer` — KEEP. Coherent with
   the webview-now/native-later term; reuses the backend-crate shape.
3. **Script-message bridge is one-directional** (page->browser via handler;
   browser->page only at document-start via inject_script). The EIP-1193 round-trip
   needs a browser->page RESPONSE push (evaluate_javascript-style). ACTIONED as a
   FORWARD-NOTE planted in `eip1193-provider-injection-via-script-bridge` so its
   build agent knows it must EXTEND the seam with that method (see below).
4. **send_pointer/send_key/send_scroll are no-ops on WebKitGTK** (documented: the
   embedded GTK widget gets real OS input natively). ACTIONED as a FORWARD-NOTE
   planted in `browser-shell-url-bar-and-live-interactive-view` so its agent knows
   webview interactivity comes from embedding the live widget, and the `send_*`
   seam methods target a FUTURE native backend, not the webview.

### Forward-notes planted (conductor step 2 — committed before those tasks build)

- `eip1193-provider-injection-via-script-bridge`: must extend the seam with a
  browser->page response-push (evaluate_javascript-style) for the request(...)
  round-trip; the landed seam is page->browser one-way + document-start injection.
- `browser-shell-url-bar-and-live-interactive-view`: on the WebKitGTK backend the
  `send_pointer/key/scroll` seam methods are intentional no-ops (GTK routes real
  OS input to the embedded widget); do not wire them to achieve interactivity —
  they are the seam surface for a future native backend with no OS input routing.

### What this unlocks

renderer-seam landing unlocks: renderer-seam-trust-hook-qualification-gate,
native-renderer-t0-subset-path-behind-seam, and
browser-shell-url-bar-and-live-interactive-view.
