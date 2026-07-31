---
title: "Nothing native ever emits the shim's EIP-1193 `connect` event, and for a keyless read-only provider that is inert: the event cannot reach a listener at document start, and no mainstream dapp library waits for it"
date: 2026-07-31
status: verified
task: provider-refuses-honestly-instead-of-resolving-an-empty-account-list
kind: finding
source:
  - https://eips.ethereum.org/EIPS/eip-1193 (Events -> connect; Connectivity; Provider Errors)
  - crates/werust-core/src/provider.rs (`provider_shim`, injected via `Renderer::inject_script` at document start)
---

## The question

`provider_shim()` exposes the full EIP-1193 event surface (`on` / `removeListener` / `emit` over `connect`, `disconnect`, `chainChanged`, `accountsChanged`, `message`), but a grep for `emit` finds ONLY the shim's own definition: no native code path ever calls it, so `connect` is never fired. Does that matter for a provider in werust's current state (keyless, read-only, one fixed chain)?

## Ground truth

- **EIP-1193 makes it a MUST, not a SHOULD.** "If the Provider becomes connected, the Provider **MUST** emit the event named `connect`", including "the Provider first connects to a chain after initialization", carrying `{ chainId }`. So on the letter of the spec werust is non-conformant on this point. (The task text said SHOULD; the spec says MUST. The conclusion below is unchanged either way.)
- **"Connected" is defined as "can service RPC requests to at least one chain".** werust's provider can do that from the moment the shim is installed: `eth_chainId` round-trips page -> native -> page, which is exactly what both trust-hook smokes measure on real macOS and Windows runners.

## Why it is nonetheless inert today

1. **The emission has no possible audience.** The shim is injected at DOCUMENT START (`Renderer::inject_script`), i.e. before any page script has run, so there is no listener registered yet. Emitting `connect` at install would iterate an empty listener map. To reach a real listener the shim would have to emit at some LATER moment chosen by guess (a timer, a microtask), which is a race, not conformance.
2. **The mainstream dapp libraries never await it.** viem/wagmi, ethers' `BrowserProvider` and web3.js all call `request(...)` directly against an injected provider; `connect` is used, when at all, as a re-connection signal after a `disconnect`. werust answers requests from document start, so nothing is gated on the event.
3. **The other three events describe changes this provider cannot have.** `chainChanged` and `accountsChanged` need a chain selector and an account set (both arrive with the deferred wallet model); `disconnect` needs a connection that can drop. A keyless read-only provider on one fixed chain has nothing true to announce.
4. **It is the same class of defect this task exists to remove, pointed the other way.** `connect` means "the provider can service requests", but dapp authors routinely read it as "a wallet connected". Firing it to satisfy the letter of the spec, at a moment picked to maximise the chance a listener sees it, would risk telling the page the very thing that is NOT true, which is exactly the small lie the `eth_requestAccounts` refusal removes.

## Verdict: record, do not fix

Not fixed here, because a fix is neither clearly correct (which moment? with which chain id? announcing what to whom?) nor small (it needs a native "the provider is live on this page" signal per backend, on five edges). The event surface stays present and conformant to subscribe to, and stays silent until there is something true to say.

**Revisit trigger:** the first of (a) a real wallet/signer landing (then `connect`, `accountsChanged` and `chainChanged` all get real triggers and a native emit path is warranted), or (b) an observed dapp that genuinely blocks on `connect` in werust. Recorded at the choice site in `provider_shim`'s doc comment, and in `docs/spikes/provider-refuses-honestly-instead-of-resolving-an-empty-account-list/README.md`.
