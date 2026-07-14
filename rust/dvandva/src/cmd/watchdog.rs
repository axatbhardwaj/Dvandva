//! CLI wrapper for `dvandva watchdog [<root>...] [flags]`.
//!
//! Parses zero or more positional root directories plus `--remind-paused`
//! and `--stale-max`, defaults the root list to the git toplevel of cwd
//! (else cwd) when none were given, and hands the resolved config to
//! [`dvandva::watchdog::run`].

use std::path::PathBuf;

use dvandva::watchdog::{self, WatchdogConfig};

const USAGE: &str = "\
Usage: dvandva watchdog [<root>...] [--remind-paused seconds] [--stale-max seconds]

One-shot out-of-band liveness monitor for headless walkaway runs — run this
from cron/systemd, not from inside a session. For each root (default: git
toplevel of cwd, else cwd) scans every .dvandva/runs/*/baton.json and the
legacy .dvandva/baton.json, classifying each baton as terminal (done /
abandoned, ignored), paused (human_question / human_decision), or mid-work.
Prints one DVANDVA_WATCHDOG <event> line per stale or reminder-due baton,
plus a DVANDVA_WATCHDOG summary line at the end. Always exits 0 (findings
included, it is a monitor, not a gate); exit 2 is reserved for usage errors.
It is a stateless scanner: findings print on every scan that finds them, with
no dedup or pacing — cron logs are the record.

--stale-max seconds (default 1800): a mid-work baton whose updated_at age is
at least this old emits a watchdog_stale finding. A baton whose updated_at
cannot be parsed at all is treated as stale too (reason=unparseable_updated_at),
and one whose updated_at sits more than 120s in the future is treated as
stale as well (reason=future_updated_at, checked before the paused branch so
even a human_question/human_decision baton is flagged) — liveness cannot be
proven without a trustworthy timestamp, regardless of status.

--remind-paused seconds (default 0 = off): a human_question / human_decision
baton whose age is at least this old emits a watchdog_paused reminder.

Duplicate/aliased root arguments (same path given twice, or a symlink alias)
are deduplicated before scanning, so each baton is counted and reported once.
An unreadable .dvandva/runs directory (e.g. permission denied) emits one
DVANDVA_WATCHDOG note skipped_unreadable_runs_dir line per root and counts
toward the summary's skipped total.";

#[derive(Default)]
struct RawArgs {
    roots: Vec<String>,
    remind_paused: Option<String>,
    stale_max: Option<String>,
}

enum ParseError {
    /// Structural error (missing value / unknown flag): print usage, exit 2.
    Usage,
    /// `-h` / `--help`: print usage, exit 0.
    Help,
}

const DEFAULT_REMIND_PAUSED: &str = "0";
const DEFAULT_STALE_MAX: &str = "1800";

/// Run the `watchdog` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let raw = match parse_args(args) {
        Ok(raw) => raw,
        Err(ParseError::Help) => {
            eprintln!("{USAGE}");
            return 0;
        }
        Err(ParseError::Usage) => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    let remind_paused_str = raw
        .remind_paused
        .as_deref()
        .unwrap_or(DEFAULT_REMIND_PAUSED);
    let stale_max_str = raw.stale_max.as_deref().unwrap_or(DEFAULT_STALE_MAX);
    if !is_all_digits(remind_paused_str) || !is_all_digits(stale_max_str) {
        eprintln!("ERROR: --remind-paused and --stale-max must be non-negative integers");
        return 2;
    }
    // `is_all_digits` only checks character shape, not range: a 20+-digit
    // value overflows u64 here. Fail loud with the same usage error rather
    // than silently falling back to a default the user didn't ask for.
    let (Ok(remind_paused), Ok(stale_max)) = (
        remind_paused_str.parse::<u64>(),
        stale_max_str.parse::<u64>(),
    ) else {
        eprintln!("ERROR: --remind-paused and --stale-max must be non-negative integers");
        return 2;
    };

    let roots: Vec<PathBuf> = if raw.roots.is_empty() {
        vec![default_root()]
    } else {
        raw.roots.into_iter().map(PathBuf::from).collect()
    };

    let cfg = WatchdogConfig {
        roots,
        remind_paused,
        stale_max,
    };
    watchdog::run(&cfg)
}

/// git toplevel of cwd, else cwd — matches `preflight`'s `work_root`.
fn default_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    dvandva::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd)
}

fn parse_args(args: &[String]) -> Result<RawArgs, ParseError> {
    let mut raw = RawArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--remind-paused" => {
                raw.remind_paused = Some(take_value(args, index)?);
                index += 2;
            }
            "--stale-max" => {
                raw.stale_max = Some(take_value(args, index)?);
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            other if other.starts_with("--") => return Err(ParseError::Usage),
            other => {
                raw.roots.push(other.to_string());
                index += 1;
            }
        }
    }
    Ok(raw)
}

fn take_value(args: &[String], index: usize) -> Result<String, ParseError> {
    args.get(index + 1).cloned().ok_or(ParseError::Usage)
}

fn is_all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_have_no_roots_and_default_thresholds() {
        let raw = parse_args(&[]).unwrap_or_else(|_| panic!());
        assert!(raw.roots.is_empty());
        assert!(raw.remind_paused.is_none());
        assert!(raw.stale_max.is_none());
    }

    #[test]
    fn parse_collects_positional_roots_and_flags_in_any_order() {
        let args: Vec<String> = [
            "root-a",
            "--stale-max",
            "60",
            "root-b",
            "--remind-paused",
            "30",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let raw = parse_args(&args).unwrap_or_else(|_| panic!());
        assert_eq!(raw.roots, vec!["root-a".to_string(), "root-b".to_string()]);
        assert_eq!(raw.stale_max.as_deref(), Some("60"));
        assert_eq!(raw.remind_paused.as_deref(), Some("30"));
    }

    #[test]
    fn missing_flag_value_is_usage_error() {
        let args = vec!["--stale-max".to_string()];
        assert!(matches!(parse_args(&args), Err(ParseError::Usage)));
    }

    #[test]
    fn unknown_flag_is_usage_error() {
        let args = vec!["--bogus".to_string()];
        assert!(matches!(parse_args(&args), Err(ParseError::Usage)));
    }

    #[test]
    fn help_flag_is_reported() {
        assert!(matches!(
            parse_args(&["--help".to_string()]),
            Err(ParseError::Help)
        ));
    }

    #[test]
    fn is_all_digits_matches_shell_regex() {
        assert!(is_all_digits("0"));
        assert!(is_all_digits("1800"));
        assert!(!is_all_digits(""));
        assert!(!is_all_digits("-1"));
        assert!(!is_all_digits("1a"));
    }
}
