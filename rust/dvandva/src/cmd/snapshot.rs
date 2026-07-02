//! CLI wrapper for `dvandva snapshot` — B1 port target.

use std::path::Path;

use dvandva::snapshot::snapshot_baton;

const USAGE: &str = "Usage: dvandva-snapshot.sh <path-to-baton.json>";

/// Run the `snapshot` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let [baton_path] = args else {
        eprintln!("{USAGE}");
        return 2;
    };

    match snapshot_baton(Path::new(baton_path)) {
        Ok(()) => 0,
        Err(error) => error.exit_code(),
    }
}
