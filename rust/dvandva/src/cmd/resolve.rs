//! `resolve` subcommand wrapper. Filled by ws-3 (prativadi): maps
//! `dvandva::resolve` outcomes to stdout token lines and exit codes.

use std::path::PathBuf;

use dvandva::emit;
use dvandva::resolve::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome};
use dvandva::Role;

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

    match resolve_active_run(
        parsed.role,
        parsed.cwd.as_deref(),
        ResolveEnv::from_process_env(),
    ) {
        Ok(outcome) => emit_outcome(parsed.role, &outcome),
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
