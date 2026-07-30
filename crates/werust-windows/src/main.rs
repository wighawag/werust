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
    println!(
        "werust {} — a Rust web browser (Windows WebView2 backend)",
        werust_core::version()
    );
    match werust_windows::window::run(url) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("werust: {e}");
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
