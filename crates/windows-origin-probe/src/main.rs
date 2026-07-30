//! The probe's entry point. See `lib.rs` for what it measures and why.
//!
//! Two modes, because WebView2 fixes a browser process's custom-scheme
//! registrations for its lifetime and the two cases register different sets:
//! the normal invocation runs each case as a CHILD of itself and aggregates,
//! and `--case <a|b>` is that child.

use std::process::ExitCode;

use windows_origin_probe::cli::{Command, USAGE};

fn main() -> ExitCode {
    let command = match Command::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        #[cfg(windows)]
        Command::Case(case) => measure::run_one_case(case),
        #[cfg(windows)]
        Command::Both { expected, out } => measure::run_both_cases(expected, out),
        // The probe measures a WebView2 runtime, so on any other host it
        // refuses loudly rather than reporting an unmeasured verdict. It still
        // COMPILES everywhere, which is what keeps the pure half (the decision
        // rule, the canned site, the CLI) inside the Ubuntu `verify` gate.
        #[cfg(not(windows))]
        Command::Case(_) | Command::Both { .. } => {
            eprintln!(
                "windows-origin-probe measures a real WebView2 runtime and only runs on Windows.\n\
                 Run it on a `windows-latest` GitHub runner via\n\
                 .github/workflows/windows-origin-probe.yml, or on a Windows box with\n\
                 `cargo run -p windows-origin-probe`."
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
mod measure {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;

    use windows_origin_probe::cli::FACTS_MARKER;
    use windows_origin_probe::facts::{
        mechanism_from, CaseFacts, CaseId, Expectations, Mechanism, Report,
    };
    use windows_origin_probe::page;
    use windows_origin_probe::win;

    pub fn run_one_case(case: CaseId) -> ExitCode {
        let facts = win::run_case(case, page::CID);
        let json = serde_json::to_string(&facts).expect("CaseFacts serializes");
        // stdout carries exactly one machine-readable line; everything else the
        // runtime prints is noise the parent skips.
        println!("{FACTS_MARKER}{json}");
        let _ = std::io::stdout().flush();
        ExitCode::SUCCESS
    }

    pub fn run_both_cases(expected: Option<PathBuf>, out: Option<PathBuf>) -> ExitCode {
        let runtime = match win::runtime_version() {
            Ok(version) => version,
            Err(message) => {
                // Not a verdict: a machine with no runtime has measured nothing.
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        };
        eprintln!("WebView2 Runtime: {runtime}");

        let report = Report {
            webview2_runtime_version: runtime,
            cid: page::CID.to_string(),
            case_a: spawn_case(CaseId::A),
            case_b: spawn_case(CaseId::B),
            case_control: spawn_case(CaseId::Control),
        };

        let rendered = serde_json::to_string_pretty(&report).expect("Report serializes");
        if let Some(path) = &out {
            if let Err(error) = std::fs::write(path, format!("{rendered}\n")) {
                eprintln!("could not write {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
            eprintln!("report written to {}", path.display());
        }
        println!("{rendered}");
        print_summary(&report);

        let Some(path) = expected else {
            // The recording run: report the measurement, do not judge it.
            return ExitCode::SUCCESS;
        };
        match load_expectations(&path) {
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
            Ok(expectations) => {
                let differences = expectations.diff(&report);
                if differences.is_empty() {
                    eprintln!(
                        "\nthe recorded verdict still holds on this runtime: {}",
                        expectations.mechanism.as_str()
                    );
                    ExitCode::SUCCESS
                } else {
                    eprintln!(
                        "\nthe WebView2 runtime no longer behaves as recorded in {}:",
                        path.display()
                    );
                    for difference in &differences {
                        eprintln!("  - {difference}");
                    }
                    eprintln!(
                        "\nThis is the evergreen-runtime regression this probe exists to catch.\n\
                         Re-read the report above, then either fix the shell's mechanism or\n\
                         re-record the verdict with the reason."
                    );
                    ExitCode::FAILURE
                }
            }
        }
    }

    fn load_expectations(path: &Path) -> Result<Expectations, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|error| format!("could not parse {}: {error}", path.display()))
    }

    /// Run one case in a child of this process and read back its facts.
    fn spawn_case(case: CaseId) -> CaseFacts {
        let mut facts = CaseFacts {
            page_url: page::case_page_url(case, page::CID),
            ..CaseFacts::default()
        };
        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                facts.harness_error = Some(format!("could not locate this executable: {error}"));
                return facts;
            }
        };
        eprintln!("running case {} ...", case.as_str());
        let output = match std::process::Command::new(executable)
            .args(["--case", case.as_str()])
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                facts.harness_error =
                    Some(format!("could not run case {}: {error}", case.as_str()));
                return facts;
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprintln!("case {} stderr:\n{stderr}", case.as_str());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let Some(line) = stdout
            .lines()
            .find_map(|line| line.strip_prefix(FACTS_MARKER))
        else {
            facts.harness_error = Some(format!(
                "case {} reported no facts (exit {}); stdout was:\n{stdout}",
                case.as_str(),
                output.status
            ));
            return facts;
        };
        match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                facts.harness_error =
                    Some(format!("case {} facts unparseable: {error}", case.as_str()));
                facts
            }
        }
    }

    /// The human-readable half of the artifact: every measured fact per case,
    /// side by side, then the mechanism they imply.
    fn print_summary(report: &Report) {
        eprintln!("\n=== werust Windows origin probe ===");
        eprintln!("WebView2 Runtime: {}", report.webview2_runtime_version);
        for (case, facts) in [
            (CaseId::A, &report.case_a),
            (CaseId::B, &report.case_b),
            (CaseId::Control, &report.case_control),
        ] {
            eprintln!("\n-- case {} ({}) --", case.as_str(), case.label());
            eprintln!("  page:                {}", facts.page_url);
            eprintln!("  navigation:          {}", facts.navigation);
            eprintln!("  origin:              {:?}", facts.origin);
            eprintln!("  secure context:      {}", facts.secure_context);
            eprintln!("  fetch:               {}", facts.fetch);
            eprintln!("  fetch handler fired: {}", facts.fetch_handler_fired);
            eprintln!("  pushState:           {}", facts.push_state);
            eprintln!("  module script:       {}", facts.module_script);
            eprintln!("  css font handler:    {}", facts.css_font_handler_fired);
            eprintln!("  service worker:      {}", facts.service_worker);
            eprintln!("  intercepted:         {:#?}", facts.handler_uris);
            eprintln!("  console:             {:#?}", facts.console);
            if let Some(error) = &facts.harness_error {
                eprintln!("  HARNESS ERROR:       {error}");
            }
        }
        eprintln!();
        match mechanism_from(report) {
            Ok(Mechanism::RegisteredIpfsScheme) => eprintln!(
                "VERDICT: registered-ipfs-scheme — a WebView2-registered `ipfs://` scheme serves a\n\
                 real tuple origin, so a Windows shell can serve `ipfs://` like desktop and iOS."
            ),
            Ok(Mechanism::InternalHttpsOrigin) => eprintln!(
                "VERDICT: internal-https-origin — the registered `ipfs://` scheme does NOT serve a\n\
                 client-side navigation, so a Windows shell maps URLs exactly as Android does and\n\
                 `origin_map.rs` is promoted from an Android module to a shared one."
            ),
            Err(why) => {
                eprintln!("NO VERDICT: neither case served a client-side navigation.\n{why}")
            }
        }
    }
}
