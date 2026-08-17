---
title: "Document how to add an Android instrumented case, so a spec can name the leg in its acceptance criteria"
slug: document-how-to-add-an-android-instrumented-case
spec: android-instrumented-ci-leg
blockedBy: [android-instrumented-emulator-ci-leg]
covers: [3]
---

## What to build

A leg nobody knows how to add a case to gets one case. The point of the Android instrumented leg is that a future task can write "asserted by an instrumented case on `.github/workflows/android-instrumented.yml`" and an agent with no conversation history can then do exactly that. Write that down, where an author of the next task will actually look.

The document answers, concretely and against the leg that landed:

- **Where a case goes** (the `androidTest` source set of the app module), and what the two existing probes establish as the house style: a raw WebView driven from the instrumentation, pages served through the WebView's own request intercept so the case is NETWORK-ISOLATED, a latch-and-timeout rendezvous instead of a sleep, and a `tearDown` that destroys the probe.
- **How to run it locally** (the Gradle connected-test invocation, and what you need installed), and how to run it in CI (the leg's trigger, and the dispatch command).
- **What the leg guarantees and what it does not.** It is NOT the acceptance gate: `verify` (pure Rust, Linux-only) remains the bar, and this is an additional platform leg like the macOS and Windows ones. A criterion that needs it must NAME it, the way existing per-edge tasks name their legs.
- **What a case must NOT do**: assert the platform's shipped defaults without pinning why the image matters, touch the developer's real settings or pin store, need the network, or rely on a wall-clock sleep.
- **How to record a measurement**, following the convention this repo already has (a spike folder with the image, API level and System WebView build alongside the numbers, as `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md` does), so a version-dependent result stays attributable later.

Put it where the leg lives rather than in a new parallel place: the Android crate's own README plus a spike folder for the leg are both existing conventions in this repo, and `CONTEXT.md`'s per-edge rules are where a task author looks for what a criterion may name. Choose deliberately, keep it to ONE home, and cross-link rather than duplicating (a second copy is a copy to drift).

## Acceptance criteria

- [ ] A single document explains where an instrumented case goes, how to run it locally, how it runs in CI, and how to name the leg in an acceptance criterion.
- [ ] It states plainly that the leg is NOT the acceptance gate and that `verify` is unchanged.
- [ ] It names the house style the two existing probes establish (network-isolated pages through the WebView's own intercept, latch-and-timeout not sleep, teardown) and the things a case must not do (real settings/pin store, network, wall-clock sleep).
- [ ] It says how to record a version-dependent measurement (image, API level, System WebView build) and where.
- [ ] It quotes the leg's ACTUAL trigger and system image rather than a guess, and a reader following it end to end reaches a runnable case (verified by following your own instructions once and saying so in the done record).
- [ ] There is exactly ONE home for this, cross-linked from the obvious other places rather than copied.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green (the pure-Rust gate must stay green even though this task is documentation).

## Blocked by

- `android-instrumented-emulator-ci-leg`: the document must quote the leg that actually landed (its trigger, its pinned image, its invocation), not a prediction of it.

## Prompt

> Goal: write the "how to add an Android instrumented case" document, so a future spec can put "asserted by an instrumented case on `.github/workflows/android-instrumented.yml`" in an acceptance criterion and the agent that picks that task can do it from the document alone. Read `work/specs/tasked/android-instrumented-ci-leg.md` (story 3) and the done record of `android-instrumented-emulator-ci-leg` first, then read the leg itself: quote what LANDED (trigger, pinned system image and API level, the Gradle invocation), never a prediction.
>
> The house style already exists in two files worth reading before you describe it: `crates/werust-android/app/src/androidTest/.../WebStorageTest.kt` and `SpaClientNavOriginTest.kt`. Both drive a raw WebView from the instrumentation, serve their pages through the WebView's own request intercept so no network is needed, use a latch with a timeout rather than a sleep, and destroy the probe in teardown. Both also record their measurements in a spike folder with the emulator image, API level and System WebView build, because Android WebView behaviour is version-dependent.
>
> Say clearly what the leg is NOT: it is not the acceptance gate. `verify` (pure Rust, Linux-only: fmt, clippy with warnings denied, build, test) stays the bar, and this is an additional platform leg like the macOS and Windows ones, which a criterion must NAME to rely on.
>
> Pick ONE home and cross-link instead of copying: the Android crate README (`crates/werust-android/README.md`) and a spike folder for the leg are both existing conventions, and `CONTEXT.md` is where a task author looks for what a criterion may name. A duplicate is a copy to drift, which this repo has been bitten by before.
>
> Then follow your own instructions once, end to end, and say in the done record that you did and what you found (that is the only way to know the document works). Do not add new instrumented coverage as part of this task: adding cases belongs to the specs that need them.
