---
title: "Gate-3 conductor review: platform-capability-parity-guard (APPROVE)"
date: 2026-07-23
status: open
reviewOf: platform-capability-parity-guard
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 0cdabb8
---

## Verdict: APPROVE ✅ — merged to origin/main as 0cdabb8 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

## Acceptance criteria — all met

- Checked-in capability matrix (`docs/platform-capability-matrix.toml`) x {desktop, iOS, Android}, each cell `implemented` / `stubbed`+task / `n-a`+reason.
- `verify`-enforced via a plain `cargo test` in `werust-core` (`tests/platform_capability_parity.rs`) that rides the existing gate with no CI change. Enforcement is real and tested: `an_untracked_stub_fails_the_guard`, `a_stub_pointing_at_a_nonexistent_task_fails_the_guard`, `a_missing_cell_fails_the_guard`, `an_na_cell_without_a_reason_fails_the_guard`.
- Seeded with TRUE current reality (green only because gaps are tracked). The no-op-seam rule is expressed THROUGH the matrix's `stubbed` state (a sound consolidation; the brittle source-scanner alternative was rejected with rationale in ADR-0005).
- ADR-0005 records the design; an observation records the seed correction.

## Notable: the guard immediately earned its keep

The agent did NOT trust my design-discussion seed. Against the code it found that `eip1193-provider` injection and `trust-indicator` are ALSO mobile silent no-ops (both mobile backends leave `register_script_message_handler`/`inject_script` empty and inherit the default `trust_posture`), not just `ipfs-render`. It corrected the seed to desktop-`implemented`, iOS/Android-`stubbed` and recorded the finding (`mobile-provider-and-trust-are-also-silent-no-ops-2026-07-23.md`). This is exactly the class of silent-one-platform gap the guard exists to catch, caught on its first run.

## Follow-up routed (non-blocking): dedicated mobile provider+trust task

All three mobile stubs (`ipfs-render`, `eip1193-provider`, `trust-indicator`) currently link to `mobile-ipfs-scheme-interception-ios-and-android`, whose body is scoped to `ipfs://` scheme interception and does NOT cover the provider bridge or trust-posture wiring on mobile. The guard's link resolution is deliberately mechanical (the slug must name an existing task), so the cells are tracked but two of them point at a task that will not complete them. Conductor files a dedicated follow-on `mobile-provider-injection-and-trust-indicator` and repoints those two cells to it, so each stubbed cell links to a task that genuinely covers its completion. (This is a capture/scoping move, not a coin-flip.)

## Gate-2 nits (non-blocking, already recorded)

Four non-blocking nits in `review-nits-platform-capability-parity-guard-2026-07-22.md`, left open for human triage.
