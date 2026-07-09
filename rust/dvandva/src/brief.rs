//! `brief` logic — fresh-context markdown pack for a Dvandva baton (design
//! §F2, `superpowers/specs/2026-07-02-flow-patches-design.html`).
//!
//! [`render_brief`] reads a baton (and its sibling `history/` directory)
//! leniently and renders a bounded markdown document: run header, artifact
//! refs to read, this role's current-phase `work_split` items, open
//! findings, the current phase's `verification_matrix` rows, the last 5
//! history checkpoints, and `next_action`. Every optional lookup goes
//! through jq `//` semantics via [`crate::util::coalesce`], matching the
//! rest of the read path.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util::{self, coalesce, is_open_finding_status, read_json_lenient};

/// At most this many history entries are shown, most-recent last.
const HISTORY_LIMIT: usize = 5;
/// History-entry summaries are bounded to this many codepoints.
const SUMMARY_LIMIT: usize = 160;

/// Failure modes of [`render_brief`], keyed to the shell exit-code
/// convention shared with `state`/`snapshot` (`21` missing, `22` invalid).
#[derive(Debug)]
pub enum BriefError {
    Missing { path: String },
    Invalid { path: String },
}

impl BriefError {
    pub fn exit_code(&self) -> i32 {
        match self {
            BriefError::Missing { .. } => 21,
            BriefError::Invalid { .. } => 22,
        }
    }
}

impl fmt::Display for BriefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BriefError::Missing { path } => write!(f, "baton file not found: {path}"),
            BriefError::Invalid { path } => write!(f, "baton JSON invalid: {path}"),
        }
    }
}

impl std::error::Error for BriefError {}

/// Render the fresh-context markdown brief for `role` from the baton at
/// `baton_path`. `role` is expected to already be validated as `vadi` or
/// `prativadi` by the caller; this function only reads and filters.
pub fn render_brief(baton_path: &Path, role: &str) -> Result<String, BriefError> {
    let value = match read_json_lenient(baton_path) {
        Ok(value) => value,
        Err(util::JsonReadError::Missing) => {
            return Err(BriefError::Missing {
                path: baton_path.display().to_string(),
            })
        }
        Err(util::JsonReadError::Invalid) => {
            return Err(BriefError::Invalid {
                path: baton_path.display().to_string(),
            })
        }
    };
    if !value.is_object() {
        return Err(BriefError::Invalid {
            path: baton_path.display().to_string(),
        });
    }

    let phase = field_str(&value, "phase");

    let mut out = String::new();
    out.push_str(&format!(
        "# Dvandva brief — {} ({role})\n\n",
        field_str(&value, "run_id")
    ));
    render_header(&mut out, &value, &phase);
    out.push('\n');
    render_artifacts(&mut out, &value);
    out.push('\n');
    render_work(&mut out, &value, role, &phase);
    out.push('\n');
    render_findings(&mut out, &value);
    out.push('\n');
    render_matrix(&mut out, &value, &phase);
    out.push('\n');
    render_history(&mut out, baton_path);
    out.push('\n');
    render_next_action(&mut out, &value);

    Ok(out)
}

// ── field extraction (jq `//` semantics via `coalesce`) ─────────────────────

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object()?.get(key)
}

fn coalesce_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    coalesce(get(value, key))
}

/// jq `tostring`: strings pass through unquoted; everything else renders as
/// its JSON text (numbers/booleans verbatim, arrays/objects compact); an
/// absent/null value is empty.
fn tostring(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn field_str(value: &Value, key: &str) -> String {
    coalesce_get(value, key).map(tostring).unwrap_or_default()
}

/// The first present (coalesce-non-null/false) field among `keys`, jq-string
/// coerced.
fn first_present_str(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(found) = coalesce_get(value, key) {
            return tostring(found);
        }
    }
    String::new()
}

fn csv(value: &Value, key: &str) -> String {
    match coalesce_get(value, key) {
        Some(Value::Array(items)) => items.iter().map(tostring).collect::<Vec<_>>().join(","),
        _ => String::new(),
    }
}

fn as_array(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

// ── section 1: header ────────────────────────────────────────────────────

fn render_header(out: &mut String, root: &Value, phase: &str) {
    let mode = field_str(root, "mode");
    let profile = field_str(root, "profile");
    let effective = effective_profile(root, phase, &profile);
    let status = field_str(root, "status");
    let assignee = field_str(root, "assignee");
    let active_roles = csv(root, "active_roles");
    let checkpoint = field_str(root, "checkpoint");
    let disagreement_cap = field_str(root, "disagreement_cap");

    out.push_str(&format!("- mode: {mode}\n"));
    out.push_str(&format!("- run profile: {profile}\n"));
    out.push_str(&format!(
        "- effective profile (phase {phase}): {effective}\n"
    ));
    out.push_str(&format!("- phase: {phase}\n"));
    out.push_str(&format!("- status: {status}\n"));
    out.push_str(&format!("- assignee: {assignee}\n"));
    out.push_str(&format!("- active_roles: {active_roles}\n"));
    out.push_str(&format!("- checkpoint: {checkpoint}\n"));
    out.push_str(&format!("- disagreement_cap: {disagreement_cap}\n"));

    if let Some(Value::Object(loop_counts)) = coalesce_get(root, "loop_counts") {
        for (key, count) in loop_counts {
            out.push_str(&format!(
                "- loop {key}: {}/{disagreement_cap}\n",
                tostring(count)
            ));
        }
    }
}

/// `phase_profiles[phase] // run_profile`. `phase_profiles` may not exist
/// yet (F9 lands separately in `write.rs`), so its absence is read leniently
/// as a plain fallback to the run profile.
fn effective_profile(root: &Value, phase: &str, run_profile: &str) -> String {
    if let Some(Value::Object(profiles)) = coalesce_get(root, "phase_profiles") {
        if let Some(value) = coalesce(profiles.get(phase)) {
            return tostring(value);
        }
    }
    run_profile.to_string()
}

// ── section 2: artifacts ─────────────────────────────────────────────────

const ARTIFACT_REFS: [&str; 4] = [
    "plan_ref",
    "research_ref",
    "review_ref",
    "run_explainer_ref",
];

fn render_artifacts(out: &mut String, root: &Value) {
    out.push_str("## Read these artifacts\n\n");
    let mut any = false;
    for key in ARTIFACT_REFS {
        if let Some(value) = coalesce_get(root, key) {
            out.push_str(&format!("- {key}: {}\n", tostring(value)));
            any = true;
        }
    }
    if !any {
        out.push_str("_none_\n");
    }
}

// ── section 3: this role's current-phase work ───────────────────────────

fn render_work(out: &mut String, root: &Value, role: &str, phase: &str) {
    out.push_str(&format!("## Your work (phase {phase})\n\n"));
    let mut any = false;
    for item in as_array(coalesce_get(root, "work_split")) {
        let owner = first_present_str(item, &["owner_role", "owner"]);
        if owner != role {
            continue;
        }
        if field_str(item, "phase") != phase {
            continue;
        }
        let id = field_str(item, "id");
        let chunk_type = first_present_str(item, &["chunk_type", "type"]);
        let status = field_str(item, "status");
        let paths = csv(item, "paths");
        let write_paths = csv(item, "write_paths");
        let depends_on = csv(item, "depends_on");
        out.push_str(&format!(
            "- {id}: type={chunk_type} status={status} paths=[{paths}] write_paths=[{write_paths}] depends_on=[{depends_on}]\n"
        ));
        any = true;
    }
    if !any {
        out.push_str("_none_\n");
    }
}

// ── section 4: open findings ────────────────────────────────────────────

fn render_findings(out: &mut String, root: &Value) {
    out.push_str("## Open findings\n\n");
    let mut any = false;
    for item in as_array(coalesce_get(root, "findings")) {
        if !is_open_finding(item) {
            continue;
        }
        if item.is_object() {
            let id = field_str(item, "id");
            let severity = field_str(item, "severity");
            let status = coalesce_get(item, "status")
                .map(tostring)
                .unwrap_or_else(|| "open".to_string());
            let summary = field_str(item, "summary");
            out.push_str(&format!(
                "- {} [severity={}] status={status}: {summary}\n",
                if id.is_empty() { "(no id)" } else { &id },
                if severity.is_empty() {
                    "n/a"
                } else {
                    &severity
                },
            ));
        } else {
            out.push_str(&format!("- {}\n", tostring(item)));
        }
        any = true;
    }
    if !any {
        out.push_str("_none_\n");
    }
}

/// Mirrors the shared finding-open predicate: terminal statuses are closed
/// case-insensitively; every other token is open by default.
fn is_open_finding(value: &Value) -> bool {
    if value.is_object() {
        let status = coalesce_get(value, "status")
            .map(tostring)
            .unwrap_or_else(|| "open".to_string());
        is_open_finding_status(Some(&status))
    } else {
        true
    }
}

// ── section 5: verification matrix ──────────────────────────────────────

fn render_matrix(out: &mut String, root: &Value, phase: &str) {
    out.push_str(&format!("## Verification matrix (phase {phase})\n\n"));

    let (rows, note) = matrix_rows(coalesce_get(root, "verification_matrix"), phase);

    if rows.is_empty() {
        out.push_str("_none_\n");
        return;
    }

    if let Some(note) = note {
        out.push_str(&format!("_note: {note}_\n\n"));
    }

    for row in rows {
        let result = coalesce_get(row, "result")
            .map(tostring)
            .unwrap_or_else(|| "pending".to_string());
        let label = first_present_str(row, &["claim", "check", "id"]);
        let planned = first_present_str(row, &["planned_check", "command"]);
        out.push_str(&format!("- [{result}] {label}: {planned}\n"));
    }
}

/// Selects the verification_matrix rows to render for `phase`, plus an
/// optional fallback note.
///
/// Filtering by phase only applies when the matrix is an array whose every
/// row is an object carrying a (coalesce-present) `phase` field. Otherwise —
/// the matrix is an object, or any row is not a phase-tagged object — every
/// row is shown unfiltered, with a note explaining why.
fn matrix_rows<'a>(raw: Option<&'a Value>, phase: &str) -> (Vec<&'a Value>, Option<&'static str>) {
    match raw {
        Some(Value::Array(items)) => {
            if items.is_empty() {
                return (Vec::new(), None);
            }
            let filterable = items
                .iter()
                .all(|item| item.is_object() && coalesce_get(item, "phase").is_some());
            if filterable {
                (
                    items
                        .iter()
                        .filter(|item| field_str(item, "phase") == phase)
                        .collect(),
                    None,
                )
            } else {
                (
                    items.iter().collect(),
                    Some("verification_matrix rows are not all phase-tagged objects; showing all rows unfiltered"),
                )
            }
        }
        Some(Value::Object(map)) if !map.is_empty() => (
            map.values().collect(),
            Some("verification_matrix is not an array; showing all rows unfiltered"),
        ),
        _ => (Vec::new(), None),
    }
}

// ── section 6: recent checkpoints ───────────────────────────────────────

fn render_history(out: &mut String, baton_path: &Path) {
    out.push_str("## Recent checkpoints\n\n");

    let history_dir = baton_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("history");

    let mut entries: Vec<(u64, String, String, PathBuf)> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&history_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some((checkpoint, status, assignee)) = parse_history_filename(&path) {
                entries.push((checkpoint, status, assignee, path));
            }
        }
    }
    entries.sort_by_key(|(checkpoint, ..)| *checkpoint);
    let start = entries.len().saturating_sub(HISTORY_LIMIT);

    if entries[start..].is_empty() {
        out.push_str("_none_\n");
        return;
    }

    for (checkpoint, status, assignee, path) in &entries[start..] {
        let summary = read_json_lenient(path)
            .ok()
            .map(|value| field_str(&value, "summary"))
            .unwrap_or_default();
        let summary = truncate(&summary, SUMMARY_LIMIT);
        out.push_str(&format!(
            "- cp{checkpoint} {status} {assignee} — {summary}\n"
        ));
    }
}

/// Parses `snapshot.rs`'s history filename convention
/// `<checkpoint>-<status>-<assignee>.json` into its three tokens.
///
/// Skips snapshot no-clobber duplicates: `write_with_no_clobber` in
/// `snapshot.rs` writes a byte-differing collision to
/// `<checkpoint>-<status>-<assignee>.dup-<epoch-ns>.json` rather than
/// overwriting the canonical file. That stem still parses as a checkpoint
/// entry (with a garbled assignee token), so it must be rejected explicitly
/// or it duplicates a row and can evict a real checkpoint from the last-5
/// window.
fn parse_history_filename(path: &Path) -> Option<(u64, String, String)> {
    let stem = path.file_stem()?.to_str()?;
    if stem.contains(".dup-") {
        return None;
    }
    let mut parts = stem.splitn(3, '-');
    let checkpoint = parts.next()?.parse::<u64>().ok()?;
    let status = parts.next()?.to_string();
    let assignee = parts.next()?.to_string();
    Some((checkpoint, status, assignee))
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() > max {
        let prefix: String = value.chars().take(max).collect();
        format!("{prefix}...")
    } else {
        value.to_string()
    }
}

// ── section 7: next action ──────────────────────────────────────────────

fn render_next_action(out: &mut String, root: &Value) {
    out.push_str("## Next action\n\n");
    match coalesce_get(root, "next_action") {
        None => out.push_str("_none_\n"),
        Some(Value::Object(map)) if !map.is_empty() => {
            let mut any = false;
            for (key, value) in map {
                if let Some(value) = coalesce(Some(value)) {
                    out.push_str(&format!("- {key}: {}\n", tostring(value)));
                    any = true;
                }
            }
            if !any {
                out.push_str("_none_\n");
            }
        }
        Some(Value::Object(_)) => out.push_str("_none_\n"),
        Some(other) => out.push_str(&format!("{}\n", tostring(other))),
    }
}
