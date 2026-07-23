---
title: "ipfs:// sub-resource fetch must not re-download the WHOLE site DAG per request: use dag-scope=entity + entity-bytes (fixes slow/partial/timeout loads)"
slug: ipfs-per-resource-car-scope-not-whole-dag
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [1]
---

## What to build

Stop refetching the entire site DAG for every resource request. FIELD FINDING (v0.2.2): real sites load slowly, partially (CSS/assets missing), and sometimes time out on the first try, then a reload works. ROOT CAUSE: the trustless-gateway CAR backend requests `GET <gateway>/ipfs/<cid>?format=car&dag-scope=all` — `dag-scope=all` fetches the WHOLE DAG under the root. Because a browser makes one request for the directory root AND a separate request for each sub-resource (css, js, images), werust ends up fetching + verifying + reassembling the ENTIRE site DAG once PER resource. That is N full-site downloads to render one page, which is slow, wastes bandwidth, and makes individual requests hit the fetch timeout (partial styling, then a warm reload succeeds).

Fix: request only the blocks needed for the SPECIFIC resource, per the Trustless Gateway spec. For a resource at `ipfs://<cid>/<path>`, fetch `?format=car&dag-scope=entity` (only the blocks to verify + read that entity: the file's blocks, or a directory listing), and use `entity-bytes=from:to` for range/large-file reads where useful. The directory-root -> index.html resolution should fetch just what it needs (the directory node + the index entity), not the whole tree. Keep verification intact: every returned block is still checked against its CID; an incomplete CAR is still a failure. Optionally cache/reuse verified blocks across the sub-resource requests of one page load so shared blocks are not refetched (a per-load verified-block cache), but the core fix is the scope.

## Acceptance criteria

- [ ] A resource request fetches only the blocks for THAT resource (`dag-scope=entity`, and `entity-bytes` where applicable), NOT the whole DAG (`dag-scope=all`) each time.
- [ ] A real multi-resource directory site loads fully and reliably on the FIRST try (all CSS/JS/images applied), without the per-resource whole-DAG refetch; no partial-then-reload-fixes-it behaviour.
- [ ] The directory root resolves to index.html by fetching only what it needs, not the entire tree.
- [ ] Verification unchanged: each block still verified against its CID; an incomplete/truncated CAR still fails closed; a mis-hashing block still fails.
- [ ] (If a per-load block cache is added) it is verified-blocks-only and does not weaken verification or leak across unrelated loads; tested.
- [ ] Tests cover per-entity scope (a directory site fixture served as scoped CARs; asserting the request uses dag-scope=entity and only the needed blocks are fetched/verified per resource), network-isolated.

## Blocked by

- None — can start immediately. (Builds on the retriever from `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` in tasks/done.)

## Prompt

> Goal: stop werust refetching the WHOLE site DAG for every sub-resource. Today the CAR backend uses `dag-scope=all`, so each of the many per-page requests (root + every css/js/image) downloads + verifies + reassembles the entire site — slow, partial loads, timeouts, fixed by a warm reload. Switch to per-resource scope: `?format=car&dag-scope=entity` (+ `entity-bytes` where useful) so each request fetches only the blocks for that resource, keeping full verification.
>
> Where to look: `crates/fetcher/src/retriever.rs` (`TrustlessGatewayCarRetriever`, the `?format=car&dag-scope=all` request builder, the per-block verify + UnixFS reassemble/traverse) and `crates/werust-core/src/ipfs.rs` (`resolve_ipfs_request` passes cid+path). The Trustless Gateway spec (`dag-scope=entity`, `entity-bytes`) is the mechanism (already referenced in the task's ADR-0004). Consider a per-load verified-block cache so shared blocks are not refetched across a page's resources, but the scope change is the core fix.
>
> Done = each resource fetches only its own blocks (dag-scope=entity), a real site loads fully and reliably on the first try, verification/fail-closed unchanged, proven with scoped-CAR fixtures. FIRST re-check the retriever still uses dag-scope=all. RECORD the scope decision (and any cache) durably.
