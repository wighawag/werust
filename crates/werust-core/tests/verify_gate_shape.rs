//! `verify` GATE shape guard (task
//! `verify-lints-test-targets-and-clears-the-existing-debt`, origin: the
//! observation `work/notes/observations/verify-clippy-does-not-lint-test-targets-2026-07-30.md`).
//!
//! WHY A SHAPE GUARD: the acceptance bar of this repo is one command, declared
//! in `dorfl.json`'s `verify` and re-typed in two GitHub workflows. Nothing in
//! the tree checked what that command SAYS, so two silent regressions were both
//! one careless edit away:
//!
//! * dropping `--all-targets`, which puts every `#[cfg(test)]` module (the
//!   PRIMARY evidence surface in this repo) back outside the linter, and
//! * letting the CI legs drift from the local gate, so `dorfl verify` and the
//!   workflow that claims to be "identical to dorfl.json's verify" quietly stop
//!   being the same bar.
//!
//! Both are properties of TEXT in declarative files, which is exactly the kind
//! of contract this repo guards by PARSING the file (see
//! `tests/release_plumbing_shape.rs`, `tests/windows_renderer_leg_shape.rs`).
//! `werust-core` hosts it for the same reason it hosts the release-plumbing
//! guard: it carries no GTK/SDK deps, so the assertion runs inside the pure-Rust
//! gate it is describing.
//!
//! Note what this guard does NOT claim: that clippy covers every crate. The
//! Ubuntu gate cannot compile the `#[cfg(target_os = "macos")]` /
//! `#[cfg(windows)]` halves of the platform crates, so `--all-targets` lints
//! their host-independent halves and their platform halves stay unlinted there.
//! The honest inventory is in
//! `docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md`.

use std::path::{Path, PathBuf};

use serde_yaml::Value;

/// The workspace root: this crate lives at `crates/werust-core`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("resolve workspace root from crates/werust-core")
}

fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `dorfl.json`'s `verify` command: THE acceptance bar, in one string.
fn verify_command() -> String {
    let cfg: serde_json::Value =
        serde_json::from_str(&read_repo_file("dorfl.json")).expect("dorfl.json must be valid JSON");
    cfg.get("verify")
        .and_then(serde_json::Value::as_str)
        .expect("dorfl.json must declare a `verify` command")
        .to_string()
}

/// The `verify` command split into the individual shell commands it chains with
/// `&&`, each trimmed. This is the list a CI leg has to reproduce step for step.
fn verify_steps() -> Vec<String> {
    verify_command()
        .split("&&")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The single `cargo clippy …` command inside `verify`.
fn clippy_step() -> String {
    let steps = verify_steps();
    let mut found = steps.into_iter().filter(|s| s.starts_with("cargo clippy"));
    let step = found
        .next()
        .expect("`verify` must run `cargo clippy` (the lint half of the gate)");
    assert!(
        found.next().is_none(),
        "`verify` must run exactly ONE `cargo clippy` invocation, so there is one lint bar to reason about"
    );
    step
}

/// Every scalar string reachable from a YAML value, flattened.
fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Sequence(seq) => seq.iter().for_each(|e| collect_strings(e, out)),
        Value::Mapping(m) => m.values().for_each(|e| collect_strings(e, out)),
        _ => {}
    }
}

/// The `run:` command lines of a named job in a workflow, as one flattened list.
fn job_run_lines(workflow: &str, job: &str) -> Vec<String> {
    let text = read_repo_file(workflow);
    let wf: Value = serde_yaml::from_str(&text).expect("workflow must be valid YAML");
    let jobs = wf
        .get("jobs")
        .and_then(Value::as_mapping)
        .unwrap_or_else(|| panic!("{workflow} must declare `jobs:`"));
    let job_value = jobs
        .get(Value::String(job.into()))
        .unwrap_or_else(|| panic!("{workflow} must declare a `{job}` job"));
    let mut out = Vec::new();
    collect_strings(job_value, &mut out);
    out
}

/// The two places the gate is re-typed for CI, and the job that runs it.
const CI_LEGS: &[(&str, &str)] = &[
    (".github/workflows/verify.yml", "verify"),
    (".github/workflows/release.yml", "verify"),
];

#[test]
fn the_gate_lints_test_targets_not_just_lib_and_bin() {
    // THE point of this task. Bare `cargo clippy` lints lib/bin targets only, so
    // every `#[cfg(test)]` module, integration test and example was unlinted,
    // and in this repo the tests (source-shape guards, parity guards,
    // recorded-verdict guards) are where most new code lands.
    let step = clippy_step();
    assert!(
        step.contains("--all-targets"),
        "`verify`'s clippy must pass `--all-targets` so test targets, integration \
         tests and the load-bearing `examples/` are linted too; got {step:?}"
    );
}

#[test]
fn a_clippy_warning_actually_fails_the_gate() {
    // `cargo clippy` EXITS 0 on warnings. Without a deny flag the lint half of
    // the gate is advisory: it prints and passes, so `--all-targets` alone would
    // widen a bar that does not bite. The gate must therefore turn warnings into
    // errors, or the criterion "a deliberate test-only lint reds it" is false.
    let step = clippy_step();
    assert!(
        step.contains("-D warnings") || step.contains("--deny warnings"),
        "`verify`'s clippy must deny warnings (`-- -D warnings`), otherwise it exits 0 \
         on every lint it finds and the gate has no teeth; got {step:?}"
    );
}

#[test]
fn every_ci_leg_runs_the_same_gate_as_dorfl_json() {
    // `verify.yml` says in a comment that it is "identical to dorfl.json's
    // verify", and `release.yml` gates the tag build on the same claim. Nothing
    // checked it, so a stricter local gate could land while CI kept waving the
    // old one through. Each step of `verify` must appear VERBATIM as a run line.
    let steps = verify_steps();
    for (workflow, job) in CI_LEGS {
        let runs = job_run_lines(workflow, job);
        for step in &steps {
            assert!(
                runs.iter().any(|r| r.trim() == step),
                "{workflow}'s `{job}` job must run `{step}` verbatim (it claims parity with \
                 dorfl.json's verify); got {runs:?}"
            );
        }
    }
}
