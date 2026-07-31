---
title: review-gate non-blocking nits for 'provider-refuses-honestly-instead-of-resolving-an-empty-account-list' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: provider-refuses-honestly-instead-of-resolving-an-empty-account-list
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'provider-refuses-honestly-instead-of-resolving-an-empty-account-list' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify or soften the stated reason the chain id is a constant: the doc says ENS resolution calls the MAINNET registry deployment so werust is wired to mainnet by construction, but the ENS registry sits at that SAME address on Sepolia/Goerli, so a WERUST_RPC_URL pointed at another chain resolves .eth against THAT chain's registry while the page is still told 0x1. Keep the constant (mainnet-only is the declared Phase-1 scope) and reword, or cut a follow-up for the endpoint/chain mismatch?
  (crates/werust-core/src/ethereum.rs:90-114 (CHAIN_ID doc) vs crates/werust-core/src/ens.rs:70-74 (REGISTRY_ADDRESS, mainnet-only, no per-network config). The endpoint lever's own doc names a local node as the archetype, e.g. anvil at chain 31337.)
- Ratify leaving the macOS typecheck harness RED: the CHAIN_ID stand-in line is fixed and verified, but occurrence 3 (the stand-in lacks trust_pin_action_label / trust_pin_action_visible / trust_pin_detail, which crates/desktop-paint imports) was found and deliberately left unrepaired as out of scope, so the next macOS agent still meets an E0432 first. Add the three stubs here, or leave it to the stand-in-ownership task?
  (work/notes/observations/macos-typecheck-stand-in-core-drifts-unwatched-2026-07-31.md occurrence 3; confirmed statically: no trust_pin symbols in docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh, imports at crates/desktop-paint/src/lib.rs:81. Pre-existing since e772025, not caused here.)
- Ratify the knowing EIP-1193 non-conformance: the shim never emits connect, which the spec makes a MUST (the task assumed SHOULD). The agent recorded why it is inert today and set a revisit trigger rather than fixing it. Accept as recorded?
  (work/notes/findings/eip1193-connect-event-never-emitted-does-not-matter-yet-2026-07-31.md plus the summary in provider_shim's doc, crates/werust-core/src/provider.rs:288-305.)
