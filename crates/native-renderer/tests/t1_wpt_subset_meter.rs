//! The T1 WPT-subset regression meter — the objective secondary meter that runs
//! the pinned WPT subsets against the native T1 path and enforces their thresholds.
//!
//! This is the acceptance evidence for task `t1-wpt-subset-regression-meter` (spec
//! story 17, `docs/conformance-tiers.md` T1). It is the SECONDARY meter, NOT the
//! roadmap: the page checklists (`t1-server-web-floor-article-and-blog`,
//! `t1-content-addressed-floor-ipfs-static-site`) define "reached"; this catches
//! regressions and gives a comparable-over-time number that also feeds the vs-wezig
//! comparison.
//!
//! Two subsets, per the T1 bar, both run against the NATIVE T1 path:
//!
//! 1. **Tree-construction** (`html/syntax/parsing/`, the html5lib-derived
//!    tree-construction tests): each pinned `.dat` case is parsed by the native T1
//!    parser ([`Html5everParser`] behind the `Parser` seam), its render tree
//!    serialized in the html5lib `#document` format, and compared to the expected
//!    tree (normalised for the nodes werust's static render tree legitimately drops
//!    — doctype/comments). Threshold: **>= 90 %**.
//! 2. **Core CSS** (`css/CSS2/normal-flow/`, `css/css-box/`, `css/css-color/`,
//!    `css/css-fonts/`, `css/css-text/`): each pinned computed-value case is driven
//!    through the native cascade surface (`Stylesheet::parse` + `cascade` +
//!    `ComputedStyle`) and its assertion checked. Threshold: **>= 70 %**.
//!
//! Complex-script / bidi subsets are EXCLUDED from the T1 bar (deferred with T2
//! shaping) — the meter asserts no such area appears in the pinned set.
//!
//! Why pinned local fixtures and not the live upstream WPT tree: the meter must run
//! hermetically under `verify` (`cargo test`, offline, no reference browser and no
//! JS engine — that is T3). The core-CSS WPT areas are testharness.js / reftest
//! suites that need a JS runtime or a reference-browser pixel diff werust does not
//! have at T1, so the raw upstream files cannot be executed here without fabricating
//! results. Instead a pinned, provenance-documented subset of cases (in the exact
//! upstream shape for tree-construction; computed-value assertions modelled on the
//! five upstream CSS areas) is committed and run against the native path. The
//! provenance and the decision are recorded in `tests/fixtures/t1-wpt/SOURCE.md`
//! and in `docs/spikes/t1-wpt-subset-regression-meter/README.md`.

use std::path::{Path, PathBuf};

use native_renderer::wpt_meter::{self, MeterReport};

/// The tree-construction pass-rate floor (`docs/conformance-tiers.md` T1).
const TREE_CONSTRUCTION_THRESHOLD: f64 = 0.90;
/// The core-CSS pass-rate floor (`docs/conformance-tiers.md` T1).
const CORE_CSS_THRESHOLD: f64 = 0.70;

/// The five core-CSS WPT areas the T1 bar names (bidi/complex-script excluded).
const CORE_CSS_AREAS: &[&str] = &[
    "css/CSS2/normal-flow",
    "css/css-box",
    "css/css-color",
    "css/css-fonts",
    "css/css-text",
];

/// The bidi / complex-script area prefixes EXCLUDED from the T1 bar.
const EXCLUDED_AREA_MARKERS: &[&str] = &["bidi", "writing-modes", "i18n"];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/t1-wpt")
}

fn tree_construction_report() -> MeterReport {
    wpt_meter::run_tree_construction(&fixtures_dir().join("tree-construction"))
}

fn core_css_report() -> MeterReport {
    wpt_meter::run_core_css(&fixtures_dir().join("core-css/cases.txt"))
}

#[test]
fn tree_construction_subset_runs_and_meets_its_threshold() {
    // The html/syntax/parsing/ tree-construction subset runs against the native T1
    // parse path and produces a pass-rate at or above the >= 90 % floor.
    let report = tree_construction_report();
    assert!(report.total >= 10, "a meaningful number of cases ran");
    println!(
        "[T1 WPT meter] tree-construction: {}/{} = {:.1}% (floor {:.0}%)",
        report.passed,
        report.total,
        report.pass_rate() * 100.0,
        TREE_CONSTRUCTION_THRESHOLD * 100.0,
    );
    assert!(
        report.pass_rate() >= TREE_CONSTRUCTION_THRESHOLD,
        "tree-construction pass-rate {:.1}% dropped below the {:.0}% T1 floor; failures:\n{}",
        report.pass_rate() * 100.0,
        TREE_CONSTRUCTION_THRESHOLD * 100.0,
        report.failure_summary(),
    );
}

#[test]
fn core_css_subset_runs_and_meets_its_threshold() {
    // The five core-CSS areas run against the native cascade and produce a
    // pass-rate at or above the >= 70 % floor.
    let report = core_css_report();
    assert!(report.total >= 10, "a meaningful number of cases ran");
    println!(
        "[T1 WPT meter] core-CSS: {}/{} = {:.1}% (floor {:.0}%)",
        report.passed,
        report.total,
        report.pass_rate() * 100.0,
        CORE_CSS_THRESHOLD * 100.0,
    );
    assert!(
        report.pass_rate() >= CORE_CSS_THRESHOLD,
        "core-CSS pass-rate {:.1}% dropped below the {:.0}% T1 floor; failures:\n{}",
        report.pass_rate() * 100.0,
        CORE_CSS_THRESHOLD * 100.0,
        report.failure_summary(),
    );
}

#[test]
fn core_css_cases_cover_every_named_t1_area() {
    // Each of the five named core-CSS areas is represented in the pinned set, so
    // the bar measures the whole named subset and not just the easy corner of it.
    let report = core_css_report();
    for area in CORE_CSS_AREAS {
        assert!(
            report.areas().iter().any(|a| a == area),
            "core-CSS area {area} is represented in the pinned set; got {:?}",
            report.areas(),
        );
    }
}

#[test]
fn complex_script_and_bidi_subsets_are_excluded_from_the_t1_bar() {
    // The T1 bar EXCLUDES complex-script / bidi (deferred with T2 shaping): no
    // pinned core-CSS case may belong to an excluded area, and every case's area
    // must be one of the five named T1 areas.
    let report = core_css_report();
    for area in report.areas() {
        assert!(
            !EXCLUDED_AREA_MARKERS
                .iter()
                .any(|marker| area.contains(marker)),
            "excluded (bidi/complex-script) area leaked into the T1 bar: {area}"
        );
        assert!(
            CORE_CSS_AREAS.contains(&area.as_str()),
            "unexpected area outside the named T1 core-CSS set: {area}"
        );
    }
}

#[test]
fn meter_reports_a_comparable_over_time_number() {
    // The meter surfaces a single comparable number per subset (passed/total and a
    // rate), the at-a-glance figure the ladder + the vs-wezig comparison consume.
    let tree = tree_construction_report();
    let css = core_css_report();
    assert!((0.0..=1.0).contains(&tree.pass_rate()));
    assert!((0.0..=1.0).contains(&css.pass_rate()));
    // The number is deterministic: re-running yields the identical count.
    assert_eq!(tree.passed, tree_construction_report().passed);
    assert_eq!(css.passed, core_css_report().passed);
}
