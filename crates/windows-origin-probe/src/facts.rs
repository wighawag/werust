//! The probe's HOST-INDEPENDENT half: the facts each case measures, the rule
//! that turns two cases into a serving-mechanism verdict, and the comparison
//! against the recorded expectations.
//!
//! Everything in this module is pure and compiles on every host, so the
//! repo's Ubuntu `verify` gate exercises the decision rule even though it can
//! never run WebView2. The Windows half that actually produces a [`CaseFacts`]
//! lives in [`crate::win`] behind `#[cfg(windows)]`, mirroring how
//! `crates/werust-android/rust` keeps its JNI layer target-gated.

use serde::{Deserialize, Serialize};

/// Which of the two candidate `ipfs://` serving mechanisms a Windows shell
/// would use. This is the whole deliverable of gate 0
/// (`docs/adr/0011-webview2-for-windows.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    /// Case A: `ipfs://` registered with `ICoreWebView2CustomSchemeRegistration`
    /// (`HasAuthorityComponent` + `TreatAsSecure`), so the document sees the
    /// real `ipfs://<cid>` origin — what desktop (WebKitGTK) and iOS already do.
    RegisteredIpfsScheme,
    /// Case B: the internal `https://<cid>.ipfs.werust.invalid` origin that
    /// `crates/werust-android/rust/src/origin_map.rs` already implements (and
    /// that `wry` ships on Windows as `custom_protocol_workaround.rs`). Picking
    /// this promotes `origin_map.rs` from an Android module to a shared one.
    InternalHttpsOrigin,
}

impl Mechanism {
    /// The wire/report spelling, also what `expected.json` carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Mechanism::RegisteredIpfsScheme => "registered-ipfs-scheme",
            Mechanism::InternalHttpsOrigin => "internal-https-origin",
        }
    }
}

/// Which case a [`CaseFacts`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseId {
    /// The real registered `ipfs://` scheme.
    A,
    /// The internal `https://<cid>.ipfs.werust.invalid` origin.
    B,
    /// The NEGATIVE CONTROL: the same registered `ipfs://` scheme with
    /// `HasAuthorityComponent = false`, which Microsoft documents as giving an
    /// opaque origin "similar to a data URI". It measures no mechanism and can
    /// never decide the verdict; it exists so a PASSING case A means something.
    /// A probe that reports success for every case is not measuring anything,
    /// and this is the run that would catch that.
    Control,
}

impl CaseId {
    /// Every case a full run measures, in order.
    pub const ALL: [CaseId; 3] = [CaseId::A, CaseId::B, CaseId::Control];

    pub fn as_str(self) -> &'static str {
        match self {
            CaseId::A => "A",
            CaseId::B => "B",
            CaseId::Control => "control",
        }
    }

    /// What the case is a measurement of, for the report.
    pub fn label(self) -> &'static str {
        match self {
            CaseId::A => "registered-ipfs-scheme",
            CaseId::B => "internal-https-origin",
            CaseId::Control => "negative control: registered ipfs:// WITHOUT HasAuthorityComponent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "a" => Some(CaseId::A),
            "b" => Some(CaseId::B),
            "control" => Some(CaseId::Control),
            _ => None,
        }
    }
}

/// Everything ONE case measured. The first five fields are the ones the task's
/// acceptance criteria name; the rest are the diagnostic context the Android
/// probe's `DIAGNOSIS.md` proved you want when a case fails.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseFacts {
    /// The URL the document was served from.
    pub page_url: String,
    /// Did the top-level navigation complete successfully at all?
    pub navigation: String,
    /// `document.location.origin` as the PAGE sees it. A tuple origin
    /// (`ipfs://<cid>` / `https://<cid>.ipfs.werust.invalid`) or the opaque
    /// `"null"`.
    pub origin: String,
    /// `window.isSecureContext` (what `TreatAsSecure` is supposed to buy).
    pub secure_context: bool,
    /// The SvelteKit-shaped same-origin `fetch`: `ok:<status>` or
    /// `reject:<ErrorName>`.
    pub fetch: String,
    /// Did `WebResourceRequested` FIRE for that fetch? Both halves matter: a
    /// fetch that resolves without the handler firing is not werust serving it,
    /// and a handler that never fires is [WebView2Feedback #4328]'s signature.
    ///
    /// [WebView2Feedback #4328]: https://github.com/MicrosoftEdge/WebView2Feedback/issues/4328
    pub fetch_handler_fired: bool,
    /// `history.pushState({}, '', '/blog/')`: `ok:<pathname>` or
    /// `throw:<ErrorName>` (Android's opaque origin throws `SecurityError`).
    pub push_state: String,
    /// Informational: a `<script type="module">`-shaped dynamic `import()`.
    pub module_script: String,
    /// Informational: a CSS `@font-face` `url()` subresource — the
    /// [#4362](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4362)
    /// shape. Only whether the handler was ASKED is meaningful; the canned font
    /// bytes are deliberately not a real font.
    pub css_font_handler_fired: bool,
    /// Informational: `navigator.serviceWorker.register('/sw.js')`, the
    /// per-origin difference recorded in
    /// `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`.
    pub service_worker: String,
    /// Every URI `WebResourceRequested` was asked about, in order (the
    /// would-be debug Network tab).
    pub handler_uris: Vec<String>,
    /// Every console/Log entry Blink emitted (the only signal Android's opaque
    /// origin left behind).
    pub console: Vec<String>,
    /// Set when the harness itself failed rather than the mechanism (no
    /// runtime, environment creation refused, the page never reported).
    pub harness_error: Option<String>,
}

impl CaseFacts {
    /// Does this case satisfy everything a SvelteKit client-side navigation
    /// needs? Exactly the four host-visible facts the ADR names, and nothing
    /// else: the informational rows never decide the mechanism.
    pub fn serves_a_client_side_navigation(&self, expected_origin: &str) -> bool {
        self.harness_error.is_none()
            && self.origin == expected_origin
            && self.fetch == "ok:200"
            && self.fetch_handler_fired
            && self.push_state == "ok:/blog/"
    }

    /// Why the case did not qualify, for the report.
    pub fn shortfalls(&self, expected_origin: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(err) = &self.harness_error {
            out.push(format!("the harness failed: {err}"));
            return out;
        }
        if self.origin != expected_origin {
            out.push(format!(
                "origin is {:?}, not the expected tuple origin {:?}",
                self.origin, expected_origin
            ));
        }
        if self.fetch != "ok:200" {
            out.push(format!(
                "the same-origin fetch did not resolve: {}",
                self.fetch
            ));
        }
        if !self.fetch_handler_fired {
            out.push("WebResourceRequested never fired for the fetch".to_string());
        }
        if self.push_state != "ok:/blog/" {
            out.push(format!("pushState did not succeed: {}", self.push_state));
        }
        out
    }
}

/// The whole probe run: both cases plus the runtime version they were measured
/// against. The runtime is EVERGREEN and this corner regressed in stable 144 in
/// January 2026, so a result without its version is not a result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// `GetAvailableCoreWebView2BrowserVersionString`, e.g. `"150.0.4078.65"`.
    pub webview2_runtime_version: String,
    /// The CID every case serves, so the expected origins are reconstructable.
    pub cid: String,
    pub case_a: CaseFacts,
    pub case_b: CaseFacts,
    pub case_control: CaseFacts,
}

/// The recorded verdict a re-run is checked against. Only the load-bearing
/// facts are pinned: a re-run that changes any of them means the evergreen
/// runtime moved under us, which is precisely the regression this probe exists
/// to catch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expectations {
    /// Free-text provenance (when it was recorded, on which runtime).
    pub recorded: String,
    pub mechanism: Mechanism,
    pub case_a: CaseExpectation,
    pub case_b: CaseExpectation,
    pub case_control: CaseExpectation,
}

/// The pinned subset of one case's facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseExpectation {
    pub origin: String,
    pub secure_context: bool,
    pub fetch: String,
    pub fetch_handler_fired: bool,
    pub push_state: String,
}

impl CaseExpectation {
    fn diff(&self, id: CaseId, actual: &CaseFacts) -> Vec<String> {
        let case = id.as_str();
        let mut out = Vec::new();
        if let Some(err) = &actual.harness_error {
            out.push(format!("case {case}: the harness failed: {err}"));
            return out;
        }
        let mut check = |field: &str, expected: String, got: String| {
            if expected != got {
                out.push(format!(
                    "case {case}: {field}: expected {expected}, got {got}"
                ));
            }
        };
        check(
            "origin",
            format!("{:?}", self.origin),
            format!("{:?}", actual.origin),
        );
        check(
            "secure_context",
            self.secure_context.to_string(),
            actual.secure_context.to_string(),
        );
        check(
            "fetch",
            format!("{:?}", self.fetch),
            format!("{:?}", actual.fetch),
        );
        check(
            "fetch_handler_fired",
            self.fetch_handler_fired.to_string(),
            actual.fetch_handler_fired.to_string(),
        );
        check(
            "push_state",
            format!("{:?}", self.push_state),
            format!("{:?}", actual.push_state),
        );
        out
    }
}

impl Expectations {
    /// Every way [`Report`] departs from what was recorded. Empty means the
    /// verdict still holds on this runner's runtime.
    pub fn diff(&self, report: &Report) -> Vec<String> {
        let mut out = self.case_a.diff(CaseId::A, &report.case_a);
        out.extend(self.case_b.diff(CaseId::B, &report.case_b));
        out.extend(
            self.case_control
                .diff(CaseId::Control, &report.case_control),
        );
        // The control is the probe's own falsification guard: if the run that is
        // SUPPOSED to fail starts passing, the harness has stopped
        // discriminating and no other line in this report can be trusted.
        let control_origin = crate::page::case_origin(CaseId::Control, &report.cid);
        if report
            .case_control
            .serves_a_client_side_navigation(&control_origin)
        {
            out.push(
                "the negative control served a client-side navigation: the probe is no longer \
                 able to detect the failure it exists to detect"
                    .to_string(),
            );
        }
        match mechanism_from(report) {
            Ok(measured) if measured != self.mechanism => out.push(format!(
                "the measured mechanism is {}, but {} was recorded",
                measured.as_str(),
                self.mechanism.as_str()
            )),
            Err(why) => out.push(format!("no mechanism could be derived: {why}")),
            Ok(_) => {}
        }
        out
    }
}

/// The DECISION RULE, straight out of ADR-0011: case A passing every check
/// means Windows serves real `ipfs://` origins like desktop and iOS; case A
/// failing ANY of them means Windows uses case B and `origin_map.rs` gets
/// promoted to a shared module. If neither case can serve a client-side
/// navigation, the probe has measured nothing usable and must say so rather
/// than pick a mechanism.
pub fn mechanism_from(report: &Report) -> Result<Mechanism, String> {
    let a_origin = crate::page::case_origin(CaseId::A, &report.cid);
    let b_origin = crate::page::case_origin(CaseId::B, &report.cid);
    if report.case_a.serves_a_client_side_navigation(&a_origin) {
        return Ok(Mechanism::RegisteredIpfsScheme);
    }
    if report.case_b.serves_a_client_side_navigation(&b_origin) {
        return Ok(Mechanism::InternalHttpsOrigin);
    }
    Err(format!(
        "case A: {}; case B: {}",
        report.case_a.shortfalls(&a_origin).join("; "),
        report.case_b.shortfalls(&b_origin).join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{case_origin, CID};

    fn passing(id: CaseId) -> CaseFacts {
        CaseFacts {
            page_url: format!("{}/", case_origin(id, CID)),
            navigation: "completed:success".to_string(),
            origin: case_origin(id, CID),
            secure_context: true,
            fetch: "ok:200".to_string(),
            fetch_handler_fired: true,
            push_state: "ok:/blog/".to_string(),
            ..CaseFacts::default()
        }
    }

    /// The control as it actually measures on a working runtime: an opaque
    /// origin where the client navigation dies (the Android failure shape).
    fn failing_control() -> CaseFacts {
        CaseFacts {
            page_url: format!("ipfs://{CID}/"),
            navigation: "completed:success".to_string(),
            origin: "null".to_string(),
            secure_context: false,
            fetch: "reject:TypeError".to_string(),
            fetch_handler_fired: false,
            push_state: "throw:SecurityError".to_string(),
            ..CaseFacts::default()
        }
    }

    fn report(case_a: CaseFacts, case_b: CaseFacts) -> Report {
        Report {
            webview2_runtime_version: "150.0.4078.65".to_string(),
            cid: CID.to_string(),
            case_a,
            case_b,
            case_control: failing_control(),
        }
    }

    #[test]
    fn case_a_passing_every_check_picks_the_registered_ipfs_scheme() {
        let measured = mechanism_from(&report(passing(CaseId::A), passing(CaseId::B)));
        assert_eq!(measured, Ok(Mechanism::RegisteredIpfsScheme));
    }

    /// The Android failure shape: an opaque origin, the fetch rejected inside
    /// Blink, and `pushState` throwing `SecurityError`.
    #[test]
    fn an_opaque_origin_in_case_a_picks_the_internal_https_origin() {
        let mut a = passing(CaseId::A);
        a.origin = "null".to_string();
        a.fetch = "reject:TypeError".to_string();
        a.fetch_handler_fired = false;
        a.push_state = "throw:SecurityError".to_string();
        assert_eq!(
            mechanism_from(&report(a, passing(CaseId::B))),
            Ok(Mechanism::InternalHttpsOrigin)
        );
    }

    /// WebView2Feedback #4328's exact signature: the document gets a real tuple
    /// origin and `pushState` works, but the `fetch` never reaches
    /// `WebResourceRequested`. werust needs BOTH, so this still fails case A —
    /// the reason the acceptance criterion says "INCLUDING whether the
    /// interception handler fired".
    #[test]
    fn a_fetch_that_never_reaches_the_handler_fails_case_a_even_if_it_resolves() {
        let mut a = passing(CaseId::A);
        a.fetch_handler_fired = false;
        assert_eq!(
            mechanism_from(&report(a, passing(CaseId::B))),
            Ok(Mechanism::InternalHttpsOrigin)
        );
    }

    /// The informational rows are diagnostics, never grounds for a verdict.
    #[test]
    fn the_informational_rows_do_not_decide_the_mechanism() {
        let mut a = passing(CaseId::A);
        a.module_script = "reject:TypeError".to_string();
        a.css_font_handler_fired = false;
        a.service_worker = "reject:SecurityError".to_string();
        assert_eq!(
            mechanism_from(&report(a, passing(CaseId::B))),
            Ok(Mechanism::RegisteredIpfsScheme)
        );
    }

    /// The control never contributes to the verdict, however it behaves.
    #[test]
    fn the_negative_control_does_not_decide_the_mechanism() {
        let mut report = report(passing(CaseId::A), passing(CaseId::B));
        report.case_control = passing(CaseId::A);
        assert_eq!(mechanism_from(&report), Ok(Mechanism::RegisteredIpfsScheme));
    }

    /// ...but a control that starts PASSING invalidates the whole run, because
    /// it means the probe can no longer tell a working origin from a broken one.
    #[test]
    fn a_control_that_starts_passing_fails_the_run_as_a_non_discriminating_probe() {
        let expected = expectations(Mechanism::RegisteredIpfsScheme);
        let mut report = report(passing(CaseId::A), passing(CaseId::B));
        report.case_control = passing(CaseId::A);
        let diff = expected.diff(&report);
        assert!(
            diff.iter().any(|d| d.contains("no longer able to detect")),
            "{diff:?}"
        );
    }

    #[test]
    fn neither_case_serving_a_client_nav_is_an_error_not_a_mechanism() {
        let mut a = passing(CaseId::A);
        a.harness_error = Some("no WebView2 Runtime".to_string());
        let mut b = passing(CaseId::B);
        b.harness_error = Some("no WebView2 Runtime".to_string());
        let err = mechanism_from(&report(a, b)).expect_err("must not invent a verdict");
        assert!(err.contains("no WebView2 Runtime"), "{err}");
    }

    fn expectations(mechanism: Mechanism) -> Expectations {
        Expectations {
            recorded: "test".to_string(),
            mechanism,
            case_a: CaseExpectation {
                origin: case_origin(CaseId::A, CID),
                secure_context: true,
                fetch: "ok:200".to_string(),
                fetch_handler_fired: true,
                push_state: "ok:/blog/".to_string(),
            },
            case_b: CaseExpectation {
                origin: case_origin(CaseId::B, CID),
                secure_context: true,
                fetch: "ok:200".to_string(),
                fetch_handler_fired: true,
                push_state: "ok:/blog/".to_string(),
            },
            case_control: CaseExpectation {
                origin: "null".to_string(),
                secure_context: false,
                fetch: "reject:TypeError".to_string(),
                fetch_handler_fired: false,
                push_state: "throw:SecurityError".to_string(),
            },
        }
    }

    #[test]
    fn a_run_matching_the_recorded_expectations_diffs_clean() {
        let expected = expectations(Mechanism::RegisteredIpfsScheme);
        assert_eq!(
            expected.diff(&report(passing(CaseId::A), passing(CaseId::B))),
            Vec::<String>::new()
        );
    }

    /// The whole point of keeping the probe re-runnable: when the evergreen
    /// runtime moves this corner, the recorded verdict stops matching and the
    /// job goes red with the field that moved.
    #[test]
    fn a_runtime_regression_shows_up_as_a_named_field_and_a_changed_mechanism() {
        let expected = expectations(Mechanism::RegisteredIpfsScheme);
        let mut a = passing(CaseId::A);
        a.fetch = "reject:TypeError".to_string();
        a.fetch_handler_fired = false;
        let diff = expected.diff(&report(a, passing(CaseId::B)));
        assert!(
            diff.iter().any(|d| d.contains("case A: fetch:")),
            "{diff:?}"
        );
        assert!(
            diff.iter()
                .any(|d| d.contains("case A: fetch_handler_fired:")),
            "{diff:?}"
        );
        assert!(
            diff.iter()
                .any(|d| d.contains("measured mechanism is internal-https-origin")),
            "{diff:?}"
        );
    }
}
