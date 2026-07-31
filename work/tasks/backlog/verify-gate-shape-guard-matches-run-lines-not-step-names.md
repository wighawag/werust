---
title: "The new verify-gate shape guard can be satisfied by a step's NAME, so a drifted command would pass it"
slug: verify-gate-shape-guard-matches-run-lines-not-step-names
blockedBy: [verify-lints-test-targets-and-clears-the-existing-debt]
covers: []
---

## What to build

A residue of `verify-lints-test-targets-and-clears-the-existing-debt`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). One test, narrowed.

`crates/werust-core/tests/verify_gate_shape.rs` exists to hold the three copies of the gate command (`dorfl.json`'s `verify`, `verify.yml`, `release.yml`) identical, so the bar cannot drift between them. But its `job_run_lines` helper flattens EVERY scalar string in the job, including each step's `name:` value, not only its `run:` value.

Three of this repo's steps are NAMED after the command they run (`cargo fmt --check`, `cargo build`, `cargo test`), so for those the assertion is satisfied by the NAME alone: a step could read `name: cargo build` with `run: cargo build --release` underneath and the guard would still pass. The clippy step happens to be safe only because its name is shorter than its command — an accident, not a property.

Narrow the helper to `run:` values (and whatever else genuinely executes, such as a composite action's `with:` command input if one is ever used), so the guard asserts what the job DOES rather than what it is called. Then prove the teeth the way this repo proves its others: drift one `run:` line under its matching `name:`, watch the test go red, revert, and say in the commit that you did — this is the second guard in the same drive that passed review while unable to fail.

**Scope:** one test helper and its doc comment. No change to the gate command, the workflows, or `dorfl.json`.

## Acceptance criteria

- [ ] The guard reads only values that are actually executed; a step whose `name:` matches the expected command but whose `run:` has drifted FAILS it.
- [ ] Proven once by deliberately drifting a `run:` line under its matching `name:` (then reverted), and stated in the commit.
- [ ] The helper's doc comment describes what it really inspects.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: `crates/werust-core/tests/verify_gate_shape.rs`'s `job_run_lines` flattens EVERY scalar in the job, including step `name:` values, so for the three steps named after their own command (`cargo fmt --check`, `cargo build`, `cargo test`) the assertion is satisfied by the NAME alone — a step could be `name: cargo build` with `run: cargo build --release` and still pass. The clippy step is safe only by the accident that its name is shorter than its command. Narrow the helper to values that are actually EXECUTED (`run:`, plus a command-carrying `with:` input if one ever appears), fix its doc comment, and prove the teeth by drifting a `run:` line under its matching `name:` and watching it red before reverting. No change to the gate command itself, the workflows, or `dorfl.json`.
