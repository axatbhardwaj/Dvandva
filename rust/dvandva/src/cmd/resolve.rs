//! `resolve` subcommand wrapper. Filled by ws-3 (prativadi): maps
//! `dvandva_core::resolve` outcomes to stdout token lines and exit codes.

/// Run the `resolve` subcommand, returning the process exit code.
pub fn run(_args: &[String]) -> i32 {
    eprintln!("dvandva: resolve not yet implemented");
    2
}
