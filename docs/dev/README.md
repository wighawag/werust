# werust provider mimic

A page-side JS script that installs a `window.ethereum` behaving **exactly like werust's injected EIP-1193 provider**, so a dapp (mandalas.eth.limo, jolly-roger, etc.) can be tested in a **normal browser** (Chrome/Firefox/Safari) without running werust.

## What it replicates

Faithful port of `crates/werust-core/src/provider.rs` (`provider_shim` + `ProviderBridge::answer`) and `crates/werust-core/src/ethereum.rs`:

- `isWerust: true`, no `isMetaMask` / `isExodus` / `isOpera` / `isGameStop` flags
- `eth_chainId` → `"0x1"` (mainnet — the chain werust's ENS backend reads)
- `eth_accounts` → `[]` (passive read: no keys, conformant empty)
- `eth_requestAccounts` → **reject 4100 Unauthorized** (not `[]`, never resolves empty)
- everything else → **reject 4200 Unsupported Method**
- EIP-1193 event surface (`on`/`removeListener`/`emit`) but **never emits**
- **No EIP-6963 announcement** (no `announceProvider` / `requestProvider`)

## How to use

Copy `werust-provider-mimic.js` into the dapp's static directory and add a `<script>` tag in the HTML head, before the app's module scripts:

```html
<script src="./werust-provider-mimic.js"></script>
```

It's **off by default** — activate with:
- `?werust=1` URL param, or
- `localStorage["werust-mimic"] = "1"` for every load

Without the flag it does nothing (zero impact on normal use).

## Companion

`provider-inspector.html` — load it in werust (to inspect werust's real provider) or in a browser with the mimic active. Shows every property on `window.ethereum`, lets you call each EIP-1193 method, tests EIP-6963 enumeration, and logs all interactions.

## How this was used

This mimic was used to reproduce and diagnose the "modal flash" issue on mandalas.eth.limo: werust's 4100 rejection on `eth_requestAccounts` is instant, so the dapp's "Waiting for Wallet Connection..." modal appears and vanishes in one frame. The fix lives in `@etherplay/connect` (distinguish EIP-1193 error codes in the connect failure message) and in the dapps' `ConnectionFlow.svelte` (render the error in a "Connection Failed" modal instead of silently returning to idle).

## Keep in sync

The source of truth is `crates/werust-core/src/provider.rs` and `crates/werust-core/src/ethereum.rs`. If werust's provider behaviour changes, update this mimic to match.