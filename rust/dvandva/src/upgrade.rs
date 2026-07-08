//! `dvandva upgrade` logic — brings the whole stack current in one command:
//! `cargo install dvandva`, the dual-engine plugin install (the same code
//! path `dvandva install` uses, via [`installers::run_install`]), and a
//! `claude plugin update dvandva@dvandva` cache bump, finishing with a
//! concise version-table report.
//!
//! Step sequencing mirrors the feature request literally: cargo, then both
//! engines' plugin install, then the Claude-only cache-bump update. The two
//! plugin engines are driven through two single-target
//! [`installers::run_install`] calls (rather than one dual-target call) so a
//! Claude-side failure never prevents the Codex side from running — matching
//! the exit rule below, which needs each engine's outcome independently.
//!
//! Exit rule: non-zero when the cargo step fails, or when *both* plugin
//! engines fail; a single engine's plugin failure is a warning, not a hard
//! failure, as long as the other engine succeeded.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::installers::{self, InstallTargets};

/// How many trailing lines of a subprocess's combined stdout+stderr to print
/// (`cargo install` in particular can produce a long compile log; only the
/// tail is relevant to an upgrade report).
const TAIL_LINES: usize = 20;

/// Runs the `upgrade` flow, printing progress and a final version-table
/// report, and returning the effective process exit code.
pub fn run_upgrade(marketplace: &str) -> i32 {
    let cargo_ok = run_cargo_install();

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

    let claude_ok = if claude_install_ok {
        run_claude_plugin_update(marketplace)
    } else {
        false
    };

    if !claude_ok && codex_ok {
        eprintln!("WARNING: Claude Code plugin upgrade failed; Codex plugin upgrade succeeded.");
    } else if claude_ok && !codex_ok {
        eprintln!("WARNING: Codex plugin upgrade failed; Claude Code plugin upgrade succeeded.");
    }

    print_version_table(&home_dir(), &installers::codex_home_dir());

    let plugins_ok = claude_ok || codex_ok;
    if cargo_ok && plugins_ok {
        0
    } else {
        1
    }
}

// ---------------------------------------------------------------------
// Step 1: `cargo install dvandva`
// ---------------------------------------------------------------------

fn run_cargo_install() -> bool {
    if !installers::command_exists("cargo") {
        eprintln!("ERROR: cargo CLI not found on PATH");
        return false;
    }

    println!("Binary: running `cargo install dvandva`...");
    let (combined, code) = installers::capture_combined("cargo", &["install", "dvandva"]);
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
    if installers::already_present_pattern().is_match(&combined) {
        println!("cargo install dvandva: already up to date; continuing.");
        return true;
    }
    false
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

fn print_version_table(home: &Path, codex_home: &Path) {
    let binary_version = installed_binary_version(home);
    let claude_cache_version =
        newest_cache_version(&home.join(".claude/plugins/cache/dvandva/dvandva"));
    let codex_cache_version =
        newest_cache_version(&codex_home.join("plugins/cache/dvandva/dvandva"));

    println!();
    println!("Upgrade summary:");
    println!("  binary (~/.cargo/bin/dvandva): {binary_version}");
    println!("  Claude plugin cache:           {claude_cache_version}");
    println!("  Codex plugin cache:            {codex_cache_version}");
}

/// Runs `~/.cargo/bin/dvandva --version` as a subprocess — the *installed*
/// binary, not this (potentially stale) running process — and returns its
/// trimmed stdout, or `"unknown"` when the binary is missing or fails.
fn installed_binary_version(home: &Path) -> String {
    let bin = home.join(".cargo/bin/dvandva");
    match Command::new(&bin).arg("--version").output() {
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
}
