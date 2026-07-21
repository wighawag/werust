---
title: Browser shell — URL bar, back/forward/reload/stop, live interactive view
slug: browser-shell-url-bar-and-live-interactive-view
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [renderer-seam-trait-and-webview-backend-navigate]
covers: [1, 2]
---

> **FORWARD-POINTER (planted by drive-tasks after `renderer-seam-trait-and-webview-backend-navigate` landed).** On the WebKitGTK backend the seam's input-forwarding methods (`send_pointer` / `send_key` / `send_scroll`, and to a degree `set_focus`) are DELIBERATE NO-OPS: the live view is a real embedded GTK widget, so the OS/GTK routes scroll/click/focus/keyboard input to it NATIVELY. For the webview backend, "input reaches the page" (acceptance criterion) is satisfied simply by embedding the live `ViewHandle` widget and giving it focus — you do NOT need to (and should not try to) drive interactivity by calling `send_*`; those calls will do nothing on this backend and that is correct. The `send_*` seam methods exist for a FUTURE native renderer that has no OS-level input routing and must be fed synthetic events. So: wire the URL bar + back/forward/reload/stop + lifecycle-driven chrome through the seam, embed the live widget for interaction, and test input-forwarding at the SEAM boundary (that the shell calls the seam / that the widget is embedded and focusable) rather than asserting the webview's `send_*` no-ops move anything. Don't "fix" the no-ops.

## What to build

Build the browser product shell around the `Renderer` seam: a window with a URL
bar (type a URL, navigate), back / forward / reload / stop controls, and a LIVE,
interactive view where scroll, click, focus, and keyboard input are forwarded to
the page and load-lifecycle events (started / committed / finished / failed) drive
the chrome. This makes werust behave like a real browser, not a static viewer.

## Acceptance criteria

- [ ] A window with a URL bar navigates to a typed URL through the `Renderer` seam.
- [ ] Back, forward, reload, and stop work and reflect navigation state.
- [ ] Scroll, click, focus, and keyboard input reach the page (interactive, not static).
- [ ] Load-lifecycle events from the seam update the chrome (e.g. loading/idle state, failure surfaced).
- [ ] Tests cover the shell↔seam wiring (navigation state transitions, input forwarding) at the seam boundary.

## Blocked by

- Blocked by `renderer-seam-trait-and-webview-backend-navigate`.

## Prompt

> Goal: the usable browser shell — URL bar, nav controls, and a live interactive
> view — driven entirely through the `Renderer` seam (see `CONTEXT.md`).
>
> All page interaction goes through the seam's input/scroll/focus forwarding and
> load-lifecycle events; the shell must not reach past the seam into the webview.
> This is the day-one product surface the trust-hook and native-renderer tasks then
> build on. Test the shell↔seam wiring at the seam boundary, not the GTK internals.
>
> Done = a user can open a URL, browse (back/forward/reload/stop), and interact with
> the page (scroll/click/focus/type), with the chrome reflecting load state.
