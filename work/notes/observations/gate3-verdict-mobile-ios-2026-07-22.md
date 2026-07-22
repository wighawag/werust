---
title: Gate-3 (conductor) verdict — mobile-ios-shell-and-static-lib — APPROVE (built via macos-14 CI, wezig pattern)
date: 2026-07-22
kind: observation
reviewOf: mobile-ios-shell-and-static-lib
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 9765e0d)

Previously parked on the "no macOS/Xcode on this Linux host" wall. UN-parked at the
human's prompt ("in wezig we only built it via GitHub Action; can't we do the same?"):
authored the CI way. `do` ran Gate-1 + Gate-2 (pure-Rust) green; the Xcode/Simulator
build is a separate `macos-14` CI leg (analogue of the Android APK-ABI check), so the
Linux pure-Rust gate legitimately does not run it. Conductor diff-vs-criteria review.

### Acceptance criteria — met (proven on the macos-14 CI leg)

- ✅ Real iOS app project (`crates/werust-ios/App/WerustShell.xcodeproj`, not a spike):
  `AppDelegate` + `WKWebViewShellController` + `WerustCore` (FFI), URL bar + back/forward
  driving the Rust core. `build-and-run.sh` runs `xcodebuild -sdk iphonesimulator ...`
  then boots an iOS 17 Simulator via `simctl`, installs, launches, asserts the
  Rust-linked greeting reached the log.
- ✅ Rust core cross-compiled for `aarch64-apple-ios-sim` as a normal Xcode build phase
  (`build-rust-lib.sh`), linked `-force_load`.
- ✅ Swift confined to the OS edge: reuses the SAME shared `werust-core` crate as the
  Android task (no per-platform browsing logic); the FFI core wraps a Rust `Renderer`
  over WKWebView.
- ✅ BUILD-leg check (`check-app-bundle.sh`) asserts the `.app` carries the bundle +
  binary; `BUILD_ONLY=1` path lets the release workflow package the `.app` without
  booting a simulator.

### wezig pattern honoured (the forward-note)

`.github/workflows/mobile-ios.yml` runs on `runs-on: macos-14`, `rustup target add
aarch64-apple-ios-sim`, then `build-and-run.sh` — mirroring wezig's mobile-ios.yml. The
wezig arch-mismatch lesson is copied verbatim (`ONLY_ACTIVE_ARCH=YES ARCHS=$HOST_ARCH`
end-to-end so the build phase and link agree on one arch), plus `CODE_SIGNING_ALLOWED=NO`
(Simulator only), iOS 17 target / 16.0 deployment floor.

### Nit triage

1. Swift `normalizeURL()` prepends `https://` to a bare host at the OS edge before
   `core.navigate` — KEEP; captured below. Not a defect (the core still validates +
   rejects, so no logic is hidden), but a cross-edge CONSISTENCY question: bare-host->https
   arguably belongs in the Rust core so desktop + Android + iOS share one rule.
2. 4 DECISIONS entries (Rust backend over WKWebView; staticlib + -force_load; single-arch
   pin from wezig; Xcode-as-CI-leg not the pure-Rust gate) — RATIFY/KEEP. Correct,
   reversible, recorded.

### Follow-up captured (not tasked here)

URL normalization (bare-host -> https) currently lives at each mobile OS edge
(Swift `normalizeURL`; check whether the Android edge matches). Consider hoisting it
into `werust-core` so all edges + desktop share one rule. Low priority; the core still
validates, so behaviour is safe, just potentially inconsistent across edges.

### What this unlocks

iOS landing means `release-goreleaser-rust-desktop-and-mobile-artifacts` is NO LONGER
blocked (both mobile deps are now done). The release task is next.
