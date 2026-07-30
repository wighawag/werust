//! The probe's HOST-INDEPENDENT half: the facts each case measures, the rule
//! that turns them into a verdict, and the comparison against the recorded
//! expectations.
//!
//! Everything in this module is pure and compiles on every host, so the repo's
//! Ubuntu `verify` gate exercises the decision rule even though it can never run
//! WebKit. The macOS half that actually produces a [`CaseFacts`] lives in
//! [`crate::mac`] behind `#[cfg(target_os = "macos")]`, mirroring how
//! `crates/windows-origin-probe` keeps its WebView2 layer target-gated.

use serde::{Deserialize, Serialize};

/// Which `ipfs://` serving mechanism a WebKit shell (macOS AND iOS) uses.
///
/// The SAME vocabulary `crates/windows-origin-probe` decided in, deliberately:
/// the question "does the platform give a handler-served document a real tuple
/// origin, or must werust map it onto an internal `https` origin like Android
/// does?" is one cross-platform question, and its two answers should not be named
/// twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    /// The page is served from `ipfs://<cid>/` by a `WKURLSchemeHandler`
    /// REGISTERED for the scheme, and the document sees the real `ipfs://<cid>` origin
    /// -- what desktop Linux (WebKitGTK) and Windows (WebView2) already do, and
    /// what the iOS shell already ships on.
    RegisteredIpfsScheme,
    /// The internal `https://<cid>.ipfs.werust.invalid` origin that
    /// `crates/werust-android/rust/src/origin_map.rs` implements. On WebKit this
    /// is NOT available (see [`Report::https_is_handled_natively`]), so a verdict
    /// can never legitimately land here on this platform -- it exists in the enum
    /// because it is the other answer to the shared question, and a probe that
    /// could not NAME the failing outcome would be hiding it.
    InternalHttpsOrigin,
}

impl Mechanism {
    /// The wire/report spelling, also what `expected.json` carries.
    #[must_use]
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
    /// The page served from `ipfs://<cid>/` by the registered
    /// `WKURLSchemeHandler`.
    A,
    /// The NEGATIVE CONTROL: the IDENTICAL bytes and page loaded with
    /// `loadHTMLString:baseURL:` and a NIL base URL, which WebKit gives an OPAQUE
    /// origin -- with the SAME handler still registered on the SAME webview, so
    /// "the handler never fired" is a measured difference rather than an absence.
    /// It measures no mechanism and can never decide the verdict; it exists so a
    /// PASSING case A means something. A probe that reports success for every
    /// case is not measuring anything, and this is the run that would catch that.
    Control,
}

impl CaseId {
    /// Every case a full run measures, in order.
    pub const ALL: [CaseId; 2] = [CaseId::A, CaseId::Control];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CaseId::A => "A",
            CaseId::Control => "control",
        }
    }

    /// What the case is a measurement of, for the report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CaseId::A => "WKURLSchemeHandler-served ipfs://",
            CaseId::Control => "negative control: the same bytes with NO handler-served origin",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "a" => Some(CaseId::A),
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
    /// `document.location.origin` as the PAGE sees it: a tuple origin
    /// (`ipfs://<cid>`) or the opaque `"null"`.
    pub origin: String,
    /// `window.isSecureContext`.
    pub secure_context: bool,
    /// `navigator.userAgent`, so a measured result always names the WebKit build
    /// it came from. Diagnostic only; never decides the verdict.
    pub user_agent: String,
    /// The SvelteKit-shaped same-origin `fetch`: `ok:<status>` or
    /// `reject:<ErrorName>`.
    pub fetch: String,
    /// Did the registered `WKURLSchemeHandler` actually FIRE for that fetch?
    /// Both halves matter: a fetch that resolves without the handler firing is
    /// not werust serving it, and a handler that never fires is the Android
    /// failure's signature.
    pub fetch_handler_fired: bool,
    /// `history.pushState({}, '', '/blog/')`: `ok:<pathname>` or
    /// `throw:<ErrorName>` (Android's opaque origin throws `SecurityError`).
    pub push_state: String,
    /// Informational: a `<script type="module">`-shaped dynamic `import()`.
    pub module_script: String,
    /// Informational: whether the handler was ASKED for a CSS `@font-face`
    /// `url()` subresource. Only "was it asked" is meaningful; the canned font
    /// bytes are deliberately not a real font.
    pub css_font_handler_fired: bool,
    /// Informational: `navigator.serviceWorker.register('/sw.js')`, the
    /// per-origin difference recorded in
    /// `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`.
    pub service_worker: String,
    /// Every URI the scheme handler was asked about, in order (the would-be debug
    /// Network tab).
    pub handler_uris: Vec<String>,
    /// Set when the harness itself failed rather than the mechanism (no window
    /// server, the page never reported, ...).
    pub harness_error: Option<String>,
}

impl CaseFacts {
    /// Does this case satisfy everything a SvelteKit client-side navigation
    /// needs? Exactly the four host-visible facts, and nothing else: the
    /// informational rows never decide the mechanism.
    #[must_use]
    pub fn serves_a_client_side_navigation(&self, expected_origin: &str) -> bool {
        self.harness_error.is_none()
            && self.origin == expected_origin
            && self.fetch == "ok:200"
            && self.fetch_handler_fired
            && self.push_state == "ok:/blog/"
    }

    /// Why the case did not qualify, for the report.
    #[must_use]
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
            out.push("the WKURLSchemeHandler never fired for the fetch".to_string());
        }
        if self.push_state != "ok:/blog/" {
            out.push(format!("pushState did not succeed: {}", self.push_state));
        }
        out
    }
}

/// The whole probe run: both cases, plus the WebKit build they were measured
/// against and the measured reason there is no case B.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// The OS build and the WebKit user-agent the run measured, so a result is
    /// never reported without saying what it was measured on.
    pub os_version: String,
    pub webkit_user_agent: String,
    /// MEASURED (`+[WKWebView handlesURLScheme:@"https"]`), not read from the
    /// documentation: WebKit handles `https` itself and will not give it to a
    /// `WKURLSchemeHandler`. This is WHY there is no case B on WebKit -- the
    /// Android/Windows internal-`https` fallback simply cannot be built here, so
    /// case A is the only mechanism.
    pub https_is_handled_natively: bool,
    /// The CID every case serves, so the expected origin is reconstructable.
    pub cid: String,
    pub case_a: CaseFacts,
    pub case_control: CaseFacts,
}

/// The recorded verdict a re-run is checked against. Only the load-bearing facts
/// are pinned: a re-run that changes any of them means WebKit moved under us,
/// which is precisely the regression this probe exists to catch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expectations {
    /// Free-text provenance (when it was recorded, on which OS/WebKit).
    pub recorded: String,
    pub mechanism: Mechanism,
    pub case_a: CaseExpectation,
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
    /// verdict still holds on this runner's WebKit.
    #[must_use]
    pub fn diff(&self, report: &Report) -> Vec<String> {
        let mut out = self.case_a.diff(CaseId::A, &report.case_a);
        out.extend(
            self.case_control
                .diff(CaseId::Control, &report.case_control),
        );
        // The control is the probe's own falsification guard: if the run that is
        // SUPPOSED to fail starts passing, the harness has stopped discriminating
        // and no other line in this report can be trusted.
        if report
            .case_control
            .serves_a_client_side_navigation(&crate::page::case_origin(CaseId::A, &report.cid))
        {
            out.push(
                "the negative control served a client-side navigation: the probe is no longer \
                 able to detect the failure it exists to detect"
                    .to_string(),
            );
        }
        match verdict_from(report) {
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

/// The DECISION RULE. Case A passing every check means a WebKit shell serves real
/// `ipfs://` origins like desktop Linux, Windows and (by the same mechanism) iOS.
/// Case A failing ANY of them is a genuine BLOCKER on WebKit rather than a
/// fallback, because the internal-`https` mechanism Android uses cannot be built
/// on a `WKURLSchemeHandler` at all -- so the probe says so instead of picking a
/// mechanism it has not measured.
pub fn verdict_from(report: &Report) -> Result<Mechanism, String> {
    let expected = crate::page::case_origin(CaseId::A, &report.cid);
    if report.case_a.serves_a_client_side_navigation(&expected) {
        return Ok(Mechanism::RegisteredIpfsScheme);
    }
    let mut why = report.case_a.shortfalls(&expected).join("; ");
    if report.https_is_handled_natively {
        why.push_str(
            "; and the Android/Windows internal-https fallback is NOT available here: WebKit \
             handles `https` itself and refuses to give it to a WKURLSchemeHandler",
        );
    }
    Err(why)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{case_origin, CID};

    fn passing_case_a() -> CaseFacts {
        CaseFacts {
            page_url: format!("{}/", case_origin(CaseId::A, CID)),
            navigation: "completed:success".to_string(),
            origin: case_origin(CaseId::A, CID),
            secure_context: true,
            fetch: "ok:200".to_string(),
            fetch_handler_fired: true,
            push_state: "ok:/blog/".to_string(),
            ..CaseFacts::default()
        }
    }

    /// The control as it must measure on a working WebKit: an opaque origin where
    /// the client navigation dies -- the Android failure shape.
    fn failing_control() -> CaseFacts {
        CaseFacts {
            page_url: "about:blank".to_string(),
            navigation: "completed:success".to_string(),
            origin: "null".to_string(),
            secure_context: false,
            fetch: "reject:TypeError".to_string(),
            fetch_handler_fired: false,
            push_state: "throw:SecurityError".to_string(),
            ..CaseFacts::default()
        }
    }

    fn report(case_a: CaseFacts) -> Report {
        Report {
            os_version: "14.7.0".to_string(),
            webkit_user_agent: "Mozilla/5.0 (Macintosh; …) AppleWebKit/605.1.15".to_string(),
            https_is_handled_natively: true,
            cid: CID.to_string(),
            case_a,
            case_control: failing_control(),
        }
    }

    fn expectations() -> Expectations {
        Expectations {
            recorded: "test".to_string(),
            mechanism: Mechanism::RegisteredIpfsScheme,
            case_a: CaseExpectation {
                origin: case_origin(CaseId::A, CID),
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
    fn case_a_passing_every_check_picks_the_handler_served_ipfs_scheme() {
        assert_eq!(
            verdict_from(&report(passing_case_a())),
            Ok(Mechanism::RegisteredIpfsScheme)
        );
    }

    /// The Android failure shape, if WebKit ever produced it: an opaque origin,
    /// the fetch rejected inside the engine, `pushState` throwing.
    #[test]
    fn an_opaque_origin_in_case_a_is_a_blocker_not_a_fallback() {
        let mut a = passing_case_a();
        a.origin = "null".to_string();
        a.fetch = "reject:TypeError".to_string();
        a.fetch_handler_fired = false;
        a.push_state = "throw:SecurityError".to_string();
        let why = verdict_from(&report(a)).expect_err("must not invent a verdict");
        assert!(why.contains("origin is"), "{why}");
        // And it must SAY that the Android fallback is unavailable here, rather
        // than quietly implying werust could just map the origin like Android.
        assert!(
            why.contains("internal-https fallback is NOT available"),
            "{why}"
        );
    }

    /// The signature werust cares about most: a real tuple origin and a working
    /// `pushState`, but a `fetch` that never reaches the handler. werust needs
    /// BOTH, so this still fails case A.
    #[test]
    fn a_fetch_that_never_reaches_the_handler_fails_case_a_even_if_it_resolves() {
        let mut a = passing_case_a();
        a.fetch_handler_fired = false;
        let why = verdict_from(&report(a)).expect_err("must not invent a verdict");
        assert!(why.contains("never fired"), "{why}");
    }

    #[test]
    fn the_informational_rows_do_not_decide_the_mechanism() {
        let mut a = passing_case_a();
        a.module_script = "reject:TypeError".to_string();
        a.css_font_handler_fired = false;
        a.service_worker = "reject:SecurityError".to_string();
        assert_eq!(
            verdict_from(&report(a)),
            Ok(Mechanism::RegisteredIpfsScheme)
        );
    }

    #[test]
    fn the_negative_control_does_not_decide_the_mechanism() {
        let mut report = report(passing_case_a());
        report.case_control = passing_case_a();
        assert_eq!(verdict_from(&report), Ok(Mechanism::RegisteredIpfsScheme));
    }

    /// ...but a control that starts PASSING invalidates the whole run, because it
    /// means the probe can no longer tell a working origin from a broken one.
    #[test]
    fn a_control_that_starts_passing_fails_the_run_as_a_non_discriminating_probe() {
        let mut report = report(passing_case_a());
        report.case_control = passing_case_a();
        let diff = expectations().diff(&report);
        assert!(
            diff.iter().any(|d| d.contains("no longer able to detect")),
            "{diff:?}"
        );
    }

    #[test]
    fn a_run_matching_the_recorded_expectations_diffs_clean() {
        assert_eq!(
            expectations().diff(&report(passing_case_a())),
            Vec::<String>::new()
        );
    }

    /// The whole point of keeping the probe re-runnable: when WebKit moves this
    /// corner, the recorded verdict stops matching and the job goes red with the
    /// field that moved.
    #[test]
    fn a_webkit_regression_shows_up_as_a_named_field() {
        let mut a = passing_case_a();
        a.fetch = "reject:TypeError".to_string();
        a.fetch_handler_fired = false;
        let diff = expectations().diff(&report(a));
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
                .any(|d| d.contains("no mechanism could be derived")),
            "{diff:?}"
        );
    }

    #[test]
    fn a_harness_failure_is_not_a_verdict() {
        let mut a = passing_case_a();
        a.harness_error = Some("no window server".to_string());
        let why = verdict_from(&report(a)).expect_err("must not invent a verdict");
        assert!(why.contains("no window server"), "{why}");
    }
}
