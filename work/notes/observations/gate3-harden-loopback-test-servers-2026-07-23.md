---
title: "Gate-3 conductor review: harden-loopback-test-servers-drain-request-body (APPROVE) — flake fixed, tree stable for v0.2.1"
date: 2026-07-23
status: open
reviewOf: harden-loopback-test-servers-drain-request-body
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: b752799
---

## Verdict: APPROVE ✅ — merged to origin/main as b752799

Filed + driven to unblock a RELIABLE v0.2.1 release: the pre-release verify gate flaked red (2 loopback end-to-end tests, "EOF while parsing a value") ~1 run in 6 under parallel load. Confirmed via the existing observations it was the KNOWN harness race (single `stream.read()` misses a body that lands in a later TCP segment), not a product regression: the tests passed 3/3 in isolation and single-threaded.

## The fix (verified)

A shared `crate::loopback_test_server::read_request_body` helper reads the full request (headers + `Content-Length` body, looping `read()` until drained) before responding, reused by the `ethereum.rs` / `ens.rs` / `ipfs.rs` loopback fixtures. Test-only; no product behaviour change; the request-body assertions still hold.

## Verified stable

After the merge, a repeated full `cargo test` sweep ran 6/6 GREEN under parallel load (the flake was ~1/6 before). The full verify gate (fmt + clippy + build + test) is green. The tree is now reliable for tagging.

## Gate-2 nits (non-blocking)

One non-blocking nit in `review-nits-harden-loopback-test-servers-drain-request-body-2026-07-23.md`, left open for human triage.
