---
title: "werust roadmap — sequenced specs and the order to task/build them"
slug: roadmap
kind: roadmap
---

> A living index that organises the proposed specs into a BUILD ORDER so they can be tasked
> and implemented in turn. Not a spec itself; the ordering rationale + dependencies. Current
> truth stays in each spec + `docs/adr/` + the code. Update as specs are tasked/land.

## Shipped (done)

- **`rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack`** \u2014 the T0+T1 browser:
  WebKitGTK webview backend behind the `Renderer` seam; native T0+T1 renderer (html5ever +
  stylo-stack cascade + parley shaping) at conformance parity; hash-verified `ipfs://`
  render; EIP-1193 provider stub; Android + iOS app modules; GoReleaser release (v0.1.0,
  desktop x86_64 + APK + iOS Simulator `.app`). This is the foundation everything below
  extends.

## Proposed specs (all `needsAnswers`) and how they relate

```
                    [gated-protocol-subsystems]   <-- the framework (consent, provider mode,
                       consent + lazy activation        management screen, configure-at-first-use)
                       /            |            \
                      /             |             \
   [trustless-ens-to-ipfs]   [embedded-freenet]   [privacy-routing]
    (Helios = a subsystem)   (node = a subsystem)  (socks5h/Tor/VPN + profiles;
            |                        |               CONSTRAINS every subsystem's egress)
            |                        |                        |
            |                        |               [fingerprinting-resistance]
            |                        |                (unlinkability; assumes no-leak;
            |                        |                 aligns with native renderer)
            +------------ all reuse the Renderer custom-scheme hook + Fetcher + trust indicator
```

Key cross-cutting truths:
- **`gated-protocol-subsystems`** is the FRAMEWORK the heavy backends plug into (Helios,
  Freenet node, embedded Tor are all `Subsystem`s). It wants to exist BEFORE / ALONGSIDE the
  first heavy subsystem so it is not retrofitted.
- **`privacy-routing`** CONSTRAINS every subsystem (a subsystem that can't route through the
  active transport is disabled in a private profile) \u2014 so it is co-designed with the
  subsystem framework, not bolted on later.
- **`fingerprinting-resistance`** ASSUMES `privacy-routing` and is deliberately LAST /
  longest-horizon, but kept in view so earlier work is fingerprinting-aware.

## Recommended build order (task in turn)

Ordered for: visible wins early, framework-before-dependents, and privacy co-designed with
subsystems. Each item is a spec to run through `to-task`, then drive with the supervised
`--review --merge` loop.

1. **Mobile chrome UX fix** (NOT a spec \u2014 a shipped-code bug task): the mobile toolbar's
   back/forward/reload/stop buttons crowd out the URL bar. Small, immediate, improves
   testability of everything else. Do FIRST. (Tasked directly.)

2. **`trustless-ens-to-ipfs` \u2014 Phase 1** (trusted-RPC skeleton): bare `ronan.eth` -> ENS
   namehash/resolver/contenthash via a (trusted, labelled) RPC -> EIP-1577 decode with
   GRACEFUL protocol-named errors (Arweave/Swarm/IPNS rejected clearly) -> existing verified
   ipfs render. Delivers the headline `ronan.eth` win, self-contained, no framework needed
   yet (the RPC skeleton is `always-on`/cheap). HIGH value, LOW risk.

3. **`gated-protocol-subsystems` \u2014 Phase 1** (the framework): the `Subsystem` trait +
   registry + first-use consent-with-provider-choice + lazy start/stop + provider-mode field,
   proven on a stub or the first real backend. Build BEFORE the heavy backends so they slot
   in. Also generalises the ipfs hardcoded gateway into a provider mode.

4. **`trustless-ens-to-ipfs` \u2014 Phase 2** (Helios light client as a gated subsystem): the
   trustless endgame \u2014 `ronan.eth` verified against a validated chain head; the first-use
   prompt offers own-RPC / public-RPC / embedded-light-client. Depends on 2 + 3.

5. **`privacy-routing` \u2014 Phase 1** (external SOCKS5h, leak-proof webview + fetcher, WebRTC
   off, fail-closed, leak self-check). The anti-leak spine. Co-reads the subsystem framework
   (3) so the "constrains subsystems" rule is honoured as subsystems land.

6. **`privacy-routing` \u2014 Phase 2** (Chrome-style profiles: isolated state + per-profile
   transport).

7. **`gated-protocol-subsystems` \u2014 Phase 2** (management/control-center screen + provider-
   mode config + remembered consent + resource telemetry). Naturally follows once there are
   multiple subsystems to manage.

8. **`embedded-freenet-node` \u2014 Phase 0 spike** then **Phase 1** (freenet:// via an embedded
   `freenet-core` node as a gated subsystem). Spike-gated (confirm the new Rust Freenet;
   validate embed feasibility) before committing. Depends on 3.

9. **`privacy-routing` \u2014 Phase 3** (embedded Tor via `arti` as a gated subsystem +
   per-subsystem under-Tor routing/disable policy). Depends on 3 + 5 + the subsystem set.

10. **`fingerprinting-resistance` \u2014 Phase 0 spike** then the cheap axes (UA/headers/timezone/
    letterboxing). Longest horizon; assumes 5/6; deepens with the native renderer.

Later/parallel, as the native renderer matures: T2 (floats/flex/grid/tables + complex
shaping), T3 (JS engine), and the deferred wallet-broker/custody + native-paint-to-window +
EIP-6963 + IPNS + CCIP-Read items from the vs-wezig parity finding.

## Open confirmations blocking specific items

- **Freenet (item 8):** confirm the NEW Rust Freenet (freenet.org / freenet-core), not
  classic Hyphanet (Java).
- **Per-spec `needsAnswers`:** each spec still has fine-grained open questions (checkpoint
  mechanics, default provider modes, arti maturity, telemetry depth, etc.) to settle at or
  before tasking \u2014 listed in each spec.

## How to use this

When ready to build the next item: run its spec through `to-task` (fills judgement gaps,
emits READY tasks into `work/tasks/`), then drive with the supervised conductor loop
(`--review --merge`), exactly as the T0+T1 spec was built. Re-order freely as priorities
shift; the DEPENDENCY arrows (framework before dependents; privacy co-designed with
subsystems; fingerprinting after no-leak) are the only hard constraints.
