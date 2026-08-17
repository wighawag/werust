//! Trusted-name PIN STORE edge-wiring shape guard (task
//! `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`, spec
//! `ipns-tofu-pin-and-warn-on-change`'s trust-on-first-use model).
//!
//! WHAT LANDED: a [`BrowserShell`](werust_core::BrowserShell) reads and writes the
//! user's `pins.json` only when its constructor was ASKED to
//! (`with_settings_pins`). The default is NO store, which is what keeps every test
//! in every crate off the developer's own blessed names — including the mobile
//! crates', which build shells through their production `CoreSession::new()` and
//! so could never be covered by a `cfg!` test branch inside `werust-core` (that
//! branch was this repo's only one, and retiring it is decision 3 of
//! `docs/spikes/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns/DECISIONS.md`).
//!
//! WHY A SOURCE-SHAPE GUARD: an opt-in inverts the failure direction. Hermeticity
//! is now safe by default, but an edge that FORGETS to ask silently loses TOFU
//! persistence — the user blesses a name, relaunches, and the pin is simply not
//! there, with nothing on screen to say so. That is the security-relevant
//! direction of failure the whole pin-store drive is about, so it is not left to
//! memory: this test reds the gate the moment a production entry point stops
//! asking. It has to be a SOURCE guard because three of the five entry points are
//! unreachable from this gate — the Android JNI module is
//! `cfg(target_os = "android")`, and the macOS/Windows windows only build on their
//! own OS — which is the same reason `debug_capture_edge_wiring_shape.rs` and
//! `browser_menu_edge_wiring_shape.rs` parse their edges rather than call them.
//!
//! It lives in `werust-core` (not one edge's crate) because it spans every edge,
//! and `werust-core` is the one crate they all sit over.
//!
//! Acceptance criteria mapped to assertions below:
//! * The store is reachable from production, and ONLY from production
//!   (`every_production_entry_point_asks_for_the_users_pin_store`).
//! * Hermeticity is the DEFAULT, not a per-test opt-in
//!   (`a_shell_nobody_asked_reads_no_store_at_all`).
//! * The retired `cfg!(test)` precedent does not come back
//!   (`production_code_does_not_branch_on_being_a_test_build`).

use std::path::{Path, PathBuf};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The slice of `text` that FOLLOWS `marker`, bounded to `len` bytes: enough to
/// read one construction site's builder chain without pinning its exact
/// formatting (which `cargo fmt` owns).
fn after(text: &str, marker: &str, len: usize) -> String {
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("the source no longer contains `{marker}`"));
    let rest = &text[start..];
    rest[..len.min(rest.len())].to_string()
}

/// The one call that hands a shell the user's real `pins.json`.
const OPT_IN: &str = "with_settings_pins()";

#[test]
fn every_production_entry_point_asks_for_the_users_pin_store() {
    // The five places a REAL user's browsing session is constructed. Each must ask
    // for the durable store; nothing else may. A new edge that builds a shell and
    // forgets this line ships a browser whose blesses evaporate on relaunch.
    for (file, marker, what) in [
        (
            "crates/werust/src/main.rs",
            "BrowserShell::new(Box::new(backend))",
            "the GTK window",
        ),
        (
            "crates/werust-macos/src/window.rs",
            "BrowserShell::new(Box::new(backend))",
            "the macOS window",
        ),
        (
            "crates/werust-windows/src/window.rs",
            "BrowserShell::new(Box::new(backend))",
            "the Windows window",
        ),
        (
            "crates/werust-android/rust/src/lib.rs",
            "fn Java_com_github_wighawag_werust_WerustCore_nativeNew(",
            "the Android JNI session constructor",
        ),
        (
            "crates/werust-ios/rust/src/lib.rs",
            "fn werust_ios_session_new()",
            "the iOS C-ABI session constructor",
        ),
    ] {
        let site = after(&source(file), marker, 600);
        assert!(
            site.contains(OPT_IN),
            "{what} ({file}) must call `{OPT_IN}`, or the user's blessed names \
             are read and written nowhere and every bless is lost on relaunch:\n{site}"
        );
    }
}

#[test]
fn a_shell_nobody_asked_reads_no_store_at_all() {
    // The default is the whole mechanism: hermeticity must not be something each
    // test remembers to ask for, because the crates that need it most build their
    // shells through a PRODUCTION constructor (`CoreSession::new`) they do not
    // own. The BEHAVIOUR is asserted where a shell can be built over a fake
    // backend (`werust_core`'s own
    // `a_test_shell_starts_from_an_empty_store_and_never_touches_the_real_pins_json`,
    // and each mobile crate's
    // `a_test_session_reads_no_pin_store_and_never_touches_the_real_pins_json`);
    // what is guarded HERE is that the default itself stays that way.
    let core = source("crates/werust-core/src/lib.rs");
    assert!(
        core.contains("#[default]\n    Ephemeral,"),
        "the pin-store location must DEFAULT to no store"
    );
}

#[test]
fn production_code_does_not_branch_on_being_a_test_build() {
    // `cfg!(test)` in production code was this repo's only such branch, and its
    // per-CRATE nature is precisely why the mobile crates' tests kept reading the
    // developer's real `pins.json` long after `werust-core`'s stopped: a branch
    // here cannot see across a crate boundary, but a default can. The explicit
    // opt-in replaced it, so the precedent should not quietly return.
    //
    // `#[cfg(test)]` ITEMS (a test module, a test-only accessor) are a different
    // thing and are untouched: what is forbidden is production BEHAVIOUR that
    // differs in a test build.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0;
    for entry in std::fs::read_dir(&src).expect("werust-core/src is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable source file");
        // Comments may DISCUSS the retired branch; only code may not use it.
        let code: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("cfg!(test)"),
            "{} branches production behaviour on being a test build",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 5, "the guard must actually have read the sources");
}
