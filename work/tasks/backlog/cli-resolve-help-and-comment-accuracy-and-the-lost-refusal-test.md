---
title: "Three small accuracy repairs around `werust resolve`: a `--help` line that is false under `--json`, a comment that describes behaviour the code does not have, and a refusal test that went missing in a refactor"
slug: cli-resolve-help-and-comment-accuracy-and-the-lost-refusal-test
blockedBy: [cli-resolve-follows-mutable-names-to-the-cid]
covers: []
---

## What to build

Three residues of `cli-resolve-follows-mutable-names-to-the-cid`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). Each is a few lines; they are grouped because they are the same class (a claim that no longer matches the code) in one task's output.

**1. `werust --help` promises something `--json` breaks.** `usage()` in `crates/werust/src/main.rs` says stdout is ALWAYS the bare `ipfs://<cid>`. That is exactly the contract `headless-cli-mode` established and it is worth keeping true, but it is not true under `--json`, where stdout is the JSON object. Correct the wording so it says what stdout carries in each mode. A `--help` that lies about the machine-readable contract is worse than one that says nothing, because the whole point of that line is to tell a script author what to parse.

**2. A comment describes a failure behaviour the code does not have.** In `crates/werust-core/src/lib.rs`, the comment above the new resolution call in `navigate_ens_name` says the pinned step is kept so a FAILURE still surfaces the stage it failed at. It does not: `fail_ens_load` sets `self.resolving_step = None` before `refresh_chrome`, so a failed resolve reports `LoadStep::Idle`, and the existing test `an_ens_resolution_failure_reports_the_resolving_name_step_and_no_lingering_step` asserts exactly that. The wording predates this change and the behaviour is unchanged by it, so this is a documentation fix, NOT a behaviour change: do not "fix" the code to match the comment. Correct the comment to describe what actually happens and why (no lingering step is the deliberate design), so the next reader does not build on a false premise.

   If, while there, you judge that surfacing the failing STAGE would genuinely be better product behaviour, do not change it here: write it up as an observation for a human, because that is a user-visible decision about failure reporting, not a comment repair.

**3. A fail-closed test was lost in a refactor.** `resolve_output` stopped returning `Result`, and with it `resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one` went away. The REFUSAL itself is still pinned in core (`name_resolution::an_unsupported_contenthash_is_a_named_refusal_not_a_reference`), so what is uncovered is only the thin CLI wrapper: that an unsupported contenthash prints the named reason to stderr and exits 1. That wrapper is small, but it is the fail-closed promise at the surface a user actually touches, and this repo treats fail-closed behaviour as load-bearing rather than incidental. Restore a direct test of the CLI arm.

**Scope:** one help-text correction, one comment correction, one restored test. No behaviour change anywhere.

## Acceptance criteria

- [ ] `werust --help` describes what stdout carries in BOTH modes (bare `ipfs://<cid>` by default, the JSON object under `--json`), and nothing in it claims a contract the code does not keep.
- [ ] The comment in `navigate_ens_name` matches actual behaviour (a failed resolve reports no lingering step), with the behaviour itself unchanged.
- [ ] A test covers the CLI's fail-closed arm directly: an unsupported contenthash prints the core's named reason to stderr and exits 1.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: three small accuracy repairs from `cli-resolve-follows-mutable-names-to-the-cid`, no behaviour change. (1) `usage()` in `crates/werust/src/main.rs` says stdout is ALWAYS the bare `ipfs://<cid>`, which `--json` breaks; say what stdout carries in each mode, because that line exists to tell a script author what to parse. (2) The comment above the resolution call in `navigate_ens_name` (`crates/werust-core/src/lib.rs`) claims the pinned step is kept so a failure surfaces the stage it failed at, but `fail_ens_load` clears `resolving_step` and the existing test asserts `LoadStep::Idle`; fix the COMMENT, not the code, and if you think surfacing the failing stage would be better product behaviour, file it as an observation rather than changing it. (3) `resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one` disappeared when `resolve_output` stopped returning `Result`; the refusal is still pinned in core, so restore a direct test of the CLI arm only (unsupported contenthash -> named reason on stderr, exit 1), because that is the fail-closed promise at the surface the user touches.
