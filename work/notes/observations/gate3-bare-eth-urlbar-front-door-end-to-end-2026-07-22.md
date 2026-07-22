---
title: "Gate-3 conductor review: bare-eth-urlbar-front-door-end-to-end (APPROVE — tracer bullet closed)"
date: 2026-07-22
status: open
reviewOf: bare-eth-urlbar-front-door-end-to-end
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: e99e480
---

## Verdict: APPROVE ✅ — merged to origin/main as e99e480 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green). This CLOSES the Phase-1 tracer bullet.

Conductor's own diff-vs-acceptance pass over the landed diff on origin/main. This is the end-to-end slice: a bare `ronan.eth` in the URL bar resolves and renders the immutable IPFS site over the trusted-RPC path, honestly labelled.

## Acceptance criteria — all met

- A bare `*.eth` URL-bar entry (no scheme, non-empty label before `.eth`, no interior `/`, case-insensitive, tolerating one trailing `/`) is recognised as an ENS name (`eth_name_from_entry`) and routed to ENS resolution — NOT treated as a literal host. It deliberately does NOT hijack `ipfs://…`/`https://….eth` (a scheme present => not a bare name).
- An `ipfs-ns` name resolves and renders through the EXISTING verified `ipfs://` render path: the front door feeds the resolved `ipfs://<cid>` into the seam's scheme handler that hash-verifies the bytes — verification is NOT re-implemented.
- The address bar keeps the `.eth` name (a `display_url` distinct from the underlying `ipfs://<cid>` load); NO `https://` rewrite, NO gateway redirect.
- Trust posture ends in "content-verified, name via trusted RPC" (`NameViaTrustedRpc`), NEVER "verified"/`ContentVerified`.
- A plain (non-ENS) `ipfs://` load still shows `ContentVerified`; a served load still shows the unverified posture — the ENS posture does not leak onto them.
- An unsupported contenthash (ipns-ns/swarm-ns/arweave/unknown) fails the load with the decoder's distinct protocol-named reason; never defaulted to `ipfs://`.
- Fail-closed on every failure path (no/invalid/unsupported contenthash, resolution error) with a legible chrome reason; nothing unverified is rendered.
- End-to-end tests are network-isolated (pinned RPC + contenthash + content fixtures) and cover the posture outcome BOTH directions.

## The posture-marking clash — RESOLVED CORRECTLY (the load-bearing trap)

The webview backend's `install_ipfs` scheme handler UNCONDITIONALLY calls `mark_content_verified()` on any successful verified `ipfs://` resolution, and the shell reads posture from the backend. A naive `navigate("ipfs://<cid>")` would therefore render the ENS page as plain `ContentVerified`, dropping the new posture. The task resolves this with a clean signal-before-mark mechanism:

- The front door calls `Renderer::mark_ens_origin()` on the resolved load BEFORE feeding the CID into the verified path.
- The scheme handler's unconditional `mark_content_verified()` then CHECKS the ens-origin flag and surfaces `NameViaTrustedRpc` instead of plain `ContentVerified` — the ENS-origin posture WINS over the unconditional content-verified mark.
- It stays driven by the REAL load path (only an actual ENS-resolved verified load is flagged) and does NOT leak: a fresh navigation resets ens-origin/posture to untrusted. Proven both directions in tests (`a_plain_ipfs_load_stays_content_verified_and_the_ens_posture_does_not_leak`, `the_name_via_trusted_rpc_posture_does_not_leak_into_a_later_served_load`).

## HONESTY REQUIREMENT — CONFIRMED

The successful ENS render ends in `NameViaTrustedRpc`; the code + docs never surface it as "verified"/"name-verified" (a grep found only the doc-comment FORBIDDING the mislabel). Phase 1 makes no name-verification claim, exactly as required.

## Drift / forward-notes honoured

- Task's READ-FIRST premise ("re-check the resolution API, the new posture variant + its wiring hook, and the ipfs render path's `mark_content_verified` behaviour") honoured: the diff builds directly on `ens::resolve`, `TrustPosture::NameViaTrustedRpc`, and the still-unconditional `mark_content_verified`. Conductor's own pre-dispatch freshness check confirmed all three before dispatch.

## Gate-2 nits (non-blocking, already recorded)

Two non-blocking nits in `review-nits-bare-eth-urlbar-front-door-end-to-end-2026-07-22.md`, left open for human triage. None block integration; none require a re-task.
