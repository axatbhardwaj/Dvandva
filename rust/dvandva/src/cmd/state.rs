//! `state` subcommand wrapper. Filled by ws-4 (prativadi): parses
//! `--compact --file <baton> --role <r>` and emits the compact projection.

use std::path::PathBuf;

use dvandva_core::emit;
use dvandva_core::state::compact_state_from_file;
use dvandva_core::Role;

const USAGE: &str = "\
Usage: dvandva state --compact --file <baton.json> [--role vadi|prativadi|team|human]

Emits BATON_STATE_COMPACT JSON: a bounded summary with refs, counts, current
role work, open findings, latest verification, and next_action.";

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateArgs {
    file: PathBuf,
    role_flag: Option<String>,
}

/// Run the `state` subcommand, returning the process exit code.
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

    let env_role = std::env::var("DVANDVA_ROLE").ok();
    let argv0 = std::env::args().next().unwrap_or_default();
    let Some(role) = resolve_role(parsed.role_flag.as_deref(), env_role.as_deref(), &argv0) else {
        eprintln!("ERROR: --role must be vadi, prativadi, team, or human");
        return 2;
    };

    match compact_state_from_file(&parsed.file, role) {
        Ok(state) => match emit::to_json_pretty(&state) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(error) => {
                eprintln!("ERROR: failed to serialize compact state: {error}");
                2
            }
        },
        Err(error) => {
            eprintln!("ERROR: {error}");
            error.exit_code()
        }
    }
}

fn parse_args(args: &[String]) -> Result<StateArgs, String> {
    let mut compact = false;
    let mut file = None;
    let mut role_flag = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--compact" => {
                compact = true;
                index += 1;
            }
            "--file" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--file requires a value".to_string())?;
                file = Some(PathBuf::from(value));
                index += 2;
            }
            "--role" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--role requires a value".to_string())?;
                role_flag = Some(value.clone());
                index += 2;
            }
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown argument: {}", args[index])),
        }
    }

    if !compact {
        return Err("--compact is required".to_string());
    }
    let file = file.ok_or_else(|| "--file is required".to_string())?;
    Ok(StateArgs { file, role_flag })
}

fn resolve_role(role_flag: Option<&str>, env_role: Option<&str>, argv0: &str) -> Option<Role> {
    Role::resolve(role_flag, env_role, argv0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dvandva_core::Role;

    use super::{parse_args, resolve_role, StateArgs};

    #[test]
    fn parse_requires_compact_and_file_and_accepts_optional_role() {
        let args = vec![
            "--compact".to_string(),
            "--file".to_string(),
            "baton.json".to_string(),
            "--role".to_string(),
            "vadi".to_string(),
        ];

        assert_eq!(
            parse_args(&args).unwrap(),
            StateArgs {
                file: PathBuf::from("baton.json"),
                role_flag: Some("vadi".to_string())
            }
        );
    }

    #[test]
    fn resolve_role_uses_flag_then_env_then_argv0() {
        assert_eq!(
            resolve_role(
                Some("prativadi"),
                Some("vadi"),
                "/repo/plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"
            )
            .unwrap(),
            Role::Prativadi
        );
        assert_eq!(
            resolve_role(
                None,
                Some("vadi"),
                "/repo/plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"
            )
            .unwrap(),
            Role::Vadi
        );
        assert_eq!(
            resolve_role(
                None,
                None,
                "/repo/plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"
            )
            .unwrap(),
            Role::Prativadi
        );
    }

    #[test]
    fn parse_rejects_missing_compact_or_file() {
        assert!(parse_args(&["--file".to_string(), "baton.json".to_string()]).is_err());
        assert!(parse_args(&["--compact".to_string()]).is_err());
    }
}
