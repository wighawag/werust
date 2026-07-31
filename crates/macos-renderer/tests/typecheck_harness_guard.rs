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
//!
//! WHY THE VICTIM DIRECTORY IS CHOSEN AND NOT HARD-CODED. The first version of
//! this test built its victim under `target/`, calling that "not a temp root".
//! That is an assumption about where the REPOSITORY lives, and it is false in the
//! environment that matters: the acceptance gate runs in a throwaway worktree cut
//! under a temp root, as does any CI runner that checks out under `/tmp`. There
//! the victim was legitimately inside a temp root, the guard CORRECTLY allowed
//! the delete, and the test read that as the guard failing. So the victim is now
//! chosen at run time from places this test owns, by the SAME rule the script
//! applies, and the assertion tests the guard rather than the checkout location.

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

/// The roots the harness allows itself to delete strictly below: the same list
/// the script walks, resolved the same way (it compares resolved paths, so a
/// symlinked `/tmp` cannot smuggle anything past either side).
#[cfg(unix)]
fn temp_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(tmpdir) = std::env::var_os("TMPDIR") {
        roots.push(PathBuf::from(tmpdir));
    }
    roots.push(PathBuf::from("/tmp"));
    roots.push(PathBuf::from("/var/tmp"));
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
}

/// An absolute, symlink-free form of a path that need NOT exist yet: resolve the
/// deepest existing ancestor and re-append the rest, which is what the script's
/// own `absolute_path` does.
#[cfg(unix)]
fn resolved(path: &Path) -> PathBuf {
    let mut head = path.to_path_buf();
    let mut tail = PathBuf::new();
    while !head.is_dir() {
        let (Some(name), Some(parent)) = (head.file_name(), head.parent()) else {
            break;
        };
        tail = Path::new(name).join(&tail);
        head = parent.to_path_buf();
    }
    head.canonicalize().unwrap_or(head).join(tail)
}

/// Whether the harness would consider this path deletable.
#[cfg(unix)]
fn is_under_a_temp_root(path: &Path) -> bool {
    let path = resolved(path);
    temp_roots()
        .iter()
        .any(|root| path.starts_with(root) && path != *root)
}

/// A directory this test owns that is provably OUTSIDE every temp root, wherever
/// the repository happens to be checked out. `$HOME` comes first because it is
/// the one location a developer machine and a CI runner agree on and neither
/// treats as scratch; the repo's own `target/` and the current directory follow
/// for the odd host with no `HOME`. Each candidate is checked with the guard's
/// rule rather than assumed, so this returns `None` rather than a wrong answer
/// when a host has genuinely put everything under a temp root.
#[cfg(unix)]
fn a_probe_dir_outside_every_temp_root() -> Option<PathBuf> {
    let bases = [
        std::env::var_os("HOME").map(PathBuf::from),
        Some(repo_root().join("target")),
        std::env::current_dir().ok(),
    ];
    bases
        .into_iter()
        .flatten()
        .map(|base| base.join(".werust-typecheck-harness-guard-probe"))
        .find(|candidate| !is_under_a_temp_root(candidate))
}

#[cfg(unix)]
#[test]
fn the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root() {
    // The exact operator mistake this guards: `SCRATCH_DIR` pointing at a working
    // directory (exported in a shell days ago, or mistyped). The harness must
    // REFUSE, legibly, and leave every file where it found it.
    let Some(victim) = a_probe_dir_outside_every_temp_root() else {
        // No location on this host is outside a temp root -- `HOME`, the checkout
        // and the working directory are all inside one -- so the refusal cannot
        // be provoked here. Assert the CONVERSE instead of skipping: the guard
        // still has to ALLOW the delete it exists to permit, and a guard whose
        // teeth are quietly ignored is the footgun it was meant to close.
        assert_the_harness_deletes_a_scratch_dir_under_a_temp_root();
        return;
    };
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
        "the harness deleted a SCRATCH_DIR outside a temp root ({}): it must refuse instead",
        victim.display()
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

/// The other half of the guard's teeth, used where the refusal cannot be
/// provoked: a `SCRATCH_DIR` that IS under a temp root must still be rebuilt from
/// nothing, or the harness would refuse its own default and be simply broken.
///
/// `rustup` and `cargo` are stubbed on `PATH` so this costs a shell run rather
/// than a cross-target `cargo clippy` (and never reaches the network): the
/// delete happens in the script's own prologue, long before any real work.
#[cfg(unix)]
fn assert_the_harness_deletes_a_scratch_dir_under_a_temp_root() {
    let unique = std::process::id();
    let stubs = std::env::temp_dir().join(format!("werust-harness-guard-stubs-{unique}"));
    std::fs::create_dir_all(&stubs).unwrap_or_else(|e| panic!("create the stub bin dir: {e}"));
    write_stub(
        &stubs,
        "rustup",
        "#!/bin/sh\n# the harness only asks whether the darwin std is installed\necho aarch64-apple-darwin\n",
    );
    write_stub(&stubs, "cargo", "#!/bin/sh\nexit 0\n");

    let scratch = std::env::temp_dir().join(format!("werust-harness-guard-allowed-{unique}"));
    std::fs::create_dir_all(&scratch).unwrap_or_else(|e| panic!("create the scratch dir: {e}"));
    let doomed = scratch.join("stale-workspace.txt");
    std::fs::write(&doomed, b"a previous run's scratch workspace")
        .unwrap_or_else(|e| panic!("write the scratch file: {e}"));

    let path = match std::env::var_os("PATH") {
        Some(existing) => format!("{}:{}", stubs.display(), existing.to_string_lossy()),
        None => stubs.display().to_string(),
    };
    let output = Command::new("bash")
        .arg(repo_root().join(HARNESS))
        .env("SCRATCH_DIR", &scratch)
        .env("PATH", path)
        .output()
        .expect("run the committed macOS type-check harness");

    let rebuilt = !doomed.exists();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = output.status;
    std::fs::remove_dir_all(&scratch).ok();
    std::fs::remove_dir_all(&stubs).ok();

    assert!(
        !stderr.contains("REFUSING"),
        "the guard must ALLOW a SCRATCH_DIR under a temp root -- the default lives there -- got:\n{stderr}"
    );
    assert!(
        rebuilt,
        "the harness must rebuild its scratch workspace from nothing, leaving no stale file behind"
    );
    assert!(
        status.success(),
        "the harness must run through with `rustup`/`cargo` stubbed, got {status}:\n{stderr}"
    );
}

#[cfg(unix)]
fn write_stub(dir: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write the {name} stub: {e}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("make the {name} stub executable: {e}"));
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
