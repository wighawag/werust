---
title: review-gate non-blocking nits for 'ethereum-provider-seam-and-trusted-rpc-backend' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: ethereum-provider-seam-and-trusted-rpc-backend
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ethereum-provider-seam-and-trusted-rpc-backend' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the default RPC endpoint: RpcProvider::new hard-codes a specific third-party public RPC (ethereum-rpc.publicnode.com) as the labelled TRUSTED default. Confirm this specific provider is an acceptable default egress target for a privacy-focused browser (a default RPC sees every ENS lookup the user makes).
  (DEFAULT_RPC_ENDPOINT in crates/werust-core/src/ethereum.rs; recorded in the Decisions block. Trust story is honest (labelled TRUSTED, overridable via with_endpoint), but the concrete host is a user-visible network default worth a human nod.)
- Ratify transport path (b): RpcProvider binds its OWN RpcTransport/UreqRpcTransport rather than extending the shared Fetcher seam with POST. The task allowed either; (b) was chosen to avoid widening a shared seam for one consumer.
  (Decisions block + module docs. Recorded, well-reasoned, keeps Fetcher untouched; a second POST consumer later is a clean follow-up. Looks correct.)
- Ratify user-visible defaults: BlockTag::Latest default, fixed JSON-RPC id:1, and the 10s/30s connect/global timeouts (mirroring Fetcher). All in-scope choices the task did not fully pin.
  (LATEST_BLOCK_TAG, build_eth_call_request id, DEFAULT_CONNECT_TIMEOUT/DEFAULT_GLOBAL_TIMEOUT. Sensible; timeouts mirror the Fetcher seam. Non-load-bearing.)
