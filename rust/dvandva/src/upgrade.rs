//! `dvandva upgrade` logic — brings the whole stack current in one command.
//!
//! The public CLI delegates ordering and rollback to [`crate::upgrade_txn`]:
//! stage the new binary under an isolated `cargo install --root`, verify it,
//! mutate both plugin engines, then swap the live binary last. Any hard failure
//! rolls back reachable snapshots and exits with the transaction taxonomy.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::installers::{self, InstallTargets};
use crate::upgrade_txn::{
    run_transactional_upgrade, TransactionConfig, UpgradeExecutor, UpgradeStepError,
};

/// How many trailing lines of a subprocess's combined stdout+stderr to print
/// (`cargo install` in particular can produce a long compile log; only the
/// tail is relevant to an upgrade report).
const TAIL_LINES: usize = 20;

/// Runs the `upgrade` flow, printing progress and a final version-table
/// report, and returning the effective process exit code.
pub fn run_upgrade(marketplace: &str) -> i32 {
    let home = home_dir();
    let codex_home = installers::codex_home_dir();
    let state_dir = home.join(".dvandva");
    let live_binary_dir = cargo_bin_dir(&home);
    let config = TransactionConfig::new(marketplace, &home, &codex_home, &state_dir)
        .with_live_binary_dir(&live_binary_dir);
    let mut executor = RealUpgradeExecutor;
    let code = run_transactional_upgrade(&config, &mut executor);
    print_version_table(&live_binary_dir.join("dvandva"), &home, &codex_home);
    code
}

struct RealUpgradeExecutor;

impl UpgradeExecutor for RealUpgradeExecutor {
    fn stage_binary(&mut self, stage_root: &Path) -> Result<PathBuf, UpgradeStepError> {
        run_cargo_install_staged(stage_root)
    }

    fn verify_binary(&mut self, binary: &Path) -> Result<(), UpgradeStepError> {
        verify_dvandva_binary("verify-staged-binary", binary)
    }

    fn upgrade_plugins(&mut self, marketplace: &str) -> Result<(), UpgradeStepError> {
        run_plugins_all_or_nothing(marketplace)
    }

    fn verify_committed(&mut self, live_binary: &Path) -> Result<(), UpgradeStepError> {
        verify_dvandva_binary("verify-committed-binary", live_binary)
    }
}

// ---------------------------------------------------------------------
// Step 1: `cargo install dvandva --root <stage>`
// ---------------------------------------------------------------------

fn run_cargo_install_staged(stage_root: &Path) -> Result<PathBuf, UpgradeStepError> {
    if !installers::command_exists("cargo") {
        return Err(UpgradeStepError::new(
            "stage-binary",
            "cargo CLI not found on PATH",
        ));
    }

    println!(
        "Binary: staging with `cargo install dvandva --root {}`...",
        stage_root.display()
    );
    let args = cargo_install_args(stage_root);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (combined, code) = installers::capture_combined("cargo", &refs);
    let tail = tail_lines(&combined, TAIL_LINES);

    if code == 0 {
        if !tail.is_empty() {
            println!("{tail}");
        }
        let staged = stage_root.join("bin/dvandva");
        if staged.is_file() {
            return Ok(staged);
        }
        return Err(UpgradeStepError::new(
            "stage-binary",
            format!(
                "cargo install succeeded but {} is missing",
                staged.display()
            ),
        ));
    }

    if !tail.is_empty() {
        eprintln!("{tail}");
    }
    if installers::already_present_pattern().is_match(&combined) {
        let staged = stage_root.join("bin/dvandva");
        if staged.is_file() {
            println!("cargo install dvandva: already present in staging root; continuing.");
            return Ok(staged);
        }
    }
    Err(UpgradeStepError::new(
        "stage-binary",
        format!(
            "cargo install dvandva --root {} exited {code}",
            stage_root.display()
        ),
    ))
}

fn cargo_install_args(stage_root: &Path) -> Vec<String> {
    vec![
        "install".to_string(),
        "dvandva".to_string(),
        "--root".to_string(),
        stage_root.display().to_string(),
    ]
}

// ---------------------------------------------------------------------
// Step 2 (Claude-only extra): `claude plugin update dvandva@dvandva`
// ---------------------------------------------------------------------

/// `claude plugin update`'s failure wording when the plugin isn't installed
/// under the target scope, e.g. `Plugin "dvandva" not found`.
fn claude_update_not_installed_pattern() -> Regex {
    Regex::new("(?i)not found|not installed").expect("static regex")
}

fn run_claude_plugin_update(marketplace: &str) -> bool {
    println!("Claude Code: bumping plugin cache via `claude plugin update dvandva@dvandva`...");
    let (combined, code) =
        installers::capture_combined("claude", &["plugin", "update", "dvandva@dvandva"]);
    let tail = tail_lines(&combined, TAIL_LINES);

    if code == 0 {
        if !tail.is_empty() {
            println!("{tail}");
        }
        return true;
    }

    if !tail.is_empty() {
        eprintln!("{tail}");
    }
    if claude_update_not_installed_pattern().is_match(&combined) {
        println!(
            "NOTE: claude plugin update reports dvandva@dvandva is not installed; \
             falling back to the normal install path."
        );
        return installers::run_install(
            InstallTargets {
                claude: true,
                codex: false,
            },
            marketplace,
        ) == 0;
    }
    false
}

fn run_plugins_all_or_nothing(marketplace: &str) -> Result<(), UpgradeStepError> {
    let claude_install_ok = installers::run_install(
        InstallTargets {
            claude: true,
            codex: false,
        },
        marketplace,
    ) == 0;
    let codex_ok = installers::run_install(
        InstallTargets {
            claude: false,
            codex: true,
        },
        marketplace,
    ) == 0;
    let claude_update_ok = claude_install_ok && run_claude_plugin_update(marketplace);

    if plugins_committed(claude_install_ok, claude_update_ok, codex_ok) {
        return Ok(());
    }

    Err(UpgradeStepError::new(
        "plugins",
        format!(
            "plugin upgrade did not fully commit: claude_install={claude_install_ok}, \
             claude_update={claude_update_ok}, codex={codex_ok}"
        ),
    ))
}

fn plugins_committed(claude_install_ok: bool, claude_update_ok: bool, codex_ok: bool) -> bool {
    claude_install_ok && claude_update_ok && codex_ok
}

fn verify_dvandva_binary(stage: &'static str, binary: &Path) -> Result<(), UpgradeStepError> {
    match Command::new(binary).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.trim().starts_with("dvandva ") {
                Ok(())
            } else {
                Err(UpgradeStepError::new(
                    stage,
                    format!(
                        "unexpected version output from {}: {text}",
                        binary.display()
                    ),
                ))
            }
        }
        Ok(output) => Err(UpgradeStepError::new(
            stage,
            format!(
                "{} --version exited {:?}",
                binary.display(),
                output.status.code()
            ),
        )),
        Err(err) => Err(UpgradeStepError::new(
            stage,
            format!("failed to execute {} --version: {err}", binary.display()),
        )),
    }
}

/// Keeps only the last `n` lines of `text` (verbatim when it has `n` lines or
/// fewer).
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        return text.to_string();
    }
    lines[lines.len() - n..].join("\n")
}

// ---------------------------------------------------------------------
// Step 3: version-table report
// ---------------------------------------------------------------------

/// `${HOME}` as a path, empty when unset (mirrors the shell-style
/// `${HOME}/...` concatenation already used by [`installers::codex_home_dir`]
/// for the Codex side).
fn home_dir() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

fn cargo_bin_dir(home: &Path) -> PathBuf {
    if let Some(root) = non_empty_env_path("CARGO_INSTALL_ROOT") {
        return root.join("bin");
    }
    if let Some(cargo_home) = non_empty_env_path("CARGO_HOME") {
        return cargo_home.join("bin");
    }
    home.join(".cargo/bin")
}

fn non_empty_env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn print_version_table(live_binary: &Path, home: &Path, codex_home: &Path) {
    let binary_version = installed_binary_version(live_binary);
    let claude_cache_version =
        newest_cache_version(&home.join(".claude/plugins/cache/dvandva/dvandva"));
    let codex_cache_version =
        newest_cache_version(&codex_home.join("plugins/cache/dvandva/dvandva"));

    println!();
    println!("Upgrade summary:");
    println!("  binary ({}): {binary_version}", live_binary.display());
    println!("  Claude plugin cache:           {claude_cache_version}");
    println!("  Codex plugin cache:            {codex_cache_version}");
}

/// Runs the selected live `dvandva --version` as a subprocess — the *installed*
/// binary, not this (potentially stale) running process — and returns its
/// trimmed stdout, or `"unknown"` when the binary is missing or fails.
fn installed_binary_version(binary: &Path) -> String {
    match Command::new(binary).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                "unknown".to_string()
            } else {
                text
            }
        }
        _ => "unknown".to_string(),
    }
}

/// The newest version-named subdirectory directly under `cache_base`
/// (`<home>/.claude/plugins/cache/dvandva/dvandva` or
/// `<codex_home>/plugins/cache/dvandva/dvandva`), or `"unknown"` when the
/// directory is missing or has no version subdirectories.
fn newest_cache_version(cache_base: &Path) -> String {
    let Ok(entries) = fs::read_dir(cache_base) else {
        return "unknown".to_string();
    };
    let mut versions: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    if versions.is_empty() {
        return "unknown".to_string();
    }
    versions.sort_by(|a, b| compare_versions(a, b));
    versions.pop().unwrap_or_else(|| "unknown".to_string())
}

/// Compares `MAJOR.MINOR.PATCH`-style directory names numerically per
/// component, falling back to a plain string compare when a component isn't
/// numeric (so an unexpected directory name never panics, just sorts by
/// its lexical value).
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse =
        |s: &str| -> Option<Vec<u64>> { s.split('.').map(|part| part.parse().ok()).collect() };
    match (parse(a), parse(b)) {
        (Some(pa), Some(pb)) => pa.cmp(&pb).then_with(|| a.cmp(b)),
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_lines_keeps_last_n_lines_only() {
        let text = "a\nb\nc\nd\ne";
        assert_eq!(tail_lines(text, 2), "d\ne");
        assert_eq!(tail_lines(text, 10), text);
        assert_eq!(tail_lines("", 5), "");
    }

    #[test]
    fn compare_versions_orders_numerically_not_lexically() {
        assert_eq!(
            compare_versions("2.0.0", "10.0.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("1.2.0", "1.10.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("1.0.0", "1.0.0"),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn newest_cache_version_picks_highest_semver_dir() {
        let dir = tempfile::tempdir().unwrap();
        for v in ["1.0.0", "2.0.0", "1.5.0"] {
            std::fs::create_dir_all(dir.path().join(v)).unwrap();
        }
        assert_eq!(newest_cache_version(dir.path()), "2.0.0");
    }

    #[test]
    fn newest_cache_version_unknown_when_missing() {
        assert_eq!(
            newest_cache_version(Path::new("/definitely/not/a/real/path")),
            "unknown"
        );
    }

    #[test]
    fn cargo_install_args_stage_into_supplied_root() {
        assert_eq!(
            cargo_install_args(Path::new("/tmp/dvandva-stage")),
            vec!["install", "dvandva", "--root", "/tmp/dvandva-stage"]
        );
    }

    #[test]
    fn plugin_commit_requires_both_engines_and_claude_update() {
        assert!(plugins_committed(true, true, true));
        assert!(!plugins_committed(false, false, true));
        assert!(!plugins_committed(true, false, true));
        assert!(!plugins_committed(true, true, false));
    }
}
