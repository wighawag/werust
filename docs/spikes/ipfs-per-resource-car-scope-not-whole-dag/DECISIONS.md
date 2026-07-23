# Decisions: per-resource CAR scope, not whole-DAG-per-request (`ipfs-per-resource-car-scope-not-whole-dag`)

Durable record of the load-bearing design choices this task made, per the runner's decision-bar rule ("RECORD the scope decision (and any cache) durably"). Linked from the task done-record. Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`; builds on `verifiable-ipfs-content-retrieval-seam-and-gateway-car-backend` (the `ContentRetriever` seam + `TrustlessGatewayCarRetriever`); ADR: `docs/adr/0004`.

## Reality re-check (before building)

- FIRST re-check per the prompt: the retriever DID still use `dag-scope=all`. `crates/fetcher/src/retriever.rs`'s `fetch_car` built `GET <gateway>/ipfs/<cid>?format=car&dag-scope=all` and IGNORED the resource path (the whole tree was fetched, then path-resolved locally). So the task premise held.
- The retriever is a SINGLE long-lived instance built once per session in each `install_ipfs` site (`crates/webview-renderer/src/backend.rs`, `crates/werust-ios/rust/src/lib.rs`, `crates/werust-android/rust/src/lib.rs`), shared across every `ipfs://` request (root + each sub-resource). The load path (`werust_core::ipfs::resolve_ipfs_request`) already passes the CID AND the path to `retrieve(cid, path)`; the path was just unused at the fetch layer.
- ADR-0004 already anticipated this: its decision (2) reads "GETs `<gateway>/ipfs/{cid}?format=car` (with `dag-scope`/`entity-bytes` so it fetches only the blocks a resource needs)". This task lands that narrowing; the ADR needed no change and the seam surface (`ContentRetriever::retrieve`) is unchanged.
- No drift found; the build proceeded.

## Decision 1 — the scope change: `dag-scope=entity` with the path in the URL

**Chosen:** `fetch_car` now builds `GET <gateway>/ipfs/<cid>[/<path>]?format=car&dag-scope=entity`. The requested resource PATH goes in the URL (percent-encoded per segment), and `dag-scope=entity` narrows the CAR to only the blocks needed to traverse each path segment plus the terminating entity (a file's blocks, or a directory's listing), per the Trustless Gateway spec / IPIP-0402. The client still walks + verifies + reassembles locally exactly as before, over whatever verified blocks the scoped CAR delivered.

**Why:** A browser makes one request for the directory root AND a separate request for every sub-resource (css/js/images). Under `dag-scope=all` each of those requests re-downloaded + re-verified + re-reassembled the ENTIRE site DAG (N full-site downloads to render one page), which is what caused the slow/partial/timeout-then-warm-reload field finding (v0.2.2). `dag-scope=entity` makes each request pull only that resource's blocks. Verification is unchanged (every block is still hash-checked; an incomplete scoped CAR still fails closed as `MissingBlock`/`IncompleteCar`).

**Touches:** the request URL shape only (the seam and every caller are unchanged). The three `install_ipfs` sites and the off-thread wrapper needed NO change. Fixtures that served a whole-DAG CAR still resolve (the scoped fetches are a superset-tolerant merge), so existing tests kept passing.

## Decision 2 — directory root -> index.html is a SECOND entity-scoped fetch, not a whole-tree fetch

**Chosen:** When the walk lands on a directory (a bare `ipfs://<cid>` root or a `.../` directory path), werust resolves its `index.html` by issuing a SECOND `dag-scope=entity` fetch for `<cid>/<path>/index.html` and merging those verified blocks into the same per-retrieval `CarBlockStore` before reassembling. The directory's own scoped CAR carries only its listing (that is what `dag-scope=entity` returns for a directory), so the index entity's blocks must be fetched explicitly.

**Why:** With per-resource scope the gateway no longer hands us the whole tree, so the previously-local "read index.html out of the already-fetched DAG" step now needs the index entity's blocks fetched on their own. This is exactly the acceptance criterion "the directory root resolves to index.html by fetching only what it needs, not the entire tree." The merge is safe because blocks are content-addressed: the directory node re-appears in both scoped CARs and is an idempotent overwrite of identical verified bytes.

**Alternative considered:** appending `/index.html` to the FIRST fetch unconditionally (rejected: we do not know the terminus is a directory vs a file until we read it; a file path must NOT get an `/index.html` suffix, and the CID codec does not distinguish directory from file). Walking first, then fetching the index entity only when the terminus is a directory, is the minimal-read correct path.

**Touches:** the retrieval budget is now enforced against the CUMULATIVE blocks across BOTH scoped fetches (`CarBlockStore` tracks running `total_bytes`/`block_count`), so a hostile gateway cannot evade the ceiling by splitting a runaway DAG across the two fetches.

## Decision 3 — `entity-bytes` is NOT used yet (whole-entity read only)

**Chosen:** The request uses `dag-scope=entity` WITHOUT `entity-bytes`. The seam's `retrieve(cid, path)` returns the FULL reassembled bytes of a resource (there is no HTTP-Range / partial-read path through the seam today), and `dag-scope=entity` already fetches exactly the whole entity, so `entity-bytes` (a trustless Range equivalent) has no applicable use case yet. `entity-bytes=0:*` would be equivalent to plain `dag-scope=entity` for a whole read.

**Why:** The acceptance criterion qualifies `entity-bytes` as "where applicable / where useful". It becomes useful only once the seam grows a byte-range read (e.g. streaming a large video, or a Range request from the webview). Adding it now with no range to serve would be dead code.

**Touches:** a future range-read task (large-file / video streaming) is the natural home for `entity-bytes=from:to`; the `fetch_car` builder is the single site it would extend. Recorded so that task reuses this builder rather than forking a second request path.

## Decision 4 — NO cross-load verified-block cache added (deferred, not built)

**Chosen:** No persistent verified-block cache was added. Each `retrieve` call uses a fresh per-retrieval `CarBlockStore`; within a single directory-root retrieval its two scoped fetches DO share one store (so the directory node is verified once), but nothing is retained across separate `retrieve` calls / resources / page loads.

**Why:** The task marks the cache explicitly OPTIONAL ("the core fix is the scope"). The scope change alone removes the N-whole-DAG-downloads pathology, which is the whole field-finding fix. A cache shared across a page's sub-resources would save re-fetching shared blocks (e.g. the root directory node fetched once per sub-resource), but the retriever is a SINGLE long-lived instance with no notion of "a page load" boundary, so a correct per-load cache would need a load-scoped lifetime the seam does not currently express, and a naive cross-load cache on the shared instance would grow unbounded over a session (a memory-growth surprise). Rather than introduce an unbounded cross-load cache or a new load-boundary concept under this task, the cache is deferred. Because blocks are content-addressed and every cached entry would be a verified block, a future cache cannot weaken verification; it is purely an optimization.

**Alternatives considered:** (a) an unbounded `Mutex<HashMap<Cid, Vec<u8>>>` on the shared retriever (rejected: unbounded session-lifetime memory growth, a user-visible surprise for a task whose core deliverable is the scope fix); (b) a bounded LRU verified-block cache (viable and safe, but it introduces a new tunable — cache size — and a new concept "verified-block cache" whose right layer/lifetime is a design question better sized as its own follow-on). 

**Touches:** a follow-on "per-load verified-block cache" task would decide the cache lifetime (per-load vs bounded-session LRU) and its size ceiling, and would live behind the same `ContentRetriever` seam. Recorded here so that task starts from this reasoning rather than re-deriving it.

## Verification / fail-closed unchanged (checked)

- Every returned block is still hash-verified by `rs-car-sync` (`validate_block_hash = true`) as each scoped CAR is parsed, and again as the walk consumes it. A mis-hashing block is still a distinct `BlockHashMismatch` (tamper, never served).
- An incomplete/truncated scoped CAR is still `IncompleteCar`; a scoped CAR that omits a block the resource needs is still `MissingBlock` (proven by the new `a_scoped_car_missing_the_resource_block_fails_closed` test). A directory with no `index.html` is still `PathNotFound`.
- The retrieval budget (bytes/blocks) now bounds the cumulative retrieval across the (at most two) scoped fetches.

## Tests (network-isolated, in `crates/fetcher/src/retriever.rs`)

- `the_request_scopes_to_the_entity_not_the_whole_dag` — the request uses `dag-scope=entity`, never `dag-scope=all`.
- `a_sub_resource_request_puts_its_path_in_the_url_scoped_to_that_entity` — the resource path is in the URL and a scoped CAR containing only that resource's blocks (not siblings') resolves it, in exactly one fetch.
- `a_directory_root_resolves_index_html_by_fetching_only_what_it_needs` — the root does a directory fetch then an `index.html` fetch, and NEVER fetches a heavy sibling that exists in the tree (proves no whole-tree fetch).
- `a_scoped_car_missing_the_resource_block_fails_closed` — an incomplete scoped entity fails closed as `MissingBlock`.
- All pre-existing retriever/ipfs tests (tamper, missing-block, truncated-CAR, budget, path-not-found, HAMT, chunked-file) stay green: verification and fail-closed behaviour are unchanged.
