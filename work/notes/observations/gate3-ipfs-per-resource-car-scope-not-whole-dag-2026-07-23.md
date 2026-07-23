---
title: "Gate-3 conductor review: ipfs-per-resource-car-scope-not-whole-dag (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: ipfs-per-resource-car-scope-not-whole-dag
gate: gate-3-conductor
mergedCommit: cc4124c
---

## Verdict: APPROVE

Third-review (conductor Gate-3) diff-vs-criteria pass over the landed work for `ipfs-per-resource-car-scope-not-whole-dag`. Gate-1 (cargo fmt/clippy/build/test) and Gate-2 (PR/code review) both passed before merge; this note is the conductor's own judgement pass against the task's acceptance criteria. Driven in place from `work/tasks/backlog/` via `dorfl do ... --allow-backlog --isolated --review --merge`.

## Done-move + landing

- `work/tasks/backlog/ipfs-per-resource-car-scope-not-whole-dag.md` -> `work/tasks/done/` confirmed on origin/main (squash merge `cc4124c`).
- Files changed: `crates/fetcher/src/retriever.rs` (+385/-24), a new `docs/spikes/.../DECISIONS.md` (+60), the gate-2 review-nits note (+17).

## Acceptance criteria (each ticked against the diff)

- [x] A resource request fetches only THAT resource's blocks (`dag-scope=entity`, path in URL), never `dag-scope=all`. `fetch_car` now builds `GET <gateway>/ipfs/<cid>[/<path>]?format=car&dag-scope=entity` with the path percent-encoded per segment. Test `the_request_scopes_to_the_entity_not_the_whole_dag` asserts the URL is entity-scoped and never `dag-scope=all`.
- [x] Multi-resource directory site loads its sub-resources scoped, no whole-DAG-per-request. Each `retrieve(cid, path)` pulls only that entity's blocks; `a_sub_resource_request_puts_its_path_in_the_url_scoped_to_that_entity` proves a scoped CAR containing only that resource's blocks resolves it in exactly one fetch. (First-try full-render is a runtime/field property; the code-level cause of the partial-then-reload pathology is removed.)
- [x] Directory root resolves index.html by fetching only what it needs. Decision 2: on landing at a directory, a SECOND `dag-scope=entity` fetch for `<cid>/<path>/index.html` merges into the same per-retrieval `CarBlockStore`. Test `a_directory_root_resolves_index_html_by_fetching_only_what_it_needs` proves the root NEVER fetches a heavy sibling that exists in the tree.
- [x] Verification unchanged, fail-closed intact. Every block still hash-verified (`validate_block_hash`), `BlockHashMismatch`/`IncompleteCar`/`MissingBlock`/`PathNotFound` preserved; the byte/block budget now bounds the CUMULATIVE across the (at most two) scoped fetches so a split-DAG gateway cannot evade the ceiling. New test `a_scoped_car_missing_the_resource_block_fails_closed`.
- [x] (Optional) per-load block cache: correctly DEFERRED (Decision 4) with a named follow-on. The task marks the cache optional ("the core fix is the scope"); a cross-load cache on the single long-lived retriever would need a load-boundary lifetime the seam does not express, and a naive one would grow unbounded. Within one directory-root retrieval the two scoped fetches already share one store. Sound deferral, not a gap.
- [x] Tests cover per-entity scope, network-isolated. 4 new in-crate tests + all pre-existing tamper/missing-block/truncated/budget/HAMT/chunked tests still green.

## Forward-notes / drift honoured

Task carried no forward-pointer or must-fix-before-consume note; the drift-check block (confirm `dag-scope=all` still present) was honoured and recorded in DECISIONS.md "Reality re-check". ADR-0004 already anticipated the narrowing, so no ADR change was needed and the `ContentRetriever::retrieve` seam surface is unchanged (all three `install_ipfs` sites + the off-thread wrapper untouched). No drift.

## Review-nits triage (Gate-2, work/notes/observations/review-nits-...)

Two non-blocking nits, both "ratify a documented deferral":
1. Ratify Decision 3 (entity-bytes not used yet: dag-scope=entity gives the whole entity; entity-bytes=0:* would be equivalent; the seam has no byte-range read, so adding it now is dead code). RATIFIED - a named follow-on (large-file/range read reusing `fetch_car`).
2. Ratify Decision 4 (no cross-load verified-block cache; deferred as above). RATIFIED - a named follow-on.
Both are benign and correctly reasoned; neither blocks. Left in their durable observation home for a future human promote/keep/delete pass.

## Net effect

The whole-DAG-per-resource refetch (root cause of the v0.2.2 slow/partial/timeout-then-warm-reload field finding) is removed at the fetch layer. This is also the cost-remover that makes the `fetch-timeout-raise-and-split-for-ipns-and-content` margin bite less; that task lands next.
