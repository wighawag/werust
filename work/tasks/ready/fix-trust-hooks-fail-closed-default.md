---
title: Flip the trust-hook qualification default to fail-closed
slug: fix-trust-hooks-fail-closed-default
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: [4]
---

## What to build

Change the `Renderer::trust_hooks()` default (in `crates/renderer/src/lib.rs`) from the
current FAIL-OPEN `TrustHooks::all()` to FAIL-CLOSED `TrustHooks::none()`, so a backend
qualifies via `qualify()` ONLY if it EXPLICITLY declares the trust hooks it actually
wires. Today a new backend that stubs the hook methods but does not override
`trust_hooks()` silently qualifies (fail-open) — the opposite of what the qualification
gate exists to enforce ("a backend qualifies only if it can satisfy the trust hooks").
Fail-closed makes an un-declared backend NOT qualify by default; trust must be opted
into, never inherited by omission.

## Required companion change (do NOT skip — the gate will catch it)

The real WebKitGTK backend `WebViewRenderer` (`crates/webview-renderer/src/backend.rs`,
`impl Renderer for WebViewRenderer`) currently RELIES on the fail-open default to
qualify — it does NOT override `trust_hooks()`. When you flip the default to `none()`,
you MUST add an explicit `fn trust_hooks(&self) -> TrustHooks { TrustHooks::all() }` to
that impl, because the webview backend genuinely wires BOTH hooks (EIP-1193 provider
injection via the script-message bridge + `evaluate_javascript` response push, and the
`ipfs://` custom-scheme handler). Without it, the existing tests
`webview_backend_passes_the_trust_hook_qualification_gate` and
`webview_renderer_does_not_downgrade_its_trust_hook_capability` (in
`crates/webview-renderer/src/lib.rs`) will fail — that failure IS the safety net proving
the flip did not silently disqualify the real backend.

The native T0 backend (`native-renderer`) ALREADY declares `TrustHooks::none()`
explicitly, so it is unaffected (it stays honestly not-qualifying). The benchmark
harness's test doubles set their own hooks explicitly too.

## Acceptance criteria

- [ ] `Renderer::trust_hooks()` defaults to `TrustHooks::none()` (fail-closed); the doc-comment explains a backend must OPT IN to each hook it wires, and is rejected by `qualify()` if it declares none.
- [ ] `WebViewRenderer` explicitly overrides `trust_hooks()` to `TrustHooks::all()` and still passes the qualification gate (its two existing qualification tests stay green).
- [ ] A test proves the fail-closed default: a backend that does NOT override `trust_hooks()` (relying on the default) is DISQUALIFIED by `qualify()` (the inverse of the old fail-open behaviour).
- [ ] The native T0 backend remains honestly not-qualifying; the trust-hook qualification gate's existing conformance tests (accepts-both, rejects-render-only, rejects-missing-one) stay green.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` all pass.

## Prompt

> Goal: make the trust-hook qualification gate FAIL-CLOSED. Flip
> `Renderer::trust_hooks()`'s default from `TrustHooks::all()` to `TrustHooks::none()`
> in `crates/renderer/src/lib.rs`, so trust is opted into, never inherited by omission
> (the human ratified this after the Gate-3 flag on the trust-hook-gate task).
>
> CRITICAL companion change: the real `WebViewRenderer` backend
> (`crates/webview-renderer/src/backend.rs`) currently relies on the fail-open default,
> so you MUST add an explicit `trust_hooks() -> TrustHooks::all()` to its `impl Renderer`
> (it genuinely wires both hooks). The two existing webview qualification tests are your
> safety net — keep them green. The native T0 backend already declares `none()`
> explicitly; leave it. Add a test that a default-relying backend is now disqualified.
>
> Done = the default is fail-closed, the webview backend explicitly qualifies, an
> un-declared backend is rejected, and the gate + all trust-hook tests are green.
