---
title: "Last sweep of the macOS harness chain: a stale in-crate pointer, a README that overclaims on some hosts, and probe dirs that accumulate on a crash"
slug: macos-harness-doc-and-probe-hygiene-residue
blockedBy: [macos-harness-guard-teeth-and-paint-path-residue]
covers: []
---

## What to build

Three small residues of `macos-harness-guard-teeth-and-paint-path-residue`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). All three are the same class as its parent (a claim that outlived the code), plus one hygiene side effect the conductor's own prescription introduced.

**This is intended as the LAST task in this chain.** Each round of this residue chain has been legitimate but smaller than the last; if the review of THIS task surfaces further micro-items, batch them for a human to judge rather than cutting a sixth follow-on.

**1. A stale in-crate pointer the path sweep could not see.** `crates/werust-macos/tests/macos_window_shape.rs` line 20 tells the reader that everything assembling a display value lives in `src/paint.rs`, and the comments near lines 206-208 repeat it — but that crate's `src/` now holds only `lib.rs`, `main.rs` and `window.rs`, because the painter moved to `crates/desktop-paint`. It escaped the sweep because it names the BARE file, not the full path the criterion grepped for. Repoint it (or annotate it the way the historical spike was annotated). Worth doing precisely because it is in a TEST that a future macOS agent reads to learn where things live.

**2. The corrected prose still overclaims, on some hosts.** `docs/spikes/macos-wkwebview-renderer-backend/README.md`'s local-type-check bullet now says every ordinary Ubuntu `verify` run EXECUTES the harness twice, once with a `SCRATCH_DIR` outside every temp root which it must REFUSE. On a host where nothing qualifies (`TMPDIR` or `HOME` itself under a temp root, as in some containers) that first run does not happen at all — the refusal half is skipped with a note that is invisible without `--nocapture`. Add the clause: the refusal half requires a location outside every temp root. This task's own parent exists because a doc claimed a check the tool did not perform; the correction should not repeat it one level down.

**3. Probe directories now accumulate under the real `HOME` after an abnormal exit.** The pid suffix (which the conductor asked for, to stop concurrent runs deleting each other's victim) means a run killed by ctrl-C or a gate timeout leaves a DISTINCT hidden directory behind, where the previous fixed name was simply reused and overwritten next time. Cleanup is unconditional on the normal path, so this is crash residue only — but it grows rather than being self-limiting, in the developer's real home directory. Make it self-limiting: reap stale sibling probe directories on startup (they are identifiable by the shared prefix), or use a location that a crash cannot orphan. Keep the collision-safety the suffix bought.

**Scope:** two doc/comment corrections and one cleanup improvement in a test helper. No change to the harness, the guard's behaviour, or either workflow.

## Acceptance criteria

- [ ] `crates/werust-macos/tests/macos_window_shape.rs` no longer tells a reader that display-value assembly lives in that crate's `src/paint.rs`.
- [ ] The macOS spike README's local-type-check bullet states that the refusal half requires a location outside every temp root.
- [ ] Probe directories under the real `HOME` do not accumulate across abnormally terminated runs, and concurrent runs still cannot delete each other's victim.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: the last sweep of the macOS harness residue chain, three small items. (1) `crates/werust-macos/tests/macos_window_shape.rs` (line ~20 and ~206-208) still tells the reader that everything assembling a display value lives in `src/paint.rs`, but that crate's `src/` now holds only `lib.rs`, `main.rs` and `window.rs` since the painter moved to `crates/desktop-paint`; it escaped the previous sweep because it names the bare file rather than the full path. Repoint or annotate it — it matters because a future macOS agent reads that test to learn where things live. (2) `docs/spikes/macos-wkwebview-renderer-backend/README.md` now says every Ubuntu `verify` run executes the harness TWICE including a refusal run, but on a host where no location is outside a temp root the refusal half is skipped entirely (with a note invisible without `--nocapture`); add that clause, because the parent task exists precisely because a doc claimed a check the tool did not perform. (3) The pid suffix on the `$HOME` probe directory (added to stop concurrent runs deleting each other's victim) means an abnormally terminated run now ORPHANS a distinct hidden directory instead of reusing one name, so they accumulate in the developer's real home; make it self-limiting — reap stale siblings by their shared prefix on startup, or pick a location a crash cannot orphan — while keeping the collision safety.
