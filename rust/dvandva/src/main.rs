//! `dvandva` multicall binary entry point.
//!
//! The subcommand is resolved from `argv[0]`'s basename when the binary is
//! invoked through a delegating shim (`dvandva-state.sh` -> `state`,
//! `dvandva-resolve.sh` -> `resolve`), otherwise from the first CLI argument.
//! `--version`/`-V` prints the exact version line and exits 0; an unknown
//! subcommand exits 2. Each subcommand returns an `i32` used as the exit code.

mod cmd;

use std::process::ExitCode;

const VERSION_LINE: &str = "dvandva 2.0.0-alpha.1";

const USAGE: &str = "\
Usage: dvandva <state|resolve> [args...]
       dvandva --version

Multicall read-path binary. When invoked as dvandva-state.sh or
dvandva-resolve.sh the subcommand is derived from argv[0].";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let argv0 = args.first().map(String::as_str).unwrap_or("");
    let basename = std::path::Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // argv[0] basename (shim multicall) takes precedence over the first CLI
    // argument. `sub_args` are the arguments forwarded to the subcommand.
    let (subcommand, sub_args): (Option<&str>, &[String]) = match basename {
        "dvandva-state.sh" => (Some("state"), &args[1..]),
        "dvandva-resolve.sh" => (Some("resolve"), &args[1..]),
        _ => match args.get(1).map(String::as_str) {
            Some("--version") | Some("-V") => {
                println!("{VERSION_LINE}");
                return ExitCode::SUCCESS;
            }
            Some("-h") | Some("--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some(token) => (Some(token), &args[2..]),
            None => (None, &args[..0]),
        },
    };

    let code = match subcommand {
        Some("state") => cmd::state::run(sub_args),
        Some("resolve") => cmd::resolve::run(sub_args),
        Some(other) => {
            eprintln!("dvandva: unknown subcommand '{other}'");
            eprintln!("{USAGE}");
            2
        }
        None => {
            eprintln!("{USAGE}");
            2
        }
    };

    ExitCode::from(u8::try_from(code).unwrap_or(2))
}
