---
title: "Flaky: ens::tests::resolution_end_to_end_over_the_bound_rpc_transport_off_the_network intermittently fails with Transport(\"io: Peer disconnected\")"
date: 2026-07-23
status: open
kind: observation
---

`cargo test -p werust-core --lib` intermittently fails `ens::tests::resolution_end_to_end_over_the_bound_rpc_transport_off_the_network` (crates/werust-core/src/ens.rs) with `Provider(Transport("io: Peer disconnected"))` — a timing race in that test's own loopback `SequencedRpcServer` (the ureq client occasionally sees the peer close before the response is read), roughly 1 run in ~6 here. Pre-existing and unrelated to the IPNS work (the IPNS/front-door tests use in-process doubles, not loopback sockets). Not fixed (out of scope for `ipns-name-resolution-and-render`); a retry/keep-alive hardening of that loopback fixture would remove the flake.
