---
title: review-gate non-blocking nits for 'platform-capability-parity-guard' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: platform-capability-parity-guard
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'platform-capability-parity-guard' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the settled design asked for BOTH a capability matrix AND a separate no-silent-no-op-seam rule; the agent expressed the no-op rule THROUGH the matrix (a no-op'd seam = a stubbed cell) instead of building a second source-scanning/proc-macro detector. Recorded in ADR-0005 with rejection rationale (a source scanner would fire on every legit-empty seam method). Reasonable and coherent; confirm this consolidation is acceptable.
  (docs/adr/0005 Considered Options; task Settled decision 1)
- Ratify a seed correction that overrode the task: settled decision 3 listed eip1193-provider and trust-indicator as implemented on all three, but against code both mobile backends no-op register_script_message_handler/inject_script and inherit the default UnverifiedOrigin/no mark_ens_origin, so the agent seeded them as stubbed. Verified true (crates/werust-android|ios/rust/src/backend.rs:253-258; core seam defaults). Correctly routed as drift into an observation note.
  (work/notes/observations/mobile-provider-and-trust-are-also-silent-no-ops-2026-07-23.md)
- Ratify the linkage nuance: all three mobile stubs (ipfs-render, eip1193-provider, trust-indicator) link to mobile-ipfs-scheme-interception-ios-and-android, whose body is scoped to ipfs:// scheme interception only and does not cover the provider bridge or trust posture. The guard resolution is deliberately mechanical (slug must name an existing file), so the gaps are tracked but not each covered by a task that addresses ITS completion. Surfaced in ADR + note; a human may want a dedicated mobile task.
  (docs/adr/0005 Known linkage nuance; task exists in work/tasks/backlog/)
- Minor: GuardError::UntrackedStub and NaWithoutReason are effectively unreachable because parse_cell rejects a stubbed-without-task and n-a-without-reason as parse errors before validate() runs. Behaviour is still correct (both red the gate) and the tests document this, but the two validate() branches are dead paths.
  (platform_capability_parity.rs parse_cell vs validate; tests an_untracked_stub_fails/an_na_cell_without_a_reason)
