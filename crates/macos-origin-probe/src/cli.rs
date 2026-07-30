//! The probe's command line. Pure, so the Ubuntu `verify` gate covers it.

use std::path::PathBuf;

/// What the probe was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print usage.
    Help,
    /// Measure every case. With `expected`, ASSERT the run against the recorded
    /// verdict and exit non-zero naming the field that moved; without it, this is
    /// the RECORDING run and it only reports.
    Run {
        expected: Option<PathBuf>,
        out: Option<PathBuf>,
    },
}

pub const USAGE: &str = "\
macos-origin-probe -- measure what origin a WKURLSchemeHandler-served document gets

USAGE:
    macos-origin-probe [--expected <path>] [--out <path>]

OPTIONS:
    --expected <path>  Assert the run against a recorded verdict (exit non-zero on any drift).
    --out <path>       Write the measured report as JSON.
    -h, --help         Print this message.
";

impl Command {
    /// Parse the argument list (already stripped of `argv[0]`).
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut expected = None;
        let mut out = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(Command::Help),
                "--expected" => {
                    expected = Some(PathBuf::from(args.next().ok_or("--expected needs a path")?));
                }
                "--out" => {
                    out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?));
                }
                other => return Err(format!("unexpected argument `{other}`\n\n{USAGE}")),
            }
        }
        Ok(Command::Run { expected, out })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_is_the_recording_run() {
        assert_eq!(
            Command::parse(Vec::<String>::new()),
            Ok(Command::Run {
                expected: None,
                out: None
            })
        );
    }

    #[test]
    fn expected_and_out_are_both_carried() {
        let parsed = Command::parse(
            ["--expected", "e.json", "--out", "r.json"]
                .into_iter()
                .map(String::from),
        );
        assert_eq!(
            parsed,
            Ok(Command::Run {
                expected: Some(PathBuf::from("e.json")),
                out: Some(PathBuf::from("r.json")),
            })
        );
    }

    #[test]
    fn help_is_help_wherever_it_appears() {
        assert_eq!(
            Command::parse(["--help"].into_iter().map(String::from)),
            Ok(Command::Help)
        );
        assert_eq!(
            Command::parse(["--out", "r.json", "-h"].into_iter().map(String::from)),
            Ok(Command::Help)
        );
    }

    #[test]
    fn an_unknown_flag_or_a_missing_value_is_refused_loudly() {
        assert!(Command::parse(["--nope"].into_iter().map(String::from)).is_err());
        assert!(Command::parse(["--expected"].into_iter().map(String::from)).is_err());
        assert!(Command::parse(["--out"].into_iter().map(String::from)).is_err());
    }
}
