//! `dvandva` multicall binary entry point.
//!
//! The subcommand is resolved from `argv[0]`'s basename first, then from the
//! first CLI argument:
//!
//! * Git-hook basenames (`pre-commit`, `prepare-commit-msg`, and the other
//!   canonical client-side hook names materialized as symlinks by
//!   `dvandva install-hooks`) dispatch to the git-hook handler.
//! * Legacy shim basenames (`dvandva-state.sh`, `dvandva-resolve.sh`) keep
//!   dispatching to `state`/`resolve` for compatibility.
//! * Otherwise the first CLI argument names the subcommand. `--version`/`-V`
//!   prints the exact version line and exits 0; an unknown subcommand exits 2.
//!
//! Each subcommand returns an `i32` used as the exit code. Exit-code
//! namespaces are per-subcommand and are protocol surface (documented in the
//! skills and the state-transition table) — never unified across helpers.

mod cmd;

use std::process::ExitCode;

use dvandva::hooks::GIT_HOOK_NAMES;

const VERSION_LINE: &str = "dvandva 2.0.0-beta.3";

const USAGE: &str = "\
Usage: dvandva <subcommand> [args...]
       dvandva --version

Core:      state | resolve | write | wait | snapshot | next | brief
Preflight: preflight | hook-preflight
Monitor:   watchdog
Git gate:  commit-gate | drift-lint | install-hooks | git-hook <name> | baton-guard
Install:   install | install-codex | smoke-install | retire-agents
Lints:     lint <artifacts|skills|protocol-phase1|skill-phase3|phase4-research|
                 run3-dynamic-agents|run4-path-gates|run4-standalone-agents>

Multicall binary: when invoked through a git-hook symlink (pre-commit,
prepare-commit-msg, ...) the hook name is taken from argv[0].";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let argv0 = args.first().map(String::as_str).unwrap_or("");
    let basename = std::path::Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // argv[0] basename (hook symlink / legacy shim multicall) takes precedence
    // over the first CLI argument. `sub_args` are forwarded to the subcommand.
    let (subcommand, sub_args): (Option<&str>, &[String]) = match basename {
        "dvandva-state.sh" => (Some("state"), &args[1..]),
        "dvandva-resolve.sh" => (Some("resolve"), &args[1..]),
        name if GIT_HOOK_NAMES.contains(&name) => {
            let code = cmd::git_hook::run(name, &args[1..]);
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
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
        Some("next") => cmd::next::run(sub_args),
        Some("brief") => cmd::brief::run(sub_args),
        Some("baton-guard") => cmd::baton_guard::run(sub_args),
        Some("resolve") => cmd::resolve::run(sub_args),
        Some("write") => cmd::write::run(sub_args),
        Some("wait") => cmd::wait::run(sub_args),
        Some("watchdog") => cmd::watchdog::run(sub_args),
        Some("snapshot") => cmd::snapshot::run(sub_args),
        Some("preflight") => cmd::preflight::run(sub_args),
        Some("hook-preflight") => cmd::hook_preflight::run(sub_args),
        Some("commit-gate") => cmd::commit_gate::run(sub_args),
        Some("drift-lint") => cmd::drift_lint::run(sub_args),
        Some("install-hooks") => cmd::install_hooks::run(sub_args),
        Some("install") => cmd::install::run(sub_args),
        Some("install-codex") => cmd::install_codex::run(sub_args),
        Some("retire-agents") => cmd::retire::run(sub_args),
        Some("smoke-install") => cmd::smoke::run(sub_args),
        Some("lint") => cmd::lint::run(sub_args),
        Some("git-hook") => match sub_args.split_first() {
            Some((name, rest)) if GIT_HOOK_NAMES.contains(&name.as_str()) => {
                cmd::git_hook::run(name, rest)
            }
            Some((name, _)) => {
                eprintln!("dvandva git-hook: unknown hook name '{name}'");
                2
            }
            None => {
                eprintln!("dvandva git-hook: missing hook name");
                2
            }
        },
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
