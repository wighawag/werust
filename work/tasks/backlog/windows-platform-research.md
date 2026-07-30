---
title: "Research: what webview/UI backend for Windows (and what would it take to make werust cross-platform — GTK, webview2, or the native Rust renderer?)"
slug: windows-platform-research
blockedBy: []
covers: []
needsAnswers: true
---

## What to research

Origin: split out of the retired `signed-multi-platform-builds` proposal (commit `fd11014`). It covers no spec criterion, so it carries `covers: []` and no `spec:` linkage, per the work contract.

werust currently targets three platform families with three separate shell backends:

| Platform | Shell | Rendering engine | Rust crate binding |
|---|---|---|---|
| Desktop Linux | GTK4 `Application` | WebKitGTK (via `webkit6` crate) | `crates/webview-renderer` |
| Android | Kotlin `WebView` | Android System WebView (Blink) | `crates/werust-android/rust` |
| iOS | Swift `WKWebViewShellController` | WKWebView (WebKit) | `crates/werust-ios/rust` |

Windows has **no supported path** because:
- `webkit6` (the Rust crate) does not support Windows (it wraps `libwebkitgtk-6.0.so`, which is Linux-only).
- There is no WebKitGTK on Windows.
- GTK4 itself has a Windows port (gtk4-win32), but WebKitGTK does not distribute Windows DLLs, and compiling it from source on Windows is a significant undertaking.

The `Renderer` trait at `crates/webview-renderer/src/lib.rs` is the seam that abstracts the rendering backend. A Windows desktop build could be a NEW implementation of `Renderer` over:

1. **WebView2** (Edge WebView2, the modern Microsoft webview control, distributed with Windows 10+ or via an evergreen runtime). This is the most practical path: WebView2 is a real webview (Chromium-based, same Blink engine as Android's System WebView), installable as a NuGet package or a system runtime, and there are Rust bindings (e.g. `webview2` or `webview` crates — check current state). The existing `crates/werust-android/rust` backend could serve as a model (custom-scheme interception for `ipfs://`, trust posture reporting, console capture, the whole EIP-1193 provider bridge). The key questions:
   - Can WebView2 intercept custom schemes (`ipfs://`)?
   - Does an intercepted scheme document get a REAL origin (so SvelteKit client-side nav works, unlike Android's opaque-origin problem)?
   - Is the WebView2 Rust binding maintainable and well-typed?
   - Can werust's `Renderer` trait express everything WebView2 needs?

2. **The native Rust renderer** (`crates/native-renderer` — exists as a skeleton, a pure-Rust renderer driven from benchmark/document layout). This is the Phase-3 "replace WebKit entirely" direction, but it is EXPERIMENTAL and not remotely production-ready. A Windows build on this path would be a multi-year project. Not recommended for Phase 1.

3. **GTK4-on-Windows with a different web engine** (e.g., the `webkitgtk` crate's Windows support is zero, but `gtk4` itself does run on Windows via the GDK Win32 backend; you could potentially embed a `WebView2` widget inside a GTK4 window via an interop layer). This is awkward: mixing GTK4 and WebView2 is not well-documented and adds a heavy system dependency (GTK4 + all its deps on Windows, which requires MSYS2 or vcpkg). Not recommended.

**Recommendation to validate:** option 1 (WebView2 via a Rust binding, implementing `Renderer` as a new backend in `crates/webview-renderer`). Research the feasibility in a single task and produce a written recommendation in `docs/adr/`.

## What to produce

A written ADR (`docs/adr/0011-webview2-for-windows.md` or similar; `0005` is already the platform-capability parity guard, and `0011` is the next free number) answering:

- Which Rust WebView2 binding is mature enough to use? (`webview2` crate, `webview` crate, or direct COM interop via `windows-rs`?)
- Can WebView2 intercept `ipfs://` custom schemes? Can it serve intercepted content with a real tuple origin (so client-side navs work without the opaque-origin workaround Android needed)?
- Can werust's `Renderer` trait express a WebView2 backend, or does the trait need widening?
- What is the approximate implementation effort in person-days?
- Is WebView2 available on Windows 10 1809+ / Windows 11 without additional installers (the evergreen runtime)?
- A build and test strategy: cross-compile from the existing ubuntu runner? Or must CI add a `windows-latest` runner?
- A comparison: WebView2 path vs. the existing macOS desktop task's WKWebView approach (similar architecture, similar seam).

The ADR ends with a **recommendation: go / no-go / defer**, and if go, a rough task breakdown.

## Research method

- Read the WebView2 documentation and API surface (Microsoft docs).
- Check the `webview2` Rust crate (crates.io, docs.rs, source) and the `webview` umbrella crate.
- Check whether any existing open-source Rust browser/webview project uses WebView2 for custom-scheme interception (e.g. `tauri`, `lapce`, `zed` — though Tauri uses a custom protocol, not custom schemes; find the exact pattern).
- Check the WebView2 custom-scheme registration API ("WebResourceRequested" event and `SetVirtualHostNameToFolderMapping` or `CoreWebView2CustomSchemeRegistration` — note: as of 2025, custom scheme interception in WebView2 has been unstable/changing; check the CURRENT state).
- Answer the SvelteKit compatibility question specifically: on a custom-scheme origin, does `fetch()` and `history.pushState()` work (Android's bug)?
- Estimate the implementation effort: new crate in `crates/webview-renderer/src/` (e.g. `webview2_backend.rs`) implementing `Renderer` + the Windows entry point (a `WinMain` / `winit` window driving the trait).

## Acceptance criteria

- [ ] A committed ADR (`docs/adr/0011-webview2-for-windows.md`) with the research findings.
- [ ] The ADR ends with a clear recommendation (go / no-go / defer) and, if go, a rough task breakdown.
- [ ] The ADR is self-contained (anyone reading it can decide whether to fund the Windows build).
- [ ] Key technical questions (custom-scheme interception, SvelteKit compatibility, CI strategy) are answered with references.

## Prompt

> Research a Windows desktop build for werust. The core question: can werust's `Renderer` trait be implemented over WebView2 (Edge's webview), and would custom-scheme `ipfs://` interception give documents a real tuple origin (avoiding the opaque-origin problem that needed the Android `origin_map.rs` hack)? Produce an ADR with findings and a go/no-go/defer recommendation. Check the `webview2` Rust crate, the custom-scheme registration API (WebResourceRequested event), and whether real-world Rust projects (Tauri, Lapce, Zed) solve this. Do NOT implement — research only.
