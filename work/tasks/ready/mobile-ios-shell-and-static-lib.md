---
title: Mobile — iOS app shell linking the Rust core, running on simulator
slug: mobile-ios-shell-and-static-lib
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [browser-shell-url-bar-and-live-interactive-view]
covers: [18]
---

> **FORWARD-POINTER (planted by drive-tasks; mirror wezig's iOS-on-CI pattern).** iOS is built and proven ENTIRELY on a GitHub `macos-14` runner ("CI is the Mac; no physical Mac") — exactly as wezig does in `.github/workflows/mobile-ios.yml` + `mobile/ios/build-and-run.sh`. Do NOT expect a local Xcode build in this repo's pure-Rust `verify` gate (which has no Xcode/SDK); the Xcode/Simulator build is a SEPARATE CI leg, the direct analogue of the Android task's `check-apk-abis.sh` leg. AUTHORING all of it (Xcode project, Swift shell, the Rust-lib cross-compile build phase, and the `macos-14` workflow) is plain files an agent on Linux writes; ACCEPTANCE is proven when that CI leg runs. Concretely, port wezig's shape swapping Zig->Rust: (1) an Xcode project (e.g. `mobile/ios/App/WerustShell.xcodeproj`) with a scheme, an `AppDelegate` + a `WKWebView`-hosting Swift shell (URL bar + back/forward driving the Rust core over a C-ABI/JNI-equivalent FFI, Swift confined to the OS edge — history/URL-bar/load-state are the Rust core's truth, reusing the SAME `werust-core` crate the Android task extracted, NOT a per-platform copy); (2) a "Build Rust lib" Xcode BUILD PHASE that runs `cargo build` for `aarch64-apple-ios-sim` (Simulator; add the target on the runner) into a staticlib linked with `-force_load`, pinning ONE arch end-to-end (`ONLY_ACTIVE_ARCH=YES ARCHS=$(uname -m)`) and `CODE_SIGNING_ALLOWED=NO` (Simulator only, no signing) — wezig hit an arch-mismatch without this, so copy that guard; (3) a `mobile/ios/build-and-run.sh` that `xcodebuild -sdk iphonesimulator ... build`, copies the built `.app` to a stable path, and (unless `BUILD_ONLY=1`) boots an iOS-17 Simulator via `xcrun simctl`, installs, launches, and asserts the Rust-linked greeting reached the log; (4) a `.github/workflows/mobile-ios.yml` on `runs-on: macos-14` that installs the Rust ios-sim target and runs `build-and-run.sh`; (5) the BUILD-leg check (criterion 4: the packaged `.app` contains the app bundle + binary) runs in that CI leg (a `BUILD_ONLY=1` path so the release workflow can package the `.app` without booting a simulator). macos-14 ships Xcode 15.4 => target the iOS 17 Simulator, deployment-target floor 16.0.

## What to build

Build a real iOS app (Swift only at the forced OS edge — the app shell, URL bar,
back/forward over the seams) that links the werust Rust core cross-compiled for iOS,
and runs in the iOS Simulator (aarch64-ios-simulator). This is the iOS half of
mobile parity with wezig. Cross-compile the Rust core as a normal Xcode build phase,
mirroring wezig's real Xcode project structure.

## Acceptance criteria

- [ ] A real iOS app project (not a spike) builds a Simulator `.app` that launches and shows a browsing surface over the seams.
- [ ] The Rust core is cross-compiled for the iOS Simulator target as a normal Xcode build phase and linked into the app.
- [ ] Swift is confined to the OS edge (app shell); browsing logic stays in the Rust core behind the seams.
- [ ] A BUILD-leg check asserts the packaged `.app` contains the app bundle + binary.

## Blocked by

- Blocked by `browser-shell-url-bar-and-live-interactive-view`.

## Prompt

> Goal: iOS parity with wezig — a real app linking the cross-compiled Rust core,
> running in the iOS Simulator (see `CONTEXT.md`: Swift only at the forced OS edge).
>
> Mirror wezig's real Xcode project (app shell + URL bar + back/forward over the
> seams), but cross-compile the RUST core (not Zig) as a normal Xcode build phase for
> aarch64-ios-simulator. Simulator only — device/store builds need signing, out of
> scope. Part of the Zig-less build experiment (`docs/adr/0002`). Feeds the release
> job (`release-goreleaser-rust-desktop-and-mobile-artifacts`).
>
> Done = a real iOS app builds a Simulator `.app` carrying the Rust core and launches
> a browsing surface.
