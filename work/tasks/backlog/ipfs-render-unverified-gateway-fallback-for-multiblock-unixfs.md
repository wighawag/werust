---
title: "ipfs:// renders multi-block UnixFS sites via gateway under the honest ⚠ unverified posture (skip byte-verification for now, never claim verified)"
slug: ipfs-render-unverified-gateway-fallback-for-multiblock-unixfs
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Make a REAL `.eth` UnixFS site actually render on desktop today, without lying about trust. Right now the verified `ipfs://` path re-hashes the whole gateway response against the CID and fails `HashMismatch` for every real (multi-block) site, because a CIDv1 `dag-pb`/UnixFS CID names the DAG's ROOT BLOCK, not the reassembled payload. Single-block `raw`/leaf CIDs still hash directly and MUST keep rendering as `ContentVerified` exactly as today.

The fix for now is an HONEST fallback, NOT a relaxed verification claim:
- A CID whose bytes hash directly to it (single-block `raw`/leaf, `sha2-256`) continues to render as `ContentVerified` (unchanged).
- A CID that cannot be byte-verified this way (a multi-block `dag-pb`/UnixFS DAG) is fetched from the gateway and rendered under the EXISTING unverified/served posture — the same one an `http(s)://` page gets, surfaced with the `⚠` warning indicator. Its bytes were NOT hash-verified, so it is NEVER labelled "content-verified"/"verified". If it was reached via an ENS `.eth` resolution, its name still came from the trusted RPC, so the honest label is the served/unverified posture (with the `⚠` warning), NOT `NameViaTrustedRpc` (which asserts content-verification of the bytes) and NEVER "verified".

The real fix (verifiable DAG/CAR retrieval that lets a multi-block site be legitimately `ContentVerified`) is deliberately OUT OF SCOPE here and is tracked by its own exploration task (`explore-trustless-verifiable-ipfs-dag-car-retrieval`). This task is the honest interim: render the site, warn clearly that it is unverified, never claim otherwise.

## Acceptance criteria

- [ ] A single-block `raw`/leaf `sha2-256` CID still renders as `ContentVerified` (no regression to the existing verified path).
- [ ] A multi-block `dag-pb`/UnixFS CID (the real-site case, e.g. `mandalas.eth`'s `bafybei...`) renders its content via the gateway instead of failing `HashMismatch`.
- [ ] That gateway-rendered multi-block page shows the EXISTING unverified/served posture with the `⚠` warning indicator (like an `http(s)://` page); it is NEVER labelled "content-verified"/"verified"/"name via trusted RPC".
- [ ] Trust honesty holds: no code path calls a non-byte-verified page "verified". The distinction between "verified single-block" and "unverified gateway-served multi-block" is driven by the ACTUAL load path (whether the bytes verified), not the URL string.
- [ ] Fail-closed is preserved for genuine errors (gateway transport failure, an unusable/malformed CID): those still fail the load with a legible reason, distinct from the "served-but-unverified" success.
- [ ] Tests cover both branches offline (a single-block CID -> verified; a multi-block/dag-pb CID -> gateway-served under the ⚠ unverified posture; an error -> fail-closed), network-isolated (loopback gateway fixture, no live network), mirroring the repo's `fetcher`/`ipfs` test style. A multi-block fixture that mirrors a real UnixFS layout (so the old "single-block only" coverage gap is closed) is included.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: let a REAL `.eth` UnixFS site render on desktop today by fetching it from the gateway and labelling it HONESTLY as unverified (the `⚠` served posture, like http), instead of the current `HashMismatch` failure on every real site. Do NOT weaken the meaning of "content-verified": single-block CIDs that genuinely hash to their bytes stay `ContentVerified`; multi-block UnixFS DAGs (which the current verifier cannot check) render served-but-unverified with the warning.
>
> Domain vocabulary: a CIDv1 `dag-pb` (codec 0x70) CID is a UnixFS DAG whose root block LINKS to child blocks; the gateway returns the reassembled content, whose bytes do NOT hash to the root CID (this is the desktop `HashMismatch`). Only a single-block `raw` (0x55) `sha2-256` CID hashes directly. The trust posture is a product surface (`docs/adr/0001`): "content-verified" MUST mean the bytes were checked against the hash. So a page we could not byte-verify is the unverified/served posture (`⚠ unverified origin`), never "verified".
>
> Where to look: the verifier is in the `fetcher` crate (`VerifyingContentFetcher` re-hashes with SHA-256 and returns `HashMismatch`); the desktop `ipfs://` handler `install_ipfs` (webview backend, `register_uri_scheme`) routes through `werust_core::ipfs::resolve_ipfs_request` over `GatewayContentSource` and marks the load on success. The three postures live in `TrustPosture` (renderer crate): `ContentVerified`, `NameViaTrustedRpc`, `UnverifiedOrigin` (the `⚠` served state, threaded by `trust-indicator-verified-vs-served` in `work/tasks/done/`). Route the un-verifiable-multi-block case to `UnverifiedOrigin` (served, warned), keeping `ContentVerified` for the single-block verified case. Preserve fail-closed for real errors.
>
> The real fix (walk+verify the DAG so a multi-block site can be LEGITIMATELY content-verified) is OUT OF SCOPE and tracked by `explore-trustless-verifiable-ipfs-dag-car-retrieval` — reference it in the decision record so the interim honesty is clearly interim.
>
> Done = single-block CIDs still render verified; a real multi-block UnixFS `.eth` site renders via the gateway under the honest `⚠` unverified posture (never "verified"); errors still fail closed; all proven offline including a multi-block fixture. FIRST re-check current reality (the verifier's codec support, the posture plumbing from the Phase-1 done tasks) and route to needs-attention if a premise drifted. RECORD the interim-honesty decision (why served-not-verified for multi-block, and its link to the exploration task) durably — it likely meets the ADR gate (a security-relevant trust trade-off), so prefer an ADR in `docs/adr/`.
