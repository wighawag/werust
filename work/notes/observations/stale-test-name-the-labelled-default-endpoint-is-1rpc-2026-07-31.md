---
title: "`the_labelled_default_endpoint_is_1rpc` now asserts an Infura URL, so the test NAME lies about the default RPC endpoint"
date: 2026-07-31
---

Noticed while adding `CHAIN_ID` beside it: `crates/werust-core/src/ethereum.rs` still names the test `the_labelled_default_endpoint_is_1rpc` and its comment still says "the labelled default is now the public, keyless `1rpc.io/eth`", but the asserted value (and `DEFAULT_RPC_ENDPOINT` itself) is now `https://mainnet.infura.io/v3/…`. The test passes, so nothing is broken; the name and comment are simply stale and would mislead the next reader about which endpoint ships by default. Out of scope for `provider-refuses-honestly-instead-of-resolving-an-empty-account-list`, which only added a constant next to it.
