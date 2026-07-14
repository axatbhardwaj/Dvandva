//! `snapshot` logic — mirrors `dvandva-snapshot.sh`: copy a baton checkpoint
//! into `<parent>/history/` and, on terminal-ish statuses, additionally into a
//! named archive at the baton's parent directory.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util;

/// Failure modes of [`snapshot_baton`], keyed to the shell exit-code
/// convention (`dvandva-snapshot.sh`'s 21 / 22 / 23).
#[derive(Debug)]
pub enum SnapshotError {
    /// The baton path is not a readable regular file (`! -f`).
    Missing,
    /// The baton file was read but is not valid JSON, or is valid JSON that
    /// is not an object/null (mirrors jq erroring when `.field` is applied
    /// to a value it cannot index, e.g. a bare number or array).
    InvalidJson,
    /// A history or archive write failed: `mkdir -p` on the history
    /// directory, or the copy (original or no-clobber `.dup-<ts>.json`).
    WriteFailed,
}

impl SnapshotError {
    /// The process exit code matching `dvandva-snapshot.sh`.
    pub fn exit_code(&self) -> i32 {
        match self {
            SnapshotError::Missing => 21,
            SnapshotError::InvalidJson => 22,
            SnapshotError::WriteFailed => 23,
        }
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Missing => write!(f, "baton file missing"),
            SnapshotError::InvalidJson => write!(f, "baton file is not valid JSON"),
            SnapshotError::WriteFailed => write!(f, "snapshot write failed"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Snapshot a baton checkpoint: copy it into `<parent>/history/` (`mkdir -p`
/// first), and — when `status` is `done`, `human_decision`, `human_question`,
/// or `abandoned` — additionally into
/// `<parent>/baton.<sanitized-branch>-<checkpoint>-<status>.json` (`/` in
/// `branch` replaced with `-`).
///
/// Both writes are no-clobber: an existing target with identical bytes is a
/// no-op; an existing target with different bytes is left untouched and the
/// new bytes land at `<target>.dup-<epoch-ns>.json` instead (stderr
/// diagnostic). Field reads use jq `//` semantics via [`util::coalesce`]:
/// `checkpoint` defaults to `0`, `status`/`assignee` default to `""`, and
/// `branch` defaults to `"unknown"`.
///
/// All `DVANDVA_SNAPSHOT ...` diagnostics are printed directly to stderr by
/// this function (matching the monolithic shell script), so an in-process
/// caller sees the same stderr output a subprocess call would have produced.
pub fn snapshot_baton(baton_path: &Path) -> Result<(), SnapshotError> {
    let value = match util::read_json_lenient(baton_path) {
        Ok(value) => value,
        Err(util::JsonReadError::Missing) => {
            eprintln!("DVANDVA_SNAPSHOT missing file={}", baton_path.display());
            return Err(SnapshotError::Missing);
        }
        Err(util::JsonReadError::Invalid) => {
            eprintln!(
                "DVANDVA_SNAPSHOT invalid_json file={}",
                baton_path.display()
            );
            return Err(SnapshotError::InvalidJson);
        }
    };

    // jq errors (and the shell command substitution that captures it fails)
    // when `.field` is applied to a value it cannot index — everything
    // except an object or null. Match that failure into the same branch.
    if !(value.is_object() || value.is_null()) {
        eprintln!(
            "DVANDVA_SNAPSHOT invalid_json file={}",
            baton_path.display()
        );
        return Err(SnapshotError::InvalidJson);
    }

    let checkpoint = field_or(&value, "checkpoint", "0");
    let status = field_or(&value, "status", "");
    let assignee = field_or(&value, "assignee", "");
    let branch = field_or(&value, "branch", "unknown");
    let sanitized_branch = branch.replace('/', "-");

    let parent_dir = parent_dir_of(baton_path);
    let history_dir = parent_dir.join("history");
    let history_target = history_dir.join(format!("{checkpoint}-{status}-{assignee}.json"));

    if std::fs::create_dir_all(&history_dir).is_err() {
        eprintln!(
            "DVANDVA_SNAPSHOT write_failed target={}",
            history_dir.display()
        );
        return Err(SnapshotError::WriteFailed);
    }

    write_with_no_clobber(baton_path, &history_target)?;

    if matches!(
        status.as_str(),
        // S2-T1: abandoned is a human-declared terminal; archive it like done.
        "done" | "human_decision" | "human_question" | "abandoned"
    ) {
        let archive_target = parent_dir.join(format!(
            "baton.{sanitized_branch}-{checkpoint}-{status}.json"
        ));
        write_with_no_clobber(baton_path, &archive_target)?;
    }

    Ok(())
}

/// jq `.field // default` read, normalized to a plain string the same way
/// jq's `@tsv` stringifies each array element (mirrors the shell's
/// `[(.checkpoint // 0 | tostring), .status // "", .assignee // "", .branch // "unknown"] | @tsv`).
fn field_or(value: &Value, field: &str, default: &str) -> String {
    match util::coalesce(value.get(field)) {
        Some(v) => jq_tostring(v),
        None => default.to_string(),
    }
}

/// jq `tostring`: strings pass through unquoted; everything else renders as
/// its JSON text (numbers/booleans/null verbatim, arrays/objects compact).
fn jq_tostring(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `dirname` semantics: the parent directory, or `.` when `path` has none
/// (matches bash `dirname` for a bare filename with no directory component).
fn parent_dir_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Copy `baton_path` to `target` without clobbering a differing existing
/// file: identical bytes are a no-op; differing bytes are preserved and the
/// new content lands at `<target>.dup-<epoch-ns>.json` (stderr diagnostic,
/// exact format from `dvandva-snapshot.sh`'s `write_with_no_clobber`).
fn write_with_no_clobber(baton_path: &Path, target: &Path) -> Result<(), SnapshotError> {
    if target.exists() {
        if files_identical(baton_path, target) {
            return Ok(());
        }
        let dup = dup_path(target, util::now_epoch_nanos());
        if std::fs::copy(baton_path, &dup).is_err() {
            eprintln!("DVANDVA_SNAPSHOT write_failed target={}", target.display());
            return Err(SnapshotError::WriteFailed);
        }
        eprintln!("DVANDVA_SNAPSHOT no_clobber wrote={}", dup.display());
        return Ok(());
    }

    if std::fs::copy(baton_path, target).is_err() {
        eprintln!("DVANDVA_SNAPSHOT write_failed target={}", target.display());
        return Err(SnapshotError::WriteFailed);
    }
    Ok(())
}

/// Byte-for-byte comparison mirroring `cmp -s`. An unreadable file on either
/// side is treated as "not identical" (falls through to the dup-write path),
/// matching `cmp`'s nonzero exit when a file can't be read.
fn files_identical(a: &Path, b: &Path) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// `${target%.json}.dup-<epoch_nanos>.json`.
fn dup_path(target: &Path, epoch_nanos: u128) -> PathBuf {
    let raw = target.to_string_lossy();
    let stem = raw.strip_suffix(".json").unwrap_or(&raw);
    PathBuf::from(format!("{stem}.dup-{epoch_nanos}.json"))
}
