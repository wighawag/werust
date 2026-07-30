//! `windows-renderer` CI-leg shape guard (task `windows-renderer-ci-leg`,
//! `docs/adr/0011-webview2-for-windows.md`, sub-task 1 of the Windows shell
//! split).
//!
//! WHY THIS GUARD EXISTS AT ALL. `.github/workflows/windows-renderer.yml` lands
//! BEFORE any Windows shell code for a MECHANICAL reason: `gh workflow run <wf>
//! --ref <branch>` is only legal once `<wf>` exists on the DEFAULT branch, so a
//! build agent can never dispatch a leg it is itself inventing. That is exactly
//! what left both macOS tasks shipping a PREDICTION where an acceptance
//! criterion demanded a MEASUREMENT. Landing the leg first is what makes the two
//! Windows code tasks (`windows-webview2-renderer-backend`,
//! `windows-win32-window-and-chrome`) measurable at all.
//!
//! The load-bearing consequence: `workflow_dispatch` is not a convenience here,
//! it IS the deliverable. A future edit that quietly drops it would re-open the
//! prediction trap with nothing going red. This file is what goes red.
//!
//! Same seam and same style as the sibling `release_plumbing_shape.rs`: the
//! deliverable is a declarative file whose SHAPE is the contract, so the
//! objective bar is "does this file declare the leg the acceptance criteria
//! name", asserted by PARSING it (not string-grepping) inside the pure-Rust
//! Ubuntu `verify` gate — `serde_yaml` is already a dev-dependency, so no new
//! dependency enters the tree. `werust-core` hosts it for the sibling's reason:
//! it is toolkit-free, so the assertion rides every `cargo test`.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. The leg exists, runs on `windows-latest`, and offers `workflow_dispatch`
//!    (`the_leg_exists_and_runs_on_a_windows_latest_runner`,
//!    `the_leg_offers_workflow_dispatch_the_dispatch_by_ref_entry_point`).
//! 2. The leg is GREEN as landed: it builds AND tests exactly the crate set
//!    measured to compile on `x86_64-pc-windows-msvc`, and never reaches for the
//!    GTK/WebKitGTK-bound crates that cannot
//!    (`the_leg_builds_and_tests_the_measured_green_crate_set`,
//!    `the_leg_never_builds_the_gtk_bound_crates`). The measurement, the method
//!    and the named exclusions:
//!    `docs/spikes/windows-renderer-ci-leg/README.md`.
//! 3. The WebView2 Runtime version is recorded through ONE shared
//!    implementation, reused by both Windows legs rather than copied
//!    (`both_windows_legs_read_the_runtime_version_through_one_shared_action`,
//!    `the_registry_read_exists_in_exactly_one_place`).
//! 4. The `pull_request` filter is the NARROW one, and the coverage it gives up
//!    is really picked up by the `push` filter
//!    (`the_pull_request_filter_stays_narrow_and_push_carries_the_rest`).

use std::path::{Path, PathBuf};

use serde_yaml::Value;

/// The leg this task lands.
const LEG: &str = ".github/workflows/windows-renderer.yml";

/// The sibling Windows leg (gate 0, task `windows-ipfs-origin-probe-on-ci`),
/// whose registry-read step is the one this leg must REUSE.
const PROBE_LEG: &str = ".github/workflows/windows-origin-probe.yml";

/// The composite action holding the ONE registry read.
const VERSION_ACTION_DIR: &str = ".github/actions/webview2-runtime-version";

/// How a workflow step references that action (a repo-local composite action).
const VERSION_ACTION_USES: &str = "./.github/actions/webview2-runtime-version";

/// The EdgeUpdate client GUID that IDENTIFIES the WebView2 Runtime in the
/// registry — the fingerprint of the registry read itself, used here to prove
/// there is exactly ONE implementation of it in the repo.
const WEBVIEW2_CLIENT_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// The crate set measured green on `x86_64-pc-windows-msvc` (see the spike
/// README for the method and the named exclusions). `webview-shared` leads it:
/// it holds `LoadLifecycle`/`SharedLifecycle`, the `navigate` URL rule and the
/// ADR-0008 off-thread `ipfs://` boundary the Windows backend reuses verbatim.
///
/// `windows-renderer` joined it with task `windows-webview2-renderer-backend`:
/// the `#[cfg(windows)]` WebView2 backend itself, which is the whole reason this
/// leg landed ahead of the shell code. `werust-windows` (the Win32 WINDOW over
/// that backend) and `desktop-paint` (the shared painter half both native desktop
/// windows consume) joined it with task `windows-win32-window-and-chrome`.
/// Extending this list is DELIBERATELY not a one-line workflow edit — the
/// assertions below hold the build steps, the test steps and the `push` path
/// filter to this ONE list, so a crate cannot join the leg without also joining
/// the filter that re-runs it.
const GREEN_ON_WINDOWS: &[&str] = &[
    "windows-renderer",
    "werust-windows",
    "desktop-paint",
    "webview-shared",
    "renderer",
    "werust-core",
    "fetcher",
    "windows-origin-probe",
];

/// The crates that CANNOT build on Windows: they bind GTK/WebKitGTK through
/// pkg-config, which a `windows-latest` runner has nothing to satisfy. Measured,
/// not assumed (the spike README records the failure).
const RED_ON_WINDOWS: &[&str] = &["werust", "webview-renderer"];

/// Read a repo file. `CARGO_MANIFEST_DIR` is `crates/werust-core`, so the root
/// is two levels up.
fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn load_yaml(relative: &str) -> Value {
    serde_yaml::from_str(&read(relative))
        .unwrap_or_else(|e| panic!("parse {relative} as YAML: {e}"))
}

/// Every scalar string reachable from a YAML value, flattened — lets an
/// assertion talk about "somewhere in this job's steps" without pinning the step
/// layout.
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

/// The workflow's `on:` mapping. YAML 1.1 reads a bare `on` as the boolean
/// `true`, so accept either key (the sibling `release_plumbing_shape.rs` does
/// the same).
fn triggers(wf: &Value) -> serde_yaml::Mapping {
    wf.get("on")
        .or_else(|| wf.get(Value::Bool(true)))
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_else(|| panic!("{LEG} must declare an `on:` trigger mapping"))
}

fn jobs(wf: &Value) -> serde_yaml::Mapping {
    wf.get("jobs")
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_else(|| panic!("{LEG} must declare `jobs:`"))
}

/// The `paths:` list of one trigger, as plain strings.
fn trigger_paths(wf: &Value, trigger: &str) -> Vec<String> {
    let on = triggers(wf);
    let t = on
        .get(Value::String(trigger.to_string()))
        .unwrap_or_else(|| panic!("{LEG} must declare an `on.{trigger}:` trigger"));
    t.get("paths")
        .and_then(Value::as_sequence)
        .unwrap_or_else(|| panic!("{LEG}'s `{trigger}` trigger must carry a `paths:` filter"))
        .iter()
        .filter_map(|p| p.as_str().map(str::to_string))
        .collect()
}

/// Every `-p <name>` / `--package <name>` / `--package=<name>` selector passed
/// to a cargo invocation matching `command`, anywhere in the workflow.
///
/// Token-based rather than substring-based on purpose: `-p webview-shared` must
/// never be mistaken for `-p webview-renderer`, which is the exact confusion the
/// green-versus-red distinction turns on.
fn cargo_package_selectors(wf: &Value, command: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in strings_of(wf) {
        if !line.contains(command) {
            continue;
        }
        let mut tokens = line.split_whitespace();
        while let Some(token) = tokens.next() {
            match token {
                "-p" | "--package" => {
                    if let Some(pkg) = tokens.next() {
                        out.push(pkg.to_string());
                    }
                }
                _ => {
                    if let Some(pkg) = token.strip_prefix("--package=") {
                        out.push(pkg.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Every `uses:` reference in a workflow's steps.
fn uses_refs(wf: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (_, job) in jobs(wf) {
        let Some(steps) = job.get("steps").and_then(Value::as_sequence) else {
            continue;
        };
        for step in steps {
            if let Some(u) = step.get("uses").and_then(Value::as_str) {
                out.push(u.to_string());
            }
        }
    }
    out
}

// --- Criterion 1: the leg exists, on windows-latest, dispatchable by ref ---

#[test]
fn the_leg_exists_and_runs_on_a_windows_latest_runner() {
    let wf = load_yaml(LEG);
    let runners: Vec<String> = jobs(&wf)
        .iter()
        .filter_map(|(_, j)| j.get("runs-on").and_then(Value::as_str).map(str::to_string))
        .collect();
    assert!(
        runners.iter().any(|r| r == "windows-latest"),
        "{LEG} must run a job on `windows-latest` — a Linux gate can never compile for \
         `x86_64-pc-windows-msvc` nor see a WebView2 Runtime; got {runners:?}"
    );
}

#[test]
fn the_leg_offers_workflow_dispatch_the_dispatch_by_ref_entry_point() {
    // The load-bearing one. WITHOUT `workflow_dispatch` on the default branch,
    // `gh workflow run windows-renderer.yml --ref work/task-...` is illegal, and
    // the two Windows code tasks are back to PREDICTING what a Windows runner
    // would do — the precise failure that bounced both macOS tasks at Gate 2.
    let wf = load_yaml(LEG);
    assert!(
        triggers(&wf).contains_key(Value::String("workflow_dispatch".into())),
        "{LEG} must offer `workflow_dispatch`: dispatch-by-ref is the ENTIRE reason this leg \
         lands ahead of the Windows shell code, so a future edit must not drop it silently"
    );
}

// --- Criterion 2: green as landed — the measured crate set, and only it ---

#[test]
fn the_leg_builds_and_tests_the_measured_green_crate_set() {
    let wf = load_yaml(LEG);
    for command in ["cargo build", "cargo test"] {
        let selected = cargo_package_selectors(&wf, command);
        for pkg in GREEN_ON_WINDOWS {
            assert!(
                selected.iter().any(|s| s == pkg),
                "`{command}` on the Windows leg must select `-p {pkg}` (measured green on \
                 x86_64-pc-windows-msvc; see docs/spikes/windows-renderer-ci-leg/README.md); \
                 got {selected:?}"
            );
        }
    }
}

#[test]
fn the_leg_never_builds_the_gtk_bound_crates() {
    // A leg that is red on arrival teaches nothing. `werust` and
    // `webview-renderer` bind gtk4/webkit6 through pkg-config; there is no .pc
    // file on a Windows runner to satisfy, so pulling either in turns this leg
    // red for a reason that has nothing to do with Windows.
    let wf = load_yaml(LEG);
    let selected = cargo_package_selectors(&wf, "cargo");
    for pkg in RED_ON_WINDOWS {
        assert!(
            !selected.iter().any(|s| s == pkg),
            "the Windows leg must NOT select `-p {pkg}`: it binds GTK/WebKitGTK via pkg-config \
             and cannot build on a Windows runner; got {selected:?}"
        );
    }
    // And never the bare workspace build, which would sweep them in.
    for line in strings_of(&wf) {
        for command in ["cargo build", "cargo test", "cargo clippy"] {
            if let Some(rest) = line.split(command).nth(1) {
                assert!(
                    rest.contains("-p ") || rest.contains("--package"),
                    "`{command}` on the Windows leg must be package-SCOPED (the workspace \
                     contains GTK-bound crates that cannot build here); got {line:?}"
                );
            }
        }
    }
}

// --- Criterion 3: one runtime-version implementation, reused by both legs ---

#[test]
fn both_windows_legs_read_the_runtime_version_through_one_shared_action() {
    // A platform result without its platform version is not a result — and the
    // WebView2 Runtime is EVERGREEN, so the version is the only thing that dates
    // a measurement. The registry read already existed in the gate-0 probe leg;
    // this leg REUSES it as a composite action rather than minting a second
    // implementation that could drift to a different key.
    let action = load_yaml(&format!("{VERSION_ACTION_DIR}/action.yml"));
    assert_eq!(
        action
            .get("runs")
            .and_then(|r| r.get("using"))
            .and_then(Value::as_str),
        Some("composite"),
        "{VERSION_ACTION_DIR}/action.yml must be a composite action so BOTH Windows legs can \
         `uses:` the one implementation"
    );
    assert!(
        strings_of(&action)
            .iter()
            .any(|s| s.contains(WEBVIEW2_CLIENT_GUID)),
        "the shared action must be the one that actually reads the WebView2 Runtime's EdgeUpdate \
         client key ({WEBVIEW2_CLIENT_GUID})"
    );

    for leg in [LEG, PROBE_LEG] {
        let wf = load_yaml(leg);
        assert!(
            uses_refs(&wf).iter().any(|u| u == VERSION_ACTION_USES),
            "{leg} must record the runtime version via `uses: {VERSION_ACTION_USES}` (the shared \
             step), not its own copy"
        );
    }
}

#[test]
fn the_registry_read_exists_in_exactly_one_place() {
    // The anti-duplication half of the same criterion, asserted on the raw TEXT:
    // if either workflow still spells the registry key out, there are two
    // implementations again and they can drift.
    for leg in [LEG, PROBE_LEG] {
        assert!(
            !read(leg).contains(WEBVIEW2_CLIENT_GUID),
            "{leg} must not spell out the WebView2 registry key itself — that read lives ONCE, in \
             {VERSION_ACTION_DIR}/action.yml"
        );
    }
}

// --- Criterion 4: the narrow PR filter, with push carrying what it gives up ---

#[test]
fn the_pull_request_filter_stays_narrow_and_push_carries_the_rest() {
    // The DELIBERATE trade-off (stated in the workflow header, recorded in
    // docs/spikes/windows-renderer-ci-leg/README.md): the sibling
    // `macos-renderer.yml` triggers on PRs touching `crates/werust-core/**`, so
    // most core work now spends `macos-14` minutes and can be gated by a red
    // cross-platform leg — a cost the human has flagged. This leg does NOT copy
    // that. Its PR filter carries only what is genuinely Windows-shaped; the
    // wider dependency surface is caught on `push` to `main` (early detection,
    // no PR gating) and on demand via `workflow_dispatch`.
    //
    // This assertion is the guard against reflexive BROADENING: widening the PR
    // filter is a real decision, and it should have to change this test and the
    // header comment together.
    let wf = load_yaml(LEG);
    let pr = trigger_paths(&wf, "pull_request");
    for narrow in [
        "crates/webview-shared/**",
        "crates/windows-origin-probe/**",
        // The Windows SHELL is as Windows-shaped as its engine: a PR that
        // touches it should be gated on the leg that is the only place its Win32
        // half compiles at all.
        "crates/werust-windows/**",
        ".github/workflows/windows-renderer.yml",
    ] {
        assert!(
            pr.iter().any(|p| p == narrow),
            "the `pull_request` filter must still catch a Windows-shaped change: missing \
             {narrow:?}; got {pr:?}"
        );
    }
    assert!(
        !pr.iter().any(|p| p == "crates/werust-core/**"),
        "the `pull_request` filter must NOT include `crates/werust-core/**`: that is the exact \
         macOS-leg cost under review (every core PR spending Windows minutes and gateable by a \
         red cross-platform leg). It is covered by the `push` filter and by workflow_dispatch; \
         got {pr:?}"
    );

    // What the PR filter gives up must really be picked up post-merge, or the
    // narrowness is a hole rather than a trade.
    let push = trigger_paths(&wf, "push");
    for pkg in GREEN_ON_WINDOWS {
        // Every crate in the set is a `crates/<package-name>` directory.
        let dir = format!("crates/{pkg}/**");
        assert!(
            push.iter().any(|p| *p == dir),
            "the `push` filter must cover every crate the leg builds, since the PR filter \
             deliberately does not: missing {dir:?}; got {push:?}"
        );
    }
}
