---
title: "Name via trusted RPC" trust state in the trust indicator
slug: name-via-trusted-rpc-trust-state
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

Extend the existing verified-vs-served trust indicator with a THIRD, distinct state for an ENS-resolved page in Phase 1: "content-verified, name via TRUSTED RPC". This is distinct from BOTH the existing "content-verified" (ipfs CID typed directly) state AND the "served" (unverified origin) state, because the name→CID mapping was taken on a trusted RPC's word even though the IPFS bytes were still hash-verified. It must NEVER be surfaced as "verified" / "name-verified" — Phase 1 has no light client, so name-verification is not claimed (the "name-verified" state is a Phase-2 addition).

Add the new posture to the `TrustPosture` enum (in the `renderer` crate), thread it through the shell's `ChromeState` (and its `is_*` helpers) and the desktop chrome's indicator rendering, exactly as the existing content-verified posture is threaded. The posture is set from the ACTUAL load path (a load whose CID came from an ENS resolution over the trusted RPC), never guessed from the URL — mirroring how the existing content-verified posture is only set when bytes actually verify. Fail-closed: a load that did not go through ENS trusted-RPC resolution never gets this state.

Scope fence / handoff: this task adds and plumbs the STATE (the enum variant, the `ChromeState`/helper threading, the desktop indicator) and proves it at the seam with a fake backend. The REAL wiring that makes an ENS-originated `ipfs://` load actually report this posture (instead of the `ContentVerified` that the `install_ipfs` scheme handler unconditionally marks) is OWNED by the front-door task `bare-eth-urlbar-front-door-end-to-end`, which consumes this variant. Keep the `TrustPosture`/`ChromeState` edit here so the two tasks do not collide on the same enum/struct.

## Acceptance criteria

- [ ] `TrustPosture` gains a distinct "content-verified, name via trusted RPC" variant, separate from `ContentVerified` and `UnverifiedOrigin`.
- [ ] The shell `ChromeState` and its helpers expose the new state, and the desktop chrome renders it as a legible, visually-distinct indicator.
- [ ] The state is driven by the actual load path (an ENS trusted-RPC resolution), not the URL string; a plain ipfs or served load never shows it.
- [ ] It is never surfaced as "verified" / "name-verified" (Phase 1 makes no name-verification claim).
- [ ] Tests at the seam (a fake backend marking the new posture, mirroring the existing trust-posture tests) assert the chrome reflects the new state and that it does not leak onto a later served/plain load.
- [ ] Tests cover the new behaviour (mirror the repo's existing test style).

## Blocked by

- None — can start immediately. (Serialized ahead of the `.eth` front-door task, which consumes this posture; keeping the `TrustPosture`/`ChromeState` edit in one task avoids a merge collision.)

## Prompt

> Goal: add the third trust-indicator state the spec requires — "content-verified, name via TRUSTED RPC" — for an ENS-resolved Phase-1 page. Its whole reason to exist is trust honesty: the IPFS bytes were hash-verified (as today), but the name→CID mapping came from a TRUSTED RPC, so the page is neither "verified" nor merely "served". It is a distinct, honestly-labelled middle state, and it must never be shown as "verified"/"name-verified" (there is no light client in Phase 1).
>
> Domain vocabulary: the trust indicator is werust's product surface for trust posture (`docs/adr/0001`: the trust posture is a product surface, not a silent internal). Today it has two states modelled by the `TrustPosture` enum in the `renderer` seam crate: `ContentVerified` (bytes hash-checked on the content-addressed path) and `UnverifiedOrigin` (a plain served load). The existing `trust-indicator-verified-vs-served` task (see `work/tasks/done/`) is the exact precedent for how a posture is threaded: `TrustPosture` (renderer) → `ChromeState` + `is_*` helpers + `refresh_chrome` (werust-core `BrowserShell`) → the desktop indicator in the `werust` binary. Add the new variant and thread it the SAME way.
>
> Crucially, mirror the existing "posture tracks the ACTUAL load path, not the URL" discipline: the current `ContentVerified` posture is set only when bytes actually verify (the `ipfs://` scheme handler calls `mark_content_verified` on a real verified resolution; a fresh `begin` resets to untrusted). This new state must likewise be set only when a load actually went through ENS trusted-RPC resolution — never inferred from a `.eth`-looking URL. This task adds and plumbs the STATE and its wiring hook; the `.eth` front-door task consumes it by marking the posture on a real ENS resolution (and owns resolving the clash where the `ipfs://` scheme handler would otherwise mark the same load plain `ContentVerified`).
>
> Test at the seam with a fake backend (the `werust-core` and `renderer` crates already have fake-backend trust-posture tests — copy that shape): assert the chrome reflects the new state, and that it does not leak onto a later plain/served load (a fresh navigation resets to untrusted).
>
> Done = the new posture exists, is threaded to a visible distinct indicator, is proven at the seam to track the real load path and not leak, and is never labelled "verified". FIRST re-check the existing trust plumbing has not moved since this snapshot (WORK-CONTRACT.md "Drift is a needs-attention signal"). RECORD any non-obvious in-scope decision (the exact variant name, the indicator's visual/text label) durably per the task template.
