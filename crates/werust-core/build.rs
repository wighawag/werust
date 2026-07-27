//! Resolve werust's ONE version string at build time and hand it to the crate
//! as `WERUST_VERSION`, which [`werust_core::version`] reads.
//!
//! Every version surface werust has — the desktop startup banner and the browser
//! menu on all three platforms — reads that single accessor, so this build script
//! is the ONE place a version enters the compiled code. Before it existed the
//! accessor read `CARGO_PKG_VERSION`, and nothing ever injected a real version
//! into the Rust build (GoReleaser derives only the ARCHIVE NAME from the tag),
//! so a tagged `v0.2.6` build shipped menus reading `werust 0.0.0`.
//!
//! The precedence and the normalisation live in
//! [`src/version_resolution.rs`](src/version_resolution.rs), `include!`d below so
//! the SAME code the build runs is also unit-tested in the pure-Rust `verify`
//! gate (a build script cannot itself be `cargo test`ed). In short:
//! `WERUST_VERSION` (CI injects it from the release tag) -> `git describe
//! --tags --always` (an informative dev build) -> `CARGO_PKG_VERSION`.
//!
//! It NEVER fails the build: no git, a tarball without `.git`, or a `git` that
//! errors all simply fall through to the Cargo version.
//!
//! # What is deliberately NOT re-run
//!
//! Only a changed `WERUST_VERSION` (and this script's own two files) re-runs the
//! resolution. A new COMMIT in a local dev checkout does not, so a warm local
//! build can show a slightly stale `git describe` suffix until something else
//! forces a rebuild. That is accepted rather than watched via `.git/HEAD`, which
//! is not even a directory entry in a git WORKTREE (where `.git` is a file) and
//! would tie the build to git's internal layout. The path that must be exact — a
//! tagged CI release — builds from a fresh checkout with `WERUST_VERSION`
//! injected, so it is unaffected.

use std::process::Command;

include!("src/version_resolution.rs");

fn main() {
    // A changed injection must REBUILD, or a tagged CI build would happily reuse
    // a cached artifact compiled with the previous (or no) version.
    println!("cargo:rerun-if-env-changed=WERUST_VERSION");
    // Emitting ANY `rerun-if-*` instruction replaces cargo's default "rerun when
    // any file in the package changes", so the two files this script is MADE of
    // must be named explicitly or an edit to them would not take effect.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/version_resolution.rs");

    let injected = std::env::var("WERUST_VERSION").ok();
    let described = git_describe();
    let cargo_version = std::env::var("CARGO_PKG_VERSION")
        .expect("cargo always sets CARGO_PKG_VERSION for a build script");

    let version = resolve_version(injected.as_deref(), described.as_deref(), &cargo_version);
    println!("cargo:rustc-env=WERUST_VERSION={version}");
}

/// `git describe --tags --always` in the crate's source tree, or [`None`] when
/// git is absent, the tree is not a checkout (an unpacked source tarball), or
/// the command fails for any other reason.
///
/// Deliberately failure-TOLERANT: a missing version source must degrade to the
/// Cargo version, never break a build. `--always` means a checkout with no
/// reachable tag still yields a short hash rather than an error.
fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}
