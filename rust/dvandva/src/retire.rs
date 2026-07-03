//! `retire` logic — reversible retirement of the 5 standalone Claude
//! user-agent symlinks superseded by the dvandva-* roster.
//!
//! Ports `scripts/retire-standalone-agents.sh`. Three flows:
//!
//! * [`dry_run_report`] — read-only preview, never touches the filesystem.
//! * [`run_apply`] — parity-gated retirement: moves the 5 allowlisted
//!   symlinks into a timestamped backup dir and writes `manifest.json`.
//! * [`run_restore`] — validates a prior `manifest.json` as a whole (allowlist
//!   membership, path bounds, backup entries are symlinks) before moving
//!   anything back.
//!
//! Every side-effecting flow returns `(stdout, stderr, exit_code)` rather
//! than printing directly, so the CLI wrapper (`cmd::retire`) and tests can
//! both drive it; wording that the shell test suite asserts on (`WOULD
//! RETIRE`, `RETIRED`, `RESTORED`, `parity`, `no-op`, `allowlist`, `already
//! restored`, `N agent(s) retired`, `Invalid manifest entry`) is preserved
//! verbatim.
//!
//! Skills are out of scope: the retirement helper never touches skill
//! files — only the five allowlisted agent symlinks.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::emit;
use crate::util::utc_compact_timestamp;

/// Exactly the 5 standalone agents eligible for retirement.
pub const ALLOWLIST: [&str; 5] = [
    "adversarial-analyst.md",
    "architect.md",
    "developer.md",
    "quality-reviewer.md",
    "sandbox-executor.md",
];

/// All 15 dvandva-* agent files that must be present in the plugin cache for
/// the parity gate to pass.
pub const REQUIRED_AGENTS: [&str; 15] = [
    "adversarial-analyst.md",
    "architect.md",
    "baton-auditor.md",
    "cross-reviewer.md",
    "debugger.md",
    "deep-reviewer.md",
    "deslopper.md",
    "doc-verifier.md",
    "implementer.md",
    "integration-checker.md",
    "pattern-mapper.md",
    "researcher.md",
    "sandbox-verifier.md",
    "security-auditor.md",
    "test-creator.md",
];

/// Default `DVANDVA_EXPECTED_VERSION` when the env var is unset or empty.
///
/// The shell source (`scripts/retire-standalone-agents.sh`) pinned `1.1.0`;
/// the Rust port moved to `1.2.0`, the flow patches bumped it to `1.3.0`, the
/// S2/S4/S5/S6 hardening slice bumped it to `1.4.0`, the html-deliverables
/// skill to `1.4.1`, and the wait-through-human docs wave bumps the default
/// to `1.4.2` to track the plugin version being shipped alongside them.
pub const DEFAULT_EXPECTED_VERSION: &str = "1.4.2";

/// Is `candidate` (a bare filename, e.g. `"architect.md"`) one of the 5
/// standalone agents eligible for retirement?
pub fn is_allowlisted(candidate: &str) -> bool {
    ALLOWLIST.contains(&candidate)
}

/// Resolved environment/paths for a `retire-agents` invocation, honouring the
/// same overrides the shell script reads (`HOME`, `CODEX_HOME`,
/// `DVANDVA_EXPECTED_VERSION`).
#[derive(Debug, Clone)]
pub struct RetirePaths {
    pub claude_agents_dir: String,
    pub dvandva_cache_base: String,
    pub codex_home: String,
    pub expected_version: String,
}

impl RetirePaths {
    /// Build from `HOME` plus the two optional overrides. `codex_home`
    /// defaults to `$HOME/.codex`; `expected_version` defaults to
    /// [`DEFAULT_EXPECTED_VERSION`] — both treat an empty string as absent,
    /// mirroring bash's `${VAR:-default}`.
    pub fn from_env(home: &str, codex_home: Option<&str>, expected_version: Option<&str>) -> Self {
        let codex_home = codex_home
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{home}/.codex"));
        let expected_version = expected_version
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| DEFAULT_EXPECTED_VERSION.to_string());

        RetirePaths {
            claude_agents_dir: format!("{home}/.claude/agents"),
            dvandva_cache_base: format!("{home}/.claude/plugins/cache/dvandva/dvandva"),
            codex_home,
            expected_version,
        }
    }
}

/// Whether a symlink/file/dangling-symlink is present at `path` at all
/// (mirrors bash's `[[ -e "$p" || -L "$p" ]]`).
fn path_present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// The on-disk state of one allowlisted agent name under the agents dir.
enum AgentStatus {
    Symlink { target: String },
    NotSymlink,
    NotPresent,
}

fn agent_status(claude_agents_dir: &str, agent: &str) -> AgentStatus {
    let src = Path::new(claude_agents_dir).join(agent);
    match fs::symlink_metadata(&src) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::read_link(&src)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            AgentStatus::Symlink { target }
        }
        Ok(_) => AgentStatus::NotSymlink,
        Err(_) => AgentStatus::NotPresent,
    }
}

/// Refuse `--apply` unless the dvandva cache at `expected_version` contains
/// all 15 required agent files. `Ok` carries the "Parity OK" line; `Err`
/// carries the full multi-line failure report (both destined for the
/// caller's stdout/stderr respectively).
pub fn parity_gate(paths: &RetirePaths) -> Result<String, String> {
    let cache_agents = format!(
        "{}/{}/agents",
        paths.dvandva_cache_base, paths.expected_version
    );
    let cache_path = Path::new(&cache_agents);

    if !cache_path.is_dir() {
        return Err(format!(
            "PARITY FAIL: dvandva {version} cache not found.\n  Expected directory: {cache_agents}\n  Install dvandva {version} first: dvandva install\n",
            version = paths.expected_version,
        ));
    }

    let missing: Vec<&str> = REQUIRED_AGENTS
        .iter()
        .copied()
        .filter(|agent| !cache_path.join(agent).is_file())
        .collect();

    if !missing.is_empty() {
        let mut message = format!(
            "PARITY FAIL: dvandva {version} cache is incomplete.\n  Missing {count} agent(s):\n",
            version = paths.expected_version,
            count = missing.len(),
        );
        for agent in &missing {
            message.push_str(&format!("    {agent}\n"));
        }
        message.push_str(&format!(
            "  Reinstall dvandva {}: dvandva install\n",
            paths.expected_version
        ));
        return Err(message);
    }

    Ok(format!(
        "Parity OK: dvandva {} cache has all {} required agent files.\n",
        paths.expected_version,
        REQUIRED_AGENTS.len()
    ))
}

/// Codex side-report: always a no-op, never retires anything from
/// `$CODEX_HOME`.
pub fn codex_check(codex_home: &str) -> String {
    let mut found = false;
    for subdir in ["agents", "prompts", "subagents"] {
        let dir = Path::new(codex_home).join(subdir);
        if dir.is_dir() {
            if let Ok(mut entries) = fs::read_dir(&dir) {
                if entries.next().is_some() {
                    found = true;
                    break;
                }
            }
        }
    }

    if found {
        format!(
            "Codex ({codex_home}): agent-axis files found but outside retirement allowlist (no-op).\n"
        )
    } else {
        format!("Codex ({codex_home}): no agent-axis files to retire (no-op).\n")
    }
}

/// Build the dry-run report: what WOULD be retired, touching nothing.
pub fn dry_run_report(paths: &RetirePaths) -> String {
    let mut out = String::new();
    out.push_str("=== Dvandva Standalone Agent Retirement (DRY RUN) ===\n");
    out.push_str(&format!(
        "Allowlisted agents directory: {}\n\n",
        paths.claude_agents_dir
    ));

    let mut found = 0usize;
    for agent in ALLOWLIST {
        match agent_status(&paths.claude_agents_dir, agent) {
            AgentStatus::Symlink { target } => {
                out.push_str(&format!("  WOULD RETIRE: {agent} -> {target}\n"));
                found += 1;
            }
            AgentStatus::NotSymlink => {
                out.push_str(&format!("  SKIP (not a symlink): {agent}\n"));
            }
            AgentStatus::NotPresent => {
                out.push_str(&format!("  SKIP (not present): {agent}\n"));
            }
        }
    }

    out.push('\n');
    if found == 0 {
        out.push_str("Nothing to retire.\n");
    } else {
        out.push_str(&format!(
            "{found} symlink(s) would be moved to: {}/.retired-<timestamp>/\n",
            paths.claude_agents_dir
        ));
        out.push_str("Run with --apply to execute (requires parity gate to pass).\n");
    }

    out.push('\n');
    out.push_str(&codex_check(&paths.codex_home));
    out
}

struct RetiredEntry {
    original_path: String,
    backup_path: String,
    symlink_target: String,
}

fn build_manifest(
    retired_at: &str,
    dvandva_version: &str,
    backup_dir: &str,
    entries: &[RetiredEntry],
) -> Value {
    let entries_json: Vec<Value> = entries
        .iter()
        .map(|entry| {
            json!({
                "original_path": entry.original_path,
                "backup_path": entry.backup_path,
                "symlink_target": entry.symlink_target,
            })
        })
        .collect();

    json!({
        "retired_at": retired_at,
        "dvandva_version": dvandva_version,
        "backup_dir": backup_dir,
        "entries": entries_json,
    })
}

/// Execute `--apply`: parity gate, then move the allowlisted symlinks into a
/// timestamped backup dir and write `manifest.json`. Returns
/// `(stdout, stderr, exit_code)`.
pub fn run_apply(paths: &RetirePaths) -> (String, String, i32) {
    let mut stdout = String::new();
    stdout.push_str("=== Dvandva Standalone Agent Retirement (APPLY) ===\n\n");

    match parity_gate(paths) {
        Ok(message) => stdout.push_str(&message),
        Err(message) => return (stdout, message, 1),
    }
    stdout.push('\n');

    let ts = utc_compact_timestamp();
    let backup_dir = format!("{}/.retired-{ts}", paths.claude_agents_dir);
    if let Err(error) = fs::create_dir_all(&backup_dir) {
        return (
            stdout,
            format!("ERROR: failed to create backup dir {backup_dir}: {error}\n"),
            1,
        );
    }

    let mut entries = Vec::new();
    let mut retired = 0usize;

    for agent in ALLOWLIST {
        let src = format!("{}/{agent}", paths.claude_agents_dir);
        match agent_status(&paths.claude_agents_dir, agent) {
            AgentStatus::Symlink { target } => {
                let dst = format!("{backup_dir}/{agent}");
                if let Err(error) = fs::rename(&src, &dst) {
                    return (
                        stdout,
                        format!("ERROR: failed to move {src} to {dst}: {error}\n"),
                        1,
                    );
                }
                stdout.push_str(&format!("  RETIRED: {agent} -> {target}\n"));
                retired += 1;
                entries.push(RetiredEntry {
                    original_path: src,
                    backup_path: dst,
                    symlink_target: target,
                });
            }
            AgentStatus::NotSymlink => {
                stdout.push_str(&format!("  SKIP (not a symlink): {agent}\n"));
            }
            AgentStatus::NotPresent => {
                stdout.push_str(&format!("  SKIP (not present): {agent}\n"));
            }
        }
    }

    let manifest = build_manifest(&ts, &paths.expected_version, &backup_dir, &entries);
    let manifest_file = format!("{backup_dir}/manifest.json");
    let manifest_text = match emit::to_json_pretty(&manifest) {
        Ok(text) => text,
        Err(error) => {
            return (
                stdout,
                format!("ERROR: failed to serialize manifest: {error}\n"),
                1,
            )
        }
    };
    if let Err(error) = fs::write(&manifest_file, format!("{manifest_text}\n")) {
        return (
            stdout,
            format!("ERROR: failed to write manifest {manifest_file}: {error}\n"),
            1,
        );
    }

    stdout.push_str(&format!("\n{retired} agent(s) retired to: {backup_dir}\n"));
    stdout.push_str(&format!("Manifest: {manifest_file}\n"));
    stdout.push_str(&format!(
        "\nTo restore: dvandva retire-agents --restore '{backup_dir}'\n"
    ));

    stdout.push('\n');
    stdout.push_str(&codex_check(&paths.codex_home));

    (stdout, String::new(), 0)
}

#[derive(Debug)]
struct ValidatedEntry {
    original_path: String,
    backup_path: String,
    missing_backup: bool,
}

/// Structural + `backup_dir` checks over the whole manifest, before any
/// per-entry (allowlist/path/symlink) validation.
fn validate_manifest_shape(
    manifest: &Value,
    restore_dir: &str,
    manifest_file: &str,
) -> Result<(), String> {
    let entries_ok = manifest
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries.iter().all(|entry| {
                entry.get("original_path").is_some_and(Value::is_string)
                    && entry.get("backup_path").is_some_and(Value::is_string)
                    && entry.get("symlink_target").is_some_and(Value::is_string)
            })
        })
        .unwrap_or(false);
    let backup_dir_ok = manifest.get("backup_dir").is_some_and(Value::is_string);

    if !backup_dir_ok || !entries_ok {
        return Err(format!(
            "Manifest is not valid Dvandva retirement JSON: {manifest_file}"
        ));
    }

    let backup_dir_field = manifest["backup_dir"].as_str().unwrap_or_default();
    if backup_dir_field != restore_dir {
        return Err(format!(
            "Invalid manifest entry: backup_dir does not match restore dir: {backup_dir_field}"
        ));
    }

    Ok(())
}

/// Per-entry validation: allowlist membership, path bounds (inside the
/// agents dir / restore dir), backup entries are symlinks. Fails fast on the
/// first invalid entry — mirrors the shell's sequential `die`, so nothing
/// moves before the whole manifest is confirmed safe.
fn validate_manifest_entries(
    manifest: &Value,
    restore_dir: &str,
    claude_agents_dir: &str,
) -> Result<Vec<ValidatedEntry>, String> {
    let entries_field = manifest
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut validated = Vec::with_capacity(entries_field.len());
    for entry in &entries_field {
        let orig = entry
            .get("original_path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let backup = entry
            .get("backup_path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let agent = orig.rsplit('/').next().unwrap_or(orig);

        if !is_allowlisted(agent) {
            return Err(format!(
                "Invalid manifest entry: non-allowlisted agent: {orig}"
            ));
        }

        let expected_orig = format!("{claude_agents_dir}/{agent}");
        if orig != expected_orig {
            return Err(format!(
                "Invalid manifest entry: original_path outside allowlist: {orig}"
            ));
        }

        let expected_backup = format!("{restore_dir}/{agent}");
        if backup != expected_backup {
            return Err(format!(
                "Invalid manifest entry: backup_path outside restore dir: {backup}"
            ));
        }

        let backup_path = Path::new(backup);
        let missing_backup = if path_present(backup_path) {
            let is_symlink = fs::symlink_metadata(backup_path)
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false);
            if !is_symlink {
                return Err(format!(
                    "Invalid manifest entry: backup_path is not a symlink: {backup}"
                ));
            }
            false
        } else {
            true
        };

        validated.push(ValidatedEntry {
            original_path: orig.to_string(),
            backup_path: backup.to_string(),
            missing_backup,
        });
    }

    Ok(validated)
}

/// Execute `--restore <dir>`: validate the whole manifest, then move the
/// backed-up symlinks back to their original locations. Returns
/// `(stdout, stderr, exit_code)`.
pub fn run_restore(paths: &RetirePaths, restore_dir: &str) -> (String, String, i32) {
    let stdout = String::new();
    let manifest_file = format!("{restore_dir}/manifest.json");
    let manifest_path = Path::new(&manifest_file);

    if !manifest_path.is_file() {
        return (
            stdout,
            format!("ERROR: Manifest not found: {manifest_file}\n"),
            1,
        );
    }

    let bytes = match fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                stdout,
                format!("ERROR: failed to read manifest {manifest_file}: {error}\n"),
                1,
            )
        }
    };
    let manifest: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return (
                stdout,
                format!("ERROR: Manifest is not valid JSON: {manifest_file}\n"),
                1,
            )
        }
    };

    if let Err(message) = validate_manifest_shape(&manifest, restore_dir, &manifest_file) {
        return (stdout, format!("ERROR: {message}\n"), 1);
    }

    let mut stdout = stdout;
    stdout.push_str("=== Dvandva Standalone Agent Retirement (RESTORE) ===\n");
    stdout.push_str(&format!("Reading manifest: {manifest_file}\n\n"));
    stdout.push_str("Allowlist validation: restore will only move Dvandva retirement entries for the 5 approved standalone agent symlinks.\n\n");

    let entries = match validate_manifest_entries(&manifest, restore_dir, &paths.claude_agents_dir)
    {
        Ok(entries) => entries,
        Err(message) => return (stdout, format!("ERROR: {message}\n"), 1),
    };

    if entries.iter().any(|entry| entry.missing_backup) {
        return (
            stdout,
            "ERROR: no agents restored; backup appears already restored or incomplete.\n"
                .to_string(),
            1,
        );
    }

    let mut stderr = String::new();
    let mut restored = 0usize;
    let attempted = entries.len();

    for entry in &entries {
        let orig_path = Path::new(&entry.original_path);
        if path_present(orig_path) {
            stderr.push_str(&format!(
                "  WARNING: original path already occupied, skipping: {}\n",
                entry.original_path
            ));
            continue;
        }

        if let Err(error) = fs::rename(&entry.backup_path, &entry.original_path) {
            return (
                stdout,
                format!(
                    "ERROR: failed to restore {}: {error}\n",
                    entry.original_path
                ),
                1,
            );
        }
        stdout.push_str(&format!("  RESTORED: {}\n", entry.original_path));
        restored += 1;
    }

    stdout.push_str(&format!("\n{restored} agent(s) restored.\n"));

    if attempted > 0 && restored == 0 {
        stderr.push_str(
            "ERROR: no agents restored; backup appears already restored or incomplete.\n",
        );
        return (stdout, stderr, 1);
    }

    (stdout, stderr, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = tempfile::Builder::new()
            .prefix(&format!("dvandva-retire-unit-{name}-"))
            .tempdir()
            .unwrap();
        dir.keep()
    }

    #[test]
    fn is_allowlisted_accepts_and_rejects() {
        assert!(is_allowlisted("architect.md"));
        assert!(is_allowlisted("developer.md"));
        assert!(!is_allowlisted("decoy.md"));
        assert!(!is_allowlisted("baton-auditor.md"));
    }

    #[test]
    fn retire_paths_from_env_applies_defaults() {
        let paths = RetirePaths::from_env("/home/fake", None, None);
        assert_eq!(paths.claude_agents_dir, "/home/fake/.claude/agents");
        assert_eq!(
            paths.dvandva_cache_base,
            "/home/fake/.claude/plugins/cache/dvandva/dvandva"
        );
        assert_eq!(paths.codex_home, "/home/fake/.codex");
        assert_eq!(paths.expected_version, DEFAULT_EXPECTED_VERSION);
        assert_eq!(paths.expected_version, "1.4.2");
    }

    #[test]
    fn retire_paths_from_env_respects_overrides_and_treats_empty_as_absent() {
        let paths = RetirePaths::from_env("/home/fake", Some("/other/codex"), Some("1.2.0"));
        assert_eq!(paths.codex_home, "/other/codex");
        assert_eq!(paths.expected_version, "1.2.0");

        let paths_empty = RetirePaths::from_env("/home/fake", Some(""), Some(""));
        assert_eq!(paths_empty.codex_home, "/home/fake/.codex");
        assert_eq!(paths_empty.expected_version, "1.4.2");
    }

    fn build_complete_cache(base: &std::path::Path, version: &str) {
        let agents = base.join(version).join("agents");
        fs::create_dir_all(&agents).unwrap();
        for agent in REQUIRED_AGENTS {
            fs::write(agents.join(agent), "# fake\n").unwrap();
        }
    }

    #[test]
    fn parity_gate_fails_when_cache_dir_missing() {
        let root = temp_dir("no-cache");
        let paths = RetirePaths {
            claude_agents_dir: root.join("agents").display().to_string(),
            dvandva_cache_base: root.join("cache").display().to_string(),
            codex_home: root.join("codex").display().to_string(),
            expected_version: "1.2.0".to_string(),
        };

        let error = parity_gate(&paths).unwrap_err();
        assert!(error.contains("PARITY FAIL"));
        assert!(error.contains("cache not found"));
    }

    #[test]
    fn parity_gate_fails_when_agents_missing() {
        let root = temp_dir("partial-cache");
        let cache_base = root.join("cache");
        build_complete_cache(&cache_base, "1.2.0");
        fs::remove_file(cache_base.join("1.2.0/agents/debugger.md")).unwrap();

        let paths = RetirePaths {
            claude_agents_dir: root.join("agents").display().to_string(),
            dvandva_cache_base: cache_base.display().to_string(),
            codex_home: root.join("codex").display().to_string(),
            expected_version: "1.2.0".to_string(),
        };

        let error = parity_gate(&paths).unwrap_err();
        assert!(error.contains("PARITY FAIL"));
        assert!(error.contains("incomplete"));
        assert!(error.contains("debugger.md"));
    }

    #[test]
    fn parity_gate_succeeds_when_cache_complete() {
        let root = temp_dir("full-cache");
        let cache_base = root.join("cache");
        build_complete_cache(&cache_base, "1.2.0");

        let paths = RetirePaths {
            claude_agents_dir: root.join("agents").display().to_string(),
            dvandva_cache_base: cache_base.display().to_string(),
            codex_home: root.join("codex").display().to_string(),
            expected_version: "1.2.0".to_string(),
        };

        let message = parity_gate(&paths).unwrap();
        assert!(message.contains("Parity OK"));
        assert!(message.contains("15"));
    }

    #[test]
    fn codex_check_reports_no_op_for_empty_dirs() {
        let root = temp_dir("codex-empty");
        for subdir in ["agents", "prompts", "subagents"] {
            fs::create_dir_all(root.join(subdir)).unwrap();
        }

        let report = codex_check(&root.display().to_string());
        assert!(report.contains("no-op"));
        assert!(report.contains("no agent-axis files"));
    }

    #[test]
    fn codex_check_reports_files_found_for_non_empty_dirs() {
        let root = temp_dir("codex-nonempty");
        let agents = root.join("agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("stray.md"), "# stray\n").unwrap();

        let report = codex_check(&root.display().to_string());
        assert!(report.contains("no-op"));
        assert!(report.contains("outside retirement allowlist"));
    }

    #[test]
    fn validate_manifest_shape_rejects_non_string_entry_fields() {
        let manifest = json!({
            "backup_dir": "/backup",
            "entries": [{"original_path": 1, "backup_path": "/b", "symlink_target": "/t"}],
        });
        let error =
            validate_manifest_shape(&manifest, "/backup", "/backup/manifest.json").unwrap_err();
        assert!(error.contains("not valid Dvandva retirement JSON"));
    }

    #[test]
    fn validate_manifest_shape_rejects_backup_dir_mismatch() {
        let manifest = json!({
            "backup_dir": "/backup-a",
            "entries": [],
        });
        let error =
            validate_manifest_shape(&manifest, "/backup-b", "/backup-b/manifest.json").unwrap_err();
        assert!(error.contains("backup_dir does not match restore dir"));
    }

    #[test]
    fn validate_manifest_shape_accepts_well_formed_manifest() {
        let manifest = json!({
            "backup_dir": "/backup",
            "entries": [{"original_path": "/a", "backup_path": "/b", "symlink_target": "/t"}],
        });
        assert!(validate_manifest_shape(&manifest, "/backup", "/backup/manifest.json").is_ok());
    }

    #[test]
    fn validate_manifest_entries_rejects_non_allowlisted_agent() {
        let manifest = json!({
            "entries": [{
                "original_path": "/home/x/.claude/agents/decoy.md",
                "backup_path": "/backup/decoy.md",
                "symlink_target": "/src/decoy.md",
            }],
        });
        let error =
            validate_manifest_entries(&manifest, "/backup", "/home/x/.claude/agents").unwrap_err();
        assert!(error.contains("non-allowlisted agent"));
    }

    #[test]
    fn validate_manifest_entries_rejects_original_path_outside_allowlist() {
        let manifest = json!({
            "entries": [{
                "original_path": "/somewhere/else/architect.md",
                "backup_path": "/backup/architect.md",
                "symlink_target": "/src/architect.md",
            }],
        });
        let error =
            validate_manifest_entries(&manifest, "/backup", "/home/x/.claude/agents").unwrap_err();
        assert!(error.contains("original_path outside allowlist"));
    }

    #[test]
    fn validate_manifest_entries_rejects_backup_path_outside_restore_dir() {
        let manifest = json!({
            "entries": [{
                "original_path": "/home/x/.claude/agents/architect.md",
                "backup_path": "/elsewhere/architect.md",
                "symlink_target": "/src/architect.md",
            }],
        });
        let error =
            validate_manifest_entries(&manifest, "/backup", "/home/x/.claude/agents").unwrap_err();
        assert!(error.contains("backup_path outside restore dir"));
    }

    #[test]
    fn validate_manifest_entries_rejects_backup_path_not_a_symlink() {
        let root = temp_dir("backup-not-symlink");
        let backup_dir = root.join("backup");
        fs::create_dir_all(&backup_dir).unwrap();
        fs::write(backup_dir.join("architect.md"), "# regular file\n").unwrap();

        let manifest = json!({
            "entries": [{
                "original_path": "/home/x/.claude/agents/architect.md",
                "backup_path": backup_dir.join("architect.md").display().to_string(),
                "symlink_target": "/src/architect.md",
            }],
        });
        let error = validate_manifest_entries(
            &manifest,
            &backup_dir.display().to_string(),
            "/home/x/.claude/agents",
        )
        .unwrap_err();
        assert!(error.contains("backup_path is not a symlink"));
    }

    #[test]
    fn validate_manifest_entries_marks_missing_backup_when_absent() {
        let root = temp_dir("backup-missing");
        let backup_dir = root.join("backup");
        fs::create_dir_all(&backup_dir).unwrap();

        let manifest = json!({
            "entries": [{
                "original_path": "/home/x/.claude/agents/architect.md",
                "backup_path": backup_dir.join("architect.md").display().to_string(),
                "symlink_target": "/src/architect.md",
            }],
        });
        let entries = validate_manifest_entries(
            &manifest,
            &backup_dir.display().to_string(),
            "/home/x/.claude/agents",
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].missing_backup);
    }
}
