# Decisions: `windows-gui-subsystem-no-console-window`

Task: `windows-gui-subsystem-no-console-window`. What landed and what is proven by what: [`README.md`](README.md). The defect: [`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`](../../../work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md), defect 3.

The attribute itself was not a choice: the task named it, and there is no other way to stop Windows allocating a console. What the attribute FORCES is a choice, because it takes werust's stdout and stderr away, and this shell has a pre-specified honest failure to report on them. These are the calls made around it.

## 1. The attribute is `cfg_attr(windows, …)`, not bare

**Chosen:** `#![cfg_attr(windows, windows_subsystem = "windows")]`.

**Why:** a bare `#![windows_subsystem = "windows"]` is documented as ignored on non-Windows targets and would probably have been harmless, but "probably harmless" is the wrong bar for the one property that keeps `werust-windows` compiling on Ubuntu, the property the whole `verify` gate's coverage of this crate rests on. The gate is the reason the crate has a `#[cfg(not(windows))]` refusal arm at all. Gating the attribute makes the non-Windows build byte-for-byte what it was.

**Touches:** nothing else. The `werust-macos` and `werust` binaries have no subsystem concept.

## 2. BOTH surfaces: attach to the parent console AND a message box, but only one per launch

The task offered "attach to the parent console, or a message box, or both". Both landed, with a rule about which is used.

**Chosen:** `attach_parent_console()` runs first; a startup failure then goes to stderr if a console was attached, and to a `MessageBoxW` if not. Never both.

**Why:** the two launch paths are genuinely different users. Someone who typed `werust-windows.exe` at a prompt is watching that prompt, and a modal dialog on top of it is an interruption they did not ask for; someone who double-clicked an icon has no prompt at all, and stderr for them is a bit bucket. One legible report, on the surface that launched werust, satisfies the acceptance criterion ("a startup FAILURE is still legible") in both cases without either path getting a second, redundant one. The failure text itself is unchanged in either surface: it is the seam's `RendererError`, which for the case that matters is the core's own honest sentence naming the WebView2 Runtime and its download page.

**Rejected:**

- **The message box ALONE** (delete the prints). It would have made `werust-windows.exe` un-scriptable and un-loggable, and it turns a CI or support flow into "read this screenshot of a dialog".
- **The console attach ALONE.** The double-click is the launch a normal Windows user performs, and it is exactly the one that would have gone silent, which is the failure mode the task called out by name.
- **Always both.** A dialog that pops in front of a terminal that already printed the same sentence is noise, and it would block a scripted run on a modal window nobody expected.

**Touches:** the pre-specified no-WebView2-Runtime behaviour from `windows-webview2-renderer-backend` (this is now the surface it appears on for a double-clicked launch) and any future Windows packaging that adds a Start-menu shortcut, which is the same no-console path.

## 3. The banner is KEPT, conditional on a console. It is not deleted

**Chosen:** the version banner (`println!` of `werust <version>` and the backend name) still runs, but only when `attach_parent_console()` returned true.

**Why:** the task allowed dropping it ("a GUI app printing a version banner on every launch is noise… dropping it is fine if you say so"), and for the double-click it IS noise going nowhere. But once the console attach exists, "print it where someone asked for it" costs one `if`, and the terminal launch is precisely where a version line is what the person wanted. The same string stays reachable from the ⋮ menu and `werust version`, so nothing depends on this either way.

**And a correctness reason, not just taste:** the print CANNOT be unconditional. Under the windows subsystem with no console, Rust's `println!`/`eprintln!` do not silently discard, they PANIC ("failed printing to stdout"), because there is no handle to write to. So a conditional print is not an optimisation, it is the safe form. That is also why the failure line in `main.rs` sits inside the same condition.

**Touches:** the GTK and AppKit shells print the same banner unconditionally, and that stays true: they are console-subsystem binaries on platforms where a terminal launch is the norm. Windows is now the one shell where the banner is conditional, which is a consequence of being the one shell that is a GUI-subsystem binary.

## 4. `AttachConsole` binds only the streams the launcher did NOT provide

**Chosen:** after attaching, each of stdout/stderr is pointed at `CONOUT$` **only if** `GetStdHandle` says it is empty; a handle the launcher passed in is left exactly as it was. The `CONOUT$` handle is deliberately leaked, because it is the process's stream for the rest of its life.

**Why:** `werust-windows.exe > log.txt` from a prompt hands the process a real file handle for stdout while stderr is empty. Overwriting both with the console would silently break the redirection the user asked for, a small invisible wrongness of exactly the kind that is never found again. Binding only what is missing gives the expected behaviour in all four combinations (nothing redirected, stdout redirected, stderr redirected, both).

**Rejected:** unconditionally rebinding both (breaks redirection); rebinding neither (then a plain terminal launch prints nothing, because a GUI-subsystem process inherits no console handles even from a console parent, which is the whole reason the attach is not sufficient on its own).

**Not verified on hardware:** this is reasoned from the documented behaviour of `AttachConsole`/`SetStdHandle` and Rust's per-write handle lookup. It is step 4 of the manual-verification list in [`README.md`](README.md#what-still-awaits-real-windows-hardware).

## 5. `AllocConsole` is refused, and the refusal is guarded

**Chosen:** when there is no parent console, werust creates none. The shape test asserts `AllocConsole` appears nowhere in the crate's Win32 half.

**Why:** allocating one would re-open the very window this task closed, and it is the obvious "fix" a later change might reach for to make a print work. Naming it in the guard is cheaper than re-discovering it on hardware.

## 6. The guard is a SOURCE-SHAPE test plus a link-line measurement, not a runtime test

**Chosen:** the acceptance property ("no console window") is pinned in two places: `the_binary_links_as_a_gui_app_and_a_startup_failure_stays_legible` on every Ubuntu gate run, and [`show-linker-subsystem.sh`](show-linker-subsystem.sh), which measures the `/SUBSYSTEM:` flag rustc hands the linker for the real bin target (with a negative control that fails).

**Why:** no test can observe this. The Windows CI leg drives an off-screen window from an EXAMPLE target, and examples are console-subsystem binaries whatever the bin links as, so the leg is structurally blind to it. The finding already made this point about all three hardware defects. The shape test is the ratchet that keeps the attribute in the file; the measurement is the evidence that the attribute does what the task says it does. Neither is a substitute for the human double-click in the README's list.

**Rejected:** adding a CI step that links the exe and reads its PE header. That is a real option and a better guard, but it needs a new workflow step on the default branch BEFORE it can be dispatched (this repo's standing rule about CI-measurable criteria), which is a bigger change than this defect fix, and the release job already links the binary for other reasons. Worth doing when the Windows packaging leg next changes; recorded here rather than silently skipped.
