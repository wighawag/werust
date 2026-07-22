---
title: review-gate non-blocking nits for 'verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the crate-binding deviation: the task named rs-car-ipfs/rs-car, but the build bound rs-car-sync + ipld-dagpb + quick-protobuf instead. Is this substitution accepted?
  (The named crates are async (futures/async-std) and rs-car-ipfs UnixFS decode is pub(crate), conflicting with the tasks DECIDED sync-seam/no-async constraint. The sync siblings share the repos cid 0.11/ipld-core lineage. Well recorded in docs/adr/0004 + work/notes/observations/car-crate-binding-async-vs-sync-seam-2026-07-22.md. Looks like the correct call; flagged only because the PR commit body carried no Decisions block so it was never surfaced for ratification.)
- Ratify the default fetch scope: the CAR backend GETs dag-scope=all (the whole DAG under the root) rather than the narrower dag-scope=entity + entity-bytes per resource. Accept this bandwidth default for now?
  (fetch_car in crates/fetcher/src/retriever.rs hardcodes ?format=car&dag-scope=all; the docstring flags entity-scoping as a future refinement that does not change the seam. Acceptance only requires ?format=car, so this satisfies the task, but it is a user-visible retrieval default (a large site fetches its whole DAG even for one sub-resource) the task did not specify.)
- Ratify the HAMT lookup strategy: resolve_hamt_entry scans matching shard links by full entry-name suffix and descends pure-hex-prefix links, rather than deriving the exact HAMT bucket from the name hash.
  (Documented in the code as pragmatic and correct-though-not-minimal-read for index.html + relative assets. It is covered by a_hamt_sharded_directory_resolves_index_and_entries. Correct for the in-scope cases; flagged as a non-obvious in-scope simplification for a human to note, not a defect.)
