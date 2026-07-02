//! Off-protocol commit detection: finds commits made while a Dvandva run
//! was active but missing the `Dvandva-Checkpoint: <N>` trailer.
//!
//! Mirrors `dvandva-drift-lint.sh`: locate the most recent commit carrying
//! the trailer, floor the scan at the hook-adoption baseline when one is
//! recorded, then flag every commit between that floor and `HEAD` that
//! lacks the trailer. A repo with no checkpointed commits at all and no
//! active baton is not drift — pre-run or non-Dvandva history is exempt.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::gitcfg;
use crate::util;

/// Sentinel recorded when hooks are installed in a repo with no root commit
/// yet; backfilled to the root sha the first time a commit exists to floor
/// the scan against.
pub const PENDING_ROOT_BASELINE: &str = "__DVANDVA_ROOT_PENDING__";

const NO_CHECKPOINTS_MESSAGE: &str =
    "DVANDVA_DRIFT ok: no checkpointed commits in history — nothing to lint.";

/// The outcome of a drift-lint scan: the process exit code plus the exact
/// stdout/stderr lines the shell helper would have printed, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftLintReport {
    pub exit_code: i32,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}

impl DriftLintReport {
    fn not_a_repo() -> DriftLintReport {
        DriftLintReport {
            exit_code: 1,
            stdout: Vec::new(),
            stderr: vec!["dvandva drift-lint: not inside a git repository".to_string()],
        }
    }
}

/// Run the drift lint from `cwd`, discovering the repository root the same
/// way the shell helper does (`git rev-parse --show-toplevel`).
pub fn lint_drift(cwd: &Path, warn_only: bool) -> DriftLintReport {
    let Some(repo_root) = gitcfg::repo_toplevel(cwd) else {
        return DriftLintReport::not_a_repo();
    };
    lint_repo(&repo_root, warn_only)
}

fn lint_repo(repo_root: &Path, warn_only: bool) -> DriftLintReport {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = match find_last_checkpoint(repo_root) {
        None => no_checkpoint_branch(repo_root, warn_only, &mut stdout, &mut stderr),
        Some((checkpoint_sha, checkpoint_num)) => checkpointed_branch(
            repo_root,
            warn_only,
            &checkpoint_sha,
            checkpoint_num,
            &mut stdout,
            &mut stderr,
        ),
    };

    DriftLintReport {
        exit_code,
        stdout,
        stderr,
    }
}

/// No commit anywhere in history carries a `Dvandva-Checkpoint` trailer.
/// Not drift unless a baton is actively running: then unstamped commits are
/// visible first-run bypasses and must be reported.
fn no_checkpoint_branch(
    repo_root: &Path,
    warn_only: bool,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
) -> i32 {
    if !active_baton_exists(repo_root, stderr) {
        stdout.push(NO_CHECKPOINTS_MESSAGE.to_string());
        return 0;
    }

    if !head_exists(repo_root) {
        stdout.push(NO_CHECKPOINTS_MESSAGE.to_string());
        return 0;
    }

    let (base, inclusive, context) = match adoption_baseline(repo_root, stderr) {
        Some((sha, inclusive)) => {
            let context = format!("since hook adoption baseline {sha}");
            (Some(sha), inclusive, context)
        }
        None => (
            None,
            false,
            "while an active baton exists and no checkpoint baseline exists".to_string(),
        ),
    };

    let drift = collect_drift(repo_root, base.as_deref(), inclusive);
    report_scan(warn_only, &drift, &context, true, stdout, stderr)
}

/// At least one commit in history carries the trailer: floor the scan at
/// that checkpoint, or further back at the hook-adoption baseline when the
/// baseline predates it (catching stamp -> no-verify -> stamp sandwiches).
fn checkpointed_branch(
    repo_root: &Path,
    warn_only: bool,
    checkpoint_sha: &str,
    checkpoint_num: u64,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
) -> i32 {
    let mut base = checkpoint_sha.to_string();
    let mut inclusive = false;
    let mut context = format!("since checkpoint {checkpoint_num} ({checkpoint_sha})");

    if let Some((adopt_sha, adopt_inclusive)) = adoption_baseline(repo_root, stderr) {
        if is_ancestor(repo_root, &adopt_sha, checkpoint_sha) {
            base = adopt_sha.clone();
            inclusive = adopt_inclusive;
            context = format!(
                "since hook adoption baseline {adopt_sha} (checkpoint {checkpoint_num} at {checkpoint_sha})"
            );
        } else if is_ancestor(repo_root, checkpoint_sha, &adopt_sha) {
            base = checkpoint_sha.to_string();
            inclusive = false;
            context = format!(
                "since checkpoint {checkpoint_num} ({checkpoint_sha}), before later hook adoption baseline {adopt_sha}"
            );
        } else {
            stderr.push(format!(
                "DVANDVA_DRIFT warning: dvandva.hooksAdoptedAt baseline is not in checkpoint ancestry: {adopt_sha}"
            ));
        }
    }

    let drift = collect_drift(repo_root, Some(&base), inclusive);
    report_scan(warn_only, &drift, &context, false, stdout, stderr)
}

/// Push the ok/warning lines for a completed scan and derive the exit code.
/// `trailing_period` distinguishes the no-checkpoint branch's ok message
/// (which the shell terminates with a period) from the checkpointed
/// branch's (which does not) — both are mirrored verbatim.
fn report_scan(
    warn_only: bool,
    drift: &[(String, String)],
    context: &str,
    trailing_period: bool,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
) -> i32 {
    if drift.is_empty() {
        let suffix = if trailing_period { "." } else { "" };
        stdout.push(format!(
            "DVANDVA_DRIFT ok: no off-protocol commits {context}{suffix}"
        ));
        return 0;
    }

    stderr.push(format!(
        "DVANDVA_DRIFT warning: {} off-protocol commit(s) found {context}",
        drift.len()
    ));
    for (sha, subject) in drift {
        stderr.push(format!("  {sha}  {subject}"));
    }

    if warn_only {
        stderr.push(
            "DVANDVA_DRIFT advisory: off-protocol commits detected — pass --warn suppresses failure."
                .to_string(),
        );
        return 0;
    }

    1
}

/// Find the most recent commit (newest first) whose body carries a
/// `Dvandva-Checkpoint: <N>` trailer.
fn find_last_checkpoint(repo_root: &Path) -> Option<(String, u64)> {
    let log = gitcfg::git_stdout(repo_root, &["log", "--format=%H"])?;
    for sha in log.lines() {
        if sha.is_empty() {
            continue;
        }
        let Some(body) = gitcfg::git_stdout(repo_root, &["show", "-s", "--format=%B", sha]) else {
            continue;
        };
        if let Some(num) = checkpoint_number(&body) {
            return Some((sha.to_string(), num));
        }
    }
    None
}

/// True if any line begins the `Dvandva-Checkpoint:` trailer followed by at
/// least one whitespace character (mirrors `grep -qE
/// "^Dvandva-Checkpoint:[[:space:]]"` — no digit required, unlike
/// [`checkpoint_number`]).
fn has_checkpoint_trailer(body: &str) -> bool {
    body.lines().any(|line| {
        line.strip_prefix("Dvandva-Checkpoint:")
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_whitespace()))
    })
}

/// The numeric value of the first `Dvandva-Checkpoint: <N>` trailer line
/// (mirrors `grep -oE "^Dvandva-Checkpoint:[[:space:]]+[0-9]+"` followed by
/// `grep -oE "[0-9]+$"`).
fn checkpoint_number(body: &str) -> Option<u64> {
    for line in body.lines() {
        let Some(rest) = line.strip_prefix("Dvandva-Checkpoint:") else {
            continue;
        };
        let trimmed = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        if trimmed.len() == rest.len() {
            continue;
        }
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(num) = digits.parse::<u64>() {
            return Some(num);
        }
    }
    None
}

/// The commit shas to scan: everything after `base` up to `HEAD`, plus
/// `base` itself when `include_base` (an inclusive adoption baseline), or
/// the full history when `base` is `None`.
fn scan_log_shas(repo_root: &Path, base: Option<&str>, include_base: bool) -> Vec<String> {
    let mut shas = Vec::new();
    match base {
        None => {
            if let Some(log) = gitcfg::git_stdout(repo_root, &["log", "--format=%H"]) {
                shas.extend(log.lines().map(str::to_string));
            }
        }
        Some(base) => {
            let range = format!("{base}..HEAD");
            if let Some(log) = gitcfg::git_stdout(repo_root, &["log", "--format=%H", &range]) {
                shas.extend(log.lines().map(str::to_string));
            }
            if include_base {
                shas.push(base.to_string());
            }
        }
    }
    shas
}

/// Every commit in the scan range that lacks the checkpoint trailer, paired
/// with its subject line for reporting.
fn collect_drift(
    repo_root: &Path,
    base: Option<&str>,
    include_base: bool,
) -> Vec<(String, String)> {
    let mut drift = Vec::new();
    for sha in scan_log_shas(repo_root, base, include_base) {
        if sha.is_empty() {
            continue;
        }
        let Some(body) = gitcfg::git_stdout(repo_root, &["show", "-s", "--format=%B", &sha]) else {
            continue;
        };
        if !has_checkpoint_trailer(&body) {
            let subject = gitcfg::git_stdout(repo_root, &["show", "-s", "--format=%s", &sha])
                .unwrap_or_else(|| "(unreadable)".to_string());
            drift.push((sha, subject));
        }
    }
    drift
}

/// Terminal statuses are inactive for drift purposes.
fn is_terminal_status(status: &str) -> bool {
    matches!(status, "done" | "human_question" | "human_decision")
}

/// Baton file candidates: `.dvandva/baton.json` plus one level of
/// `.dvandva/runs/*/baton.json` (mirrors `find -maxdepth 2 -name
/// baton.json`).
fn baton_candidates(repo_root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    let root_baton = repo_root.join(".dvandva").join("baton.json");
    if root_baton.is_file() {
        candidates.push(root_baton);
    }

    let runs_dir = repo_root.join(".dvandva").join("runs");
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        let mut run_dirs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        run_dirs.sort();
        for dir in run_dirs {
            let candidate = dir.join("baton.json");
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }

    candidates
}

/// jq `-r '.status // ""'` semantics: coalesce null/false to absent, then
/// render as a raw string.
fn status_string(value: &Value) -> String {
    let status = value.as_object().and_then(|map| map.get("status"));
    match util::coalesce(status) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// True when at least one candidate baton represents active (non-terminal)
/// work. Malformed JSON fails closed: it is treated as active and pushes a
/// warning, matching the shell's `jq empty` check.
fn active_baton_exists(repo_root: &Path, stderr: &mut Vec<String>) -> bool {
    let candidates = baton_candidates(repo_root);
    if candidates.is_empty() {
        return false;
    }

    for path in candidates {
        match util::read_json_lenient(&path) {
            Err(_) => {
                stderr.push(format!(
                    "DVANDVA_DRIFT warning: malformed baton JSON: {}",
                    path.display()
                ));
                return true;
            }
            Ok(value) => {
                if !is_terminal_status(&status_string(&value)) {
                    return true;
                }
            }
        }
    }

    false
}

fn head_exists(repo_root: &Path) -> bool {
    gitcfg::git(repo_root, &["rev-parse", "--verify", "HEAD"])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn commit_exists(repo_root: &Path, sha: &str) -> bool {
    let spec = format!("{sha}^{{commit}}");
    gitcfg::git(repo_root, &["cat-file", "-e", &spec])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn adoption_inclusive_flag(repo_root: &Path) -> bool {
    gitcfg::cfg_get(repo_root, "dvandva.hooksAdoptedAtInclusive")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn is_ancestor(repo_root: &Path, ancestor: &str, descendant: &str) -> bool {
    gitcfg::git(
        repo_root,
        &["merge-base", "--is-ancestor", ancestor, descendant],
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

/// Read `dvandva.hooksAdoptedAt`, backfilling a pending root-commit marker
/// and resolving `dvandva.hooksAdoptedAtInclusive`. Returns `None` when no
/// usable baseline exists; an unresolvable non-pending value pushes a
/// warning (mirrors the shell's `hook_adoption_baseline`).
fn adoption_baseline(repo_root: &Path, stderr: &mut Vec<String>) -> Option<(String, bool)> {
    let baseline = gitcfg::cfg_get(repo_root, "dvandva.hooksAdoptedAt")?;
    if baseline.is_empty() {
        return None;
    }

    if baseline == PENDING_ROOT_BASELINE {
        let root_sha = gitcfg::git_stdout(
            repo_root,
            &["rev-list", "--max-parents=0", "--reverse", "HEAD"],
        )
        .and_then(|out| out.lines().next().map(str::to_string))?;
        if root_sha.is_empty() {
            return None;
        }
        gitcfg::cfg_set_local(repo_root, "dvandva.hooksAdoptedAt", &root_sha);
        gitcfg::cfg_set_local(repo_root, "dvandva.hooksAdoptedAtInclusive", "true");
        return Some((root_sha, true));
    }

    if commit_exists(repo_root, &baseline) {
        return Some((baseline.clone(), adoption_inclusive_flag(repo_root)));
    }

    stderr.push(format!(
        "DVANDVA_DRIFT warning: invalid dvandva.hooksAdoptedAt baseline: {baseline}"
    ));
    None
}
