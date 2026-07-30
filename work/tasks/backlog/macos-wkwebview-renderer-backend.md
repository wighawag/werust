---
title: "macOS: the WKWebView `Renderer` backend (engine only, no window chrome)"
slug: macos-wkwebview-renderer-backend
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

The ENGINE half of the macOS desktop shell, split out so it can land and be reviewed without the chrome painting. Its sibling `macos-appkit-window-and-chrome` builds the window on top of it. From the `macos-desktop-build` cut prescribed by `docs/adr/0011-webview2-for-windows.md`; the ADR's Amendment 1 funds it.

A `Renderer` implementation over **WKWebView** on macOS. NOT WebKitGTK via Homebrew (that binary would need the user to have Homebrew's WebKitGTK, which is not a distribution story), and NOT a cross-platform GUI toolkit (werust has deliberately not adopted one).

**Where things live** (both premises an earlier version of this task got wrong):

- The `Renderer` trait is `crates/renderer/src/lib.rs` (`pub trait Renderer`, line 695), NOT `crates/webview-renderer`.
- `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so nothing in it compiles on macOS. This backend needs its own crate (or a cfg-gated sibling), and `crates/webview-renderer/src/offthread.rs` (genuinely toolkit-free: it imports only `fetcher`, `renderer`, `werust_core` and the crate's own `SharedLifecycle`) must MOVE to a shared home rather than be copied.

**Lean on the iOS edge, which is already a working WKWebView `Renderer` backend** driving the shared Rust core (`crates/werust-ios/rust`, `IosBackend` / `CoreSession`). The engine plumbing is largely the same; what differs on macOS is the host (an `NSView` in an `NSWindow` rather than a `UIViewController`), and that is the sibling task's problem. Where iOS logic is genuinely shared, extract rather than fork it, and say which parts you extracted.

**Trust hooks are the qualification bar, not rendering** (ADR-0001): this backend qualifies only when `ipfs://` custom-scheme interception AND EIP-1193 provider injection both work. A backend that renders but cannot serve verified content is not a werust backend.

**Confirm the origin behaviour AT RUNTIME, and write it down.** WebKit is expected to give `WKURLSchemeHandler`-served documents real tuple origins, which is why macOS is the better-placed platform, but this repo's iOS parity on that point is a recorded MECHANISM ANALYSIS whose runtime confirmation still awaits a Mac (`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`, "iOS parity"). Confirming it on macOS RETIRES that caveat for both platforms, so it is worth doing deliberately, in the spirit of the Windows probe: assert the document origin, a same-origin `fetch` that fires the handler, and a non-throwing `pushState`. `crates/windows-origin-probe` is the shape to copy, including its negative control.

**Scope boundary: no window chrome.** No URL bar, no trust indicator, no menus, no debug view. A minimal host (a hidden or bare `NSWindow`/`NSView`) is fine and expected: this task proves the SEAM, not the product surface. No signing, no packaging.

**Verification honesty (ADR-0011 Amendment 1):** this cannot be verified on real hardware from the development machine, so state explicitly what CI proved versus what remains analysis awaiting a Mac. The `macos-14` runner already exists in `.github/workflows/`.

## Acceptance criteria

- [ ] A `Renderer` implementation over WKWebView compiles and runs on macOS, with NO widening of the trait.
- [ ] It does not live in a crate that unconditionally depends on gtk4/webkit6; `offthread.rs` is MOVED to a shared toolkit-free home, not copied.
- [ ] Navigation, history, the load lifecycle, the script-message bridge and custom-scheme interception all go through the seam.
- [ ] Both trust hooks work: an `ipfs://<cid>` URL loads hash-verified content, and a page sees the native EIP-1193 `window.ethereum`.
- [ ] The origin behaviour is CONFIRMED at runtime on macOS (document origin, same-origin `fetch` that fires the handler, `pushState`), recorded, and the iOS mechanism-analysis caveat is updated to say what is now measured.
- [ ] A CI job on the existing `macos-14` runner builds and exercises the backend; trait-contract tests cover what is testable without a Mac.
- [ ] What CI proved versus what still awaits real hardware is stated explicitly.
- [ ] The repo `verify` gate on Ubuntu stays green (the macOS half is `cfg`-gated; use the repo's source-shape test pattern where the gate cannot compile it).

## Prompt

> Goal: the ENGINE half of the macOS shell, no chrome. Implement the `Renderer` trait (`crates/renderer/src/lib.rs:695`) over WKWebView on macOS, in its own crate (`crates/webview-renderer` depends on gtk4/webkit6 unconditionally and cannot host it), MOVING the toolkit-free `offthread.rs` to a shared home rather than copying it. Lean on the existing iOS WKWebView backend (`crates/werust-ios/rust`, `IosBackend`/`CoreSession`) and extract what is genuinely shared instead of forking it. A backend qualifies on the TRUST HOOKS (`ipfs://` interception + EIP-1193 injection), not on rendering. Confirm the `WKURLSchemeHandler` origin behaviour AT RUNTIME the way `crates/windows-origin-probe` did on Windows, negative control included, since that also retires this repo's recorded iOS mechanism-analysis caveat. A hidden or bare NSWindow host is fine: the window, URL bar, trust indicator, menus and debug view are the sibling task `macos-appkit-window-and-chrome`. No signing, no packaging. State plainly what CI proved versus what awaits a Mac.

## Requeue 2026-07-30

CONDUCTOR HANDOFF (2026-07-30, drive-tasks). Gate 2 blocked this correctly: acceptance criterion 5 (the origin behaviour CONFIRMED at runtime on macOS) is undelivered, and expected.json is a PREDICTION, not a recording. That is not the agent's fault: a worker cannot reach CI on the repo it works in, and workflow_dispatch is refused for macos-renderer.yml because the workflow is not on the default branch yet. The conductor opened PR #2 from THIS branch purely as a CI vehicle so the macos-14 leg can run against this code; the branch was rebased onto current main so GitHub can compute a merge ref. The measured result will be appended to this task body before the next dispatch. DO NOT re-derive the answer by hand and DO NOT relabel the prediction as a measurement: wait for the recorded run in the task body, then re-stamp expected.json with the real OS/WebKit build, and correct the README's 'What still awaits a Mac' section and the DIAGNOSIS addendum to say what is now measured versus what remains analysis.
