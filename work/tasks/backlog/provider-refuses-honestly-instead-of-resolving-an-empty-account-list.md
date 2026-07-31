---
title: "`eth_requestAccounts` must REJECT rather than resolve `[]`, so a dapp shows 'no wallet' instead of silently connecting to nobody"
slug: provider-refuses-honestly-instead-of-resolving-an-empty-account-list
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

Found by the human testing `mandalas.eth` against the shipped v0.3.0 build: werust's injected `window.ethereum` "seems to be causing confusion" for the dapp. Root-caused by the conductor on 2026-07-31; the human then chose the policy (option (a) below).

## The defect

`crates/werust-core/src/provider.rs::answer()` currently says:

```rust
"eth_chainId"                          => Ok(json!(STUB_CHAIN_ID)),   // "0x1", hard-coded
"eth_accounts" | "eth_requestAccounts" => Ok(json!([])),
other                                  => Err(ProviderError::unsupported(other)),
```

`eth_accounts -> []` is CORRECT and stays: an empty array is the conformant way to say "no accounts are authorised".

**`eth_requestAccounts -> []` is the bug, and it is worse than a failure.** Every dapp on earth writes:

```js
const [account] = await ethereum.request({ method: 'eth_requestAccounts' })
```

Because the Promise RESOLVES, the dapp takes its SUCCESS path with `account === undefined`, believes it is connected, reads `chainId: 0x1`, believes it is on mainnet, and proceeds. Every dapp has an error path for "connect failed"; none has one for "connected to nobody". Resolving an empty array from `eth_requestAccounts` is also not an outcome the method is specified to have — it either yields accounts or fails.

## 1. Reject `eth_requestAccounts`, legibly

Return an EIP-1193 provider error instead of `Ok(json!([]))`.

**Use code 4100 (Unauthorized), and record why over 4001.** 4100 means "the requested method and/or account has not been authorized by the user", which is exactly true here. 4001 (User Rejected Request) would produce a smoother dapp UX, because many dapps special-case 4001 and silence it — but the user rejected nothing, so 4001 is a small lie, and this project's whole posture is not telling small lies to the page. Record the trade-off explicitly: with 4100 some dapps will surface a generic error rather than a quiet "cancelled".

**The message is user-visible** (dapps commonly render `error.message`), so make it a legible sentence rather than a code name: it should say that werust has no wallet yet, that it injects a read-only provider and holds no keys, so no account can be authorised. Short, no jargon, no stack.

## 2. Stop hard-coding the chain id as a `STUB_`

`STUB_CHAIN_ID: &str = "0x1"` unconditionally tells every page it is on Ethereum mainnet. Source it from the ONE place werust already knows its chain (the configured RPC/ENS backend in `crates/werust-core/src/ethereum.rs` and the `WERUST_RPC_URL` configuration) rather than minting a second constant that cannot drift into truth. This is the same one-source rule the repo applied to the version (`android-apk-version-from-the-release-tag`, `macos-release-packaging-leg`) and to the wire vocabulary.

If it turns out werust genuinely has no chain concept outside the ENS RPC, then keeping a single documented constant is acceptable — but rename it away from `STUB_` and say in its doc WHY that value is correct rather than provisional.

**Do not break the trust-hook smokes.** `crates/windows-renderer/examples/trust_hooks_smoke.rs` and its macOS twin assert the round-tripped chain id against the `werust_core::provider::STUB_CHAIN_ID` SYMBOL (not a literal), so they follow a rename automatically — but keep a public symbol for them to compare against, and keep both smokes green. They are the qualification evidence on real runners.

## 3. Record the policy the human chose, so it is not re-litigated

The human was offered three options and chose **(a)**: keep injecting `window.ethereum` and make every path honest. Record it as a decision with its alternatives and its revisit trigger:

- **(a) CHOSEN, for now.** Keep the injection. It is one of werust's two TRUST HOOKS (`TrustHook::ProviderInjection`, the `Renderer` qualification bar in `crates/renderer/src/lib.rs`), asserted by the macOS and Windows trust-hook smokes and marked `implemented` in all five cells of the `eip1193-provider` parity row. A provider that refuses honestly is strictly better for a dapp than no provider at all.
- **(b) DEFERRED, tracked separately.** Stop squatting `window.ethereum` and announce via EIP-6963 only once a real signer exists. werust implements NO EIP-6963 today (no `announceProvider`, no `eip6963:requestProvider` listener), so legacy detection currently says "a wallet is here" while 6963 enumeration says "no wallets" — the two contradict each other. Moving to (b) changes what the trust hook MEANS, so it needs an ADR-0001 amendment, not a code change. Tracked as `provider-eip6963-announcement-and-the-window-ethereum-namespace`.

Do NOT implement any part of (b) here.

## 4. Check one more conformance point and RECORD it (do not necessarily fix it)

The shim exposes the EIP-1193 event surface (`on`, `removeListener`, `emit` over `connect`, `disconnect`, `chainChanged`, `accountsChanged`, `message`), but nothing on the NATIVE side ever calls `emit` — a grep for it finds only the shim's own definition. EIP-1193 says a provider SHOULD emit `connect` once it can serve requests, and some dapps wait on it. Determine whether that matters in practice for a provider in this state, and write the answer down. Fix it only if it is both clearly correct and small; otherwise record it as a finding.

## Acceptance criteria

- [ ] `eth_requestAccounts` REJECTS with EIP-1193 code 4100 and a legible, user-facing message; it never resolves an empty array.
- [ ] `eth_accounts` still resolves `[]` (unchanged, and covered by a test asserting the two now DIFFER).
- [ ] The 4100-over-4001 trade-off is recorded, including that some dapps will show a generic error rather than a quiet cancel.
- [ ] The chain id comes from one source, or its constant is renamed off `STUB_` and documented as correct rather than provisional.
- [ ] Both trust-hook smokes (macOS and Windows) still pass, and the `Renderer` qualification bar is unchanged.
- [ ] The chosen policy (a) is recorded with its alternatives and the (b) revisit trigger.
- [ ] Whether the missing native `connect` emission matters is answered in writing.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: werust's injected `window.ethereum` breaks dapps by RESOLVING `eth_requestAccounts` with `[]`. Dapps write `const [account] = await ethereum.request({method:'eth_requestAccounts'})`, so they take the SUCCESS path with `account === undefined` and believe they are connected to nobody — worse than a clean failure, since every dapp handles "connect failed" and none handles that. In `crates/werust-core/src/provider.rs::answer()`, make `eth_requestAccounts` REJECT with EIP-1193 **4100 Unauthorized** and a legible user-facing message (dapps render `error.message`): werust has no wallet yet, injects a read-only provider, holds no keys, so no account can be authorised. Record why 4100 and not 4001 — 4001 gives smoother dapp UX because many dapps silence it, but the user rejected nothing and this project does not tell the page small lies. Keep `eth_accounts -> []` (that is correct) and add a test asserting the two now differ. Separately, `STUB_CHAIN_ID = "0x1"` unconditionally claims mainnet: source the chain id from the one place werust already knows its chain (the configured RPC/ENS backend, `WERUST_RPC_URL`) rather than a second constant, or if there genuinely is no other chain concept, rename it off `STUB_` and document why the value is correct rather than provisional. Both trust-hook smokes compare against the `werust_core::provider::STUB_CHAIN_ID` SYMBOL, so keep a public symbol and keep both green — they are the qualification evidence on real runners, and the `Renderer` trust-hook bar must not change. Record that the human chose policy (a) (keep injecting and be honest) over (b) (stop squatting `window.ethereum`, announce via EIP-6963 once a real signer exists — deferred, needs an ADR-0001 amendment, tracked separately); implement no part of (b). Finally, the shim has an EIP-1193 event surface but NOTHING native ever calls `emit`, so `connect` is never fired: determine whether that matters for a provider in this state and write the answer down, fixing it only if clearly correct and small.
