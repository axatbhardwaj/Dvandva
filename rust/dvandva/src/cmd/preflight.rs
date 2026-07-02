//! CLI wrapper for `dvandva preflight --role <vadi|prativadi> [--mode auto|strict|off]`.
//!
//! Parses `--role`/`--mode`, defaulting `--mode` to the `DVANDVA_HOOK_PREFLIGHT`
//! environment variable (else `auto`), then delegates to
//! [`dvandva::preflight::run_preflight`].

use dvandva::hook_preflight::HookMode;
use dvandva::preflight::run_preflight;
use dvandva::Role;

const USAGE: &str = "Usage: dvandva-preflight.sh --role <vadi|prativadi> [--mode auto|strict|off]";

/// Run the `preflight` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut role_str = String::new();
    let mut mode_str =
        std::env::var("DVANDVA_HOOK_PREFLIGHT").unwrap_or_else(|_| "auto".to_string());

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--role" => match args.get(index + 1) {
                Some(value) => {
                    role_str = value.clone();
                    index += 2;
                }
                None => {
                    eprintln!("{USAGE}");
                    return 2;
                }
            },
            "--mode" => match args.get(index + 1) {
                Some(value) => {
                    mode_str = value.clone();
                    index += 2;
                }
                None => {
                    eprintln!("{USAGE}");
                    return 2;
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return 0;
            }
            _ => {
                eprintln!("{USAGE}");
                return 2;
            }
        }
    }

    let role = match role_str.as_str() {
        "vadi" => Role::Vadi,
        "prativadi" => Role::Prativadi,
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    let mode = match HookMode::parse(&mode_str) {
        Some(mode) => mode,
        None => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    run_preflight(role, mode)
}
