---
title: "Third strike: make the cross-target harness's stand-in `werust-core` impossible to drift, instead of repairing it after each break"
slug: typecheck-harness-standin-core-must-not-drift-again
blockedBy: []
covers: []
---

## What to build

`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` builds a scratch workspace containing a HAND-WRITTEN stand-in `werust-core`, symlinks the REAL macOS sources into it, and type-checks them from Linux. It is the only pre-CI feedback a macOS task gets in a repo that writes all its Apple code blind.

It has now broken **three times in two days**, always the same way — the real core moved and the hand-written stand-in did not:

1. `windows-win32-window-and-chrome` extracted `crates/desktop-paint`, deleting `crates/werust-macos/src/paint.rs`, leaving a dangling symlink. Cost: a whole task (`macos-spike-doc-accuracy-and-harness-guard`, item 0, titled "THE HARNESS IS NOW BROKEN").
2. `provider-refuses-honestly-instead-of-resolving-an-empty-account-list` renamed `STUB_CHAIN_ID` -> `CHAIN_ID`; the stand-in still declared the old name. Cost: a Gate-2 block and a requeue.
3. **Still broken right now:** the stand-in lacks `trust_pin_action_label`, `trust_pin_action_visible` and `trust_pin_detail`, which `crates/desktop-paint` imports. The next macOS agent to run the harness meets an `E0432` before it does anything useful.

**Why the existing guard cannot catch any of these.** `crates/macos-renderer/tests/typecheck_harness_guard.rs` stubs `cargo` with `exit 0`, so it proves the harness ASSEMBLES and never that the assembly COMPILES. Symbol drift walks straight through a green gate. That limitation is already recorded in `work/notes/observations/macos-typecheck-stand-in-core-drifts-unwatched-2026-07-31.md`; three occurrences is enough to stop recording it and fix it.

## Do both halves

**1. Unbreak it now (occurrence 3).** Add the three missing symbols so the harness runs clean, and RUN it to prove that, rather than reasoning about it. Check the Windows sibling `typecheck-windows-from-linux.sh` for the same class of drift while you are there.

**2. Make drift DETECTABLE on the Ubuntu gate, which is the real deliverable.** A stubbed `cargo` can never compile the scratch workspace, so do not try to make it: check the SYMBOLS instead. A cheap, dependency-free shape test can go a long way — parse the `werust_core::`-qualified paths referenced by the sources the harness symlinks in, parse the items the stand-in DECLARES, and assert the first set is contained in the second. That would have caught all three occurrences on the ordinary gate, in milliseconds, with no Mac and no cargo.

Prefer that over the two obvious alternatives, but say why you chose what you chose:

- **Generating the stand-in from the real crate** would end drift by construction, but the stand-in exists precisely because the real `werust-core` cannot cross-compile cheaply here, so generation has to strip bodies and dependencies — a real tool, not a small one. If you judge it genuinely cheap, take it; if not, say so.
- **Deleting the stand-in and cross-compiling the real core** removes the whole problem and is the honest end state, but it is a `cargo xwin`-scale change for the macOS target. Record it as the direction if you believe it, and leave it.

**Prove the teeth.** Rename a symbol in the real core locally, watch the new check go red, revert, and say in the commit that you did. This drive has now shipped three guards that could not fail; do not make it four.

## Acceptance criteria

- [ ] The macOS harness runs clean (occurrence 3 fixed), PROVEN by running it, not by reading it.
- [ ] The Windows sibling harness is checked for the same drift and fixed if present.
- [ ] A check on the ordinary Ubuntu gate FAILS when the real core gains, renames or removes a symbol the harness's symlinked sources use but the stand-in does not declare.
- [ ] That check is proven to have teeth once (deliberate rename, red, revert), and the commit says so.
- [ ] The chosen approach is recorded against the generate-it and cross-compile-the-real-core alternatives.
- [ ] `macos-typecheck-stand-in-core-drifts-unwatched-2026-07-31.md` is updated to point at the fix rather than describing an open gap.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: the macOS cross-target type-check harness's hand-written stand-in `werust-core` has drifted from the real crate THREE times in two days (the `desktop-paint` extraction deleting `paint.rs`; the `STUB_CHAIN_ID` -> `CHAIN_ID` rename; and right now, missing `trust_pin_action_label`/`trust_pin_action_visible`/`trust_pin_detail` that `crates/desktop-paint` imports, so the next macOS agent hits an `E0432`). Each break cost a task or a Gate-2 requeue. `typecheck_harness_guard.rs` cannot catch any of them because it stubs `cargo` with `exit 0`, proving the harness ASSEMBLES but never that it COMPILES. (1) Fix occurrence 3 and RUN the harness to prove it, and check `typecheck-windows-from-linux.sh` for the same class. (2) The real deliverable: make drift detectable on the Ubuntu gate WITHOUT compiling — parse the `werust_core::`-qualified paths used by the sources the harness symlinks in, parse the items the stand-in declares, and assert containment; that catches all three occurrences in milliseconds with no Mac. Weigh and record the alternatives (generating the stand-in from the real crate; deleting it and cross-compiling the real core with `cargo xwin`-scale tooling) and say why you chose what you chose. Prove the new check has teeth (rename a symbol, watch it red, revert, say so in the commit) — this drive has already shipped three guards that could not fail.
