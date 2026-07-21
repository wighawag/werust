---
title: Inject the native EIP-1193 provider via the script-message bridge
slug: eip1193-provider-injection-via-script-bridge
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view]
covers: [5]
---

> **FORWARD-POINTER (planted by drive-tasks after `renderer-seam-trait-and-webview-backend-navigate` landed).** The `Renderer` seam as landed wires the script-message bridge in ONE direction: `register_script_message_handler` (page -> browser) and `inject_script` (browser -> page, but only at document-start). There is currently NO browser -> page RESPONSE push for a LIVE page. The `request(...)` round-trip this task requires (page calls -> native handler -> result/error delivered back to the page's pending promise) therefore needs you to EXTEND the seam with a browser->page evaluation/response method (e.g. an `evaluate_javascript(&self, script)` on the `Renderer` trait, implemented on the WebKitGTK backend via `webkit6`'s `WebView::evaluate_javascript`). Declare it on the trait, implement it on the webview backend, and route the native handler's result back through it. Do this as part of THIS task — the seam gap is a deliberate hand-off, not an oversight.

## What to build

Inject a native Ethereum EIP-1193 provider into pages via the `Renderer` seam's
script-message bridge, so a page's JS sees a native provider object (the standard
`request({ method, params })` interface) that round-trips messages across the bridge
to a native handler. This is the plumbing for the first-class provider — a page can
detect and call the provider. Key CUSTODY is out of scope here (the wallet broker
security model is deferred to the exploration spec); this task wires the provider
surface and message transport, not signing keys.

## Acceptance criteria

- [ ] Pages see an injected EIP-1193 provider exposing the standard `request(...)` interface (and event emitter surface) via the script-message bridge.
- [ ] A page-side `request(...)` call round-trips across the bridge to a native handler and back with a result/error.
- [ ] A benign, read-only method (e.g. a chain-id / accounts stub) demonstrates the full round-trip end-to-end without holding any private keys.
- [ ] Tests cover the injection + round-trip at the bridge seam.

## Blocked by

- Blocked by `browser-shell-url-bar-and-live-interactive-view`.

## Prompt

> Goal: inject a native EIP-1193 provider through the seam's script-message bridge —
> a first-class capability, not an extension (see `CONTEXT.md`, `docs/adr/0001`).
>
> Build the PROVIDER SURFACE and message transport: page JS gets a provider with
> `request({method, params})` that round-trips to a native handler. Do NOT implement
> key custody / signing here — the wallet broker security model (own-process signing
> broker, page never holds keys) is a deferred open question on the exploration spec
> `rust-successor-native-renderer-architecture-benchmark`. Demonstrate the round-trip
> with a read-only method stub. This is one of the two trust hooks that qualify the
> backend (`renderer-seam-trust-hook-qualification-gate`).
>
> Done = a page can detect and call an injected EIP-1193 provider whose calls
> round-trip across the bridge to native code, with no keys involved yet.
