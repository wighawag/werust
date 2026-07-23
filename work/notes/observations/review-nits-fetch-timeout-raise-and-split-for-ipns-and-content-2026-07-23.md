---
title: review-gate non-blocking nits for 'fetch-timeout-raise-and-split-for-ipns-and-content' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: fetch-timeout-raise-and-split-for-ipns-and-content
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fetch-timeout-raise-and-split-for-ipns-and-content' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The two new consts and DEFAULT_CONNECT_TIMEOUT were widened to pub and are now imported cross-crate into werust-core to wire the IPNS split. Intended and recorded in DECISIONS.md, but ratify the widened public surface of the fetcher crate.
  (crates/fetcher/src/lib.rs pub const DEFAULT_*; crates/werust-core/src/lib.rs use fetcher::{HttpFetcher, DEFAULT_CONNECT_TIMEOUT, DEFAULT_IPNS_RECORD_TIMEOUT})
- ethereum.rs keeps its own private DEFAULT_GLOBAL_TIMEOUT=30s for the RPC transport, deliberately NOT raised. Confirm that a slow ENS RPC read on the same cold-network condition will not itself spuriously time out; the field finding was about the gateway fetch, so this is likely correct but is an adjacent surface the task did not cover.
  (crates/werust-core/src/ethereum.rs:102 DEFAULT_GLOBAL_TIMEOUT = 30s (RPC transport, separate from fetcher))
