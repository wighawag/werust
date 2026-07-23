---
title: review-gate non-blocking nits for 'ipfs-per-resource-car-scope-not-whole-dag' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: ipfs-per-resource-car-scope-not-whole-dag
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ipfs-per-resource-car-scope-not-whole-dag' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify Decision 3: entity-bytes is NOT used yet (dag-scope=entity only, whole-entity reads). The task TITLE says 'dag-scope=entity + entity-bytes', but acceptance qualifies entity-bytes as 'where applicable/useful'. The seam retrieve(cid,path) has no byte-range path, so entity-bytes=0:* would equal plain dag-scope=entity and adding it now is dead code. Named as a follow-on (large-file/range read reusing the same fetch_car builder). Reasonable to ratify.
  (docs/spikes/ipfs-per-resource-car-scope-not-whole-dag/DECISIONS.md Decision 3; retriever.rs fetch_car builds no entity-bytes param)
- Ratify Decision 4: NO cross-load verified-block cache added (deferred). The optional per-load cache is explicitly deferred because the retriever is a single long-lived instance with no page-load boundary, so a correct cache needs a lifetime the seam does not express; a naive cross-load cache would grow unbounded. Within one directory-root retrieval the two scoped fetches DO share one store. Task marks the cache optional and the scope change alone removes the N-whole-DAG pathology, so this is a sound deferral with a named follow-on.
  (DECISIONS.md Decision 4; retrieve() uses a fresh per-retrieval CarBlockStore)
