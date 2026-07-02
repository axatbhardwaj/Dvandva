//! CLI wrapper for `dvandva brief` — flow-patch target (design §F2).
//!
//! Resolves the baton path with the same precedence as `dvandva wait`
//! (`--file` > `DVANDVA_BATON_FILE` > `DVANDVA_RUN_DIR/baton.json` >
//! `DVANDVA_RUN_ID` mapped under `.dvandva/runs/<id>/baton.json` > legacy
//! `.dvandva/baton.json`), but — unlike `wait` — never delegates to the
//! run-discovery resolver: `brief` requires an existing baton and exits 21
//! when one cannot be found at the resolved path.

use dvandva::brief::render_brief;
use dvandva::util::is_safe_run_id;

const USAGE: &str = "\
Usage: dvandva brief --role <vadi|prativadi> [--file <baton.json>] [--out <file>]

Renders a fresh-context markdown brief from the resolved baton: run header
(mode, run profile, effective profile, phase, status, assignee, active_roles,
checkpoint, disagreement_cap, loop_counts), artifact refs to read, this
role's current-phase work_split items, open findings, the current phase's
verification_matrix rows, the last 5 history checkpoints, and next_action.

Default file resolution: --file wins; otherwise DVANDVA_BATON_FILE,
DVANDVA_RUN_DIR/baton.json, DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json, then legacy .dvandva/baton.json.
DVANDVA_RUN_ID must be one safe path segment: letters, numbers, dot,
underscore, or dash; no slash or '..'.

Markdown is written to stdout, or to --out <file> when given.";

const RUN_ID_UNSAFE: &str =
    "DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash or '..')";

#[derive(Default)]
struct RawArgs {
    role: Option<String>,
    file: Option<String>,
    out: Option<String>,
}

enum ParseError {
    /// Structural error (missing value / unknown flag): print usage, exit 2.
    Usage,
    /// `-h` / `--help`: print usage, exit 0.
    Help,
}

/// Run the `brief` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let raw = match parse_args(args) {
        Ok(raw) => raw,
        Err(ParseError::Help) => {
            eprintln!("{USAGE}");
            return 0;
        }
        Err(ParseError::Usage) => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    let role = match raw.role.as_deref() {
        Some("vadi") => "vadi",
        Some("prativadi") => "prativadi",
        _ => {
            eprintln!("ERROR: --role must be vadi or prativadi");
            eprintln!("{USAGE}");
            return 2;
        }
    };

    let baton_file = match resolve_baton_file(raw.file) {
        Ok(file) => file,
        Err(code) => return code,
    };

    match render_brief(std::path::Path::new(&baton_file), role) {
        Ok(markdown) => {
            if let Some(out_path) = raw.out {
                if std::fs::write(&out_path, &markdown).is_err() {
                    eprintln!("ERROR: failed to write brief to {out_path}");
                    return 2;
                }
            } else {
                print!("{markdown}");
            }
            0
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            error.exit_code()
        }
    }
}

/// Baton-path precedence, mirroring `cmd::wait`'s non-legacy branches
/// exactly. The legacy default is returned as-is (no resolver delegation):
/// `render_brief` reports a missing file as exit 21.
fn resolve_baton_file(file_flag: Option<String>) -> Result<String, i32> {
    if let Some(file) = file_flag {
        return Ok(file);
    }
    if let Some(file) = non_empty_env("DVANDVA_BATON_FILE") {
        return Ok(file);
    }
    if let Some(dir) = non_empty_env("DVANDVA_RUN_DIR") {
        return Ok(format!(
            "{}/baton.json",
            dir.strip_suffix('/').unwrap_or(&dir)
        ));
    }
    if let Some(run_id) = non_empty_env("DVANDVA_RUN_ID") {
        if !is_safe_run_id(&run_id) {
            eprintln!("ERROR: {RUN_ID_UNSAFE}");
            return Err(2);
        }
        return Ok(format!(".dvandva/runs/{run_id}/baton.json"));
    }
    Ok(".dvandva/baton.json".to_string())
}

fn parse_args(args: &[String]) -> Result<RawArgs, ParseError> {
    let mut raw = RawArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--role" => {
                raw.role = Some(take_value(args, index)?);
                index += 2;
            }
            "--file" => {
                raw.file = Some(take_value(args, index)?);
                index += 2;
            }
            "--out" => {
                raw.out = Some(take_value(args, index)?);
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            _ => return Err(ParseError::Usage),
        }
    }
    Ok(raw)
}

fn take_value(args: &[String], index: usize) -> Result<String, ParseError> {
    args.get(index + 1).cloned().ok_or(ParseError::Usage)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
