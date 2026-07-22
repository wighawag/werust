//! Release-plumbing shape test (task
//! `release-goreleaser-rust-desktop-and-mobile-artifacts`, spec story 19,
//! `docs/adr/0002`).
//!
//! This is the agreed seam for a CI/release-config task: the deliverables are
//! two declarative files (`.goreleaser.yaml` + `.github/workflows/release.yml`)
//! whose SHAPE is the contract. There is no Rust production code to unit-test,
//! so the objective bar is "do these files declare the release the acceptance
//! criteria name" — asserted here by PARSING them (not string-grepping) so the
//! test breaks if a future edit drifts the shape.
//!
//! `werust-core` hosts it because it is the ONE shared artifact all three
//! release legs build (the desktop GoReleaser leg compiles the binary that links
//! it; the Android + iOS legs cross-compile it into their mobile artifacts) and
//! it carries no GTK/SDK deps, so the assertion runs inside the pure-Rust
//! `verify` gate (`cargo test`) with no extra toolchain.
//!
//! Acceptance criteria mapped to assertions below (updated by task
//! `fix-release-native-x86-desktop-and-decouple-mobile`, human choice B+C —
//! desktop is now native x86_64-only with NO Zig, and the mobile legs are
//! DECOUPLED from the desktop leg; see `docs/adr/0002` "Update" section):
//! 1. `.goreleaser.yaml` uses `builder: rust` and produces the desktop Linux
//!    x86_64 binary NATIVELY (`command: build`, no `command: zigbuild`, no
//!    cargo-zigbuild install, no arm64 target) + checksums.
//! 2. The changelog is generated from conventional-commit git history (no
//!    per-change changeset files) — `changelog.use: git` + conventional groups.
//! 3. The release workflow builds + attaches the Android debug APK and the iOS
//!    Simulator `.app` zip INDEPENDENTLY of the desktop leg (`needs: verify`,
//!    NOT `needs: goreleaser`), guaranteeing the Release exists on a tag via an
//!    idempotent `gh release create ... || true` (a Release-EXISTENCE guarantee,
//!    not a desktop-build dependency).
//! 4. A `workflow_dispatch` dry-run builds everything via snapshot + uploads
//!    workflow artifacts WITHOUT publishing a release.
//! 5. The same `verify` gate runs before a tag build so a tag can't ship a red
//!    tree.
//! 6. The desktop leg carries NO Zig: no `command: zigbuild`, no cargo-zigbuild
//!    install hook, and the release workflow has no "Set up Zig" step.

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

fn load_yaml(rel: &str) -> Value {
    let path = repo_root().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {} as YAML: {e}", path.display()))
}

/// Every scalar string reachable from a YAML value, flattened. Lets a shape
/// assertion look for a substring "somewhere in this job's run-steps" without
/// pinning the exact step layout.
fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Sequence(seq) => seq.iter().for_each(|e| collect_strings(e, out)),
        Value::Mapping(m) => m.values().for_each(|e| collect_strings(e, out)),
        _ => {}
    }
}

fn strings_of(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_strings(v, &mut out);
    out
}

fn contains_substr(v: &Value, needle: &str) -> bool {
    strings_of(v).iter().any(|s| s.contains(needle))
}

// --- Criterion 1: GoReleaser Rust builder, native desktop Linux x86_64, checksums ---

#[test]
fn goreleaser_uses_the_rust_builder() {
    let cfg = load_yaml(".goreleaser.yaml");
    let builds = cfg
        .get("builds")
        .and_then(Value::as_sequence)
        .expect(".goreleaser.yaml must declare a `builds:` list");
    assert!(
        builds
            .iter()
            .any(|b| b.get("builder").and_then(Value::as_str) == Some("rust")),
        "at least one build must set `builder: rust` (the Zig-less path, ADR-0002)"
    );
}

#[test]
fn goreleaser_targets_only_native_desktop_linux_x86_64() {
    // Human choice B: the desktop leg is NATIVE x86_64-only. cargo-zigbuild's
    // `zig cc` linker cannot link the binary's system WebKitGTK/GTK/glib (it does
    // not search system lib paths), so arm64 desktop Linux is dropped and the
    // one x86_64 target is built with the native system linker. arm64 now lives
    // only on the mobile side (`docs/adr/0002` Update).
    let cfg = load_yaml(".goreleaser.yaml");
    let builds = cfg
        .get("builds")
        .and_then(Value::as_sequence)
        .expect("`builds:` list");
    // Gather every declared target triple across the rust builds.
    let mut targets: Vec<String> = Vec::new();
    for b in builds {
        if b.get("builder").and_then(Value::as_str) != Some("rust") {
            continue;
        }
        if let Some(t) = b.get("targets").and_then(Value::as_sequence) {
            for triple in t {
                if let Some(s) = triple.as_str() {
                    targets.push(s.to_string());
                }
            }
        }
    }
    assert!(
        targets.iter().any(|t| t == "x86_64-unknown-linux-gnu"),
        "must build the desktop Linux x86_64 target (x86_64-unknown-linux-gnu); got {targets:?}"
    );
    assert!(
        !targets.iter().any(|t| t == "aarch64-unknown-linux-gnu"),
        "the desktop leg must NOT build arm64 Linux (dropped — arm64 is mobile-only now); got {targets:?}"
    );
}

#[test]
fn goreleaser_desktop_build_is_native_no_zig() {
    // Criterion 6 / choice B: the desktop `builder: rust` build must use the
    // NATIVE cargo build (`command: build`), NOT `command: zigbuild`, and there
    // must be no cargo-zigbuild install hook anywhere in the config. The native
    // system linker links WebKitGTK; `zig cc` cannot.
    let cfg = load_yaml(".goreleaser.yaml");
    let builds = cfg
        .get("builds")
        .and_then(Value::as_sequence)
        .expect("`builds:` list");
    let rust_build = builds
        .iter()
        .find(|b| b.get("builder").and_then(Value::as_str) == Some("rust"))
        .expect("a `builder: rust` desktop build");
    let command = rust_build.get("command").and_then(Value::as_str);
    assert_ne!(
        command,
        Some("zigbuild"),
        "the desktop build must NOT use `command: zigbuild` (zig cc cannot link system WebKitGTK)"
    );
    // The native build is either the explicit `command: build` or the default
    // when `command` is absent — but the rust builder's DEFAULT is `zigbuild`, so
    // to be native the config MUST set `command: build` explicitly.
    assert_eq!(
        command,
        Some("build"),
        "the desktop build must set `command: build` (plain native cargo build); got {command:?}"
    );
    // No cargo-zigbuild install hook anywhere (before-hooks or otherwise).
    assert!(
        !contains_substr(&cfg, "cargo-zigbuild"),
        "the config must NOT install cargo-zigbuild (the desktop build is native, Zig-less)"
    );
    // No zig-based cross target left in the config either.
    assert!(
        !contains_substr(&cfg, "zigbuild"),
        "the config must reference no `zigbuild` command (the desktop build is native)"
    );
}

#[test]
fn goreleaser_selects_the_werust_package_in_the_workspace() {
    // werust is a multi-crate Cargo workspace, so GoReleaser's rust builder
    // errors "you need to specify which workspace to build, please add
    // '--package=[name]'" unless a package selector is passed through `flags`.
    // GoReleaser accepts either `-p=<name>` or `--package=<name>` (its
    // `isSettingPackage` check keys off those two prefixes), so the desktop
    // build MUST carry one selecting the `werust` binary crate.
    let cfg = load_yaml(".goreleaser.yaml");
    let builds = cfg
        .get("builds")
        .and_then(Value::as_sequence)
        .expect("`builds:` list");
    let rust_build = builds
        .iter()
        .find(|b| b.get("builder").and_then(Value::as_str) == Some("rust"))
        .expect("a `builder: rust` desktop build");
    let flags: Vec<String> = rust_build
        .get("flags")
        .and_then(Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        flags
            .iter()
            .any(|f| f == "--package=werust" || f == "-p=werust"),
        "the `builder: rust` build must pass a package selector (`--package=werust` or \
         `-p=werust`) so cargo builds the werust binary crate in the workspace; got flags {flags:?}"
    );
    // Keeping the release build a RELEASE build is load-bearing: setting an
    // explicit `flags` list REPLACES GoReleaser's `--release` default, and its
    // rust builder then copies the binary from `target/<triple>/release/`, so a
    // debug build would leave nothing to package. Preserve `--release`.
    assert!(
        flags.iter().any(|f| f == "--release"),
        "an explicit `flags` list drops GoReleaser's `--release` default; the release build \
         must keep `--release` (the builder copies from target/<triple>/release/); got flags {flags:?}"
    );
}

#[test]
fn goreleaser_emits_a_checksums_file() {
    let cfg = load_yaml(".goreleaser.yaml");
    let checksum = cfg
        .get("checksum")
        .expect(".goreleaser.yaml must declare a `checksum:` block so a tag ships checksums");
    // A non-empty name_template is the concrete evidence checksums are produced.
    assert!(
        checksum
            .get("name_template")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty()),
        "`checksum.name_template` must name the checksums artifact"
    );
}

// --- Criterion 2: conventional-commit changelog from git history, no changesets ---

#[test]
fn changelog_is_generated_from_conventional_commit_git_history() {
    let cfg = load_yaml(".goreleaser.yaml");
    let changelog = cfg
        .get("changelog")
        .expect(".goreleaser.yaml must declare a `changelog:` block");
    // Built FROM git history (not a maintained changeset file set).
    assert_eq!(
        changelog.get("use").and_then(Value::as_str),
        Some("git"),
        "`changelog.use` must be `git` (changelog comes from conventional-commit history)"
    );
    // Conventional-commit grouping is the concrete evidence the history is read
    // as conventional commits (feat/fix groups keyed off the subject prefix).
    let groups = changelog
        .get("groups")
        .and_then(Value::as_sequence)
        .expect("`changelog.groups` must classify conventional-commit types");
    let group_blob = {
        let mut s = Vec::new();
        for g in groups {
            collect_strings(g, &mut s);
        }
        s.join("\n")
    };
    assert!(
        group_blob.contains("feat") && group_blob.contains("fix"),
        "changelog groups must key off conventional-commit prefixes (feat/fix); got: {group_blob}"
    );

    // Guard the "no per-change changeset files" convention: this repo must not
    // grow a changesets dir (the wezig-parity rule; CONTEXT.md Conventions).
    assert!(
        !repo_root().join(".changeset").exists(),
        "there must be NO `.changeset/` dir: the changelog comes from git history, not changeset files"
    );
}

// --- Criteria 3/4/5: the three-job release workflow, dry-run, verify-before-tag ---

fn release_workflow() -> Value {
    load_yaml(".github/workflows/release.yml")
}

fn jobs() -> serde_yaml::Mapping {
    release_workflow()
        .get("jobs")
        .and_then(Value::as_mapping)
        .cloned()
        .expect("release.yml must declare `jobs:`")
}

fn job(name: &str) -> Value {
    jobs()
        .get(Value::String(name.to_string()))
        .cloned()
        .unwrap_or_else(|| panic!("release.yml must declare a `{name}` job"))
}

#[test]
fn release_triggers_on_a_version_tag_and_on_workflow_dispatch() {
    // `on:` parses to a mapping keyed by trigger. YAML's bare `on` is the boolean
    // true, so accept either the string key or the boolean key.
    let wf = release_workflow();
    let on = wf
        .get("on")
        .or_else(|| wf.get(Value::Bool(true)))
        .and_then(Value::as_mapping)
        .expect("release.yml must declare an `on:` trigger mapping");

    // A real release fires on a version tag (`v*`).
    let push = on
        .get(Value::String("push".into()))
        .expect("release must trigger on `push:` (the tag path)");
    assert!(
        contains_substr(push, "v*"),
        "the push trigger must match version tags (`v*`)"
    );

    // The dispatch dry-run path.
    assert!(
        on.contains_key(Value::String("workflow_dispatch".into())),
        "release must ALSO offer a `workflow_dispatch` dry-run"
    );
}

#[test]
fn verify_gate_runs_before_the_tag_build() {
    // Criterion 5: the same `verify` gate (cargo fmt/clippy/build/test) runs
    // before the desktop build, and the goreleaser job depends on it, so a tag
    // cannot ship a red tree.
    let jobs = jobs();
    let verify = jobs
        .get(Value::String("verify".into()))
        .expect("release.yml must run a `verify` job before building");
    let verify_blob = strings_of(verify).join("\n");
    for step in [
        "cargo fmt --check",
        "cargo clippy",
        "cargo build",
        "cargo test",
    ] {
        assert!(
            verify_blob.contains(step),
            "the release `verify` job must run `{step}` (parity with dorfl.json verify)"
        );
    }

    // The desktop build must be GATED on verify.
    let goreleaser = job("goreleaser");
    let needs = strings_of(
        goreleaser
            .get("needs")
            .expect("goreleaser must declare `needs:`"),
    );
    assert!(
        needs.iter().any(|n| n == "verify"),
        "the goreleaser (desktop) job must `needs: verify` so a tag can't ship a red tree; got {needs:?}"
    );
}

#[test]
fn release_workflow_has_no_zig_setup_step() {
    // Criterion 6 / choice B: with the native desktop build there is no Zig, so
    // the workflow must not set Zig up anywhere.
    let wf = release_workflow();
    assert!(
        !contains_substr(&wf, "setup-zig"),
        "the release workflow must have NO Zig setup step (setup-zig) — the desktop build is native"
    );
    // The goreleaser job must still install the WebKitGTK system deps (the native
    // linker links them).
    let goreleaser = job("goreleaser");
    assert!(
        contains_substr(&goreleaser, "libwebkitgtk-6.0-dev"),
        "the goreleaser job must still install the WebKitGTK system deps for the native build"
    );
}

#[test]
fn mobile_jobs_are_decoupled_from_the_desktop_leg() {
    // Criterion 3 / choice C: the Android APK + iOS Simulator `.app` legs are
    // DECOUPLED from the desktop leg — a desktop build failure must not block
    // them. They gate on `verify` (the green-tree gate) and must NOT `needs:
    // goreleaser`. On a tag they still attach to the Release, guaranteeing its
    // existence with an idempotent `gh release create` (a Release-EXISTENCE
    // guarantee, not a desktop-build dependency).
    for mobile in ["android-apk", "ios-simulator-app"] {
        let j = job(mobile);
        let needs = strings_of(
            j.get("needs")
                .unwrap_or_else(|| panic!("the `{mobile}` job must declare `needs:`")),
        );
        assert!(
            !needs.iter().any(|n| n == "goreleaser"),
            "the `{mobile}` job must NOT `needs: goreleaser` (decoupled from desktop); got {needs:?}"
        );
        assert!(
            needs.iter().any(|n| n == "verify"),
            "the `{mobile}` job must `needs: verify` (gated on a green tree, independent of desktop); got {needs:?}"
        );
        // On a tag, guarantee the Release exists before uploading into it — an
        // idempotent create, since this leg no longer waits on the desktop job.
        assert!(
            contains_substr(&j, "gh release create"),
            "the `{mobile}` job must idempotently `gh release create` on a tag (Release-existence \
             guarantee) since it no longer `needs: goreleaser`"
        );
    }
}

#[test]
fn android_job_builds_the_apk_and_runs_the_abi_check() {
    // Criterion 3: the Android leg builds the debug APK from the REAL app module
    // and runs the acceptance ABI check on it.
    let j = job("android-apk");
    assert!(
        j.get("runs-on").and_then(Value::as_str) == Some("ubuntu-latest"),
        "the Android leg runs on Linux (ubuntu-latest)"
    );
    assert!(
        contains_substr(&j, ":app:assembleDebug"),
        "the Android leg must build the debug APK via `./gradlew :app:assembleDebug`"
    );
    assert!(
        contains_substr(&j, "check-apk-abis.sh"),
        "the Android leg must RUN the acceptance ABI check (check-apk-abis.sh) on the built APK"
    );
    assert!(
        contains_substr(&j, "app-debug.apk"),
        "the Android leg must reference the built APK artifact (app-debug.apk)"
    );
}

#[test]
fn ios_job_builds_the_simulator_app_on_macos_and_runs_the_bundle_check() {
    // Criterion 3: the iOS leg builds the Simulator `.app` on macOS and runs the
    // acceptance bundle check on it, then attaches a zip.
    let j = job("ios-simulator-app");
    assert!(
        j.get("runs-on").and_then(Value::as_str) == Some("macos-14"),
        "the iOS leg MUST run on macos-14 (Xcode/Simulator are macOS-only)"
    );
    assert!(
        contains_substr(&j, "aarch64-apple-ios-sim"),
        "the iOS leg must add the Simulator Rust target (aarch64-apple-ios-sim)"
    );
    assert!(
        contains_substr(&j, "build-and-run.sh"),
        "the iOS leg must build the `.app` via build-and-run.sh (BUILD_ONLY packaging path)"
    );
    assert!(
        contains_substr(&j, "BUILD_ONLY"),
        "the iOS leg must use the BUILD_ONLY path (build the `.app` without booting a simulator)"
    );
    assert!(
        contains_substr(&j, "check-app-bundle.sh"),
        "the iOS leg must RUN the acceptance bundle check (check-app-bundle.sh)"
    );
    assert!(
        contains_substr(&j, "WerustShell.app"),
        "the iOS leg must reference the built Simulator `.app` (WerustShell.app)"
    );
}

#[test]
fn dry_run_snapshots_and_uploads_artifacts_without_publishing() {
    // Criterion 4: on `workflow_dispatch`, goreleaser runs `--snapshot`
    // (publishes nothing) and every leg uploads workflow artifacts instead of
    // attaching to a Release. On a tag, goreleaser publishes and the mobile legs
    // `gh release upload`.
    let goreleaser = job("goreleaser");
    let gr_blob = strings_of(&goreleaser).join("\n");
    assert!(
        gr_blob.contains("--snapshot"),
        "the dispatch dry-run must run goreleaser with `--snapshot` (publishes nothing)"
    );
    // The real tag path publishes (goreleaser `release`, i.e. NOT --snapshot).
    // Evidence: the job distinguishes the tag path from the dispatch path.
    assert!(
        gr_blob.contains("workflow_dispatch") || gr_blob.contains("startsWith(github.ref"),
        "the goreleaser job must branch on tag-vs-dispatch (real release vs snapshot dry-run)"
    );

    // Every leg uploads workflow artifacts on the dry-run.
    for leg in ["goreleaser", "android-apk", "ios-simulator-app"] {
        let j = job(leg);
        assert!(
            contains_substr(&j, "actions/upload-artifact"),
            "the `{leg}` leg must upload workflow artifacts on the dispatch dry-run (actions/upload-artifact)"
        );
    }

    // The mobile legs attach to the Release with `gh release upload` on a tag.
    for leg in ["android-apk", "ios-simulator-app"] {
        let j = job(leg);
        assert!(
            contains_substr(&j, "gh release upload"),
            "the `{leg}` leg must attach its artifact to the Release with `gh release upload` on a tag"
        );
    }
}
