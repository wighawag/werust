---
title: "Give the type-check harness guard teeth on BOTH branches, and sweep the docs that still point at the deleted `paint.rs`"
slug: macos-harness-guard-teeth-and-paint-path-residue
blockedBy: [macos-spike-doc-accuracy-and-harness-guard]
covers: []
---

## What to build

Four small residues of `macos-spike-doc-accuracy-and-harness-guard`, cut by the conductor at Gate-3 (2026-07-31). Same shape as its parent: doc accuracy plus test rigour, no behaviour change.

**1. The guard test only ever runs ONE of its two branches, and it is the wrong one for catching breakage.** `crates/macos-renderer/tests/typecheck_harness_guard.rs` reaches `assert_the_harness_deletes_a_scratch_dir_under_a_temp_root()` only from the `let Some(victim) = … else` fallback arm, i.e. only on a host where NO location is outside a temp root. On any ordinary host (including the gate, where `$HOME` is outside every temp root) that arm never executes, so the REFUSAL path is tested and the ALLOW path — and with it the entire repaired BODY of the harness — is never run by the gate at all. The sibling test `the_harnesss_default_scratch_dir_stays_under_a_temp_root` only string-matches the script source, so nothing in the gate ever EXECUTES the harness's happy path.

That is exactly the blind spot that let this task's own item 0 (the `desktop-paint` extraction silently breaking the harness) sit unnoticed until a reviewer read it. Make the allow-path assertion its own UNCONDITIONAL test rather than a fallback. It is already cheap (`rustup` and `cargo` are stubbed on `PATH`), and it gives the gate teeth on both halves.

**2. The harness repair is evidenced by PROSE, and its parent task asked for a run.** The parent's acceptance criterion said the repair must be proven "by running it, not by reading it". What landed is a sentence in the spike README and `DECISIONS.md` saying it ran clean on 2026-07-31 against `aarch64-apple-darwin`; no transcript is committed and nothing in CI runs the harness. This repo pins recorded runs against committed evidence elsewhere (`crates/windows-origin-probe/tests/recorded_verdict.rs`, `crates/macos-origin-probe/tests/recorded_verdict.rs`). Close the gap the cheap way: with item 1 done, the gate EXECUTES the harness on every run, which is stronger than a committed transcript and needs no Mac. Then correct the prose so it claims what is actually enforced. If you also want a committed transcript, name the run that produced it.

**3. The probe directory can collide with itself.** `a_probe_dir_outside_every_temp_root()` writes `$HOME/.werust-typecheck-harness-guard-probe`, a FIXED name, while the sibling helper correctly suffixes `std::process::id()`. Two concurrent gate runs sharing a `HOME` (two worktrees, a parallel gate) will delete each other's victim mid-assertion, producing a failure that looks exactly like the guard breaking. Add the same process-id suffix. Also record, in the test's own module doc, WHY it writes into the real `$HOME` at all (it needs a path provably outside every temp root, cleanup is unconditional) so the next reader does not file it as a shared-write violation; ratified at Gate-3, but ratified silently is not ratified.

**4. Docs still send readers to a file that no longer exists.** `windows-win32-window-and-chrome` deleted `crates/werust-macos/src/paint.rs` when it extracted `crates/desktop-paint`, and several committed docs still name that path: `.github/workflows/macos-renderer.yml` (header comment), `docs/spikes/macos-appkit-window-and-chrome/README.md` (three places), and the harness's own comment in `typecheck-macos-from-linux.sh`. One of those is in a file the parent task edited, which is the tell that a targeted fix missed a sweep. Repoint them all at `crates/desktop-paint`. This is the same defect class the parent task exists to fix — a doc that describes a tree that is gone — so it belongs here rather than being left for the next macOS agent to trip over.

**Scope:** one test restructure, one process-id suffix, one doc-comment addition, one path sweep, and the prose correction that follows. No change to the harness's behaviour, no change to what the leg builds.

## Acceptance criteria

- [ ] The harness's ALLOW path is asserted by an UNCONDITIONAL test, so every ordinary gate run EXECUTES the repaired harness body rather than only string-matching it.
- [ ] The refusal path keeps its current teeth on hosts where a non-temp location exists, and the two tests do not depend on each other's environment.
- [ ] The `$HOME` probe directory carries a process-id suffix, so concurrent runs sharing a `HOME` cannot delete each other's victim.
- [ ] The test's module doc says why it writes into the real `$HOME` and that cleanup is unconditional.
- [ ] No committed doc, comment or script names `crates/werust-macos/src/paint.rs`; every reference points at `crates/desktop-paint`.
- [ ] The claim about the harness being verified matches what is actually enforced.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: four residues of `macos-spike-doc-accuracy-and-harness-guard`. (1) `crates/macos-renderer/tests/typecheck_harness_guard.rs` reaches its ALLOW-path assertion only from the `let Some(victim) = … else` fallback, so on any ordinary host (the gate included) only the REFUSAL path runs and the repaired harness body is never EXECUTED by CI — the same blind spot that let the `desktop-paint` breakage sit unnoticed. Make the allow-path assertion an unconditional test of its own; it is already cheap because `rustup`/`cargo` are stubbed on `PATH`. (2) With that done, the gate really runs the harness, which is stronger evidence than the prose claim that currently backs the parent's "prove it by running it" criterion — correct the README/DECISIONS prose to claim what is enforced. (3) `a_probe_dir_outside_every_temp_root()` uses a FIXED `$HOME/.werust-typecheck-harness-guard-probe`, so two concurrent runs sharing a HOME delete each other's victim; give it the `std::process::id()` suffix its sibling helper already uses, and say in the module doc why writing to the real `$HOME` is necessary and safe. (4) Sweep the docs that still name the deleted `crates/werust-macos/src/paint.rs` (`macos-renderer.yml` header, `docs/spikes/macos-appkit-window-and-chrome/README.md` in three places, and `typecheck-macos-from-linux.sh`'s own comment) and repoint them at `crates/desktop-paint`. No behaviour change to the harness.
