---
title: werust vs wezig — internal capability parity analysis
date: 2026-07-22
kind: finding
tags: [parity, wezig, capabilities, conformance-tiers, web3, native-renderer]
---

## Question

The werust APK behaves like wezig's externally. Do they match INTERNALLY on capabilities?

## Short answer

**On the shipped product surface (T0+T1 webview browser, ipfs verified loads, EIP-1193
provider, mobile shells): YES, at parity.** werust delivers, on a pure-Rust stack, the
same reached capability wezig's build arm reached.

**On the EXPLORATION surface wezig accumulated beyond that (deeper web3, native-paint
spikes, secure-origin/service-worker patches): NO — werust intentionally has less,
because those were wezig's DEFERRED/spike findings, and in werust they are gated behind
the exploration spec this drive did not build (by scope).**

So: parity on what was BUILT to ship; werust is behind on what wezig EXPLORED but also
deferred.

## Side by side

| Capability | wezig | werust | Parity? |
|---|---|---|---|
| WebKitGTK webview backend, navigate + show | ✓ (Zig) | ✓ (Rust, webkit6) | YES |
| Renderer seam + trust-hook qualification | ✓ | ✓ (fail-closed) | YES (werust stricter) |
| T0 native subset (tokenize/tree/cascade/flow/software-text) | ✓ | ✓ | YES |
| T1 real parser | subset+? | ✓ html5ever | YES (werust uses the real WHATWG parser) |
| T1 core CSS cascade | ✓ (hand) | ✓ (cssparser/stylo-stack) | YES |
| T1 text shaping | ✓ HarfBuzz + stb_truetype | ✓ parley (harfrust/skrifa/fontique) | YES (equivalent, both real shaping) |
| T0/T1 server + content-addressed floors + WPT meter | ✓ | ✓ | YES |
| ipfs:// hash-verified fetch + render at parity | ✓ | ✓ | YES |
| EIP-1193 provider injection, read-only round-trip, no keys | ✓ | ✓ | YES |
| Android app (real module, cross-compiled core) | ✓ | ✓ | YES |
| iOS app (real Xcode project, Simulator) | ✓ | ✓ | YES |
| Release: desktop + APK + iOS .app artifacts | ✓ | ✓ (v0.1.0 shipped) | YES |
| **Native paint to a REAL window** (SDL3/stb, GPU frame spikes) | spiked (`paint-sdl3-stb-window`, `spike-page-gpu-context`) | in-memory `Surface` + transcript only; `view_handle()` returns null (no windowing blit yet) | **wezig ahead** (spike) |
| **EIP-6963** multi-provider discovery/announce | spiked (`spike-wallet-broker-eip6963-provider`) | only `window.ethereum` (EIP-1193); no EIP-6963 announce | **wezig ahead** (spike) |
| **Wallet broker** (out-of-process signing, custody model) | spiked (`wallet_broker.zig`, threat-analysed custody) | deferred to exploration spec; no broker | **wezig ahead** (spike) |
| **ipfs:// secure-origin / service-worker hosting** | spiked (WebKitGTK SW-scheme patch, ADR-0015/0016) | not built; ipfs is a custom-scheme render, not a secure-origin SW host | **wezig ahead** (spike) |
| **Native-renderer architecture DECISION** | findings + build plan | benchmark HARNESS built; decision gated (needsAnswers spec) | both deferred; werust has the evidence generator |

## The important nuance

wezig's 45 "done" items include a lot of **exploration spikes + findings + ADRs**
(`spike-*`, `*-findings-and-build-plan`, `evaluate-custody-*`). Those are wezig's
research arm answering "what should the successor build?" — they are NOT all shipped
product capability; many landed as findings that explicitly DEFER the real work
(custody, secure-origin SW hosting, native paint-to-window, the native-renderer arch
pick).

werust's 21-task spec was scoped to "**ship the webview + reach T1 on a pure-Rust
stack**" — i.e. BUILD the reached capability, and produce the benchmark HARNESS +
exploration SPEC for the deferred decisions (which stay gated on human answers). So the
gaps above are BY DESIGN: they are precisely the things wezig also deferred, now parked
in werust's `rust-successor-native-renderer-architecture-benchmark` spec (needsAnswers:
true) and the deferred web3 custody model.

## What is genuinely MISSING in werust vs wezig's built spikes (candidates, if wanted)

These are real, if you want to close the exploration gap (each is a spike wezig did that
werust has not):

1. **Native paint to a real window.** werust's T0/T1 native renderer paints to an
   in-memory `Surface` (+ text transcript) and returns a null `ViewHandle` — there is no
   windowing layer that blits the surface to screen, and `main.rs` only ever constructs
   the `WebViewRenderer`, never the `NativeRenderer`, for the live desktop. wezig spiked
   an SDL3/stb window + GPU-context frames. To match: a windowing/blit layer + wire the
   native backend as a selectable live desktop backend.
2. **EIP-6963 multi-provider announce.** werust installs only `window.ethereum`
   (single-provider, hard-override). wezig spiked EIP-6963 discovery. To match: add the
   EIP-6963 `announceProvider` event surface.
3. **Wallet broker + custody model.** werust holds NO keys and defers the whole broker;
   wezig spiked an out-of-process signing broker + a threat-analysed custody model
   (OS-keychain primary / encrypted-at-rest fallback / hardware-wallet). To match: build
   the broker spike behind the provider.
4. **ipfs:// as a secure origin / service-worker host.** wezig located + drafted a
   WebKitGTK service-worker-scheme patch so `ipfs://` can host service workers (a secure
   origin). werust treats `ipfs://` as a custom-scheme render only. To match: the
   secure-origin/SW-scheme work.

## Recommendation

If the goal was "ship a pure-Rust successor at parity with what wezig REACHED", werust is
there (T0+T1, webview, verified ipfs, EIP-1193, mobile, release). If the goal is to also
match wezig's EXPLORATION depth, items 1-4 are the concrete backlog — but they map onto
the deferred exploration spec + web3 custody model that were intentionally out of this
drive's scope, so they are human-gated design decisions, not oversights.
