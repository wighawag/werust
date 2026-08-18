---
title: "Android gets an instrumented CI leg, so on-device behaviour can be an acceptance criterion instead of a field report"
slug: android-instrumented-ci-leg
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.
> Tasked 2026-08-17 (`to-task`): the Implementation / Testing detail this spec carried moved INTO the tasks it emitted (`work/tasks/`, carrying `spec: android-instrumented-ci-leg`), which are the current truth for what to build. Durable rationale is recorded as ADRs by the tasks that decide it, not predicted here.

## Problem Statement

**Android has no CI leg of any kind.** `.github/workflows/` holds `verify`, `macos-renderer`, `windows-renderer`, `windows-origin-probe`, `mobile-ios` and `release`; nothing runs `connectedAndroidTest`, an emulator, or any instrumented test. `release.yml` builds an APK but drives nothing.

So Android on-device behaviour can only ever be a FIELD REPORT. The consequences are already in the repo's history:

- `android-enable-dom-storage-and-guard-web-platform-parity` had to be discovered by a human noticing `localStorage` was `null` on a phone while it worked on GTK, and its probe (`WebStorageTest.kt`) is hand-run.
- Trust decisions not persisting on Android was found the same way, in v0.4.0 field use.

`CONTEXT.md` ratifies the rule this violates: a CI-measurable criterion needs its CI leg on `main` FIRST, or the tasks that depend on it each pay an extra round trip. Several pending specs are blocked by exactly that: `settings-location-on-every-edge`'s "a trust decision survives a restart" has no leg to name, and `explore-name-as-origin`'s Android column cannot be measured.

Android is also werust's most divergent edge — it is the one that does NOT serve `ipfs://` as the document origin, mapping loads to an internal `https://<cid>.ipfs.werust.invalid` host instead — so it is the edge where "the engine surely does the right thing" has already been proven wrong once.

## Solution

An emulator-backed instrumented-test job in **`.github/workflows/android-instrumented.yml`** — the filename is pinned here because three other specs write acceptance criteria that must name a real workflow file, alongside `macos-renderer.yml` and `windows-renderer.yml`.

It that runs the existing `androidTest` sources on a real Android image in CI, and a documented way to add a case to it.

The leg runs the tests that already exist (the DOM-storage probe and any siblings) so its first commit proves the harness on known-good assertions rather than introducing new ones at the same time. From then on, an Android acceptance criterion has somewhere to live.

Scope is deliberately narrow: **the leg, not new coverage.** Adding cases is the job of the specs that need them.

## User Stories

1. As a werust developer, I want the existing Android instrumented tests to run in CI on an emulator, so that on-device behaviour is a gate result rather than something a human has to notice on a phone.
2. As a werust developer, I want the leg to run the tests that already exist before any new ones are added, so that a red result on its first run means the harness is wrong, not the assertions.
3. As a task author, I want a documented way to add an instrumented case, so that a spec needing Android evidence can name this leg in its acceptance criteria.
4. As a maintainer of CI cost, I want the leg's trigger and image pinned deliberately, so that an emulator job does not silently become the slowest thing on every push.
5. As a developer reading a red build, I want the failure to name the device image and API level it ran on, so that an emulator-specific result is distinguishable from a real regression.

## Out of Scope

- **New instrumented coverage.** Owned by the specs that need it.
- **A macOS-style interactive smoke** that drives the app through gameplay-like input.
- **Changing the acceptance gate.** `verify` is untouched.
- **iOS.** `mobile-ios.yml` exists and builds; making it DRIVE anything is separate work.
