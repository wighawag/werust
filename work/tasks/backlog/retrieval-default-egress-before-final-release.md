---
title: "RELEASE GATE: the shipped default IPFS egress must not be a single third-party gateway (built-in verified retrieval, or a first-run community-gateway choice)"
slug: retrieval-default-egress-before-final-release
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
needsAnswers: true
blockedBy: [retrieval-backend-user-setting]
covers: [1]
---

<!-- open-questions -->

## Open questions

1. **Which final default?** Two acceptable end-states (decide, likely both offered): (a) werust's BUILT-IN verified retrieval (embedded-p2p / fetch-only, no third-party gateway) as the default — this depends on the embedded-p2p backend existing (Phase-2 async), so confirm timing; or (b) a FIRST-RUN user choice from a curated community-provided gateway set (no silent default gateway). Which ships, and is (b) the interim until (a) is ready?
2. **Community gateway set.** If (b): where does the curated list come from, how is it kept current, and how is the privacy/trust of each conveyed at choice time?
3. **Enforcement.** How is "the shipped default is not a single hard-coded third-party gateway" ENFORCED so it cannot regress silently (a test/guard tied to the release, in the spirit of the parity guard)?

<!-- /open-questions -->

## What to build

A privacy release-gate. The Phase-1 default retrieval backend is a public trustless gateway (convenient, but it sees every site the user visits). That is fine for dev / early builds, but MUST NOT be the silent default of a shipped privacy-focused browser. Before final release, change the default to either werust's built-in verified retrieval (embedded-p2p / fetch-only, no third-party gateway) OR a first-run user choice from a curated community-provided gateway set — and enforce that the shipped default is never a single hard-coded third-party gateway.

This is the tracked companion to the default-RPC-endpoint privacy concern (same class: a default egress a third party observes). It exists so the decision is made deliberately at release, not defaulted-by-omission.

## Acceptance criteria

- [ ] The shipped final-release default IPFS egress is NOT a single hard-coded third-party gateway: it is either built-in verified retrieval or a first-run user choice from a curated set.
- [ ] The privacy/trust trade-off of the default is legible to the user (what a chosen backend can observe).
- [ ] A guard/test tied to the release prevents silently regressing to a hard-coded third-party default (in the spirit of the parity guard).
- [ ] Tests network-isolated; mirror the repo's style.

## Blocked by

- Blocked by `retrieval-backend-user-setting` (the selector + custom-URL mechanism this changes the default of). `needsAnswers: true` until the final-default choice, the community-set sourcing, and the enforcement are settled. Note: option (a) built-in retrieval depends on the embedded-p2p backend (Phase-2 async), so this may sequence after that.

## Prompt

> Goal: ensure werust does not SHIP with a single third-party gateway as its silent default IPFS egress (it sees every site the user visits). Before final release, make the default either built-in verified retrieval (embedded-p2p / fetch-only) or a first-run choice from a curated community gateway set, with the privacy trade-off legible and a guard preventing silent regression. This is the tracked companion to the default-RPC privacy concern. FIRST re-check the retrieval seam + setting landed as assumed. RECORD the final-default decision as an ADR.
