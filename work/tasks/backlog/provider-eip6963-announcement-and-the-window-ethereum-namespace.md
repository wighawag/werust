---
title: "Should werust stop squatting `window.ethereum` and announce via EIP-6963 instead?"
slug: provider-eip6963-announcement-and-the-window-ethereum-namespace
needsAnswers: true
blockedBy: [provider-refuses-honestly-instead-of-resolving-an-empty-account-list]
covers: []
---

<!-- open-questions -->

## Open questions

1. **Does werust advertise a wallet at all before it has a signer?** Today it injects `window.ethereum`, which is the de-facto "a wallet is installed" signal, while holding no keys and being able to authorise no account. Option (a) — keep injecting, refuse honestly — was chosen on 2026-07-31 as the immediate fix. This task asks whether that is the END state or an interim one.
2. **If EIP-6963 is adopted, is `window.ethereum` retained for legacy dapps, or dropped?** Retaining both means a dapp can still mistake werust for a wallet; dropping it means older dapps see nothing at all, which is honest but is a capability REMOVAL from five shipped platforms.
3. **What does that do to the trust hook?** EIP-1193 provider injection is one of the two hooks in the `Renderer` qualification bar (`TrustHook::ProviderInjection`), asserted by the macOS and Windows trust-hook smokes against a real `window.ethereum` object. If injection becomes conditional or 6963-only, the bar has to be re-expressed — an ADR-0001 amendment, not a code change.

<!-- /open-questions -->

## What to build

Deferred option (b) from the 2026-07-31 provider decision (see `provider-refuses-honestly-instead-of-resolving-an-empty-account-list`, decision 3).

**The inconsistency this would resolve.** werust implements NO EIP-6963 today: there is no `announceProvider` and no `eip6963:requestProvider` listener anywhere in the tree. So a page asking the two standard questions gets two opposite answers — legacy detection (`window.ethereum` exists) says "a wallet is here", while 6963 enumeration says "no wallets exist". EIP-6963 was created precisely to end the `window.ethereum` namespace war and let a dapp present a CHOICE rather than assume a single injected wallet; a browser that ships a keyless read-only provider is exactly the case it was designed for.

**Why it is gated on a human.** It is not a bug fix, it is a product position about whether werust presents itself as a wallet-bearing browser. It also touches the qualification bar that five platforms are measured against, so it cannot be decided inside a build.

**Do not start this before the questions above are answered**, and expect it to land as an ADR-0001 amendment plus the code, not code alone.
