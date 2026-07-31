//! Rust TOOLCHAIN PIN shape guard (task
//! `pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main`).
//!
//! WHY A SHAPE GUARD: `verify` denies warnings (`cargo clippy --all-targets --
//! -D warnings`, pinned by `tests/verify_gate_shape.rs`). A deny-warnings gate
//! is only safe when every runner of it compiles with the SAME compiler.
//! It was not: CI installed clippy with a bare `rustup component add` and got
//! whatever the runner shipped (1.97.0), while the development machine — and so
//! every local `verify` run, which is this repo's acceptance bar — was on
//! 1.91.1. `clippy::question_mark` fires on 1.97 and not on 1.91, so a change
//! passed the gate locally and RED-ed `main` minutes later, taking every release
//! job to `skipped` (run 30622910777).
//!
//! The fix is a workspace-root `rust-toolchain.toml`, which `rustup` honours for
//! every proxied `cargo`/`rustc`/`clippy`/`rustfmt` invocation from anywhere in
//! the tree — laptop, `dorfl` gate and all five CI legs alike. That is a
//! property of TEXT in declarative files, which is exactly the kind of contract
//! this repo guards by PARSING the file (`tests/verify_gate_shape.rs`,
//! `tests/release_plumbing_shape.rs`, `tests/windows_renderer_leg_shape.rs`).
//! `werust-core` hosts it for the same reason it hosts those: no GTK/SDK deps,
//! so the assertion runs inside the pure-Rust gate it is describing.
//!
//! What it does NOT claim: that the pin is the LATEST toolchain. It is
//! deliberately a fixed version, so a bump is a reviewable one-file change made
//! when someone is ready to clear the new lints — the property that makes
//! `-D warnings` safe. The rationale + the bump procedure live in
//! `docs/spikes/pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main/README.md`,
//! and the last test below reds if that record stops naming the version that is
//! actually pinned.

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

/// The parsed `rust-toolchain.toml`.
fn toolchain_table() -> toml::Table {
    read_repo_file("rust-toolchain.toml")
        .parse::<toml::Table>()
        .expect("rust-toolchain.toml must be valid TOML")
}

/// The pinned channel string, e.g. `"1.97.0"`.
fn pinned_channel() -> String {
    toolchain_table()
        .get("toolchain")
        .and_then(toml::Value::as_table)
        .expect("rust-toolchain.toml must declare a `[toolchain]` table")
        .get("channel")
        .and_then(toml::Value::as_str)
        .expect("`[toolchain]` must declare a `channel`")
        .to_string()
}

/// Every `.yml`/`.yaml` file under `.github/` — the workflows AND the composite
/// actions they call, since a step in either could select a toolchain.
fn github_yaml_files() -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}"));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path
                .extension()
                .is_some_and(|ext| ext == "yml" || ext == "yaml")
            {
                out.push(
                    path.strip_prefix(root)
                        .expect("path under repo root")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let root = repo_root();
    let mut out = Vec::new();
    walk(&root.join(".github"), &root, &mut out);
    out.sort();
    assert!(!out.is_empty(), ".github must hold workflow YAML");
    out
}

/// Every scalar string reachable from a YAML value, flattened. Same helper as
/// `tests/verify_gate_shape.rs`: it catches `run:` script bodies AND `uses:`
/// action references, which are the two ways a step can pick a toolchain.
fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Sequence(seq) => seq.iter().for_each(|e| collect_strings(e, out)),
        Value::Mapping(m) => m.values().for_each(|e| collect_strings(e, out)),
        _ => {}
    }
}

fn yaml_strings(rel: &str) -> Vec<String> {
    let doc: Value = serde_yaml::from_str(&read_repo_file(rel))
        .unwrap_or_else(|e| panic!("{rel} must be valid YAML: {e}"));
    let mut out = Vec::new();
    collect_strings(&doc, &mut out);
    out
}

#[test]
fn the_workspace_pins_an_exact_toolchain_channel() {
    // A floating channel (`stable`, `beta`, `nightly`, or a bare `1.97` that
    // tracks the newest patch) reintroduces the exact failure this task exists
    // to close: two runners of the same `-D warnings` gate compiling with two
    // different clippies. The pin must name ONE resolvable version.
    let channel = pinned_channel();
    let exact = channel.split('.').count() == 3
        && channel
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    assert!(
        exact,
        "rust-toolchain.toml must pin an EXACT `major.minor.patch` channel so every \
         machine and every CI leg resolves the same compiler; got {channel:?}"
    );
}

#[test]
fn the_pin_carries_the_components_the_gate_runs() {
    // `verify` runs `cargo fmt --check` and `cargo clippy`. Both live in
    // OPTIONAL rustup components, so pinning the channel without them would
    // leave CI needing a `rustup component add` — and it is precisely that
    // unpinned step this task deletes. Declaring them here is what lets the
    // workflows drop it.
    let toolchain = toolchain_table();
    let components: Vec<String> = toolchain
        .get("toolchain")
        .and_then(toml::Value::as_table)
        .expect("`[toolchain]` table")
        .get("components")
        .and_then(toml::Value::as_array)
        .expect("`[toolchain]` must declare `components`")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("each component must be a string")
                .to_string()
        })
        .collect();
    for required in ["rustfmt", "clippy"] {
        assert!(
            components.iter().any(|c| c == required),
            "`[toolchain].components` must include {required:?} (the `verify` gate runs it); \
             got {components:?}"
        );
    }
}

#[test]
fn no_ci_step_selects_a_toolchain_of_its_own() {
    // The pin is only a pin if nothing overrides it. `rustup component add`
    // (the step this task removes) is the historical offender, but a
    // `rustup default`/`override`/`toolchain install`, a `cargo +1.99.0 …`
    // proxy call or a setup-toolchain ACTION would each re-open the same gap.
    //
    // `rustup target add <triple>` is deliberately NOT here: a target is
    // ADDITIVE and rustup installs it INTO the active (pinned) toolchain, which
    // is how the mobile/macOS/Windows legs stay correct without selecting
    // anything.
    const FORBIDDEN_RUN_FRAGMENTS: &[&str] = &[
        "rustup component add",
        "rustup toolchain install",
        "rustup default",
        "rustup override",
        "rustup update",
        "cargo +",
        "rustc +",
    ];
    const FORBIDDEN_ACTIONS: &[&str] = &[
        "dtolnay/rust-toolchain",
        "actions-rs/toolchain",
        "actions-rust-lang/setup-rust-toolchain",
    ];
    for file in github_yaml_files() {
        for s in yaml_strings(&file) {
            for fragment in FORBIDDEN_RUN_FRAGMENTS {
                assert!(
                    !s.contains(fragment),
                    "{file} must not run `{fragment}`: it overrides the \
                     rust-toolchain.toml pin, which is what let CI and the local \
                     `verify` gate drift six minor versions apart; got {s:?}"
                );
            }
            for action in FORBIDDEN_ACTIONS {
                assert!(
                    !s.starts_with(action),
                    "{file} must not use the toolchain-selecting action `{action}`: \
                     the workspace pin is the one source of the compiler version; got {s:?}"
                );
            }
        }
    }
}

#[test]
fn the_pinned_version_and_its_rationale_are_recorded() {
    // Acceptance asks that the pinned version AND why that version is written
    // down. A record that names a version the repo no longer pins is worse than
    // none, so the spike must quote the pin verbatim.
    let spike = read_repo_file(
        "docs/spikes/pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main/README.md",
    );
    let channel = pinned_channel();
    assert!(
        spike.contains(&channel),
        "the spike record must name the pinned channel ({channel}) so the reason for \
         THIS version, and the bump procedure, cannot drift from the pin"
    );
}
