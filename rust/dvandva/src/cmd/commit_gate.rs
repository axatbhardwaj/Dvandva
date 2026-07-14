//! CLI wrapper for `dvandva commit-gate`.
//!
//! Evaluates the commit gate against the current working directory and mirrors
//! `scripts/dvandva-commit-gate.sh`: exit 0 to allow, 1 to block, with the
//! diagnostic lines printed to stderr.

use std::path::PathBuf;

/// Run the commit gate, returning the process exit code.
pub fn run(_args: &[String]) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let role = std::env::var("DVANDVA_ROLE").ok().filter(|r| !r.is_empty());
    let result = dvandva::commit_gate::evaluate(&cwd, role.as_deref());
    for line in &result.stderr {
        eprintln!("{line}");
    }
    result.code
}
