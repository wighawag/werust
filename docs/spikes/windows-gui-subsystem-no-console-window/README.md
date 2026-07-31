# Windows: no console window beside the browser. What landed, and what is proven by what

Task: `windows-gui-subsystem-no-console-window`. Defect it closes: defect 3 of the first run on REAL Windows hardware, [`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`](../../../work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md), where launching `werust-windows` opened a console window alongside the browser window. Judgement calls made while closing it: [`DECISIONS.md`](DECISIONS.md). The window this defect sits on: [`windows-win32-window-and-chrome`](../windows-win32-window-and-chrome/README.md).

**Read this first.** The console window, and its absence, can only be SEEN on a Windows desktop with a person in front of it. This work was written on Linux and its central claim is therefore backed by a MEASUREMENT of the link line rather than by a screenshot: [What the local measurement proves](#what-the-local-measurement-proves). What still needs a human on hardware is listed at the end, unhedged.

## What landed

- **`crates/werust-windows/src/main.rs`**: `#![cfg_attr(windows, windows_subsystem = "windows")]` at the binary crate root. That one attribute is the whole fix. It makes the linker mark the image as a GUI application, and Windows allocates no console for such a process. It is `cfg`-gated, so nothing about the non-Windows build changes and the `#[cfg(not(windows))]` arm that keeps this crate inside the Ubuntu `verify` gate still refuses loudly with the same words.
- **`crates/werust-windows/src/startup.rs`** (new, `#[cfg(windows)]`): where werust's own words go now that there is no console.
  - `attach_parent_console()` calls `AttachConsole(ATTACH_PARENT_PROCESS)` and then binds whichever standard stream the launcher did not already hand over (a `> log.txt` redirection is left alone). A run started FROM a terminal prints exactly as it did before; a double-clicked one attaches to nothing and spawns nothing. werust never calls `AllocConsole`.
  - `report_startup_failure()` is a `MessageBoxW` for the launch that has no console to read, so a machine with no WebView2 Runtime is still TOLD so (`windows_renderer::missing_runtime_error`, ADR-0011 finding 6) instead of showing nothing at all.
- **The startup banner and the failure line are kept, not deleted.** Both are now conditional on a console being attached, because printing without one does not merely go unseen: it PANICS ("failed printing to stdout"). Which surface a failure takes is decided once, in `main.rs`. Why that rule: [`DECISIONS.md`](DECISIONS.md) §2 and §3.
- **`crates/werust-windows/tests/windows_window_shape.rs`**: `the_binary_links_as_a_gui_app_and_a_startup_failure_stays_legible`, in the file's existing source-shape style. It asserts the attribute is present and `cfg`-gated, the non-Windows refusal survives, both failure surfaces exist, no `AllocConsole` appears anywhere in the Win32 half, and the decision is recorded. The guard runs on the Ubuntu gate, which is where a Windows-only property like this can still be pinned by reading the source. `startup.rs` joined the `win32_half()` set that file asserts over, so the crate's existing rules (no chrome rule called here, no label restated, no second palette) cover the new module too.
- **[`show-linker-subsystem.sh`](show-linker-subsystem.sh)** (new): the measurement below, reproducible from Linux, with a negative control.
- **`crates/werust-windows/Cargo.toml`**: one added `windows` feature, `Win32_System_Console`, for `AttachConsole`/`SetStdHandle`.

Out of scope, deliberately: the window, the chrome and the engine are untouched, and so are the other two defects that same hardware run found (the half-size chrome on a high-DPI display, and the phantom horizontal scrollbar, each of which has its own task).

## What the Ubuntu `verify` gate proves TODAY (every ordinary run)

`the_binary_links_as_a_gui_app_and_a_startup_failure_stays_legible` reads `main.rs` and `startup.rs` and asserts the shape above. It proves the attribute is THERE and that neither message was quietly deleted; it cannot prove what Windows does with it. It is a ratchet against a later refactor silently re-opening the console, which is exactly what it is for.

The whole crate's host-independent half still compiles and tests on Ubuntu, unchanged. That is the point of the `cfg_attr`.

## What the local measurement proves

[`show-linker-subsystem.sh`](show-linker-subsystem.sh) asks rustc for the real link line of the `werust-windows` bin target, cross-compiled for `x86_64-pc-windows-msvc` via `cargo-xwin`, and reads the subsystem out of it. Run on this change, 2026-07-31, from Linux:

| tree | what rustc passes the linker | what Windows does |
|---|---|---|
| with the attribute (what landed) | `/SUBSYSTEM:windows /ENTRY:mainCRTStartup` | allocates NO console |
| NEGATIVE CONTROL: the same tree, attribute commented out | neither flag, so the MSVC linker defaults to `/SUBSYSTEM:console` | allocates a console: the window the human saw |

The script exits 0 in the first case and 1 in the second, and both were run. This is stronger than "the attribute is in the file" (which is all the shape test can say) and weaker than "no console appeared on a desktop" (which needs a human): it proves the attribute reaches the LINKER and changes the image's subsystem, which is the mechanism the console window comes from.

The link itself is not completed here and is not meant to be: embedding `app.manifest` needs `mt.exe`, a Windows-only tool. The link args are printed before that point. Real linking stays the `windows-desktop-app` release job's business, on a `windows-latest` runner.

The sibling harness [`typecheck-windows-from-linux.sh`](../windows-webview2-renderer-backend/typecheck-windows-from-linux.sh) was also run against this change: `cargo xwin clippy -p werust-windows -p windows-renderer --target x86_64-pc-windows-msvc --tests --examples` came back clean, no errors and no warnings (re-run with `-- -D warnings` and `--all-targets`, also clean). So `startup.rs` type-checks against the real Windows SDK bindings; nothing in it has ever been executed.

## What the `windows-latest` CI leg proves

Nothing new, and that is worth saying plainly rather than implying otherwise. `windows-renderer.yml` builds and tests the crates and runs `window_smoke`, which is an EXAMPLE target, and examples are console-subsystem binaries regardless of what the bin target links as. So the leg neither exercises the attribute nor could report the console window it fixes. It does prove the crate still builds and its tests still pass with the new module in it.

## What still awaits real Windows hardware

Stated plainly, because nothing above can substitute for it:

1. **Double-click `werust-windows.exe` in Explorer.** Expected: the browser window appears, and NO console window appears beside it. This is the defect, and its verification.
2. **Run `werust-windows.exe` from a `cmd` prompt and from PowerShell.** Expected: the version banner still prints into that terminal, the prompt returns immediately (a GUI app is not waited for, so the banner may land beside a new prompt), and no second window appears.
3. **Force the startup failure on a machine with no WebView2 Runtime** (or rename the runtime folder), twice:
   - double-clicked: a message box titled `werust` carrying the runtime's name and the download link, then nothing else;
   - from a terminal: the same sentence on stderr as `werust: …`, and NO message box.
4. **`werust-windows.exe > log.txt` from a prompt.** Expected: the banner lands in the file rather than on the console, because the redirection the launcher set up is not overwritten.

Steps 3 and 4 are the ones a reviewer should be most sceptical of: they are the paths that were reasoned about rather than run.
