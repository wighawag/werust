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
- a **readiness signal** the scheme handler awaits before fetching;
- **live resource/status telemetry** it exposes while Active (state, uptime, peers/sync
  height where relevant, and best-effort memory / CPU / network / disk usage) for the
  management screen;
- a **PROVIDER MODE** (see below) — how the capability is actually reached.

The existing cheap schemes (HTTP-gateway `ipfs://`) are `always-on` (no behaviour change);
new heavy ones (embedded Freenet node, Ethereum light client, a future local IPFS node,
Tor) are `consent-gated`.

### Provider mode — embedded / external / gateway (a shared abstraction)

The SAME choice recurs across IPFS, Freenet, AND Ethereum: WHERE does the capability come
from? A user who already runs their own node wants werust to USE it, not spin up a redundant
embedded one. So each `Subsystem` (where it makes sense) has a configurable provider mode:

- **Embedded** — werust runs the node/client IN-PROCESS (the heavyweight, gated case: an
  embedded Freenet node, a local IPFS node, the Helios light client). This is the mode that
  needs consent + lazy start.
- **External / local** — point at a node the USER already runs (e.g. `localhost:5001` Kubo
  for IPFS, `127.0.0.1:50509` for a Freenet node, a local Ethereum RPC / their own light
  client). Cheap for werust (no embedded node to start) and often the user's PREFERRED,
  most-trusted option; typically no consent prompt (they opted in by configuring it).
- **Gateway / remote** — a public endpoint (a `dweb.link`-class IPFS gateway, a public
  Ethereum RPC). Cheapest and always-on, but the LEAST trustless (a trusted third party);
  the trust indicator must reflect that.

Provider mode interacts with gating: `embedded` is the consent-gated heavy case; `external`
and `gateway` are lightweight and usually `always-on` (but the trust indicator differs).
Defaults are a per-protocol product decision (see open Q), but the pattern is uniform, so
the config UI and the `Subsystem` trait model it ONCE for all protocols.

- **IPFS:** embedded local node | external Kubo (`localhost:5001`) | public gateway
  (`dweb.link`, today's default). (This generalises the ipfs task's existing hardcoded
  gateway into a user-choosable provider mode.)
- **Freenet:** embedded `freenet-core` node | external node the user runs (`:50509`).
- **Ethereum:** embedded Helios light client | external local RPC / user's own light client
  | public RPC (the trusted-skeleton default). (Note: even `embedded` Helios still needs an
  untrusted execution RPC underneath — itself a provider-mode-like endpoint choice.)

### Consent semantics (the UX contract)

- **Informed:** the prompt names the subsystem and its real costs (P2P node = network +
  battery + a startup delay; light client = sync time + an untrusted-RPC dependency), not a
  generic "allow?".
- **Lazy:** the subsystem starts ONLY on first consented use, never at browser startup.
- **CONFIGURE-AT-FIRST-USE (not just yes/no):** the first-use prompt is where the user picks
  the PROVIDER MODE for that subsystem, not merely grants a boolean. On the FIRST `ronan.eth`
  navigation, werust asks how the user wants Ethereum resolution to happen — e.g. "Resolve
  ENS via: [your own RPC (enter URL)] / [a public RPC (trusted)] / [the embedded light
  client (trustless, syncs on first use)]" — with the trust/cost trade-off of each shown, and
  a sensible default preselected. Same on first `ipfs://` (own Kubo node / public gateway /
  embedded) and first `freenet://` (embedded node / your own node). The choice is REMEMBERED
  and is exactly the provider-mode config the management screen later lets them change. So
  first-use consent and provider configuration are ONE moment: the browser asks "how should
  I do this?", the user answers once, and it proceeds. A user who just wants it to work takes
  the default; a user who runs their own node points at it right there. Keep the prompt
  lightweight — a good default + an "advanced/configure" affordance, not a wall of options.
- **Remembered (policy, open Q):** does consent persist (per-scheme "always start Freenet"),
  ask every time, or per-session? A remembered "always" turns a gated subsystem effectively
  `always-on` for that user.
- **Revocable:** the user can stop a running subsystem (reclaim resources) and revoke a
  remembered consent, from settings.
- **Fail-closed:** decline / start-failure -> the load FAILS with a legible reason
  ("Freenet subsystem not started, so freenet://... could not be opened"), consistent with
  the ipfs/ENS fail-closed posture. Never render a partial/unverified fallback.

### Subsystems management screen (the control center)

A dedicated settings/management surface where the user SEES and CONTROLS every subsystem —
the visible home of the model above. It lists each registered `Subsystem` and, per entry:

- **Status:** Inactive / Starting / Active / Failed / Stopped (+ error reason if failed).
- **Resource consumption (ideally):** best-effort memory / CPU / network / disk for the
  subsystem, plus protocol-relevant stats (Freenet/IPFS peer count; light-client sync
  height / last-verified block; uptime). Honest "not available" where a metric can't be
  measured rather than a fake number.
- **Controls:** start / STOP a running subsystem (reclaim resources / battery); its gating
  policy (allow / ask / disable); revoke a remembered consent; a warm PRE-START (start it
  from here before navigating).
- **Configure — provider mode + endpoint:** choose Embedded / External / Gateway and set
  the endpoint (e.g. IPFS: embedded | `localhost:5001` | `dweb.link`; Freenet: embedded |
  the user's node; Ethereum: embedded Helios | local RPC | public RPC, plus the underlying
  untrusted execution-RPC URL). Plus per-subsystem config the backend exposes (data dir,
  ports, gateway/bootstrap peers, checkpoint).
- **Trust note:** what each provider mode means for trust (embedded/external = your own;
  gateway/public = a trusted third party), tying into the trust indicator.

This is one screen, protocol-agnostic: it renders whatever each `Subsystem` declares
(status + telemetry + config schema), so a NEW protocol appears here automatically by
implementing the trait — no bespoke settings page per protocol. Reachable from a settings
entry point (and, sensibly, `about:`-style, e.g. `werust://subsystems`, if that fits the
scheme model).

### Reuse vs new

- REUSE: the `Renderer` custom-scheme hook (schemes already dispatch through it); the load-
  failure -> chrome-reason path; the trust-indicator surface; the config/settings system.
- NEW: the `Subsystem` abstraction + registry; the provider-mode (embedded/external/gateway)
  + endpoint config; the consent prompt UI (desktop chrome + mobile edges); the lazy async
  start/stop lifecycle + readiness gating in front of a scheme handler; the remembered-
  consent policy store; the SUBSYSTEMS MANAGEMENT SCREEN (status + resource telemetry +
  controls + provider/endpoint config).

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
   the scheme; the gate/consent/lifecycle/config/management-row are provided, not re-
   implemented per protocol.
7. As a user, on the FIRST use of a protocol, the browser asks me HOW to do it (e.g. on
   `ronan.eth`: my own RPC / a public RPC / the embedded light client), with the trade-offs
   shown and a sensible default — not a bare yes/no — and remembers my choice.
8. As a user, I can open a subsystems management screen that shows every subsystem, whether
   it is running, and (ideally) how much memory / CPU / network it is using, and stop,
   disable, or RECONFIGURE (change provider mode/endpoint) any of them.
9. As a user who runs my OWN node, I can point werust at it (external `localhost` / my own
   RPC) instead of the embedded one, per protocol — or choose a public gateway — and werust
   uses my choice and reflects its trust implication.

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
  first-use consent prompt (with a provider-mode CHOICE, not bare yes/no) + lazy start/stop
  + readiness gating + the provider-mode field, proven end-to-end on ONE real gated
  subsystem (whichever heavy backend lands first \u2014 likely the Freenet spike or the light
  client). Existing cheap schemes stay always-on, unchanged.
- **Phase 2 \u2014 the management screen + provider config + remembered consent** (the subsystems control
  center: per-subsystem status + controls + STOP + revoke remembered consent; provider-mode
  and endpoint config, embedded | external | gateway, per protocol — starting by
  generalising the ipfs task's hardcoded gateway into a user-choosable provider; the
  first-use prompt writes into the SAME config this screen edits; resource telemetry
  best-effort here, deeper metrics later).
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
8. **First-use prompt weight.** How much config surfaces on first use vs. "take the default,
   configure later in the management screen"? (Recommend: a good default preselected + a
   compact provider choice + an "advanced" affordance — NOT a wall of endpoint fields on
   first navigation; the management screen holds the full config.)
9. **Default provider mode per protocol.** What ships as default? (IPFS = public gateway
   `dweb.link` today, cheapest but least trustless; Freenet/Ethereum = embedded, or
   gateway/public-RPC to start?) And how prominently is the trust trade-off surfaced when a
   user is on gateway/public mode vs their own node?
10. **Resource-telemetry depth + portability.** How deep do per-subsystem metrics go, and
    how measured cross-platform (Linux/Android/iOS have very different per-process/thread
    accounting)? (Recommend: best-effort with honest "unavailable", never a fabricated
    number.)
11. **Management-screen surface.** A native settings pane, an internal page (e.g.
    `werust://subsystems`), or both? (An internal page dogfoods werust's own rendering; a
    native pane is simpler on mobile.)

## Why this is the right long-run bet

As werust adds protocols, "run every backend always" does not scale \u2014 it is wasteful and
exposes users to subsystems they never use. A single, consistent consent + lazy-activation
gate lets werust support MANY heavy protocols cheaply: each new backend implements one trait
and gets consent/lifecycle for free, the user stays in control of what runs on their
machine, and the default posture stays lean and fail-closed. It is the framework that makes
"support all the decentralised protocols" sustainable rather than a resource free-for-all.
