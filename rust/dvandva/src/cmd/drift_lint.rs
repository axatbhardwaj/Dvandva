//! CLI wrapper for `dvandva drift-lint`: parses the `--warn` flag,
//! discovers the repo root from the process cwd, and prints the exact
//! stdout/stderr lines `dvandva-drift-lint.sh` would have printed.

use std::env;
use std::path::PathBuf;

use dvandva::drift_lint::lint_drift;

const USAGE: &str = "Usage: dvandva drift-lint [--warn]";

/// Run the `drift-lint` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut warn_only = false;
    for arg in args {
        match arg.as_str() {
            "--warn" => warn_only = true,
            other => {
                eprintln!("dvandva drift-lint: unknown option: {other}");
                eprintln!("{USAGE}");
                return 2;
            }
        }
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let report = lint_drift(&cwd, warn_only);

    for line in &report.stdout {
        println!("{line}");
    }
    for line in &report.stderr {
        eprintln!("{line}");
    }

    report.exit_code
}
