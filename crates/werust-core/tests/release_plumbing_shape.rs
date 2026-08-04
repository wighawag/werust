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
//! 10. The Android APK's `versionCode`/`versionName` are DERIVED from the release
//!     tag through that same one version source (`WERUST_VERSION` / `git
//!     describe`), folded into the monotonic integer Android sequences updates
//!     on, with a placeholder that keeps an untagged local build working (task
//!     `android-apk-version-from-the-release-tag`).
//! 11. The WINDOWS desktop leg (task `windows-release-packaging-leg`,
//!     `docs/adr/0011`'s Windows split, sub-task 5) builds `werust-windows` in
//!     release for `x86_64-pc-windows-msvc` on `windows-latest` and attaches an
//!     UNSIGNED zip of the one self-contained `.exe`, as a SIBLING job decoupled
//!     from every other leg — with the application MANIFEST (comctl32 v6 +
//!     per-monitor-v2 DPI awareness) embedded by the MSVC linker at build time,
//!     and no second version source anywhere.
//! 12. The published release BODY is the conventional-commit changelog
//!     `.goreleaser.yaml` prepares, not GitHub's auto-notes: the four artifact
//!     legs create the Release with an EMPTY placeholder body (never
//!     `--generate-notes`, whose winner of the create race used to own the
//!     body), GoReleaser `release.mode: replace`s it, a re-run replaces its own
//!     first attempt's assets instead of failing 422, and the changelog's
//!     exclude filters are SCOPE-AWARE and drop the runner's bookkeeping
//!     commits (task
//!     `release-notes-lose-the-conventional-commit-changelog-to-a-race`).
//!
//! The whole test is NETWORK-ISOLATED: it only parses files in this repo (it
//! never runs Gradle, never reads a secret's value, and performs no I/O beyond
//! `std::fs::read_to_string`), so it passes identically on a fork with no secrets
//! configured and on the real repo with all of them.

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
    for leg in [
        "goreleaser",
        "android-apk",
        "ios-simulator-app",
        MACOS_LEG,
        WINDOWS_LEG,
    ] {
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
    for leg in [
        "goreleaser",
        "android-apk",
        "ios-simulator-app",
        MACOS_LEG,
        WINDOWS_LEG,
    ] {
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
    for leg in [
        "goreleaser",
        "android-apk",
        "ios-simulator-app",
        MACOS_LEG,
        WINDOWS_LEG,
    ] {
        let j = job(leg);
        assert!(
            contains_substr(&j, "actions/upload-artifact"),
            "the `{leg}` leg must upload workflow artifacts on the dispatch dry-run (actions/upload-artifact)"
        );
    }

    // Every non-GoReleaser leg attaches its artifact to the Release with
    // `gh release upload` on a tag (GoReleaser publishes its own).
    for leg in ["android-apk", "ios-simulator-app", MACOS_LEG, WINDOWS_LEG] {
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
        "expected EXACTLY one step of this leg to mention {needle:?}; found {}",
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

    // The signed build itself: `assembleRelease`, gated, with the passwords
    // supplied from the secrets IN THAT STEP's env (and the alias from a
    // variable, asserted separately below).
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
    for secret in ["ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_PASSWORD"] {
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

    // The signature must be proven with the SCHEME MATRIX, not just a cert.
    // `apksigner verify --print-certs` alone prints the signer certificates; only
    // `--verbose` prints `Verified using v1/v2/v3 scheme`, which is what decides
    // whether Android 11+ installs this APK (decision 1: a v1-only signature is
    // rejected for a targetSdk-30+ app). Without it the release recorded weaker
    // evidence than the docs claimed.
    let verify = the_one_step_mentioning(&j, "apksigner");
    for flag in ["--verbose", "--print-certs"] {
        assert!(
            contains_substr(&verify, flag),
            "the signature check must run `apksigner verify {flag}` so the release log records \
             the signature SCHEMES, not merely a certificate"
        );
    }
}

/// The key ALIAS is a repository VARIABLE, never a secret.
///
/// An alias is not key material: it is printed in the certificate of every APK
/// the key signs, and `apksigner verify --print-certs` puts it in the public
/// release log. Holding it as a SECRET is actively harmful, because GitHub
/// redacts a secret's value everywhere it appears in EVERY workflow's logs — so
/// the alias `werust` masked that substring across this whole repo's CI output
/// (`crates/***-android`, `lib***_mobile.so`, `OU=***`), degrading every log in
/// the project to hide something already public.
///
/// Pins the fix so it cannot regress into a secret again. Decision 9 in
/// docs/spikes/android-apk-signing/README.md.
#[test]
fn the_android_key_alias_is_a_variable_and_never_a_secret() {
    let j = job("android-apk");
    let sign = the_one_step_mentioning(&j, ":app:assembleRelease");
    let alias = sign
        .get("env")
        .and_then(Value::as_mapping)
        .and_then(|env| env.get(Value::String("ANDROID_KEY_ALIAS".into())))
        .and_then(Value::as_str)
        .expect("the signing step's `env:` must pass `ANDROID_KEY_ALIAS` to Gradle");

    assert!(
        alias.contains("vars.ANDROID_KEY_ALIAS"),
        "the key alias must come from the `vars` context (a repository VARIABLE); got {alias:?}"
    );
    assert!(
        !alias.contains("secrets."),
        "the key alias must NOT be a secret: GitHub redacts a secret's value in every log of the \
         repo, and an alias is public in the signing certificate anyway; got {alias:?}"
    );
    // A default keeps the documented three-secret setup working with no variable
    // configured at all, so the common path needs nothing extra.
    assert!(
        alias.contains("||"),
        "the alias expression must fall back to a literal default so a repo that follows the \
         documented keytool command needs no variable; got {alias:?}"
    );

    // And the whole leg must be free of the alias-as-secret spelling.
    for step in job_steps(&j) {
        let blob = strings_of(&step).join("\n");
        assert!(
            !blob.contains("secrets.ANDROID_KEY_ALIAS"),
            "no step may read `secrets.ANDROID_KEY_ALIAS` (it is a variable now); found in:\n{blob}"
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

// --- Criterion 10: the APK's versionCode/versionName come from the release tag ---
//
// Task `android-apk-version-from-the-release-tag`, the other half of what makes a
// signed release UPDATABLE: Android sequences updates on a strictly increasing
// INTEGER `versionCode`, so an APK that hardcodes `versionCode = 1` /
// `versionName = "0.0.0"` looks like the same build forever no matter what tag
// cut it — and its `versionName` disagrees with the version the ⋮ menu reports
// from the Rust core, the two-version-sources drift this repo keeps removing.
//
// Pinned the same way the rest of this file pins the Gradle side: by reading the
// app module's Kotlin DSL as TEXT inside the pure-Rust gate (there is no
// Kotlin-DSL parser here, and no SDK, no NDK and no network either). What the
// text has to show is exactly the contract: the version is READ from the one
// existing source, folded into one monotonic integer, and degrades to a
// placeholder instead of failing a local untagged build.
//
// Decisions (the fold, and the rejected CI-run-number alternative):
// docs/spikes/android-apk-signing/README.md.

/// The Android app module's Gradle script — the Android half of the release
/// plumbing (signing config, and now the version mapping).
const ANDROID_APP_GRADLE: &str = "crates/werust-android/app/build.gradle.kts";

/// The Android module README, where the human-facing release notes live (how to
/// build, and what installing the first signed release requires).
const ANDROID_README: &str = "crates/werust-android/README.md";

/// Where the Android release DECISIONS live: the signing ones landed there, and
/// the version mapping is the other half of the same "an update can actually be
/// offered" story, so it is recorded beside them rather than in a new file.
const ANDROID_DECISIONS: &str = "docs/spikes/android-apk-signing/README.md";

/// The right-hand side of the last `<name> =` assignment in the Gradle script,
/// trimmed. Used to assert a `defaultConfig` field is bound to a RESOLVED value
/// rather than to a committed literal.
fn gradle_assignment(gradle: &str, name: &str) -> String {
    let prefix = format!("{name} = ");
    let mut found: Vec<&str> = gradle
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix(&prefix))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected EXACTLY one `{name} = ` assignment in {ANDROID_APP_GRADLE}; found {found:?}"
    );
    found.pop().unwrap().trim().to_string()
}

#[test]
fn android_apk_version_is_read_from_the_one_existing_version_source() {
    // The version must arrive from the SAME source `crates/werust-core/build.rs`
    // resolves — `WERUST_VERSION` (which the `android-apk` job already exports
    // from the tag), else `git describe --tags --always`, else the workspace
    // Cargo version — so the APK manifest and the ⋮ menu cannot disagree. A
    // second source (a hand-bumped literal, a CI run number, a date) is exactly
    // the drift this pins shut.
    let gradle = read_repo_file(ANDROID_APP_GRADLE);

    assert!(
        gradle.contains("System.getenv(\"WERUST_VERSION\")"),
        "{ANDROID_APP_GRADLE} must read the released version from the SAME `WERUST_VERSION` the \
         `android-apk` job injects from the tag"
    );
    for arg in ["\"describe\"", "\"--tags\"", "\"--always\""] {
        assert!(
            gradle.contains(arg),
            "{ANDROID_APP_GRADLE} must fall back to the SAME `git describe --tags --always` \
             build.rs uses (missing {arg}), so an untagged dev build still reports the version \
             the core reports"
        );
    }

    // Bound to the resolved values, never to a committed literal: `versionCode =
    // 1` / `versionName = "0.0.0"` is the bug.
    let code = gradle_assignment(&gradle, "versionCode");
    assert!(
        code.parse::<i64>().is_err(),
        "`versionCode` must be bound to the version resolved from the tag, not to the literal \
         {code:?} (a literal can never increase, so no release can be offered as an update)"
    );
    let name = gradle_assignment(&gradle, "versionName");
    assert!(
        !name.starts_with('"'),
        "`versionName` must be bound to the SAME resolved version string the Rust core reports, \
         not to the literal {name:?}"
    );
}

#[test]
fn android_version_code_folds_the_semver_triple_into_one_monotonic_integer() {
    // The chosen mapping: `major * 10000 + minor * 100 + patch` (v0.2.9 -> 209,
    // v1.0.0 -> 10000). Monotonic across every release this project will
    // plausibly cut, and readable back by eye — unlike a CI run number, which is
    // monotonic too but destroys the correspondence between the APK's version and
    // the release it came from.
    let gradle = read_repo_file(ANDROID_APP_GRADLE);
    let folded = gradle.replace(' ', "");
    assert!(
        folded.contains("*10000+") && folded.contains("*100+"),
        "{ANDROID_APP_GRADLE} must fold the semver triple as `major * 10000 + minor * 100 + \
         patch` (the recorded mapping)"
    );
    // Only a CLEAN triple folds; anything else (a `git describe` suffix, a
    // pre-release tag) is not a released version.
    assert!(
        gradle.contains(r"^(\d+)\.(\d+)\.(\d+)$"),
        "{ANDROID_APP_GRADLE} must fold only a CLEAN `major.minor.patch` triple (anchored), so a \
         `git describe`-shaped dev version takes the placeholder path instead of folding into a \
         meaningless code"
    );
}

/// The Gradle text that BINDS `werustVersionCode`: everything between its
/// declaration and the `android { }` extension it feeds. The dev-versus-release
/// branch lives here, so the assertions below can talk about that ONE decision
/// site instead of sweeping the whole file (where the word `GradleException`
/// also appears for the NDK lookup and the ≤ 99 collision guard).
fn version_code_binding(gradle: &str) -> String {
    let after = gradle
        .split_once("val werustVersionCode")
        .unwrap_or_else(|| panic!("{ANDROID_APP_GRADLE} must bind `werustVersionCode`"))
        .1;
    after
        .split_once("android {")
        .expect("the `android { }` extension follows the version block")
        .0
        .to_string()
}

#[test]
fn a_local_untagged_android_build_keeps_working_on_a_placeholder() {
    // A local `./gradlew :app:assembleDebug` with no tag, no `WERUST_VERSION` and
    // possibly no git must still BUILD and still INSTALL: a dev APK with a
    // placeholder version is a far better outcome than a dev APK that will not
    // build. So the mapping degrades to the previous hardcoded `versionCode = 1`
    // rather than throwing, and every version lookup is failure-tolerant.
    let gradle = read_repo_file(ANDROID_APP_GRADLE);
    assert!(
        gradle.contains("devPlaceholderVersionCode = 1"),
        "{ANDROID_APP_GRADLE} must keep the `versionCode = 1` placeholder for an untagged local \
         build (named, so the fallback is visible)"
    );
    assert!(
        version_code_binding(&gradle).contains("devPlaceholderVersionCode"),
        "the resolved versionCode must FALL BACK to the placeholder when no version was INJECTED \
         (the dev path), never fail an untagged local build"
    );
    assert!(
        gradle.contains("catch"),
        "the `git describe` lookup must be failure-TOLERANT (no git, a source tarball, a git that \
         errors) exactly as build.rs's is"
    );
}

#[test]
fn an_injected_release_version_that_cannot_be_sequenced_fails_the_apk_build() {
    // Task `android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1`.
    //
    // `release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is an acceptable
    // release tag today. It is not a clean triple, so it folds to no
    // `versionCode` — and the placeholder fallback above would then attach a
    // SIGNED release APK carrying `versionCode = 1`: unsequenceable,
    // un-updatable, indistinguishable from every dev build. That is precisely the
    // bug `android-apk-version-from-the-release-tag` existed to remove, reachable
    // through the front door, and the OPPOSITE of what decision 5 already does
    // for an out-of-range component (a loud `GradleException`).
    //
    // So the tolerance is keyed on DEV-versus-RELEASE, not on the shape of the
    // string: the placeholder is right for a build with no injected version and
    // wrong for one CI resolved from a tag. `WERUST_VERSION`'s PRESENCE is the
    // only thing that says "a release is being cut" — the other two sources
    // (`git describe`, the workspace Cargo version) are dev sources by
    // construction.
    let gradle = read_repo_file(ANDROID_APP_GRADLE);

    // The distinction has to be NAMED, and read from the injected variable.
    assert!(
        gradle.contains("val injectedReleaseVersion"),
        "{ANDROID_APP_GRADLE} must name the dev-versus-release distinction \
         (`injectedReleaseVersion`), so the failure path keys on WHICH BUILD this is rather than \
         on the shape of the resolved string"
    );
    let injected = gradle_assignment(&gradle, "val injectedReleaseVersion: String?");
    assert!(
        injected.contains("System.getenv(\"WERUST_VERSION\")"),
        "`injectedReleaseVersion` must come from the PRESENCE of `WERUST_VERSION` (the variable \
         the `android-apk` job sets only on a tag), not from the resolved version — `git \
         describe` and the Cargo fallback are dev sources; got {injected:?}"
    );

    // The failure itself, at the one binding that decides the shipped code.
    let binding = version_code_binding(&gradle);
    assert!(
        binding.contains("injectedReleaseVersion != null"),
        "the versionCode binding must BRANCH on whether a version was injected: fail for a \
         release, placeholder for a dev build; got:\n{binding}"
    );
    assert!(
        binding.contains("throw GradleException"),
        "an injected version that folds to no versionCode must FAIL the build loudly (the same \
         treatment decision 5 gives an out-of-range component), never ship the placeholder; \
         got:\n{binding}"
    );

    // The message has to be actionable: WHICH version, WHY it cannot be
    // sequenced, and WHAT shape would work. A bare "invalid version" on a red
    // release job costs the reader the whole investigation.
    assert!(
        binding.contains("$injectedReleaseVersion"),
        "the failure message must NAME the offending version (interpolate \
         `injectedReleaseVersion`); got:\n{binding}"
    );
    assert!(
        binding.contains("sequence"),
        "the failure message must say WHY it fails (it cannot be SEQUENCED as an update); \
         got:\n{binding}"
    );
    assert!(
        binding.contains("major.minor.patch"),
        "the failure message must state the ACCEPTED shape (a clean `major.minor.patch` triple); \
         got:\n{binding}"
    );
}

#[test]
fn the_pre_release_sequencing_mapping_is_recorded_as_deliberately_undesigned() {
    // The alternative to failing is a mapping that CAN sequence a pre-release tag
    // (reserving a digit for `rc1`, the way many Android projects do). That is a
    // product decision about whether this project ever cuts pre-release tags at
    // all, so it is deliberately NOT invented here — and "we did not decide this"
    // is only useful if it is written down next to the decisions it abstains
    // from, together with what would have to be settled if it is ever wanted.
    let decisions = read_repo_file(ANDROID_DECISIONS);
    assert!(
        decisions.contains("pre-release"),
        "{ANDROID_DECISIONS} must record what happens to a PRE-RELEASE tag (`v0.3.0-rc1`), since \
         `release.yml`'s `v*` trigger accepts one"
    );
    assert!(
        decisions.contains("rc1"),
        "{ANDROID_DECISIONS} must name the concrete case (an `rc1` tag), not just the category"
    );
    assert!(
        decisions.to_lowercase().contains("not designed")
            || decisions.to_lowercase().contains("undesigned"),
        "{ANDROID_DECISIONS} must say the pre-release sequencing mapping is deliberately NOT \
         designed here (an abstention is a decision, and an unrecorded one reads as an oversight)"
    );
}

/// Every value of `job` that could actually MINT something: each step's `run`
/// script (with whole-line shell comments dropped, per [`shell_code_of`]) and
/// every scalar under a `with:` or `env:` mapping, step-level and job-level.
///
/// Deliberately NOT "every string in the job". A step `name:` and an explanatory
/// comment are DOCUMENTATION, and this repo's whole habit is a comment that
/// explains WHY next to the thing it explains; a guard that reds the gate
/// because the word it forbids appears in prose punishes the documentation
/// instead of the defect. Only these three carry values the runner executes or
/// hands to a tool, so they are where a second source could actually be minted.
fn minting_values_of(job: &Value) -> Vec<String> {
    /// The scalars under a node's `with:` / `env:` mappings.
    fn handed_values(v: &Value) -> Vec<String> {
        ["env", "with"]
            .iter()
            .filter_map(|key| v.get(key))
            .flat_map(strings_of)
            .collect()
    }

    let mut out = handed_values(job);
    for step in job_steps(job) {
        out.extend(handed_values(&step));
        if let Some(run) = step.get("run").and_then(Value::as_str) {
            out.push(shell_code_of(run));
        }
    }
    out
}

#[test]
fn the_android_leg_mints_no_second_version_source_in_the_workflow() {
    // The workflow's ONLY version input stays the job-level `WERUST_VERSION` the
    // shared loop above already pins. A version computed in the JOB (a run
    // number, a `git describe`, a hand-written versionCode passed as a Gradle
    // property) would be the second source that makes the APK manifest and the ⋮
    // menu disagree.
    //
    // Scoped to the values that could MINT one (`run` code, `with:`/`env:`
    // values) rather than to the whole job, so an explanatory comment or a step
    // name may say the words without reding the gate for a non-defect.
    let j = job("android-apk");
    let values = minting_values_of(&j);
    for second_source in [
        "git describe",
        "github.run_number",
        "versionCode",
        "versionName",
    ] {
        if let Some(offender) = values.iter().find(|v| v.contains(second_source)) {
            panic!(
                "the `android-apk` leg must not derive a version from `{second_source}`: the \
                 version comes from `WERUST_VERSION`, resolved once, so the APK manifest and the \
                 ⋮ menu cannot disagree. Found it in a run/with/env VALUE:\n{offender}"
            );
        }
    }
}

#[test]
fn the_second_version_source_guard_reads_values_not_comments() {
    // The guard above is only worth having if it still has TEETH after being
    // narrowed, so both halves are pinned here against synthetic jobs — the
    // cheapest way to prove a NEGATIVE assertion is actually load-bearing rather
    // than vacuously true.
    let minting: Value = serde_yaml::from_str(
        r#"
steps:
  - name: Build the APK
    run: |
      ./gradlew :app:assembleDebug -PversionCode="$(git describe --tags)"
"#,
    )
    .expect("synthetic job YAML");
    let minted = minting_values_of(&minting).join("\n");
    for second_source in ["versionCode", "git describe"] {
        assert!(
            minted.contains(second_source),
            "the guard must still CATCH `{second_source}` in a `run:` value; got:\n{minted}"
        );
    }

    let documented: Value = serde_yaml::from_str(
        r#"
steps:
  - name: Build the APK (versionCode comes from Gradle, not from here)
    # The versionName is resolved by build.gradle.kts from WERUST_VERSION.
    run: |
      # Not `git describe` here: that would mint a second version source.
      ./gradlew :app:assembleDebug
"#,
    )
    .expect("synthetic job YAML");
    let documented_values = minting_values_of(&documented).join("\n");
    for second_source in ["versionCode", "versionName", "git describe"] {
        assert!(
            !documented_values.contains(second_source),
            "the guard must NOT fire on `{second_source}` in a step name, a YAML comment or a \
             shell comment (documentation is not a second source); got:\n{documented_values}"
        );
    }
}

#[test]
fn the_apk_version_mapping_decision_is_recorded_beside_the_signing_decisions() {
    // Criterion 5 of the task: the mapping is a USER-VISIBLE, hard-to-reverse
    // choice (a versionCode can never go DOWN for an installed app), so it is
    // recorded — with the alternative it rejected — where the signing decisions
    // already live, because the two together are what make a release updatable.
    let decisions = read_repo_file(ANDROID_DECISIONS);
    assert!(
        decisions
            .replace(' ', "")
            .contains("major*10000+minor*100+patch"),
        "{ANDROID_DECISIONS} must record the CHOSEN versionCode mapping (major * 10000 + minor * \
         100 + patch)"
    );
    assert!(
        decisions.contains("run number"),
        "{ANDROID_DECISIONS} must record the REJECTED alternative (a CI run number / timestamp: \
         monotonic, but it destroys the correspondence to the release it came from)"
    );
}

#[test]
fn the_android_readme_warns_that_the_first_signed_release_needs_an_uninstall() {
    // The release APK keeps the DEBUG `applicationId` but is signed with a
    // different key, so a device holding a previously installed debug APK must
    // uninstall it before the signed one will install. A one-time transition, not
    // a defect — but an undocumented one looks exactly like a broken download.
    let readme = read_repo_file(ANDROID_README);
    assert!(
        readme.contains("uninstall"),
        "{ANDROID_README} must say that installing the first RELEASE-signed APK over a previously \
         installed debug APK requires an uninstall first (different signing key, same \
         applicationId)"
    );
    // And the version story the same reader needs: what the APK's version now IS.
    assert!(
        readme.contains("versionCode"),
        "{ANDROID_README} must explain where the APK's `versionCode`/`versionName` come from (the \
         release tag, via the one version source)"
    );
}

#[test]
fn the_resolved_version_reaches_the_cross_compiled_core_too_not_just_the_manifest() {
    // The manifest and the ⋮ menu must agree, and they are produced by two
    // different steps of the SAME Gradle build: `defaultConfig` (evaluated every
    // configuration) and the `cargoBuildRustCore` cross-compile (an UP-TO-DATE-
    // checked task). Without the resolved version as a task INPUT, a local
    // rebuild after the version changed re-stamps the manifest while reusing the
    // previously compiled `libwerust_mobile.so` — observed on a real build: the
    // APK read `versionCode=300 / versionName=0.3.0` while the packaged `.so`
    // still carried `0.2.9-91-g…`, i.e. exactly the disagreement this task
    // exists to make impossible.
    //
    // So the ONE resolved version is BOTH the manifest's and the cargo build's:
    // declared as an input so a changed version re-runs the cross-compile, and
    // exported into the cargo environment so `build.rs` resolves that same value
    // instead of whatever the (possibly reused) Gradle daemon inherited.
    let gradle = read_repo_file(ANDROID_APP_GRADLE);
    assert!(
        gradle.contains("environment(\"WERUST_VERSION\""),
        "{ANDROID_APP_GRADLE} must export the RESOLVED version into the cargo cross-compile's \
         environment, so the core the APK carries reports the same string the manifest declares"
    );
    let cargo_task = gradle
        .split_once("abstract class CargoBuildRustCore")
        .expect("the cross-compile task class")
        .1;
    let task_body = cargo_task
        .split_once("@TaskAction")
        .expect("the task's property block, before its action")
        .0;
    assert!(
        task_body.contains("werustVersion"),
        "the cross-compile task must take the resolved version as a declared @Input, or a \
         version change leaves an UP-TO-DATE task shipping a stale core"
    );
}

// --- Criterion 11: the Windows desktop packaging leg (task `windows-release-packaging-leg`) ---
//
// The FIFTH release leg, and the last sub-task of `docs/adr/0011`'s Windows
// split: `windows-win32-window-and-chrome` shipped a window a person can use,
// and nothing yet handed it to them. This leg builds `werust-windows` in release
// for `x86_64-pc-windows-msvc` on the `windows-latest` runner the two existing
// Windows legs already use, and attaches a zip of the one self-contained `.exe`
// to the tagged Release beside the desktop Linux binary, the Android APK, the
// iOS Simulator `.app` and the macOS `.app`.
//
// A SIBLING of `windows-renderer.yml`'s job rather than an extension of it: that
// leg COMPILES AND EXERCISES the Windows crates on every relevant push, and this
// one PACKAGES them on a tag. It carries the same decoupling every other release
// leg carries (`needs: verify` only), so no platform can withhold another
// platform's artifact.
//
// What is genuinely NEW here versus the macOS twin is the application MANIFEST.
// The window deliberately shipped without one
// (`docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §4), because
// embedding a Win32 manifest is a BUILD-TIME concern, and two user-visible
// consequences were parked on this task: classic-styled chrome (no comctl32 v6)
// and a DPI-unaware process. The manifest lands here, embedded by the MSVC
// linker, and the assertions below pin its two declarations plus the mechanism.
//
// Pinned HERE, in the pure-Rust gate, exactly like the rest of this file: the
// assertions parse the workflow YAML and read the manifest, the build script and
// the check script as text, so they need no Windows, no MSVC toolchain and no
// network. What they CANNOT judge is how any of it LOOKS, which is why the
// by-hand items stay named in
// `docs/spikes/windows-win32-window-and-chrome/README.md`.
// Decisions: docs/spikes/windows-release-packaging-leg/README.md.

/// The Windows desktop leg's job key in `release.yml`.
const WINDOWS_LEG: &str = "windows-desktop-app";

/// The one target this leg ships. `*-pc-windows-msvc` statically links
/// `WebView2LoaderStatic.lib`, which is what makes a SINGLE-exe zip possible at
/// all (`docs/adr/0011` finding 6); the `-gnu` target would have to ship
/// `WebView2Loader.dll` beside it.
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

/// The shipped binary, and the only entry in the zip.
const WINDOWS_EXE: &str = "werust-windows.exe";

/// The attached asset. NAMED unsigned, the Android/macOS honest-naming
/// precedent, so nothing on the Release page implies a signature it lacks.
const WINDOWS_ZIP: &str = "werust-windows-x86_64-unsigned.zip";

/// The application manifest the chrome was waiting for: comctl32 v6 (visual
/// styles) and per-monitor-v2 DPI awareness.
const WINDOWS_APP_MANIFEST: &str = "crates/werust-windows/app.manifest";

/// The build script that EMBEDS it, by handing the MSVC linker the manifest as
/// an input to embed (the recorded mechanism; see the spike README, decision 1).
const WINDOWS_BUILD_SCRIPT: &str = "crates/werust-windows/build.rs";

/// The BUILD-leg acceptance check the job runs on the built `.exe` and its zip:
/// the Windows twin of `check-apk-abis.sh` / `check-app-bundle.sh` /
/// `check-macos-app-bundle.sh`, and the place the "the manifest is really
/// embedded" and "the exe carries the ONE version" assertions actually EXECUTE
/// (neither can run in this Linux gate, which has no `.exe`).
const WINDOWS_ARTIFACT_CHECK: &str =
    "docs/spikes/windows-release-packaging-leg/check-windows-artifact.sh";

#[test]
fn windows_desktop_leg_is_a_decoupled_sibling_on_the_windows_latest_runner() {
    // Criterion 4: the existing `windows-latest` runner shape (the one
    // `windows-renderer.yml` and `windows-origin-probe.yml` already use, so no
    // new runner class), and decoupled in BOTH directions.
    let j = job(WINDOWS_LEG);
    assert_eq!(
        j.get("runs-on").and_then(Value::as_str),
        Some("windows-latest"),
        "the Windows desktop leg MUST run on the existing `windows-latest` runner shape (the MSVC \
         toolchain and the Windows SDK are Windows-only)"
    );
    let needs = strings_of(
        j.get("needs")
            .unwrap_or_else(|| panic!("the `{WINDOWS_LEG}` job must declare `needs:`")),
    );
    assert!(
        needs.iter().any(|n| n == "verify"),
        "the `{WINDOWS_LEG}` leg must `needs: verify` (gated on a green tree, like every leg); \
         got {needs:?}"
    );
    for sibling in ["goreleaser", "ios-simulator-app", "android-apk", MACOS_LEG] {
        assert!(
            !needs.iter().any(|n| n == sibling),
            "the `{WINDOWS_LEG}` leg must NOT `needs: {sibling}`: it is a SIBLING leg, so no other \
             platform's failure may withhold the Windows artifact; got {needs:?}"
        );
    }
    // Decoupled means the Release may not exist yet on a tag, so this leg
    // guarantees its EXISTENCE the same idempotent way every other leg does.
    assert!(
        contains_substr(&j, "gh release create"),
        "the `{WINDOWS_LEG}` leg must idempotently `gh release create` on a tag (a \
         Release-EXISTENCE guarantee, since it waits on no other leg)"
    );
}

#[test]
fn windows_desktop_leg_builds_the_msvc_target_and_checks_the_shipped_exe() {
    // Criterion 1: the release binary for the one shipped triple, zipped, with
    // the artifact CHECKED on the runner rather than assumed.
    let j = job(WINDOWS_LEG);
    let values = minting_values_of(&j).join("\n");
    assert!(
        values.contains("-p werust-windows"),
        "the `{WINDOWS_LEG}` leg must build the WINDOW crate (`-p werust-windows`), not the \
         GTK-bound `werust` binary, which cannot build on Windows at all"
    );
    assert!(
        values.contains(WINDOWS_TARGET),
        "the `{WINDOWS_LEG}` leg must build for `{WINDOWS_TARGET}` (the triple whose static \
         WebView2 loader makes a single-exe zip possible)"
    );
    assert!(
        values.contains("--release"),
        "the shipped binary must be a RELEASE build (a debug build also leaves devtools enabled, \
         which every platform's `web-inspector` row gates on `debug_assertions`)"
    );
    assert!(
        contains_substr(&j, WINDOWS_ARTIFACT_CHECK),
        "the `{WINDOWS_LEG}` leg must RUN the BUILD-leg check `{WINDOWS_ARTIFACT_CHECK}` over what \
         it built (the Windows twin of check-apk-abis.sh / check-macos-app-bundle.sh)"
    );
    // The asset that travels is the ZIP, and it is named for what it is.
    for step in ["gh release upload", "actions/upload-artifact"] {
        let s = the_one_step_mentioning(&j, step);
        assert!(
            contains_substr(&s, WINDOWS_ZIP),
            "the `{step}` step must carry `{WINDOWS_ZIP}` (a bare `.exe` is not what this leg \
             attaches, and the name must say UNSIGNED)"
        );
    }

    // The check itself must assert the two things only a Windows runner can see:
    // that the manifest is really IN the shipped exe, and that the zip really
    // contains the exe.
    let check = read_repo_file(WINDOWS_ARTIFACT_CHECK);
    for needle in [
        "Microsoft.Windows.Common-Controls",
        "PerMonitorV2",
        WINDOWS_EXE,
    ] {
        assert!(
            check.contains(needle),
            "{WINDOWS_ARTIFACT_CHECK} must assert `{needle}` is present in what the leg built"
        );
    }
}

#[test]
fn the_windows_app_manifest_declares_visual_styles_and_per_monitor_v2() {
    // Criterion 2, the half of this task that is NOT packaging plumbing. Both
    // declarations are user-visible and neither can be checked by any runner:
    //
    //   * comctl32 v6 -- without the dependency the process links comctl32 5.82
    //     and every control draws in the pre-Vista style
    //     (`docs/spikes/windows-win32-window-and-chrome/DECISIONS.md` §4).
    //   * PerMonitorV2 -- without it Windows bitmap-scales the whole process on
    //     a 150%/200% display.
    let manifest = read_repo_file(WINDOWS_APP_MANIFEST);
    for declaration in [
        // The comctl32 v6 dependency, by its documented identity: the name, the
        // version and the public key token are ALL part of it, and a wrong one
        // silently leaves the app on 5.82.
        "Microsoft.Windows.Common-Controls",
        "6.0.0.0",
        "6595b64144ccf1df",
        // Per-monitor-v2 DPI awareness.
        "dpiAwareness",
        "PerMonitorV2",
    ] {
        assert!(
            manifest.contains(declaration),
            "{WINDOWS_APP_MANIFEST} must declare `{declaration}`"
        );
    }
    // `<dpiAwareness>` is Windows 10 1607+; older Windows ignores it and reads
    // `<dpiAware>` instead, so both are declared (Microsoft's own documented
    // pairing). Without the fallback a Windows 10 1511 machine is DPI-UNAWARE.
    assert!(
        manifest.contains("dpiAware>") && manifest.contains("true/pm"),
        "{WINDOWS_APP_MANIFEST} must ALSO carry the legacy `<dpiAware>true/pm</dpiAware>` \
         fallback, which is what a pre-1607 Windows 10 reads"
    );
    // The side-by-side ENVELOPE, by its two identifying strings. This says
    // nothing about well-formedness (two `contains` calls cannot): that is
    // `the_windows_app_manifest_is_well_formed_xml` below, which exists because
    // this assertion USED to claim it and a malformed manifest sailed past it
    // into a red release job.
    assert!(
        manifest.contains("urn:schemas-microsoft-com:asm.v1") && manifest.contains("</assembly>"),
        "{WINDOWS_APP_MANIFEST} must be a side-by-side assembly manifest (the asm.v1 namespace, \
         closed by `</assembly>`)"
    );
}

// ---------------------------------------------------------------------------
// The manifest is XML, and NOTHING in this repo parsed it (task
// `windows-app-manifest-is-malformed-xml-and-breaks-the-release-link`).
//
// `mt.exe` parses it on the Windows runner, at LINK time, and refuses a
// malformed one: `general error c1010070` -> `LNK1327` -> the whole
// `windows-desktop-app` release job, and its artifact, are lost (release run
// 30624906073). The Ubuntu gate cannot link a Windows binary, so the ONLY way it
// can hold that contract is to parse the file itself. The shape assertions above
// pin identity STRINGS, which a malformed file satisfies just as happily.
//
// The trigger was this repo's own prose style: a `--` used as a dash inside an
// XML comment, which XML forbids outright (XML 1.0 §2.5). The comments are
// load-bearing and stay; only the punctuation was illegal.
//
// DECISION (dependency-free, hand-written scanner rather than an XML crate):
// `docs/spikes/windows-app-manifest-is-malformed-xml-and-breaks-the-release-link/README.md`.
// In short: one 60-line file, in ONE crate's dev-dependencies, would not justify
// a new parsing lineage in a workspace whose deps are each argued for by name;
// the scanner below covers exactly the vocabulary an application manifest uses
// and REFUSES anything outside it (loudly) rather than passing it silently, and
// `the_manifest_guard_has_teeth` proves it catches the regression that actually
// shipped instead of trivially returning "fine".
// ---------------------------------------------------------------------------

/// Line and column (both 1-based) of a byte offset, so the guard's message reads
/// like the parser's own ("line 17, column 52") and points at the character to
/// fix rather than at the file.
fn xml_line_and_column(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// One reported problem, located.
fn xml_problem(src: &str, offset: usize, problem: &str) -> Option<String> {
    let (line, column) = xml_line_and_column(src, offset);
    Some(format!("{problem} (line {line}, column {column})"))
}

/// The length in bytes of the XML Name at the start of `s` (0 if there is none).
fn xml_name_len(s: &str) -> usize {
    let mut len = 0;
    for (idx, ch) in s.char_indices() {
        let ok = if idx == 0 {
            ch.is_ascii_alphabetic() || ch == '_' || ch == ':'
        } else {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-' | '.')
        };
        if !ok {
            break;
        }
        len = idx + ch.len_utf8();
    }
    len
}

/// `true` if `s` STARTS with a syntactically valid entity or character
/// reference (`&amp;`, `&#38;`, `&#x26;`). A bare `&` is not well-formed XML,
/// and is the other punctuation trap waiting in a hand-edited manifest whose
/// `<description>` is English prose.
fn xml_reference_is_valid(s: &str) -> bool {
    let Some(end) = s[1..].find(';') else {
        return false;
    };
    let body = &s[1..1 + end];
    match body.strip_prefix('#') {
        Some(digits) => {
            let digits = digits.strip_prefix('x').unwrap_or(digits);
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => !body.is_empty() && xml_name_len(body) == body.len(),
    }
}

/// Scan `src` for the FIRST well-formedness violation, `None` if there is none.
///
/// Deliberately narrow: it understands the XML declaration, comments, CDATA,
/// elements (nesting, matching end tags, quoted attribute values, self-closing
/// tags), character data and references, which is the complete vocabulary of a
/// Win32 application manifest. Anything else (a DOCTYPE, an internal subset) is
/// REPORTED rather than skipped, so this can never silently bless a construct it
/// does not actually check. It is not a validator: it says nothing about
/// namespaces, schemas or whether `mt.exe` likes the CONTENT, only about the
/// well-formedness `mt.exe` refused.
fn xml_well_formedness_error(src: &str) -> Option<String> {
    let bytes = src.as_bytes();
    // Every open start tag, with the offset it was opened at.
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut roots = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            // Character data.
            if bytes[i] == b'&' && !xml_reference_is_valid(&src[i..]) {
                return xml_problem(
                    src,
                    i,
                    "a bare `&` is not well-formed XML: write `&amp;` (or a character reference)",
                );
            }
            if src[i..].starts_with("]]>") {
                return xml_problem(
                    src,
                    i,
                    "the sequence `]]>` may not appear in character data",
                );
            }
            i += 1;
            continue;
        }

        let rest = &src[i..];

        if rest.starts_with("<!--") {
            let content_at = i + 4;
            let Some(rel) = src[content_at..].find("-->") else {
                return xml_problem(src, i, "an XML comment is never closed (no `-->`)");
            };
            let content = &src[content_at..content_at + rel];
            if let Some(dash) = content.find("--") {
                return xml_problem(
                    src,
                    content_at + dash,
                    "an XML comment may not contain `--` (XML 1.0 §2.5), so this repo's prose dash \
                     is illegal HERE: rephrase with a comma, a colon, parentheses or two \
                     sentences (NOT a spaced hyphen)",
                );
            }
            if content.ends_with('-') {
                return xml_problem(
                    src,
                    content_at + content.len(),
                    "an XML comment may not end with `-` (it would close as `--->`)",
                );
            }
            i = content_at + rel + 3;
            continue;
        }

        if rest.starts_with("<?") {
            let Some(rel) = src[i + 2..].find("?>") else {
                return xml_problem(
                    src,
                    i,
                    "an XML declaration / processing instruction is never closed (no `?>`)",
                );
            };
            i += 2 + rel + 2;
            continue;
        }

        if rest.starts_with("<![CDATA[") {
            let Some(rel) = src[i + 9..].find("]]>") else {
                return xml_problem(src, i, "a CDATA section is never closed (no `]]>`)");
            };
            i += 9 + rel + 3;
            continue;
        }

        if rest.starts_with("<!") {
            return xml_problem(
                src,
                i,
                "this guard understands only the declaration, comments, CDATA and elements an \
                 application manifest is made of; it refuses to bless a `<!…>` construct it does \
                 not parse",
            );
        }

        if rest.starts_with("</") {
            let name_at = i + 2;
            let name_len = xml_name_len(&src[name_at..]);
            if name_len == 0 {
                return xml_problem(src, name_at, "expected an element name after `</`");
            }
            let name = &src[name_at..name_at + name_len];
            let mut j = name_at + name_len;
            while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                j += 1;
            }
            if bytes.get(j) != Some(&b'>') {
                return xml_problem(src, j.min(bytes.len()), "expected `>` to close the end tag");
            }
            match open.pop() {
                None => {
                    return xml_problem(
                        src,
                        i,
                        &format!("end tag `</{name}>` has no matching start tag"),
                    );
                }
                Some((expected, opened_at)) if expected != name => {
                    let (line, _) = xml_line_and_column(src, opened_at);
                    return xml_problem(
                        src,
                        i,
                        &format!(
                            "end tag `</{name}>` does not match the still-open `<{expected}>` from \
                             line {line}"
                        ),
                    );
                }
                Some(_) => {}
            }
            i = j + 1;
            continue;
        }

        // A start tag (possibly self-closing).
        let name_at = i + 1;
        let name_len = xml_name_len(&src[name_at..]);
        if name_len == 0 {
            return xml_problem(
                src,
                i,
                "`<` must start a tag, a comment or a declaration here: escape a literal one as \
                 `&lt;`",
            );
        }
        let name = &src[name_at..name_at + name_len];
        let mut j = name_at + name_len;
        let mut quote: Option<u8> = None;
        let self_closing;
        loop {
            let Some(&c) = bytes.get(j) else {
                return xml_problem(src, i, &format!("the start tag `<{name}` is never closed"));
            };
            match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    } else if c == b'&' && !xml_reference_is_valid(&src[j..]) {
                        return xml_problem(
                            src,
                            j,
                            "a bare `&` in an attribute value is not well-formed XML: write \
                             `&amp;`",
                        );
                    }
                }
                None => {
                    if c == b'"' || c == b'\'' {
                        quote = Some(c);
                    } else if c == b'<' {
                        return xml_problem(src, j, "`<` may not appear inside a tag");
                    } else if c == b'>' {
                        self_closing = bytes[j - 1] == b'/';
                        break;
                    }
                }
            }
            j += 1;
        }
        if open.is_empty() {
            roots += 1;
            if roots > 1 {
                return xml_problem(
                    src,
                    i,
                    &format!("`<{name}>` is a SECOND root element; XML allows exactly one"),
                );
            }
        }
        if !self_closing {
            open.push((name, i));
        }
        i = j + 1;
    }

    if let Some((name, opened_at)) = open.last() {
        return xml_problem(src, *opened_at, &format!("`<{name}>` is never closed"));
    }
    if roots == 0 {
        return Some("the document has no root element".to_string());
    }
    None
}

#[test]
fn the_windows_app_manifest_is_well_formed_xml() {
    // THE GUARD. `mt.exe` parses this file at link time on the Windows runner,
    // and the Ubuntu gate cannot link a Windows binary, so the gate parses it
    // here instead: an edit to those (load-bearing, deliberately kept) comments
    // can no longer red a release job that only runs on a tag.
    let manifest = read_repo_file(WINDOWS_APP_MANIFEST);
    if let Some(problem) = xml_well_formedness_error(&manifest) {
        panic!(
            "{WINDOWS_APP_MANIFEST} is not well-formed XML: {problem}.\n`mt.exe` refuses a \
             malformed manifest (`general error c1010070`), `link.exe` then dies (`LNK1327`), and \
             the `{WINDOWS_LEG}` release job ships NOTHING (release run 30624906073)."
        );
    }
}

#[test]
fn the_manifest_guard_has_teeth() {
    // A guard nobody can see fail is not a guard. Each sample below is a way
    // this file has broken or could break; the FIRST is verbatim the shape that
    // actually cost a release artifact (a `--` used as a prose dash inside a
    // comment), and the last is the manifest as it stands, which must pass.
    let broken = [
        (
            "the `--` prose dash that broke run 30624906073",
            "<!--\n  the identity (name + version + publicKeyToken) -- a wrong token is silent.\n\
             -->\n<assembly/>",
        ),
        (
            "a comment ending in a hyphen",
            "<!-- a trailing hyphen -\n--->\n<assembly/>",
        ),
        ("an unterminated comment", "<!-- forever\n<assembly/>"),
        (
            "a mismatched end tag",
            "<assembly><dependency></assembly></dependency>",
        ),
        ("an unclosed element", "<assembly><dependency></assembly>"),
        ("an unclosed start tag", "<assembly\n"),
        (
            "an unquoted-run-on attribute value",
            "<assembly name=\"werust>\n",
        ),
        (
            "a bare `&` in prose",
            "<description>werust & friends</description>",
        ),
        ("two root elements", "<assembly/><assembly/>"),
        ("no root element at all", "<!-- only a comment -->\n"),
    ];
    for (what, sample) in broken {
        assert!(
            xml_well_formedness_error(sample).is_some(),
            "the manifest guard must REJECT {what}, or it cannot hold the contract it claims: \
             {sample:?}"
        );
    }

    // …and it must not cry wolf on the constructs the manifest legitimately uses.
    let fine = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
        "<!-- a comment with a hyphenated-word, an <element> and a stray - dash -->\n",
        "<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\">\n",
        "  <assemblyIdentity type=\"win32\" name=\"a.b-c\" version=\"1.0.0.0\" />\n",
        "  <description>prose with an &amp; and a &#x2014; reference</description>\n",
        "  <windowsSettings><dpiAware>true/pm</dpiAware></windowsSettings>\n",
        "</assembly>\n",
    );
    assert_eq!(
        xml_well_formedness_error(fine),
        None,
        "the manifest guard must ACCEPT the ordinary manifest vocabulary"
    );
}

#[test]
fn the_windows_manifest_is_embedded_by_the_linker_for_the_binaries_that_run() {
    // Criterion 2's other half: the MECHANISM, recorded and pinned. The manifest
    // is handed to the MSVC linker (`/MANIFEST:EMBED` + `/MANIFESTINPUT:`) from
    // the crate's build script, which is what makes it a RESOURCE inside the
    // shipped `.exe` rather than a loose file beside it.
    let build = read_repo_file(WINDOWS_BUILD_SCRIPT);
    for flag in ["/MANIFEST:EMBED", "/MANIFESTINPUT:"] {
        assert!(
            build.contains(flag),
            "{WINDOWS_BUILD_SCRIPT} must pass `{flag}` to the MSVC linker (the recorded embedding \
             mechanism)"
        );
    }
    // Gated on the MSVC target: these are link.exe flags, and a build script that
    // emitted them unconditionally would break every non-Windows build of this
    // workspace member -- which the Ubuntu `verify` gate compiles on every run.
    assert!(
        build.contains("CARGO_CFG_TARGET_ENV") && build.contains("msvc"),
        "{WINDOWS_BUILD_SCRIPT} must emit the linker flags ONLY for the MSVC target (the Ubuntu \
         gate builds this crate too)"
    );
    assert!(
        build.contains("rerun-if-changed"),
        "{WINDOWS_BUILD_SCRIPT} must declare `rerun-if-changed` for the manifest, or an edit to it \
         would not re-link"
    );
    // BOTH kinds of binary this crate produces get it: the shipped `werust-windows.exe`
    // AND the `window_smoke` example, which is the ONLY place this window is
    // executed anywhere. A manifest that applied to the product but not to the
    // smoke would ship a comctl32 version no test had ever run under.
    for scope in ["rustc-link-arg-bins", "rustc-link-arg-examples"] {
        assert!(
            build.contains(scope),
            "{WINDOWS_BUILD_SCRIPT} must apply the manifest via `{scope}`, so the CI smoke runs in \
             the SAME configuration the release ships"
        );
    }
}

#[test]
fn the_windows_progress_bar_keeps_the_shared_palette_under_visual_styles() {
    // The one behaviour change the comctl32 v6 manifest FORCES in the window, and
    // the reason it is pinned in the release-plumbing file rather than left to be
    // discovered: `PBM_SETBARCOLOR` "has no effect" once visual styles are
    // enabled (Microsoft's own documented remark), so the moment this task's
    // manifest lands, the URL bar's in-flight progress strip would silently stop
    // being the SHARED palette's blue and start being whatever the theme paints.
    //
    // That is exactly the cross-edge colour drift `desktop-paint`'s
    // `gtk_stylesheet_agreement` test exists to prevent, arriving through a
    // packaging change instead of a code change. The window therefore opts THAT
    // ONE control out of theming (`SetWindowTheme(progress, "", "")`), which is
    // the documented way to keep a custom bar colour.
    let window = read_repo_file("crates/werust-windows/src/window.rs");
    assert!(
        window.contains("SetWindowTheme("),
        "the window must opt the progress bar out of visual styles, or the comctl32 v6 manifest \
         silently drops the shared palette's LOAD_PROGRESS_COLOR"
    );
    assert!(
        window.contains("PBM_SETBARCOLOR"),
        "the progress bar must still be given the shared palette's colour"
    );
}

#[test]
fn windows_desktop_leg_is_deliberately_unsigned() {
    // Criterion 6 / the task's "unsigned, deliberately": no code signing and no
    // installer anywhere in this leg (both need a certificate and are a separate
    // follow-on, the Windows analogue of `android-apk-signing`). Pinned as an
    // ABSENCE so a later "just sign it here" edit has to come with the
    // secrets-presence-flag pattern the Android leg established.
    let j = job(WINDOWS_LEG);
    for tool in [
        "signtool",
        "Set-AuthenticodeSignature",
        "New-SelfSignedCertificate",
        "makeappx",
        "candle",
        "light",
    ] {
        assert!(
            !contains_substr(&j, tool),
            "the `{WINDOWS_LEG}` leg must contain NO `{tool}` step (unsigned and uninstallered by \
             design; signing is a follow-on that must copy the Android secrets-presence-flag \
             pattern)"
        );
    }
    // Honest naming, the Android precedent (`app-debug-unsigned.apk`).
    assert!(
        contains_substr(&j, "unsigned"),
        "the `{WINDOWS_LEG}` leg's artifact must be NAMED unsigned, so the Release page never \
         implies a signature it does not carry"
    );
}

#[test]
fn the_windows_leg_mints_no_second_version_source() {
    // Criterion 3. The leg's ONLY version input is the job-level
    // `WERUST_VERSION` the shared loop above already pins, compiled into the exe
    // by `crates/werust-core/build.rs`. Nothing here stamps a version anywhere
    // else: the zip's NAME carries none (so it cannot disagree with the ⋮ menu),
    // and no resource is re-derived in PowerShell.
    //
    // Scoped to the values that could MINT one (`run` code, `with:`/`env:`
    // values) rather than to the whole job, exactly as the Android guard is, so
    // an explanatory comment may still say the words.
    let j = job(WINDOWS_LEG);
    let values = minting_values_of(&j);
    for second_source in ["git describe", "github.run_number", "github.sha"] {
        if let Some(offender) = values.iter().find(|v| v.contains(second_source)) {
            panic!(
                "the `{WINDOWS_LEG}` leg must not derive a version from `{second_source}`: the \
                 version comes from `WERUST_VERSION`, resolved once by werust-core's build.rs, so \
                 the shipped exe and the ⋮ menu cannot disagree. Found it in a run/with/env \
                 VALUE:\n{offender}"
            );
        }
    }
    // And the artifact check PROVES it on the runner: the built exe must contain
    // the exact string the compiled core reports, which is the Windows twin of
    // the macOS `CFBundleVersion` equality check.
    let check = read_repo_file(WINDOWS_ARTIFACT_CHECK);
    assert!(
        check.contains(VERSION_EXAMPLE),
        "{WINDOWS_ARTIFACT_CHECK} must read the expected version from the ONE source by running \
         the `werust-core` `{VERSION_EXAMPLE}` example, exactly as the macOS check does"
    );
}

#[test]
fn readme_states_the_windows_artifact_is_unsigned_needs_webview2_and_names_the_follow_on() {
    // Criterion 6. Two facts a person must meet BEFORE the download, not after:
    // an unsigned exe makes SmartScreen interrupt the first run, and the browser
    // does not render at all without the WebView2 Runtime (`docs/adr/0011`
    // finding 6: "no installer ever needed" is not a promise werust can make).
    let readme = read_repo_file("README.md");
    assert!(
        readme.contains(WINDOWS_ZIP),
        "the README must name the Windows release artifact (`{WINDOWS_ZIP}`)"
    );
    assert!(
        readme.contains("unsigned"),
        "the README must state plainly that the Windows artifact is UNSIGNED"
    );
    assert!(
        readme.contains("SmartScreen"),
        "the README must say what an unsigned download DOES on a real machine (SmartScreen \
         interrupts it), not merely that it is unsigned"
    );
    assert!(
        readme.contains("More info") && readme.contains("Run anyway"),
        "the README must give the actual click-path through the SmartScreen prompt (More info -> \
         Run anyway), or it documents a wall without its door"
    );
    assert!(
        readme.contains("WebView2 Runtime"),
        "the README must name the RUNTIME DEPENDENCY the artifact needs (the Edge WebView2 \
         Runtime)"
    );
    assert!(
        readme.contains("android-apk-signing"),
        "the README must NAME the signing follow-on by pointing at the precedent it will copy \
         (the landed `android-apk-signing` leg)"
    );
}

#[cfg(unix)]
#[test]
fn the_windows_artifact_check_is_executable() {
    // Kept in step with its three siblings, so a human on a unix box can run it
    // by path against a downloaded artifact. The Windows JOB deliberately does
    // not depend on this bit (it invokes the script as an argument to `bash`,
    // because a Windows checkout has no POSIX executable bit to lose).
    use std::os::unix::fs::PermissionsExt;
    let path = repo_root().join(WINDOWS_ARTIFACT_CHECK);
    let mode = std::fs::metadata(&path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "{WINDOWS_ARTIFACT_CHECK} must be executable (the release workflow runs it by path)"
    );
}

// --- Criterion 12: the published BODY is the conventional-commit changelog ----
//
// Task `release-notes-lose-the-conventional-commit-changelog-to-a-race`;
// decisions, the alternatives weighed, and the next-real-tag confirmation
// checklist live in
// `docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/README.md`.
//
// TWO DEFECTS, both of which made a release page LIE about what shipped, and
// NEITHER of which the `workflow_dispatch` dry run can see (GoReleaser's
// `--snapshot` never touches the forge, so no release body is ever written on
// that path; see the spike README's "What the dry run cannot prove"):
//
//   1. A RACE for who CREATES the Release. Every artifact leg is deliberately
//      decoupled (`needs: verify`), so each one guarantees the Release exists
//      before uploading into it. That create used to pass `--generate-notes`,
//      so whichever leg won the race filled the body with GITHUB's auto-notes,
//      and GoReleaser (which by default `keep-existing`s a non-empty body)
//      attached its artifacts and threw away the changelog it had prepared.
//      `v0.3.1` shipped a body that was one compare link; `v0.3.0`, a release
//      that added two desktop platforms, shipped a body naming one PR.
//   2. SCOPE-BLIND exclude filters. The changelog GROUPS were scope-aware
//      (`feat(\(.+\))??!?:`) but the excludes were bare `^chore:` / `^ci:` /
//      `^test:` / `^docs:`, and this repo scopes nearly every housekeeping
//      commit, so over `v0.3.2..main` they matched 0, 0, 0 and 1 of 22
//      respectively, and `v0.3.2` published an `### Others` section full of the
//      exact commits the filters existed to delete.
//
// The assertions below are deliberately SEMANTIC where they can be: rather than
// eyeballing the patterns, `ChangelogRules` COMPILES the ones in the config and
// APPLIES them to real commit subjects, because "the filter looks right" is
// precisely the mistake that shipped defect 2.

/// The four artifact legs that may have to CREATE the Release they upload into.
///
/// They are decoupled from the desktop leg on purpose (`needs: verify`), which
/// is what makes any of them a potential creator, so the body they create with
/// is a property of ALL FOUR, never of whichever one happens to be quickest.
const ARTIFACT_LEGS: [&str; 4] = ["android-apk", "ios-simulator-app", MACOS_LEG, WINDOWS_LEG];

/// The one `gh release create` command line inside a leg.
fn release_create_command(leg: &str) -> String {
    let j = job(leg);
    let found: Vec<String> = strings_of(&j)
        .into_iter()
        .filter(|s| s.contains("gh release create"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "the `{leg}` leg must carry exactly ONE `gh release create` (the Release-EXISTENCE \
         guarantee); found {}",
        found.len()
    );
    found.into_iter().next().expect("one create command")
}

#[test]
fn the_artifact_legs_create_an_empty_bodied_release_never_githubs_auto_notes() {
    // The fix for defect 1, and the reason it is asserted on all four legs at
    // once: the race has no fixed winner, so ONE leg still passing
    // `--generate-notes` is enough to lose the changelog again, in a way only a
    // real tag would reveal.
    for leg in ARTIFACT_LEGS {
        let create = release_create_command(leg);
        assert!(
            !create.contains("--generate-notes"),
            "the `{leg}` leg must NOT create the Release with `--generate-notes`: whichever leg \
             wins the race would fill the body with GitHub's auto-notes, and GoReleaser would \
             then attach its artifacts to a Release whose body is not the conventional-commit \
             changelog. Create it EMPTY and let GoReleaser write the body.\n{create}"
        );
        assert!(
            !create.contains("--notes-from-tag") && !create.contains("--notes-file"),
            "the `{leg}` leg must not source a body from anywhere else either: the body belongs \
             to GoReleaser (`release.mode: replace`).\n{create}"
        );
        assert!(
            create.contains("--notes \"\"") || create.contains("--notes ''"),
            "the `{leg}` leg must create the Release with an EXPLICIT empty body (`--notes \"\"`): \
             `gh release create` needs a body flag to run non-interactively, and an empty one is \
             the placeholder GoReleaser then fills.\n{create}"
        );
        // The create must stay idempotent + non-fatal: it is an existence
        // guarantee, and three of the four legs will lose the race.
        assert!(
            create.contains("|| true"),
            "the `{leg}` leg's create must stay non-fatal (`|| true`): three of the four legs \
             lose the race, and losing it is the normal case, not a failure.\n{create}"
        );
    }
}

#[test]
fn goreleaser_writes_the_body_of_a_release_a_leg_already_created() {
    // The OTHER half of the fix, and the half that is easy to leave out: an
    // empty placeholder body is only enough by accident. GoReleaser's default
    // `mode: keep-existing` returns the EXISTING body whenever it is non-empty
    // (`internal/client/release_notes.go`), so the changelog survives today only
    // because the placeholder happens to be empty, which is exactly the implicit
    // coupling that produced this defect. `replace` states the ownership
    // outright: the body of this repo's releases is GoReleaser's to write.
    let cfg = load_yaml(".goreleaser.yaml");
    let release = cfg
        .get("release")
        .expect(".goreleaser.yaml must declare a `release:` block");
    assert_eq!(
        release.get("mode").and_then(Value::as_str),
        Some("replace"),
        "`release.mode` must be `replace`: GoReleaser defaults to `keep-existing`, which hands the \
         body to whichever artifact leg created the Release first"
    );
}

#[test]
fn a_rerun_of_the_tag_build_replaces_the_assets_its_first_attempt_uploaded() {
    // The recorded sibling defect (evidence + the two runs:
    // `docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/README.md`,
    // "The sibling defect"): `v0.2.9` is red because the re-run hit
    // `422 Validation Failed [{Resource:ReleaseAsset Field:name Code:already_exists}]`
    // on `checksums.txt` and the tarball ITS OWN first attempt had uploaded. The
    // four artifact legs were made idempotent on purpose (`gh release upload
    // --clobber`); the desktop leg is the one that was not, and it is the one
    // that is red. GoReleaser's own knob does the same job: on a 422 it deletes
    // the colliding asset and retries the upload.
    let cfg = load_yaml(".goreleaser.yaml");
    let release = cfg
        .get("release")
        .expect(".goreleaser.yaml must declare a `release:` block");
    assert_eq!(
        release
            .get("replace_existing_artifacts")
            .and_then(Value::as_bool),
        Some(true),
        "`release.replace_existing_artifacts` must be true, or a re-run of a tag build fails with \
         422 already_exists on the assets its own first attempt uploaded (the `v0.2.9` red), while \
         every OTHER leg re-runs cleanly with `gh release upload --clobber`"
    );
}

/// The changelog rules, COMPILED from `.goreleaser.yaml` exactly as GoReleaser
/// compiles them, so a subject can be run through them instead of eyeballed.
///
/// Faithfulness notes, from GoReleaser's own source
/// (`internal/pipe/changelog/changelog.go`):
///
/// * Both the exclude filters and the group regexps are matched against
///   `entry.Message`, the commit SUBJECT alone, with no SHA prefix. (The
///   groups' leading `^.*?` is vestigial; it is kept because a group's shape is
///   the style the excludes now mirror.)
/// * Excludes run FIRST (`filterEntries`), then the surviving entries are
///   assigned to groups in CONFIG order, each group consuming what it matches;
///   a group with no `regexp` takes everything left. `order` only sorts the
///   rendered sections, so config order is what decides membership.
struct ChangelogRules {
    /// (pattern source, compiled). The source is quoted in failure messages so
    /// a red test names the filter that fired.
    exclude: Vec<(String, regex::Regex)>,
    /// (group title, compiled regexp). `None` is the catch-all group.
    groups: Vec<(String, Option<regex::Regex>)>,
}

impl ChangelogRules {
    fn load() -> Self {
        let cfg = load_yaml(".goreleaser.yaml");
        let changelog = cfg
            .get("changelog")
            .expect(".goreleaser.yaml must declare a `changelog:` block");

        let compile = |pattern: &str| {
            regex::Regex::new(pattern).unwrap_or_else(|e| {
                panic!("changelog pattern `{pattern}` is not a valid regexp: {e}")
            })
        };

        let exclude = changelog
            .get("filters")
            .and_then(|f| f.get("exclude"))
            .and_then(Value::as_sequence)
            .expect("`changelog.filters.exclude` must list what never reaches a release page")
            .iter()
            .map(|v| {
                let p = v
                    .as_str()
                    .expect("every `changelog.filters.exclude` entry must be a string");
                (p.to_string(), compile(p))
            })
            .collect();

        let groups = changelog
            .get("groups")
            .and_then(Value::as_sequence)
            .expect("`changelog.groups` must classify conventional-commit types")
            .iter()
            .map(|g| {
                let title = g
                    .get("title")
                    .and_then(Value::as_str)
                    .expect("every changelog group must have a `title`")
                    .to_string();
                let re = g.get("regexp").and_then(Value::as_str).map(compile);
                (title, re)
            })
            .collect();

        Self { exclude, groups }
    }

    /// The exclude PATTERN that removes this subject, if any.
    fn excluded_by(&self, subject: &str) -> Option<&str> {
        self.exclude
            .iter()
            .find(|(_, re)| re.is_match(subject))
            .map(|(src, _)| src.as_str())
    }

    /// Where this subject lands in the published body. `None` when a filter
    /// removes it entirely.
    fn section_of(&self, subject: &str) -> Option<&str> {
        if self.excluded_by(subject).is_some() {
            return None;
        }
        for (title, re) in &self.groups {
            match re {
                Some(re) if re.is_match(subject) => return Some(title),
                None => return Some(title),
                Some(_) => {}
            }
        }
        panic!(
            "no changelog group claims `{subject}`, and there is no catch-all group: it would \
             vanish from the body with no filter having decided to drop it"
        )
    }
}

#[test]
fn the_housekeeping_excludes_are_scope_aware() {
    // Defect 2, stated as the property rather than as the patterns: this repo
    // writes `chore(some-task):`, not `chore:`, so an exclude that only knows
    // the unscoped form is a filter that never fires. Both forms (and the
    // breaking-change `!` the group regexps already tolerate) must drop out,
    // and the unscoped form must KEEP working (it is what `docs:` commits and
    // any hand-typed commit use).
    let rules = ChangelogRules::load();
    for ty in ["chore", "ci", "test", "docs"] {
        for subject in [
            format!("{ty}: an unscoped housekeeping commit"),
            format!("{ty}(some-scope): a scoped housekeeping commit"),
            format!("{ty}(some-scope)!: a scoped breaking housekeeping commit"),
            format!("{ty}!: an unscoped breaking housekeeping commit"),
        ] {
            assert!(
                rules.excluded_by(&subject).is_some(),
                "`{subject}` must be excluded from the changelog: the filters must treat the \
                 SCOPED and unscoped forms of `{ty}` identically, exactly as the GROUP regexps \
                 already do"
            );
        }
    }
}

#[test]
fn no_exclude_filter_can_eat_a_feature_or_a_fix() {
    // The teeth on the OTHER side. The excludes just grew broader; a pattern
    // that also swallowed `feat`/`fix` would empty the release body while every
    // other assertion here stayed green, and only a real tag would show it.
    let rules = ChangelogRules::load();
    for (ty, section) in [("feat", "Features"), ("fix", "Bug fixes")] {
        for subject in [
            format!("{ty}: an unscoped change"),
            format!("{ty}(some-task): a scoped change"),
            format!("{ty}(some-task)!: a scoped breaking change"),
            format!("{ty}!: an unscoped breaking change"),
        ] {
            assert_eq!(
                rules.section_of(&subject),
                Some(section),
                "`{subject}` must survive every filter and land under `{section}`: the excludes \
                 must never reach a user-facing commit"
            );
        }
    }
}

#[test]
fn the_configured_filters_sort_real_history_the_way_the_release_page_should_read() {
    // The corpus is REAL: every subject below is taken from this repo's own
    // history (the `v0.3.2..main` range wherever the shape occurs there, earlier
    // history for the shapes that do not), one per distinct SHAPE. A handful of
    // very long ones are cut at a clause boundary to keep this table readable;
    // the PREFIX, the only part any filter or group regexp reads, is untouched
    // in every entry. The
    // full-range measurement over all 77 commits, with the command that produces
    // it, is recorded in
    // `docs/spikes/release-notes-lose-the-conventional-commit-changelog-to-a-race/README.md`;
    // this test is the part that runs in the `verify` gate on every change.
    //
    // `None` = filtered out, and MUST be: none of these tell a user anything
    // about what the release does.
    const CORPUS: &[(&str, Option<&str>)] = &[
        // --- user-facing: the whole point of the body -----------------------
        (
            "feat(reload-stop-collapse-and-spinner-on-the-windows-chrome): The Windows chrome \
             collapses Reload/Stop into one control and shows the spinner, reading the painted \
             snapshot; done",
            Some("Features"),
        ),
        (
            "fix(chrome): show load progress in the URL bar instead of a displacing banner",
            Some("Bug fixes"),
        ),
        (
            "fix: cargo fmt — wrap the long Infura URL line",
            Some("Bug fixes"),
        ),
        // --- housekeeping conventional types, SCOPED (defect 2) -------------
        (
            "chore(enable-the-ios-back-forward-swipe-gesture): requeue handoff note",
            None,
        ),
        (
            "ci(android-apk): the key alias is a variable, and the signature check records the \
             scheme matrix",
            None,
        ),
        (
            "docs(work): record the Gate-3 verdict for the Windows chrome collapse",
            None,
        ),
        // --- the same types UNSCOPED, which must keep working ---------------
        ("chore: add mobile chrome URL-bar-crowding fix task", None),
        (
            "docs: add werust provider mimic and inspector for dapp testing",
            None,
        ),
        // --- work/-contract bookkeeping: an item moved, not a change --------
        (
            "task: the release notes lose the conventional-commit changelog to a race",
            None,
        ),
        (
            "task(backlog): close the aggregate and tooltip gaps in the shared chrome derivation",
            None,
        ),
        (
            "tasking(ens-to-ipfs-resolution-phase1-rpc-skeleton): werust: ENS name -> IPFS \
             resolution (Phase 1 — the EthereumProvider seam + trusted-RPC skeleton); tasked",
            None,
        ),
        (
            "notes(gate-3): provider refuses honestly (APPROVE after requeue) — and it corrected \
             me twice",
            None,
        ),
        (
            "obs(field): v0.2.1 manual test surfaced 4 real render/UX issues (main-thread freeze, \
             broken styling, invisible IPNS error, black mobile page)",
            None,
        ),
        (
            "findings: werust ran on REAL Windows hardware, and the sweep found three defects in \
             five minutes",
            None,
        ),
        (
            "spec(proposed): signed-multi-platform-builds — Android signed APK, macOS desktop \
             (signed + notarized), Windows deferred (GTK blocker)",
            None,
        ),
        (
            "review(spec): in-app-debug-menu — fix covers map (all were [2]; now \
             store[5,6]/menu[2]/capture[4,5]/views[1,3]), APPROVE, move proposed->tasked",
            None,
        ),
        (
            "task+notes: ipfs-site-mobile-black-page closed by v0.2.7 origin-map fix — \
             mandalas.eth renders on mobile per human re-test",
            None,
        ),
        (
            "findings+task: window.localStorage is null on Android — DOM storage was never enabled",
            None,
        ),
        // --- dorfl runner lifecycle: pure bookkeeping pushed to main --------
        (
            "surface task:macos-smoke-blur-url-bar-does-not-end-the-field-editor (stuck): The \
             task's diagnosis is incomplete at a load-bearing point, and acceptance criterion 3 as \
             written is unachievable by the sanctioned fix.",
            None,
        ),
        ("dorfl sync", None),
        // --- and the pre-existing merge filter, still doing its job ---------
        ("Merge branch 'main' of github.com:wighawag/werust", None),
    ];

    let rules = ChangelogRules::load();
    for (subject, expected) in CORPUS {
        let got = rules.section_of(subject);
        assert_eq!(
            got,
            *expected,
            "the changelog puts this subject in {got:?}, expected {expected:?}:\n  {subject}\n\
             (excluded by: {:?})",
            rules.excluded_by(subject)
        );
    }
}
