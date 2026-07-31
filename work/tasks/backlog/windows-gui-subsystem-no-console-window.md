---
title: "werust on Windows must not open a console window beside itself"
slug: windows-gui-subsystem-no-console-window
blockedBy: []
covers: []
---

## What to build

Found by the human on REAL Windows hardware, 2026-07-31 (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`): launching `werust-windows` opens a console window alongside the browser window.

**Cause, confirmed by reading the source:** there is no `#![windows_subsystem = "windows"]` anywhere in `crates/werust-windows`, so the binary links as a CONSOLE subsystem application and Windows allocates a console for it. `src/main.rs` then prints a startup banner (`println!("werust {} — a Rust web browser (Windows WebView2 backend)")`), which is what that console displays.

**The fix, and the two things to get right around it.**

Add `#![windows_subsystem = "windows"]` at the top of `crates/werust-windows/src/main.rs`. It must be an inner attribute on the BINARY crate root, and it must be `cfg`-gated or otherwise harmless on non-Windows hosts, because this crate deliberately still COMPILES everywhere (that is what keeps its host-independent half inside the Ubuntu `verify` gate, and `main.rs` already carries a `#[cfg(not(windows))]` arm that refuses loudly). Do not break that property.

1. **Decide what happens to the startup banner and the error path.** Under the windows subsystem there is no console attached, so `println!` goes nowhere and `eprintln!("werust: {e}")` on a failed launch would VANISH — which would turn a legible startup failure into a window that silently never appears. That is the real risk in this change, and it matters here because the shell has a pre-specified honest failure (a machine with no WebView2 Runtime must say so, `windows-webview2-renderer-backend`). Choose deliberately and record it: either attach to the parent console when one exists (`AttachConsole(ATTACH_PARENT_PROCESS)`, so `werust.exe` run FROM a terminal still prints and a double-clicked one does not spawn a console), or surface startup failures in a `MessageBox` instead of stderr, or both. Do not simply delete the messages.
2. **Keep the banner useful where it is useful.** A GUI app printing a version banner on every launch is noise; the same string is already reachable via the ⋮ menu and the `werust` CLI's `version` verb. Dropping it from the GUI path is fine if you say so.

**Guard it.** Add an assertion in the existing `crates/werust-windows/tests/windows_window_shape.rs` style that the subsystem attribute is present in `main.rs`, so a later refactor cannot silently reintroduce the console. That test runs on the Ubuntu gate, which is exactly where a Windows-only property like this can still be pinned by reading the source.

**Scope:** the subsystem attribute, the console/failure-reporting decision it forces, and one shape assertion. No change to the window, the chrome or the engine.

## Acceptance criteria

- [ ] Launching the Windows binary opens NO console window.
- [ ] A startup FAILURE (for example, no WebView2 Runtime) is still legible to the user rather than silently vanishing; how it is surfaced is a recorded decision.
- [ ] `werust-windows` still compiles on non-Windows hosts and its `#[cfg(not(windows))]` refusal still works.
- [ ] A shape test pins the subsystem attribute so the console cannot come back unnoticed.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: `werust-windows` opens a console window beside the browser, found on real hardware. There is no `#![windows_subsystem = "windows"]` in the crate, so it links as a console-subsystem app. Add it at the binary crate root, `cfg`-gated so this crate still compiles on non-Windows hosts (its `#[cfg(not(windows))]` refusal arm must keep working — that property is what keeps its host-independent half in the Ubuntu gate). The real risk is what you do about output: under the windows subsystem there is no console, so `main.rs`'s startup `println!` goes nowhere and, more importantly, `eprintln!("werust: {e}")` on a failed launch would VANISH, turning the pre-specified honest failure (no WebView2 Runtime, from `windows-webview2-renderer-backend`) into a window that never appears with no explanation. Decide deliberately and record it: `AttachConsole(ATTACH_PARENT_PROCESS)` so a terminal-launched run still prints while a double-clicked one spawns nothing, and/or a `MessageBox` for startup failures. Do not just delete the messages. Pin the attribute with an assertion in the `crates/werust-windows/tests/windows_window_shape.rs` style so the console cannot silently return.
