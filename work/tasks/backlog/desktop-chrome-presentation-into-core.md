---
title: "Move the desktop chrome PRESENTATION rules out of the GTK edge into the shared toolkit-free core, so an edge only paints"
slug: desktop-chrome-presentation-into-core
blockedBy: []
covers: []
---

## What to build

Prescribed by `docs/adr/0011-webview2-for-windows.md` as the piece worth doing with ZERO new platforms, and as sub-task 1 of the `macos-desktop-build` split. It is a behaviour-preserving refactor.

The desktop chrome's display rules are pure functions of `ChromeState`, but they live in the GTK EDGE (`crates/werust/src/main.rs`): `status_line`, `trust_indicator` / `trust_indicator_detail` / `trust_indicator_css_class`, `error_banner_visible` / `error_banner_text` / `error_banner_css_class`, `invalid_entry_badge_visible` / `invalid_entry_badge_text`, `load_progress_visible` / `load_progress_fraction` / `load_progress_hint`. They are core logic sitting in an edge, and the proof is that they are already re-derived TWICE more: `WerustCore.kt` in Kotlin and `WerustCore.swift` in Swift each carry a hand-written twin (`statusLine()`, `trustIndicator()`, `errorBanner()`, `loadProgress*()`, …). Three copies of one derivation is three chances to disagree, and every new window (Win32, AppKit) would mint a fourth.

Move the Rust copy into the shared toolkit-free core beside `ChromeState`, leave `crates/werust/src/main.rs` as a pure PAINTER that reads the derivation and sets widget properties, and move the existing unit tests with the functions. No behaviour changes, no new vocabulary, no new seam: "seam" in this codebase means a hot-swappable backend interface (`Renderer`, `Fetcher`, `ScriptEngine`), and this is not one. It is core logic returning home.

**Scope boundary:** this task does NOT touch the Kotlin/Swift twins. Collapsing those onto the shared derivation means deciding how a non-Rust edge consumes it (extend the chrome JSON with the derived strings, or expose the functions over FFI), which is its own task with its own trade-offs. Name that follow-up; do not attempt it here.

**Where to look:** `crates/werust/src/main.rs` (the functions above and their tests), `crates/werust-core/src/lib.rs` (`ChromeState` and its `is_loading` / `load_step` / failure accessors), `crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt` and `crates/werust-ios/App/Sources/WerustCore.swift` (the twins, for reference only). The CSS class names stay a GTK concern in name only: they are already returned as plain strings by pure functions, so they move too, but the stylesheet stays in the edge.

## Acceptance criteria

- [ ] Every listed presentation function lives in `werust-core` (toolkit-free: no gtk4/webkit6 import reaches it) and `crates/werust/src/main.rs` calls them.
- [ ] The desktop chrome behaves EXACTLY as before: the existing desktop tests move with the functions and still pass unchanged in substance (a rename of the test module is fine, a change of an assertion is not).
- [ ] No new dependency, no new seam, no widening of `ChromeState`'s public surface beyond what the moved functions need.
- [ ] The follow-up for the Kotlin/Swift twins is NAMED (a task or a recorded note), with the consume-over-FFI vs extend-the-chrome-JSON choice stated as the open question.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: move the desktop chrome PRESENTATION rules (`status_line`, `trust_indicator*`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`) from the GTK edge `crates/werust/src/main.rs` into the shared toolkit-free `werust-core`, beside `ChromeState`, taking their unit tests with them, so the GTK file becomes a pure painter. They are already pure functions of `ChromeState`, and the same derivation is hand-written twice more in Kotlin and Swift, so this is core logic living in an edge. Behaviour-preserving: no assertion changes, no new dependency, no new seam. Do NOT collapse the Kotlin/Swift twins here; name that as a follow-up whose open question is whether a non-Rust edge consumes the derivation over FFI or through an extended chrome JSON. Prescribed by ADR-0011 as the step that turns "a fourth shell" into "a fourth window".
