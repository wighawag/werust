//! The RECORDING guard for the macOS origin verdict (task
//! `macos-wkwebview-renderer-backend`, acceptance criterion 5).
//!
//! WHY THIS EXISTS: `expected.json` is what every later `macos-renderer` run is
//! asserted against, so it is load-bearing for two WebKit shells at once (macOS
//! and iOS share `WKURLSchemeHandler`). The failure mode this file exists to
//! catch is the one that actually happened here: a plausible, carefully argued
//! PREDICTION written from Linux and committed in the slot a MEASUREMENT belongs
//! in. Nothing about that file's shape distinguishes the two, so this test
//! demands the evidence -- the verbatim run report, committed next to it -- and
//! replays the probe's own comparison against it on the Ubuntu gate.
//!
//! What it therefore pins, on every ordinary `cargo test`:
//!
//! * the recorded verdict AGREES, field for field, with a committed run report
//!   (so re-stamping `expected.json` by hand, in either direction, reds here),
//! * that report is a real run's output (it carries the OS build and the WebKit
//!   user agent the run measured, and the recorded provenance names both), and
//! * the provenance names the CI run it came from, so a reader can go and look.
//!
//! It can never replace the `macos-14` job: it proves the recorded verdict was
//! MEASURED, not that WebKit still behaves that way today. That is the job's
//! half, and it is why the probe stays re-runnable.

use std::path::{Path, PathBuf};

use macos_origin_probe::facts::{verdict_from, Expectations, Mechanism, Report};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spike(file: &str) -> String {
    let path = repo_root()
        .join("docs/spikes/macos-wkwebview-renderer-backend")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The verbatim report the recorded verdict is stamped from. Named for the day
/// it was measured, exactly like `windows-ipfs-origin-probe-on-ci`.
const RECORDED_RUN: &str = "probe-report-2026-07-30.json";

fn expectations() -> Expectations {
    serde_json::from_str(&spike("expected.json")).expect("expected.json must parse")
}

fn recorded_run() -> Report {
    serde_json::from_str(&spike(RECORDED_RUN))
        .unwrap_or_else(|e| panic!("{RECORDED_RUN} must be a verbatim probe report: {e}"))
}

/// The tooth. `expected.json` must be a RECORDING of the committed run, not a
/// hand-written guess about what a Mac would say: the probe's own diff, replayed
/// here, must come back clean.
#[test]
fn the_recorded_verdict_is_the_committed_run_not_a_prediction() {
    let diff = expectations().diff(&recorded_run());
    assert_eq!(
        diff,
        Vec::<String>::new(),
        "`expected.json` disagrees with the run it claims to record ({RECORDED_RUN}). Re-stamp it \
         FROM the report, or commit the newer report it was stamped from -- never edit the \
         recorded verdict by hand"
    );
}

/// ...and the report it is stamped from must itself be a real run: a fabricated
/// one would have to invent an OS build and a WebKit user agent, which the
/// provenance line then has to name.
#[test]
fn the_provenance_names_the_ci_run_and_the_build_it_was_measured_on() {
    let expectations = expectations();
    let run = recorded_run();
    let recorded = &expectations.recorded;

    assert!(
        !run.os_version.trim().is_empty() && !run.webkit_user_agent.contains('…'),
        "a report without the OS build and the real WebKit user agent it measured is not a run"
    );
    assert!(
        recorded.contains(&run.os_version),
        "the provenance must name the OS build the run measured ({}), so a re-stamp cannot \
         silently keep an older machine's line: {recorded}",
        run.os_version
    );
    assert!(
        recorded.contains("AppleWebKit/605.1.15"),
        "the provenance must name the WebKit build, which is what actually decides this \
         behaviour: {recorded}"
    );
    assert!(
        recorded.contains("actions/runs/"),
        "the provenance must link the CI run a reader can go and check: {recorded}"
    );
    assert!(
        recorded.contains(RECORDED_RUN),
        "the provenance must point at the verbatim report committed beside it: {recorded}"
    );
}

/// The verdict the recorded run supports, derived rather than asserted: it is the
/// same rule the `macos-14` job applies, so the recorded mechanism cannot drift
/// away from the evidence.
#[test]
fn the_recorded_run_derives_the_recorded_mechanism() {
    let run = recorded_run();
    assert_eq!(
        verdict_from(&run),
        Ok(Mechanism::RegisteredIpfsScheme),
        "the committed run must be the one that measured a real `ipfs://` tuple origin"
    );
    assert_eq!(expectations().mechanism, Mechanism::RegisteredIpfsScheme);
    // And the reason there is no case B is MEASURED in that same run, not read
    // out of Apple's documentation.
    assert!(
        run.https_is_handled_natively,
        "the run must record the measured `+[WKWebView handlesURLScheme:@\"https\"]`, which is \
         why WebKit has no internal-https fallback"
    );
}

/// The negative control is the reason case A passing means anything, so the
/// COMMITTED run must contain a control that genuinely failed -- the Android
/// failure shape, reproduced on WebKit.
#[test]
fn the_committed_run_carries_a_control_that_really_failed() {
    let run = recorded_run();
    let control = &run.case_control;
    assert_eq!(control.origin, "null", "the control must be opaque-origin");
    assert!(
        control.fetch.starts_with("reject:") && !control.fetch_handler_fired,
        "the control's same-origin fetch must have died before the handler, as it did on Android"
    );
    assert!(
        control.push_state.starts_with("throw:"),
        "the control's pushState must throw, as it did on Android"
    );
    assert!(
        run.case_a.handler_uris.len() > control.handler_uris.len(),
        "case A must have reached the handler for subresources the control never did"
    );
}
