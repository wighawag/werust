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
//! 7. Every leg that COMPILES Rust injects `WERUST_VERSION` from the tag, so the
//!    shipped binary/APK/`.app` reports the released version instead of the
//!    un-injected placeholder (task
//!    `general-browser-menu-with-version-and-debug-entry`; the resolution itself
//!    is `crates/werust-core/build.rs`).
//! 8. The Android leg SIGNS a release APK when the release-keystore secrets are
//!    configured (`app-release.apk` attached alongside the debug APK), and is a
//!    graceful NO-OP when they are not — the debug APK is then attached under the
//!    honest name `app-debug-unsigned.apk`. The signing key material is supplied
//!    ONLY through the environment, so the Gradle `signingConfigs.release` block
//!    is inert for local dev builds (task `android-apk-signing`).
//! 9. The macOS DESKTOP leg (task `macos-release-packaging-leg`,
//!    `docs/adr/0011` macOS split sub-task 4) builds BOTH darwin slices,
//!    `lipo`s them into ONE universal binary, bundles a minimal-`Info.plist`
//!    `Werust.app` and attaches it, as a SIBLING job on the same `macos-14`
//!    runner the iOS leg uses: decoupled from every other leg, deliberately
//!    UNSIGNED, with `CFBundleVersion` read from the ONE Rust version source.
//!
//! The whole test is NETWORK-ISOLATED: it only parses files in this repo (it
//! never runs Gradle, never reads a secret's value, and performs no I/O beyond
//! `std::fs::read_to_string`), so it passes identically on a fork with no secrets
//! configured and on the real repo with all four.

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

/// A repo-relative text file, read whole. Used where the contract lives in a
/// file no pure-Rust parser can model (a shell script, a Kotlin DSL, the
/// README): asserting on its TEXT is enough for the shape that matters.
fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn load_yaml(rel: &str) -> Value {
    let path = repo_root().join(rel);
    let text = read_repo_file(rel);
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

// --- Criterion 7: the released version reaches the compiled Rust ---

#[test]
fn every_rust_compiling_leg_injects_the_tag_version_into_the_build() {
    // The version werust SHOWS (the desktop startup banner + the ⋮ browser menu
    // on all three platforms) is `werust_core::version()`, which its `build.rs`
    // resolves from `WERUST_VERSION` when CI injects it. GoReleaser derives only
    // the ARCHIVE NAME from the tag, never the compiled code, so WITHOUT this
    // injection a tagged `v0.2.6` release shipped every menu reading
    // `werust 0.0.0`. Each leg that compiles Rust must therefore export
    // `WERUST_VERSION` derived from the tag ref name.
    //
    // All four legs qualify: the desktop-Linux leg `cargo build`s the binary,
    // both mobile legs cross-compile the shared core into their artifact, and the
    // macOS desktop leg compiles it into both slices of the universal binary
    // `Werust.app` carries (where the SAME resolved string is ALSO stamped into
    // `CFBundleVersion`; see `macos_bundle_version_comes_from_the_one_rust_version_source`).
    //
    // Asserted on the job's `env:` MAPPING (not a substring sweep) so the
    // variable really is exported to every step of the leg, which is what makes
    // the cargo invocation — wherever it lives, GoReleaser's or Gradle's — see it.
    for leg in ["goreleaser", "android-apk", "ios-simulator-app", MACOS_LEG] {
        let j = job(leg);
        let env = j.get("env").and_then(Value::as_mapping).unwrap_or_else(|| {
            panic!(
                "the `{leg}` leg compiles Rust, so it must declare an `env:` block injecting \
                 WERUST_VERSION, or its artifact reports the un-injected placeholder version"
            )
        });
        let injected = env
            .get(Value::String("WERUST_VERSION".into()))
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!("the `{leg}` leg's `env:` must set WERUST_VERSION (a string expression)")
            });
        // Derived from the TAG, not hardcoded: the ref name is the only thing
        // that tracks the release being cut. (`build.rs` strips the leading `v`,
        // so passing the ref name through verbatim is correct.)
        assert!(
            injected.contains("github.ref_name"),
            "the `{leg}` leg must derive WERUST_VERSION from the tag ref name, not hardcode it; \
             got {injected:?}"
        );
        // Only on a TAG: on the dispatch dry-run there is no release version, so
        // the variable must be empty and `build.rs` falls through to
        // `git describe` rather than labelling a snapshot with a stale tag.
        assert!(
            injected.contains("refs/tags/"),
            "the `{leg}` leg must inject the version only on a TAG (empty on the dispatch \
             dry-run, so build.rs falls through to `git describe`); got {injected:?}"
        );
        // The `git describe` fallback needs the tags, which a default shallow
        // checkout does not fetch — without this the dry-run artifacts would
        // silently report the last-resort Cargo version.
        assert_eq!(
            checkout_fetch_depth(&j),
            Some(0),
            "the `{leg}` leg must check out with `fetch-depth: 0` so build.rs's `git describe` \
             fallback can see the tags on the non-tag path"
        );
    }
}

#[test]
fn every_rust_compiling_leg_passes_the_rpc_endpoint_secret_through() {
    // Task `configurable-rpc-endpoint-via-env`: each leg that compiles Rust also
    // exports `WERUST_RPC_URL` from the OPTIONAL repository secret of the same
    // name, so a release pipeline CAN supply a private ENS-resolution endpoint
    // without its URL ever entering the repo. BOTH secret states are covered:
    //
    //   * WITH the secret configured, Actions substitutes its value into the
    //     build env.
    //   * WITHOUT it (the secret is optional — a fork, or simply unconfigured),
    //     the expression substitutes the EMPTY string and `rpc_endpoint()` in
    //     `ethereum.rs` falls back to the public `DEFAULT_RPC_ENDPOINT`, whose
    //     empty-falls-back rule exists precisely for this case.
    //
    // This test parses the workflow FILE (never the secret's value), so it
    // passes identically on either path. What it pins: the injection
    // EXPRESSION references the secret, and no leg hardcodes a literal
    // endpoint URL (a private RPC URL must never be committed).
    for leg in ["goreleaser", "android-apk", "ios-simulator-app", MACOS_LEG] {
        let j = job(leg);
        let env = j.get("env").and_then(Value::as_mapping).unwrap_or_else(|| {
            panic!(
                "the `{leg}` leg compiles Rust, so it must declare an `env:` block (WERUST_VERSION \
                 and WERUST_RPC_URL live there)"
            )
        });
        let injected = env
            .get(Value::String("WERUST_RPC_URL".into()))
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "the `{leg}` leg's `env:` must pass WERUST_RPC_URL through from the optional \
                     repository secret (empty when unconfigured -> public default)"
                )
            });
        assert!(
            injected.contains("secrets.WERUST_RPC_URL"),
            "the `{leg}` leg must source WERUST_RPC_URL from `secrets.WERUST_RPC_URL` (the SAME \
             secret-name pattern as WERUST_VERSION); got {injected:?}"
        );
        assert!(
            !injected.contains("http"),
            "the `{leg}` leg must NOT hardcode a literal RPC URL (private endpoints are never \
             committed; the default lives in `DEFAULT_RPC_ENDPOINT`); got {injected:?}"
        );
    }
}

/// The `fetch-depth` the job's `actions/checkout` step requests, or [`None`] when
/// the job has no checkout step or sets no depth (cargo's default shallow
/// checkout, which carries no tags).
fn checkout_fetch_depth(job: &Value) -> Option<u64> {
    job.get("steps")
        .and_then(Value::as_sequence)?
        .iter()
        .find(|s| {
            s.get("uses")
                .and_then(Value::as_str)
                .is_some_and(|u| u.starts_with("actions/checkout"))
        })?
        .get("with")?
        .get("fetch-depth")?
        .as_u64()
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
    for leg in ["goreleaser", "android-apk", "ios-simulator-app", MACOS_LEG] {
        let j = job(leg);
        assert!(
            contains_substr(&j, "actions/upload-artifact"),
            "the `{leg}` leg must upload workflow artifacts on the dispatch dry-run (actions/upload-artifact)"
        );
    }

    // Every non-GoReleaser leg attaches its artifact to the Release with
    // `gh release upload` on a tag (GoReleaser publishes its own).
    for leg in ["android-apk", "ios-simulator-app", MACOS_LEG] {
        let j = job(leg);
        assert!(
            contains_substr(&j, "gh release upload"),
            "the `{leg}` leg must attach its artifact to the Release with `gh release upload` on a tag"
        );
    }
}

// --- Criterion 8: the Android leg signs a release APK (task `android-apk-signing`) ---
//
// Two paths, BOTH pinned here by PARSING the workflow + the app module's Gradle
// script (never by reading a secret's value, which a test cannot see anyway):
//
//   * SECRETS CONFIGURED — the release keystore is decoded from
//     `ANDROID_KEYSTORE_B64` into a runner-temp file, `ANDROID_KEYSTORE_PATH`
//     points Gradle at it, and `:app:assembleRelease` produces a SIGNED (and, as
//     AGP always does once a `signingConfig` applies, zipaligned)
//     `app-release.apk`, attached to the Release alongside the debug APK.
//   * SECRETS ABSENT (forks, dry-runs) — every signing step is skipped and the
//     debug APK is renamed `app-debug-unsigned.apk`, so no attached artifact
//     claims a release signature it does not carry.
//
// Both paths therefore pass identically on a fork with no secrets configured.

/// The CI-local PRESENCE FLAG every signing step gates on.
///
/// GitHub Actions does NOT expose the `secrets` context to a step's `if:` (the
/// allowed contexts there are github/needs/strategy/matrix/job/runner/env/vars/
/// steps/inputs), so `if: ${{ secrets.X != '' }}` cannot work. The job-level
/// `env:` CAN read `secrets`, so the leg mirrors mere PRESENCE of the keystore
/// secret into this flag and the steps gate on `env.<flag>`.
const SIGNING_FLAG: &str = "ANDROID_SIGNING_CONFIGURED";

/// The secret carrying the base64 release keystore (the one whose PRESENCE
/// decides whether the leg signs at all).
const KEYSTORE_SECRET: &str = "ANDROID_KEYSTORE_B64";

fn job_steps(job: &Value) -> Vec<Value> {
    job.get("steps")
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default()
}

/// A step's `if:` condition, or `""` when it is unconditional.
fn step_if(step: &Value) -> &str {
    step.get("if").and_then(Value::as_str).unwrap_or("")
}

/// Every step of `job` whose scalars mention `needle` — lets an assertion talk
/// about "the step that does X" without pinning the step order.
fn steps_mentioning(job: &Value, needle: &str) -> Vec<Value> {
    job_steps(job)
        .into_iter()
        .filter(|s| contains_substr(s, needle))
        .collect()
}

fn the_one_step_mentioning(job: &Value, needle: &str) -> Value {
    let found = steps_mentioning(job, needle);
    assert_eq!(
        found.len(),
        1,
        "expected EXACTLY one step of the android-apk leg to mention {needle:?}; found {}",
        found.len()
    );
    found.into_iter().next().unwrap()
}

#[test]
fn android_leg_gates_signing_on_an_env_presence_flag_not_the_secrets_context() {
    let j = job("android-apk");
    let env = j
        .get("env")
        .and_then(Value::as_mapping)
        .expect("the android-apk leg must declare an `env:` block");
    let flag = env
        .get(Value::String(SIGNING_FLAG.into()))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "the android-apk leg's `env:` must derive `{SIGNING_FLAG}` from the PRESENCE of \
                 `secrets.{KEYSTORE_SECRET}`, because a step `if:` cannot read the secrets context"
            )
        });
    assert!(
        flag.contains(&format!("secrets.{KEYSTORE_SECRET}")),
        "`{SIGNING_FLAG}` must be derived from `secrets.{KEYSTORE_SECRET}`; got {flag:?}"
    );
    assert!(
        flag.contains("!= ''"),
        "`{SIGNING_FLAG}` must test the secret for PRESENCE (`!= ''`) so an unconfigured repo \
         gracefully skips signing; got {flag:?}"
    );

    // The flag carries PRESENCE, never the key material: the base64 keystore and
    // the two passwords must NOT sit in the job-wide `env:`, which every step of
    // the leg (including the cargo/Gradle builds) would inherit. They belong to
    // the single step that needs them.
    for material in [
        KEYSTORE_SECRET,
        "ANDROID_KEYSTORE_PASSWORD",
        "ANDROID_KEY_PASSWORD",
    ] {
        assert!(
            !env.contains_key(Value::String(material.into())),
            "the android-apk leg must NOT put `{material}` in the job-wide `env:` (key material \
             belongs to the one step that uses it; the job env carries only the presence flag)"
        );
    }

    // No step may gate on `secrets` directly — that expression is not available
    // in a step `if:` and would silently evaluate to the empty string.
    for step in job_steps(&j) {
        let cond = step_if(&step);
        assert!(
            !cond.contains("secrets."),
            "a step `if:` cannot read the `secrets` context (it is unavailable there) — gate on \
             `env.{SIGNING_FLAG}` instead; got {cond:?}"
        );
    }
}

#[test]
fn android_leg_builds_and_attaches_a_signed_release_apk_when_the_keystore_secret_is_configured() {
    let j = job("android-apk");

    // The keystore is materialised from the secret into a file and handed to
    // Gradle through `ANDROID_KEYSTORE_PATH`, exported via GITHUB_ENV so the
    // later Gradle step (a separate shell) sees it.
    let decode = the_one_step_mentioning(&j, "ANDROID_KEYSTORE_PATH=");
    assert!(
        step_if(&decode).contains(SIGNING_FLAG),
        "the keystore-decode step must be gated on `env.{SIGNING_FLAG}`; got {:?}",
        step_if(&decode)
    );
    assert!(
        contains_substr(&decode, "GITHUB_ENV"),
        "the decode step must export ANDROID_KEYSTORE_PATH via GITHUB_ENV so the Gradle step sees it"
    );
    assert!(
        contains_substr(&decode, "base64 -d"),
        "the decode step must base64-decode the keystore secret into the keystore file"
    );
    assert!(
        contains_substr(&decode, KEYSTORE_SECRET),
        "the decode step must read the keystore from `secrets.{KEYSTORE_SECRET}`"
    );

    // The signed build itself: `assembleRelease`, gated, with the alias +
    // passwords supplied from the secrets IN THAT STEP's env.
    let sign = the_one_step_mentioning(&j, ":app:assembleRelease");
    assert!(
        step_if(&sign).contains(SIGNING_FLAG),
        "the release-APK build must be gated on `env.{SIGNING_FLAG}` (a graceful no-op without \
         the secrets); got {:?}",
        step_if(&sign)
    );
    let sign_env = sign
        .get("env")
        .and_then(Value::as_mapping)
        .expect("the signing step must declare its own `env:` with the keystore credentials");
    for secret in [
        "ANDROID_KEYSTORE_PASSWORD",
        "ANDROID_KEY_ALIAS",
        "ANDROID_KEY_PASSWORD",
    ] {
        let v = sign_env
            .get(Value::String(secret.into()))
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("the signing step's `env:` must pass `{secret}` to Gradle"));
        assert!(
            v.contains(&format!("secrets.{secret}")),
            "`{secret}` must come from the repository secret of the same name, never a literal; \
             got {v:?}"
        );
    }

    // The artifact users actually install gets the SAME ABI guarantee as the
    // debug APK (criterion 4 of the mobile-android task).
    assert!(
        steps_mentioning(&j, "app-release.apk")
            .iter()
            .any(|s| contains_substr(s, "check-apk-abis.sh")),
        "the signed release APK must also be run through check-apk-abis.sh (it is the artifact \
         users install)"
    );

    // Attached to the Release on a tag, uploaded as a workflow artifact on the
    // dry-run — alongside the debug APK, distinguishable by name.
    let attach = the_one_step_mentioning(&j, "gh release upload");
    for apk in ["app-release.apk", "app-debug.apk"] {
        assert!(
            contains_substr(&attach, apk),
            "the tag attach step must upload `{apk}` (the signed release APK alongside the debug APK)"
        );
    }
    let upload = the_one_step_mentioning(&j, "actions/upload-artifact");
    assert!(
        contains_substr(&upload, "app-release.apk"),
        "the dispatch dry-run must also surface `app-release.apk` as a workflow artifact"
    );
}

#[test]
fn android_leg_names_the_debug_apk_unsigned_when_the_signing_secrets_are_absent() {
    let j = job("android-apk");

    // The rename is the NO-SECRETS path: gated on the flag NOT being set.
    let renames: Vec<Value> = steps_mentioning(&j, "app-debug-unsigned.apk")
        .into_iter()
        .filter(|s| {
            let cond = step_if(s);
            cond.contains(SIGNING_FLAG) && cond.contains("!=")
        })
        .collect();
    assert_eq!(
        renames.len(),
        1,
        "exactly one step must rename the debug APK to `app-debug-unsigned.apk`, gated on \
         `env.{SIGNING_FLAG}` NOT being set (forks, unconfigured repos)"
    );

    // The honestly-named artifact must actually REACH both destinations,
    // otherwise the no-secrets path would attach nothing at all.
    for dest in ["gh release upload", "actions/upload-artifact"] {
        let step = the_one_step_mentioning(&j, dest);
        assert!(
            contains_substr(&step, "app-debug-unsigned.apk"),
            "the `{dest}` step must also handle `app-debug-unsigned.apk` (the no-secrets path's \
             only artifact)"
        );
    }
}

#[test]
fn android_app_gradle_declares_an_env_gated_release_signing_config() {
    // The Gradle side of the seam. Asserted as TEXT (there is no Kotlin-DSL
    // parser available in the pure-Rust gate), which is enough for the contract
    // that matters: the release signing config exists, takes every input from the
    // ENVIRONMENT, and is created only when that environment is present — so a
    // local `./gradlew :app:assembleDebug` (and even `assembleRelease`) is
    // completely unaffected, and no key material or keystore path is committed.
    let path = repo_root().join("crates/werust-android/app/build.gradle.kts");
    let gradle =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    assert!(
        gradle.contains("signingConfigs"),
        "the app module must declare a `signingConfigs` block"
    );
    assert!(
        gradle.contains("create(\"release\")"),
        "the app module must create the `release` signing config"
    );
    for env_var in [
        "ANDROID_KEYSTORE_PATH",
        "ANDROID_KEYSTORE_PASSWORD",
        "ANDROID_KEY_ALIAS",
        "ANDROID_KEY_PASSWORD",
    ] {
        assert!(
            gradle.contains(&format!("System.getenv(\"{env_var}\")")),
            "the release signing config must read `{env_var}` from the environment"
        );
    }
    // Gated on env PRESENCE: no `ANDROID_KEYSTORE_PATH` -> no release signing
    // config at all, so local dev builds behave exactly as before.
    assert!(
        gradle.contains("!= null"),
        "the release signing config must be gated behind the env var being present (`!= null`), \
         so it does not affect local dev builds"
    );
    // A keystore or password must never be committed.
    assert!(
        !gradle.contains(".jks"),
        "no keystore path may be committed in the Gradle script (the keystore comes from CI, \
         base64 in a secret)"
    );
    assert!(
        !gradle.contains("storePassword = \""),
        "the store password must never be a committed literal"
    );
    assert!(
        gradle.contains("storePassword = System.getenv("),
        "the store password must come from the environment"
    );
}

#[test]
fn the_keystore_handling_steps_touch_no_network() {
    // Criterion "network-isolated": the keystore never travels anywhere — it is
    // decoded LOCALLY from the secret and its signature verified LOCALLY with the
    // SDK's apksigner, so no step in the signing path fetches or posts anything.
    // (This test itself is network-free too: it only parses repo files.)
    let j = job("android-apk");
    for needle in ["ANDROID_KEYSTORE_PATH=", "apksigner"] {
        for step in steps_mentioning(&j, needle) {
            let blob = strings_of(&step).join("\n");
            for net in ["curl", "wget", "http://", "https://"] {
                assert!(
                    !blob.contains(net),
                    "the signing step mentioning {needle:?} must touch NO network; found {net:?} \
                     in:\n{blob}"
                );
            }
        }
    }
}

// --- Criterion 9: the macOS desktop packaging leg (task `macos-release-packaging-leg`) ---
//
// The FOURTH release leg (`docs/adr/0011-webview2-for-windows.md`'s macOS split,
// sub-task 4): the AppKit shell `macos-appkit-window-and-chrome` shipped is a
// binary nothing ever handed to a person. This leg builds it for BOTH darwin
// architectures, `lipo`s the two slices into ONE universal binary, wraps it in a
// minimal-`Info.plist` `Werust.app` and attaches the zip to the tagged Release
// beside the desktop Linux binary, the Android APK and the iOS Simulator `.app`.
//
// It is a SIBLING of `ios-simulator-app`, not an extension of it: both run on the
// same `macos-14` runner, but an iOS failure must never withhold the desktop
// artifact (and vice versa), the same decoupling rule the mobile legs already
// carry. Everything else is modelled on the mobile legs: `needs: verify`, an
// idempotent `gh release create` before the upload, and a dry-run that uploads a
// workflow artifact instead of publishing.
//
// Pinned HERE, in the pure-Rust gate, exactly like the rest of this file: the
// assertions parse the workflow YAML and read the two packaging scripts as text,
// so they need no macOS, no Xcode and no network.
// Decisions: docs/spikes/macos-release-packaging-leg/README.md.

/// The macOS desktop leg's job key in `release.yml`.
const MACOS_LEG: &str = "macos-desktop-app";

/// The script that builds both slices, `lipo`s them and assembles `Werust.app`.
/// It lives in the CRATE (like `crates/werust-ios/build-and-run.sh`) because it
/// is the product's packaging step, runnable by a human on a Mac, not a CI-only
/// snippet.
const MACOS_BUNDLE_SCRIPT: &str = "crates/werust-macos/bundle-app.sh";

/// The BUILD-leg acceptance check the job runs on the assembled bundle: the
/// macOS twin of `check-apk-abis.sh` / `check-app-bundle.sh`, and the place
/// criterion 2's "both architectures verified with `lipo -info`" actually
/// EXECUTES (it cannot run in this Linux gate).
const MACOS_BUNDLE_CHECK: &str =
    "docs/spikes/macos-release-packaging-leg/check-macos-app-bundle.sh";

/// How a packaging SCRIPT reads werust's ONE version: the `werust-core` example
/// that prints `werust_core::version()`, the accessor `build.rs` resolves from
/// the release tag. Re-deriving the version in shell would mint the second
/// source `version()`'s own docs forbid.
const VERSION_EXAMPLE: &str = "--example print_version";

/// The macOS bundle's user-facing name. One `.app`, one name, everywhere.
const MACOS_APP_BUNDLE: &str = "Werust.app";

/// A shell script's CODE, with whole-line `#` comments (and the shebang)
/// dropped.
///
/// The absence assertions below ("no second version source", "no signing tool")
/// have to distinguish RUNNING a command from EXPLAINING why the script does
/// not: the packaging script's header names `git describe` and `codesign`
/// precisely to record that it deliberately does neither, and that prose must
/// not read as a violation. Whole-line comments are the only comment form this
/// strips, which is all these scripts use for prose.
fn shell_code_of(script: &str) -> String {
    script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn macos_desktop_leg_is_a_decoupled_sibling_on_the_shared_macos_runner() {
    // Criterion 4: the existing `macos-14` runner shape (the one the iOS leg
    // already uses, so no new runner class), and decoupled in BOTH directions.
    let j = job(MACOS_LEG);
    assert_eq!(
        j.get("runs-on").and_then(Value::as_str),
        Some("macos-14"),
        "the macOS desktop leg MUST run on the existing `macos-14` runner shape (Xcode/`lipo`/\
         the darwin SDKs are macOS-only)"
    );
    let needs = strings_of(
        j.get("needs")
            .unwrap_or_else(|| panic!("the `{MACOS_LEG}` job must declare `needs:`")),
    );
    assert!(
        needs.iter().any(|n| n == "verify"),
        "the `{MACOS_LEG}` leg must `needs: verify` (gated on a green tree, like every leg); got {needs:?}"
    );
    for sibling in ["goreleaser", "ios-simulator-app", "android-apk"] {
        assert!(
            !needs.iter().any(|n| n == sibling),
            "the `{MACOS_LEG}` leg must NOT `needs: {sibling}`: it is a SIBLING leg, so no other \
             platform's failure may withhold the macOS artifact; got {needs:?}"
        );
    }
    // Being decoupled means the Release may not exist yet on a tag, so this leg
    // guarantees its EXISTENCE the same idempotent way the mobile legs do.
    assert!(
        contains_substr(&j, "gh release create"),
        "the `{MACOS_LEG}` leg must idempotently `gh release create` on a tag (a Release-EXISTENCE \
         guarantee, since it waits on no other leg)"
    );
}

#[test]
fn macos_desktop_leg_builds_both_slices_and_lipos_them_into_one_universal_binary() {
    // Criteria 1 + 2: both darwin targets, ONE universal binary, and the
    // universality actually VERIFIED on the runner rather than assumed.
    let j = job(MACOS_LEG);
    assert!(
        contains_substr(&j, MACOS_BUNDLE_SCRIPT),
        "the `{MACOS_LEG}` leg must build + bundle via `{MACOS_BUNDLE_SCRIPT}` (the same \
         script a human can run on a Mac), not an inline CI-only snippet"
    );
    assert!(
        contains_substr(&j, MACOS_BUNDLE_CHECK),
        "the `{MACOS_LEG}` leg must RUN the BUILD-leg check `{MACOS_BUNDLE_CHECK}` on the \
         assembled bundle (the macOS twin of check-apk-abis.sh / check-app-bundle.sh)"
    );

    let script = read_repo_file(MACOS_BUNDLE_SCRIPT);
    for triple in ["x86_64-apple-darwin", "aarch64-apple-darwin"] {
        assert!(
            script.contains(triple),
            "{MACOS_BUNDLE_SCRIPT} must build the `{triple}` slice"
        );
    }
    assert!(
        script.contains("lipo -create"),
        "{MACOS_BUNDLE_SCRIPT} must `lipo -create` the two slices into ONE universal binary"
    );

    let check = read_repo_file(MACOS_BUNDLE_CHECK);
    for arch in ["x86_64", "arm64"] {
        assert!(
            check.contains(arch),
            "{MACOS_BUNDLE_CHECK} must assert the `{arch}` slice is present (criterion 2: BOTH \
             architectures verified with `lipo`)"
        );
    }
    assert!(
        check.contains("lipo"),
        "{MACOS_BUNDLE_CHECK} must read the architectures with `lipo` (the tool that can see them)"
    );
}

#[test]
fn macos_desktop_leg_bundles_a_minimal_info_plist_and_attaches_the_zip() {
    // Criterion 1: a real `.app` bundle, with the minimal key set the task names,
    // zipped and attached beside the existing artifacts.
    let script = read_repo_file(MACOS_BUNDLE_SCRIPT);
    assert!(
        script.contains(MACOS_APP_BUNDLE),
        "{MACOS_BUNDLE_SCRIPT} must assemble `{MACOS_APP_BUNDLE}`"
    );
    assert!(
        script.contains("Contents/MacOS"),
        "{MACOS_BUNDLE_SCRIPT} must put the binary at the bundle's `Contents/MacOS/` location \
         (a `.app` is a LAYOUT, not a renamed directory)"
    );
    for key in [
        "CFBundleName",
        "CFBundleIdentifier",
        "CFBundleVersion",
        "CFBundlePackageType",
        // Not in the task's list but load-bearing: without it the bundle names no
        // binary to launch, so the `.app` opens nothing at all.
        "CFBundleExecutable",
    ] {
        assert!(
            script.contains(key),
            "{MACOS_BUNDLE_SCRIPT}'s Info.plist must declare `{key}`"
        );
    }
    assert!(
        script.contains("<string>APPL</string>"),
        "{MACOS_BUNDLE_SCRIPT}'s Info.plist must set CFBundlePackageType to `APPL`"
    );

    // The artifact reaches BOTH destinations: attached on a tag, uploaded on the
    // dry-run (the shared assertions live in
    // `dry_run_snapshots_and_uploads_artifacts_without_publishing`; here we pin
    // that it is the ZIPPED bundle that travels, not the bundle DIRECTORY, which
    // no release asset can be).
    let j = job(MACOS_LEG);
    let zip_steps = steps_mentioning(&j, ".zip");
    assert!(
        !zip_steps.is_empty(),
        "the `{MACOS_LEG}` leg must attach/upload a ZIP of the `.app` (a bundle directory is not \
         a release asset)"
    );
}

#[test]
fn macos_bundle_version_comes_from_the_one_rust_version_source() {
    // Criterion 3, and the reason the Android sibling task
    // `android-apk-version-from-the-release-tag` exists: a packaging step that
    // re-derives the version in shell IS a second source, and it drifts from the
    // version the shipped binary reports the moment either side changes. So
    // `CFBundleVersion` is READ OUT of the compiled core (the `print_version`
    // example prints `werust_core::version()`, which `build.rs` resolved from
    // `WERUST_VERSION` / `git describe`) instead of re-computed.
    let script = shell_code_of(&read_repo_file(MACOS_BUNDLE_SCRIPT));
    assert!(
        script.contains(VERSION_EXAMPLE),
        "{MACOS_BUNDLE_SCRIPT} must read the version from the ONE source by running the \
         `werust-core` `{VERSION_EXAMPLE}` example"
    );
    for second_source in ["git describe", "CARGO_PKG_VERSION", "github.ref_name"] {
        assert!(
            !script.contains(second_source),
            "{MACOS_BUNDLE_SCRIPT} must NOT re-derive the version from `{second_source}`: that is \
             the SECOND version source `werust_core::version()`'s docs forbid"
        );
    }

    let example = read_repo_file("crates/werust-core/examples/print_version.rs");
    assert!(
        example.contains("werust_core::version()"),
        "the `print_version` example must print `werust_core::version()` itself (it exists ONLY to \
         make that accessor readable by a packaging script)"
    );

    // The leg must not smuggle a version in either: its only version input is the
    // job-level `WERUST_VERSION` the shared loop above already pins.
    let j = job(MACOS_LEG);
    assert!(
        !contains_substr(&j, "git describe"),
        "the `{MACOS_LEG}` leg must not re-derive a version in the workflow (it inherits \
         WERUST_VERSION and the bundling script reads the resolved value back out)"
    );
}

#[test]
fn macos_desktop_leg_is_deliberately_unsigned() {
    // Criterion 6 / the task's "unsigned, deliberately": no signing and no
    // notarization anywhere in this leg (both need an Apple Developer account and
    // are a separate follow-on, the macOS analogue of `android-apk-signing`).
    // Pinned as an ABSENCE so a later "just add codesign here" edit has to come
    // with the secrets-presence-flag pattern the Android leg established, rather
    // than half a signing path.
    let j = job(MACOS_LEG);
    let script = shell_code_of(&read_repo_file(MACOS_BUNDLE_SCRIPT));
    for tool in ["codesign", "notarytool", "altool", "stapler"] {
        assert!(
            !contains_substr(&j, tool),
            "the `{MACOS_LEG}` leg must contain NO `{tool}` step (unsigned by design; signing + \
             notarization are a follow-on that must copy the Android secrets-presence-flag pattern)"
        );
        assert!(
            !script.contains(tool),
            "{MACOS_BUNDLE_SCRIPT} must contain no `{tool}` step (unsigned by design)"
        );
    }
    // Honest naming, the Android precedent (`app-debug-unsigned.apk`): the
    // attached asset must SAY it is unsigned, so nothing on the Release page
    // claims a signature it does not carry.
    assert!(
        contains_substr(&j, "unsigned"),
        "the `{MACOS_LEG}` leg's artifact must be NAMED unsigned (the Android honest-naming \
         precedent), so the Release page never implies a signature it does not carry"
    );
}

#[test]
fn readme_states_the_macos_artifact_is_unsigned_and_how_to_open_it() {
    // Criterion 6: an unsigned `.app` does NOT open by double-click on a machine
    // that has never seen it, so shipping one without saying how to open it ships
    // a broken download. The README is where a person looks before downloading.
    let readme = read_repo_file("README.md");
    assert!(
        readme.contains(MACOS_APP_BUNDLE),
        "the README must name the macOS release artifact (`{MACOS_APP_BUNDLE}`)"
    );
    assert!(
        readme.contains("unsigned"),
        "the README must state plainly that the macOS artifact is UNSIGNED"
    );
    assert!(
        readme.contains("xattr -d com.apple.quarantine"),
        "the README must give the quarantine-clearing command (how to actually OPEN an unsigned \
         `.app`)"
    );
    // And the instruction a reader tries FIRST must be one that still works on a
    // CURRENT macOS. Sequoia (15) REMOVED right-click -> Open as a Gatekeeper
    // bypass for an unsigned app; the surviving GUI path is System Settings ->
    // Privacy & Security -> Open Anyway. Leading with the withdrawn flow promises
    // behaviour the platform took away, so it must not come first.
    let open_anyway = readme.find("Open Anyway").expect(
        "the README must give the Sequoia-era GUI path (Privacy & Security -> Open Anyway)",
    );
    assert!(
        readme.contains("Privacy & Security"),
        "the README must name where that GUI path lives (System Settings -> Privacy & Security)"
    );
    if let Some(right_click) = readme.find("right-click") {
        assert!(
            open_anyway < right_click,
            "the README must LEAD with the path that works on a current macOS (Open Anyway); \
             right-click -> Open was removed as a Gatekeeper bypass for unsigned apps in \
             macOS 15 (Sequoia) and may only appear afterwards, labelled as the older path"
        );
        assert!(
            readme[right_click..].contains("Sequoia") || readme[..right_click].contains("Sequoia"),
            "if the README keeps the right-click path it must say which macOS versions it \
             still applies to (pre-Sequoia)"
        );
    }
    assert!(
        readme.contains("android-apk-signing"),
        "the README must NAME the signing follow-on by pointing at the precedent it will copy \
         (the landed `android-apk-signing` leg)"
    );
}

#[cfg(unix)]
#[test]
fn the_macos_packaging_scripts_are_executable() {
    // The workflow invokes both by PATH (`crates/werust-macos/bundle-app.sh`),
    // exactly as the mobile legs invoke theirs, so a lost executable bit is a red
    // release leg, caught here instead of on a tag.
    use std::os::unix::fs::PermissionsExt;
    for script in [MACOS_BUNDLE_SCRIPT, MACOS_BUNDLE_CHECK] {
        let path = repo_root().join(script);
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
            .permissions()
            .mode();
        assert!(
            mode & 0o111 != 0,
            "{script} must be executable (the release workflow runs it by path)"
        );
    }
}
