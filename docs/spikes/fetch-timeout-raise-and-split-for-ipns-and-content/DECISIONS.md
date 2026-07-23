# Fetch timeout: raise + split for IPNS and content — chosen budgets & rationale

Task: `fetch-timeout-raise-and-split-for-ipns-and-content` (spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`, field finding v0.2.2).

## The problem (re-confirmed against the code)

`ronan.eth` failed its FIRST load with `transport error: timeout: global` then worked on reload. Re-checking the current constants confirmed the premise: `crates/fetcher/src/lib.rs` set one `DEFAULT_GLOBAL_TIMEOUT = 30s` whole-request wall-clock on the single `ureq::Agent` that ALL network I/O flows through (both the IPNS record fetch via `GatewayIpnsRecordSource` and every content CAR fetch via `TrustlessGatewayCarRetriever`). An IPNS load does an EXTRA record round-trip before content, and a cold trustless-gateway CAR fetch of a real multi-block site is slow, so 30s killed a merely-slow-but-progressing first load.

## What was changed

All network timeouts live on the `HttpFetcher` (`crates/fetcher/src/lib.rs`), the one seam every fetch goes through. The existing `DEFAULT_* const + with_*()` override pattern (as `TrustlessGatewayCarRetriever::with_gateway` / `RetrievalBudget::with_max_bytes` use) was extended, NOT a config subsystem.

- Raised `DEFAULT_GLOBAL_TIMEOUT` 30s -> **120s** (the CONTENT-fetch budget). A cold, slow, PROGRESSING multi-block load now completes.
- Kept `DEFAULT_CONNECT_TIMEOUT` at **10s** (deliberately tight). A dead/unreachable host still fails fast on the connect bound, never the raised global bound. Keeping connect tight is precisely what makes it safe to raise the global read budget.
- Added `DEFAULT_IPNS_RECORD_TIMEOUT` = **45s** (the split-out IPNS-RECORD budget). The record fetch is a small single signed-record GET, a distinct step from the content fetch it precedes, so it gets its own budget: above the tight connect bound, below the content budget. It does not need the full 120s content budget, and it must not eat the content step's budget.
- Added `HttpFetcher::with_timeouts(connect, global)` — the override lever. `HttpFetcher::new()` now delegates to it with the two content defaults.

## Wiring

- Content path (`backend.rs`, `werust-ios`, `werust-android`): unchanged `HttpFetcher::new()`, so it picks up the raised 120s content budget automatically.
- IPNS record source (`crates/werust-core/src/lib.rs`, `BrowserShell::with_provider`): now builds its `HttpFetcher` with `HttpFetcher::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_IPNS_RECORD_TIMEOUT)` — same tight connect, the shorter record budget.

## Why these numbers

- **120s content**: a cold gateway serving a real static site issues a separate `dag-scope=entity` CAR fetch PER resource (`docs/adr/0004`, `ipfs-per-resource-car-scope-not-whole-dag`). The SLOWEST single cold CAR fetch (the one that timed out) needs headroom; 120s is a generous-but-bounded ceiling for one progressing GET. It is a wall-clock, NOT a size ceiling — the DAG bytes/blocks budgets (`RetrievalBudget`, 32 MiB / 100k blocks) are UNCHANGED and remain the size safety ceilings.
- **45s record**: an IPNS record is small, but a cold gateway resolving the name (a DHT / routing lookup behind the gateway) can still be slow; 45s covers that while staying well under the content budget so the two steps do not compound into one over-long hang.
- **10s connect (unchanged)**: fast failure for a dead host; the split's whole point is that raising the read budget does NOT slow down a dead-host failure.

All three stay BOUNDED (no unbounded/absent timeout is offered on `with_timeouts`): a hostile/silent host always fails eventually.

## Alternatives considered

- **One raised global for everything (no split).** Simpler, but then the small record fetch would carry the full content budget and a stuck record lookup could hang for the whole content budget before the content step even starts. The split gives each step a budget appropriate to its size.
- **A per-retrieval WHOLE-load wall-clock (record + all content fetches summed).** Rejected as out of scope and heavier: it needs a clock threaded through the retriever + resolver, and under per-resource scope each fetch is already independently bounded. The task asks for per-step budgets, which the `HttpFetcher` timeout already expresses.
- **Making the timeout unbounded / configurable to infinite.** Explicitly refused by the task and the trust stance: a hostile/silent host must fail eventually.

## Coherence check

`with_timeouts` and the two new `DEFAULT_*` consts reuse the crate's established `DEFAULT_* + with_*()` override vocabulary; they introduce no new config concept, status, or flag, and sit at the transport layer where the other timeout constants already live. `DEFAULT_IPNS_RECORD_TIMEOUT` names a genuinely distinct step (the record fetch) rather than re-meaning the existing content timeout.

## Proven by (network-isolated fixtures)

`crates/fetcher/src/lib.rs` tests:

- `the_default_content_budget_is_raised_above_the_old_thirty_seconds` — the constant relationships (content > 30s, connect tight, record between connect and content).
- `a_slow_but_within_budget_fetch_succeeds` — a loopback server that answers slowly (but within budget) still succeeds.
- `a_fetch_that_exceeds_the_global_budget_fails_bounded_not_hangs` — a server slower than the budget is abandoned as a seam error, bounded (asserted via elapsed time), not a hang.
- `a_dead_host_fails_fast_on_the_tight_connect_bound_not_the_raised_budget` — an RFC 5737 documentation-block address (no live-network dependency) fails near the tight connect bound, far under a large global budget.
