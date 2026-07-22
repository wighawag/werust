---
title: review-gate non-blocking nits for 'ensip7-contenthash-decoder-typed-graceful-errors' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: ensip7-contenthash-decoder-typed-graceful-errors
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ensip7-contenthash-decoder-typed-graceful-errors' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify an un-recorded in-scope decision: the agent added a THIRD error variant, ContenthashError::InvalidCid, beyond the two the acceptance criteria named (NoContenthash and Malformed). It distinguishes an ipfs-ns protoCode carrying unparseable CID bytes from generic Malformed. Reasonable and well-motivated, but not in the spec/task and not recorded in a Decisions block (the commit message is a bare one-liner).
  (crates/werust-core/src/contenthash.rs ContenthashError enum + test an_ipfs_ns_protocode_with_invalid_cid_bytes_is_a_distinct_invalid_cid_error)
- Ratify the chosen human-facing protocol display names and the unknown-code wording: Tor onion / Tor onion v3 for onion/onion3, an unknown protocol for Unknown, and unsupported/unknown contenthash protocol (0x..). These are user-visible strings the spec left to the builder.
  (ProtoCode::display_name + DecodedContenthash::reason)
- Minor varint edge case: read_protocode_varint rejects shift>=64 but still processes a 10th byte at shift=63, which could drop bits above bit 63 for a non-canonical oversized varint rather than flagging it Malformed. No real multicodec protoCode reaches this and malformed input never panics, so impact is nil; noted only for completeness.
  (read_protocode_varint overlong guard)
