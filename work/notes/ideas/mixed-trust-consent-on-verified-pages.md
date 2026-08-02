---
title: "Mixed content is really mixed TRUST on a verified page: block plaintext http outright, but ask consent when an `ipfs://` page reaches out to a server"
date: 2026-08-01
kind: idea
---

## The opportunity

Two different problems usually filed under one name, raised by the human 2026-08-01 as "consider early, maybe not now".

**Case 1, `http://` subresources on an `https://` page: just block them.** Settled browser behaviour, no werust-specific thinking required. The whole web is https now and the failure mode of blocking is well understood.

**Case 2, `http(s)://` subresources on an `ipfs://` page: this is NOT classic mixed content, and the difference is the interesting part.** Classic mixed content is about transport confidentiality: an https page pulling a plaintext resource. Here nothing is wrong with the transport. What is happening is that a **content-verified page is importing server-authoritative data**, so the page's verified-ness no longer covers what the user actually sees. Call it mixed TRUST rather than mixed content.

Blocking it outright would break real sites: an IPFS-hosted dapp with a hardcoded Ethereum endpoint stops working. The proposal is therefore **consent**, per origin, so such a page keeps working with the user's knowledge rather than silently or not at all.

## Why it is worth recording now rather than building now

Three things constrain the design, and all three are moving:

1. **werust itself already does this.** The injected EIP-1193 provider reaches a trusted RPC from inside a verified page, which is exactly what a page's own hardcoded node would do. A consent model that intercepts page-originated egress while silently exempting the built-in provider needs to justify the asymmetry rather than inherit it by accident.

2. **Helios changes the question from trust to privacy.** With a light client the provider preserves INTEGRITY, so werust's own egress and a page's egress stop differing on the trust axis and differ only on observability, which is precisely the axis `docs/adr/0012` adds as a second indicator. That simplifies the consent story considerably: it becomes "this page wants to make an observable request", not "this page wants to import unverifiable data". Designing the consent before that lands risks building for the harder, soon-to-be-obsolete framing.

3. **A consent mechanism already has a proposed home.** `work/specs/proposed/gated-protocol-subsystems-consent-and-lazy-activation.md` covers consent and lazy activation for protocol subsystems. Whether page-originated egress consent is part of that spec or a sibling should be decided BEFORE either is built, so werust does not end up with two permission models and two prompt vocabularies.

## Shape, if it is built

- The permission is per (page origin -> target origin), remembered, and revocable.
- The prompt states the consequence in the vocabulary the chrome already uses: (pre-Helios) part of what the page shows was not verified, and the request is observable. Note it cannot lean on a privacy INDICATOR to carry that, since the indicator is deferred (`docs/adr/0012` status note), so the prompt must say it in words.
- The natural home for both the prompt's detail and the revocation is the trust details panel (`trust-indicator-and-details-panel`), which is already positioned to be the single trust/permissions surface. Do not mint a second permissions UI.
- Case 1 (plaintext http anywhere) needs no consent: block, and say so.

## Not decided

Whether an ipfs page's egress to a *content-addressed* endpoint (another gateway, another CID) is the same class as egress to a server API. It probably is not, since the former stays verifiable, but that distinction has not been thought through and would change how coarse the prompt can afford to be.
