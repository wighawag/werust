---
title: "werust: embedded Freenet node + a first-class Freenet scheme"
slug: embedded-freenet-node-and-scheme
needsAnswers: true
taskedAfter: [gated-protocol-subsystems-consent-and-lazy-activation]
---

> PROPOSED spec \u2014 records intent for human review before tasking. SEPARATE from the ENS/IPFS
> spec (`trustless-ens-to-ipfs-resolution-ethereum-light-client`). It adds Freenet as a
> first-class decentralised backend in werust by EMBEDDING a Freenet node in-process. Not
> yet tasked; the OPEN QUESTIONS (esp. WHICH Freenet) must be answered first.

## Problem Statement

werust should support **Freenet** as a first-class decentralised-web backend \u2014 a user opens
a Freenet app/site in werust and it works with NO separately-installed daemon, because the
node is EMBEDDED in the browser. This extends the thesis (verifiable / local-first /
serverless content) beyond `ipfs://` to Freenet's real-time decentralised-app model.

## CRITICAL DISAMBIGUATION (open question 1 \u2014 answer first)

"Freenet" is TWO different projects; the whole spec depends on which one is meant:

- **The new Freenet (freenet.org, formerly "Locutus", 2023+)** \u2014 a ground-up **Rust**
  rewrite. A global key-value store where keys are **WebAssembly "contracts"** (a contract
  defines the state's rules); a general-purpose platform for real-time decentralised APPS.
  Ships `freenet-core` (Rust) with an **embeddable node** exposing a local HTTP/WebSocket
  client API (default `127.0.0.1:50509`, publish port `7509`). A browser points at the local
  node, downloads the app's SPA over the node's HTTP proxy, then the app talks to the node
  over WebSocket. **This is almost certainly what is meant** (Rust, embeddable, browser-
  oriented, serverless-app model) and this spec assumes it unless corrected.
- **Hyphanet (the classic Freenet, since 2000)** \u2014 **Java**, a decentralised anonymous
  "hard drive" (freesites / content store, strong anonymity). Embedding a Java node in a
  Rust browser is a very different (and heavier, non-Rust) proposition. If THIS is meant,
  the spec changes substantially (JVM dependency or FFI, different addressing, anonymity
  threat model) \u2014 flag it.

**Everything below assumes the NEW Freenet (freenet.org / freenet-core, Rust).**

## Solution (shape, not final)

The new Freenet integrates through the SAME kind of seam werust already uses for `ipfs://`,
plus a live channel:

```
User opens a Freenet app  (freenet://<contract-key>[/path], or a bare key \u2014 see open Q)
   │
   ▼
[Embedded Freenet node]  freenet-core running IN-PROCESS in werust
   │   exposes the local HTTP proxy + WebSocket client API (127.0.0.1:50509 / 7509)
   ▼
[Renderer custom-scheme hook]  intercept freenet:// -> pull the app's SPA (HTML/JS/CSS)
   │   from the local node's HTTP proxy, hand the bytes to the webview to render
   ▼
[Live WebSocket bridge]  the loaded app connects to the local node's WebSocket API to
   read/create/modify contract state \u2014 real-time decentralised app behaviour
```

> **Governed by the subsystem-consent framework.** The embedded Freenet node is a HEAVY
> subsystem, so it is a `consent-gated` `Subsystem` under
> `gated-protocol-subsystems-consent-and-lazy-activation`: NOT started at browser startup; a
> `freenet://` navigation triggers the consent prompt ("this needs the Freenet subsystem: an
> embedded P2P node, uses network + battery, takes a moment to join"), and only a user who
> agrees starts it, after which the fetch proceeds. Decline gives a clean failed load. That
> framework provides the start/stop/readiness lifecycle described here.

### Embedded node (the core new capability)

- Run `freenet-core` IN-PROCESS (a background task/thread inside werust), NOT as a
  user-installed daemon \u2014 so werust is a Freenet browser out of the box. Manage its
  lifecycle with the app (start on demand / on first `freenet://` navigation; shut down
  cleanly). Config (data dir, ports, gateway peers) lives in werust's config.
- On desktop this is a native background task; on MOBILE (Android/iOS) it must run inside
  the app process (battery/network/background-execution limits are a real open question).
- The node needs to JOIN the network (gateway peers / bootstrap) \u2014 a bootstrap-trust +
  connectivity question analogous to IPFS gateways / the ENS checkpoint.

### Freenet scheme + the two channels

- A `freenet://` scheme (register on the `Renderer` custom-scheme hook, like `ipfs://`):
  the request is served by fetching the app container from the local node's HTTP proxy.
- The LIVE channel: the loaded app opens a WebSocket to the local node's client API. werust
  must allow that connection (it is same-machine localhost) and, ideally, keep the address
  bar showing the Freenet identity (the contract key), not `http://127.0.0.1:50509/...`
  (the same "keep the decentralised identity in the bar" principle as ENS/IPFS).

### Reuse vs new

- REUSE: the `Renderer` custom-scheme/interception hook (already carries `ipfs://`), the
  trust-indicator surface, the config system, the mobile app modules.
- NEW: the embedded `freenet-core` lifecycle; the `freenet://` scheme -> local-proxy fetch;
  allowing/managing the app's WebSocket to the local node; the node's bootstrap/gateway
  config; the mobile in-process-node story.

## User Stories

1. As a user, I open a Freenet app in werust and it just works \u2014 no separately-installed
   Freenet daemon (the node is embedded).
2. As a user, the Freenet app is live (real-time reads/writes of contract state via the
   node), not a static snapshot.
3. As a user, the address bar shows the Freenet identity (contract key), not a localhost URL.
4. As a user on mobile, the embedded node works within the app's platform limits (or the
   spec is explicit about what is desktop-only first).
5. As a developer, the embedded node lifecycle + the `freenet://` scheme are a clean seam,
   consistent with how `ipfs://` is wired.

## Phased delivery (proposed, for review)

- **Phase 0 \u2014 spike:** embed `freenet-core` in a throwaway build, start a node, hit its
  local HTTP/WebSocket API, load one known Freenet app end-to-end on DESKTOP. Answer the
  hard unknowns (build weight, async runtime fit, bootstrap, whether an in-process node is
  practical) before committing. (This is a `spike-*` finding, like wezig's exploration.)
- **Phase 1 \u2014 desktop, first-class:** embedded node lifecycle + `freenet://` scheme served
  via the local proxy + the app's WebSocket channel, on the WebKitGTK desktop backend, with
  the identity in the bar. Tests against a local node + a pinned app.
- **Phase 2 \u2014 mobile:** run the embedded node inside the Android/iOS app within platform
  limits (or document why it is deferred / degraded there).
- **Phase 3 \u2014 hardening:** bootstrap/gateway trust + config UX, node resource/battery
  management, cohabiting with the ipfs path + trust indicator, upgrades.

## Out of Scope (for this spec)

- Hyphanet (classic Java Freenet) \u2014 unless open question 1 says otherwise.
- Authoring Freenet contracts/apps (werust HOSTS/serves them; it does not build them).
- The ENS/IPFS work (separate spec) \u2014 though the scheme/trust-indicator patterns are shared.

## OPEN QUESTIONS (must be answered before tasking \u2014 needsAnswers: true)

1. **WHICH Freenet? (blocking)** The new Rust Freenet (freenet.org / freenet-core \u2014 assumed),
   or classic Hyphanet (Java)? Everything depends on this. (Strong recommendation + working
   assumption: the new Rust Freenet.)
2. **Embedded node feasibility + weight.** Is embedding `freenet-core` in-process
   ACCEPTABLE (binary size, dependency tree, its async runtime vs werust's, license)? Or is
   a bundled-but-separate node process (werust spawns/manages a `freenet` binary) preferable
   to an in-crate embed? (A Phase-0 spike should answer this before committing to "embedded".)
3. **Mobile in-process node.** Can a Freenet node realistically run inside the Android/iOS
   app (background execution, battery, NAT/connectivity, platform networking limits)? Is
   desktop-first acceptable, with mobile deferred/degraded?
4. **Bootstrap / gateway trust.** How does the embedded node join the network (default
   gateway peers shipped? user-configured?) \u2014 the Freenet analogue of the IPFS gateway / ENS
   checkpoint trust decision.
5. **Scheme + addressing.** `freenet://<contract-key>` as the user-facing scheme? How to
   keep the contract-key identity in the address bar rather than the local
   `http://127.0.0.1:50509/...` proxy URL the node actually serves from? How does a
   Freenet app's same-machine WebSocket to the node fit werust's request/security model?
6. **Trust-indicator semantics.** What does the verified-vs-served indicator MEAN for a
   Freenet app (its state is contract-governed, not a single hash)? Define the trust state.
7. **Maturity / stability.** The new Freenet is young and evolving; is werust comfortable
   depending on a fast-moving `freenet-core`? Pin a version + treat it as a spike-gated bet.

## Why this fits werust

The new Freenet's model \u2014 a local Rust node serving decentralised apps to a browser over a
local proxy + WebSocket \u2014 maps almost 1:1 onto werust's existing custom-scheme + fetch
seams, and it is Rust (thesis-aligned) and embeddable (browser-out-of-the-box). It extends
werust from "render verified static content (ipfs)" to "host live decentralised apps
(freenet)", a natural next capability on the same seams. The embedded-node ambition is the
differentiator \u2014 and also the main risk \u2014 which is why a Phase-0 spike gates the commitment.
