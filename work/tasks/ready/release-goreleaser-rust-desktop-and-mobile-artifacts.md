---
title: Release via GoReleaser Rust builder — desktop binaries + mobile artifacts
slug: release-goreleaser-rust-desktop-and-mobile-artifacts
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [mobile-android-shell-and-static-lib, mobile-ios-shell-and-static-lib]
covers: [19]
---

> **FORWARD-POINTER (planted by drive-tasks after `mobile-android-shell-and-static-lib` landed; extend when the iOS task lands).** The Android app module landed at `crates/werust-android/` as a real Gradle (Kotlin DSL) module WITH a wrapper (`./gradlew`). Concrete contract for the Android mobile job to REUSE (do not reinvent): (1) build the debug APK with `(cd crates/werust-android && ./gradlew :app:assembleDebug)` — a Gradle task cross-compiles the Rust core `cdylib` (`libwerust_mobile.so`) for `aarch64-linux-android` (arm64-v8a) + `x86_64-linux-android` (x86_64) via the NDK and copies each into `src/main/jniLibs/<abi>/`; the APK output is `crates/werust-android/app/build/outputs/apk/debug/app-debug.apk`. (2) RUN the acceptance ABI check `docs/spikes/mobile-android-shell-and-static-lib/check-apk-abis.sh <apk>` after building — this is where criterion 4's "APK carries the Rust core lib for both ABIs" assertion actually EXECUTES (it is deliberately NOT in the pure-Rust `verify` gate, which lacks the Android SDK/NDK; the mobile CI job owns running it). (3) The shared browsing logic now lives in the `werust-core` crate (extracted from the desktop binary; the Android FFI core links it) — the release build compiles that, not a per-platform copy. The mobile jobs need the Android SDK+NDK on the runner (this task's CI job must install them); the desktop GoReleaser leg does not. Mirror wezig's mobile-job SHAPE but swap the Zig-lib cross-compile for this Gradle+cargo-NDK path.

> **FORWARD-POINTER UPDATE (iOS has now landed).** The iOS module landed at `crates/werust-ios/` as a real Xcode project (`App/WerustShell.xcodeproj`), mirroring wezig's iOS pattern. Concrete contract for the iOS mobile job to REUSE: (1) it MUST run on `runs-on: macos-14` (Xcode/Simulator are macOS-only — this is the whole reason iOS is a CI leg, exactly like wezig's `mobile-ios.yml`); the desktop GoReleaser + Android legs run on Linux, the iOS leg on macos-14. (2) `rustup target add aarch64-apple-ios-sim`, then build the Simulator `.app` with `BUILD_ONLY=1 crates/werust-ios/build-and-run.sh` (the `BUILD_ONLY` path stops after `xcodebuild` produces the `.app`, WITHOUT booting a simulator — the packaging path a release wants); the built app is copied to a stable path (`crates/werust-ios/build/WerustShell.app`). (3) ZIP that `.app` and attach it (the iOS Simulator `.app` zip artifact criterion 3 names). (4) The acceptance build-leg check is `docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh` (asserts the `.app` contains the bundle + binary) — run it after building, same pattern as Android's `check-apk-abis.sh`. (5) It links the SAME shared `werust-core` crate. So the release workflow is THREE jobs mirroring wezig's `release.yml`: `goreleaser` (Linux desktop, runs first so the Release exists to upload into), then `android-apk` (Linux, needs SDK+NDK) and `ios-simulator-app` (macos-14), both `needs: goreleaser`, using `gh release upload <tag>` on a tag and `actions/upload-artifact` on the `workflow_dispatch` dry-run (where goreleaser runs `--snapshot`, publishing nothing).

## What to build

Wire the release pipeline via GoReleaser's native Rust builder (`builder: rust`,
cargo-zigbuild) — a deliberately Zig-less build path (`docs/adr/0002`). A tag push
(`v*`) cuts one GitHub Release carrying the desktop Linux binaries (amd64, arm64) +
checksums + a conventional-commit changelog, PLUS the mobile artifacts (the Android
debug APK and the iOS Simulator `.app` zip) built by hand-written jobs alongside it.
Port the desktop + mobile artifact shape from wezig's `release.yml`, swapping the
Zig builder/steps for the Rust equivalents. Include the `workflow_dispatch` dry-run
(snapshot, publishes nothing) that wezig's release workflow has.

## Acceptance criteria

- [ ] `.goreleaser.yaml` uses `builder: rust` and produces desktop Linux binaries (amd64, arm64) + checksums on a tag.
- [ ] The changelog is generated from conventional-commit git history (no per-change changeset files).
- [ ] The release workflow also builds + attaches the Android debug APK and the iOS Simulator `.app` zip (from the real mobile app modules), gated after the desktop build.
- [ ] A `workflow_dispatch` dry-run builds everything via snapshot + uploads workflow artifacts WITHOUT publishing a release.
- [ ] The same `verify` gate runs before a tag build so a tag can't ship a red tree.

## Blocked by

- Blocked by `mobile-android-shell-and-static-lib` and `mobile-ios-shell-and-static-lib`.

## Prompt

> Goal: release parity with wezig — GoReleaser Rust builder for desktop + the mobile
> artifacts, mobile included (see `docs/adr/0002`, `CONTEXT.md`). This proves the
> deliberately Zig-less build path end-to-end.
>
> Port wezig's `release.yml` + `.goreleaser.yaml` shape, swapping `builder: zig` →
> `builder: rust` (cargo-zigbuild) and the Zig-lib cross-compile steps in the mobile
> jobs for the Rust static-lib cross-compiles from `mobile-android-shell-and-static-lib`
> / `mobile-ios-shell-and-static-lib`. Keep the tag path (real release) + the
> `workflow_dispatch` dry-run (snapshot, workflow artifacts only) that wezig has. The
> changelog comes from conventional commits.
>
> Done = a version tag cuts a GitHub Release with desktop binaries + checksums +
> changelog + the Android APK + iOS Simulator `.app`, and a dispatch dry-run validates
> all artifacts without publishing.
