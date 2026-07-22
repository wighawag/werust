---
title: "Trust-posture model: two orthogonal axes (resolution-trust x mutability) + a most-important-warning display rule"
date: 2026-07-22
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: design-decision
---

## The model (settled with the human, 2026-07-22) — record as an ADR when the MutableName posture lands

werust's trust indicator communicates two ORTHOGONAL axes, and the chrome shows the MOST IMPORTANT (loudest) applicable warning.

### Axis 1 — resolution-trust: HOW did we learn the name -> CID mapping?

- direct `ipfs://<cid>`: n/a (the hash IS the name; nothing was resolved).
- ENS `.eth` (Phase 1): via a TRUSTED RPC (`NameViaTrustedRpc`) — the RPC could misdirect the name to a different valid CID.
- ENS `.eth` (Phase 2): trustless (Helios light client) — the RPC-trust warning goes away.
- IPNS: a signature-verified IPNS record (`0x0300`), verified client-side against the key — no RPC-trust warning.

### Axis 2 — mutability: can the CONTROLLER repoint the name to different bytes?

- direct `ipfs://<cid>`: NO — immutable, the CID pins the exact bytes.
- ENS `.eth` (ipfs-ns OR ipns-ns): YES — the name owner can call `setContenthash` and repoint. Mutable, change is onchain/public.
- IPNS: YES — the key holder can publish a new record. Mutable, change is off-chain/opaque.

KEY POINT (human's insight): an ENS name is NOT truly immutable at the name layer — the owner can always repoint it. We CANNOT cheaply detect "this name is locked" (burned keys, NameWrapper fuses, locked resolver) from a single read; that detection is its own deferred problem. So until we can PROVE immutability, ENS names are treated as MUTABLE, exactly like IPNS.

### The postures

- `ContentVerified` — bytes hash-verified AND immutable (only a direct `ipfs://<cid>`).
- `MutableName` (NEW) — bytes hash-verified, but the name->CID mapping is controller-repointable. The honest floor for ANY mutable name (ENS or IPNS) whose bytes verified. NOT `ContentVerified` (it is honestly weaker), NOT "served/unverified" (the bytes DID verify).
- `NameViaTrustedRpc` — resolution-trust warning: the name was resolved over a trusted RPC. STRONGER (louder) than `MutableName` because a misdirecting RPC is worse than an honest owner repointing.
- `UnverifiedOrigin` — the `⚠` served state (bytes not hash-verified).

### Display rule: show the MOST IMPORTANT applicable warning

When several apply, the loudest wins:

| Load | Mutable? | Resolution trust | Posture SHOWN |
| --- | --- | --- | --- |
| direct `ipfs://<cid>` | no | n/a | `ContentVerified` |
| ENS ipfs-ns (Phase 1) | yes | trusted RPC | `NameViaTrustedRpc` (dominant) |
| ENS ipfs-ns (Phase 2, trustless) | yes | trustless | `MutableName` (RPC warning cleared, mutability remains) |
| IPNS | yes | n/a | `MutableName` |

Elegance: when Phase 2 removes RPC trust, ENS naturally falls back to `MutableName` with NO display-rule change — the louder warning just clears. And a future "prove immutability" capability (burn-keys / NameWrapper-fuse / locked-resolver detection) is what would let a SPECIFIC ENS name earn `ContentVerified` instead of `MutableName`.

### Follow-ons this model spawns (tracked, not forgotten)

- `ipns-tofu-pin-and-warn-on-change` — TOFU (trust-on-first-use, SSH-host-key style): let the user record a mutable name's current CID as genuine; on a later resolution to a DIFFERENT CID, warn "this name now points somewhere new since you last trusted it". Needs a pin store + a popup flow.
- ENS immutability detection (burned keys / NameWrapper fuses / locked resolver) — would upgrade a provably-locked ENS name from `MutableName` to `ContentVerified`. Its own research/task, not Phase 1.
