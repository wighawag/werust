---
title: review-gate non-blocking nits for 'harden-loopback-test-servers-drain-request-body' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: harden-loopback-test-servers-drain-request-body
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'harden-loopback-test-servers-drain-request-body' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the in-scope premise correction: the task named three fixtures (ethereum LocalRpcServer, ens SequencedRpcServer, ipfs LocalGateway) but ipfs.rs has no LocalGateway/TcpListener today, so only two were hardened and the shared helper was placed crate-wide in lib.rs (not in ethereum.rs) for any future gateway fixture to reuse. Confirmed correct against the bytes: grep finds no TcpListener/LocalGateway in ipfs.rs.
  (work/notes/observations/ipfs-loopback-gateway-fixture-absent-2026-07-23.md records the decision + rejected alternative (re-export from ethereum.rs). No ## Decisions block in the PR body, but the decision is captured in-band.)
