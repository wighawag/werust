---
title: Native-renderer benchmark harness — capability + trust-hooks + vs-wezig meter
slug: native-renderer-benchmark-harness-capability-and-trust-hooks
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-core-css-stylo-and-latin-shaping-parley, renderer-seam-trust-hook-qualification-gate]
covers: [20, 21]
---

## What to build

Build the capability + trust-hook benchmark harness that evaluates a native-renderer
architecture against BOTH rendering capability (the pinned conformance-ladder page
checklists + WPT subsets) AND the trust hooks (provider injection + `ipfs://` scheme,
via the qualification gate). Also capture the vs-wezig comparison meter: the T1 climb
measured against wezig's Zig arm on the SHARED ladder — effort, code volume, and
friction (especially DOM object-graph friction). The harness is the EVIDENCE
generator the follow-on exploration spec
(`rust-successor-native-renderer-architecture-benchmark`) consumes to DECIDE the
architecture — it does NOT itself pick the architecture.

## Acceptance criteria

- [ ] The harness scores a candidate native-renderer path on the pinned page checklists + WPT subsets (capability) and on the trust-hook qualification (pass/fail).
- [ ] It records a comparable vs-wezig meter (capability score + effort/code-volume/friction signals) on the shared conformance ladder.
- [ ] Its output is a structured, comparable report suitable for the exploration spec's architecture decision (candidates: own-engine vs Servo vs Blitz/Stylo assembly).
- [ ] The harness is re-runnable and its scores are reproducible; tests cover the scoring logic.

## Blocked by

- Blocked by `t1-core-css-stylo-and-latin-shaping-parley` and `renderer-seam-trust-hook-qualification-gate`.

## Prompt

> Goal: build the EVIDENCE generator for the deferred architecture decision — a
> capability + trust-hook benchmark harness + the vs-wezig meter (see `CONTEXT.md`,
> `docs/conformance-tiers.md`, and the exploration spec
> `rust-successor-native-renderer-architecture-benchmark`).
>
> Score a candidate native-renderer path on capability (page checklists + WPT subsets)
> AND trust-hooks (provider injection + `ipfs://` — reuse
> `renderer-seam-trust-hook-qualification-gate`), and capture the vs-wezig comparison
> (effort, code volume, DOM object-graph friction) on the shared ladder — this is the
> reversible experiment's measurement (`docs/adr/0001`). Do NOT decide the architecture
> here: this produces the report the exploration spec's human-resolved decision
> consumes. Make scores reproducible.
>
> Done = a re-runnable harness emits a structured capability + trust-hook + vs-wezig
> report that the follow-on exploration spec can decide the native-renderer
> architecture from.
