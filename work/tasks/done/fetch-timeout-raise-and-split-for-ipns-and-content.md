---
title: "Raise/split the fetch timeout so IPNS + content retrieval does not spuriously time out on the first load"
slug: fetch-timeout-raise-and-split-for-ipns-and-content
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Give retrieval a realistic time budget so a cold load does not fail with `transport error: timeout: global`. FIELD FINDING (v0.2.2): `ronan.eth` failed the first load with the fetcher's 30s `DEFAULT_GLOBAL_TIMEOUT`, then a reload worked. An IPNS load does MORE work than a plain ipfs load (fetch+verify the IPNS record, THEN fetch the content), and a cold gateway fetch can be slow, so 30s global for the whole thing is too tight — especially compounded with the whole-DAG-per-request issue (see `ipfs-per-resource-car-scope-not-whole-dag`).

Raise and, where it makes sense, SPLIT the budget so a legitimate slow-but-progressing load is not killed: a larger overall content-fetch timeout; a separate, appropriately-bounded timeout for the IPNS record fetch vs the content fetch (they are distinct steps); keep the connect timeout tight (a dead host should still fail fast). The values should be overridable (mirroring the existing `DEFAULT_*` const + constructor pattern). Do NOT make timeouts unbounded (a hostile/silent host must still fail eventually) and do NOT weaken the budget-based DAG ceilings (bytes/blocks) — this is about wall-clock for a progressing fetch, not removing limits.

## Acceptance criteria

- [ ] A cold IPNS load (record fetch + content fetch) has a realistic time budget and does not spuriously fail with a global timeout on the first try when the network is merely slow (not dead).
- [ ] The connect timeout stays tight (a dead/unreachable host fails fast); the global/read budget is raised to a sensible value; the record-fetch and content-fetch budgets are appropriate to each step.
- [ ] Timeouts remain BOUNDED (no infinite hang) and overridable via the existing const + constructor pattern; the DAG size/block budgets are unchanged.
- [ ] Tests cover the timeout values/behaviour (a slow-but-within-budget fixture succeeds; a dead host still fails fast; the budget is still bounded), network-isolated.

## Blocked by

- None — can start immediately. (Best landed alongside / after `ipfs-per-resource-car-scope-not-whole-dag`, which removes the whole-DAG-per-request cost that makes the timeout bite; but the timeout margin is worth having regardless.)

## Prompt

> Goal: stop cold IPNS/content loads from spuriously hitting `timeout: global`. ronan.eth failed the first load on the 30s global timeout then worked on reload; IPNS does an extra record round-trip and cold gateway fetches are slow. Raise the global/read budget, split the IPNS-record vs content budgets where sensible, keep connect tight and everything BOUNDED + overridable.
>
> Where to look: `crates/fetcher/src/lib.rs` (`DEFAULT_CONNECT_TIMEOUT` 10s, `DEFAULT_GLOBAL_TIMEOUT` 30s on the `ureq::Agent`), the IPNS record source `crates/werust-core/src/ipns.rs` and the content retriever `crates/fetcher/src/retriever.rs`. Use the existing `DEFAULT_*` const + constructor-override pattern. Do NOT remove the DAG bytes/blocks budgets (those are the safety ceilings); this is wall-clock for a progressing fetch.
>
> Done = a cold IPNS load has a realistic bounded budget and does not spuriously time out, a dead host still fails fast, values overridable, proven with fixtures. FIRST re-check the current timeout constants. RECORD the chosen budgets + rationale durably.
