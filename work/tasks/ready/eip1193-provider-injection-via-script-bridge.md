---
title: Inject the native EIP-1193 provider via the script-message bridge
slug: eip1193-provider-injection-via-script-bridge
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view]
covers: [5]
---

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
