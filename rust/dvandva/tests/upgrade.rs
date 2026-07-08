//! Integration tests for `dvandva upgrade`, the one-command stack refresh
//! that folds `cargo install dvandva`, `dvandva install`'s dual-engine
//! plugin install, and a `claude plugin update dvandva@dvandva` cache bump
//! into a single verb.
//!
//! Fake `cargo`/`claude`/`codex` executables are written as
//! `#!/usr/bin/env bash` scripts into a per-test tempdir and prepended onto
//! `PATH`, mirroring `tests/install.rs`'s `FAKE_BIN` fixture pattern. A fake
//! `~/.cargo/bin/dvandva --version` binary and pre-seeded plugin cache
//! directories exercise the final version-table report.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Fake `cargo` stub: `cargo install dvandva`. `CARGO_FAKE_ALREADY=1` makes it
/// exit non-zero with cargo's real "already installed" wording (still a
/// success case for `upgrade`).
const FAKE_CARGO: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'cargo %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  "install dvandva")
    if [[ "${CARGO_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Ignored package \`dvandva v3.1.0\` is already installed, use --force to override" >&2
      exit 101
    fi
    echo "Installed package \`dvandva v3.1.0\` (executable \`dvandva\`)"
    ;;
  *)
    echo "unexpected fake cargo invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// Fake `claude` stub covering the three calls `upgrade` makes: marketplace
/// add, plugin install (both via the reused `dvandva install` code path),
/// and `plugin update dvandva@dvandva` (the extra cache-bump step).
///
/// Toggles:
/// - `CLAUDE_FAKE_ALREADY=1`: marketplace add / plugin install report
///   "already present" (still success, exercises `run_idempotent`).
/// - `CLAUDE_FAKE_UPDATE_ALREADY=1`: `plugin update` reports "already at the
///   latest version" (real `claude`'s own wording; exit 0).
/// - `CLAUDE_FAKE_UPDATE_NOT_INSTALLED=1`: `plugin update` fails with real
///   `claude`'s "not found" wording (exit 1), exercising the fallback path.
const FAKE_CLAUDE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'claude %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  plugin\ marketplace\ add\ *)
    if [[ "${CLAUDE_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already registered" >&2
      exit 1
    fi
    ;;
  "plugin install dvandva@dvandva")
    if [[ "${CLAUDE_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  "plugin update dvandva@dvandva")
    if [[ "${CLAUDE_FAKE_UPDATE_NOT_INSTALLED:-0}" == "1" ]]; then
      echo "Checking for updates for plugin \"dvandva@dvandva\" at user scope..."
      echo "Failed to update plugin \"dvandva@dvandva\": Plugin \"dvandva\" not found" >&2
      exit 1
    fi
    if [[ "${CLAUDE_FAKE_UPDATE_ALREADY:-0}" == "1" ]]; then
      echo "dvandva is already at the latest version (1.5.1)."
      exit 0
    fi
    echo "Updated dvandva to the latest version."
    ;;
  *)
    echo "unexpected fake claude invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// `tests/install.rs`'s modern-path `codex` stub, verbatim.
const FAKE_CODEX_MODERN: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'codex %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already added" >&2
      exit 1
    fi
    ;;
  "plugin add dvandva@dvandva")
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  app-server\ *)
    echo "app-server fallback should not run when codex plugin add exists" >&2
    exit 42
    ;;
  *)
    echo "unexpected fake codex invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// Fake `codex` stub that always fails, used to exercise "both engines
/// failed" -> non-zero exit.
const FAKE_CODEX_ALWAYS_FAILS: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'codex %s\n' "$*" >> "$UPGRADE_TEST_LOG"
echo "codex is broken" >&2
exit 1
"#;

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// Local-dir marketplace fixture: `<dir>/marketplace/.agents/plugins/marketplace.json`.
fn write_marketplace_fixture(dir: &Path) -> std::path::PathBuf {
    let marketplace = dir.join("marketplace");
    fs::create_dir_all(marketplace.join(".agents/plugins")).unwrap();
    fs::write(
        marketplace.join(".agents/plugins/marketplace.json"),
        r#"{"name":"dvandva","plugins":[{"name":"dvandva"}]}"#,
    )
    .unwrap();
    marketplace
}

/// Writes a fake `<home>/.cargo/bin/dvandva --version` binary that prints
/// `dvandva <version>` to stdout, standing in for the freshly-`cargo
/// install`ed binary the running (old) test process can't shell out to
/// itself.
fn write_fake_installed_binary(home: &Path, version: &str) {
    let bin_dir = home.join(".cargo/bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_executable(
        &bin_dir.join("dvandva"),
        &format!("#!/usr/bin/env bash\nset -euo pipefail\necho \"dvandva {version}\"\n"),
    );
}

/// Seeds `<base>/<version>` under a plugin cache root so the version-table
/// lookup has something to find.
fn seed_cache_version(cache_base: &Path, version: &str) {
    fs::create_dir_all(cache_base.join(version)).unwrap();
}

fn prepend_path(dir: &Path) -> OsString {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![dir.to_path_buf()];
    paths.extend(std::env::split_paths(&existing));
    std::env::join_paths(paths).expect("join PATH")
}

struct UpgradeRun {
    fake_bin: std::path::PathBuf,
    _tmp: tempfile::TempDir,
    envs: Vec<(String, OsString)>,
}

impl UpgradeRun {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_bin = tmp.path().join("bin");
        fs::create_dir_all(&fake_bin).unwrap();
        UpgradeRun {
            fake_bin,
            _tmp: tmp,
            envs: Vec::new(),
        }
    }

    fn tmp_path(&self) -> &Path {
        self._tmp.path()
    }

    fn env(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.envs.push((key.to_string(), value.into()));
        self
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
        cmd.arg("upgrade").args(args);
        cmd.env("PATH", prepend_path(&self.fake_bin));
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
        cmd.output().expect("failed to run dvandva upgrade")
    }
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn contains(text: &str, needle: &str) -> bool {
    text.contains(needle)
}

/// Common fixture wiring shared by most cases: writes the marketplace fixture
/// plus `cargo`/`claude`/`codex` stubs, and points `HOME`/`CODEX_HOME` at
/// fresh per-test directories. Returns `(run, tmp, marketplace, home,
/// codex_home)`.
fn base_fixture(
    codex_stub: &str,
) -> (
    UpgradeRun,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let run = UpgradeRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("cargo"), FAKE_CARGO);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), codex_stub);

    let home = tmp.join("home");
    let codex_home = tmp.join("codex-home");
    let log = tmp.join("upgrade.log");
    let run = run
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .env("UPGRADE_TEST_LOG", &log);
    (run, tmp, marketplace, home, codex_home)
}

// ---------------------------------------------------------------------
// (a) happy path: cargo -> plugins (claude+codex) -> claude update, then the
// version table.
// ---------------------------------------------------------------------
#[test]
fn happy_path_runs_cargo_then_plugins_then_claude_update_and_reports_table() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.1.0");
    seed_cache_version(&home.join(".claude/plugins/cache/dvandva/dvandva"), "3.1.0");
    seed_cache_version(&codex_home.join("plugins/cache/dvandva/dvandva"), "3.1.0");

    let output = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    // Ordering: cargo, then claude+codex plugin install, then claude update.
    let cargo_pos = log
        .find("cargo install dvandva")
        .expect("cargo call logged");
    let claude_install_pos = log
        .find("claude plugin install dvandva@dvandva")
        .expect("claude install call logged");
    let codex_pos = log
        .find("codex plugin add dvandva@dvandva")
        .expect("codex call logged");
    let claude_update_pos = log
        .find("claude plugin update dvandva@dvandva")
        .expect("claude update call logged");
    assert!(cargo_pos < claude_install_pos);
    assert!(cargo_pos < codex_pos);
    assert!(claude_install_pos < claude_update_pos);
    assert!(codex_pos < claude_update_pos);

    let text = combined(&output);
    assert!(contains(&text, "dvandva 3.1.0"), "text: {text}");
}

// ---------------------------------------------------------------------
// (b) cargo "already installed" no-op counts as success
// ---------------------------------------------------------------------
#[test]
fn cargo_already_installed_counts_as_success() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.1.0");

    let run = run.env("CARGO_FAKE_ALREADY", "1");
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
}

// ---------------------------------------------------------------------
// (c) claude "already at the latest version" counts as success
// ---------------------------------------------------------------------
#[test]
fn claude_update_already_latest_counts_as_success() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.1.0");

    let run = run.env("CLAUDE_FAKE_UPDATE_ALREADY", "1");
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
}

// ---------------------------------------------------------------------
// (d) claude update failing with "not installed" falls back to the normal
// install path without failing the overall run (codex side succeeded).
// ---------------------------------------------------------------------
#[test]
fn claude_update_not_installed_falls_back_without_failing_run() {
    let (run, tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.1.0");

    let run = run.env("CLAUDE_FAKE_UPDATE_NOT_INSTALLED", "1");
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        contains(&text, "falling back to the normal install path"),
        "text: {text}"
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    // The fallback re-runs the normal claude install path: a second
    // `claude plugin install dvandva@dvandva` call beyond the one from the
    // initial dual-engine install.
    let install_calls = log
        .lines()
        .filter(|line| *line == "claude plugin install dvandva@dvandva")
        .count();
    assert_eq!(
        install_calls, 2,
        "expected the fallback to re-run claude plugin install; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// (e) missing cache dirs degrade to "unknown" in the version table
// ---------------------------------------------------------------------
#[test]
fn missing_cache_dirs_degrade_to_unknown_in_table() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    // No fake installed binary, no seeded cache dirs: every row must degrade.
    let _ = &home;

    let output = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.matches("unknown").count() >= 3,
        "expected all three table rows to degrade to unknown; text: {text}"
    );
}

// ---------------------------------------------------------------------
// Both plugin engines failing is a hard failure (non-zero exit), even
// when cargo itself succeeded.
// ---------------------------------------------------------------------
#[test]
fn both_plugin_engines_failing_exits_nonzero() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_ALWAYS_FAILS);
    write_fake_installed_binary(&home, "3.1.0");

    // Force the claude side to fail too: the plugin marketplace add call
    // reports "already registered" as a *failure* exit that isn't tolerated
    // (simulate a hard failure via an unrecognized claude invocation by
    // pointing CLAUDE_FAKE_UPDATE_NOT_INSTALLED at a marketplace that can't
    // be installed). Simplest reliable failure: drop claude off PATH by
    // shadowing it with a script that always errors on every call.
    const FAKE_CLAUDE_ALWAYS_FAILS: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'claude %s\n' "$*" >> "$UPGRADE_TEST_LOG"
echo "claude is broken" >&2
exit 1
"#;
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE_ALWAYS_FAILS);

    let output = run.run(&[marketplace.to_str().unwrap()]);
    assert_ne!(output.status.code(), Some(0), "text: {}", combined(&output));
}

// ---------------------------------------------------------------------
// (f) the top-level usage surface lists the new verb
// ---------------------------------------------------------------------
#[test]
fn main_usage_lists_upgrade_verb() {
    let output = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("--help")
        .output()
        .expect("failed to run dvandva --help");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("upgrade"), "stdout: {stdout}");
}

#[test]
fn upgrade_help_flag_prints_usage_and_exits_zero() {
    let run = UpgradeRun::new();
    let output = run.run(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: dvandva upgrade"));
}
