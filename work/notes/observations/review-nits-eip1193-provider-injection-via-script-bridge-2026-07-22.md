---
title: review-gate non-blocking nits for 'eip1193-provider-injection-via-script-bridge' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: eip1193-provider-injection-via-script-bridge
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'eip1193-provider-injection-via-script-bridge' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: new seam method Renderer::evaluate_javascript(&self, script) with a no-op default, added transport-neutral (not a provider-specific respond method). Cross-task: every Renderer impl inherits the default; the ipfs task and benchmark harness may reuse it. Looks correct and reversible.
  (crates/renderer/src/lib.rs:489; recorded in spike README Decisions block.)
- Ratify user-visible provider defaults: eth_chainId -> 0x1 (mainnet), eth_accounts/eth_requestAccounts -> [], every other method (incl. signing) refused with EIP-1193 4200. Truthfully keyless; key custody deferred per spec Out of Scope.
  (ProviderBridge::answer, STUB_CHAIN_ID in crates/werust-core/src/provider.rs; recorded in README Decisions.)
- window.ethereum is installed via Object.defineProperty with configurable:true and no EIP-6963 multi-provider announcement; it will hard-override any other injected provider. Acceptable for a day-one single-provider stub, but note as a future interaction if other providers coexist.
  (provider_shim() defineProperty(window,'ethereum',{value:provider,configurable:true}) in provider.rs:311.)
- Nit: pub fn inject_provider_shim(&mut dyn Renderer) is defined and documented but never called (the real backend uses install_provider -> inject_script directly). It is the documented dyn-seam half; consider whether it earns its place or should be the path install_provider reuses.
  (crates/werust-core/src/provider.rs:354; no callers found outside docs.)
