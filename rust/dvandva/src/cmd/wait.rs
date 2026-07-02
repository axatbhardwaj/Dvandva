//! CLI wrapper for `dvandva wait`.
//!
//! Parses the flags, applies the baton-path precedence (`--file` >
//! `DVANDVA_BATON_FILE` > `DVANDVA_RUN_DIR` > `DVANDVA_RUN_ID` > legacy default),
//! and — for the legacy default only — delegates run discovery to
//! [`dvandva::resolve`] in-process, translating `RESOLVED`/`CREATE`/`ASK` into
//! wait behavior exactly like the shell helper. The resolved config is then
//! handed to [`dvandva::wait::run`].

use dvandva::resolve::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome};
use dvandva::util::is_safe_run_id;
use dvandva::wait::{self, WaitConfig};
use dvandva::Role;

const USAGE: &str = "\
Usage: dvandva wait --role <vadi|prativadi> [--file .dvandva/baton.json] [--interval seconds] [--max-wait seconds] [--allow-missing] [--persist] [--persist-max seconds] [--stall-max seconds] [--since-checkpoint checkpoint] [--until-actionable] [--finite] [--notify <url>]

Defaults: --interval 60 --max-wait 540
Default file resolution: --file wins; otherwise DVANDVA_BATON_FILE,
DVANDVA_RUN_DIR/baton.json, DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json, then legacy .dvandva/baton.json.
DVANDVA_RUN_ID must be one safe path segment: letters, numbers, dot,
underscore, or dash; no slash or '..'.

The default mode is continuous: --max-wait is a heartbeat interval, not a stop
condition, and the helper keeps polling until this role owns the baton, the
baton reaches post-handshake done, it enters human_question / human_decision, or
the user interrupts. termination_review is an active handoff state, not terminal.

With --allow-missing, a missing baton file does not exit 21 immediately; the
helper instead sleeps INTERVAL and retries until the file appears or --finite
--max-wait elapses (returns 20 on timeout).

--persist is accepted for older call sites and is now the default. Use
--persist-max for a total wall-clock cap (0 = uncapped); --finite restores the
old single-heartbeat exit-20 behavior.

Use --since-checkpoint after installing a handoff checkpoint: the helper keeps
polling while the selected baton remains at or below that checkpoint, even when
the current team-owned state lists this role in active_roles. Terminal done,
human_question, and human_decision still stop immediately.

Use --until-actionable in team-owned states to keep polling until this role has
actionable work, not merely because active_roles names it. This prevents a
parallel_implementing role from waking while only the peer has ready chunks.

Use --notify <url> (or DVANDVA_NOTIFY_URL; --notify wins, empty disables) to
POST a best-effort ntfy-compatible notification on human_question,
human_decision, done, split_brain, and stalled.";

const RUN_ID_UNSAFE: &str =
    "DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash or '..')";

/// Parsed flags, numeric values still as strings for shell-exact validation.
struct RawArgs {
    role: Option<String>,
    file: Option<String>,
    interval: String,
    max_wait: String,
    allow_missing: bool,
    persist: bool,
    persist_max: String,
    stall_max: String,
    since_checkpoint: Option<String>,
    until_actionable: bool,
    notify: Option<String>,
}

impl Default for RawArgs {
    fn default() -> RawArgs {
        RawArgs {
            role: None,
            file: None,
            interval: "60".to_string(),
            max_wait: "540".to_string(),
            allow_missing: false,
            persist: true,
            persist_max: "0".to_string(),
            stall_max: "0".to_string(),
            since_checkpoint: None,
            until_actionable: false,
            notify: None,
        }
    }
}

enum ParseError {
    /// Structural error (missing value / unknown flag): print usage, exit 2.
    Usage,
    /// `-h` / `--help`: print usage, exit 0.
    Help,
}

/// Run the `wait` subcommand, returning the process exit code.
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

    // --role is required and (matching the shell) only checked for non-emptiness.
    let role = match raw.role.as_deref() {
        Some(role) if !role.is_empty() => role.to_string(),
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    // Numeric validation, in the shell's grouping/order.
    if !is_all_digits(&raw.interval)
        || !is_all_digits(&raw.max_wait)
        || !is_all_digits(&raw.persist_max)
    {
        eprintln!("ERROR: --interval, --max-wait, and --persist-max must be non-negative integers");
        return 2;
    }
    if let Some(since) = raw.since_checkpoint.as_deref() {
        if !is_all_digits(since) {
            eprintln!("ERROR: --since-checkpoint must be a non-negative integer");
            return 2;
        }
    }
    let interval: u64 = raw.interval.parse().unwrap_or(0);
    let max_wait: u64 = raw.max_wait.parse().unwrap_or(0);
    let persist_max: u64 = raw.persist_max.parse().unwrap_or(0);
    let stall_max: u64 = raw.stall_max.parse().unwrap_or(0);
    let since_checkpoint = raw
        .since_checkpoint
        .as_deref()
        .map(|s| s.parse().unwrap_or(0));

    if interval == 0 && max_wait > 0 {
        eprintln!("ERROR: --interval 0 is only valid with --max-wait 0");
        return 2;
    }
    if !raw.persist && persist_max > 0 {
        eprintln!("ERROR: --persist-max requires continuous wait mode; remove --finite");
        return 2;
    }

    // Baton-path precedence. `selected_by` captures the env-derived source before
    // --file (the shell captures it pre-arg-parse), so --file changes the source
    // gate for resolver delegation without changing the surfaced selector.
    let env_run_id = non_empty_env("DVANDVA_RUN_ID");
    let (mut baton_file, env_source) = if let Some(file) = non_empty_env("DVANDVA_BATON_FILE") {
        (file, "env_file")
    } else if let Some(dir) = non_empty_env("DVANDVA_RUN_DIR") {
        (
            format!("{}/baton.json", dir.strip_suffix('/').unwrap_or(&dir)),
            "run_dir",
        )
    } else if let Some(ref run_id) = env_run_id {
        (format!(".dvandva/runs/{run_id}/baton.json"), "run_id")
    } else {
        (".dvandva/baton.json".to_string(), "legacy")
    };
    let mut selected_by = env_source.to_string();
    let mut source = env_source;
    if let Some(file) = raw.file {
        baton_file = file;
        source = "file";
    }

    if source == "run_id" {
        // env_run_id is Some here (source == run_id requires a non-empty value).
        let run_id = env_run_id.as_deref().unwrap_or("");
        if !is_safe_run_id(run_id) {
            eprintln!("ERROR: {RUN_ID_UNSAFE}");
            return 2;
        }
    }

    // Legacy default only: delegate discovery to the resolver, replicating the
    // shell's translation of RESOLVED/CREATE/ASK. No selector is set in this
    // branch, so an empty ResolveEnv forces discovery (cwd from the process).
    if source == "legacy" {
        let role_enum = Role::parse(&role).unwrap_or(Role::Vadi);
        match resolve_active_run(role_enum, None, ResolveEnv::default()) {
            Ok(ResolveOutcome::Resolved(path)) => {
                baton_file = path;
                selected_by = "resolve".to_string();
            }
            Ok(ResolveOutcome::Create(path)) => {
                baton_file = path;
                selected_by = "resolve_create".to_string();
            }
            Ok(outcome @ (ResolveOutcome::AskMultiple(_) | ResolveOutcome::AskCorrupt { .. })) => {
                match outcome.stdout_line() {
                    Ok(line) => eprintln!("DVANDVA_WAIT selection_required role={role} {line}"),
                    Err(error) => eprintln!("ERROR: failed to render resolver outcome: {error}"),
                }
                return 2;
            }
            Err(ResolveError::Usage(message)) => {
                eprintln!("{message}");
                return 2;
            }
            Err(ResolveError::Cwd { path }) => {
                eprintln!("--cwd is not a directory: {path}");
                return 2;
            }
        }
    }

    let concurrent = std::env::var("DVANDVA_CONCURRENT")
        .map(|value| value == "1")
        .unwrap_or(false);

    // --notify wins over DVANDVA_NOTIFY_URL; an empty value from either source
    // disables notification.
    let notify_url = raw
        .notify
        .or_else(|| std::env::var("DVANDVA_NOTIFY_URL").ok())
        .filter(|value| !value.is_empty());

    let cfg = WaitConfig {
        role,
        baton_file,
        selected_by,
        interval,
        max_wait,
        allow_missing: raw.allow_missing,
        persist: raw.persist,
        persist_max,
        stall_max,
        since_checkpoint,
        until_actionable: raw.until_actionable,
        concurrent,
        notify_url,
    };
    wait::run(&cfg)
}

fn parse_args(args: &[String]) -> Result<RawArgs, ParseError> {
    let mut raw = RawArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--role" => {
                raw.role = Some(take_value(args, index)?);
                index += 2;
            }
            "--file" => {
                raw.file = Some(take_value(args, index)?);
                index += 2;
            }
            "--interval" => {
                raw.interval = take_value(args, index)?;
                index += 2;
            }
            "--max-wait" => {
                raw.max_wait = take_value(args, index)?;
                index += 2;
            }
            "--allow-missing" => {
                raw.allow_missing = true;
                index += 1;
            }
            "--persist" => {
                raw.persist = true;
                index += 1;
            }
            "--persist-max" => {
                raw.persist_max = take_value(args, index)?;
                index += 2;
            }
            "--stall-max" => {
                raw.stall_max = take_value(args, index)?;
                index += 2;
            }
            "--since-checkpoint" => {
                raw.since_checkpoint = Some(take_value(args, index)?);
                index += 2;
            }
            "--until-actionable" => {
                raw.until_actionable = true;
                index += 1;
            }
            "--finite" => {
                raw.persist = false;
                index += 1;
            }
            "--notify" => {
                raw.notify = Some(take_value(args, index)?);
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            _ => return Err(ParseError::Usage),
        }
    }
    Ok(raw)
}

fn take_value(args: &[String], index: usize) -> Result<String, ParseError> {
    args.get(index + 1).cloned().ok_or(ParseError::Usage)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn is_all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_match_shell() {
        let raw =
            parse_args(&["--role".to_string(), "vadi".to_string()]).unwrap_or_else(|_| panic!());
        assert_eq!(raw.role.as_deref(), Some("vadi"));
        assert_eq!(raw.interval, "60");
        assert_eq!(raw.max_wait, "540");
        assert!(raw.persist);
        assert_eq!(raw.persist_max, "0");
        assert!(!raw.until_actionable);
    }

    #[test]
    fn finite_clears_persist_and_flags_parse() {
        let args: Vec<String> = [
            "--role",
            "prativadi",
            "--finite",
            "--allow-missing",
            "--until-actionable",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let raw = parse_args(&args).unwrap_or_else(|_| panic!());
        assert!(!raw.persist);
        assert!(raw.allow_missing);
        assert!(raw.until_actionable);
    }

    #[test]
    fn missing_flag_value_is_usage_error() {
        let args = vec!["--role".to_string()];
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
        assert!(is_all_digits("540"));
        assert!(!is_all_digits(""));
        assert!(!is_all_digits("-1"));
        assert!(!is_all_digits("1a"));
    }
}
