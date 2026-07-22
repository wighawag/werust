---
title: "Gate-3 conductor review: verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend (APPROVE)"
date: 2026-07-22
status: open
reviewOf: verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 75456a2
---

## Verdict: APPROVE ✅ — merged to origin/main as 75456a2 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

This is the fix for the `mandalas.eth` HashMismatch: real multi-block UnixFS directory sites now render legitimately content-verified via trustless-gateway CAR retrieval.

## Recovery note (environment)

The first two isolated dispatches failed at infra level, not build level: (1) the spawned agent could not find `git` (`spawnSync git ENOENT`) because the child process PATH lacked `/usr/bin`; fixed by exporting an explicit PATH (`/usr/bin:/bin:/usr/local/bin:$HOME/.cargo/bin:$HOME/.volta/bin`) alongside `BASH_ENV` on the dispatch. (2) A stale job worktree registration from the crashed attempt blocked the next worktree setup; fixed with `git worktree prune` in the mirror + `rm -rf` of the stale dir. Each stranded claim was released via `dorfl requeue` (no work branch had been pushed, so nothing lost). The clean re-dispatch built + merged.

## Acceptance criteria — all met

- `ContentRetriever` seam (CID + path -> verified bytes or typed failure) with a default trustless-gateway CAR backend (`GET ?format=car`, verify each block). No IPFS node.
- Codec-gated: RAW_CODEC 0x55 / DAG_PB_CODEC 0x70. A raw block that does not hash to its CID is `BlockHashMismatch` and is NEVER served (`a_raw_block_that_does_not_hash_to_its_cid_is_a_hard_tamper_failure_never_served`) — the security-critical requirement.
- Real multi-block directory renders: index.html resolution + per-sub-resource path resolution + HAMT-sharded directories + chunked-file reassembly, all fully hash-verified (`a_real_multi_block_directory_site_renders_index_and_sub_resources_all_verified`, `a_hamt_sharded_directory_resolves_index_and_entries`, `a_chunked_file_reassembles_in_link_order`).
- Fail-closed + budget: distinct typed failures for mis-hashing block, missing linked block, incomplete/truncated CAR (the Trustless Gateway client obligation), unresolved path, directory-without-index, unsupported codec, invalid CID, transport error. DEFAULT_MAX_BYTES 32MB + DEFAULT_MAX_BLOCKS 100k, both overridable; budget refusal tested for both a runaway-byte and a fan-out DAG.
- Vetted crates: `rs-car-sync` + `ipld-dagpb` + `cid 0.11` (single CID lineage at the trust edge); no hand-rolled CAR/dag-pb/crypto (`docs/adr/0001`).
- Network-isolated (loopback gateway serving a canned CAR; `the_request_is_a_format_car_get` asserts the request shape); mirrors the fetcher/ipfs test style.

## In-scope decisions recorded (ADR-0004)

The decision record `docs/adr/0004-verifiable-ipfs-content-retrieval-seam-and-trustless-gateway-car-backend.md` captures the seam shape, the codec+budget trust model, and a notable swap: the task named `rs-car`/`rs-car-ipfs`, but those are async-only; the agent bound the SYNCHRONOUS `rs-car-sync` to keep the seam off the async runtime (the task's explicit constraint). This is exactly the record-non-obvious-decisions discipline; the swap is sound and keeps Phase-1 sync.

## Drift / forward-notes honoured

The task's READ-FIRST premises (single-block/ignore-path `resolve_ipfs_request`, raw-only `VerifyingContentFetcher`, `DEFAULT_IPFS_GATEWAY` const + `with_gateway`) were re-confirmed by the conductor pre-dispatch and correctly extended (path-aware DAG resolution replaces single-block/ignore-path).

## Gate-2 nits (non-blocking, already recorded)

Three non-blocking nits in `review-nits-verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend-2026-07-22.md`, left open for human triage. None block integration; none require a re-task.

## Unlocks

`retrieval-backend-user-setting` and `ipns-name-resolution-and-render` (both `blockedBy` this seam) are now unblocked.
