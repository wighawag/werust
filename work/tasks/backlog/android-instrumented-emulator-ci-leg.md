---
title: "Android gets an emulator-backed instrumented CI leg that runs the probes that already exist, on a pinned image"
slug: android-instrumented-emulator-ci-leg
spec: android-instrumented-ci-leg
blockedBy: []
covers: [1, 2, 4, 5]
---

## What to build

`.github/workflows/` holds `verify`, `macos-renderer`, `windows-renderer`, `windows-origin-probe`, `mobile-ios` and `release`. Nothing runs an emulator or a single instrumented test; the release leg builds an APK and drives nothing. So Android on-device behaviour can only ever be a FIELD REPORT, and it already has been twice: `localStorage` being `null` was found by a human noticing it on a phone, and trust decisions not persisting was found the same way in v0.4.0 use. Android is also werust's most divergent edge, the one that does NOT serve `ipfs://` as the document origin.

Add `.github/workflows/android-instrumented.yml` (the filename is pinned by the spec because other specs write acceptance criteria naming it) which boots an Android emulator and runs the `androidTest` sources that ALREADY exist: the DOM-storage probe and the SPA client-navigation origin probe, both of which are network-isolated (they serve their pages through the WebView's own intercept) and are hand-run today with `./gradlew :app:connectedDebugAndroidTest`.

Scope is deliberately narrow: **the leg, not new coverage.** Do not add assertions, do not change the probes' expectations, do not touch the app. Introducing a harness and new assertions together makes a red result ambiguous, and that ambiguity is the whole reason the first landing runs only what exists.

Four properties the leg must have, each for a stated reason:

- **The system image and API level are PINNED and NAMED IN THE OUTPUT.** Android WebView behaviour is version-dependent, and one of the probes asserts the SHIPPED DEFAULT behaviour of the platform (that `window.localStorage` is `null` with `WebSettings` exactly as Android ships them), so a different image can legitimately change the result. The recorded hand-run measurements were taken on an emulator at API 36 with System WebView 142 (`docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`): start from that, and print the image, the API level and the System WebView build in the job output so a reader of a red build can tell an emulator-specific result from a real regression.
- **The trigger is CHOSEN, with its cost recorded.** An emulator boot plus an instrumented run is minutes, not seconds. Follow how the macOS and Windows legs already handle expensive legs (`workflow_dispatch` plus a path-filtered push, or a schedule) and write down what you chose and what it costs. Do not default it onto every push.
- **A skipped run must not report success.** A CI leg that goes green without running anything is worse than no leg. Demonstrate that inverting one existing assertion actually FAILS the build, and make the job fail if zero tests ran.
- **It must NOT put `WERUST_VERSION` in the Gradle environment.** The release leg exports it deliberately (it versions the APK manifest), but the app module's Gradle script hard-fails when `WERUST_VERSION` is present and does not fold to a clean `major.minor.patch` triple, and a reused Gradle daemon can carry an earlier shell's value. Copy the release leg's SDK/NDK/target setup, not its `env:` block. See `work/tasks/backlog/android-version-guard-teeth-and-the-stale-daemon-trap.md`, which is sharpening exactly that failure and its message.
- **It must not disturb the pure-Rust gate.** `crates/werust-core/tests/toolchain_pin_shape.rs` parses EVERY YAML file under `.github/` and reds if any step re-selects a toolchain (`rustup component add`/`default`/`override`/`toolchain install`, a `cargo +…` proxy call, or a toolchain-selecting action); `rustup target add <triple>` is fine and is how the release leg installs the Android targets. `verify_gate_shape.rs`, `release_plumbing_shape.rs` and `windows_renderer_leg_shape.rs` parse specific workflow files: do not edit those files.

Add a shape guard for this leg in the repo's existing idiom (a `crates/werust-core/tests/` test that PARSES the workflow), pinning the properties above that are properties of text: the pinned image/API, the deliberate trigger, that the leg runs the connected-Android-test task, and that it does not re-select a toolchain. That is how `windows_renderer_leg_shape.rs` guards its leg, and it is what keeps this leg from silently rotting.

**The acceptance hazard, stated so it cannot be papered over.** Every per-edge workflow here triggers on push to `main`, and `workflow_dispatch` only becomes available once a workflow is ON the default branch, so a leg authored on a work branch CANNOT produce a green run before it lands. `CONTEXT.md` ratifies both halves of this: the leg must be GREEN THE DAY IT LANDS, and when a criterion still ends up unmeasured, obtaining the measurement is the CONDUCTOR's job, not the build agent's. So do not mark this done claiming a green CI run you cannot have. State which evidence path you took, in the done record, from these: a local emulator run of the same commands the leg runs (recording the image, API level and WebView build); a run on a fork or a temporary branch that IS a default branch; or explicitly handing the first-run measurement to the conductor with the exact dispatch command to use after it lands. "It should work" is not one of the options.

## Acceptance criteria

- [ ] `.github/workflows/android-instrumented.yml` exists, boots an emulator on a PINNED system image and API level, and runs the EXISTING `androidTest` sources with no new assertions and no change to the probes' expectations.
- [ ] The job output names the system image, the API level and the System WebView build it ran against, so a red result is attributable.
- [ ] The trigger is deliberate (not every push) and the choice plus its cost are recorded in the workflow's own comments, following the macOS/Windows leg precedent.
- [ ] The leg does not set `WERUST_VERSION` (the app module's Gradle script hard-fails on a value that does not fold to a clean triple, and the release leg's `env:` block is the tempting thing to copy).
- [ ] The job FAILS if zero instrumented tests ran, and a deliberately inverted assertion is shown to red the build (state where and how you demonstrated it).
- [ ] A new shape guard under `crates/werust-core/tests/` parses the leg and pins the image/API, the trigger, the connected-Android-test invocation and the absence of toolchain re-selection, in the style of `windows_renderer_leg_shape.rs`.
- [ ] The three existing workflow-parsing guards (`verify_gate_shape.rs`, `release_plumbing_shape.rs`, `windows_renderer_leg_shape.rs`) are untouched and green, and `toolchain_pin_shape.rs` stays green over the new YAML (`rustup target add` only, no toolchain selection).
- [ ] `verify` is unchanged: this is an additional platform leg, not a change to the acceptance bar.
- [ ] The done record names the EVIDENCE PATH actually taken for the first run (local emulator run with the recorded image/API/WebView build, a default-branch run elsewhere, or an explicit hand-off of the first dispatch to the conductor with the command to run), and does not claim a CI green that does not exist.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- None: can start immediately.

## Prompt

> Goal: give Android an emulator-backed instrumented CI leg at `.github/workflows/android-instrumented.yml` that runs the instrumented tests that ALREADY exist, so Android on-device behaviour can be an acceptance criterion instead of a field report. Read `work/specs/tasked/android-instrumented-ci-leg.md` for the framing.
>
> What exists: `crates/werust-android/app/src/androidTest/.../WebStorageTest.kt` and `SpaClientNavOriginTest.kt`, both network-isolated (they serve their pages through the WebView's own request intercept), both hand-run today with `cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest`. The app module already declares the AndroidJUnit runner and the `androidTest` dependencies. `release.yml`'s `android-apk` job shows how this repo sets up JDK 17, the Android SDK/NDK and the Android Rust targets, and how it caches cargo; reuse that shape rather than inventing one.
>
> Run ONLY what exists. Do not add assertions or touch the app: a red first run must mean the harness is wrong, not the assertions.
>
> PIN the system image and API level and PRINT them (with the System WebView build) in the job output. This is not hygiene: one probe asserts the platform's SHIPPED DEFAULT (that `window.localStorage` is `null` with untouched `WebSettings`), so a different WebView build can legitimately flip it. The recorded hand-run values are emulator, API 36, System WebView 142 in `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`.
>
> CHOOSE the trigger deliberately and write the cost down in the workflow's comments (see how `macos-renderer.yml` and `mobile-ios.yml` justify theirs: `workflow_dispatch` plus a path-filtered push). Make the job fail when zero tests ran, and prove a red is possible by inverting one assertion locally.
>
> One trap when you copy from `release.yml`: take its JDK/SDK/NDK/targets/caching setup, but NOT its `env:` block. `WERUST_VERSION` present in the Gradle environment makes the app module's version resolution hard-fail unless it folds to a clean `major.minor.patch` triple, and a reused Gradle daemon can carry an earlier shell's value (`work/tasks/backlog/android-version-guard-teeth-and-the-stale-daemon-trap.md` is sharpening that failure's message). An instrumented run needs no version injection at all.
>
> Do not break the pure-Rust gate: `crates/werust-core/tests/toolchain_pin_shape.rs` parses EVERY `.github/**/*.yml` and forbids any toolchain re-selection (`rustup component add`/`default`/`override`/`toolchain install`, `cargo +…`, toolchain-selecting actions); `rustup target add <triple>` is allowed and is what the release leg uses. Leave `verify.yml`, `release.yml`, `windows-renderer.yml` and their three shape-guard tests alone. Add YOUR leg's shape guard as a new test under `crates/werust-core/tests/`, in the style of `windows_renderer_leg_shape.rs`, pinning the image/API, the trigger and the test invocation.
>
> The acceptance hazard you must handle honestly: `workflow_dispatch` is only available once a workflow is on the DEFAULT branch, so this leg cannot produce a green run from a work branch. `CONTEXT.md` says the leg must be green the day it lands, and that obtaining an unobtainable measurement is the conductor's job. So state in the done record which evidence path you took (a local emulator run with the image/API/WebView build recorded; a run on a fork/branch that is itself a default branch; or an explicit hand-off with the exact dispatch command for after it lands). Do NOT claim a CI green you cannot have, and do not land a red workflow.
>
> Out of scope: new instrumented coverage (the specs that need it own their cases), a macOS-style interactive smoke, any change to `verify`, and iOS.
