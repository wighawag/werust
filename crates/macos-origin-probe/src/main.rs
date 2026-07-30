//! The probe's entry point. See `lib.rs` for what it measures and why.

use std::process::ExitCode;

use macos_origin_probe::cli::{Command, USAGE};

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
        #[cfg(target_os = "macos")]
        Command::Run { expected, out } => measure::run(expected, out),
        // The probe measures a real WebKit; on any other host it refuses loudly
        // rather than reporting an unmeasured verdict. It still COMPILES
        // everywhere, which is what keeps the pure half (the decision rule, the
        // canned site, the CLI) inside the Ubuntu `verify` gate.
        #[cfg(not(target_os = "macos"))]
        Command::Run { .. } => {
            eprintln!(
                "macos-origin-probe measures a real WKWebView and only runs on macOS.\n\
                 Run it on a `macos-14` GitHub runner via .github/workflows/macos-renderer.yml,\n\
                 or on a Mac with `cargo run -p macos-origin-probe`."
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
mod measure {
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;

    use macos_origin_probe::facts::{verdict_from, CaseId, Expectations, Mechanism, Report};
    use macos_origin_probe::{mac, page};

    pub fn run(expected: Option<PathBuf>, out: Option<PathBuf>) -> ExitCode {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            eprintln!("the probe must run on the main thread");
            return ExitCode::FAILURE;
        };

        let case_a = mac::run_case(CaseId::A, page::CID, mtm);
        let case_control = mac::run_case(CaseId::Control, page::CID, mtm);
        let webkit_user_agent = if case_a.user_agent.is_empty() {
            case_control.user_agent.clone()
        } else {
            case_a.user_agent.clone()
        };
        let report = Report {
            os_version: mac::os_version(),
            webkit_user_agent,
            https_is_handled_natively: mac::https_is_handled_natively(mtm),
            cid: page::CID.to_string(),
            case_a,
            case_control,
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
            // The RECORDING run: report the measurement, do not judge it.
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
                        "\nthe recorded verdict still holds on this WebKit: {}",
                        expectations.mechanism.as_str()
                    );
                    ExitCode::SUCCESS
                } else {
                    eprintln!(
                        "\nWebKit no longer behaves as recorded in {}:",
                        path.display()
                    );
                    for difference in &differences {
                        eprintln!("  - {difference}");
                    }
                    eprintln!(
                        "\nThis is the regression this probe exists to catch, and it lands on BOTH\n\
                         WebKit shells (macOS and iOS share the WKURLSchemeHandler mechanism).\n\
                         Re-read the report above, then either fix the shell's mechanism or\n\
                         re-record the verdict WITH the reason."
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

    /// The human-readable half of the artifact: every measured fact per case,
    /// side by side, then the verdict they imply.
    fn print_summary(report: &Report) {
        eprintln!("\n=== werust macOS origin probe ===");
        eprintln!("macOS:            {}", report.os_version);
        eprintln!("WebKit UA:        {}", report.webkit_user_agent);
        eprintln!(
            "https handled natively by WebKit (so no internal-https case B is possible): {}",
            report.https_is_handled_natively
        );
        for (case, facts) in [
            (CaseId::A, &report.case_a),
            (CaseId::Control, &report.case_control),
        ] {
            eprintln!("\n-- case {} ({}) --", case.as_str(), case.label());
            eprintln!("  page:                 {}", facts.page_url);
            eprintln!("  navigation:           {}", facts.navigation);
            eprintln!("  origin:               {:?}", facts.origin);
            eprintln!("  secure context:       {}", facts.secure_context);
            eprintln!("  fetch:                {}", facts.fetch);
            eprintln!("  scheme handler fired: {}", facts.fetch_handler_fired);
            eprintln!("  pushState:            {}", facts.push_state);
            eprintln!("  module script:        {}", facts.module_script);
            eprintln!("  css font handler:     {}", facts.css_font_handler_fired);
            eprintln!("  service worker:       {}", facts.service_worker);
            eprintln!("  intercepted:          {:#?}", facts.handler_uris);
            if let Some(error) = &facts.harness_error {
                eprintln!("  HARNESS ERROR:        {error}");
            }
        }
        eprintln!();
        match verdict_from(report) {
            Ok(Mechanism::RegisteredIpfsScheme) => eprintln!(
                "VERDICT: registered-ipfs-scheme -- a WKURLSchemeHandler-served document gets a\n\
                 REAL `ipfs://<cid>` tuple origin, so a WebKit shell (macOS AND iOS) serves\n\
                 `ipfs://` like desktop Linux and Windows. The iOS mechanism analysis is now\n\
                 backed by a runtime measurement."
            ),
            Ok(Mechanism::InternalHttpsOrigin) => eprintln!(
                "VERDICT: internal-https-origin -- which is NOT constructible on WebKit. If you are\n\
                 reading this, the decision rule was changed without changing this message."
            ),
            Err(why) => eprintln!(
                "NO VERDICT: the handler-served `ipfs://` origin did not serve a client-side\n\
                 navigation.\n{why}"
            ),
        }
    }
}
