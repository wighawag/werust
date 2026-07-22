---
title: "TOFU for mutable names: let the user bless a name's current CID, then warn when it changes (SSH-host-key-style pin-and-warn)"
slug: ipns-tofu-pin-and-warn-on-change
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
needsAnswers: true
blockedBy: [ipns-name-resolution-and-render]
covers: [1]
---

<!-- open-questions -->

## Open questions

1. **UX of the bless + warning flow.** How does the user record a mutable name's current CID as genuine (an explicit action on the trust indicator? a first-visit prompt?), and how is a later CHANGE surfaced (a popup? a banner? a distinct posture)? A change should be legible ("this name now points to different content than the version you trusted on <date>"), not silently accepted and not a hard block.
2. **Pin store — where + isolation.** Where is the pinned name->CID+timestamp store kept (a small file; likely alongside the settings mechanism from `retrieval-backend-user-setting`), and how is it isolated in tests (shared-write rule)? Does it also record the resolution posture at bless time?
3. **Scope of "mutable name".** Applies to IPNS AND ENS (both mutable per the two-axis trust model). Confirm it covers both, and how it interacts with the `MutableName` / `NameViaTrustedRpc` postures (a blessed-then-changed name is a louder warning than plain mutable).

<!-- /open-questions -->

## What to build

Trust-on-first-use for mutable names. A mutable name (IPNS, or ENS — both are controller-repointable per the settled two-axis trust model, see `work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`) resolves to a CID that can change under the user. This task lets the user BLESS the current CID as genuine, then WARNS them if a later resolution returns a different CID — the SSH-host-key model applied to names. It turns the `MutableName` warning from "this could change" into "this changed since you trusted it", which is the actually-actionable signal.

Builds on `ipns-name-resolution-and-render` (which adds IPNS resolution + the `MutableName` posture) and likely reuses the settings/persistence mechanism from `retrieval-backend-user-setting`.

## Acceptance criteria

- [ ] A user can record (bless) a mutable name's current CID as genuine from the trust surface; the pin (name -> CID + timestamp + posture) persists across launches.
- [ ] On a later resolution of a blessed name to a DIFFERENT CID, the user is warned legibly ("points to new content since you trusted it on <date>"), not silently accepted and not hard-blocked.
- [ ] Applies to both IPNS and ENS mutable names (per the two-axis model); a blessed-then-changed name is a distinct, louder signal than plain `MutableName`.
- [ ] Fail-safe: an unblessed name behaves exactly as before (plain `MutableName` / `NameViaTrustedRpc`); the pin store never causes a load to render unverified content.
- [ ] Present-or-tracked on desktop + iOS + Android (parity guard); tests network-isolated and isolate the pin store (shared-write rule), asserting the real store is untouched.

## Blocked by

- Blocked by `ipns-name-resolution-and-render` (the `MutableName` posture + IPNS resolution this pins). `needsAnswers: true` until the bless/warn UX, the pin store, and the ENS+IPNS scope are settled.

## Prompt

> Goal: add trust-on-first-use for mutable names — let the user bless a name's current CID, then warn on a later change (SSH-host-key model). Turns `MutableName` from "could change" into the actionable "changed since you trusted it". Covers both IPNS and ENS (both mutable per the settled two-axis trust model). Reuse the settings/persistence mechanism; keep it fail-safe (unblessed names unchanged; the pin never causes unverified rendering). FIRST re-check the `MutableName` posture + IPNS resolution landed as assumed and route to needs-attention on drift. RECORD the UX + pin-store decisions durably.
