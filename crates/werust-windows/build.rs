//! Embed werust's Win32 application manifest into every binary this crate
//! produces (task `windows-release-packaging-leg`).
//!
//! The manifest itself is [`app.manifest`](app.manifest), which explains what it
//! declares (comctl32 v6 visual styles, per-monitor-v2 DPI awareness) and what
//! each declaration costs. This script only decides HOW it gets in.
//!
//! # The mechanism, and why this one
//!
//! An application manifest reaches an executable as an `RT_MANIFEST` RESOURCE.
//! There are two ways to put one there from a Cargo build:
//!
//! * a resource COMPILER (`embed-resource` / `winres` driving `rc.exe`), or
//! * the MSVC LINKER, which has embedding built in: `/MANIFEST:EMBED` plus
//!   `/MANIFESTINPUT:<file>`.
//!
//! werust takes the linker route. It adds NO dependency to the tree (the reason
//! that matters here is the same one `docs/spikes/windows-win32-window-and-chrome/DECISIONS.md`
//! §1 gives for plain Win32: this is the trust-carrying browser chrome, and a
//! build-time dependency is still a dependency), and it needs no resource
//! compiler on the box — which is what keeps the Linux-hosted cross-target
//! type-check harness (`docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh`)
//! working: it runs `cargo xwin clippy`, which never links, so these flags are
//! simply inert there rather than demanding an `rc.exe` that Linux does not
//! have.
//!
//! The cost, recorded rather than hidden: `/MANIFEST:EMBED` is a `link.exe`
//! flag, so this works for `*-pc-windows-msvc` and nothing else. That is the only
//! Windows target werust ships (`docs/adr/0011` finding 6: the MSVC target
//! statically links the WebView2 loader, which is what makes a single-exe zip
//! possible), and a `*-pc-windows-gnu` build simply gets no manifest rather than
//! a broken link.
//!
//! # Why the EXAMPLES get it too
//!
//! `examples/window_smoke.rs` is the only place this window is EXECUTED
//! anywhere: on Windows CI, `cargo run -p werust-windows --example window_smoke`
//! constructs the real window and reads its real widgets back. comctl32 v6 is a
//! different DLL with different control behaviour (it is what made the tooltip's
//! `cbSize` load-bearing in the first place), so shipping a manifested product
//! while smoking an unmanifested build would test a configuration nobody ships.

use std::path::Path;

/// The manifest, beside this script in the crate root.
const MANIFEST: &str = "app.manifest";

fn main() {
    // Emitting any `rerun-if-*` replaces cargo's default "rerun when any file in
    // the package changes", so both files this decision is made of are named.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={MANIFEST}");

    // `link.exe` flags: MSVC-target only. The Ubuntu `verify` gate builds this
    // crate too (its host-independent half), and so does every other host.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    // An ABSOLUTE path: the linker runs with a working directory that is not
    // this crate's, so a relative `/MANIFESTINPUT:` would silently not resolve.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST);
    let manifest = manifest.display();

    // Applied to the binaries that RUN: the shipped `werust-windows.exe` and the
    // `window_smoke` example (see the module docs). Deliberately not to test
    // targets, which link nothing that opens a window.
    for scope in ["rustc-link-arg-bins", "rustc-link-arg-examples"] {
        println!("cargo:{scope}=/MANIFEST:EMBED");
        println!("cargo:{scope}=/MANIFESTINPUT:{manifest}");
    }
}
