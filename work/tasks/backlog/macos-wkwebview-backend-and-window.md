---
title: "macOS desktop: a WKWebView `Renderer` backend driven from a native AppKit window"
slug: macos-wkwebview-backend-and-window
blockedBy: [desktop-chrome-presentation-into-core]
covers: []
---

## What to build

Sub-task 2 of the `macos-desktop-build` split prescribed by `docs/adr/0011-webview2-for-windows.md`. It replaces the original combined task, whose 3-way cut and file paths the ADR superseded.

A native `Werust.app` on macOS: a new `Renderer` backend over **WKWebView** plus an AppKit window that paints the chrome. NOT WebKitGTK via Homebrew (that binary would depend on the user having Homebrew's WebKitGTK installed, which is not a distribution story), and NOT a cross-platform GUI toolkit (werust has deliberately not adopted one).

**The `Renderer` trait lives in `crates/renderer/src/lib.rs` (`pub trait Renderer` at line 695), NOT in `crates/webview-renderer/src/lib.rs`.** The original task said otherwise; that premise was wrong and cost a Gate-2 nit. Related and load-bearing: `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so the macOS backend needs its own crate (or a cfg-gated sibling), and anything genuinely toolkit-free that it wants to reuse (notably `crates/webview-renderer/src/offthread.rs`, which imports only `fetcher`, `renderer`, `werust_core` and the crate's own `SharedLifecycle`) must move to a shared home first.

**Lean on the iOS edge, which is already a working WKWebView `Renderer` backend** driving the shared Rust core (`crates/werust-ios/rust`, `IosBackend` / `CoreSession`, and the Swift controller in `crates/werust-ios/App/Sources/WKWebViewShellController.swift`). What differs on macOS is the window and the controls (NSWindow, a real menu bar, a desktop-shaped toolbar), not the engine plumbing. The chrome's presentation rules come from `desktop-chrome-presentation-into-core`, so this task PAINTS, it does not re-derive.

**Trust hooks are the qualification bar, not rendering** (ADR-0001): the backend qualifies only when `ipfs://` custom-scheme interception and EIP-1193 provider injection both work, exactly as on the other edges. WebKit gives `WKURLSchemeHandler`-served documents real tuple origins, which is why macOS is the better-placed platform, but note the repo's own honesty caveat: iOS parity on that point is a recorded MECHANISM ANALYSIS whose runtime confirmation still awaits a Mac (`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`, "iOS parity"). Confirming it on macOS retires that caveat for both.

**Scope: unsigned.** No code signing, no notarization (they need an Apple Developer account). An unsigned `.app` opens via right-click then Open, or `xattr -d com.apple.quarantine`. Packaging and the release leg are sub-task 4 (`macos-release-packaging-leg`); the parity-matrix column is sub-task 3.

ADR sizing for this step: 8 to 14 person-days, lower because sub-task 1 landed first.

## Acceptance criteria

- [ ] A native macOS binary opens an AppKit window with a WKWebView rendering content, driven by the shared `BrowserShell` (no browsing decision in Swift/ObjC).
- [ ] The `Renderer` trait from `crates/renderer` is implemented with NO widening; navigation, history, load lifecycle, script-message bridge and custom-scheme interception all go through it.
- [ ] Both trust hooks work: an `ipfs://<cid>` URL renders hash-verified content, and a page sees the native EIP-1193 `window.ethereum`.
- [ ] The chrome (URL bar, nav controls, trust indicator, error/loading surfaces, invalid-entry badge, menu, debug view) paints from the SHARED derivation produced by `desktop-chrome-presentation-into-core`, not from a re-derivation.
- [ ] The macOS code does not live in a crate that unconditionally depends on gtk4/webkit6; anything reused from `webview-renderer` is moved to a shared, toolkit-free home rather than copied.
- [ ] Trait-contract tests cover the new backend where testable without a Mac; visible macOS behaviour is recorded manual steps in a spike README.
- [ ] Whether a `WKURLSchemeHandler`-served document gets a REAL tuple origin (same-origin `fetch` + `pushState` working) is CONFIRMED at runtime on macOS and recorded, since that also retires the iOS mechanism-analysis caveat.

## Prompt

> Goal: a native macOS desktop werust: a new `Renderer` backend over WKWebView (the trait is `crates/renderer/src/lib.rs:695`, NOT in `webview-renderer`, which depends on gtk4/webkit6 unconditionally and cannot be the home) plus an AppKit/NSWindow shell that PAINTS the chrome from the shared derivation `desktop-chrome-presentation-into-core` produced. Lean on the existing iOS WKWebView backend (`crates/werust-ios/rust`, `IosBackend`/`CoreSession`) for the engine plumbing; what is new is the desktop window, not the engine. A backend qualifies only when the TRUST HOOKS work (`ipfs://` interception + EIP-1193 injection), not merely when it renders. Move `offthread.rs` (already toolkit-free) to a shared home rather than copying it. Unsigned only: no signing, no notarization, no packaging (separate sub-tasks). Confirm at runtime whether a `WKURLSchemeHandler`-served document gets a real tuple origin, which also retires the recorded iOS mechanism-analysis caveat.
