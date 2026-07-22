//! The native-renderer benchmark harness: the EVIDENCE generator for the deferred
//! native-renderer architecture decision.
//!
//! This is the harness of spec stories 20 + 21 (task
//! `native-renderer-benchmark-harness-capability-and-trust-hooks`,
//! `docs/conformance-tiers.md`, `CONTEXT.md`, and the exploration spec
//! `rust-successor-native-renderer-architecture-benchmark`). It SCORES a candidate
//! native-renderer path on three axes and emits ONE structured, comparable,
//! reproducible report the follow-on exploration spec consumes to DECIDE the
//! architecture:
//!
//! 1. **Capability** ([`CapabilityScore`]) — the pinned conformance-ladder page
//!    checklists (each T1 server-floor page rendered through the [`Renderer`] seam)
//!    plus the WPT subsets (`html/syntax/parsing/` tree-construction ≥ 90 %,
//!    the five core-CSS areas ≥ 70 %), reusing [`crate::wpt_meter`].
//! 2. **Trust hooks** ([`TrustHookScore`]) — a PASS/FAIL qualification, reusing the
//!    seam's own [`renderer::qualify`] gate (provider injection + `ipfs://` scheme),
//!    NOT a graded score. A candidate that renders well but cannot satisfy the
//!    thesis is correctly disqualified (`docs/adr/0001`, the `Renderer` seam's
//!    qualifying rule).
//! 3. **vs-wezig meter** ([`VsWezigMeter`]) — the reversible experiment's
//!    measurement (`docs/adr/0001`): the T1 climb measured AGAINST wezig's Zig arm
//!    on the SHARED conformance ladder — effort, code volume, and friction
//!    (especially DOM object-graph friction).
//!
//! # It MEASURES; it does NOT decide
//!
//! This harness produces the report — it does not pick the architecture. The three
//! candidate paths the exploration spec compares
//! ([`Candidate::OwnEngine`], [`Candidate::Servo`],
//! [`Candidate::BlitzStyloAssembly`]) are scored on the SAME pinned checklists so
//! they are comparable on evidence, not preference. A candidate is scored either as
//! MEASURED (a real [`Renderer`] backend driven through the ladder now) or as
//! DECLARED (a not-yet-built path carried as an honest, comparable slot the
//! exploration fills in) — see [`CandidateScoring`]. Today the assembled pure-Rust
//! stack behind [`NativeRenderer`](crate::NativeRenderer) (the Blitz/Stylo-assembly
//! class) is the one MEASURED candidate; own-engine and Servo are DECLARED.
//!
//! # Reproducible
//!
//! Every score is deterministic: the page checklist renders committed fixtures
//! through the native path, the WPT meter runs pinned local subsets, the trust-hook
//! gate is a pure function of the backend's declared [`renderer::TrustHooks`], and
//! the wezig arm is a pinned recorded comparison. Re-running yields the identical
//! report; [`BenchmarkReport::to_json`] serialises it in a stable, diffable shape so
//! a captured run can be committed as evidence.

use std::path::Path;

use renderer::{qualify, LoadState, Renderer, TrustHook};

use crate::wpt_meter::{self, MeterReport};
use crate::NativeRenderer;

/// The tree-construction pass-rate floor (`docs/conformance-tiers.md` T1). Mirrors
/// the meter's enforcement in `tests/t1_wpt_subset_meter.rs` so the report can mark
/// a subset pass/fail on the SAME bar the ladder guards.
pub const TREE_CONSTRUCTION_THRESHOLD: f64 = 0.90;
/// The core-CSS pass-rate floor (`docs/conformance-tiers.md` T1).
pub const CORE_CSS_THRESHOLD: f64 = 0.70;

/// A candidate native-renderer ARCHITECTURE the exploration spec compares.
///
/// These are exactly the three paths the exploration spec
/// (`rust-successor-native-renderer-architecture-benchmark`, open question 1) puts
/// to the benchmark: own from-scratch engine vs reused Servo behind the seam vs a
/// Blitz/Stylo-component assembly. The harness scores each on the SAME pinned
/// checklists so they are comparable on evidence, not preference. This enum is the
/// stable identity a report row is keyed by; it does NOT rank them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    /// werust's own from-scratch Rust renderer.
    OwnEngine,
    /// Reused Servo behind the [`Renderer`] seam.
    Servo,
    /// A Blitz/Stylo-component assembly on the mature pure-Rust stack (html5ever +
    /// stylo + taffy + parley + vello/wgpu) — the class werust climbs T1 on today.
    BlitzStyloAssembly,
}

impl Candidate {
    /// Every candidate the exploration spec compares, in a stable order.
    pub const ALL: [Candidate; 3] = [
        Candidate::OwnEngine,
        Candidate::Servo,
        Candidate::BlitzStyloAssembly,
    ];

    /// The stable machine identifier used as the report key (JSON, comparisons).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Candidate::OwnEngine => "own-engine",
            Candidate::Servo => "servo",
            Candidate::BlitzStyloAssembly => "blitz-stylo-assembly",
        }
    }
}

/// How a candidate's scores were obtained: really measured now, or declared as a
/// not-yet-built slot.
///
/// This keeps the report HONEST and comparable: the exploration spec must see which
/// numbers are real evidence (a backend driven through the ladder now) and which are
/// placeholders it still has to fill (a candidate not built in this repo yet). A
/// harness that silently reported zeros for an unbuilt candidate would let the
/// decision rest on fiction; a `Declared` row says "not scored here" out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateScoring {
    /// The candidate was scored end-to-end against the pinned ladder in this run.
    Measured,
    /// The candidate is not built in this repo yet; its row is a comparable slot the
    /// exploration spec fills in when the path is prototyped.
    Declared,
}

impl CandidateScoring {
    /// The stable machine identifier for this scoring mode.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            CandidateScoring::Measured => "measured",
            CandidateScoring::Declared => "declared",
        }
    }
}

/// One page-checklist entry's result: did the pinned page render through the native
/// path, and did it produce painted content?
///
/// The page checklist is the PRIMARY, human-legible capability driver
/// (`docs/conformance-tiers.md`: "the page checklist defines and drives each tier").
/// A page counts as rendered only if it loaded to
/// [`Finished`](renderer::LoadState::Finished) through the [`Renderer`] seam AND
/// produced at least one painted run — the same "render correctly" the floor goldens
/// assert, reduced to a comparable pass/fail signal for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageResult {
    /// The pinned page's name (its fixture stem).
    pub page: String,
    /// Whether the page rendered through the native path with painted content.
    pub rendered: bool,
}

/// The capability axis of a candidate's score: the page checklist plus the WPT
/// subset pass-rates, on the pinned conformance ladder.
///
/// Both halves are load-bearing exactly as the ladder frames them: the page
/// checklist ([`pages`](CapabilityScore::pages)) is the primary driver ("can a real
/// page of this class render at all?"); the WPT pass-rates
/// ([`tree_construction_rate`](CapabilityScore::tree_construction_rate),
/// [`core_css_rate`](CapabilityScore::core_css_rate)) are the objective secondary
/// regression meter, compared against the T1 thresholds.
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityScore {
    /// One result per pinned page-checklist page.
    pub pages: Vec<PageResult>,
    /// The `html/syntax/parsing/` tree-construction pass-rate in `[0.0, 1.0]`.
    pub tree_construction_rate: f64,
    /// The core-CSS pass-rate in `[0.0, 1.0]` across the five named areas.
    pub core_css_rate: f64,
}

impl CapabilityScore {
    /// The count of pinned pages that rendered.
    #[must_use]
    pub fn pages_rendered(&self) -> usize {
        self.pages.iter().filter(|p| p.rendered).count()
    }

    /// Whether EVERY pinned page rendered.
    #[must_use]
    pub fn all_pages_rendered(&self) -> bool {
        !self.pages.is_empty() && self.pages.iter().all(|p| p.rendered)
    }

    /// Whether the tree-construction subset meets the T1 ≥ 90 % floor.
    #[must_use]
    pub fn tree_construction_meets_bar(&self) -> bool {
        self.tree_construction_rate >= TREE_CONSTRUCTION_THRESHOLD
    }

    /// Whether the core-CSS subset meets the T1 ≥ 70 % floor.
    #[must_use]
    pub fn core_css_meets_bar(&self) -> bool {
        self.core_css_rate >= CORE_CSS_THRESHOLD
    }

    /// Whether BOTH the page checklist and BOTH WPT bars are met — the capability
    /// axis is fully satisfied at T1.
    #[must_use]
    pub fn meets_t1_capability(&self) -> bool {
        self.all_pages_rendered() && self.tree_construction_meets_bar() && self.core_css_meets_bar()
    }
}

/// The trust-hook axis of a candidate's score: a PASS/FAIL qualification, reusing
/// the seam's own [`renderer::qualify`] gate.
///
/// This is deliberately NOT a graded number (the exploration spec: "a pass/fail
/// qualifying gate, not a graded score"): a candidate that renders beautifully but
/// cannot inject the EIP-1193 provider AND resolve `ipfs://` is DISQUALIFIED,
/// naming exactly the hooks it does not satisfy. Reusing `qualify` means the
/// benchmark holds every candidate to the SAME gate the webview and native backends
/// already pass through — the thesis encoded once (`docs/adr/0001`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustHookScore {
    /// Whether the candidate satisfies BOTH trust hooks (the qualifying set).
    pub qualifies: bool,
    /// The trust hooks the candidate does NOT satisfy, in [`TrustHook::ALL`] order
    /// (empty iff it qualifies).
    pub missing: Vec<TrustHook>,
}

impl TrustHookScore {
    /// Score a candidate's [`Renderer`] backend against the trust-hook gate.
    #[must_use]
    pub fn qualify_backend(backend: &dyn Renderer) -> Self {
        match qualify(backend) {
            Ok(()) => TrustHookScore {
                qualifies: true,
                missing: Vec::new(),
            },
            Err(disq) => TrustHookScore {
                qualifies: false,
                missing: disq.missing,
            },
        }
    }
}

/// The vs-wezig comparison arm on the SHARED conformance ladder: the reversible
/// experiment's measurement (`docs/adr/0001`, spec story 20).
///
/// werust runs as a reversible experiment against wezig's Zig arm: does standing on
/// the pure-Rust stack give a simpler/faster path to T1? These are the comparable
/// SIGNALS that answer it — effort, code volume, and friction (especially DOM
/// object-graph friction) — carried per arm so the two are read side by side on the
/// SAME rung. A higher [`friction`](VsWezigMeter::friction) is the "Rust drowns in
/// DOM object-graph friction is a valid finding" signal, recorded honestly rather
/// than papered over.
///
/// The numbers are pinned inputs (recorded evidence), not computed here: this
/// harness STRUCTURES the comparison so it is comparable and reproducible; the raw
/// effort/volume/friction figures for each arm come from the measured build and are
/// recorded in the harness fixture (see the spike README). The meter's job is to put
/// them on the shared ladder, not to invent them.
#[derive(Debug, Clone, PartialEq)]
pub struct VsWezigMeter {
    /// The ladder rung this comparison is measured at (e.g. `"T1"`).
    pub tier: String,
    /// The candidate capability score's rendered-page count / total, as a fraction
    /// in `[0.0, 1.0]` — the capability the friction bought, comparable across arms.
    pub capability_fraction: f64,
    /// The Rust arm's signals at this rung.
    pub rust: ArmSignals,
    /// The wezig (Zig) arm's signals at the SAME rung.
    pub wezig: ArmSignals,
}

impl VsWezigMeter {
    /// The DOM object-graph friction delta (Rust − wezig): positive means the Rust
    /// arm carries MORE DOM object-graph friction at this rung — the experiment's
    /// central "does Rust drown in friction?" signal, surfaced as one number.
    #[must_use]
    pub fn dom_friction_delta(&self) -> i64 {
        self.rust.dom_object_graph_friction as i64 - self.wezig.dom_object_graph_friction as i64
    }
}

/// One arm's comparable signals on the shared ladder (Rust or wezig).
///
/// The three signals are the axes spec story 20 names — effort, code volume, and
/// friction — kept as plain comparable scalars so the two arms read side by side.
/// They are recorded evidence (pinned inputs to the harness), NOT computed from the
/// source tree here: what the harness owns is putting them on the shared ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmSignals {
    /// Effort to reach the rung, in person-days (the recorded build effort).
    pub effort_person_days: u32,
    /// Renderer code volume at the rung, in lines of code.
    pub code_volume_loc: u32,
    /// DOM object-graph friction — a recorded count of the seams/adapters/clones the
    /// arm needs to thread the DOM through its layout/paint stages (higher = more
    /// friction). The experiment's central axis.
    pub dom_object_graph_friction: u32,
}

/// One candidate's whole benchmark row: identity, how it was scored, and its three
/// axes.
///
/// A row is the unit the exploration spec compares: the capability axis, the
/// pass/fail trust-hook gate, and the vs-wezig meter, all keyed by [`Candidate`] and
/// tagged with [`CandidateScoring`] so measured evidence is never confused with a
/// not-yet-built slot.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateReport {
    /// Which architecture this row scores.
    pub candidate: Candidate,
    /// Whether the scores are measured now or a declared slot.
    pub scoring: CandidateScoring,
    /// The capability axis (page checklist + WPT subsets).
    pub capability: CapabilityScore,
    /// The trust-hook qualification (pass/fail).
    pub trust_hooks: TrustHookScore,
    /// The vs-wezig comparison meter on the shared ladder.
    pub vs_wezig: VsWezigMeter,
}

/// The whole structured benchmark report: every candidate row on the SAME pinned
/// ladder.
///
/// This is the harness's deliverable — the structured, comparable, reproducible
/// evidence the exploration spec decides the architecture from. It intentionally
/// does NOT rank or pick a winner: it lays the candidates side by side on the same
/// axes and lets the human-resolved decision (informed by the human's *why*) choose.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkReport {
    /// One row per candidate architecture.
    pub candidates: Vec<CandidateReport>,
}

impl BenchmarkReport {
    /// The row for `candidate`, if present.
    #[must_use]
    pub fn row(&self, candidate: Candidate) -> Option<&CandidateReport> {
        self.candidates.iter().find(|c| c.candidate == candidate)
    }

    /// Serialise the report to a stable, diffable JSON string.
    ///
    /// Hand-serialised (no serde dependency) in a fixed field order so a captured
    /// run is byte-stable and reviewable as committed evidence. The shape is the
    /// contract the exploration spec reads.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n  \"candidates\": [\n");
        for (i, c) in self.candidates.iter().enumerate() {
            out.push_str(&candidate_to_json(c));
            if i + 1 < self.candidates.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// Run the WPT capability subsets against the native path, given the pinned fixture
/// directory (`crates/native-renderer/tests/fixtures/t1-wpt`).
///
/// This is the reused [`crate::wpt_meter`] measurement engine, packaged as the
/// capability half's WPT input so the benchmark and the ladder's own regression
/// meter measure the identical thing. Returns the two subset reports
/// (tree-construction, core-CSS).
#[must_use]
pub fn run_wpt_subsets(fixtures_dir: &Path) -> (MeterReport, MeterReport) {
    let tree = wpt_meter::run_tree_construction(&fixtures_dir.join("tree-construction"));
    let css = wpt_meter::run_core_css(&fixtures_dir.join("core-css/cases.txt"));
    (tree, css)
}

/// One pinned page-checklist page: its report name and the HTML source the native
/// path renders (a self-contained document, no network — the T1 backend renders
/// `data:text/html,…` today).
///
/// The harness scores capability by rendering each such page through the
/// [`Renderer`] seam, exactly as the browser shell would, and marking it rendered
/// iff it reached [`LoadState::Finished`] with painted content — the same "render
/// correctly" the floor goldens assert, reduced to the report's pass/fail signal.
#[derive(Debug, Clone)]
pub struct ChecklistPage {
    /// The report name for this page (its fixture stem).
    pub name: String,
    /// The page's HTML source (committed fixture bytes).
    pub html: String,
}

/// Score a page checklist against a native [`Renderer`] backend: render each page
/// through the seam and record whether it rendered.
///
/// This is the capability half's PAGE-checklist scorer (`docs/conformance-tiers.md`:
/// the page checklist is the primary capability driver). A page counts as rendered
/// only when it loaded to [`LoadState::Finished`] through the seam AND painted at
/// least one run — a comparable pass/fail per page. Deterministic: the same pages
/// and backend always yield the same results.
#[must_use]
pub fn score_page_checklist(pages: &[ChecklistPage]) -> Vec<PageResult> {
    pages
        .iter()
        .map(|page| PageResult {
            page: page.name.clone(),
            rendered: page_renders_through_native_path(&page.html),
        })
        .collect()
}

/// Render one page through a fresh native backend, returning whether it reached
/// [`LoadState::Finished`] with painted content.
fn page_renders_through_native_path(html: &str) -> bool {
    let mut backend = NativeRenderer::new();
    {
        let seam: &mut dyn Renderer = &mut backend;
        if seam.navigate(&data_url(html)).is_err() {
            return false;
        }
        if seam.load_state() != LoadState::Finished {
            return false;
        }
    }
    backend
        .last_render()
        .is_some_and(|out| !out.layout.runs.is_empty())
}

/// Build a `data:text/html,…` URL for `html`, percent-encoding exactly the bytes the
/// backend's decoder treats specially (`%`, `+`) plus spaces — so the committed page
/// reaches the native path byte-for-byte intact with NO network fetch (mirrors the
/// floor goldens' `data_url`).
fn data_url(html: &str) -> String {
    let mut payload = String::new();
    for b in html.bytes() {
        match b {
            b'%' => payload.push_str("%25"),
            b'+' => payload.push_str("%2B"),
            b' ' => payload.push_str("%20"),
            _ => payload.push(b as char),
        }
    }
    format!("data:text/html,{payload}")
}

/// Score the MEASURED assembled-pure-Rust-stack candidate (the Blitz/Stylo-assembly
/// class) end to end: render the page checklist through the native backend, run the
/// WPT subsets, and qualify the backend on the trust hooks. The vs-wezig meter is a
/// pinned recorded comparison the caller supplies (the raw effort/volume/friction
/// figures are measured evidence, not computed here — see the module docs).
///
/// Returns a [`CandidateReport`] tagged [`CandidateScoring::Measured`]. This is the
/// re-runnable scoring the integration test drives; it is deterministic.
#[must_use]
pub fn score_measured_candidate(
    candidate: Candidate,
    pages: &[ChecklistPage],
    wpt_fixtures_dir: &Path,
    vs_wezig: VsWezigMeter,
) -> CandidateReport {
    let page_results = score_page_checklist(pages);
    let (tree, css) = run_wpt_subsets(wpt_fixtures_dir);
    let capability = CapabilityScore {
        pages: page_results,
        tree_construction_rate: tree.pass_rate(),
        core_css_rate: css.pass_rate(),
    };

    // Qualify the SAME native backend the pages rendered through, reusing the seam's
    // own gate: the trust-hook axis is the pass/fail qualification, not a score.
    let backend = NativeRenderer::new();
    let trust_hooks = TrustHookScore::qualify_backend(&backend);

    CandidateReport {
        candidate,
        scoring: CandidateScoring::Measured,
        capability,
        trust_hooks,
        vs_wezig,
    }
}

/// A DECLARED candidate row: a not-yet-built path carried as an honest, comparable
/// slot the exploration spec fills in when the path is prototyped.
///
/// Its capability is empty (no page rendered, zero WPT — a slot, not a score) and
/// its trust-hook axis records the candidate as not-yet-qualifying with both hooks
/// unmet. Tagged [`CandidateScoring::Declared`] so the exploration never mistakes a
/// placeholder for measured evidence.
#[must_use]
pub fn declared_candidate(candidate: Candidate, tier: &str) -> CandidateReport {
    CandidateReport {
        candidate,
        scoring: CandidateScoring::Declared,
        capability: CapabilityScore {
            pages: Vec::new(),
            tree_construction_rate: 0.0,
            core_css_rate: 0.0,
        },
        trust_hooks: TrustHookScore {
            qualifies: false,
            missing: TrustHook::ALL.to_vec(),
        },
        vs_wezig: VsWezigMeter {
            tier: tier.to_string(),
            capability_fraction: 0.0,
            rust: ArmSignals {
                effort_person_days: 0,
                code_volume_loc: 0,
                dom_object_graph_friction: 0,
            },
            wezig: ArmSignals {
                effort_person_days: 0,
                code_volume_loc: 0,
                dom_object_graph_friction: 0,
            },
        },
    }
}

fn candidate_to_json(c: &CandidateReport) -> String {
    let pages = c
        .capability
        .pages
        .iter()
        .map(|p| {
            format!(
                "          {{ \"page\": \"{}\", \"rendered\": {} }}",
                p.page, p.rendered
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let missing = c
        .trust_hooks
        .missing
        .iter()
        .map(|h| format!("\"{}\"", trust_hook_id(*h)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "    {{\n\
         \x20     \"candidate\": \"{cand}\",\n\
         \x20     \"scoring\": \"{scoring}\",\n\
         \x20     \"capability\": {{\n\
         \x20       \"pages\": [\n{pages}\n        ],\n\
         \x20       \"pages_rendered\": {pr},\n\
         \x20       \"tree_construction_rate\": {tc:.4},\n\
         \x20       \"tree_construction_meets_bar\": {tcb},\n\
         \x20       \"core_css_rate\": {cc:.4},\n\
         \x20       \"core_css_meets_bar\": {ccb},\n\
         \x20       \"meets_t1_capability\": {mt1}\n\
         \x20     }},\n\
         \x20     \"trust_hooks\": {{ \"qualifies\": {tq}, \"missing\": [{missing}] }},\n\
         \x20     \"vs_wezig\": {{\n\
         \x20       \"tier\": \"{tier}\",\n\
         \x20       \"capability_fraction\": {capf:.4},\n\
         \x20       \"rust\": {rust},\n\
         \x20       \"wezig\": {wezig},\n\
         \x20       \"dom_friction_delta\": {dfd}\n\
         \x20     }}\n\
         \x20 }}",
        cand = c.candidate.id(),
        scoring = c.scoring.id(),
        pages = pages,
        pr = c.capability.pages_rendered(),
        tc = c.capability.tree_construction_rate,
        tcb = c.capability.tree_construction_meets_bar(),
        cc = c.capability.core_css_rate,
        ccb = c.capability.core_css_meets_bar(),
        mt1 = c.capability.meets_t1_capability(),
        tq = c.trust_hooks.qualifies,
        missing = missing,
        tier = c.vs_wezig.tier,
        capf = c.vs_wezig.capability_fraction,
        rust = arm_to_json(&c.vs_wezig.rust),
        wezig = arm_to_json(&c.vs_wezig.wezig),
        dfd = c.vs_wezig.dom_friction_delta(),
    )
}

fn arm_to_json(a: &ArmSignals) -> String {
    format!(
        "{{ \"effort_person_days\": {}, \"code_volume_loc\": {}, \"dom_object_graph_friction\": {} }}",
        a.effort_person_days, a.code_volume_loc, a.dom_object_graph_friction
    )
}

fn trust_hook_id(hook: TrustHook) -> &'static str {
    match hook {
        TrustHook::ProviderInjection => "provider-injection",
        TrustHook::IpfsScheme => "ipfs-scheme",
    }
}

/// A tiny in-test macro: stamp out a minimal render-only `Renderer` impl whose only
/// meaningful behaviour is the declared trust-hook set, so the trust-hook scoring
/// tests can exercise `qualify` without a real backend.
#[cfg(test)]
macro_rules! impl_stub_renderer {
    ($ty:ty, $hooks:expr) => {
        impl renderer::Renderer for $ty {
            fn navigate(&mut self, _url: &str) -> Result<(), renderer::RendererError> {
                Ok(())
            }
            fn reload(&mut self) -> Result<(), renderer::RendererError> {
                Ok(())
            }
            fn stop(&mut self) {}
            fn load_state(&self) -> renderer::LoadState {
                renderer::LoadState::Idle
            }
            fn current_url(&self) -> Option<String> {
                None
            }
            fn poll_event(&mut self) -> Option<renderer::LoadEvent> {
                None
            }
            fn view_handle(&self) -> renderer::ViewHandle {
                renderer::ViewHandle(std::ptr::null_mut())
            }
            fn send_pointer(&mut self, _event: renderer::PointerEvent) {}
            fn send_key(&mut self, _event: renderer::KeyEvent) {}
            fn send_scroll(&mut self, _delta: renderer::ScrollDelta) {}
            fn set_focus(&mut self, _focused: bool) {}
            fn register_script_message_handler(
                &mut self,
                _name: &str,
                _handler: renderer::ScriptMessageHandler,
            ) {
            }
            fn inject_script(&mut self, _script: &str) {}
            fn register_scheme_handler(
                &mut self,
                _scheme: &str,
                _handler: renderer::SchemeHandler,
            ) {
            }
            fn trust_hooks(&self) -> renderer::TrustHooks {
                $hooks
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::TrustHook;

    fn measured_pages() -> Vec<PageResult> {
        vec![
            PageResult {
                page: "article".into(),
                rendered: true,
            },
            PageResult {
                page: "blog-post".into(),
                rendered: true,
            },
        ]
    }

    fn sample_capability() -> CapabilityScore {
        CapabilityScore {
            pages: measured_pages(),
            tree_construction_rate: 1.0,
            core_css_rate: 0.96,
        }
    }

    fn sample_vs_wezig() -> VsWezigMeter {
        VsWezigMeter {
            tier: "T1".into(),
            capability_fraction: 1.0,
            rust: ArmSignals {
                effort_person_days: 12,
                code_volume_loc: 3200,
                dom_object_graph_friction: 7,
            },
            wezig: ArmSignals {
                effort_person_days: 18,
                code_volume_loc: 5100,
                dom_object_graph_friction: 4,
            },
        }
    }

    #[test]
    fn candidate_ids_are_stable_and_cover_the_three_exploration_paths() {
        assert_eq!(Candidate::OwnEngine.id(), "own-engine");
        assert_eq!(Candidate::Servo.id(), "servo");
        assert_eq!(Candidate::BlitzStyloAssembly.id(), "blitz-stylo-assembly");
        assert_eq!(Candidate::ALL.len(), 3);
    }

    #[test]
    fn capability_page_checklist_counts_and_gate() {
        let cap = sample_capability();
        assert_eq!(cap.pages_rendered(), 2);
        assert!(cap.all_pages_rendered());
        // A page that did not render drops the checklist gate.
        let mut with_gap = cap.clone();
        with_gap.pages.push(PageResult {
            page: "ipfs-site".into(),
            rendered: false,
        });
        assert_eq!(with_gap.pages_rendered(), 2);
        assert!(!with_gap.all_pages_rendered());
    }

    #[test]
    fn capability_wpt_bars_compare_against_the_t1_thresholds() {
        let cap = sample_capability();
        assert!(cap.tree_construction_meets_bar());
        assert!(cap.core_css_meets_bar());
        assert!(cap.meets_t1_capability());

        // Below either bar, or with an unrendered page, T1 capability is not met.
        let low_css = CapabilityScore {
            core_css_rate: 0.5,
            ..sample_capability()
        };
        assert!(!low_css.core_css_meets_bar());
        assert!(!low_css.meets_t1_capability());

        let low_tree = CapabilityScore {
            tree_construction_rate: 0.5,
            ..sample_capability()
        };
        assert!(!low_tree.tree_construction_meets_bar());
        assert!(!low_tree.meets_t1_capability());
    }

    #[test]
    fn empty_page_checklist_never_passes_the_gate() {
        // An accidentally-empty checklist must not read as "all pages rendered".
        let cap = CapabilityScore {
            pages: Vec::new(),
            tree_construction_rate: 1.0,
            core_css_rate: 1.0,
        };
        assert!(!cap.all_pages_rendered());
        assert!(!cap.meets_t1_capability());
    }

    #[test]
    fn trust_hook_score_reuses_the_qualify_gate_pass() {
        // A backend that declares both hooks qualifies (pass), with nothing missing.
        struct Both;
        impl_stub_renderer!(Both, renderer::TrustHooks::all());
        let score = TrustHookScore::qualify_backend(&Both);
        assert!(score.qualifies);
        assert!(score.missing.is_empty());
    }

    #[test]
    fn trust_hook_score_reuses_the_qualify_gate_fail_naming_missing_hooks() {
        // A render-only backend fails the SAME gate, naming BOTH missing hooks — a
        // pass/fail qualification, not a graded score.
        struct RenderOnly;
        impl_stub_renderer!(RenderOnly, renderer::TrustHooks::none());
        let score = TrustHookScore::qualify_backend(&RenderOnly);
        assert!(!score.qualifies);
        assert_eq!(
            score.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme]
        );
    }

    #[test]
    fn vs_wezig_meter_surfaces_the_dom_friction_delta() {
        let meter = sample_vs_wezig();
        // Rust carries 7 vs wezig's 4: a positive delta is the "Rust carries more
        // DOM object-graph friction" signal, honestly surfaced.
        assert_eq!(meter.dom_friction_delta(), 3);
    }

    #[test]
    fn report_row_lookup_and_json_is_stable_and_diffable() {
        let report = BenchmarkReport {
            candidates: vec![CandidateReport {
                candidate: Candidate::BlitzStyloAssembly,
                scoring: CandidateScoring::Measured,
                capability: sample_capability(),
                trust_hooks: TrustHookScore {
                    qualifies: false,
                    missing: vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme],
                },
                vs_wezig: sample_vs_wezig(),
            }],
        };
        assert!(report.row(Candidate::BlitzStyloAssembly).is_some());
        assert!(report.row(Candidate::Servo).is_none());

        let json = report.to_json();
        // The JSON is deterministic: re-serialising the same report is byte-equal.
        assert_eq!(json, report.to_json());
        // It carries the load-bearing fields the exploration spec reads.
        assert!(json.contains("\"candidate\": \"blitz-stylo-assembly\""));
        assert!(json.contains("\"scoring\": \"measured\""));
        assert!(json.contains("\"qualifies\": false"));
        assert!(json.contains("\"provider-injection\""));
        assert!(json.contains("\"ipfs-scheme\""));
        assert!(json.contains("\"tree_construction_rate\": 1.0000"));
        assert!(json.contains("\"dom_friction_delta\": 3"));
    }

    #[test]
    fn score_page_checklist_renders_real_pages_through_the_native_path() {
        // A real semantic page renders (Finished + painted runs); an empty document
        // paints nothing and is not counted as rendered — the pass/fail per page.
        let pages = vec![
            ChecklistPage {
                name: "article".into(),
                html: "<!doctype html><html><body><h1>Real</h1><p>page <em>text</em></p>\
                       </body></html>"
                    .into(),
            },
            ChecklistPage {
                name: "empty".into(),
                html: "<!doctype html><html><head></head><body></body></html>".into(),
            },
        ];
        let results = score_page_checklist(&pages);
        assert_eq!(results.len(), 2);
        assert!(results[0].rendered, "a real page renders through the seam");
        assert!(!results[1].rendered, "an empty document paints no runs");
        // Deterministic: re-scoring yields the identical results.
        assert_eq!(results, score_page_checklist(&pages));
    }

    #[test]
    fn declared_candidate_is_an_honest_not_yet_built_slot() {
        // A declared candidate is a comparable placeholder: no page rendered, zero
        // WPT, does not qualify (both hooks missing), tagged Declared so it is never
        // confused with measured evidence.
        let row = declared_candidate(Candidate::Servo, "T1");
        assert_eq!(row.scoring, CandidateScoring::Declared);
        assert!(row.capability.pages.is_empty());
        assert!(!row.capability.meets_t1_capability());
        assert!(!row.trust_hooks.qualifies);
        assert_eq!(
            row.trust_hooks.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme]
        );
        assert_eq!(row.vs_wezig.tier, "T1");
    }
}
