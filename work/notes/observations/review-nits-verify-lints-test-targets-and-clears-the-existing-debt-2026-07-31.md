---
title: review-gate non-blocking nits for 'verify-lints-test-targets-and-clears-the-existing-debt' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: verify-lints-test-targets-and-clears-the-existing-debt
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'verify-lints-test-targets-and-clears-the-existing-debt' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the decision to add -D warnings, which the task did not ask for (it asked only for --all-targets). It is well argued and recorded in the spike README Decisions block: without a deny flag clippy exits 0 and the criterion 'a deliberate test-only lint reds the gate' is unreachable. The cost is real though: the toolchain is unpinned (rustup component add rustfmt clippy), so a future Rust release that adds a lint can red the gate for a task that did not cause it.
  (dorfl.json verify; docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md, section Decisions)
- Ratify that the tightened bar was also applied to release.yml, not just verify.yml. A new-toolchain lint can now fail a TAG build, not only a PR. The shape guard forces the three copies to stay identical, so this is hard to relax in isolation.
  (.github/workflows/release.yml:123 now runs cargo clippy --all-targets -- -D warnings; the guard asserts every dorfl.json step appears verbatim in both workflows)
- The new shape guard is weaker than its doc comment claims: job_run_lines flattens EVERY scalar string in the job, including step name: values, not just run: lines. For the three steps whose name equals the command (cargo fmt --check, cargo build, cargo test), the assertion is satisfied by the NAME alone, so a drifted run line (e.g. run: cargo build --release under name: cargo build) would pass. The clippy step is safe only because its name is shorter than the command. Worth narrowing to run: values.
  (crates/werust-core/tests/verify_gate_shape.rs, fn job_run_lines and every_ci_leg_runs_the_same_gate_as_dorfl_json)
- Bookkeeping: the task file landed in work/tasks/done/ still carries needsAnswers: true and its sidecar work/questions/task-verify-lints-test-targets-and-clears-the-existing-debt.md still holds 5 unanswered stuck questions that this round resolves. Someone should clear them so a completed item does not read as open.
  (work/tasks/done/verify-lints-test-targets-and-clears-the-existing-debt.md frontmatter; sidecar has 5 Q blocks)
