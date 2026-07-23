---
title: "Gate-3 conductor review: fetch-timeout-raise-and-split-for-ipns-and-content (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: fetch-timeout-raise-and-split-for-ipns-and-content
gate: gate-3-conductor
mergedCommit: f3b06fa
---

## Verdict: APPROVE

Conductor Gate-3 diff-vs-criteria pass. Gate-1 (cargo fmt/clippy/build/test) and Gate-2 (code review) passed before merge. Driven in place from `work/tasks/backlog/` via `dorfl do ... --allow-backlog --isolated --review --merge`.

## Done-move + landing

- `work/tasks/backlog/fetch-timeout-raise-and-split-for-ipns-and-content.md` -> `work/tasks/done/` confirmed on origin/main (squash merge `f3b06fa`).
- Files: `crates/fetcher/src/lib.rs` (+261/-…), `crates/werust-core/src/lib.rs` (+12, the IPNS-record wiring), new `docs/spikes/.../DECISIONS.md` (+48), gate-2 nits note.

## Acceptance criteria (ticked against the diff)

- [x] Cold IPNS load has a realistic budget, no spurious global timeout when merely slow. Content budget `DEFAULT_GLOBAL_TIMEOUT` raised 30s -> 120s; IPNS record fetch split to its own `DEFAULT_IPNS_RECORD_TIMEOUT` = 45s. Record step + content step each get an appropriate budget rather than sharing one 30s wall-clock. Test `a_slow_but_within_budget_fetch_succeeds`.
- [x] Connect stays tight; global raised; record/content budgets appropriate to each step. `DEFAULT_CONNECT_TIMEOUT` unchanged at 10s (a dead host still fails fast on connect, proven by `a_dead_host_fails_fast_on_the_tight_connect_bound_not_the_raised_budget` using an RFC 5737 doc-block address, no live-network dep). 45s record sits above connect, below content.
- [x] Timeouts stay BOUNDED + overridable via the const + constructor pattern; DAG size/block budgets unchanged. New `HttpFetcher::with_timeouts(connect, global)` extends the established `DEFAULT_* + with_*()` vocabulary (no config subsystem). `RetrievalBudget` (32 MiB / 100k blocks) untouched. `a_fetch_that_exceeds_the_global_budget_fails_bounded_not_hangs` asserts boundedness via elapsed time. Const-relationship invariants asserted in `the_default_content_budget_is_raised_above_the_old_thirty_seconds`.
- [x] Tests cover the timeout values/behaviour, network-isolated. 4 loopback/doc-block-address tests; no live network.

## Wiring verified

- Content path (`backend.rs` / `werust-ios` / `werust-android`) uses `HttpFetcher::new()` -> auto picks up the raised 120s content budget. Untouched.
- IPNS record source (`werust-core` `BrowserShell::with_provider`) now builds its fetcher with `HttpFetcher::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_IPNS_RECORD_TIMEOUT)` — same tight connect, shorter 45s record budget. Correct split.

## Forward-notes / drift honoured

Task carried the "best landed alongside/after ipfs-per-resource-car-scope" note; T1 landed first (correct order). DAG-size budgets explicitly left unchanged per the task's "do NOT weaken the budget-based DAG ceilings". No drift.

## Review-nits triage (Gate-2)

1. Widened `pub const DEFAULT_*` + cross-crate import into werust-core to wire the split. Intended and recorded. RATIFIED — benign public-surface widening at the transport layer.
2. `crates/werust-core/src/ethereum.rs` keeps its OWN private `DEFAULT_GLOBAL_TIMEOUT = 30s` for the RPC transport, deliberately NOT raised. This is an ADJACENT surface the task did not cover (task scope = the fetcher/gateway path, which was the actual field finding). NOT a regression and correctly out of scope, but flagged here as a genuine off-path observation: a slow ENS RPC read on the SAME cold-network condition could itself hit that 30s and spuriously time out the ENS-resolve step BEFORE content fetch even starts. Worth a human decision on whether the RPC transport wants the same raise/split treatment (candidate follow-on). Not blocking this task.

## Net effect

A merely-slow-but-progressing cold IPNS/content load is no longer killed at 30s; a dead host still fails fast. Combined with T1 (per-resource scope removing the whole-DAG cost), the v0.2.2 "first load times out, reload works" pathology should be resolved at both its root (excess work) and its symptom (too-tight budget). One off-path follow-on candidate captured: the RPC-transport 30s in ethereum.rs.
