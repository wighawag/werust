---
title: "Android: 'Uncaught TypeError: Cannot redefine property: werustProvider' in the console on https pages (unverified double-injection)"
date: 2026-07-28
kind: observation
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
---

Spotted in the in-app debug view's Console tab during the `mobile-ronan-eth-buttons-no-navigation` device run (API-36 emulator, debug APK): after the app's launch load of `https://example.com/`, the Console tab shows `[error] Uncaught TypeError: Cannot redefine property: werustProvider (https://example.com/:88)`.

That reads as the EIP-1193 provider preamble + shim being evaluated TWICE in the same document (the shim defines `window.werustProvider` non-configurably, so a second evaluation throws). Android injects the provider script from `CoreWebViewClient.onPageStarted` via `evaluateJavascript` (the earliest edge hook, since WebView has no document-start user-script API); if `onPageStarted` fires more than once for one logical load (e.g. a redirect or a back/forward restoration), the shim re-runs and throws. Unverified beyond the single observation: which double-fire path produces it, and whether it can disturb a page that reads the error. Desktop and iOS inject via real document-start user scripts, which run once per document by construction. A guard in the preamble (skip if already defined) would make the injection idempotent.
