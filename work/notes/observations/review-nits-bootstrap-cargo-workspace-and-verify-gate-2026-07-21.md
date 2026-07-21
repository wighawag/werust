---
title: review-gate non-blocking nits for 'bootstrap-cargo-workspace-and-verify-gate' (Gate 2 approve)
date: 2026-07-21
status: open
reviewOf: bootstrap-cargo-workspace-and-verify-gate
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'bootstrap-cargo-workspace-and-verify-gate' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify an unrecorded in-scope decision: the werust binary declares Cargo path-dependencies on all four seam crates (renderer, native-renderer, fetcher, script-engine) and native-renderer depends on renderer, but NONE are actually used in code (only referenced in doc comments). This is dead dependency wiring. It compiles green because cargo/stable-clippy do not error on unused crate deps by default. Intent was likely to pre-wire the dependency graph; fine to keep, but the human should ratify it rather than have it silently linger.
  (crates/werust/Cargo.toml deps vs crates/werust/src/main.rs (no use of any seam crate); crates/native-renderer/Cargo.toml deps on renderer, unused in lib.rs)
- The task file landed in tasks/done/ still carries needsAnswers: true in its frontmatter. The Requeue note states this was only a PATH/env gate failure and no code change was needed, so the axis appears to be a stale leftover rather than a real open question. Confirm there is no genuine unanswered question and clear the flag.
  (work/tasks/done/bootstrap-cargo-workspace-and-verify-gate.md frontmatter needsAnswers: true; Requeue 2026-07-21 note says the failure was exit 127 cargo-not-on-PATH, env-fixed)
- The PR/commit description carries no Decisions block; the unused-seam-dependency wiring above is exactly the kind of non-obvious in-scope choice that block should record. Add a Decisions note for future traceability.
  (git log -1 body for 96e67d3 is a bare subject line with no ## Decisions section)
