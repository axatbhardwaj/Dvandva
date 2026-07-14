//! CLI wrapper for `dvandva install-codex` — ports `scripts/install-codex.sh`.

use dvandva::installers;

/// Run the `install-codex` subcommand, returning the process exit code.
///
/// Mirrors `install-codex.sh`'s `MARKETPLACE="${1:-axatbhardwaj/Dvandva}"`:
/// only the first argument is consulted. The shell script performs no flag
/// parsing and silently ignores any further arguments, so this wrapper does
/// too (no usage/help output, no argument-count validation).
pub fn run(args: &[String]) -> i32 {
    let marketplace = args
        .first()
        .cloned()
        .unwrap_or_else(|| installers::DEFAULT_MARKETPLACE.to_string());
    installers::run_install_codex(&marketplace)
}
