---
title: review-gate non-blocking nits for 'name-via-trusted-rpc-trust-state' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: name-via-trusted-rpc-trust-state
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'name-via-trusted-rpc-trust-state' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify enum variant name TrustPosture::NameViaTrustedRpc and hook name LoadLifecycle::mark_name_via_trusted_rpc(). The sibling front-door task bare-eth-urlbar-front-door-end-to-end must call these exact names; they are the intra-spec handoff surface.
  (Recorded in work/notes/observations/name-via-trusted-rpc-posture-decisions.md; matches spec phrase 'name via TRUSTED RPC' and mirrors ContentVerified/mark_content_verified.)
- Ratify the desktop indicator label glyph+text (U+25C8 'name via trusted RPC'), tooltip wording, and CSS class trust-name-trusted-rpc at blue #1a5fb4. User-visible default; deliberately omits the word 'verified'.
  (crates/werust/src/main.rs trust_indicator/detail/css_class; distinct from green #0a7d28 verified and amber #9a6a00 unverified.)
- Ratify leaving the mobile ffi_json unchanged. Verified: ffi_json.rs encodes no trust posture today, so this is a pre-existing gap not introduced here, but the third state is desktop-only until mobile encodes posture.
  (grep of crates/werust-ios|android ffi_json.rs shows no trust/posture/verif references.)
