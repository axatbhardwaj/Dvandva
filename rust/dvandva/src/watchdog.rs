//! `dvandva watchdog` — out-of-band liveness monitor for headless walkaway
//! runs.
//!
//! In-protocol liveness (the peer stall watchdog inside `wait`) only covers
//! one session dying while its peer survives to notice. If BOTH coordinating
//! sessions die (reboot, OOM, host loss) a mid-work baton goes silent
//! forever — nothing left in-process ever wakes up to say so. This module is
//! the out-of-band answer: run one-shot from cron/systemd, it scans every
//! run under the given roots, classifies each baton, and pushes a
//! best-effort notification whenever a baton looks stuck. Unlike `wait`, it
//! never blocks and always exits 0 on a healthy scan — it is a monitor, not
//! a gate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use time::OffsetDateTime;
use ureq::Agent;

use crate::util::{coalesce, read_json_lenient};
use crate::wait::parse_rfc3339;

/// A fully-resolved watchdog invocation, built by the `cmd::watchdog`
/// wrapper after flag parsing and root-default resolution.
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Directories to scan (already defaulted: git toplevel of cwd, else
    /// cwd, when none were given on the command line).
    pub roots: Vec<PathBuf>,
    /// `--remind-paused` seconds; `0` disables the human-pause reminder.
    pub remind_paused: u64,
    /// `--stale-max` seconds; a mid-work baton at or past this age is stale.
    pub stale_max: u64,
    /// Best-effort webhook URL for findings (`--notify` / `DVANDVA_NOTIFY_URL`,
    /// flag wins). `None` (or an empty string) disables notification, but
    /// finding lines are still printed either way.
    pub notify_url: Option<String>,
}

/// How a single baton was classified this scan.
enum Classification {
    /// `done` / `abandoned`: not competing for attention, ignored entirely.
    Terminal,
    /// Non-terminal, below the stale threshold (or a paused baton whose
    /// pause age has not crossed `--remind-paused`, or `--remind-paused`
    /// is off).
    Healthy,
    /// Mid-work and stale, or non-terminal with an unparseable `updated_at`.
    Stale,
    /// `human_question` / `human_decision`.
    Paused,
}

/// Finding context passed to [`report_finding`] — grouped to keep the
/// function's parameter count sane.
struct Finding<'a> {
    event: &'static str,
    run_id: &'a str,
    status: &'a str,
    assignee: &'a str,
    checkpoint: &'a str,
    age_s: Option<u64>,
    reason: Option<&'static str>,
    /// The threshold (`stale_max` or `remind_paused`) this finding's age
    /// bucket is measured against, for dedupe bucketing.
    threshold: u64,
}

/// Run one scan across every root, returning the process exit code. Always
/// `0`: findings are reported, never gated on.
pub fn run(cfg: &WatchdogConfig) -> i32 {
    let mut batons: u64 = 0;
    let mut stale: u64 = 0;
    let mut paused: u64 = 0;
    let mut skipped: u64 = 0;

    let roots = dedupe_roots(&cfg.roots);

    for root in &roots {
        let (baton_files, runs_dir_unreadable) = discover_batons(root);
        if runs_dir_unreadable {
            skipped += 1;
            println!(
                "DVANDVA_WATCHDOG note skipped_unreadable_runs_dir root={} path={}",
                root.display(),
                root.join(".dvandva/runs").display()
            );
        }
        for baton_file in baton_files {
            batons += 1;
            match read_json_lenient(&baton_file) {
                Err(_) => {
                    skipped += 1;
                    println!(
                        "DVANDVA_WATCHDOG note skipped_unreadable run_id={} root={} path={}",
                        fallback_run_id(&baton_file),
                        root.display(),
                        baton_file.display()
                    );
                }
                Ok(value) => match classify_baton(cfg, root, &baton_file, &value) {
                    Classification::Stale => stale += 1,
                    Classification::Paused => paused += 1,
                    Classification::Terminal | Classification::Healthy => {}
                },
            }
        }
    }

    if cfg
        .notify_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .is_none()
    {
        println!("DVANDVA_WATCHDOG note notify_unconfigured");
    }

    println!(
        "DVANDVA_WATCHDOG summary roots={} batons={batons} stale={stale} paused={paused} skipped={skipped}",
        roots.len()
    );
    0
}

/// Canonicalized+deduped roots, in first-seen order — `watchdog <r> <r>`
/// (identical or symlink-aliased paths) scans and counts each baton once. A
/// root that fails to canonicalize (e.g. doesn't exist) keeps its original
/// form and is deduped on that literal path instead.
fn dedupe_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for root in roots {
        let key = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        if seen.insert(key) {
            out.push(root.clone());
        }
    }
    out
}

/// Classify one successfully-parsed baton, reporting a finding (stdout line
/// plus a dedupe-gated notify POST) when it warrants one.
fn classify_baton(
    cfg: &WatchdogConfig,
    root: &Path,
    baton_file: &Path,
    value: &Value,
) -> Classification {
    let status = field_str(value, "status");
    if is_terminal(&status) {
        return Classification::Terminal;
    }

    let run_id = resolved_run_id(baton_file, value);
    let assignee = field_str(value, "assignee");
    let checkpoint = checkpoint_str(value);
    let updated_at = field_str(value, "updated_at");

    // Fail loud: a baton that cannot prove when it last advanced cannot
    // prove liveness at all, regardless of what status it happens to carry.
    // This applies globally — even a paused baton is reclassified to
    // Stale, since an unparseable or materially-future timestamp defeats
    // the monitor's whole purpose regardless of status.
    let age_s = match classify_age(&updated_at) {
        ParsedAge::Unparseable => {
            report_finding(
                cfg,
                root,
                baton_file,
                Finding {
                    event: "watchdog_stale",
                    run_id: &run_id,
                    status: &status,
                    assignee: &assignee,
                    checkpoint: &checkpoint,
                    age_s: None,
                    reason: Some("unparseable_updated_at"),
                    threshold: cfg.stale_max,
                },
            );
            return Classification::Stale;
        }
        ParsedAge::FutureBeyondTolerance => {
            report_finding(
                cfg,
                root,
                baton_file,
                Finding {
                    event: "watchdog_stale",
                    run_id: &run_id,
                    status: &status,
                    assignee: &assignee,
                    checkpoint: &checkpoint,
                    age_s: None,
                    reason: Some("future_updated_at"),
                    threshold: cfg.stale_max,
                },
            );
            return Classification::Stale;
        }
        ParsedAge::Age(age_s) => age_s,
    };

    if is_paused(&status) {
        if cfg.remind_paused > 0 && age_s >= cfg.remind_paused {
            report_finding(
                cfg,
                root,
                baton_file,
                Finding {
                    event: "watchdog_paused",
                    run_id: &run_id,
                    status: &status,
                    assignee: &assignee,
                    checkpoint: &checkpoint,
                    age_s: Some(age_s),
                    reason: None,
                    threshold: cfg.remind_paused,
                },
            );
        }
        return Classification::Paused;
    }

    if age_s >= cfg.stale_max {
        report_finding(
            cfg,
            root,
            baton_file,
            Finding {
                event: "watchdog_stale",
                run_id: &run_id,
                status: &status,
                assignee: &assignee,
                checkpoint: &checkpoint,
                age_s: Some(age_s),
                reason: None,
                threshold: cfg.stale_max,
            },
        );
        return Classification::Stale;
    }

    Classification::Healthy
}

/// Print the finding's `DVANDVA_WATCHDOG` line (always), then POST the
/// notify event unless a marker file next to the baton shows this exact
/// (status, checkpoint, age-bucket) combination was already reported.
fn report_finding(cfg: &WatchdogConfig, root: &Path, baton_file: &Path, finding: Finding) {
    let age_field = finding
        .age_s
        .map(|a| a.to_string())
        .unwrap_or_else(|| "unparseable".to_string());
    let reason_suffix = finding
        .reason
        .map(|r| format!(" reason={r}"))
        .unwrap_or_default();
    println!(
        "DVANDVA_WATCHDOG {} run_id={} status={} assignee={} checkpoint={} age_s={age_field} root={}{reason_suffix}",
        finding.event,
        finding.run_id,
        finding.status,
        finding.assignee,
        finding.checkpoint,
        root.display(),
    );

    // No URL configured: never touch the dedupe marker at all, so the first
    // real POST after the user later sets DVANDVA_NOTIFY_URL is not
    // pre-suppressed by markers that accrued while notification was off.
    let Some(url) = cfg.notify_url.as_deref().filter(|u| !u.is_empty()) else {
        return;
    };

    let bucket = match finding.age_s {
        None => "unparseable".to_string(),
        Some(age_s) => bucket_label(age_s, finding.threshold).to_string(),
    };
    let key = format!(
        "status={} checkpoint={} bucket={bucket}",
        finding.status, finding.checkpoint
    );
    // Read-only decision: a failed POST below must not be recorded as
    // delivered, so the marker is written only after confirmed success.
    if !should_attempt_notify(baton_file, finding.event, &key) {
        return;
    }
    let delivered = send_notify(
        url,
        finding.event,
        finding.run_id,
        finding.status,
        finding.assignee,
        &age_field,
        finding.reason,
    );
    if delivered {
        write_marker(baton_file, finding.event, &key);
    }
}

/// Which re-notify bucket `age_s` falls into relative to `threshold`
/// (`stale_max` or `remind_paused`): the finding only exists once
/// `age_s >= threshold`, so a stuck run re-reminds at `threshold`, ~4x, and
/// ~24x, then stays in the same (final) bucket forever — silent from then on.
fn bucket_label(age_s: u64, threshold: u64) -> &'static str {
    if age_s >= threshold.saturating_mul(24) {
        "24x"
    } else if age_s >= threshold.saturating_mul(4) {
        "4x"
    } else {
        "1x"
    }
}

/// The dedupe marker path for `event` next to `baton_file`.
fn marker_path(baton_file: &Path, event: &str) -> PathBuf {
    baton_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".watchdog-{event}"))
}

/// `true` exactly when `key` differs from the last-confirmed-delivered
/// marker content for this event next to `baton_file`. Read-only: a marker
/// is only ever written after a confirmed successful POST, by
/// [`write_marker`]. A read failure (including "marker doesn't exist yet")
/// degrades silently to "treat as new" — best-effort, never crashes the scan.
fn should_attempt_notify(baton_file: &Path, event: &str, key: &str) -> bool {
    let path = marker_path(baton_file, event);
    std::fs::read_to_string(&path).ok().as_deref() != Some(key)
}

/// Persist `key` as the delivered marker content for `event` next to
/// `baton_file`. Called only after [`send_notify`] confirms success — a
/// failed POST must never be recorded as delivered. Write failure degrades
/// silently — best-effort, never crashes the scan.
fn write_marker(baton_file: &Path, event: &str, key: &str) {
    let path = marker_path(baton_file, event);
    let _ = std::fs::write(&path, key);
}

/// Best-effort webhook notification with an ntfy-style
/// `Title: Dvandva <run_id>: <event>` header and a 3-second timeout.
/// Returns `true` on confirmed success (2xx/3xx). Failure is logged to
/// stderr as `DVANDVA_WATCHDOG notify_failed url=<u> err=<short>` and never
/// affects the scan's exit code.
fn send_notify(
    url: &str,
    event: &str,
    run_id: &str,
    status: &str,
    assignee: &str,
    age_s: &str,
    reason: Option<&str>,
) -> bool {
    let reason_suffix = reason.map(|r| format!(" reason={r}")).unwrap_or_default();
    let body = format!(
        "run_id={run_id} event={event} status={status} assignee={assignee} age_s={age_s}{reason_suffix}"
    );
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build();
    let agent: Agent = config.into();
    let result = agent
        .post(url)
        .header("Title", format!("Dvandva {run_id}: {event}"))
        .send(body);
    match result {
        Ok(_) => true,
        Err(err) => {
            eprintln!(
                "DVANDVA_WATCHDOG notify_failed url={url} err={}",
                truncate_chars(&err.to_string(), 200)
            );
            false
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Every baton under `root`: the legacy `.dvandva/baton.json` (if present)
/// first, then each `.dvandva/runs/*/baton.json` sorted by run directory
/// name. A local scan documented as consistent with `resolve.rs`'s
/// discovery (same legacy-then-runs layout), not a shared helper: the
/// resolver's own `candidate_files`/`CandidateFile` are private and, more
/// importantly, entangled with ASK/CREATE selection (status filtering) that
/// this monitor must not apply — a watchdog needs to see terminal and
/// corrupt batons too, not just resumable ones.
///
/// The second element is `true` when `.dvandva/runs` exists but could not
/// be read (e.g. permission denied) — a missing `.dvandva/runs` (never
/// created) is not an error and yields `false`.
fn discover_batons(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut files = Vec::new();
    let legacy = root.join(".dvandva/baton.json");
    if legacy.is_file() {
        files.push(legacy);
    }
    let runs_dir = root.join(".dvandva/runs");
    let mut runs_dir_unreadable = false;
    match std::fs::read_dir(&runs_dir) {
        Ok(entries) => {
            let mut run_batons: Vec<PathBuf> = Vec::new();
            for entry in entries.flatten() {
                let candidate = entry.path().join("baton.json");
                if candidate.is_file() {
                    run_batons.push(candidate);
                }
            }
            run_batons.sort();
            files.extend(run_batons);
        }
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            runs_dir_unreadable = true;
        }
        Err(_) => {}
    }
    (files, runs_dir_unreadable)
}

/// Tolerance for a parsed `updated_at` sitting in the future before it is
/// treated as corrupt rather than benign clock skew.
const FUTURE_TOLERANCE_SECS: i64 = 120;

/// The result of parsing and age-classifying a baton's `updated_at`.
enum ParsedAge {
    /// A non-negative age in whole seconds (within the future-tolerance
    /// window, a future value saturates to `0`).
    Age(u64),
    /// `updated_at` did not parse as RFC3339 at all.
    Unparseable,
    /// `updated_at` parsed but sits more than [`FUTURE_TOLERANCE_SECS`] in
    /// the future — a far-future or corrupt-but-valid date, which would
    /// otherwise read as "freshest forever" if simply saturated to `0`.
    FutureBeyondTolerance,
}

/// Parse `updated_at` and classify its age relative to now, saturating a
/// within-tolerance future value to `0` (benign clock skew) but flagging a
/// materially-future value instead of silently treating it as fresh.
fn classify_age(updated_at: &str) -> ParsedAge {
    let Some(parsed) = parse_rfc3339(updated_at) else {
        return ParsedAge::Unparseable;
    };
    let now = OffsetDateTime::now_utc();
    let diff_secs = (now - parsed).whole_seconds();
    if diff_secs < -FUTURE_TOLERANCE_SECS {
        return ParsedAge::FutureBeyondTolerance;
    }
    ParsedAge::Age(diff_secs.max(0) as u64)
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "done" | "abandoned")
}

fn is_paused(status: &str) -> bool {
    matches!(status, "human_question" | "human_decision")
}

/// `basename(dirname(path))`, except a legacy `.dvandva/baton.json` path
/// always yields `"legacy"` — the same fallback shape `resolve.rs` and
/// `wait.rs` derive a run id from when the baton carries none of its own.
fn fallback_run_id(baton_file: &Path) -> String {
    if baton_file.ends_with(".dvandva/baton.json") {
        return "legacy".to_string();
    }
    baton_file
        .parent()
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The baton's own `run_id` field when non-empty, else the path-derived
/// fallback.
fn resolved_run_id(baton_file: &Path, value: &Value) -> String {
    let field = field_str(value, "run_id");
    if field.is_empty() {
        fallback_run_id(baton_file)
    } else {
        field
    }
}

/// jq `//`-style string read: `null`/`false`/absent coalesce to `""`.
fn field_str(value: &Value, key: &str) -> String {
    match coalesce(value.get(key)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// `(.checkpoint // 0 | tostring)`: `null`/`false`/absent -> `"0"`.
fn checkpoint_str(value: &Value) -> String {
    match coalesce(value.get("checkpoint")) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bucket_label_thresholds() {
        assert_eq!(bucket_label(10, 10), "1x");
        assert_eq!(bucket_label(39, 10), "1x");
        assert_eq!(bucket_label(40, 10), "4x");
        assert_eq!(bucket_label(239, 10), "4x");
        assert_eq!(bucket_label(240, 10), "24x");
        assert_eq!(bucket_label(1_000_000, 10), "24x");
    }

    #[test]
    fn is_terminal_and_is_paused_classify_status_tokens() {
        assert!(is_terminal("done"));
        assert!(is_terminal("abandoned"));
        assert!(!is_terminal("implementing"));
        assert!(is_paused("human_question"));
        assert!(is_paused("human_decision"));
        assert!(!is_paused("done"));
    }

    #[test]
    fn fallback_run_id_treats_legacy_baton_specially() {
        assert_eq!(
            fallback_run_id(Path::new("/repo/.dvandva/baton.json")),
            "legacy"
        );
        assert_eq!(
            fallback_run_id(Path::new("/repo/.dvandva/runs/alpha/baton.json")),
            "alpha"
        );
    }

    #[test]
    fn resolved_run_id_prefers_baton_field_over_fallback() {
        let baton = json!({"run_id": "field-id"});
        assert_eq!(
            resolved_run_id(Path::new("/repo/.dvandva/runs/alpha/baton.json"), &baton),
            "field-id"
        );
        let empty = json!({});
        assert_eq!(
            resolved_run_id(Path::new("/repo/.dvandva/runs/alpha/baton.json"), &empty),
            "alpha"
        );
    }

    #[test]
    fn field_and_checkpoint_extraction_follow_jq_semantics() {
        let baton = json!({"status": "implementing", "checkpoint": 7, "assignee": null});
        assert_eq!(field_str(&baton, "status"), "implementing");
        assert_eq!(field_str(&baton, "assignee"), "");
        assert_eq!(field_str(&baton, "missing"), "");
        assert_eq!(checkpoint_str(&baton), "7");
        assert_eq!(checkpoint_str(&json!({})), "0");
    }

    #[test]
    fn classify_age_is_unparseable_for_unparseable_updated_at() {
        assert!(matches!(
            classify_age("not-a-timestamp"),
            ParsedAge::Unparseable
        ));
        assert!(matches!(classify_age(""), ParsedAge::Unparseable));
    }

    #[test]
    fn classify_age_computes_a_positive_age_for_a_past_timestamp() {
        assert!(matches!(
            classify_age("2020-01-01T00:00:00Z"),
            ParsedAge::Age(age) if age > 0
        ));
    }

    #[test]
    fn classify_age_saturates_within_tolerance_future_to_zero() {
        let ts = OffsetDateTime::now_utc() + time::Duration::seconds(30);
        let formatted = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            ts.year(),
            ts.month() as u8,
            ts.day(),
            ts.hour(),
            ts.minute(),
            ts.second()
        );
        assert!(matches!(classify_age(&formatted), ParsedAge::Age(0)));
    }

    #[test]
    fn classify_age_flags_materially_future_timestamps() {
        assert!(matches!(
            classify_age("2099-01-01T00:00:00Z"),
            ParsedAge::FutureBeyondTolerance
        ));
    }

    #[test]
    fn discover_batons_finds_legacy_and_run_scoped_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".dvandva/runs/beta")).unwrap();
        std::fs::create_dir_all(root.join(".dvandva/runs/alpha")).unwrap();
        std::fs::write(root.join(".dvandva/baton.json"), "{}").unwrap();
        std::fs::write(root.join(".dvandva/runs/beta/baton.json"), "{}").unwrap();
        std::fs::write(root.join(".dvandva/runs/alpha/baton.json"), "{}").unwrap();

        let (found, runs_dir_unreadable) = discover_batons(root);
        assert!(!runs_dir_unreadable);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0], root.join(".dvandva/baton.json"));
        assert_eq!(found[1], root.join(".dvandva/runs/alpha/baton.json"));
        assert_eq!(found[2], root.join(".dvandva/runs/beta/baton.json"));
    }

    #[test]
    fn discover_batons_missing_runs_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (found, runs_dir_unreadable) = discover_batons(dir.path());
        assert!(found.is_empty());
        assert!(!runs_dir_unreadable);
    }

    #[test]
    fn dedupe_roots_removes_repeated_and_canonicalized_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let deduped = dedupe_roots(&[root.clone(), root.clone()]);
        assert_eq!(deduped.len(), 1);
    }
}
