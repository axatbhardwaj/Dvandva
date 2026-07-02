//! `next` logic — B1 target (design §F1,
//! superpowers/specs/2026-07-02-flow-patches-design.html).
//!
//! `dvandva next` is the candidate scaffolder. It never hand-builds a divergent
//! copy of the transition rules: both modes drive the SAME `pub(crate)` surface
//! that `dvandva write` validates with
//! ([`crate::write::legal_transitions`] / [`crate::write::validate_candidate`]),
//! so a generated candidate cannot desync from the engine.
//!
//! * LIST mode (`dvandva next [--file <baton>] [--role <r>]`): resolve the
//!   baton, read it leniently, and print one line per legal transition from the
//!   CURRENT baton, followed by the fixed over-approximation note. Exit 0.
//! * GENERATE mode (`--to <status> --summary <t> --next-action <t> [...]`):
//!   deep-copy the baton, apply the chosen edge's status/assignee/active_roles/
//!   review_target/phase/loop_counts/amendment fields, then run the full write
//!   validation pipeline in-process. A validation failure is a bug — the
//!   candidate is NEVER emitted; the baton itself is never touched.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util::{self, is_safe_run_id};
use crate::write::{
    expected_phase_for, legal_transitions, validate_candidate, PhaseMove, TransitionOption,
};

const USAGE: &str = "\
Usage:
  dvandva next [--file <baton>] [--role <vadi|prativadi>]          # list legal transitions
  dvandva next [--file <baton>] --to <status> [--phase N]
               --summary <text> --next-action <text>
               [--question <t> --resume-assignee <r> --resume-status <s>]
               [--out <file>]                                      # generate a candidate

LIST prints one line per legal transition from the current baton:
  DVANDVA_NEXT <status> owner=<assignee> phase=<same|advance|spec> [loop=<k>/<cap>] [review_target=<t>]
followed by:
  DVANDVA_NEXT note content_gates_not_reflected
--role filters to transitions this role (or a team including it) owns.

GENERATE copies the baton, applies the chosen edge, validates the result with
the same pipeline `dvandva write` runs, and writes the candidate (default
<baton-dir>/baton.next.json). It NEVER writes the baton itself.
--phase N is required when the target is ambiguous (a phase advancement or an
amendment exit). --summary and --next-action are required. human_question
additionally requires --question, --resume-assignee, and --resume-status.

Default baton resolution: --file, else DVANDVA_BATON_FILE,
DVANDVA_RUN_DIR/baton.json, DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json, then legacy .dvandva/baton.json.

Exit codes: 0 ok · 2 usage/illegal-target/ambiguous · 21 baton missing ·
22 baton invalid JSON · 25 baton unparseable-strict · 23 generated-candidate
validation failure.";

const RUN_ID_UNSAFE: &str =
    "DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash or '..')";

#[derive(Default)]
struct Args {
    file: Option<String>,
    role: Option<String>,
    to: Option<String>,
    phase: Option<String>,
    summary: Option<String>,
    next_action: Option<String>,
    question: Option<String>,
    resume_assignee: Option<String>,
    resume_status: Option<String>,
    out: Option<String>,
}

enum ParseError {
    /// Structural error (missing value / unknown flag): print usage, exit 2.
    Usage,
    /// `-h` / `--help`: print usage, exit 0.
    Help,
}

/// Run the `next` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(ParseError::Help) => {
            eprintln!("{USAGE}");
            return 0;
        }
        Err(ParseError::Usage) => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    if let Some(role) = parsed.role.as_deref() {
        if role != "vadi" && role != "prativadi" {
            eprintln!("ERROR: --role must be vadi or prativadi");
            eprintln!("{USAGE}");
            return 2;
        }
    }

    let baton_file = match resolve_baton_file(parsed.file.clone()) {
        Ok(file) => PathBuf::from(file),
        Err(code) => return code,
    };

    let baton = match util::read_json_lenient(&baton_file) {
        Ok(value) => value,
        Err(util::JsonReadError::Missing) => {
            eprintln!("DVANDVA_NEXT missing baton={}", baton_file.display());
            return 21;
        }
        Err(util::JsonReadError::Invalid) => {
            eprintln!("DVANDVA_NEXT invalid_json baton={}", baton_file.display());
            return 22;
        }
    };

    match parsed.to.as_deref() {
        None => run_list(&baton, parsed.role.as_deref()),
        Some(_) => run_generate(&baton_file, &baton, &parsed),
    }
}

// ===========================================================================
// LIST mode
// ===========================================================================
fn run_list(baton: &Value, role_filter: Option<&str>) -> i32 {
    for option in legal_transitions(baton) {
        if let Some(role) = role_filter {
            if !role_owns(&option, role) {
                continue;
            }
        }
        let mut line = format!(
            "DVANDVA_NEXT {} owner={} phase={}",
            option.to_status,
            option.assignee,
            phase_move_token(option.to_phase)
        );
        if let Some((_edge, next, cap)) = &option.loop_key {
            line.push_str(&format!(" loop={next}/{cap}"));
        }
        if let Some(target) = &option.review_target {
            line.push_str(&format!(" review_target={target}"));
        }
        println!("{line}");
    }
    // Fixed token: the surface over-approximates content gates (evidence tracks,
    // done approvals, parallel work_split, F6/F10 angles); a listed transition
    // may still be rejected once its candidate is materialised.
    println!("DVANDVA_NEXT note content_gates_not_reflected");
    0
}

/// `--role` filter: keep transitions this role owns directly, or a team
/// transition whose `active_roles` include it.
fn role_owns(option: &TransitionOption, role: &str) -> bool {
    option.assignee == role
        || (option.assignee == "team" && option.active_roles.iter().any(|r| r == role))
}

fn phase_move_token(move_: PhaseMove) -> &'static str {
    match move_ {
        PhaseMove::Same => "same",
        PhaseMove::Advance => "advance",
        PhaseMove::Spec => "spec",
    }
}

// ===========================================================================
// GENERATE mode
// ===========================================================================
fn run_generate(baton_file: &Path, baton: &Value, args: &Args) -> i32 {
    let to = args.to.as_deref().unwrap_or_default();

    // --summary / --next-action are always required.
    let (summary, next_action) = match (args.summary.as_deref(), args.next_action.as_deref()) {
        (Some(summary), Some(next_action)) => (summary, next_action),
        _ => {
            eprintln!(
                "DVANDVA_NEXT usage: --to requires --summary <text> and --next-action <text>"
            );
            return 2;
        }
    };

    // Select the matching legal transition (one source of truth with the engine).
    let options = legal_transitions(baton);
    let option = match options.iter().find(|o| o.to_status == to) {
        Some(option) => option,
        None => {
            let mut legal: Vec<&str> = options.iter().map(|o| o.to_status.as_str()).collect();
            legal.dedup();
            eprintln!(
                "DVANDVA_NEXT illegal_target to={to} legal={}",
                legal.join(",")
            );
            return 2;
        }
    };

    // human_question needs the resume triple in addition to summary/next_action.
    let is_human_question = to == "human_question";
    if is_human_question
        && (args.question.is_none()
            || args.resume_assignee.is_none()
            || args.resume_status.is_none())
    {
        eprintln!(
            "DVANDVA_NEXT usage: --to human_question requires --question <t>, --resume-assignee <r>, and --resume-status <s>"
        );
        return 2;
    }

    // Resolve the candidate phase for the chosen edge.
    let phase_value = match option.to_phase {
        PhaseMove::Advance => match args.phase.as_deref() {
            Some(raw) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => Value::from(n),
                _ => {
                    eprintln!(
                        "DVANDVA_NEXT bad_phase value={raw} (expected a non-negative integer)"
                    );
                    return 2;
                }
            },
            None => {
                eprintln!(
                    "DVANDVA_NEXT ambiguous to={to}: a phase advancement / amendment exit requires --phase N"
                );
                return 2;
            }
        },
        PhaseMove::Spec => {
            // Consume the engine-owned phase producer: the planning-phase value is
            // MODE-AWARE (development/research resolve research_*/spec_* to
            // "research"/"spec"; review mode pins every status to "review"), so the
            // candidate's phase can never desync from what phase_status_ok demands.
            let mode = baton
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let current_phase = baton.get("phase").cloned().unwrap_or(Value::Null);
            expected_phase_for(mode, to, &current_phase)
        }
        PhaseMove::Same => baton.get("phase").cloned().unwrap_or(Value::Null),
    };

    // ---- build the candidate (deep copy, then apply the edge) --------------
    let mut candidate = baton.clone();
    let current_checkpoint = baton.get("checkpoint").and_then(Value::as_i64).unwrap_or(0);
    let new_checkpoint = current_checkpoint + 1;

    candidate["checkpoint"] = Value::from(new_checkpoint);
    candidate["status"] = Value::from(to);
    candidate["assignee"] = Value::from(option.assignee.as_str());
    candidate["active_roles"] = Value::Array(
        option
            .active_roles
            .iter()
            .map(|role| Value::String(role.clone()))
            .collect(),
    );
    candidate["review_target"] = match &option.review_target {
        Some(target) => Value::from(target.as_str()),
        None => Value::Null,
    };
    candidate["phase"] = phase_value;
    candidate["updated_at"] = Value::from(now_iso8601_utc());
    candidate["summary"] = Value::from(summary);
    candidate["next_action"] = Value::from(next_action);

    if is_human_question {
        candidate["question"] = Value::from(args.question.as_deref().unwrap_or_default());
        candidate["resume_assignee"] =
            Value::from(args.resume_assignee.as_deref().unwrap_or_default());
        candidate["resume_status"] = Value::from(args.resume_status.as_deref().unwrap_or_default());
    } else {
        candidate["question"] = Value::Null;
        candidate["resume_assignee"] = Value::Null;
        candidate["resume_status"] = Value::Null;
    }

    apply_amendment(&mut candidate, option);
    apply_loop_counts(&mut candidate, baton, option);

    // ---- validate the generated candidate in-process (never emit invalid) --
    let baton_dir = baton_dir_of(baton_file);
    match validate_candidate(&baton_dir, Some(baton), &candidate) {
        Ok(()) => {}
        // A strictly-broken CURRENT baton surfaces from the engine as code 25
        // (mirroring `dvandva write`'s read namespace), not a candidate defect.
        Err((25, message)) => {
            eprintln!("{message}");
            return 25;
        }
        Err((code, message)) => {
            eprintln!("DVANDVA_NEXT invalid_candidate code={code} reason={message}");
            return 23;
        }
    }

    // ---- write the candidate (NEVER the baton) -----------------------------
    let out_path = match &args.out {
        Some(path) => PathBuf::from(path),
        None => baton_dir.join("baton.next.json"),
    };
    if same_file(&out_path, baton_file) {
        eprintln!(
            "DVANDVA_NEXT refusing to write the candidate onto the baton itself: {}",
            out_path.display()
        );
        return 2;
    }
    let rendered = serde_json::to_string_pretty(&candidate).unwrap_or_default();
    if std::fs::write(&out_path, rendered).is_err() {
        eprintln!("DVANDVA_NEXT write_failed out={}", out_path.display());
        return 2;
    }

    println!(
        "DVANDVA_NEXT ok wrote={} to={to} checkpoint={new_checkpoint}",
        out_path.display()
    );
    0
}

/// Apply the amendment field per the edge: entry sets it to the current numeric
/// phase, exit nulls it, everything else preserves the copied value.
fn apply_amendment(candidate: &mut Value, option: &TransitionOption) {
    if let Some(from_phase) = option.sets_amendment_from_phase {
        candidate["amendment_from_phase"] = Value::from(from_phase);
    } else if option.clears_amendment {
        candidate["amendment_from_phase"] = Value::Null;
    }
}

/// Apply `loop_counts` per the edge: increment the loop / amendment key,
/// reset to `{}` on a phase change, otherwise preserve the copied counts.
fn apply_loop_counts(candidate: &mut Value, baton: &Value, option: &TransitionOption) {
    if option.clears_amendment {
        // Amendment exit re-enters a numeric phase and resets the per-episode
        // loop counts.
        candidate["loop_counts"] = Value::Object(serde_json::Map::new());
        return;
    }
    if let Some((edge, next, _cap)) = &option.loop_key {
        // Loop edge (including the amendment-entry `plan_amendment:<from>` edge):
        // set the incremented count, preserving any sibling keys.
        let mut counts = baton
            .get("loop_counts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        counts.insert(edge.clone(), Value::from(*next));
        candidate["loop_counts"] = Value::Object(counts);
        return;
    }
    // No loop key: reset on any phase change (the engine forbids carrying loop
    // counts across a phase boundary), else preserve the copied counts.
    let current_phase = phase_string(baton.get("phase"));
    let new_phase = phase_string(candidate.get("phase"));
    if current_phase != new_phase {
        candidate["loop_counts"] = Value::Object(serde_json::Map::new());
    }
}

// ===========================================================================
// helpers
// ===========================================================================
/// jq `-r` render of an optional phase field (`null`/absent -> "null").
fn phase_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn baton_dir_of(baton_file: &Path) -> PathBuf {
    baton_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// True when two paths point at the same file (canonicalised where possible).
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// ISO-8601 UTC stamp `YYYY-MM-DDTHH:MM:SSZ`, matching the baton `updated_at`
/// convention. `chrono` is not a dependency; the `time` crate is.
fn now_iso8601_utc() -> String {
    let format = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z",
    )
    .expect("static format");
    time::OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Baton-path precedence, mirroring `cmd::brief` / `cmd::wait`'s non-legacy
/// branches exactly. The legacy default is returned as-is; a missing file is
/// reported as exit 21 by the caller.
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

fn parse_args(args: &[String]) -> Result<Args, ParseError> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                parsed.file = Some(take_value(args, index)?);
                index += 2;
            }
            "--role" => {
                parsed.role = Some(take_value(args, index)?);
                index += 2;
            }
            "--to" => {
                parsed.to = Some(take_value(args, index)?);
                index += 2;
            }
            "--phase" => {
                parsed.phase = Some(take_value(args, index)?);
                index += 2;
            }
            "--summary" => {
                parsed.summary = Some(take_value(args, index)?);
                index += 2;
            }
            "--next-action" => {
                parsed.next_action = Some(take_value(args, index)?);
                index += 2;
            }
            "--question" => {
                parsed.question = Some(take_value(args, index)?);
                index += 2;
            }
            "--resume-assignee" => {
                parsed.resume_assignee = Some(take_value(args, index)?);
                index += 2;
            }
            "--resume-status" => {
                parsed.resume_status = Some(take_value(args, index)?);
                index += 2;
            }
            "--out" => {
                parsed.out = Some(take_value(args, index)?);
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            _ => return Err(ParseError::Usage),
        }
    }
    Ok(parsed)
}

fn take_value(args: &[String], index: usize) -> Result<String, ParseError> {
    args.get(index + 1).cloned().ok_or(ParseError::Usage)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
