//! CLI wrapper for `dvandva write` — B2 port target.
//!
//! Parses exactly two positional args `<baton.json> <candidate.json>` (usage
//! error exit 2 otherwise) and delegates to [`dvandva::write::run_write`],
//! which owns every validation/exit-code decision and prints all diagnostics.

use std::path::Path;

use dvandva::write::run_write;

const USAGE: &str = "Usage: dvandva-write.sh <path-to-baton.json> <path-to-candidate.json>";

/// Run the `write` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let [baton_file, candidate_file] = args else {
        eprintln!("{USAGE}");
        return 2;
    };
    run_write(Path::new(baton_file), Path::new(candidate_file))
}
