//! The RECORDING guard for the Windows origin verdict: the Ubuntu `verify`
//! gate's copy of the check only a `windows-latest` runner would otherwise make.
//!
//! WHY THIS EXISTS: `docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json`
//! is the PINNED baseline every later `windows-origin-probe` run is asserted
//! against, and ADR-0011's verdict (the Windows shell serves real `ipfs://`
//! origins, `origin_map.rs` stays an Android module) rests on it. The failure
//! mode this file exists to catch is the one that has actually happened in this
//! repo, twice: a plausible, carefully argued PREDICTION written from Linux and
//! committed in the slot a MEASUREMENT belongs in. Nothing about the file's
//! shape distinguishes the two. So this test demands the evidence -- the
//! verbatim run report committed next to it -- and replays the probe's own
//! [`Expectations::diff`] against it on every ordinary `cargo test`.
//!
//! Both files were already committed; nothing compared them, so an edit to
//! either could silently make the pinned verdict and its evidence disagree and
//! only a Windows runner would notice. That is the drift this closes.
//!
//! What it therefore pins:
//!
//! * the pinned baseline AGREES, field for field, with the committed run report
//!   (so re-stamping `expected.json` by hand, in either direction, reds here),
//! * that report is a real run's output (it carries the evergreen WebView2
//!   runtime version and the CID the run actually served), and
//! * the provenance names the runtime and runner it was measured on, and points
//!   at the verbatim report, so a reader can go and look.
//!
//! This is the Windows twin of `crates/macos-origin-probe/tests/recorded_verdict.rs`,
//! kept deliberately symmetrical with it, with two differences that follow from
//! the two probes' shapes:
//!
//! * the engine identity a Windows run measures is the WebView2 runtime version
//!   (there is no user-agent field in [`Report`]), and the provenance names the
//!   runner IMAGE and that runtime rather than an `actions/runs/` URL, because
//!   that is what the committed `recorded` line says -- this test asserts the
//!   evidence that exists, it does not edit the baseline to suit itself;
//! * the mechanism check and the negative-control falsification guard are
//!   already INSIDE this probe's [`Expectations::diff`], so replaying the diff
//!   covers here what the macOS file needs separate tests for.
//!
//! It can never replace the `windows-origin-probe` job: it proves the recorded
//! verdict was MEASURED, not that the evergreen runtime still behaves that way
//! today. That is the job's half, and it is why the probe stays re-runnable.

use std::path::{Path, PathBuf};

use windows_origin_probe::facts::{Expectations, Report};
use windows_origin_probe::page::CID;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spike(file: &str) -> String {
    let path = repo_root()
        .join("docs/spikes/windows-ipfs-origin-probe-on-ci")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The PINNED baseline: what the `windows-origin-probe` workflow asserts every
/// re-run against.
const BASELINE: &str = "expected.json";

/// The RECORDED RUN the baseline is stamped from, named for the day it was
/// measured.
const RECORDED_RUN: &str = "probe-report-2026-07-30.json";

fn baseline() -> Expectations {
    serde_json::from_str(&spike(BASELINE)).unwrap_or_else(|e| panic!("{BASELINE} must parse: {e}"))
}

fn recorded_run() -> Report {
    serde_json::from_str(&spike(RECORDED_RUN))
        .unwrap_or_else(|e| panic!("{RECORDED_RUN} must be a verbatim probe report: {e}"))
}

/// The tooth. `expected.json` must be a RECORDING of the committed run, not a
/// hand-written guess about what a Windows box would say: the probe's own diff,
/// replayed here on a host that has never seen WebView2, must come back clean.
#[test]
fn the_recorded_verdict_is_the_committed_run_not_a_prediction() {
    let diff = baseline().diff(&recorded_run());
    assert_eq!(
        diff,
        Vec::<String>::new(),
        "the two committed evidence files disagree. {BASELINE} is the PINNED BASELINE (what the \
         `windows-origin-probe` workflow asserts every re-run against) and {RECORDED_RUN} is the \
         RECORDED RUN (the verbatim report the verdict was stamped from). Each reported line \
         names the field that moved: `expected` is the BASELINE's value, `got` is the RECORDED \
         RUN's. Re-stamp the baseline FROM the report, or commit the newer report it was stamped \
         from -- never edit the recorded verdict by hand"
    );
}

/// ...and the report it is stamped from must itself be a real run: a fabricated
/// one would have to invent an evergreen runtime version and serve a real CID,
/// which the provenance line then has to name.
#[test]
fn the_provenance_names_the_runtime_and_the_report_it_was_stamped_from() {
    let run = recorded_run();
    let recorded = &baseline().recorded;

    assert!(
        !run.webview2_runtime_version.trim().is_empty() && !run.cid.trim().is_empty(),
        "a report without the WebView2 runtime version and the CID it served is not a run"
    );
    assert_eq!(
        run.cid, CID,
        "the run must have served the probe's own fixture CID, not some other document"
    );
    assert!(
        recorded.contains(&run.webview2_runtime_version),
        "the provenance must name the WebView2 runtime the run measured ({}), which is what \
         actually decides this behaviour and is EVERGREEN, so a re-stamp cannot silently keep an \
         older runtime's line: {recorded}",
        run.webview2_runtime_version
    );
    assert!(
        recorded.contains("windows-latest"),
        "the provenance must name the runner the measurement came from, so a reader can go and \
         re-run it: {recorded}"
    );
    assert!(
        recorded.contains(RECORDED_RUN),
        "the provenance must point at the verbatim report committed beside it: {recorded}"
    );
}
