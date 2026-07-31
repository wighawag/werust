---
title: "URGENT: main is RED — pin the Rust toolchain so the gate is reproducible, and fix the 1.97 clippy lint the unpinned CI found"
slug: pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main
blockedBy: []
covers: []
---

## What to build

**`main` is currently RED and every release is blocked.** Cut by the conductor on 2026-07-31, minutes after diagnosing it. This is the predicted consequence of `verify-lints-test-targets-and-clears-the-existing-debt` landing `-D warnings` onto an UNPINNED toolchain, recorded as an exposure at that task's Gate-3 and realised within the hour.

## The incident, diagnosed

[Run 30622910777](https://github.com/wighawag/werust/actions/runs/30622910777) failed at `verify`, taking every downstream release job (`goreleaser`, `android-apk`, `ios-simulator-app`, `macos-desktop-app`, `windows-desktop-app`) to `skipped`. The failure:

```
error: this block may be rewritten with the `?` operator
   --> crates/native-renderer/src/tokenizer.rs:377:20
377 |               } else if let Some(dec) = other.strip_prefix('#') {
378 | |                 dec.parse::<u32>().ok()?
379 | |             } else {
380 | |                 return None;
381 | |             };
    = note: `-D clippy::question-mark` implied by `-D warnings`
    = help: for further information visit .../rust-clippy/rust-1.97.0/index.html#question_mark
```

**Why it passed every local gate and still reds CI, which is the part that matters:** CI installs clippy via `rustup component add` with NO pin and is currently on **1.97.0**; this development machine (and therefore every `dorfl` Gate-1 run, which executes `verify` locally) is on **1.91.1**. `clippy::question_mark` does not fire on 1.91 for this code. So the acceptance gate and CI are running six minor versions apart, and the gate that is supposed to DECIDE pass/fail cannot see the failure. Until that is closed, every task in this repo will keep going green locally and red on `main`.

## Do both halves, in this order

**1. Fix the lint.** Rewrite the block at `crates/native-renderer/src/tokenizer.rs:377` with the `?` operator as clippy suggests. It is a genuine simplification, not a false positive, so fix it rather than allowing it.

**2. PIN the toolchain, which is the actual fix.** Add a `rust-toolchain.toml` at the workspace root pinning `channel` plus the `rustfmt` and `clippy` components, so `rustup` gives the SAME compiler to this machine, every `dorfl` gate, and every CI leg. Then make the workflows honour it (remove or adjust any `rustup component add` / toolchain-selecting step that would override the pin — check `verify.yml`, `release.yml`, `macos-renderer.yml`, `windows-renderer.yml`, `mobile-ios.yml`).

**Choose the pinned version deliberately and say why.** Pinning to **1.97.0** (what CI has today) is the honest default: it is the bar `main` is already being judged against, and it keeps the strict gate meaningful. Be aware of the cost and handle it: **this machine will download 1.97 on the next gate run, and 1.97's clippy may well find MORE lints across the workspace than the one above** — the local runs that produced today's green were all 1.91. Clear whatever the pinned toolchain reports, across `--all-targets`, before declaring done. Pinning DOWN to 1.91 instead would turn CI green immediately and is tempting, but it makes the repo's bar the developer's laptop rather than a chosen version; if you take that route, say so explicitly and record when it should be raised.

**Also record, in the spike:** with a pin in place, a toolchain bump becomes a deliberate, reviewable change (one file) that can be made when someone is ready to clear the new lints — which is the property `-D warnings` needs to be safe. Note that the same bar is in `release.yml`, so before the pin an unrelated Rust release could fail a TAG build.

**Do not weaken the gate to escape this.** Do not drop `-D warnings`, do not drop `--all-targets`, do not add a blanket `#[allow]`. The gate is doing its job; what is missing is reproducibility.

## Acceptance criteria

- [ ] `crates/native-renderer/src/tokenizer.rs` no longer trips `clippy::question_mark`, fixed with `?` rather than an allow.
- [ ] A `rust-toolchain.toml` pins the channel and the `rustfmt`/`clippy` components at the workspace root.
- [ ] No workflow step overrides the pin; every leg (Ubuntu, macOS, Windows, mobile) resolves the SAME toolchain.
- [ ] `cargo clippy --all-targets -- -D warnings` is clean UNDER THE PINNED TOOLCHAIN, not merely under whatever the machine had before.
- [ ] The pinned version, and why that version, is recorded — including that a bump is now a deliberate one-file change.
- [ ] `main`'s `verify` run goes GREEN after this lands.

## Prompt

> Goal: `main` is RED and all release jobs are skipped. Run 30622910777 failed `verify` on `error: this block may be rewritten with the ? operator` at `crates/native-renderer/src/tokenizer.rs:377`, from `-D clippy::question-mark` under clippy **1.97.0** in CI — while this machine, and therefore every `dorfl` Gate-1 run of `verify`, is on **1.91.1**, where the lint does not fire. So the acceptance gate cannot see what CI sees, and tasks will keep passing locally and redding main. Fix BOTH halves: (1) rewrite that block with `?` (a real simplification, not a false positive; do not `#[allow]` it); (2) add a workspace-root `rust-toolchain.toml` pinning the channel plus `rustfmt` and `clippy`, and make every workflow honour it (drop or adjust any `rustup component add`/toolchain-selecting step in `verify.yml`, `release.yml`, `macos-renderer.yml`, `windows-renderer.yml`, `mobile-ios.yml`). Pin to 1.97.0 by default — it is the bar main is already judged against — but EXPECT 1.97 to surface more lints than 1.91 did across `--all-targets`, and clear them all under the pinned toolchain before declaring done. If you instead pin down to 1.91, say so explicitly and record when it should be raised. Do NOT weaken the gate (`-D warnings` and `--all-targets` both stay). Record that a bump is now a deliberate one-file change, which is what makes `-D warnings` safe, and that the same bar sits in `release.yml` so an unrelated Rust release could otherwise fail a tag build.
