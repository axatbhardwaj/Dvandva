//! Provenance and Git-native input-change checks for delta re-verification.

use std::path::Path;

use serde_json::Value;

use crate::gitcfg;
use crate::util;

/// Read the unique history snapshot for `ckpt`; missing, unreadable, or
/// ambiguous snapshots fail closed.
pub fn read_origin_snapshot(dir: &Path, ckpt: i64) -> Option<Value> {
    let history = dir.join("history");
    let prefix = format!("{ckpt}-");
    let mut matches = std::fs::read_dir(history)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        });
    let path = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    util::read_json_lenient(&path).ok()
}

/// Return the earliest checkpoint in the current contiguous `phase` cycle.
/// Status changes and human pauses do not break a cycle; a phase change does.
pub fn current_phase_cycle_start(dir: &Path, cur: &Value, current_ckpt: i64, phase: &str) -> i64 {
    if render_phase(cur.get("phase")) != phase {
        return current_ckpt;
    }
    let mut rows = history_documents(dir, current_ckpt);
    rows.retain(|(checkpoint, _)| *checkpoint < current_ckpt);
    rows.sort_by_key(|(checkpoint, _)| *checkpoint);
    if rows.is_empty() {
        return 0;
    }

    let mut start = None;
    for (checkpoint, snapshot) in rows.iter().rev() {
        if render_phase(snapshot.get("phase")) == phase {
            start = Some(*checkpoint);
        } else {
            break;
        }
    }
    start.unwrap_or(current_ckpt)
}

/// Whether `origin` is an earlier snapshot on the current phase-cycle lineage.
pub fn on_current_cycle_ancestry(
    dir: &Path,
    cur: &Value,
    current_ckpt: i64,
    phase: &str,
    origin: i64,
) -> bool {
    let start = current_phase_cycle_start(dir, cur, current_ckpt, phase);
    if origin < start || origin >= current_ckpt {
        return false;
    }
    read_origin_snapshot(dir, origin)
        .and_then(|snapshot| snapshot.get("phase").map(|value| render_phase(Some(value))))
        .is_some_and(|origin_phase| origin_phase == phase)
}

/// Verify that every covered path is a tracked regular non-symlink file and
/// unchanged from the origin snapshot's commit anchor.
pub fn commit_anchor_valid(dir: &Path, origin_snapshot_anchor: &str, covered: &[String]) -> bool {
    if origin_snapshot_anchor.trim().is_empty() || covered.is_empty() {
        return false;
    }
    if covered.iter().any(|path| {
        std::fs::symlink_metadata(dir.join(path))
            .map(|metadata| !metadata.file_type().is_file())
            .unwrap_or(true)
    }) {
        return false;
    }

    let mut tracked_args = vec!["ls-files", "--error-unmatch", "--"];
    tracked_args.extend(covered.iter().map(String::as_str));
    if gitcfg::git_stdout(dir, &tracked_args).is_none() {
        return false;
    }

    let mut diff_args = vec!["diff", "--quiet", origin_snapshot_anchor, "--"];
    diff_args.extend(covered.iter().map(String::as_str));
    gitcfg::git_stdout(dir, &diff_args).is_some()
}

/// Whether the unit's coalesced `current // result` value is passed or approved.
pub fn was_pass(unit: &Value) -> bool {
    let result = unit
        .get("current")
        .filter(|value| !value.is_null())
        .or_else(|| unit.get("result").filter(|value| !value.is_null()));
    matches!(result, Some(Value::String(value)) if {
        value.eq_ignore_ascii_case("passed") || value.eq_ignore_ascii_case("approved")
    })
}

/// Find one kind-qualified unit by id, rejecting absent or duplicate matches.
pub fn find_unit(snap: &Value, kind: &str, id: &str) -> Option<Value> {
    let field = match kind {
        "verification_matrix_row" => "verification_matrix",
        "subagent_track" => "subagent_tracks",
        _ => return None,
    };
    let mut matches = snap
        .get(field)?
        .as_array()?
        .iter()
        .filter(|unit| unit.get("id").and_then(Value::as_str) == Some(id));
    let unit = matches.next()?.clone();
    if matches.next().is_some() {
        return None;
    }
    Some(unit)
}

/// Kind-qualified lookup that distinguishes an ABSENT unit from a POISONED
/// (duplicate-id) one — unlike [`find_unit`], whose public `Option` collapses
/// both to `None`. DR53-F4: the freshness scan must fail closed on a duplicate
/// rather than skip past it, so it needs the three-way outcome.
enum UnitLookup {
    /// No matching `(kind, id)` unit in this snapshot — keep scanning.
    Absent,
    /// Two or more matching units — the snapshot is poisoned; fail closed.
    Duplicate,
    /// Exactly one matching unit.
    Found(Value),
}

fn lookup_unit(snap: &Value, kind: &str, id: &str) -> UnitLookup {
    let field = match kind {
        "verification_matrix_row" => "verification_matrix",
        "subagent_track" => "subagent_tracks",
        _ => return UnitLookup::Absent,
    };
    let Some(array) = snap.get(field).and_then(Value::as_array) else {
        return UnitLookup::Absent;
    };
    let mut matches = array
        .iter()
        .filter(|unit| unit.get("id").and_then(Value::as_str) == Some(id));
    let Some(unit) = matches.next() else {
        return UnitLookup::Absent;
    };
    if matches.next().is_some() {
        return UnitLookup::Duplicate;
    }
    UnitLookup::Found(unit.clone())
}

/// Return the earliest installed checkpoint where `(kind, id)` is completed
/// and passing, without consulting candidate-authored checkpoint fields.
///
/// DR53-F4: a DUPLICATE-id snapshot poisons the scan (returns `None`, fail
/// closed) rather than being skipped, so a planted early duplicate cannot hide
/// the real first-completion and inflate freshness. An ABSENT id keeps scanning.
pub fn first_completed_checkpoint(
    dir: &Path,
    cur_doc: &Value,
    current_ckpt: i64,
    kind: &str,
    id: &str,
) -> Option<i64> {
    let mut documents = history_documents(dir, current_ckpt);
    if let Some(checkpoint) = cur_doc.get("checkpoint").and_then(Value::as_i64) {
        if checkpoint <= current_ckpt {
            documents.push((checkpoint, cur_doc.clone()));
        }
    }
    documents.sort_by_key(|(checkpoint, _)| *checkpoint);
    for (checkpoint, snapshot) in documents {
        match lookup_unit(&snapshot, kind, id) {
            UnitLookup::Duplicate => return None,
            UnitLookup::Found(unit) => {
                if unit.get("status").and_then(Value::as_str) == Some("completed")
                    && was_pass(&unit)
                {
                    return Some(checkpoint);
                }
            }
            UnitLookup::Absent => {}
        }
    }
    None
}

fn history_documents(dir: &Path, current_ckpt: i64) -> Vec<(i64, Value)> {
    let Ok(entries) = std::fs::read_dir(dir.join("history")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| util::read_json_lenient(&path).ok())
        .filter_map(|snapshot| {
            let checkpoint = snapshot.get("checkpoint")?.as_i64()?;
            (checkpoint <= current_ckpt).then_some((checkpoint, snapshot))
        })
        .collect()
}

fn render_phase(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) if !value.is_null() => value.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn git_ok(repo: &Path, args: &[&str]) {
        assert!(crate::gitcfg::git(repo, args).unwrap().status.success());
    }

    fn committed_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git_ok(repo.path(), &["init", "-q"]);
        git_ok(repo.path(), &["config", "user.name", "Dvandva Test"]);
        git_ok(
            repo.path(),
            &["config", "user.email", "dvandva@example.test"],
        );
        std::fs::write(repo.path().join("tracked.txt"), "origin\n").unwrap();
        git_ok(repo.path(), &["add", "tracked.txt"]);
        git_ok(repo.path(), &["commit", "-q", "-m", "origin"]);
        repo
    }

    fn write_snapshot(dir: &Path, checkpoint: i64, status: &str, phase: &str, extra: Value) {
        let history = dir.join("history");
        std::fs::create_dir_all(&history).unwrap();
        let mut snapshot = json!({
            "checkpoint": checkpoint,
            "status": status,
            "phase": phase,
            "verification_matrix": [],
            "subagent_tracks": []
        });
        if let (Some(target), Some(fields)) = (snapshot.as_object_mut(), extra.as_object()) {
            target.extend(fields.clone());
        }
        std::fs::write(
            history.join(format!("{checkpoint}-{status}-team.json")),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }

    fn current(checkpoint: i64, status: &str, phase: &str) -> Value {
        json!({"checkpoint": checkpoint, "status": status, "phase": phase})
    }

    #[test]
    fn anchor_valid_true_on_unchanged_tracked_file() {
        let repo = committed_repo();
        let anchor = crate::gitcfg::git_stdout(repo.path(), &["rev-parse", "HEAD"]).unwrap();

        assert!(commit_anchor_valid(
            repo.path(),
            &anchor,
            &["tracked.txt".to_string()]
        ));
    }

    #[test]
    fn anchor_invalid_on_untracked() {
        let repo = committed_repo();
        let anchor = crate::gitcfg::git_stdout(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(repo.path().join("untracked.txt"), "new\n").unwrap();

        assert!(!commit_anchor_valid(
            repo.path(),
            &anchor,
            &["untracked.txt".to_string()]
        ));
    }

    #[cfg(unix)]
    #[test]
    fn anchor_invalid_on_symlink() {
        let repo = committed_repo();
        std::os::unix::fs::symlink("tracked.txt", repo.path().join("link.txt")).unwrap();
        git_ok(repo.path(), &["add", "link.txt"]);
        git_ok(repo.path(), &["commit", "-q", "-m", "track symlink"]);
        let anchor = crate::gitcfg::git_stdout(repo.path(), &["rev-parse", "HEAD"]).unwrap();

        assert!(!commit_anchor_valid(
            repo.path(),
            &anchor,
            &["link.txt".to_string()]
        ));
    }

    #[test]
    fn anchor_invalid_on_changed_content() {
        let repo = committed_repo();
        let anchor = crate::gitcfg::git_stdout(repo.path(), &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

        assert!(!commit_anchor_valid(
            repo.path(),
            &anchor,
            &["tracked.txt".to_string()]
        ));
    }

    #[test]
    fn anchor_invalid_on_empty_paths() {
        let repo = committed_repo();
        let anchor = crate::gitcfg::git_stdout(repo.path(), &["rev-parse", "HEAD"]).unwrap();

        assert!(!commit_anchor_valid(repo.path(), &anchor, &[]));
    }

    #[test]
    fn ancestry_rejects_future() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 5, "test_creation", "1", json!({}));
        let cur = current(10, "cross_review", "1");

        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 10, "1", 10));
        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 10, "1", 11));
    }

    #[test]
    fn ancestry_rejects_before_cycle_start() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 2, "cross_review", "1", json!({}));
        write_snapshot(dir.path(), 3, "parallel_implementing", "2", json!({}));
        write_snapshot(dir.path(), 6, "phase_fixing", "1", json!({}));
        let cur = current(10, "cross_review", "1");

        assert_eq!(current_phase_cycle_start(dir.path(), &cur, 10, "1"), 6);
        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 10, "1", 2));
    }

    #[test]
    fn ancestry_rejects_off_lineage() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 5, "test_creation", "2", json!({}));
        let cur = current(10, "cross_review", "1");

        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 10, "1", 5));
    }

    #[test]
    fn ancestry_rejects_same_phase_behind_intervening_phase() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 2, "cross_review", "1", json!({}));
        write_snapshot(dir.path(), 3, "parallel_implementing", "2", json!({}));
        let cur = current(4, "phase_fixing", "1");

        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 4, "1", 2));
    }

    #[test]
    fn cycle_start_rejects_current_phase_mismatch_and_renders_numeric_phase() {
        let dir = tempfile::tempdir().unwrap();
        let numeric = json!({"checkpoint": 4, "status": "phase_fixing", "phase": 1});
        assert_eq!(current_phase_cycle_start(dir.path(), &numeric, 4, "1"), 0);

        let missing = json!({"checkpoint": 4, "status": "phase_fixing"});
        assert_eq!(current_phase_cycle_start(dir.path(), &missing, 4, "1"), 4);
    }

    #[test]
    fn ancestry_rejects_missing_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let cur = current(10, "cross_review", "1");

        assert!(!on_current_cycle_ancestry(dir.path(), &cur, 10, "1", 5));
    }

    #[test]
    fn ancestry_accepts_in_cycle_across_status_break() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 4, "phase_fixing", "1", json!({}));
        write_snapshot(dir.path(), 5, "human_question", "1", json!({}));
        write_snapshot(dir.path(), 6, "test_creation", "1", json!({}));
        let cur = current(8, "cross_review", "1");

        assert_eq!(current_phase_cycle_start(dir.path(), &cur, 8, "1"), 4);
        assert!(on_current_cycle_ancestry(dir.path(), &cur, 8, "1", 4));
    }

    #[test]
    fn find_unit_is_kind_qualified_and_fail_closed_on_duplicates() {
        let snap = json!({
            "verification_matrix": [{"id": "same", "result": "passed"}],
            "subagent_tracks": [{"id": "same", "status": "completed", "result": "approved"}]
        });
        assert_eq!(
            find_unit(&snap, "verification_matrix_row", "same").unwrap()["result"],
            "passed"
        );
        assert_eq!(
            find_unit(&snap, "subagent_track", "same").unwrap()["result"],
            "approved"
        );

        let duplicate = json!({
            "subagent_tracks": [{"id": "same"}, {"id": "same"}]
        });
        assert!(find_unit(&duplicate, "subagent_track", "same").is_none());
        assert!(find_unit(&snap, "unknown", "same").is_none());
    }

    #[test]
    fn read_origin_snapshot_rejects_duplicate_checkpoint_files() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(dir.path(), 5, "test_creation", "1", json!({}));
        std::fs::write(
            dir.path().join("history/5-test_creation-team.dup-1.json"),
            serde_json::to_vec(&current(5, "test_creation", "1")).unwrap(),
        )
        .unwrap();

        assert!(read_origin_snapshot(dir.path(), 5).is_none());
    }

    #[test]
    fn was_pass_coalesces_current_before_result() {
        assert!(was_pass(&json!({"current": null, "result": "passed"})));
        assert!(was_pass(
            &json!({"current": "APPROVED", "result": "failed"})
        ));
        assert!(!was_pass(&json!({"current": "failed", "result": "passed"})));
        assert!(!was_pass(&json!({"result": "pending"})));
    }

    #[test]
    fn first_completed_returns_smallest_completed_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]}),
        );
        write_snapshot(
            dir.path(),
            9,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]}),
        );
        let cur = current(10, "cross_review", "1");

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 10, "subagent_track", "test-a"),
            Some(5)
        );
    }

    #[test]
    fn first_completed_ignores_later_rewritten_appearance() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed", "evidence_checkpoint": 5}]}),
        );
        write_snapshot(
            dir.path(),
            9,
            "phase_fixing",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed", "evidence_checkpoint": 9}]}),
        );
        let cur = current(10, "test_creation", "1");

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 10, "subagent_track", "test-a"),
            Some(5)
        );
    }

    #[test]
    fn first_completed_none_when_never_completed() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "running", "result": "passed"}]}),
        );
        write_snapshot(
            dir.path(),
            9,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "failed"}]}),
        );
        let cur = current(10, "cross_review", "1");

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 10, "subagent_track", "test-a"),
            None
        );
    }

    #[test]
    fn first_completed_considers_current_installed_document() {
        let dir = tempfile::tempdir().unwrap();
        let cur = json!({
            "checkpoint": 9,
            "status": "test_creation",
            "phase": "1",
            "subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]
        });

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 9, "subagent_track", "test-a"),
            Some(9)
        );
    }

    #[test]
    fn first_completed_tolerates_current_document_without_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]}),
        );

        assert_eq!(
            first_completed_checkpoint(
                dir.path(),
                &json!({"subagent_tracks": []}),
                9,
                "subagent_track",
                "test-a"
            ),
            Some(5)
        );
    }

    #[test]
    fn first_completed_duplicate_id_snapshot_poisons_scan() {
        // DR53-F4: a duplicate-id snapshot must POISON the freshness scan
        // (fail-closed not-fresh), NOT be skipped over to a later clean
        // completion. A planted early duplicate would otherwise hide the real
        // first-completion checkpoint and inflate freshness.
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [
                {"id": "test-a", "status": "completed", "result": "passed"},
                {"id": "test-a", "status": "completed", "result": "passed"}
            ]}),
        );
        write_snapshot(
            dir.path(),
            9,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]}),
        );
        let cur = current(10, "cross_review", "1");

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 10, "subagent_track", "test-a"),
            None
        );
    }

    #[test]
    fn first_completed_absent_early_snapshot_keeps_scanning() {
        // DR53-F4: an ABSENT id in an early snapshot is distinct from a
        // duplicate — the scan continues forward to a later genuine completion
        // (only a duplicate poisons).
        let dir = tempfile::tempdir().unwrap();
        write_snapshot(
            dir.path(),
            5,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "other", "status": "completed", "result": "passed"}]}),
        );
        write_snapshot(
            dir.path(),
            9,
            "test_creation",
            "1",
            json!({"subagent_tracks": [{"id": "test-a", "status": "completed", "result": "passed"}]}),
        );
        let cur = current(10, "cross_review", "1");

        assert_eq!(
            first_completed_checkpoint(dir.path(), &cur, 10, "subagent_track", "test-a"),
            Some(9)
        );
    }
}
