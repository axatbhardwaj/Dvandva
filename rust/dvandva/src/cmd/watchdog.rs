//! CLI wrapper for `dvandva watchdog [<root>...] [flags]`.
//!
//! Parses zero or more positional root directories plus `--remind-paused`,
//! `--stale-max`, and `--notify`, defaults the root list to the git toplevel
//! of cwd (else cwd) when none were given, and hands the resolved config to
//! [`dvandva::watchdog::run`].

use std::path::PathBuf;

use dvandva::watchdog::{self, WatchdogConfig};

const USAGE: &str = "\
Usage: dvandva watchdog [<root>...] [--remind-paused seconds] [--stale-max seconds] [--notify <url>]

One-shot out-of-band liveness monitor for headless walkaway runs — run this
from cron/systemd, not from inside a session. For each root (default: git
toplevel of cwd, else cwd) scans every .dvandva/runs/*/baton.json and the
legacy .dvandva/baton.json, classifying each baton as terminal (done /
abandoned, ignored), paused (human_question / human_decision), or mid-work.
Prints one DVANDVA_WATCHDOG <event> line per stale or reminder-due baton,
plus a DVANDVA_WATCHDOG summary line at the end. Always exits 0 (findings
included, it is a monitor, not a gate); exit 2 is reserved for usage errors.

--stale-max seconds (default 1800): a mid-work baton whose updated_at age is
at least this old emits a watchdog_stale finding. A baton whose updated_at
cannot be parsed at all is treated as stale too (reason=unparseable_updated_at)
since liveness cannot be proven without it, regardless of its status.

--remind-paused seconds (default 0 = off): a human_question / human_decision
baton whose age is at least this old emits a watchdog_paused reminder.

--notify <url> (or DVANDVA_NOTIFY_URL; --notify wins, empty disables) posts a
best-effort ntfy-compatible notification for each new finding. Repeat
findings are deduplicated per baton via a small marker file next to it,
keyed on status + checkpoint + age bucket (1x/4x/24x the threshold) — a
continuously stuck run re-notifies at the threshold, ~4x, and ~24x, then
stays silent. Missing both --notify and DVANDVA_NOTIFY_URL still prints the
finding lines, plus one DVANDVA_WATCHDOG note notify_unconfigured line so
cron logs show it.";

#[derive(Default)]
struct RawArgs {
    roots: Vec<String>,
    remind_paused: Option<String>,
    stale_max: Option<String>,
    notify: Option<String>,
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
    let remind_paused: u64 = remind_paused_str.parse().unwrap_or(0);
    let stale_max: u64 = stale_max_str.parse().unwrap_or(1800);

    let roots: Vec<PathBuf> = if raw.roots.is_empty() {
        vec![default_root()]
    } else {
        raw.roots.into_iter().map(PathBuf::from).collect()
    };

    let notify_url = raw
        .notify
        .or_else(|| std::env::var("DVANDVA_NOTIFY_URL").ok())
        .filter(|value| !value.is_empty());

    let cfg = WatchdogConfig {
        roots,
        remind_paused,
        stale_max,
        notify_url,
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
            "--notify" => {
                raw.notify = Some(take_value(args, index)?);
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
        assert!(raw.notify.is_none());
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
