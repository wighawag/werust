//! The T1 WPT-subset regression meter — the objective secondary meter machinery.
//!
//! This module is the reusable measurement engine for the conformance ladder's
//! **T1 WPT-subset bar** (`docs/conformance-tiers.md` T1; spec story 17, task
//! `t1-wpt-subset-regression-meter`). It runs the two named WPT subsets against the
//! NATIVE T1 path and reports a pass-rate per subset; the threshold ENFORCEMENT
//! (>= 90 % tree-construction, >= 70 % core-CSS) lives in the integration test
//! `tests/t1_wpt_subset_meter.rs`, which is what makes the meter a CI regression
//! guard (it runs under `cargo test`, i.e. the `verify` gate).
//!
//! It is the SECONDARY meter, NOT the roadmap driver: the page checklists
//! (`t1-server-web-floor-*`) define "reached"; this catches regressions and gives a
//! comparable-over-time number (also feeding the vs-wezig comparison).
//!
//! # The two subsets, both run against the native path
//!
//! - [`run_tree_construction`] runs the `html/syntax/parsing/` html5lib-derived
//!   tree-construction tests: each `#data` fragment is parsed by the native T1
//!   parser ([`Html5everParser`](crate::Html5everParser) behind the
//!   [`Parser`](crate::Parser) seam), the resulting render [`Dom`](crate::Dom)
//!   serialized in the html5lib `#document` format, and compared to the expected
//!   tree — normalised for the nodes werust's static render tree legitimately drops
//!   (doctype, comments), which the parser task already documented.
//! - [`run_core_css`] runs the five core-CSS areas' computed-value cases through
//!   the native cascade surface ([`Stylesheet::parse`](crate::css::Stylesheet) +
//!   [`cascade`](crate::css::cascade) + [`ComputedStyle`](crate::css::ComputedStyle))
//!   and checks each assertion.
//!
//! # Why pinned local fixtures (a recorded decision)
//!
//! The meter runs hermetically under `verify` (offline, no reference browser, no JS
//! engine — that is T3). The raw upstream core-CSS WPT files are testharness.js /
//! reftest suites needing a JS runtime or a reference-browser pixel diff werust
//! does not have at T1, so they cannot be executed here without fabricating
//! results. The tree-construction `.dat` files ARE self-contained (no JS, a plain
//! text tree assertion) and are pinned in their exact upstream shape; the core-CSS
//! cases are computed-value assertions modelled on the five upstream areas, driven
//! against the real cascade. Provenance + the decision are recorded in
//! `crates/native-renderer/tests/fixtures/t1-wpt/SOURCE.md` and
//! `docs/spikes/t1-wpt-subset-regression-meter/README.md`.

pub mod core_css;
pub mod tree_construction;

use std::path::Path;

pub use core_css::run as run_core_css;
pub use tree_construction::run as run_tree_construction;

/// One failed case in a [`MeterReport`]: the case name and a short reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The case identifier (file + index, or the case's name).
    pub name: String,
    /// A one-line reason the case failed (an expected-vs-actual summary).
    pub reason: String,
}

/// The outcome of running one WPT subset against the native path: the pass-rate
/// plus the list of failures for a legible regression report.
///
/// The pass-rate ([`pass_rate`](MeterReport::pass_rate)) is the comparable
/// over-time number the ladder + the vs-wezig comparison consume; the threshold
/// enforcement lives in the integration test.
#[derive(Debug, Clone, Default)]
pub struct MeterReport {
    /// The number of cases that ran.
    pub total: usize,
    /// The number of cases that passed.
    pub passed: usize,
    /// The cases that failed, with reasons.
    pub failures: Vec<Failure>,
    /// The distinct WPT areas represented (for core-CSS; empty for
    /// tree-construction, which is a single area).
    areas: Vec<String>,
}

impl MeterReport {
    /// The pass-rate in `[0.0, 1.0]`. An empty subset reports `0.0` (which fails
    /// any positive threshold, so an accidentally-empty subset can never pass).
    #[must_use]
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.passed as f64 / self.total as f64
        }
    }

    /// The distinct WPT areas represented in this report, sorted and deduplicated.
    #[must_use]
    pub fn areas(&self) -> &[String] {
        &self.areas
    }

    /// A multi-line, human-legible summary of the failures (for a red assertion).
    #[must_use]
    pub fn failure_summary(&self) -> String {
        if self.failures.is_empty() {
            return "(no failures)".to_string();
        }
        self.failures
            .iter()
            .map(|f| format!("  - {}: {}", f.name, f.reason))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Record a passing case.
    fn record_pass(&mut self) {
        self.total += 1;
        self.passed += 1;
    }

    /// Record a failing case with a reason.
    fn record_fail(&mut self, name: impl Into<String>, reason: impl Into<String>) {
        self.total += 1;
        self.failures.push(Failure {
            name: name.into(),
            reason: reason.into(),
        });
    }

    /// Note an area as represented (deduplicating, keeping sorted order).
    fn note_area(&mut self, area: &str) {
        if !self.areas.iter().any(|a| a == area) {
            self.areas.push(area.to_string());
            self.areas.sort();
        }
    }
}

/// Read a fixture file, panicking with the path on error (fixtures are committed,
/// so a read failure is a broken checkout, not a runtime condition).
pub(crate) fn read_fixture(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read WPT fixture {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_scores_zero_and_fails_any_positive_threshold() {
        let report = MeterReport::default();
        assert_eq!(report.pass_rate(), 0.0);
        assert!(report.pass_rate() < 0.70);
    }

    #[test]
    fn pass_rate_is_passed_over_total() {
        let mut report = MeterReport::default();
        report.record_pass();
        report.record_pass();
        report.record_fail("x", "boom");
        assert_eq!(report.total, 3);
        assert_eq!(report.passed, 2);
        assert!((report.pass_rate() - 2.0 / 3.0).abs() < 1e-9);
        assert!(report.failure_summary().contains("x: boom"));
    }

    #[test]
    fn note_area_is_deduplicated_and_sorted() {
        let mut report = MeterReport::default();
        report.note_area("css/css-fonts");
        report.note_area("css/css-box");
        report.note_area("css/css-fonts");
        assert_eq!(report.areas(), &["css/css-box", "css/css-fonts"]);
    }
}
