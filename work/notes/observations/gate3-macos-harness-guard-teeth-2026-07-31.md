---
title: "Gate-3 verdict: macos-harness-guard-teeth-and-paint-path-residue (APPROVE) — the gate now runs the harness, and the macOS PR filter is narrowed as ratified"
date: 2026-07-31
status: open
reviewOf: macos-harness-guard-teeth-and-paint-path-residue
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. This one carried the human's ratified answer to the macOS CI-cost question, and it is now live: **`crates/werust-core/**` is GONE from `macos-renderer.yml`'s `pull_request` filter.** Core work no longer spends `macos-14` minutes on every PR, the leg still catches a macOS break on `push` to `main` minutes after it merges, and `workflow_dispatch` covers the deliberate case. The choice is pinned by an exact-set test, so the next widening must edit a test rather than accrete — which matters, because this filter had drifted wider twice in three tasks.

## Criteria, ticked

1. **The harness's ALLOW path is asserted UNCONDITIONALLY, so every gate run EXECUTES the repaired harness.** MET, and improved on: the test also asserts the assembled scratch workspace contains NO DANGLING SYMLINK. That addition is what makes the claim honest, and the agent explained exactly why — `rustup` and `cargo` are stubbed, and `ln -s` is perfectly happy to point at a deleted file, so the precise failure that broke this harness (`windows-win32-window-and-chrome` deleting `paint.rs` while the harness went on symlinking it) only reds at `cargo check` time, which a stub never reaches. Executing the script proves it ASSEMBLES; asserting the assembly RESOLVES is what proves the gate would have caught the real breakage. Verified by re-pointing the symlink at the deleted path and watching it red.
2. **The refusal path keeps its teeth where a non-temp location exists, and the two tests no longer depend on each other's environment.** MET.
3. **The probe directory carries a process-id suffix.** MET (with a side effect, below).
4. **The module doc explains the real-`$HOME` write and its unconditional cleanup.** MET.
5. **No committed doc, comment or script names the deleted `paint.rs`.** MET IN SPIRIT, deliberately NOT in letter — and this is the best judgement call in the diff. Five mentions remain by design: two in a HISTORICAL spike's decisions (annotated with a dated path note rather than rewritten), one `windows_window_shape.rs` assertion that the path does NOT exist, and two doc comments narrating the extraction. The agent's reasoning: taken literally, the criterion "would have me falsify a transcript and delete a working assertion", while its PURPOSE is that no reader is sent to a file that is gone — which annotation satisfies. It flagged the deviation rather than burying it. That is exactly what I want an agent to do with a criterion I wrote imprecisely.
6. **The macOS PR filter narrowed, pinned, with the trade-off in the header.** MET.

## Residues, cut as `macos-harness-doc-and-probe-hygiene-residue` — and I am ending the chain there

- **A stale in-crate pointer the sweep could not see**: `macos_window_shape.rs` still tells readers that display-value assembly lives in that crate's `src/paint.rs`. It escaped because it names the BARE file, not the full path my criterion grepped for. In a test a future macOS agent reads to learn where things live.
- **The corrected prose overclaims on some hosts**: the README now says every run executes the harness twice including the refusal, but where no location is outside a temp root that half is skipped with a note invisible without `--nocapture`. The same class, one level down.
- **Probe dirs now ACCUMULATE under the real `$HOME` after a crash** — a side effect of the pid suffix I prescribed. The old fixed name was reused and overwritten; distinct names orphan. My fault, and worth fixing properly rather than reverting the collision safety.

**Chain note:** each round of this residue chain has been legitimate but smaller than the last. I have told that task it is the last one; further micro-items should be batched for a human rather than chained into a sixth follow-on.

## Ratified

- **The two PR-filter pins now have different homes and readers** (`macos_backend_shape.rs` reading whole list items, `windows_renderer_leg_shape.rs` parsing YAML with `serde_yaml`), with matching concept and const names. The stated reason — not adding a dev-dependency purely for symmetry — is sound, and the vocabulary is coherent even though the mechanism is not identical.
- **The no-candidate host now skips the refusal test with a note rather than standing in for the allow assertion.** That is what my independence criterion asked for; the cost (a silently unexercised half on such hosts) is covered by residue 2 above.
