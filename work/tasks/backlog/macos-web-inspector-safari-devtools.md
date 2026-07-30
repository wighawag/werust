---
title: "macOS: make the page inspectable from Safari's Web Inspector, gated on a debug build"
slug: macos-web-inspector-safari-devtools
blockedBy: []
covers: []
---

## What to build

Close the ONE capability the `macos` parity column had to mark `stubbed` for a missing wire rather than a missing platform: `web-inspector` in `docs/platform-capability-matrix.toml`. Desktop (WebKitGTK, F12 in-window), iOS (Safari over USB) and Android (`chrome://inspect`) all reach their platform's OWN full devtools, gated on a debug build. The macOS shell reaches none: neither `crates/macos-renderer` nor `crates/werust-macos` sets `WKWebView.isInspectable` or touches `WKPreferences`, so Safari's Web Inspector cannot attach to a werust page at all. The in-app debug view (`debug-view-console-network`) is deliberately READ-ONLY and is not a substitute: this row is about the typeable console REPL + the engine's own network/DOM/sources view.

Wire the macOS twin of what the iOS edge already does, and reuse its decisions rather than re-deciding them: set `isInspectable = true` on the backend's `WKWebView`, gated on a DEBUG build so a release build is not silently inspectable (the desktop `enable-developer-extras` / `cfg!(debug_assertions)` gate and Android's `FLAG_DEBUGGABLE` gate are the same rule; the gating rationale is recorded in `work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`). The property is macOS 13.3+ (Safari 16.4), so mirror the iOS edge's availability handling rather than assuming it: on an older OS the shell must still build and run, with the property simply not set.

Where it belongs is the ENGINE crate (`crates/macos-renderer`), beside the other `WKWebView` configuration the backend owns and before/at realisation, NOT in the AppKit window: the window is a painter and the source-shape guard `crates/werust-macos/tests/macos_window_shape.rs` exists to keep it one. Note also that the engine is created LAZILY (the `WKWebViewConfiguration` is copied at construction), so decide and state where in that lifecycle the flag is set.

REACH, and say so honestly in the docs: unlike desktop, there is no in-window inspector to bind a key to — `WKWebView` exposes no "show inspector" API, so the reach on macOS is Safari's Develop menu (Develop -> the machine/app -> the page) with the Develop menu enabled in Safari's settings. If you conclude a shell-side affordance is still wanted (a ⋮/menu-bar hint, a startup line naming how to attach), treat that as a DESIGN choice and record it; do not silently invent a new menu item, because the ⋮ menu's items are the shared `werust_core::menu::BrowserMenu` and adding one there changes every platform's menu.

## Acceptance criteria

- [ ] A DEBUG build of the macOS shell serves pages that Safari's Web Inspector can attach to; a RELEASE build does not (the flag is gated, never unconditional).
- [ ] The gating is covered by a test the Ubuntu `verify` gate runs (the repo's source-shape pattern, `crates/macos-renderer/tests/macos_backend_shape.rs`), so the `#[cfg(target_os = "macos")]` code the gate cannot compile is still guarded: the property is set, and it is set INSIDE the debug gate.
- [ ] The macos-14 CI leg still builds/tests/runs the backend and window smokes green with the change in place (the flag must not disturb the trust hooks or the lazy-realisation ordering).
- [ ] `docs/spikes/enable-web-inspector-devtools-all-platforms/README.md` gains a macOS section in the same shape as the iOS/Android ones (how to open it, what gates it, where it is wired), and states the Safari-only reach.
- [ ] The `web-inspector` row's `macos` cell in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented` in the same change, naming what proves it; the parity guard stays green with no weakening.
- [ ] Any judgement call (availability handling, where in the lifecycle the flag is set, whether a shell-side affordance is added) is recorded durably and linked from the done record.

## Blocked by

- None — the macOS engine and window have both landed (`macos-wkwebview-renderer-backend`, `macos-appkit-window-and-chrome`).

## Prompt

> Goal: give the macOS shell the platform's OWN web inspector, exactly as the other three edges have it, and flip the `web-inspector` row's `macos` cell in `docs/platform-capability-matrix.toml` from `stubbed` to `implemented`. Today nothing in `crates/macos-renderer` or `crates/werust-macos` sets `WKWebView.isInspectable`, so Safari's Web Inspector cannot attach to a werust page; the in-app debug view is read-only and is NOT this capability. Wire `isInspectable` on the backend's `WKWebView` (the engine crate, not the painter window), gated on a DEBUG build so a release build is not silently inspectable — the same rule desktop (`enable-developer-extras` under `cfg!(debug_assertions)`), iOS (`#if DEBUG`) and Android (`FLAG_DEBUGGABLE`) follow, with the rationale in `work/notes/observations/web-inspector-devtools-gating-decisions-2026-07-23.md`. Mirror the iOS edge's OS-availability handling (the property is macOS 13.3+). Respect the backend's lazy realisation: the `WKWebViewConfiguration` is copied when the `WKWebView` is constructed. Cover the wiring with a source-shape test in `crates/macos-renderer/tests/macos_backend_shape.rs` so the Ubuntu gate guards code it cannot compile, keep the macos-14 leg green, and document the macOS reach (Safari -> Develop -> the machine/app -> the page; there is no in-window inspector API to bind a key to) in `docs/spikes/enable-web-inspector-devtools-all-platforms/README.md`. Do not add an item to the shared `BrowserMenu` without recording that decision: those items are every platform's menu.
