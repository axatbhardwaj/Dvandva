//! `resolve` subcommand wrapper. Filled by ws-3 (prativadi): maps
//! `dvandva::resolve` outcomes to stdout token lines and exit codes.

use std::path::{Path, PathBuf};

use dvandva::emit;
use dvandva::gitcfg::repo_toplevel;
use dvandva::resolve::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome};
use dvandva::util::{read_json_lenient, JsonReadError};
use dvandva::{sla_marker, Role};

const USAGE: &str = "\
Usage: dvandva resolve --role <vadi|prativadi> [--cwd <dir>]

Resolves the active Dvandva run selector-first, then by discovery. Prints one
of:
  RESOLVED <path>   (exit 0)  an existing baton is selected
  CREATE <path>     (exit 0)  no resumable run -> new named path to scaffold
  ASK <json-array>  (exit 12) >1 resumable run + no selector -> caller stops";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolveArgs {
    role: Role,
    cwd: Option<PathBuf>,
}

/// Run the `resolve` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    if matches!(args, [flag] if flag == "-h" || flag == "--help") {
        eprintln!("{USAGE}");
        return 0;
    }

    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{USAGE}");
            if !message.is_empty() {
                eprintln!("ERROR: {message}");
            }
            return 2;
        }
    };
    let env = ResolveEnv::from_process_env();
    let explicit_selector = has_explicit_selector(&env);

    match resolve_active_run(parsed.role, parsed.cwd.as_deref(), env) {
        Ok(outcome) => {
            if let Some(code) = handle_selector_bootstrap(
                parsed.role,
                parsed.cwd.as_deref(),
                explicit_selector,
                &outcome,
            ) {
                print_deadline_if_armed(parsed.cwd.as_deref());
                return code;
            }
            sync_sla_marker(parsed.role, parsed.cwd.as_deref(), &outcome);
            let code = emit_outcome(parsed.role, &outcome);
            print_deadline_if_armed(parsed.cwd.as_deref());
            code
        }
        Err(ResolveError::Usage(message)) => {
            eprintln!("ERROR: {message}");
            2
        }
        Err(ResolveError::Cwd { path }) => {
            eprintln!("ERROR: --cwd is not a directory: {path}");
            2
        }
    }
}

enum SelectorBootstrap {
    ValidBaton,
    MissingClean,
    StaleRunDir(&'static str),
}

fn has_explicit_selector(env: &ResolveEnv) -> bool {
    env.baton_file.as_deref().is_some_and(|v| !v.is_empty())
        || env.run_dir.as_deref().is_some_and(|v| !v.is_empty())
        || env.run_id.as_deref().is_some_and(|v| !v.is_empty())
}

fn handle_selector_bootstrap(
    role: Role,
    cwd: Option<&Path>,
    explicit_selector: bool,
    outcome: &ResolveOutcome,
) -> Option<i32> {
    if !explicit_selector {
        return None;
    }
    let ResolveOutcome::Resolved(path) = outcome else {
        return None;
    };
    let root = command_root(cwd)?;
    let baton = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    match selector_bootstrap_state(&root, &baton) {
        SelectorBootstrap::ValidBaton => None,
        SelectorBootstrap::MissingClean => {
            if role == Role::Vadi {
                sla_marker::arm_if_absent(&root);
            }
            println!("{}", emit::create_line(path));
            Some(0)
        }
        SelectorBootstrap::StaleRunDir(detail) => {
            eprintln!("DVANDVA_RESOLVE stale_run_dir path={path} reason={detail}");
            Some(12)
        }
    }
}

fn command_root(cwd: Option<&Path>) -> Option<PathBuf> {
    let base = cwd
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())?;
    Some(repo_toplevel(&base).unwrap_or(base))
}

/// Surface the armed SLA countdown on stdout after every resolve, so the
/// deadline stays visible at each turn entry on any engine (the Codex
/// compensating control — Codex has no hook surface).
fn print_deadline_if_armed(cwd: Option<&Path>) {
    if let Some(root) = command_root(cwd) {
        if let Some(line) = sla_marker::deadline_line_if_armed(&root) {
            println!("{line}");
        }
    }
}

fn selector_bootstrap_state(root: &Path, baton_path: &Path) -> SelectorBootstrap {
    match read_json_lenient(baton_path) {
        Ok(_) => SelectorBootstrap::ValidBaton,
        Err(JsonReadError::Invalid) => SelectorBootstrap::StaleRunDir("invalid_baton"),
        Err(JsonReadError::Missing) => {
            let run_dir = baton_path.parent().unwrap_or(root);
            match read_json_lenient(&run_dir.join("baton.next.json")) {
                Err(JsonReadError::Invalid) => {
                    return SelectorBootstrap::StaleRunDir("invalid_candidate");
                }
                Ok(_) | Err(JsonReadError::Missing) => {}
            }
            let marker = sla_marker::marker_path(root);
            if marker.exists() && sla_marker::read(root).is_none() {
                return SelectorBootstrap::StaleRunDir("garbage_marker");
            }
            SelectorBootstrap::MissingClean
        }
    }
}

/// Keep the baton-creation SLA marker in step with the resolver outcome:
/// `CREATE` for the vadi arms it (the one moment a session verifiably owes
/// a baton), `RESOLVED` clears it. Arming never resets an existing marker,
/// so re-resolving cannot restart the countdown. The SLA is vadi-owned —
/// a batonless prativadi is sent to `wait --discover`, never told to
/// scaffold — so a prativadi resolve arms nothing.
fn sync_sla_marker(role: Role, cwd: Option<&std::path::Path>, outcome: &ResolveOutcome) {
    let Some(root) = command_root(cwd) else {
        return;
    };
    match outcome {
        ResolveOutcome::Create(_) if role == Role::Vadi => sla_marker::arm_if_absent(&root),
        ResolveOutcome::Resolved(_) => sla_marker::clear(&root),
        _ => {}
    }
}

fn parse_args(args: &[String]) -> Result<ResolveArgs, String> {
    let mut role = None;
    let mut cwd = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--role" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--role requires a value".to_string())?;
                role = Some(match Role::parse(value) {
                    Some(Role::Vadi | Role::Prativadi) => Role::parse(value).unwrap(),
                    _ => return Err("--role must be vadi or prativadi".to_string()),
                });
                index += 2;
            }
            "--cwd" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--cwd requires a value".to_string())?;
                cwd = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown argument: {}", args[index])),
        }
    }

    let role = role.ok_or_else(|| "--role is required".to_string())?;
    Ok(ResolveArgs { role, cwd })
}

fn emit_outcome(role: Role, outcome: &ResolveOutcome) -> i32 {
    match outcome.stdout_line() {
        Ok(line) => println!("{line}"),
        Err(error) => {
            eprintln!("ERROR: failed to serialize resolver output: {error}");
            return 2;
        }
    }

    match outcome {
        ResolveOutcome::AskMultiple(candidates) => {
            eprintln!(
                "{}",
                emit::dvandva_resolve_ask(role.as_str(), candidates.len())
            );
            for candidate in candidates {
                eprintln!(
                    "  - run_id={} status={} assignee={} updated_at={} path={}",
                    candidate.run_id,
                    candidate.status,
                    candidate.assignee,
                    candidate.updated_at,
                    candidate.path
                );
            }
            eprintln!(
                "Choose one via DVANDVA_RUN_ID, DVANDVA_RUN_DIR, or DVANDVA_BATON_FILE, then re-run."
            );
        }
        ResolveOutcome::AskCorrupt { path } => {
            eprintln!("{}", emit::dvandva_resolve_corrupt(path, role.as_str()));
            eprintln!("ERROR: baton at '{path}' is not valid JSON; cannot safely discover runs.");
            eprintln!(
                "Hint: inspect/repair the file or bypass discovery with an explicit selector"
            );
            eprintln!("      (DVANDVA_RUN_ID, DVANDVA_RUN_DIR, or DVANDVA_BATON_FILE).");
        }
        ResolveOutcome::Resolved(_) | ResolveOutcome::Create(_) => {}
    }

    outcome.exit_code()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dvandva::Role;

    use super::{parse_args, ResolveArgs};

    #[test]
    fn parse_requires_vadi_or_prativadi_role_and_accepts_cwd() {
        let args = vec![
            "--role".to_string(),
            "prativadi".to_string(),
            "--cwd".to_string(),
            "/tmp/dvandva".to_string(),
        ];

        assert_eq!(
            parse_args(&args).unwrap(),
            ResolveArgs {
                role: Role::Prativadi,
                cwd: Some(PathBuf::from("/tmp/dvandva"))
            }
        );
    }

    #[test]
    fn parse_rejects_missing_or_non_peer_role() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--role".to_string(), "team".to_string()]).is_err());
    }
}
