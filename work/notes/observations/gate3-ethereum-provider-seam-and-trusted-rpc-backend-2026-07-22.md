---
title: "Gate-3 conductor review: ethereum-provider-seam-and-trusted-rpc-backend (APPROVE)"
date: 2026-07-22
status: open
reviewOf: ethereum-provider-seam-and-trusted-rpc-backend
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: f2bb41b
---

## Verdict: APPROVE ✅ — merged to origin/main as f2bb41b (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

Conductor's own diff-vs-acceptance pass (the third review layer after `do`'s acceptance gate and PR/code-review gate). Reviewed the landed diff on `origin/main` against the task's acceptance criteria.

## Acceptance criteria — all met

- `EthereumProvider` seam trait exists: `eth_call(&self, call: &EthCall) -> Result<Vec<u8>, ProviderError>`, inputs are address + ABI calldata + `BlockTag` (Latest/Number), scoped to what ENS needs (not a general dapp-RPC surface).
- `RpcProvider` backend performs JSON-RPC `eth_call` as an HTTP POST with the JSON body. TRANSPORT PATH (b) chosen: `RpcProvider` binds its OWN minimal sync `RpcTransport` seam (default `UreqRpcTransport`, binding `ureq`'s POST) rather than widening the GET-only `Fetcher` seam for one consumer. Decision recorded in module docs + `docs/spikes/ethereum-provider-seam-and-trusted-rpc-backend/README.md`. NOT a GET URL.
- Endpoint overridable via `DEFAULT_RPC_ENDPOINT` const + `RpcProvider::new` / `with_endpoint` (mirrors `GatewayContentSource::new`/`with_gateway`; no config subsystem invented).
- Async accommodation is documented and real: `eth_call` returns an owned `Vec<u8>` by value, explicitly shaped so a Phase-2 async (tokio/Helios) backend can `block_on` its client internally and return owned bytes — the signature does not structurally block the swap.
- Typed errors only: `ProviderError::{Transport(String), Rpc{code,message}, Decode(String)}`. Never panics, never returns empty bytes as success (`an_unparseable_result_is_refused_not_returned_as_empty_bytes`).
- Network-isolated tests: loopback fixture server (`TcpListener::bind("127.0.0.1:0")`, mirroring the fetcher `LocalHttpServer`); NO live network. The request-body assertion is present (`the_outgoing_request_carries_the_eth_call_json_body`: asserts `method == "eth_call"` and the `[callObject, blockTag]` params), so a GET-only transport that silently dropped the body could not pass. Failure paths each covered: JSON-RPC error object -> `Rpc`, transport failure -> `Transport`, non-2xx -> `Transport`, unparseable -> `Decode`, empty result, pinned block -> hex quantity.

## Drift / forward-notes honoured

- Task's READ-FIRST premise ("confirm whether `Fetcher` is still GET-only") was honoured: the agent re-verified `Fetcher::fetch` is GET-only and chose path (b) deliberately rather than assuming a drop-in. Conductor's own up-front freshness check independently confirmed `Fetcher::fetch(&self, url)` is GET-only.

## Gate-2 nits (non-blocking, already recorded)

Three ratification nits in `review-nits-ethereum-provider-seam-and-trusted-rpc-backend-2026-07-22.md`: (1) the concrete default RPC host `ethereum-rpc.publicnode.com` is a user-visible egress default worth a human nod (trust story is honest + overridable); (2) transport path (b) choice; (3) block-tag/id/timeout defaults. All benign in-scope choices; none block integration and none require a re-task. Left open for human triage.
