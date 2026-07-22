---
title: review-gate non-blocking nits for 'ens-namehash-registry-resolver-contenthash-resolution' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: ens-namehash-registry-resolver-contenthash-resolution
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ens-namehash-registry-resolver-contenthash-resolution' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the bound dependency choice: ens-normalize 0.1.1 (adraffy Rust port) sets the user-visible ENSIP-1 normalization behaviour every ENS input flows through, chosen over ens-normalize-rs. Recorded in the spike Decisions block; confirm the lighter port is acceptable long-term.
  (docs/spikes/.../README.md Decisions; Cargo.toml ens-normalize = 0.1.1)
- Ratify hand-rolled ABI encode/decode for the fixed ENS shapes (selector+bytes32 encode, address return, dynamic bytes return) instead of a bound ABI codec. Layout-only (crypto stays the bound keccak), overflow-guarded, refuses impossible framing as MalformedReturn. Reversible via a follow-up if richer ABI is later needed.
  (ens.rs encode_bytes32_call/decode_address_return/decode_bytes_return; Decisions block)
- Observation recorded, not fixed: the new loopback fixture test resolution_end_to_end_over_the_bound_rpc_transport_off_the_network shares the same accept-race flake family as the ethereum end-to-end test. A shared race-hardened harness would fix the family; acceptable to defer.
  (work/notes/observations/flaky-ethereum-end-to-end-loopback-test-2026-07-22.md)
