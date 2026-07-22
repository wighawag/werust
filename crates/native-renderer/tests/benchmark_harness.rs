//! The native-renderer benchmark harness — end-to-end acceptance evidence.
//!
//! This is the acceptance test for task
//! `native-renderer-benchmark-harness-capability-and-trust-hooks` (spec stories 20 +
//! 21, `docs/conformance-tiers.md`, the exploration spec
//! `rust-successor-native-renderer-architecture-benchmark`). It RUNS the whole
//! harness against the pinned conformance ladder and asserts it emits ONE
//! structured, comparable, reproducible report the exploration spec can decide the
//! native-renderer architecture from — WITHOUT deciding it here.
//!
//! What it proves, criterion by criterion:
//!
//! 1. The harness scores a candidate on the pinned page checklist + WPT subsets
//!    (capability) AND on the trust-hook qualification (pass/fail).
//! 2. It records a comparable vs-wezig meter (capability + effort/code-volume/
//!    friction signals) on the shared ladder.
//! 3. Its output is a structured, comparable report over the three candidate
//!    architectures (own-engine vs Servo vs Blitz/Stylo assembly).
//! 4. It is re-runnable and reproducible: two runs are byte-equal.
//!
//! It runs hermetically under `verify` (`cargo test`, offline): the page checklist
//! renders committed T1 snapshots through the native path via `data:` URLs (no
//! fetch), the WPT subsets are pinned local fixtures, the trust-hook gate is a pure
//! function of the backend's declared hooks, and the vs-wezig arm signals are a
//! pinned fixture. See `tests/fixtures/benchmark/SOURCE.md`.

use std::path::{Path, PathBuf};

use native_renderer::benchmark::{
    declared_candidate, score_measured_candidate, ArmSignals, BenchmarkReport, ChecklistPage,
    VsWezigMeter, CORE_CSS_THRESHOLD, TREE_CONSTRUCTION_THRESHOLD,
};
use native_renderer::{Candidate, CandidateScoring};

/// The pinned T1 server-floor pages the harness scores capability against (reused
/// from the T1 floor task's committed snapshots — one source of truth).
const CHECKLIST_PAGES: &[&str] = &["article", "blog-post"];

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Load the pinned T1 page snapshots as the harness's page checklist.
fn checklist_pages() -> Vec<ChecklistPage> {
    let dir = fixtures_root().join("t1-server-floor");
    CHECKLIST_PAGES
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.html"));
            let html = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read checklist page {}: {e}", path.display()));
            ChecklistPage {
                name: (*name).to_string(),
                html,
            }
        })
        .collect()
}

/// Read the pinned vs-wezig arm signals fixture into a [`VsWezigMeter`].
fn vs_wezig_meter(capability_fraction: f64) -> VsWezigMeter {
    let path = fixtures_root().join("benchmark/vs-wezig.txt");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read vs-wezig fixture {}: {e}", path.display()));
    let mut fields = std::collections::HashMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("malformed vs-wezig line: {line}"));
        fields.insert(k.trim().to_string(), v.trim().to_string());
    }
    let int = |key: &str| -> u32 {
        fields
            .get(key)
            .unwrap_or_else(|| panic!("missing vs-wezig field {key}"))
            .parse()
            .unwrap_or_else(|e| panic!("vs-wezig field {key} not an integer: {e}"))
    };
    VsWezigMeter {
        tier: fields.get("tier").cloned().unwrap_or_else(|| "T1".into()),
        capability_fraction,
        rust: ArmSignals {
            effort_person_days: int("rust.effort_person_days"),
            code_volume_loc: int("rust.code_volume_loc"),
            dom_object_graph_friction: int("rust.dom_friction"),
        },
        wezig: ArmSignals {
            effort_person_days: int("wezig.effort_person_days"),
            code_volume_loc: int("wezig.code_volume_loc"),
            dom_object_graph_friction: int("wezig.dom_friction"),
        },
    }
}

/// Assemble the whole benchmark report exactly as a real run would: the assembled
/// pure-Rust stack (Blitz/Stylo-assembly class) is the MEASURED candidate, own-engine
/// and Servo are DECLARED slots the exploration fills in.
fn run_harness() -> BenchmarkReport {
    let pages = checklist_pages();
    let wpt_dir = fixtures_root().join("t1-wpt");

    // Score the measured candidate first to know its capability fraction, then pin
    // that into the vs-wezig meter so the meter reports the capability the friction
    // bought.
    let measured_pages = native_renderer::benchmark::score_page_checklist(&pages);
    let rendered = measured_pages.iter().filter(|p| p.rendered).count();
    let fraction = rendered as f64 / measured_pages.len() as f64;

    let measured = score_measured_candidate(
        Candidate::BlitzStyloAssembly,
        &pages,
        &wpt_dir,
        vs_wezig_meter(fraction),
    );

    BenchmarkReport {
        candidates: vec![
            declared_candidate(Candidate::OwnEngine, "T1"),
            declared_candidate(Candidate::Servo, "T1"),
            measured,
        ],
    }
}

#[test]
fn scores_capability_on_page_checklist_and_wpt_subsets_and_trust_hooks_pass_fail() {
    // Criterion 1: the measured candidate is scored on the pinned page checklist +
    // WPT subsets (capability) AND on the trust-hook qualification (pass/fail).
    let report = run_harness();
    let row = report
        .row(Candidate::BlitzStyloAssembly)
        .expect("the measured candidate is in the report");
    assert_eq!(row.scoring, CandidateScoring::Measured);

    // Page checklist: every pinned T1 page rendered through the native path.
    assert_eq!(row.capability.pages.len(), CHECKLIST_PAGES.len());
    assert!(
        row.capability.all_pages_rendered(),
        "every pinned T1 page rendered: {:?}",
        row.capability.pages
    );

    // WPT subsets: real pass-rates in range, meeting the T1 bars.
    assert!((0.0..=1.0).contains(&row.capability.tree_construction_rate));
    assert!((0.0..=1.0).contains(&row.capability.core_css_rate));
    assert!(
        row.capability.tree_construction_meets_bar(),
        "tree-construction {:.3} >= {:.2}",
        row.capability.tree_construction_rate,
        TREE_CONSTRUCTION_THRESHOLD
    );
    assert!(
        row.capability.core_css_meets_bar(),
        "core-CSS {:.3} >= {:.2}",
        row.capability.core_css_rate,
        CORE_CSS_THRESHOLD
    );
    assert!(row.capability.meets_t1_capability());

    // Trust-hook axis is a PASS/FAIL qualification (not a graded score): the T0/T1
    // native backend honestly declares no trust hook yet, so it does NOT qualify and
    // names BOTH missing hooks. This is the reused `renderer::qualify` gate.
    assert!(
        !row.trust_hooks.qualifies,
        "the assembled native backend does not yet wire the trust hooks"
    );
    assert_eq!(
        row.trust_hooks.missing.len(),
        2,
        "both trust hooks named as missing: {:?}",
        row.trust_hooks.missing
    );
}

#[test]
fn records_a_comparable_vs_wezig_meter_on_the_shared_ladder() {
    // Criterion 2: a comparable vs-wezig meter (capability + effort/code-volume/
    // friction signals) on the shared conformance ladder.
    let report = run_harness();
    let row = report.row(Candidate::BlitzStyloAssembly).unwrap();
    let meter = &row.vs_wezig;

    assert_eq!(meter.tier, "T1", "measured on the shared ladder rung");
    // The capability fraction the friction bought is recorded and in range.
    assert!((0.0..=1.0).contains(&meter.capability_fraction));
    assert_eq!(
        meter.capability_fraction, 1.0,
        "both pinned pages rendered, so full capability at this rung"
    );

    // Both arms carry all three comparable signals; the DOM-friction delta is
    // surfaced as one number (the experiment's central axis).
    assert!(meter.rust.effort_person_days > 0 && meter.wezig.effort_person_days > 0);
    assert!(meter.rust.code_volume_loc > 0 && meter.wezig.code_volume_loc > 0);
    // Recorded evidence: the delta is Rust − wezig; here Rust carries more DOM
    // object-graph friction (the honestly-recorded "does Rust drown?" signal).
    assert_eq!(
        meter.dom_friction_delta(),
        meter.rust.dom_object_graph_friction as i64 - meter.wezig.dom_object_graph_friction as i64
    );
}

#[test]
fn output_is_a_structured_comparable_report_over_the_three_candidates() {
    // Criterion 3: the output is a structured, comparable report suitable for the
    // exploration spec's architecture decision — the three candidates (own-engine vs
    // Servo vs Blitz/Stylo assembly) side by side on the SAME axes.
    let report = run_harness();
    assert_eq!(report.candidates.len(), 3, "all three candidates present");
    for candidate in Candidate::ALL {
        assert!(
            report.row(candidate).is_some(),
            "candidate {} has a report row",
            candidate.id()
        );
    }

    // The two not-yet-built paths are honest DECLARED slots, not fabricated zeros
    // masquerading as measurement.
    for candidate in [Candidate::OwnEngine, Candidate::Servo] {
        let row = report.row(candidate).unwrap();
        assert_eq!(row.scoring, CandidateScoring::Declared);
        assert!(row.capability.pages.is_empty());
    }

    // The harness does NOT decide: it does not rank or pick a winner — it lays the
    // candidates side by side. (No "winner"/"decision" field exists on the report.)
    let json = report.to_json();
    assert!(json.contains("\"candidate\": \"own-engine\""));
    assert!(json.contains("\"candidate\": \"servo\""));
    assert!(json.contains("\"candidate\": \"blitz-stylo-assembly\""));
    assert!(json.contains("\"scoring\": \"measured\""));
    assert!(json.contains("\"scoring\": \"declared\""));
    assert!(json.contains("\"capability\""));
    assert!(json.contains("\"trust_hooks\""));
    assert!(json.contains("\"vs_wezig\""));
}

#[test]
fn harness_is_re_runnable_and_its_report_is_reproducible() {
    // Criterion 4: re-runnable + reproducible. Two independent runs of the whole
    // harness produce an identical report (and identical serialisation), so a
    // captured run is stable evidence.
    let a = run_harness();
    let b = run_harness();
    assert_eq!(a, b, "two harness runs produce the identical report");
    assert_eq!(
        a.to_json(),
        b.to_json(),
        "the serialised report is byte-stable across runs"
    );
}

#[test]
fn prints_the_report_for_capture() {
    // Emit the structured report so a captured run can be committed as evidence
    // (run with `-- --nocapture`). This is the re-runnable EVIDENCE generator the
    // exploration spec consumes; it does not assert anything beyond producing it.
    let report = run_harness();
    println!("{}", report.to_json());
}
