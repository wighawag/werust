---
title: "Gate-3 verdict: verify-lints-test-targets-and-clears-the-existing-debt (APPROVE, after a requeue) — the gate got stricter than I asked, and the record got honest"
date: 2026-07-31
status: open
reviewOf: verify-lints-test-targets-and-clears-the-existing-debt
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main` after one requeue. `dorfl.json`'s `verify` is now:

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test
```

## The block was right, and I checked it myself before acting on it

Gate 2 blocked round 1 because the RECORD was wrong, not the code. The new observation claimed the platform halves were unlinted EVERYWHERE. I verified the reviewer's counter-claim against the branch before requeuing, rather than relaying it:

- `typecheck-windows-from-linux.sh` really does end in `cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples`, and its header says it runs clippy rather than `check` deliberately.
- `typecheck-macos-from-linux.sh` really does run three `cargo clippy` invocations against `aarch64-apple-darwin`.
- `Cargo.toml` really does list 17 workspace members, not the 18 the README claimed.

All three confirmed. The block also caught that the task's explicitly-requested follow-on ("if the harnesses can cheaply lint them from Linux, note that as the follow-on") had been dropped. Requeued with the verification and a record-only instruction: do not re-do the lint work, do not weaken the gate.

Round 2 came back with a note that is now better than what I asked for. It names both harnesses by path, states precisely how their bar is LOWER (`--tests --examples` rather than `--all-targets`, `werust-macos` only `--lib --examples`, no `-D warnings` anywhere so clippy prints and exits 0), names what is in no harness at all (`windows-origin-probe`'s Win32 half, `werust-android`'s `jni_exports`), and MEASURED the follow-on's cost: three of the four legs are already clean at the higher bar, and the fourth could not be measured because the harness's stand-in core has drifted — itself filed as its own note.

## Criteria, ticked

1. **`cargo clippy --all-targets` clean, every fix a real fix.** MET.
2. **The gate flipped in the SAME change as the cleanup**, so the repo was never knowingly red between commits. MET.
3. **The record states which crates the flipped gate covers and which it cannot.** MET (after the requeue), and now correctly rather than approximately.
4. **The stricter gate is proven to have teeth.** MET.

## Ratified, with one consequence the human should see

**`-D warnings` was added, which I did not ask for.** The argument is right and recorded: without a deny flag clippy exits 0, so the criterion "a deliberate test-only lint reds the gate" is literally unreachable. I ratify it.

**But it interacts with an unpinned toolchain, and it was also applied to `release.yml`.** The repo installs clippy via `rustup component add` with no toolchain pin, so a future Rust release that adds or tightens a lint can red the gate for a task that did not cause it — and now that the same bar is in `release.yml`, it can fail a TAG build, not only a PR. The shape guard deliberately forces all three copies to stay identical, so this cannot be relaxed in one place without a decision. That is a sensible design and a real operational exposure at once; **pinning the toolchain is the obvious mitigation and is a human call**, so it goes to the batch rather than being invented here.

## Residue, cut as `verify-gate-shape-guard-matches-run-lines-not-step-names`

The new guard is weaker than its doc claims: `job_run_lines` flattens every scalar in the job, INCLUDING step `name:` values. Three steps are named after their own command, so for those the assertion passes on the name alone — `name: cargo build` over `run: cargo build --release` would sail through. The clippy step is safe only because its name is shorter than its command, which is an accident rather than a property. **This is the second guard in this drive that passed review while unable to fail**, which is starting to look like a pattern worth naming: a guard is not done until someone has watched it go red.
