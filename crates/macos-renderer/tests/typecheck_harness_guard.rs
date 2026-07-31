//! The `SCRATCH_DIR` guard on the committed macOS type-check harness (task
//! `macos-spike-doc-accuracy-and-harness-guard`, item 1).
//!
//! WHY THIS IS A TEST AND NOT A README LINE:
//! `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh`
//! rebuilds its scratch workspace from nothing on every run, so it `rm -rf`s that
//! directory first -- and the directory is CALLER-supplied via `SCRATCH_DIR`. The
//! default is under a temp root and safe; an exported or mistyped `SCRATCH_DIR`
//! pointing at a working directory was not, and a committed dev harness that eats
//! a directory on a typo is one nobody should keep running. The guard is
//! therefore behaviour, and behaviour that deletes files is worth EXECUTING in
//! the ordinary Ubuntu gate rather than trusting to review.
//!
//! It is cheap: the guard refuses BEFORE any `rustup`/`cargo` work, so the
//! refusal path costs one `bash` process and touches no network.

use std::path::{Path, PathBuf};
use std::process::Command;

const HARNESS: &str = "docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[cfg(unix)]
#[test]
fn the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root() {
    // The exact operator mistake this guards: `SCRATCH_DIR` pointing at a working
    // directory (exported in a shell days ago, or mistyped). The harness must
    // REFUSE, legibly, and leave every file where it found it.
    //
    // The victim lives under `target/` because it must NOT be under a temp root
    // (which is precisely what the harness is allowed to delete), and `target/` is
    // both git-ignored and a real working directory of this repo.
    let victim = repo_root().join("target/typecheck-harness-guard-probe");
    std::fs::create_dir_all(&victim).unwrap_or_else(|e| panic!("create the probe dir: {e}"));
    let precious = victim.join("precious.txt");
    std::fs::write(&precious, b"an operator's working directory")
        .unwrap_or_else(|e| panic!("write the probe file: {e}"));

    let output = Command::new("bash")
        .arg(repo_root().join(HARNESS))
        .env("SCRATCH_DIR", &victim)
        .output()
        .expect("run the committed macOS type-check harness");

    // The file survived. This is the assertion that matters; the rest is about
    // the operator being TOLD why.
    let survived = precious.exists();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = output.status;
    std::fs::remove_dir_all(&victim).ok();

    assert!(
        survived,
        "the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead"
    );
    assert!(
        !status.success(),
        "refusing must FAIL the run (a silent skip reads as a clean type-check), got {status}"
    );
    for phrase in ["SCRATCH_DIR", "temp root"] {
        assert!(
            stderr.contains(phrase),
            "the refusal must say what it refused and why (missing `{phrase}`), got:\n{stderr}"
        );
    }
}

#[test]
fn the_harnesss_default_scratch_dir_stays_under_a_temp_root() {
    // The guard is only tolerable because the DEFAULT is inside what it allows:
    // a harness that refused its own default would just be broken. Pinned on the
    // source rather than by running it, because the accepting path is a full
    // `cargo clippy` against `aarch64-apple-darwin` (minutes, and a toolchain
    // target the gate need not have installed).
    let harness = source(HARNESS);
    assert!(
        harness.contains(r#"SCRATCH="${SCRATCH_DIR:-${TMPDIR:-/tmp}/werust-macos-typecheck}""#),
        "the default scratch workspace must stay under the temp root the guard allows"
    );
}
