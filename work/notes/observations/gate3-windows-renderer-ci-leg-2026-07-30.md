---
title: "Gate-3 verdict: windows-renderer-ci-leg (APPROVE) — the leg is green because it RAN, not because it type-checked"
date: 2026-07-30
status: open
reviewOf: windows-renderer-ci-leg
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main` as `d4692ff` (the build) + `ea232c9` (the re-stamp after the measurement). Conductor's own diff-vs-criteria pass, third layer after the acceptance gate (Gate 1) and the PR/code review gate (Gate 2).

This task was blocked once at Gate 2, correctly, and the block is the most valuable thing that happened here: see "The recovery" below.

## Criteria, ticked against the merged diff

1. **`.github/workflows/windows-renderer.yml` exists, runs on `windows-latest`, has `workflow_dispatch`.** MET. All three are also PINNED by `crates/werust-core/tests/windows_renderer_leg_shape.rs`, which matters more than the file itself: `workflow_dispatch` is not a convenience on this leg, it is the deliverable, and a future edit that dropped it would silently re-open the prediction trap this task exists to close.
2. **The leg is GREEN as landed; excluded crates named with reasons.** MET, and met by MEASUREMENT rather than argument, which is the whole point. Two `windows-latest` runs on 2026-07-30: [30581522002](https://github.com/wighawag/werust/actions/runs/30581522002) (`main`, push-triggered, 441 tests) and [30581549437](https://github.com/wighawag/werust/actions/runs/30581549437) (this branch, `workflow_dispatch --ref`, 448 tests). Zero failures, zero ignored. Exclusions are each named with a reason and, unusually, with a MEASUREMENT behind the reason: every workspace member was cross-checked with `cargo xwin`, so `werust`/`webview-renderer` are excluded as measured-red on pkg-config, the cfg-gated platform crates as asserting nothing their own legs do not, and `native-renderer`/`script-engine` on COST rather than redness. Naming cost as cost, instead of dressing it as a technical impossibility, is the honest version.
3. **The runtime version is recorded, reusing the probe's step rather than a second implementation.** MET, and exceeded: instead of copying the step, the build lifted it into a shared composite action `.github/actions/webview2-runtime-version` and pointed BOTH Windows legs at it. See the ratify note below; I am approving the widening.
4. **Narrowest honest `pull_request` filter, trade-off stated in the header.** MET. The PR trigger carries only `webview-shared`, `windows-origin-probe`, the action and the workflow; `werust-core`, `fetcher` and `renderer` are caught on `push` to `main` instead. The header states the cost in one sentence ("a core change that breaks the Windows build is found minutes after it merges rather than before, on a leg that gates nothing") and both halves are pinned by the shape test, so broadening it later has to be a decision rather than a reflex. This is deliberately NOT the macOS leg's shape, and it is now the sharpest available data point for the open question about `macos-renderer.yml`'s `crates/werust-core/**` PR trigger.
5. **Shape test in the `release_plumbing_shape.rs` style, no new dependency.** MET. 377 lines, 7 tests, no `Cargo.toml` change in the diff.
6. **`cargo fmt --check && cargo clippy && cargo build && cargo test` green.** MET (Gate 1, on the rebased-onto-main tree via the fresh-worktree gate).

Nothing in the task carried a drift note or forward-pointer to honour beyond the "do not ship a prediction" instruction, which is the one the recovery below is about.

## The recovery: a build agent cannot measure its own CI leg

Gate 2 blocked round 1 because the evidence for criterion 2 was a `cargo xwin check --tests` sweep: it type-checks and runs build scripts, but it does not link and it runs ZERO tests. The reviewer named it as the same prediction-instead-of-measurement gap that bounced both macOS tasks, and it was right.

The agent could not have closed that from inside its build. `gh workflow run --ref` is refused while the workflow is absent from the default branch, and the PR route failed too: PR #3 could not compute a merge ref, because the runner's own stuck-surface commits on `main` touched the very task file the branch moves `backlog/ -> done/`. That is the conflict shape the conductor was warned about, observed for real.

What worked, and is now the third confirmation of this loop: the conductor landed the workflow and the composite action on `main` as byte-identical copies (`c9e7430`, a chore commit, explicitly NOT the task's landing), which made the push-triggered run fire immediately AND made dispatch-by-ref legal; then dispatched the branch's own tree; then `dorfl requeue --reconcile -m "<the measured result, verbatim>"` and re-dispatched. The agent re-stamped the README from the recording and Gate 2 approved. Note the ordering discipline that made it safe: the measurement was obtained BEFORE the `-m` note was written, so the note committed to `main` exactly once and the reconcile rebase was clean.

Worth stating plainly, because it generalises past Windows: **for any task whose acceptance criterion is a CI measurement, the measurement is the conductor's job, not the build agent's.** The agent can build the leg and can re-stamp a recording; it structurally cannot obtain one. Three tasks have now paid one extra round trip each to rediscover that.

## Review-nit triage (5 raised at Gate 2, all non-blocking)

- **D2, the composite-action lift (scope widening).** RATIFIED, approved. The task said "one workflow file, one shape test" and the build edited a second workflow (`windows-origin-probe.yml`) to consume a shared action. That is a widening, and it is the right one: the alternative is the same registry GUID transcribed in two places, which is the exact class of duplication this repo has been paying down all week. The cross-leg coupling (gate-0's workflow is now pinned by this task's test) is a feature: the assertion is that the GUID lives in the action and in NEITHER workflow, so a regression to a copy fails the Ubuntu gate. Flagged to the human as a ratify item, not a question.
- **D1, the narrow PR filter.** Kept as built, and PROMOTED to the human's batch, because it is the same question as the `macos-renderer.yml` cost, one level down. This leg now stands as the worked counter-example: narrow PR filter + broad push filter + `workflow_dispatch`.
- **D3, `core.autocrlf false` before checkout.** Kept. The run proves it sufficient (all source-parsing shape tests passed on Windows). The residual is real but small and correctly scoped out: the CRLF fragility of the `*_shape.rs` tests is unowned for any FUTURE Windows job that forgets the step. Not worth a task today; worth a line in the next Windows task, which is where a second Windows job would be added. Not planted as a forward-note because the shape test's own failure would be legible.
- **The `GREEN_ON_WINDOWS` coupling.** Genuinely load-bearing and NOT recorded where the next agent would see it, so I planted it as a forward-note in `work/tasks/backlog/windows-webview2-renderer-backend.md`: the backend task cannot merely add its crate to the workflow, it must update the constant and the push filter in the same change or the Ubuntu gate goes red. This is the one nit that would have cost a round trip if left in an observation file.
- **Housekeeping: `needsAnswers: true` on a done task + a stale questions sidecar.** Left as found (precedented: five other done tasks carry it). The sidecar's FIVE duplicate entries are a separate finding, filed as `dorfl-surface-retry-duplicates-questions-2026-07-30.md`.

## What this unlocks

`windows-webview2-renderer-backend` is now buildable AND measurable: the workflow is on `main`, so whoever drives it can dispatch the leg against the work branch. `windows-win32-window-and-chrome` follows it. Neither has an excuse to ship a prediction.
