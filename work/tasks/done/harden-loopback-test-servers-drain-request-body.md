---
title: "Harden the loopback test HTTP servers to drain the full request body (fix the intermittent EOF flake in the RPC/ENS/gateway fixtures)"
slug: harden-loopback-test-servers-drain-request-body
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: []
---

## What to build

Fix the known intermittent test flake that reds the `verify` gate roughly 1 run in 6 under parallel load, so a release/CI run is never failed by a test-harness race rather than a real defect. Two `werust-core` end-to-end tests are affected today: `ethereum::tests::end_to_end_eth_call_over_the_bound_transport_off_the_network` and `ens::tests::resolution_end_to_end_over_the_bound_rpc_transport_off_the_network` (and the same class threatens the `ipfs.rs` gateway fixture).

Root cause (already diagnosed in `work/notes/observations/flaky-loopback-rpc-server-partial-read.md` and `flaky-ethereum-end-to-end-loopback-test-2026-07-22.md`): the throwaway loopback HTTP fixtures (`LocalRpcServer` in `ethereum.rs`, `SequencedRpcServer` in `ens.rs`, `LocalGateway` in `ipfs.rs`) capture the request with a SINGLE `stream.read(&mut buf)` and assume the whole HTTP request (headers + body) arrives in that one read. Under parallel test load the body can land in a later TCP segment, so the captured body is empty and the `serde_json::from_slice`/body assertion panics with "EOF while parsing a value". It is a harness race, not product code.

Fix: make the fixtures read until the full request is received — parse the headers, read the declared `Content-Length` body, and loop `read()` until that many body bytes are drained (or headers are complete for a body-less request) before asserting on / responding to the request. Prefer a SINGLE shared, race-hardened loopback helper the three fixtures reuse, so the whole family is fixed in one place rather than three copies. This is test-only code; no product behaviour changes.

## Acceptance criteria

- [ ] The loopback HTTP fixtures read the complete request (headers + full `Content-Length` body) before asserting/responding, looping `read()` until the body is drained — no single-read assumption.
- [ ] The two affected end-to-end tests (`ethereum` eth_call, `ens` resolution) pass reliably under parallel load: a repeated full `cargo test` sweep (e.g. 10+ runs) is green, with no intermittent EOF panic.
- [ ] The fix is shared across the `ethereum.rs` / `ens.rs` / `ipfs.rs` loopback fixtures (one hardened helper reused), not patched in only one.
- [ ] No product code changes (test-harness only); the request-body assertions the tests rely on (e.g. the `eth_call` JSON body check) still hold.
- [ ] The two flake observations are resolved/closed (referenced from the done record).

## Blocked by

- None — can start immediately.

## Prompt

> Goal: kill the intermittent "EOF while parsing a value" flake that reds the `verify` gate ~1 run in 6 under parallel load, so releases and CI are never failed by a harness race. The product code is sound; the throwaway loopback HTTP fixtures just read the request with a single `stream.read()` and miss the body when it arrives in a later TCP segment.
>
> Where to look: `crates/werust-core/src/ethereum.rs` (`LocalRpcServer`), `crates/werust-core/src/ens.rs` (`SequencedRpcServer`), `crates/werust-core/src/ipfs.rs` (`LocalGateway`) — all copy the same single-read pattern. The diagnosis + prescription are in `work/notes/observations/flaky-loopback-rpc-server-partial-read.md` and `flaky-ethereum-end-to-end-loopback-test-2026-07-22.md`: loop the read until the `Content-Length` body is drained. Prefer one shared race-hardened loopback helper the three fixtures reuse.
>
> Done = the fixtures drain the full request body before asserting/responding; the two affected end-to-end tests pass reliably across a repeated full `cargo test` sweep under parallel load; the fix is shared, not one-off; no product code changes; the flake observations are closed. FIRST re-check the fixtures still match this description (they may have been refactored). This is test-only; keep the existing request-body assertions intact.
