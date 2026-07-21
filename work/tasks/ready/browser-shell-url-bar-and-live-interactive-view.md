---
title: Browser shell — URL bar, back/forward/reload/stop, live interactive view
slug: browser-shell-url-bar-and-live-interactive-view
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [renderer-seam-trait-and-webview-backend-navigate]
covers: [1, 2]
---

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
