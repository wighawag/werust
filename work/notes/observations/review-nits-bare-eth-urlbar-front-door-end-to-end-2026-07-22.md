---
title: review-gate non-blocking nits for 'bare-eth-urlbar-front-door-end-to-end' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: bare-eth-urlbar-front-door-end-to-end
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'bare-eth-urlbar-front-door-end-to-end' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the front door does NOT call the prior task's LoadLifecycle::mark_name_via_trusted_rpc hook (the name-via-trusted-rpc-trust-state task added it as 'the hook the front-door path will call'). Instead it added a new mark_ens_origin flag + redirected mark_content_verified. This is the correct fix for the async clash (the scheme handler fires after navigate and would clobber a direct mark), but it leaves mark_name_via_trusted_rpc dead in production (only a webview-renderer test still calls it). Retained-not-removed is deliberate and recorded in the spike Decisions; ratify keeping the now-unused hook or fold it out later.
  (crates/webview-renderer/src/lib.rs:178 mark_name_via_trusted_rpc has no production caller; front door drives ens_origin+mark_content_verified redirect instead. Documented in docs/spikes/.../README.md Decisions.)
- Ratify: reloading an ENS page drops the pinned name and degrades the posture from NameViaTrustedRpc to plain ContentVerified (reload does not re-resolve in Phase 1). The reloaded bytes are still the same content-addressed CID so it is honest-if-coarser, and it is not an acceptance criterion, but a user reloading ronan.eth silently loses the name-via-trusted-RPC label and the .eth name in the bar.
  (crates/werust-core/src/lib.rs BrowserShell::reload clears url_override; recorded in spike Decisions as a deliberate Phase-1 deferral.)
