// werust is a BROWSER, so on Windows it links as a GUI application: without this
// the binary is a CONSOLE-subsystem app and Windows opens a console window
// beside the browser window, which is what the first run on real hardware found
// (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`,
// task `windows-gui-subsystem-no-console-window`).
//
// `cfg`-gated because this binary deliberately still COMPILES on every host --
// that is what keeps its host-independent half inside the Ubuntu `verify` gate,
// and the `#[cfg(not(windows))]` arm below is what it does there. The attribute
// changes the LINK only, so no test can observe it; it is pinned instead by
// `tests/windows_window_shape.rs`.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! The `werust-windows` entry point: open the Win32 window on a URL.
//!
//! See `lib.rs` for what this shell is and what it deliberately leaves to the
//! engine crate. The dispatch here is intentionally tiny -- werust's verb-first
//! headless CLI (`resolve`, `version`) lives in the `werust` binary and is
//! toolkit-free, so it is not re-implemented for Windows; this binary opens a
//! window, which is the one thing only it can do.

use std::process::ExitCode;

/// The URL werust opens when none is given on the command line (the same default
/// the GTK and AppKit shells use).
const DEFAULT_URL: &str = "https://example.com/";

fn main() -> ExitCode {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_URL.into());
    run(&url)
}

#[cfg(windows)]
fn run(url: &str) -> ExitCode {
    // A GUI-subsystem binary gets no console, so werust borrows the console of
    // the terminal that launched it, and borrows nothing when it was
    // double-clicked, which is the whole point. Printing before this call, or
    // printing at all when it returned false, PANICS: there is no handle to
    // write to. See `werust_windows::startup`.
    let console = werust_windows::startup::attach_parent_console();
    if console {
        // Kept for the terminal launch, where a version line is what the person
        // typing asked for; a double-clicked browser has nobody to print it to.
        // The same string is on the ⋮ menu and behind `werust version`.
        println!(
            "werust {} — a Rust web browser (Windows WebView2 backend)",
            werust_core::version()
        );
    }
    match werust_windows::window::run(url) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // The one legible report, on the surface this launch can see. A
            // startup failure is PRE-SPECIFIED product behaviour here (a machine
            // with no WebView2 Runtime is told so, with the download link), so it
            // must never be swallowed just because there is no console.
            if console {
                eprintln!("werust: {e}");
            } else {
                werust_windows::startup::report_startup_failure(&e.to_string());
            }
            ExitCode::FAILURE
        }
    }
}

/// On any other host this binary refuses LOUDLY rather than pretending to be a
/// browser: it still COMPILES everywhere, which is what keeps the window's
/// host-independent half inside the Ubuntu `verify` gate.
#[cfg(not(windows))]
fn run(_url: &str) -> ExitCode {
    eprintln!(
        "werust-windows is the Win32 shell and only runs on Windows.\n\
         On Linux run `cargo run -p werust` (the GTK shell) instead."
    );
    ExitCode::FAILURE
}
