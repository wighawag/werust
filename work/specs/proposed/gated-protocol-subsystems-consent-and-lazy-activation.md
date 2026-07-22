---
title: "werust: gated protocol subsystems \u2014 consent + lazy activation for heavyweight backends"
slug: gated-protocol-subsystems-consent-and-lazy-activation
status: proposed
needsAnswers: true
---

> PROPOSED spec \u2014 records intent for human review before tasking. A CROSS-CUTTING model
> that governs how heavyweight protocol backends (embedded Freenet node, Ethereum light
> client, an IPFS node, Tor, \u2026) are activated. It is the shared framework the per-protocol
> specs (`embedded-freenet-node-and-scheme`, `trustless-ens-to-ipfs-resolution-...`) plug
> into. Not yet tasked; OPEN QUESTIONS below need answers first.

## Problem Statement

werust aims to support MANY decentralised protocols (`ipfs://`, Freenet, ENS-via-light-
client, and more later). Some are cheap (an HTTP-gateway IPFS fetch); others are HEAVY \u2014 an
embedded Freenet node, an Ethereum light client, a local IPFS node, Tor \u2014 each carrying real
cost: memory/CPU/battery, network connections, background execution, a bootstrap/sync delay,
and its own trust/privacy surface. Booting ALL of them at startup would be wasteful,
slow, and would expose the user to subsystems they may never use.

The user wants: **heavyweight subsystems are NOT run by default.** When the user navigates
to a resource whose scheme REQUIRES such a subsystem, werust should TELL the user "this
scheme needs the <X> subsystem", and only if they AGREE does werust start it, then proceed
to fetch/render the resource. Lightweight schemes proceed without a prompt.

## Solution (shape, not final)

A **capability/subsystem gate** sitting between "navigate to a gated scheme" and "the
scheme's handler runs". Each protocol backend declares whether it needs a SUBSYSTEM and how
heavy it is; the gate handles consent + lazy lifecycle.

```
User navigates to  freenet://<key>   (a GATED scheme)
   │
   ▼
[Subsystem gate]  is the `freenet` subsystem RUNNING?
   ├─ running        -> proceed straight to the handler
   ├─ not running    -> CONSENT PROMPT: "Opening this needs the Freenet subsystem
   │                     (an embedded peer-to-peer node: uses network + battery,
   │                     takes a moment to join the network). Start it?"  [Start] [Cancel]
   │        ├─ user AGREES  -> START the subsystem (async, with progress) -> then run handler
   │        └─ user DECLINES -> FAIL the load cleanly with a clear reason (no partial start)
   ▼
[Scheme handler]  (freenet:// / light-client-backed ens / ipfs-node / ...) fetches + renders
```

### A `Subsystem` abstraction (the new seam)

Each heavy backend is a `Subsystem` with:
- an **id + human description** (what it is, what it costs \u2014 network/battery/disk, sync
  delay, trust/privacy notes) shown in the consent prompt;
- a **gating policy**: `always-on` (cheap, no prompt), `consent-gated` (prompt then lazy-
  start), or `disabled`;
- an **async lifecycle**: `Inactive -> Starting(progress) -> Active -> (Failed | Stopped)`,
  startable on demand and stoppable to reclaim resources;
- a **readiness signal** the scheme handler awaits before fetching.

The existing cheap schemes (HTTP-gateway `ipfs://`) are `always-on` (no behaviour change);
new heavy ones (embedded Freenet node, Ethereum light client, a future local IPFS node,
Tor) are `consent-gated`.

### Consent semantics (the UX contract)

- **Informed:** the prompt names the subsystem and its real costs (P2P node = network +
  battery + a startup delay; light client = sync time + an untrusted-RPC dependency), not a
  generic "allow?".
- **Lazy:** the subsystem starts ONLY on first consented use, never at browser startup.
- **Remembered (policy, open Q):** does consent persist (per-scheme "always start Freenet"),
  ask every time, or per-session? A remembered "always" turns a gated subsystem effectively
  `always-on` for that user.
- **Revocable:** the user can stop a running subsystem (reclaim resources) and revoke a
  remembered consent, from settings.
- **Fail-closed:** decline / start-failure -> the load FAILS with a legible reason
  ("Freenet subsystem not started, so freenet://... could not be opened"), consistent with
  the ipfs/ENS fail-closed posture. Never render a partial/unverified fallback.

### Reuse vs new

- REUSE: the `Renderer` custom-scheme hook (schemes already dispatch through it); the load-
  failure -> chrome-reason path; the trust-indicator surface; the config/settings system.
- NEW: the `Subsystem` abstraction + registry; the consent prompt UI (desktop chrome +
  mobile edges); the lazy async start/stop lifecycle + readiness gating in front of a
  scheme handler; the remembered-consent policy store.

## User Stories

1. As a user, opening a lightweight resource (HTTP, gateway-`ipfs://`) never prompts me.
2. As a user, opening a scheme that needs a heavy subsystem tells me WHAT it needs and what
   it costs, and starts it only if I agree.
3. As a user, if I decline, the page fails clearly (naming the subsystem) rather than
   hanging or silently doing nothing.
4. As a user, I can choose to remember my choice ("always start Freenet for freenet://") so
   I am not asked every time.
5. As a user, I can stop a running subsystem and revoke a remembered consent to reclaim
   resources / battery.
6. As a developer, adding a new heavy protocol = implement the `Subsystem` trait + register
   the scheme; the gate/consent/lifecycle are provided, not re-implemented per protocol.

## How the per-protocol specs plug in

- **`embedded-freenet-node-and-scheme`:** the embedded Freenet node IS a `consent-gated`
  `Subsystem`; `freenet://` navigation triggers the consent+lazy-start defined here. (That
  spec's "start on demand / shut down cleanly" lifecycle is THIS spec's mechanism.)
- **`trustless-ens-to-ipfs-resolution-...`:** the Ethereum LIGHT CLIENT (Helios) is a
  `consent-gated` `Subsystem` (sync delay + untrusted-RPC dependency to disclose); the
  trusted-RPC skeleton backend is cheap enough to be `always-on`. Resolving a `.eth` name
  via the light client triggers the gate.
- Future: a local IPFS node, Tor, other chains \u2014 all the same pattern.

## Phased delivery (proposed, for review)

- **Phase 1 \u2014 the framework, one gated subsystem:** the `Subsystem` trait + registry +
  consent prompt + lazy start/stop + readiness gating, proven end-to-end on ONE real gated
  subsystem (whichever heavy backend lands first \u2014 likely the Freenet spike or the light
  client). Existing cheap schemes stay always-on, unchanged.
- **Phase 2 \u2014 remembered-consent policy + revocation UI** (settings: per-scheme allow/ask/
  disable, stop-running-subsystem).
- **Phase 3 \u2014 mobile consent UX** (native prompt at the OS edge) + resource/battery-aware
  auto-stop of idle subsystems.

## Out of Scope (for this spec)

- The per-protocol backends themselves (their own specs) \u2014 this is the GATE/consent/
  lifecycle framework they use, not the protocols.
- A full site-permissions system (camera/mic/geolocation-style) \u2014 this is specifically
  about PROTOCOL SUBSYSTEM activation, though the two may later share a settings surface.

## OPEN QUESTIONS (must be answered before tasking \u2014 needsAnswers: true)

1. **Consent granularity.** Per-SCHEME ("allow Freenet") or per-SITE/per-origin ("allow
   freenet for THIS key")? Per-scheme is simpler and matches "the subsystem is what's
   heavy"; per-site is finer but more prompts. (Recommend: per-SCHEME for the subsystem
   start; the subsystem is the cost, not the individual resource.)
2. **Remembered consent default.** Ask-every-time, remember-per-session, or offer a
   "remember" checkbox that persists? And is the default gating for a NEW heavy subsystem
   `consent-gated` (recommended) \u2014 never silently `always-on`?
3. **Which subsystem proves Phase 1.** Build the framework against the Freenet node, the
   Ethereum light client, or a deliberately trivial stub subsystem first? (Recommend: land
   the framework with a stub/first-real subsystem so it is not entangled with one backend's
   maturity.)
4. **Mobile consent + node lifecycle.** How does the consent prompt + a running heavy
   subsystem behave on Android/iOS (native prompt; background-execution limits; auto-stop on
   backgrounding)? Desktop-first acceptable?
5. **Startup-cost disclosure.** How much does the prompt promise about cost/latency (a P2P
   node's join time, a light client's sync)? A generic warning vs a per-subsystem detailed
   disclosure. (Recommend: per-subsystem, honest about network/battery/delay/trust.)
6. **Trust-indicator interaction.** Does an active heavy subsystem change the trust
   indicator (e.g. "served via your local Freenet node")? Coordinate with the trust-
   indicator surface.
7. **Failure vs retry.** On start FAILURE (not decline), retry/backoff UX vs a plain failed
   load with a reason. And can the user pre-start a subsystem from settings before
   navigating (warm start)?

## Why this is the right long-run bet

As werust adds protocols, "run every backend always" does not scale \u2014 it is wasteful and
exposes users to subsystems they never use. A single, consistent consent + lazy-activation
gate lets werust support MANY heavy protocols cheaply: each new backend implements one trait
and gets consent/lifecycle for free, the user stays in control of what runs on their
machine, and the default posture stays lean and fail-closed. It is the framework that makes
"support all the decentralised protocols" sustainable rather than a resource free-for-all.
