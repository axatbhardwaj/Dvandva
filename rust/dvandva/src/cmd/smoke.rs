//! CLI wrapper for `dvandva smoke-install`.

use dvandva::smoke;

const USAGE: &str = "\
Usage: dvandva smoke-install

End-to-end packaging probe: builds a temp marketplace from the repo's plugin
sources and drives the Claude/Codex plugin lifecycle, the skill-surface
probe, seed-JSON validation, and the read/write/lint round-trip through this
same binary. Requires the `claude` and `codex` CLIs on PATH.

Env: DVANDVA_TMPDIR overrides the temp-dir parent (default: the OS temp dir).";

/// Run the `smoke-install` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    if matches!(args, [flag] if flag == "-h" || flag == "--help") {
        eprintln!("{USAGE}");
        return 0;
    }
    if !args.is_empty() {
        eprintln!("{USAGE}");
        eprintln!("ERROR: unexpected argument: {}", args[0]);
        return 2;
    }

    match smoke::run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("FAIL: {error}");
            error.exit_code()
        }
    }
}
