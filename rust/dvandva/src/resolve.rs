//! Active-run discovery, selector precedence, sort, and outcome.
//!
//! Filled by ws-3 (prativadi): mirrors `dvandva-resolve.sh`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::{emit, util, Role};

const UNSAFE_RUN_ID_MESSAGE: &str = "DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash, backslash, or '..')";

/// Statuses excluded from the resumable set during discovery. A local literal
/// terminal set — deliberately not coupled to `crate::baton::Status` or
/// `write.rs` (this consumer's terminal taxonomy is its own, matching the
/// gate/drift-lint modules' equivalent local sets).
const RESOLVE_TERMINAL_STATUSES: &[&str] = &["done", "abandoned"];

/// The three selector environment variables consumed by the resolver.
///
/// `run_id: Some("")` intentionally differs from `None`: an empty but present
/// `DVANDVA_RUN_ID` is a usage error, matching the shell helper.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveEnv {
    pub baton_file: Option<String>,
    pub run_dir: Option<String>,
    pub run_id: Option<String>,
}

impl ResolveEnv {
    /// Capture resolver selectors from the current process environment.
    pub fn from_process_env() -> ResolveEnv {
        ResolveEnv {
            baton_file: env_string("DVANDVA_BATON_FILE"),
            run_dir: env_string("DVANDVA_RUN_DIR"),
            run_id: env_string("DVANDVA_RUN_ID"),
        }
    }
}

/// A resumable run candidate surfaced in an `ASK` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunCandidate {
    pub run_id: String,
    pub path: String,
    pub status: String,
    pub assignee: String,
    pub updated_at: String,
}

impl RunCandidate {
    pub fn new(
        run_id: impl Into<String>,
        path: impl Into<String>,
        status: impl Into<String>,
        assignee: impl Into<String>,
        updated_at: impl Into<String>,
    ) -> RunCandidate {
        RunCandidate {
            run_id: run_id.into(),
            path: path.into(),
            status: status.into(),
            assignee: assignee.into(),
            updated_at: updated_at.into(),
        }
    }
}

/// The resolver's stdout-level outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOutcome {
    Resolved(String),
    Create(String),
    AskMultiple(Vec<RunCandidate>),
    AskCorrupt { path: String },
}

impl ResolveOutcome {
    /// Render the exact single-line stdout token for this outcome.
    pub fn stdout_line(&self) -> Result<String, serde_json::Error> {
        match self {
            ResolveOutcome::Resolved(path) => Ok(emit::resolved_line(path)),
            ResolveOutcome::Create(path) => Ok(emit::create_line(path)),
            ResolveOutcome::AskMultiple(candidates) => {
                Ok(emit::ask_line(&emit::to_json_compact(candidates)?))
            }
            ResolveOutcome::AskCorrupt { .. } => Ok(emit::ask_line("[]")),
        }
    }

    /// Process exit code matching `dvandva-resolve.sh`.
    pub fn exit_code(&self) -> i32 {
        match self {
            ResolveOutcome::Resolved(_) | ResolveOutcome::Create(_) => 0,
            ResolveOutcome::AskMultiple(_) | ResolveOutcome::AskCorrupt { .. } => 12,
        }
    }
}

/// Usage/cwd failures that map to resolver exit code 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Usage(String),
    Cwd { path: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Usage(message) => write!(f, "{message}"),
            ResolveError::Cwd { path } => write!(f, "--cwd is not a directory: {path}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolve the active run using explicit selectors first, then discovery.
///
/// The implementation intentionally reads discovered baton files as raw
/// `serde_json::Value` instead of the strict typed [`crate::baton::Baton`]:
/// discovery must tolerate future status tokens, missing checkpoints, and
/// sparse-but-valid JSON exactly like the jq shell implementation.
pub fn resolve_active_run(
    _role: Role,
    cwd: Option<&Path>,
    env: ResolveEnv,
) -> Result<ResolveOutcome, ResolveError> {
    let explicit = explicit_selector(&env)?;
    let root = resolve_cwd(cwd)?;
    if let Some(path) = explicit {
        return Ok(ResolveOutcome::Resolved(path));
    }
    discover_runs(&root)
}

fn env_string(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

fn explicit_selector(env: &ResolveEnv) -> Result<Option<String>, ResolveError> {
    if let Some(path) = env.baton_file.as_ref().filter(|value| !value.is_empty()) {
        return Ok(Some(path.clone()));
    }
    if let Some(dir) = env.run_dir.as_ref().filter(|value| !value.is_empty()) {
        let base = strip_one_trailing_slash(dir);
        return Ok(Some(format!("{base}/baton.json")));
    }
    if let Some(run_id) = env.run_id.as_ref() {
        if !util::is_safe_run_id(run_id) {
            return Err(ResolveError::Usage(UNSAFE_RUN_ID_MESSAGE.to_string()));
        }
        return Ok(Some(format!(".dvandva/runs/{run_id}/baton.json")));
    }
    Ok(None)
}

fn strip_one_trailing_slash(value: &str) -> &str {
    value.strip_suffix('/').unwrap_or(value)
}

fn resolve_cwd(cwd: Option<&Path>) -> Result<PathBuf, ResolveError> {
    match cwd {
        Some(path) if path.is_dir() => Ok(path.to_path_buf()),
        Some(path) => Err(ResolveError::Cwd {
            path: path.display().to_string(),
        }),
        None => std::env::current_dir().map_err(|_| ResolveError::Cwd {
            path: ".".to_string(),
        }),
    }
}

fn discover_runs(root: &Path) -> Result<ResolveOutcome, ResolveError> {
    let mut files = candidate_files(root);
    files.sort_by(|a, b| a.relative.cmp(&b.relative));

    let legacy = root.join(".dvandva/baton.json");
    if legacy.symlink_metadata().is_ok() {
        files.push(CandidateFile {
            absolute: legacy,
            relative: ".dvandva/baton.json".to_string(),
            fallback_run_id: "legacy".to_string(),
        });
    }

    let mut resumable = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file.absolute) else {
            return Ok(ResolveOutcome::AskCorrupt {
                path: file.relative,
            });
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Ok(ResolveOutcome::AskCorrupt {
                path: file.relative,
            });
        };
        let candidate = RunCandidate::from_value(&value, &file.relative, &file.fallback_run_id);
        if !RESOLVE_TERMINAL_STATUSES.contains(&candidate.status.as_str()) {
            resumable.push(candidate);
        }
    }

    resumable.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });

    match resumable.len() {
        0 => Ok(ResolveOutcome::Create(format!(
            ".dvandva/runs/{}/baton.json",
            derive_create_slug(root)
        ))),
        1 => Ok(ResolveOutcome::Resolved(resumable[0].path.clone())),
        _ => Ok(ResolveOutcome::AskMultiple(resumable)),
    }
}

struct CandidateFile {
    absolute: PathBuf,
    relative: String,
    fallback_run_id: String,
}

fn candidate_files(root: &Path) -> Vec<CandidateFile> {
    let mut files = Vec::new();
    let runs_dir = root.join(".dvandva/runs");
    let Ok(entries) = fs::read_dir(runs_dir) else {
        return files;
    };

    for entry in entries.flatten() {
        let run_dir = entry.path();
        let baton = run_dir.join("baton.json");
        if baton.symlink_metadata().is_err() {
            continue;
        }
        let fallback_run_id = entry.file_name().to_string_lossy().into_owned();
        let relative = format!(".dvandva/runs/{fallback_run_id}/baton.json");
        files.push(CandidateFile {
            absolute: baton,
            relative,
            fallback_run_id,
        });
    }
    files
}

impl RunCandidate {
    fn from_value(value: &Value, path: &str, fallback_run_id: &str) -> RunCandidate {
        let run_id = object_field(value, "run_id").filter(|s| !s.is_empty());
        RunCandidate::new(
            run_id.unwrap_or_else(|| fallback_run_id.to_string()),
            path,
            object_field(value, "status").unwrap_or_default(),
            object_field(value, "assignee").unwrap_or_default(),
            object_field(value, "updated_at").unwrap_or_default(),
        )
    }
}

// Extract a discovery field (run_id/status/assignee/updated_at) as a string,
// coalescing null/false to None exactly like jq `.x // ""` treats them as absent.
//
// SYNTHETIC RESIDUAL (numeric fields): the shell resolver does NOT apply
// `tostring` to these fields, so a NUMERIC `run_id`/`status`/`assignee`/`updated_at`
// (e.g. `run_id: 1.5`) surfaces in the jq ASK array as a JSON NUMBER, whereas
// this helper stringifies it (`"1.5"`). Real Dvandva batons always carry these
// as strings, so the divergence is unreachable in practice; preserving the JSON
// number type here would require RunCandidate to hold `Value` and would ripple
// into the updated_at/run_id sort ordering (which must stay byte-identical to
// the shell), so it is intentionally left as a documented synthetic residual
// rather than fixed. See rust/dvandva/README.md "Known limitations".
//
// KNOWN RESIDUAL (exponential): as in state's bounded_scalar, a numeric literal
// in exponential form stringifies with a lowercase `e` here vs jq's uppercase
// `E` (`1e10` -> "1e+10" vs "1E+10"); also synthetic.
fn object_field(value: &Value, key: &str) -> Option<String> {
    let field = value.as_object()?.get(key)?;
    match field {
        Value::Null | Value::Bool(false) => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn derive_create_slug(root: &Path) -> String {
    let base = "run";
    if !root.join(".dvandva/runs").join(base).exists() {
        return base.to_string();
    }

    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !root.join(".dvandva/runs").join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Bootstrap state of a selector-targeted run directory, shared by the
/// `preflight` and `resolve` command layers (deslop: previously duplicated
/// byte-identically in both). Lives here — not in `resolve_active_run` —
/// so the resolver itself keeps its no-filesystem-checks contract.
pub enum SelectorBootstrap {
    /// The selected baton file parses as JSON: normal resolution.
    ValidBaton,
    /// The selected baton file is missing and the run dir carries no stale
    /// leftovers: a clean bootstrap (arm for vadi, wait for prativadi).
    MissingClean,
    /// The run dir holds stale state a human must clear first; the payload
    /// is the `detail=` token (`invalid_baton` | `invalid_candidate` |
    /// `garbage_marker`).
    StaleRunDir(&'static str),
}

/// Classify a selector-targeted `baton_path` for bootstrap handling.
pub fn selector_bootstrap_state(root: &Path, baton_path: &Path) -> SelectorBootstrap {
    match util::read_json_lenient(baton_path) {
        Ok(_) => SelectorBootstrap::ValidBaton,
        Err(util::JsonReadError::Invalid) => SelectorBootstrap::StaleRunDir("invalid_baton"),
        Err(util::JsonReadError::Missing) => {
            let run_dir = baton_path.parent().unwrap_or(root);
            match util::read_json_lenient(&run_dir.join("baton.next.json")) {
                Err(util::JsonReadError::Invalid) => {
                    return SelectorBootstrap::StaleRunDir("invalid_candidate");
                }
                Ok(_) | Err(util::JsonReadError::Missing) => {}
            }
            let marker = crate::sla_marker::marker_path(root);
            if marker.exists() && crate::sla_marker::read(root).is_none() {
                return SelectorBootstrap::StaleRunDir("garbage_marker");
            }
            SelectorBootstrap::MissingClean
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::Role;

    use super::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome, RunCandidate};

    fn temp_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dvandva-resolve-{name}-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seed_baton(root: &Path, run_id: &str, status: &str, updated_at: &str) {
        let path = root.join(".dvandva/runs").join(run_id).join("baton.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!(
                r#"{{
  "schema": "dvandva.baton.v2",
  "run_id": "{run_id}",
  "status": "{status}",
  "assignee": "vadi",
  "phase": 1,
  "checkpoint": 3,
  "updated_at": "{updated_at}"
}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn explicit_selectors_short_circuit_without_file_checks() {
        let root = temp_root("explicit");
        seed_baton(&root, "active", "implementing", "2026-07-01T00:00:00Z");

        let outcome = resolve_active_run(
            Role::Vadi,
            Some(&root),
            ResolveEnv {
                baton_file: Some("/tmp/no-such-dvandva-baton.json".to_string()),
                run_dir: Some(root.join(".dvandva/runs/active").display().to_string()),
                run_id: Some("active".to_string()),
            },
        )
        .unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::Resolved("/tmp/no-such-dvandva-baton.json".to_string())
        );
    }

    #[test]
    fn unsafe_run_id_is_rejected_before_cwd_lookup() {
        let missing = temp_root("unsafe").join("does-not-exist");
        let err = resolve_active_run(
            Role::Vadi,
            Some(&missing),
            ResolveEnv {
                run_id: Some("../escape".to_string()),
                ..ResolveEnv::default()
            },
        )
        .unwrap_err();

        match err {
            ResolveError::Usage(message) => assert!(message.contains("safe path segment")),
            other => panic!("expected unsafe selector usage error, got {other:?}"),
        }
    }

    #[test]
    fn discovery_tolerates_future_statuses_and_sorts_for_ask() {
        let root = temp_root("future-status");
        seed_baton(&root, "alpha", "future_status", "2026-07-01T10:00:00Z");
        seed_baton(&root, "beta", "implementing", "2026-07-01T11:00:00Z");
        let no_checkpoint = root.join(".dvandva/runs/gamma/baton.json");
        fs::create_dir_all(no_checkpoint.parent().unwrap()).unwrap();
        fs::write(
            no_checkpoint,
            r#"{
  "schema": "dvandva.baton.v2",
  "run_id": "gamma",
  "status": "human_question",
  "assignee": "human",
  "updated_at": "2026-07-01T11:00:00Z"
}"#,
        )
        .unwrap();

        let outcome =
            resolve_active_run(Role::Prativadi, Some(&root), ResolveEnv::default()).unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::AskMultiple(vec![
                RunCandidate::new(
                    "beta",
                    ".dvandva/runs/beta/baton.json",
                    "implementing",
                    "vadi",
                    "2026-07-01T11:00:00Z"
                ),
                RunCandidate::new(
                    "gamma",
                    ".dvandva/runs/gamma/baton.json",
                    "human_question",
                    "human",
                    "2026-07-01T11:00:00Z"
                ),
                RunCandidate::new(
                    "alpha",
                    ".dvandva/runs/alpha/baton.json",
                    "future_status",
                    "vadi",
                    "2026-07-01T10:00:00Z"
                )
            ])
        );
    }

    #[test]
    fn corrupt_baton_during_discovery_fails_closed() {
        let root = temp_root("corrupt");
        seed_baton(&root, "active", "implementing", "2026-07-01T00:00:00Z");
        let corrupt = root.join(".dvandva/runs/corrupt/baton.json");
        fs::create_dir_all(corrupt.parent().unwrap()).unwrap();
        fs::write(&corrupt, "{ not valid json\n").unwrap();

        let outcome = resolve_active_run(Role::Vadi, Some(&root), ResolveEnv::default()).unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::AskCorrupt {
                path: ".dvandva/runs/corrupt/baton.json".to_string()
            }
        );
    }

    #[test]
    fn only_done_archives_create_first_free_run_slug() {
        let root = temp_root("create");
        seed_baton(&root, "run", "done", "2026-07-01T00:00:00Z");

        let outcome = resolve_active_run(Role::Vadi, Some(&root), ResolveEnv::default()).unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::Create(".dvandva/runs/run-2/baton.json".to_string())
        );
    }

    #[test]
    fn only_abandoned_archives_create_first_free_run_slug() {
        let root = temp_root("create-abandoned");
        seed_baton(&root, "run", "abandoned", "2026-07-01T00:00:00Z");

        let outcome = resolve_active_run(Role::Vadi, Some(&root), ResolveEnv::default()).unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::Create(".dvandva/runs/run-2/baton.json".to_string())
        );
    }

    #[test]
    fn abandoned_and_done_are_both_excluded_from_resumable_set() {
        let root = temp_root("abandoned-mixed");
        seed_baton(&root, "done-run", "done", "2026-07-01T00:00:00Z");
        seed_baton(&root, "abandoned-run", "abandoned", "2026-07-01T00:00:00Z");
        seed_baton(&root, "active-run", "implementing", "2026-07-01T00:00:00Z");

        let outcome = resolve_active_run(Role::Vadi, Some(&root), ResolveEnv::default()).unwrap();

        assert_eq!(
            outcome,
            ResolveOutcome::Resolved(".dvandva/runs/active-run/baton.json".to_string())
        );
    }
}
