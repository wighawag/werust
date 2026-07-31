---
title: "Make `verify`'s clippy lint TEST targets too, clearing the existing debt in the same change so the gate is never knowingly left red"
slug: verify-lints-test-targets-and-clears-the-existing-debt
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

Origin: the observation `work/notes/observations/verify-clippy-does-not-lint-test-targets-2026-07-30.md`, raised again by the conductor in the 2026-07-31 drive and RATIFIED by the human.

`dorfl.json`'s `verify` runs bare `cargo clippy`, which lints lib and bin targets only. Every `#[cfg(test)]` module in this repo is therefore unlinted, and lint debt accumulates there invisibly. That matters more here than in most repos: this project's tests are not an afterthought but the primary evidence surface (source-shape guards, parity guards, recorded-verdict guards, fetch counters), they are large, and they are where most new code lands in a typical task.

**Do BOTH halves in one change, in this order.** The whole reason this was not done earlier is that flipping the flag alone reds the gate for every unrelated task until someone cleans up:

1. **Clear the debt first.** As of the observation, `cargo clippy --all-targets` reports an `unnecessary use of copied` in `crates/werust-core/src/debug.rs` and nine `field_reassign_with_default` in the `crates/werust-macos/src/paint.rs` tests. That inventory is from 2026-07-30 and the tree has moved a long way since (a `desktop-paint` extraction, a Windows shell, a mobile collapse, a pins module), so **re-take the inventory rather than trusting that list**, and fix what it actually reports.
2. **Then flip the gate** to `cargo clippy --all-targets` in `dorfl.json`'s `verify`, and only then, so the repo is never knowingly left with a red gate between two commits.

**Fix the lints properly, do not silence them.** `#[allow(...)]` is acceptable only where the lint is genuinely wrong for that code, and then with a comment saying why. A blanket crate-level allow defeats the entire point of this task. If a lint turns out to be pervasive and unhelpful for test code specifically, that is a legitimate finding: say so, name the lint, and propose configuring it deliberately (in `Cargo.toml`'s `[lints]` or a `clippy.toml`) rather than smearing allows through the tree.

**Watch the blast radius.** `--all-targets` also picks up `examples/` and `benches/`, and this repo has real examples that are load-bearing CI steps (`trust_hooks_smoke`, `window_smoke`, `print_version`, `chrome_json_cost`). Those are not throwaway code and their lints should be fixed like any other. Note also that the platform-specific crates (`macos-renderer`, `werust-macos`, `windows-renderer`, `werust-windows`) do NOT build on the Ubuntu gate, so their `cfg`-gated test code will not be linted there either way: say plainly in the task's record which crates the flipped gate actually covers, so nobody believes it covers more than it does. If the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here.

**Confirm the gate really is stricter afterwards.** Prove it once during development (introduce a deliberate test-only lint, watch the gate go red, revert) exactly as the repo's other teeth-having guards are proven, and say in the commit that you did.

## Acceptance criteria

- [ ] `cargo clippy --all-targets` is clean on the Ubuntu gate's crate set, with every fix being a real fix (any `#[allow]` is narrowly scoped and carries a reason).
- [ ] `dorfl.json`'s `verify` runs `cargo clippy --all-targets`, and the debt is cleared in the SAME change so the gate is never red between commits.
- [ ] The record states which crates the flipped gate actually covers and which (the platform-gated ones) it still cannot, so no one over-reads it.
- [ ] The stricter gate is proven to have teeth once (a deliberate test-only lint reds it), and that is stated.
- [ ] `cargo fmt --check && cargo clippy --all-targets && cargo build && cargo test` green.

## Prompt

> Goal: `verify` runs bare `cargo clippy`, which lints lib/bin targets only, so every `#[cfg(test)]` module in this repo — the primary evidence surface here — is unlinted. Ratified by the human on 2026-07-31. Do both halves in ONE change and in this order: first RE-TAKE the `cargo clippy --all-targets` inventory (the 2026-07-30 observation lists a `debug.rs` `unnecessary use of copied` and nine `field_reassign_with_default` in `werust-macos`'s paint tests, but the tree has moved a long way since) and fix what it really reports; THEN flip `dorfl.json`'s `verify` to `--all-targets`, so the repo is never knowingly red between two commits. Fix lints properly — `#[allow]` only where the lint is genuinely wrong, narrowly scoped, with a reason; no blanket crate-level allow, and if a lint is pervasive and unhelpful for test code specifically, say so and propose configuring it deliberately instead. `--all-targets` also covers `examples/`, which here are load-bearing CI steps (`trust_hooks_smoke`, `window_smoke`, `print_version`, `chrome_json_cost`), so treat them as real code. State plainly which crates the flipped gate covers and which platform-gated ones it still cannot. Prove the stricter gate has teeth once (deliberate test-only lint reds it, then revert) and say so.

## Requeue 2026-07-31

CONDUCTOR HANDOFF (2026-07-31, drive-tasks). Gate 2 blocked this CORRECTLY, and I verified every one of its factual claims against your own branch before writing this. The CODE is fine; the RECORD is wrong, and it drops a follow-on the task asked for by name.

WHAT IS WRONG, verified on your branch tip:

1. `work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md` says the only two clippy invocations in the repo are in `verify.yml` and `release.yml`, and that the ~7.5k platform lines are unlinted EVERYWHERE. That is not true, and both counter-examples are files you edited or read in this very task:
   - `docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh` ends in `exec cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples`, and its own header says it runs CLIPPY rather than `check` deliberately.
   - `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` runs THREE clippy invocations against `aarch64-apple-darwin`: the engine `--all-targets`, `werust-macos --lib --examples`, and `macos-origin-probe --all-targets`.
   So the platform halves ARE linted from Linux today, just not by a gate and not at the gate's bar. Saying they are unlinted everywhere leaves the next agent with a false premise AND hides the cheapest available lever.

2. The task said, in as many words: "If the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here." Neither the note nor the spike README mentions the harnesses at all, so that follow-on does not exist. Record it.

3. The README says the gate compiles all 18 workspace members. `Cargo.toml` lists 17. I counted.

WHAT TO DO (record-only; do NOT re-do the lint work, and do NOT flip anything else):

- Correct the observation note so it says what is true: `verify` now lints all targets for the crates the Ubuntu gate can build; the platform-gated crates are linted from Linux by the two cross-target HARNESSES, which are developer tools rather than gates, and which run at a LOWER bar than `verify` now does (the Windows one uses `--tests --examples` rather than `--all-targets`, `werust-macos` is only `--lib --examples`, and none of them use `-D warnings`). Name the harnesses by path.
- Correct the spike README's coverage section the same way, and fix 18 -> 17.
- RECORD THE FOLLOW-ON the task asked for: raise the two harnesses to the gate's bar (`--all-targets` plus `-D warnings`) so the platform halves are held to the same standard the rest of the tree now is, and say what it would cost. Whether it becomes a task is the human's call; your job is that the lever is visible instead of invisible.

Everything else about this build stands. Do not weaken the `--all-targets` gate you landed, do not revert the lint fixes, and keep the teeth proof.
