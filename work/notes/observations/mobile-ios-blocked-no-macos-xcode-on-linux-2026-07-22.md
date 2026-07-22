---
title: mobile-ios-shell-and-static-lib cannot be built on this Linux host — needs macOS/Xcode (stuck-set)
date: 2026-07-22
kind: observation
tags: [infrastructure, toolchain, ios, xcode, macos, blocked, needs-human]
reviewOf: mobile-ios-shell-and-static-lib
---

## Environment wall — NOT dispatched, parked in the stuck-set

`mobile-ios-shell-and-static-lib` was NOT dispatched to `dorfl do` because its
acceptance is irreducibly macOS/Xcode-bound and this drive host is Linux:

- Host: `Linux nono ... Debian 6.12` (x86_64). No `xcodebuild`, no `xcrun`/`simctl`,
  no `swift` compiler; no iOS Rust targets installed (and they could not be LINKED
  here anyway without the Apple SDK/linker).
- The task's acceptance criteria require: a real iOS app that "builds a Simulator
  `.app` ... as a normal **Xcode build phase**" and "launches ... in the iOS
  Simulator (aarch64-ios-simulator)". Xcode, the iOS Simulator runtime, the `.app`
  bundle/codesign machinery, and the Apple linker exist ONLY on macOS.

This is a HARD environment blocker, not a fixable-by-retry failure (contrast: the
cargo-not-on-PATH issue was a shell-env fix, and the Android task had a full SDK+NDK
present on this laptop). No amount of `requeue` + re-`do` can make an Xcode/Simulator
build happen on Linux. Dispatching `do` would only burn a claim on a gate the host
cannot satisfy (or produce an unverifiable scaffold my Gate-3 would have to BLOCK).

## Consequence for the graph

- `mobile-ios-shell-and-static-lib` stays in `work/tasks/ready/` (NOT claimed, NOT
  stuck-locked — nothing was dispatched, so there is no lock to preserve). It is
  parked in the conductor's stuck-set for the human.
- Its downstream `release-goreleaser-rust-desktop-and-mobile-artifacts` is
  `blockedBy: [mobile-android-shell-and-static-lib, mobile-ios-shell-and-static-lib]`
  — Android has landed, but iOS has not, so the release task remains BLOCKED and
  cannot be driven on this host either.

## What a human needs to decide (surfaced in the end-of-run batch)

Build `mobile-ios-shell-and-static-lib` on a **macOS runner with Xcode** (the only
environment that can satisfy it), then `release-goreleaser` (which also needs the
macOS leg for the iOS `.app`, plus the Linux/Android legs already contracted). The
forward-note already planted on the release task covers reusing the Android
Gradle/`check-apk-abis.sh` contract; extend it with the iOS `.app` path once the iOS
module is built on a Mac.

Everything else in the 21-task pool that does NOT transitively depend on iOS can and
did continue (accumulate-don't-block).
