---
title: Encode the Renderer-seam trust-hook qualification gate
slug: renderer-seam-trust-hook-qualification-gate
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [renderer-seam-trait-and-webview-backend-navigate]
covers: [4]
---

## What to build

Encode, as a seam contract plus a conformance test, the rule that a `Renderer`
backend QUALIFIES only if it can satisfy the trust hooks — EIP-1193 provider
injection AND `ipfs://` custom-scheme resolution — not merely if it renders well.
This turns the thesis into an enforced property of the seam: any backend (the
webview now, a native renderer later) must pass the same trust-hook qualification
to be usable.

## Acceptance criteria

- [ ] The seam expresses trust-hook capability as a required, checkable contract (e.g. the backend must provide provider-injection and custom-scheme entry points), not an optional add-on.
- [ ] A conformance test asserts a backend is only accepted/qualified when both trust hooks are satisfiable, and rejects/flags one that renders but cannot.
- [ ] The WebKitGTK backend passes the qualification (its hook entry points exist, wired by the sibling provider/ipfs tasks).
- [ ] Tests mirror the repo's style and run under the `verify` gate.

## Blocked by

- Blocked by `renderer-seam-trait-and-webview-backend-navigate`.

## Prompt

> Goal: make "a backend qualifies only if it satisfies the trust hooks" an ENFORCED
> seam property, not a comment (see `CONTEXT.md`, `docs/adr/0001` — the seam encodes
> the thesis).
>
> Define the qualification as a contract the seam checks (provider-injection +
> `ipfs://`-scheme entry points must exist), and write a conformance test that a
> render-only backend fails it. The actual injection and scheme resolution are wired
> by `eip1193-provider-injection-via-script-bridge` and
> `ipfs-scheme-resolution-through-renderer-seam`; here you enforce that a qualifying
> backend MUST expose them. This same gate will later qualify the native renderer.
>
> Done = the seam mechanically requires trust-hook capability, with a test proving a
> non-qualifying backend is rejected.
