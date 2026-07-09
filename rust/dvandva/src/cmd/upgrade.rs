//! CLI wrapper for `dvandva upgrade`.

use dvandva::{installers, upgrade};

const USAGE: &str = "\
Usage: dvandva upgrade [<marketplace-path-or-repo>]

Upgrades the dvandva binary (`cargo install dvandva`) and both engine plugin
caches (Claude Code + Codex), including a `claude plugin update
dvandva@dvandva` cache bump. Prints a version-table report at the end.

Exit codes:
  0  committed and verified
  20 failed and rolled back cleanly
  21 rollback incomplete; inspect the residual report

Options:
  -h, --help      Show this help.

Default marketplace: axatbhardwaj/Dvandva";

/// Run the `upgrade` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            0
        }
        Some(flag) if flag.starts_with('-') => {
            eprintln!("ERROR: unknown option: {flag}");
            eprintln!("{USAGE}");
            2
        }
        Some(_) if args.len() > 1 => {
            eprintln!("ERROR: expected at most one marketplace argument");
            eprintln!("{USAGE}");
            2
        }
        Some(marketplace) => upgrade::run_upgrade(marketplace),
        None => upgrade::run_upgrade(installers::DEFAULT_MARKETPLACE),
    }
}
