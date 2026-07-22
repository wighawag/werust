---
title: Gate-3 (conductor) verdict — eip1193-provider-injection-via-script-bridge — APPROVE
date: 2026-07-22
kind: observation
reviewOf: eip1193-provider-injection-via-script-bridge
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 73dcacc)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Pages see an injected EIP-1193 provider (`window.ethereum`) exposing the
  standard `request({method,params})` + event-emitter surface, via the seam's
  script-message bridge (`provider_shim()` injected with `inject_script`).
- ✅ A page-side `request(...)` round-trips: page -> `register_script_message_handler`
  -> native `ProviderBridge` -> response push settles the page's pending Promise.
- ✅ Read-only stub demonstrates the full round-trip with NO keys: `eth_chainId` ->
  `0x1`, `eth_accounts`/`eth_requestAccounts` -> `[]`, all else (incl. signing)
  refused with EIP-1193 `4200`. Key custody deferred per spec.
- ✅ Tests cover injection + round-trip at the bridge seam WITHOUT a webview/GTK
  (clean split: shim JS, native handler, response push all correlate by id).

### FORWARD-NOTE HONOURED (conductor value confirmed)

My forward-note (planted after the renderer seam landed one-directional) was followed
exactly: the seam was EXTENDED with `Renderer::evaluate_javascript(&self, script)`
("browser -> page", no-op default), implemented on the WebKitGTK backend via
`WebView::evaluate_javascript` on the GTK loop, and used as the response-push that
completes the `request(...)` round-trip. The seam gap I flagged is now closed.

### Nit triage

1. New seam method `evaluate_javascript` (no-op default, transport-neutral) — RATIFY.
   All backends inherit the default safely; reversible. The ipfs task + benchmark
   harness MAY reuse it (not load-bearing for either; no extra forward-note needed).
2. Provider defaults (chainId `0x1`, accounts `[]`, signing refused 4200, keyless) —
   RATIFY. Truthfully keyless; custody deferred per spec Out of Scope.
3. `window.ethereum` installed via `defineProperty` (configurable:true), NO EIP-6963
   multi-provider announcement — hard-overrides any other provider. KEEP; acceptable
   for a day-one single-provider stub. EIP-6963 multi-provider is a future
   enhancement out of scope here (noted for future coexistence).
4. `inject_provider_shim(&mut dyn Renderer)` defined + documented but NEVER called
   (the backend uses `install_provider -> inject_script` directly) — KEEP; captured
   below. Harmless documented pub helper; the real path is tested.

### Follow-up captured (not tasked here)

`werust-core/src/provider.rs` `inject_provider_shim(&mut dyn Renderer)` is a
documented dyn-seam helper with no callers. Either have `install_provider` reuse it
(so the documented path is the real path) or trim it. Low priority; harmless.

### What this unlocks

This is one of the two trust hooks: the webview backend can now genuinely satisfy
PROVIDER injection. Landing it (after browser-shell + fetcher-hash-verified) UNLOCKS
`ipfs-scheme-resolution-through-renderer-seam` (the deliberate serialization edge:
eip1193 and ipfs both wire the same webview backend hook surface, so ipfs was ordered
after eip1193 to avoid a parallel merge conflict).
