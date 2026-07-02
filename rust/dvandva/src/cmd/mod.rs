//! Subcommand dispatch targets for the `dvandva` multicall binary.
//!
//! `main` resolves a subcommand from `argv[0]` (shim basename) or the first CLI
//! argument, then calls the matching `run(args) -> i32`; the returned `i32` is
//! used as the process exit code.

pub mod baton_guard;
pub mod brief;
pub mod commit_gate;
pub mod drift_lint;
pub mod git_hook;
pub mod hook_preflight;
pub mod install;
pub mod install_codex;
pub mod install_hooks;
pub mod lint;
pub mod next;
pub mod preflight;
pub mod resolve;
pub mod retire;
pub mod smoke;
pub mod snapshot;
pub mod state;
pub mod wait;
pub mod write;
