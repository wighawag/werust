//! Where werust's own words go once the binary has NO console of its own.
//!
//! `main.rs` links this binary with `#![cfg_attr(windows, windows_subsystem =
//! "windows")]` (task `windows-gui-subsystem-no-console-window`), because a
//! console-subsystem browser drags a black rectangle onto the desktop beside its
//! window -- found on real hardware, `work/notes/findings/
//! windows-shell-first-run-on-real-hardware-2026-07-31.md`. The cost of that one
//! attribute is that `println!` and `eprintln!` have nowhere to go, and this
//! shell has a PRE-SPECIFIED honest failure to report: a machine with no
//! `Microsoft Edge WebView2 Runtime` must be TOLD so, with the download link
//! (`docs/adr/0011` finding 6, carried by `windows_renderer::missing_runtime_error`).
//! Deleting the messages would turn that sentence into a window that never
//! appears and says nothing.
//!
//! So this module gives the failure a surface on BOTH launch paths, and nothing
//! else:
//!
//! * [`attach_parent_console`] borrows the console of the terminal that launched
//!   werust, when there was one. `werust-windows.exe` typed at a prompt keeps
//!   printing exactly as it did before the attribute; a double-clicked one
//!   attaches to nothing and therefore spawns nothing. werust never CREATES a
//!   console (no `AllocConsole`) -- that would re-open the very window this task
//!   closed.
//! * [`report_startup_failure`] puts the failure in a message box, for the launch
//!   that has no console to read: the double-click, the Start-menu shortcut, the
//!   file association.
//!
//! Which of the two is used is decided ONCE, in `main.rs`, by whether a console
//! was attached -- one legible report per launch, on the surface that launched
//! it. Why that rule and not "always both": `docs/spikes/
//! windows-gui-subsystem-no-console-window/DECISIONS.md`.
//!
//! Nothing here decides anything about browsing, trust or wording: the sentence
//! it carries is the seam's error, verbatim.

use std::fs::OpenOptions;
use std::os::windows::io::IntoRawHandle;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_HANDLE,
    STD_OUTPUT_HANDLE,
};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

use crate::win32::wide;

/// The message box's caption: the program, as the taskbar spells it. The BODY is
/// the seam's own sentence, which already names what failed and what to do.
const FAILURE_CAPTION: &str = "werust";

/// The console device a process writes its output to. Opening it by name is how
/// a freshly-attached process gets a handle to the console it just joined.
const CONSOLE_OUTPUT: &str = "CONOUT$";

/// Attach to the console of the process that launched werust, if it had one, and
/// report whether werust can now be HEARD in writing.
///
/// `true` means both standard streams lead somewhere -- the attached console, or
/// a redirection the launcher set up (`werust-windows.exe > log.txt`), which is
/// deliberately left alone rather than overwritten. `false` means nobody is
/// listening on stdout/stderr and a failure must take [`report_startup_failure`]
/// instead.
///
/// Call this BEFORE the first print: Rust's standard streams read their handle
/// per write, so an attach that happens first is seen, and one that happens after
/// is not. Printing when this returned `false` is not merely invisible, it
/// PANICS ("failed printing to stdout"), because there is no handle to write to.
#[must_use]
pub fn attach_parent_console() -> bool {
    // No `AllocConsole` fallback, by design: a browser that could not find a
    // terminal must stay silent on the desktop, not open one.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        bind_missing_stream(STD_OUTPUT_HANDLE);
        bind_missing_stream(STD_ERROR_HANDLE);
    }
    stream_is_usable(STD_OUTPUT_HANDLE) && stream_is_usable(STD_ERROR_HANDLE)
}

/// Show a startup FAILURE to a user who has no console: the launch path that
/// double-clicked werust rather than typing its name.
///
/// A modal box is the only surface such a launch has. It is used ONLY for a
/// failure that prevents the window from appearing at all; everything a running
/// browser has to say belongs in the chrome the core derives.
pub fn report_startup_failure(message: &str) {
    let text = wide(message);
    let caption = wide(FAILURE_CAPTION);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Point one standard stream at the attached console -- unless the launcher
/// already handed us a handle for it, in which case that redirection wins.
///
/// A GUI-subsystem process started from a prompt inherits no console handles, so
/// after `AttachConsole` the streams are still empty and have to be opened by
/// name. The handle is deliberately LEAKED (`into_raw_handle`): it is the
/// process's stdout/stderr for the rest of its life, and closing it would leave
/// `SetStdHandle` pointing at a closed device.
fn bind_missing_stream(stream: STD_HANDLE) {
    if stream_is_usable(stream) {
        return;
    }
    let Ok(console) = OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONSOLE_OUTPUT)
    else {
        return;
    };
    unsafe {
        let _ = SetStdHandle(stream, HANDLE(console.into_raw_handle()));
    }
}

/// Whether a standard stream leads anywhere at all.
fn stream_is_usable(stream: STD_HANDLE) -> bool {
    unsafe { GetStdHandle(stream) }.is_ok_and(|handle| !handle.is_invalid())
}
