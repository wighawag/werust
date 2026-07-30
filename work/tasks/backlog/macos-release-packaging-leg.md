---
title: "Package the macOS desktop build in CI: universal binary, unsigned `.app`, attached to the tagged Release"
slug: macos-release-packaging-leg
blockedBy: [macos-wkwebview-backend-and-window]
covers: []
---

## What to build

Sub-task 4 of the `macos-desktop-build` split prescribed by `docs/adr/0011-webview2-for-windows.md`. This is the ONLY part of the original combined task that was genuinely a CI job.

Build the macOS desktop binary for both `x86_64-apple-darwin` and `aarch64-apple-darwin`, `lipo` them into a universal binary, bundle it as `Werust.app` with a minimal `Info.plist` (`CFBundleName`, `CFBundleIdentifier`, `CFBundleVersion` from the same version source the rest of the release uses, `CFBundlePackageType=APPL`), and attach it as a zip or tarball to the tagged GitHub Release beside the existing desktop Linux binary, Android APK and iOS Simulator `.app`.

**Reuse the existing `macos-14` runner shape.** `.github/workflows/release.yml` already runs an `ios-simulator-app` job there; model this on it (and on the `android-apk` leg, which is the closest sibling: decoupled with `needs: verify`, idempotent `gh release create`, dry-run artifact upload). Prefer a sibling job over extending the iOS one, so an iOS failure cannot block the desktop artifact and vice versa.

**Unsigned, deliberately.** No signing, no notarization: those need an Apple Developer account and are a separate follow-on, the macOS analogue of `android-apk-signing`. An unsigned `.app` opens via right-click then Open, or `xattr -d com.apple.quarantine`; say so in the README rather than pretending the artifact is ready for general distribution. If you add a signing path later, follow the Android precedent: gate on a secrets-presence env flag, graceful no-op without it, honest artifact naming.

**Version:** `CFBundleVersion` must come from the SAME version source the Rust core resolves (`WERUST_VERSION` on a tag, else `build.rs`'s `git describe`), never a second one. The Android sibling task `android-apk-version-from-the-release-tag` exists precisely because that was got wrong there.

Pin the workflow shape with a test in the existing `crates/werust-core/tests/release_plumbing_shape.rs` style, which parses the workflow inside the pure-Rust `verify` gate (no macOS, no network).

ADR sizing: 2 to 4 person-days.

## Acceptance criteria

- [ ] A tagged release attaches a macOS desktop artifact (universal `.app` zip or tarball) alongside the existing artifacts.
- [ ] The binary is universal (both architectures verified with `lipo -info` or equivalent in the job).
- [ ] `CFBundleVersion` / the reported version comes from the existing version source; no second source is introduced.
- [ ] The job runs on the existing `macos-14` runner shape, decoupled so it cannot be blocked by (or block) the iOS or desktop-Linux legs, and the dry-run path uploads an artifact without publishing.
- [ ] The workflow shape is pinned by a `release_plumbing_shape.rs`-style test; network-isolated.
- [ ] The README states the artifact is unsigned and how to open it, and names the signing follow-on.

## Prompt

> Goal: add the macOS desktop packaging leg to the release workflow. Build `x86_64-apple-darwin` + `aarch64-apple-darwin`, `lipo` into a universal binary, bundle `Werust.app` with a minimal `Info.plist`, attach a zip/tarball to the tagged Release beside the Linux binary, Android APK and iOS Simulator `.app`. Model it on the existing `ios-simulator-app` and `android-apk` jobs (same `macos-14` runner shape, `needs: verify` decoupling, idempotent `gh release create`, dry-run artifact upload), as a SIBLING job so the two mobile/desktop legs cannot block each other. Unsigned only: no signing, no notarization (a separate follow-on, and when it comes, copy the Android secrets-presence-flag pattern). `CFBundleVersion` comes from the SAME version source the Rust core uses, never a second one. Pin the workflow shape with a `release_plumbing_shape.rs`-style test that parses the YAML inside the pure-Rust verify gate.
