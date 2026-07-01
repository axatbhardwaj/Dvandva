//! Subcommand dispatch targets for the `dvandva` multicall binary.
//!
//! `main` resolves a subcommand from `argv[0]` (shim basename) or the first CLI
//! argument, then calls the matching `run(args) -> i32`; the returned `i32` is
//! used as the process exit code.

pub mod resolve;
pub mod state;
