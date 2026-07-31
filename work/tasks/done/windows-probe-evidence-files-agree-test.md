---
title: "Assert the Windows probe's two committed evidence files against each other in the ordinary gate, so the pinned verdict and the verbatim run cannot drift apart"
slug: windows-probe-evidence-files-agree-test
blockedBy: []
covers: []
---

## What to build

A small gap found at Gate-2/Gate-3 of `windows-ipfs-origin-probe-on-ci`.

That task committed two evidence files: `docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json` (the PINNED baseline the Windows workflow asserts against) and `probe-report-2026-07-30.json` (the VERBATIM run that produced the verdict). Nothing in the Ubuntu `verify` gate compares them. The 23 host-independent tests in `crates/windows-origin-probe` build `Expectations` in memory only, so a later edit to either file can make the pinned verdict and its evidence disagree, and the only thing that would notice is a Windows runner, which is exactly the runner this repo does not have on every push.

> DRIFT UPDATE (conductor, 2026-07-31): **the macOS sibling already built this test, and it is the model to follow.** `crates/macos-origin-probe/tests/recorded_verdict.rs` does exactly what this task asks, for the WebKit probe: it loads `expected.json` and `probe-report-2026-07-30.json` from the spike directory and replays the probe's own `Expectations::diff` on the Ubuntu gate. It also does two things this task's text did not think to ask for, and you should copy both: it asserts the report is a REAL run's output (it carries the OS build and the engine's user agent the run measured), and it asserts the recorded provenance NAMES the CI run so a reader can go and look. Its module doc states the failure mode plainly — "a plausible, carefully argued PREDICTION written from Linux and committed in the slot a MEASUREMENT belongs in" — which is this repo's actual history, twice. Follow that file's shape rather than inventing a second one, and keep the Windows-specific difference in mind: the Windows probe's evidence pair was committed by `windows-ipfs-origin-probe-on-ci`, which the macOS one was explicitly modelled on, so the two should end up symmetrical.

Add one host-independent test that loads BOTH committed files and asserts `Expectations::diff` between them is empty. It runs in the ordinary gate on Ubuntu, needs no WebView2, and closes the drift.

**Keep the failure legible.** If the two ever disagree, the message should name the field that moved and say plainly which file is which (the baseline versus the recorded run), because the reader's next question is always "which one is wrong?".

**Scope discipline:** this is one test plus whatever tiny loader it needs. Do not restructure the probe, do not add a dependency for JSON if the crate already has one, and do not touch the Windows-only half.

## Acceptance criteria

- [ ] A test in the ordinary Ubuntu `verify` gate loads the committed `expected.json` and `probe-report-2026-07-30.json` and asserts they agree (an empty `Expectations::diff`).
- [ ] The failure message names the differing field and identifies which file is the pinned baseline and which is the recorded run.
- [ ] The test has teeth: a deliberate mismatch fails it (prove it once during development, then revert).
- [ ] No new dependency, no change to the Windows-only half, no restructuring of the probe.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: close a drift hole in the Windows probe's evidence. `docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json` (the pinned baseline the Windows workflow asserts against) and `probe-report-2026-07-30.json` (the verbatim run behind the recorded verdict) are both committed, but nothing in the Ubuntu gate compares them, so they can silently drift and only a Windows runner would notice. Add ONE host-independent test that loads both committed files and asserts `Expectations::diff` is empty, with a failure message naming the differing field and saying which file is the baseline and which is the recorded run. Prove it fails on a deliberate mismatch, then revert that. One test, no new dependency, no touching the `#[cfg(windows)]` half.
