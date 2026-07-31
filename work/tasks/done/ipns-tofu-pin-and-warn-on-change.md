---
title: "TOFU for mutable names: let the user bless a name's current CID, then warn when it changes (SSH-host-key-style pin-and-warn)"
slug: ipns-tofu-pin-and-warn-on-change
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [ipns-name-resolution-and-render]
covers: [1]
---

## Answered questions (human, 2026-07-30, via drive-tasks — build to these)

The three open questions below were ANSWERED; the gate is cleared. Treat these as settled decisions, and record any refinement in a spike DECISIONS block rather than reopening them.

1. **UX of the bless + warning flow.** The bless is an EXPLICIT user action reached from the TRUST INDICATOR, not a first-visit prompt (a prompt on first visit trains people to dismiss it, and werust's trust surface is already the place the posture is explained). Clicking the indicator opens a small surface showing the name, the CID it currently resolves to, and a "trust this content" action; blessing records the pin. A later resolution to a DIFFERENT CID is surfaced with the SAME prominence the repo already reserves for a fail-closed failure: a distinct, louder posture on the indicator PLUS the high-contrast in-view banner treatment, carrying a legible line of the form "this name now points to different content than the version you trusted on <date>". It is NEVER silently accepted, and it is NEVER a hard block: the user can look and decide. (Note the sibling constraint from `loading-progress-in-the-url-bar-not-a-banner`: a FAILURE-class banner may displace the page, transient in-flight state may not. A changed pin is failure-class.)
2. **Pin store — where + isolation.** A `pins.json` living NEXT TO the existing `retrieval.json`, reusing the settings mechanism in `crates/werust-core/src/retrieval.rs` verbatim: the same directory resolution and the same `WERUST_SETTINGS_DIR` env lever, and the same directory-taking `load_from` / `save_to` cores so tests isolate the store into a temp dir and assert the real one is untouched (the shared-write rule). Each pin records name -> CID + timestamp + the resolution POSTURE at bless time, so a later change can say which trust level the user was actually blessing.
3. **Scope of "mutable name".** BOTH IPNS and ENS, per the settled two-axis model (`docs/adr/0006`): both are controller-repointable, so both are blessable. A blessed-then-CHANGED name is a strictly LOUDER signal than plain `MutableName` or `NameViaTrustedRpc`, and it must not be flattened into either; an UNBLESSED name behaves exactly as it does today.

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
