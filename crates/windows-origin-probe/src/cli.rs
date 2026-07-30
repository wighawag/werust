//! Argument parsing, kept pure so the Ubuntu `verify` gate covers the entry
//! points CI depends on.

use std::path::PathBuf;

use crate::facts::CaseId;

/// What this invocation should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Child mode: run ONE case in THIS process and print its facts as a
    /// single `PROBE-CASE-FACTS <json>` line. See `crate::win`'s module doc for
    /// why each case needs its own process.
    Case(CaseId),
    /// Normal mode: run every case (each as a child of this process), write the
    /// report, and compare it against the recorded expectations if given.
    Both {
        /// The recorded verdict to assert against. Absent means "record what
        /// you find and do not fail on the outcome" — how the very first run
        /// (the one that decides the verdict) is made.
        expected: Option<PathBuf>,
        /// Where to write the full JSON report.
        out: Option<PathBuf>,
    },
    /// `--help`.
    Help,
}

pub const USAGE: &str = "\
windows-origin-probe — gate 0 of the Windows work (ADR-0011)

Measures, on a real WebView2 runtime, whether a REGISTERED `ipfs://` scheme
gives a document a real tuple origin that serves a SvelteKit-shaped client-side
navigation, and compares it against the internal `https://<cid>.ipfs.werust.invalid`
origin `crates/werust-android/rust/src/origin_map.rs` already implements.

It also runs a NEGATIVE CONTROL (the same registered scheme WITHOUT
`HasAuthorityComponent`), which must FAIL — otherwise a passing case A would be
evidence of nothing.

USAGE:
    windows-origin-probe [--expected <path>] [--out <path>]
    windows-origin-probe --case <a|b|control>

OPTIONS:
    --expected <path>       Assert the run against a recorded verdict; a
                            mismatch exits non-zero (this is how the evergreen
                            runtime is watched for regressions).
    --out <path>            Write the full JSON report here.
    --case <a|b|control>    Internal: run a single case in this process.
    -h, --help              Show this message.
";

impl Command {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
        let mut args = args.into_iter();
        let mut case = None;
        let mut expected = None;
        let mut out = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(Command::Help),
                "--case" => {
                    let value = args
                        .next()
                        .ok_or("--case needs a value (a, b or control)")?;
                    case = Some(CaseId::parse(&value).ok_or_else(|| {
                        format!("unknown case {value:?}: expected a, b or control")
                    })?);
                }
                "--expected" => {
                    expected = Some(PathBuf::from(args.next().ok_or("--expected needs a path")?));
                }
                "--out" => {
                    out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?));
                }
                other => return Err(format!("unknown argument {other:?}\n\n{USAGE}")),
            }
        }
        match case {
            Some(case) if expected.is_some() || out.is_some() => Err(format!(
                "--case {} runs a single case in-process and reports only that case; \
                 --expected/--out belong to the aggregate run",
                case.as_str()
            )),
            Some(case) => Ok(Command::Case(case)),
            None => Ok(Command::Both { expected, out }),
        }
    }
}

/// The marker a child process prefixes its one-line JSON facts with, so the
/// parent can pick it out of stdout that also carries WebView2's own noise.
pub const FACTS_MARKER: &str = "PROBE-CASE-FACTS ";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        Command::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_arguments_runs_every_case_and_records_whatever_it_finds() {
        assert_eq!(
            parse(&[]),
            Ok(Command::Both {
                expected: None,
                out: None
            })
        );
    }

    #[test]
    fn ci_asserts_against_the_recorded_verdict_and_keeps_the_report() {
        assert_eq!(
            parse(&["--expected", "expected.json", "--out", "report.json"]),
            Ok(Command::Both {
                expected: Some(PathBuf::from("expected.json")),
                out: Some(PathBuf::from("report.json")),
            })
        );
    }

    #[test]
    fn a_single_case_is_addressable_for_the_per_case_child_process() {
        assert_eq!(parse(&["--case", "a"]), Ok(Command::Case(CaseId::A)));
        assert_eq!(parse(&["--case", "B"]), Ok(Command::Case(CaseId::B)));
        assert_eq!(
            parse(&["--case", "control"]),
            Ok(Command::Case(CaseId::Control))
        );
    }

    #[test]
    fn a_mistyped_case_is_refused_rather_than_silently_defaulted() {
        assert!(parse(&["--case", "c"]).is_err());
        assert!(parse(&["--case"]).is_err());
        assert!(parse(&["--verbose"]).is_err());
    }

    /// A single case cannot produce the two-case verdict, so asking it to
    /// assert one would silently compare half a run.
    #[test]
    fn a_single_case_cannot_be_asked_to_assert_the_verdict() {
        assert!(parse(&["--case", "a", "--expected", "expected.json"]).is_err());
    }

    #[test]
    fn help_is_reachable() {
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert!(USAGE.contains("--expected"));
    }
}
