//! Shared helpers used across subcommands.
//!
//! These are the crate-wide invariants the shell helpers shared by
//! copy-paste: safe run-id / relative-path validation, the jq `//`
//! null-and-false coalesce, lenient JSON reads, and UTC stamps.

use std::path::Path;

use serde_json::Value;

/// jq `//` semantics: `null` and `false` coalesce to absent.
///
/// Mirrors the shell helpers' pervasive `.field // default` reads. Any port
/// that reads baton fields optionally must go through this, not plain
/// `Option`.
pub fn coalesce(value: Option<&Value>) -> Option<&Value> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(value) => Some(value),
    }
}

/// Safe run id: `^[A-Za-z0-9][A-Za-z0-9._-]*$` and never contains `..`.
///
/// Shared verbatim by resolve, write, wait, preflight, and the lints.
pub fn is_safe_run_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !value.contains("..")
}

/// Safe relative path per `dvandva-write.sh`'s `safe_rel_path`:
/// non-empty, not absolute, no `//`, and no empty / `.` / `..` segment.
pub fn is_safe_rel_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains("//")
        && value
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// Failure modes of a lenient JSON read, keyed to the shell exit-code
/// convention (missing file vs unparseable content).
#[derive(Debug)]
pub enum JsonReadError {
    /// The path is not a readable regular file.
    Missing,
    /// The bytes were read but are not valid JSON.
    Invalid,
}

/// Read a JSON file leniently: any readable valid-JSON document is accepted
/// (unknown fields, future status tokens, sparse objects included).
pub fn read_json_lenient(path: &Path) -> Result<Value, JsonReadError> {
    let bytes = std::fs::read(path).map_err(|_| JsonReadError::Missing)?;
    serde_json::from_slice(&bytes).map_err(|_| JsonReadError::Invalid)
}

/// Seconds since the Unix epoch (wall clock), saturating at 0.
pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Nanoseconds since the Unix epoch, mirroring the shell's `date +%s%N`.
pub fn now_epoch_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Compact UTC timestamp `YYYYMMDDTHHMMSSZ` (retire backup dirs, manifests).
pub fn utc_compact_timestamp() -> String {
    let fmt =
        time::format_description::parse_borrowed::<2>("[year][month][day]T[hour][minute][second]Z")
            .expect("static format");
    time::OffsetDateTime::now_utc()
        .format(&fmt)
        .unwrap_or_else(|_| "19700101T000000Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coalesce_treats_null_and_false_as_absent() {
        let null = json!(null);
        let f = json!(false);
        let t = json!(true);
        let zero = json!(0);
        let empty = json!("");
        assert!(coalesce(None).is_none());
        assert!(coalesce(Some(&null)).is_none());
        assert!(coalesce(Some(&f)).is_none());
        assert!(coalesce(Some(&t)).is_some());
        assert!(coalesce(Some(&zero)).is_some());
        assert!(coalesce(Some(&empty)).is_some());
    }

    #[test]
    fn safe_run_id_accepts_and_rejects() {
        assert!(is_safe_run_id("run"));
        assert!(is_safe_run_id("run-2.x_y"));
        assert!(is_safe_run_id("0abc"));
        assert!(!is_safe_run_id(""));
        assert!(!is_safe_run_id("-run"));
        assert!(!is_safe_run_id(".run"));
        assert!(!is_safe_run_id("a..b"));
        assert!(!is_safe_run_id("a/b"));
        assert!(!is_safe_run_id("a b"));
    }

    #[test]
    fn safe_rel_path_accepts_and_rejects() {
        assert!(is_safe_rel_path("src/lib.rs"));
        assert!(is_safe_rel_path("a"));
        assert!(!is_safe_rel_path(""));
        assert!(!is_safe_rel_path("/abs"));
        assert!(!is_safe_rel_path("a//b"));
        assert!(!is_safe_rel_path("a/./b"));
        assert!(!is_safe_rel_path("a/../b"));
        assert!(!is_safe_rel_path("./a"));
        assert!(!is_safe_rel_path("a/"));
    }

    #[test]
    fn read_json_lenient_distinguishes_missing_and_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.json");
        assert!(matches!(
            read_json_lenient(&missing),
            Err(JsonReadError::Missing)
        ));
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{not json").unwrap();
        assert!(matches!(
            read_json_lenient(&bad),
            Err(JsonReadError::Invalid)
        ));
        let good = dir.path().join("good.json");
        std::fs::write(&good, "{\"status\": \"future_token\"}").unwrap();
        assert!(read_json_lenient(&good).is_ok());
    }

    #[test]
    fn utc_compact_timestamp_shape() {
        let ts = utc_compact_timestamp();
        assert_eq!(ts.len(), 16);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[8..9], "T");
    }
}
