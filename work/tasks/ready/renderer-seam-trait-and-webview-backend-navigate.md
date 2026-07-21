---
title: Define the Renderer seam and a WebKitGTK backend that navigates
slug: renderer-seam-trait-and-webview-backend-navigate
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [bootstrap-cargo-workspace-and-verify-gate]
covers: [1, 3]
---

## What to build

Define the wide, hot-swappable `Renderer` seam as a Rust trait (navigate/reload/
stop, a live interactive view handle, input/scroll/focus forwarding, load-lifecycle
events, a script-message bridge, and a request-interception / custom-scheme hook —
declare the surface even where a given method is not yet exercised), and implement a
FIRST backend over a system webview (WebKitGTK) that can navigate to a URL and show
the page on Linux. This is the day-one usable path: a real page rendered by the
system webview behind the seam.

## Acceptance criteria

- [ ] A `Renderer` trait exists declaring the full seam surface (navigate/reload/stop, live view, input/scroll/focus forwarding, load-lifecycle events, script-message bridge, request-interception/custom-scheme hook).
- [ ] A WebKitGTK backend implements the trait far enough to navigate to an `https://` URL and display the rendered page in a window on Linux.
- [ ] The rest of the browser talks to the webview ONLY through the trait (no direct WebKitGTK calls leak past the seam).
- [ ] Tests cover the seam contract (e.g. navigate transitions load-lifecycle state) at the trait level, mirroring the repo's test style.

## Blocked by

- Blocked by `bootstrap-cargo-workspace-and-verify-gate`.

## Prompt

> Goal: stand up the `Renderer` seam and a WebKitGTK backend that renders a real
> page — the "webview now, native later" hedge (see `CONTEXT.md`, `docs/adr/0001`).
>
> The `Renderer` seam is the load-bearing abstraction: a backend qualifies as real
> ONLY if it can satisfy the trust hooks (EIP-1193 provider injection via a
> script-message bridge, and an `ipfs://` custom-scheme / request-interception hook)
> — so DECLARE those methods in the trait now even though other tasks wire them up
> (`eip1193-provider-injection-via-script-bridge`, `ipfs-scheme-resolution-through-renderer-seam`).
> Here, implement navigate + show-the-page. Bind WebKitGTK; do not hand-roll a
> renderer. Test at the seam (the trait), not the GTK internals.
>
> Done = on Linux, werust opens a URL through the `Renderer` trait and shows the
> page via the WebKitGTK backend, with the seam surface fully declared.
