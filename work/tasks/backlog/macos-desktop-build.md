---
title: "Native macOS desktop build (Apple Silicon + Intel universal binary as a .dmg or .app zip)"
slug: macos-desktop-build
blockedBy: []
covers: []
---

## What to build

Origin: split out of the retired `signed-multi-platform-builds` proposal (commit `fd11014`). It covers no spec criterion, so it carries `covers: []` and no `spec:` linkage, per the work contract.

werust's CI already runs a `macos-14` job for the `ios-simulator-app` target. Extend that job (or create a sibling job that runs on the same macos-14 runner) to also produce a **native macOS desktop binary**. The macOS desktop app is a separate target from the iOS Simulator app — it runs as a native `Werust.app` on a Mac without Xcode or the Simulator, using WebKitGTK's sibling framework WebKit.framework (which macOS has built-in) OR the default system webview (WKWebView) through the existing `crates/webview-renderer` backend (which already supports the seam `Renderer` trait with WebKitGTK on Linux; macOS could use either WebKitGTK via Homebrew or the system WebKit framework natively).

**Scope: build + bundle only. No signing, no notarization.** (Those are a separate follow-on that needs an Apple Developer account.)

**Sizing warning (read before picking this up):** the title and the CI framing undersell it. The acceptance criteria below require a native macOS window with the FULL chrome (URL bar, trust indicator, menu, debug view) driving the shared core, which is a FOURTH SHELL alongside desktop GTK, iOS and Android, comparable in size to the iOS shell task, not a CI change. It also adds a platform to `docs/platform-capability-matrix.toml`, and the parity guard then FORCES an explicit cell for macOS in EVERY existing capability row (each one `implemented`, or `stubbed` with a real follow-on task, or `n-a` with a reason). Consider cutting it into (1) the WKWebView/AppKit backend + shell, (2) the parity-matrix column and its stub tasks, and (3) the CI packaging leg, before dispatching any of it.

**Architecture:**
- Build for `x86_64-apple-darwin` (Intel) and `aarch64-apple-darwin` (Apple Silicon).
- Combine into a universal binary with `lipo -create -output werust`.
- Bundle into `Werust.app/Contents/MacOS/werust` with a bare-minimum `Info.plist` (CFBundleName, CFBundleIdentifier, CFBundleVersion from `WERUST_VERSION`, CFBundlePackageType=APPL).
- Distribute as `.tar.gz` or `.zip` (DMG is a polish step, not required for Phase 1 — tar.gz is fine and avoids signing for the archive).

**The rendering backend question:** macOS has the system `WebKit.framework` (WKWebView) which is the same engine the iOS edge uses under the hood. The existing seam in `crates/webview-renderer` (the `Renderer` trait) has one implementation: WebKitGTK for Linux/desktop. A macOS desktop build needs either:
- (a) A new `Renderer` backend using `WKWebView` (the same WKWebView the iOS shell already uses, wrapped for the desktop seam), OR
- (b) WebKitGTK installed via Homebrew, compiled against the same `webkit6` Rust crate the Linux build uses.

**Recommendation:** Option (a) — a new `WKWebView` desktop backend that reuses the iOS shell's WKWebView seam but runs it in a native macOS window. This is more work but avoids depending on Homebrew WebKitGTK (which the user may not have installed). The macOS app is then a self-contained bundle with NO external library dependencies. The cost is a new crate or a new implementation of `Renderer` targeting `WKWebView` + `AppKit` (or SwiftUI/NSWindow).

**Alternative (b) is simpler but fragile:** the CI can `brew install webkitgtk` and compile against it, but the resulting binary links against Homebrew's WebKitGTK which the user must also have installed. Not a good distribution story.

**Where to look:**
- `crates/webview-renderer/src/lib.rs` — the `Renderer` trait and `install_ipfs`/`navigate`/`inject_script` etc. A new implementation for macOS desktop would implement the same trait over `WKWebView`.
- `crates/werust-ios/App/Sources/WKWebViewShellController.swift` — the iOS edge already drives a WKWebView through the same Rust core via FFI. The macOS desktop backend would share a lot of this but present a native NSWindow/MenuBar instead of a full-screen mobile controller.
- `.github/workflows/release.yml` `ios-simulator-app` job — model for the new macOS-desktop runner.

## Acceptance criteria (Phase 1: build-only, no signing)

- [ ] A `Werust-darwin-x86_64.tar.gz` and/or `Werust-darwin-aarch64.tar.gz` is attached to every tagged release (or a universal `Werust-darwin-universal.tar.gz` via lipo).
- [ ] The binary opens a native macOS window with a WKWebView rendering content and the same rust-core chrome (URL bar, trust indicator, menu, debug view).
- [ ] The existing `Renderer` trait is honoured: navigation, trust posture, script injection, and custom-scheme interception all work through the WKWebView implementation.
- [ ] The CI job extends the existing `macos-14` runner (no new runner cost) or adds a separate `macos-14` job that runs in parallel with the ios-simulator job.
- [ ] Tests cover the new backend's seam where testable (the trait-contract tests in `webview-renderer`); the visible macOS behaviour is recorded manual steps.
- [ ] No signing required for Phase 1 (the unsigned `.app` zip can be opened via right-click → Open on macOS, or via `xattr -d com.apple.quarantine`).

## Prompt

> Add a native macOS desktop build to the release CI. The new backend implements the existing `Renderer` trait over `WKWebView` in a native `NSWindow` (reusing the iOS edge's WKWebView seam patterns but in a desktop AppKit context). Build for both x86_64-apple-darwin and aarch64-apple-darwin, lipo a universal binary, bundle as an unsigned `.app` zip attached to the Release. No signing (follow-on). No Homebrew dependency — the WKWebView is built into macOS.
