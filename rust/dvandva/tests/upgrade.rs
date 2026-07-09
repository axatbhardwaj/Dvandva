//! Integration tests for `dvandva upgrade`, the one-command stack refresh
//! that folds a staged `cargo install dvandva --root <tmp>`, the dual-engine
//! plugin install (`dvandva install`'s code path), and a `claude plugin
//! update dvandva@dvandva` cache bump into a single, all-or-nothing
//! transaction (see `src/upgrade_txn.rs`): any hard failure at any step
//! rolls back every reachable snapshot and the process exits with the
//! transaction taxonomy — `0` committed, `20` rolled back cleanly, `21`
//! rollback incomplete. A single plugin engine failing is a hard failure
//! now; the old warn-and-continue-if-one-engine-succeeded tolerance is gone.
//!
//! Fake `cargo`/`claude`/`codex` executables are written as
//! `#!/usr/bin/env bash` scripts into a per-test tempdir and prepended onto
//! `PATH`, mirroring `tests/install.rs`'s `FAKE_BIN` fixture pattern. The
//! fake `cargo` stub stages a real `bin/dvandva` script under whatever
//! `--root` it's given (the transactional flow checks the staged binary
//! actually exists on disk before proceeding), standing in for what real
//! `cargo install --root` produces.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use dvandva::upgrade_txn::{
    TransactionConfig, EXIT_COMMITTED, EXIT_ROLLBACK_INCOMPLETE, EXIT_ROLLED_BACK,
};

/// Fake `cargo` stub: `cargo install dvandva --root <stage>`. Stages a real
/// `bin/dvandva` script under `<stage>` (so the staged-binary existence
/// check the transactional flow performs passes), mirroring what real
/// `cargo install --root` produces. `CARGO_FAKE_ALREADY=1` makes it exit
/// non-zero with cargo's real "already installed" wording *after* staging
/// the binary (still a success case for `upgrade`, matching
/// `already_present_pattern`'s fallback). `CARGO_FAKE_FAIL=1` fails without
/// staging anything, for the hard-failure case.
const FAKE_CARGO: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'cargo %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  install\ dvandva\ --root\ *)
    if [[ "${CARGO_FAKE_FAIL:-0}" == "1" ]]; then
      echo "error: simulated cargo network failure" >&2
      exit 101
    fi
    root="$4"
    mkdir -p "$root/bin"
    cat > "$root/bin/dvandva" <<'BIN'
#!/usr/bin/env bash
set -euo pipefail
echo "dvandva 3.1.0"
BIN
    chmod +x "$root/bin/dvandva"
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

/// Fake `codex` stub that always fails, used to exercise a codex-only plugin
/// failure.
const FAKE_CODEX_ALWAYS_FAILS: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'codex %s\n' "$*" >> "$UPGRADE_TEST_LOG"
echo "codex is broken" >&2
exit 1
"#;

/// Fake `claude` stub that always fails, used (alongside
/// `FAKE_CODEX_ALWAYS_FAILS`) to exercise both plugin engines failing at
/// once.
const FAKE_CLAUDE_ALWAYS_FAILS: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'claude %s\n' "$*" >> "$UPGRADE_TEST_LOG"
echo "claude is broken" >&2
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
/// `dvandva <version>` to stdout, standing in for a pre-existing installed
/// binary (used as the pre-upgrade snapshot content in rollback tests).
fn write_fake_installed_binary(home: &Path, version: &str) {
    write_fake_installed_binary_at(&home.join(".cargo/bin/dvandva"), version);
}

fn write_fake_installed_binary_at(path: &Path, version: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_executable(
        path,
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

/// Reconstructs the same `TransactionConfig` that `upgrade::run_upgrade`
/// builds internally (`state_dir = <home>/.dvandva`), purely to read its
/// `lock_path()` / `breadcrumb_path()` / `live_binary_path()` derivations
/// instead of hand-duplicating path literals that could silently drift from
/// the source of truth. The marketplace value is irrelevant to these three
/// accessors.
fn txn_config(home: &Path, codex_home: &Path) -> TransactionConfig {
    TransactionConfig::new("unused", home, codex_home, home.join(".dvandva"))
}

/// JSON-string-encodes a path the way `serde_json` would for these tests'
/// plain-ASCII tempdir paths (Rust's `Debug` escaping for `&str` matches
/// JSON's basic escaping closely enough here).
fn json_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// Hand-writes a crash breadcrumb referencing exactly one snapshot record,
/// as `upgrade_txn::Breadcrumb` serializes it — standing in for a previous
/// `dvandva upgrade` process that died mid-transaction after taking its W0
/// snapshot but before committing or cleaning up.
fn write_breadcrumb(breadcrumb_path: &Path, snapshot_root: &Path, target: &Path, backup: &Path) {
    fs::create_dir_all(breadcrumb_path.parent().unwrap()).unwrap();
    let json = format!(
        r#"{{"pid":1,"timestamp":1,"snapshot_root":{},"targets":[{{"target":{},"backup":{},"existed":true,"was_dir":false}}]}}"#,
        json_path(snapshot_root),
        json_path(target),
        json_path(backup),
    );
    fs::write(breadcrumb_path, json).unwrap();
}

/// Hand-writes an `upgrade.lock` file in the shape `UpgradeLock` writes,
/// standing in for a lock held by a concurrent (or crashed) `dvandva
/// upgrade` process.
fn write_lock(lock_path: &Path, pid: u32, timestamp: u64) {
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    fs::write(
        lock_path,
        format!("pid={pid}\ntimestamp={timestamp}\ntoken={pid}:{timestamp}\n"),
    )
    .unwrap();
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
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
        cmd.env_remove("CARGO_INSTALL_ROOT");
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
        .env("CARGO_HOME", home.join(".cargo"))
        .env("CODEX_HOME", &codex_home)
        .env("UPGRADE_TEST_LOG", &log);
    (run, tmp, marketplace, home, codex_home)
}

// ---------------------------------------------------------------------
// (a) happy path: cargo -> plugins (claude+codex) -> claude update, then the
// version table. Exit taxonomy: EXIT_COMMITTED (0).
// ---------------------------------------------------------------------
#[test]
fn happy_path_runs_cargo_then_plugins_then_claude_update_and_reports_table() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.0.0");
    seed_cache_version(&home.join(".claude/plugins/cache/dvandva/dvandva"), "3.1.0");
    seed_cache_version(&codex_home.join("plugins/cache/dvandva/dvandva"), "3.1.0");

    let output = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(EXIT_COMMITTED),
        "stderr: {}",
        combined(&output)
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    // Ordering: cargo, then claude+codex plugin install, then claude update.
    let cargo_pos = log
        .find("cargo install dvandva --root")
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
    // The live binary is swapped to whatever the staged cargo install
    // produced, not the pre-upgrade fake binary's version.
    assert!(contains(&text, "dvandva 3.1.0"), "text: {text}");
}

#[test]
fn cargo_home_selects_live_binary_target_and_version_table() {
    let (run, tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    let cargo_home = tmp.join("custom-cargo");
    let custom_binary = cargo_home.join("bin/dvandva");
    write_fake_installed_binary_at(&custom_binary, "3.0.0");

    let run = run.env("CARGO_HOME", &cargo_home);
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_COMMITTED),
        "text: {}",
        combined(&output)
    );
    assert!(
        fs::read_to_string(&custom_binary)
            .unwrap_or_else(|err| {
                panic!(
                    "could not read custom CARGO_HOME binary {}: {err}; output: {}",
                    custom_binary.display(),
                    combined(&output)
                )
            })
            .contains("dvandva 3.1.0"),
        "custom CARGO_HOME binary should receive the staged upgrade"
    );
    assert!(
        !home.join(".cargo/bin/dvandva").exists(),
        "upgrade must not create a default HOME cargo binary when CARGO_HOME is set"
    );
    let text = combined(&output);
    assert!(
        text.contains(&format!(
            "binary ({}): dvandva 3.1.0",
            custom_binary.display()
        )),
        "version table should report the selected live binary path; text: {text}"
    );
}

#[test]
fn cargo_install_root_takes_precedence_over_cargo_home() {
    let (run, tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    let cargo_home = tmp.join("custom-cargo");
    let cargo_home_binary = cargo_home.join("bin/dvandva");
    write_fake_installed_binary_at(&cargo_home_binary, "wrong-target");
    let install_root = tmp.join("install-root");
    let install_root_binary = install_root.join("bin/dvandva");
    write_fake_installed_binary_at(&install_root_binary, "3.0.0");

    let run = run
        .env("CARGO_HOME", &cargo_home)
        .env("CARGO_INSTALL_ROOT", &install_root);
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_COMMITTED),
        "text: {}",
        combined(&output)
    );
    assert!(
        fs::read_to_string(&install_root_binary)
            .unwrap_or_else(|err| {
                panic!(
                    "could not read CARGO_INSTALL_ROOT binary {}: {err}; output: {}",
                    install_root_binary.display(),
                    combined(&output)
                )
            })
            .contains("dvandva 3.1.0"),
        "CARGO_INSTALL_ROOT binary should receive the staged upgrade"
    );
    assert!(
        fs::read_to_string(&cargo_home_binary)
            .unwrap()
            .contains("dvandva wrong-target"),
        "CARGO_HOME binary should be left untouched when CARGO_INSTALL_ROOT is set"
    );
    assert!(
        !home.join(".cargo/bin/dvandva").exists(),
        "upgrade must not create a default HOME cargo binary when cargo env overrides are set"
    );
    let text = combined(&output);
    assert!(
        text.contains(&format!(
            "binary ({}): dvandva 3.1.0",
            install_root_binary.display()
        )),
        "version table should report the install-root live binary path; text: {text}"
    );
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
        Some(EXIT_COMMITTED),
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
        Some(EXIT_COMMITTED),
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
        Some(EXIT_COMMITTED),
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
// (e) missing plugin cache dirs degrade to "unknown" in the version table.
// The binary row can no longer degrade: a committed upgrade always ends
// with a real staged binary swapped into the selected live binary path.
// ---------------------------------------------------------------------
#[test]
fn missing_plugin_cache_dirs_degrade_to_unknown_in_table() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_MODERN);
    // No pre-seeded plugin cache dirs: both cache rows must degrade.
    let _ = &home;

    let output = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(EXIT_COMMITTED),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.matches("unknown").count() >= 2,
        "expected both plugin-cache rows to degrade to unknown; text: {text}"
    );
    let binary_line = text
        .lines()
        .find(|line| line.trim_start().starts_with("binary ("))
        .expect("version table should include a binary row");
    assert!(
        !binary_line.contains("unknown"),
        "a committed upgrade must report a real binary version; text: {text}"
    );
}

// ---------------------------------------------------------------------
// Both plugin engines failing rolls back and exits EXIT_ROLLED_BACK (20).
// ---------------------------------------------------------------------
#[test]
fn both_plugin_engines_failing_rolls_back_and_exits_20() {
    let (run, _tmp, marketplace, home, _codex_home) = base_fixture(FAKE_CODEX_ALWAYS_FAILS);
    write_fake_installed_binary(&home, "3.0.0");
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE_ALWAYS_FAILS);

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLED_BACK),
        "text: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        contains(&text, "rollback restored all reachable snapshots"),
        "text: {text}"
    );
}

// ---------------------------------------------------------------------
// A single plugin engine failing (codex only; claude fully succeeds,
// including its update step) is now a hard failure too — the old
// warn-and-exit-0-if-one-engine-succeeded tolerance is dead.
// ---------------------------------------------------------------------
#[test]
fn single_engine_codex_failure_rolls_back_even_though_claude_fully_succeeds() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_ALWAYS_FAILS);
    write_fake_installed_binary(&home, "3.0.0");
    let config = txn_config(&home, &codex_home);

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLED_BACK),
        "text: {}",
        combined(&output)
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    assert!(
        log.contains("claude plugin install dvandva@dvandva"),
        "claude's install step must have run; log:\n{log}"
    );
    assert!(
        log.contains("claude plugin update dvandva@dvandva"),
        "claude's update step must have run (and succeeded) despite the overall rollback; log:\n{log}"
    );
    assert!(
        log.lines().any(|line| line.starts_with("codex ")),
        "codex must have been attempted; log:\n{log}"
    );

    // The live binary is swapped only after plugins commit, so a
    // plugin-stage failure must leave it untouched.
    assert_eq!(
        fs::read_to_string(config.live_binary_path()).unwrap(),
        "#!/usr/bin/env bash\nset -euo pipefail\necho \"dvandva 3.0.0\"\n",
        "live binary must be untouched when the plugin step fails"
    );
}

// ---------------------------------------------------------------------
// A cargo staging failure rolls back before either plugin engine runs, and
// exits EXIT_ROLLED_BACK (20).
// ---------------------------------------------------------------------
#[test]
fn cargo_install_failure_rolls_back_before_touching_plugins() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.0.0");
    let config = txn_config(&home, &codex_home);

    let run = run.env("CARGO_FAKE_FAIL", "1");
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLED_BACK),
        "text: {}",
        combined(&output)
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    assert!(
        !log.lines()
            .any(|line| line.starts_with("claude ") || line.starts_with("codex ")),
        "neither plugin engine should run after a staging failure; log:\n{log}"
    );
    assert_eq!(
        fs::read_to_string(config.live_binary_path()).unwrap(),
        "#!/usr/bin/env bash\nset -euo pipefail\necho \"dvandva 3.0.0\"\n",
        "live binary must be untouched when staging fails"
    );
}

// ---------------------------------------------------------------------
// Lock contention: a concurrently-held (non-stale) lock refuses the
// upgrade outright without running any step, and exits EXIT_ROLLED_BACK
// (20) — the landed lock has no wait/retry loop, only a stale-timeout
// takeover.
// ---------------------------------------------------------------------
#[test]
fn concurrent_upgrade_lock_is_refused_without_running_any_step() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    write_fake_installed_binary(&home, "3.0.0");
    let config = txn_config(&home, &codex_home);
    write_lock(&config.lock_path(), std::process::id(), now_secs());

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLED_BACK),
        "text: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(contains(&text, "upgrade lock unavailable"), "text: {text}");

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    assert!(
        log.is_empty(),
        "no subprocess should run while a live lock is held; log:\n{log}"
    );
    // A foreign lock is left alone (only its own holder cleans it up).
    assert!(config.lock_path().exists());
}

// ---------------------------------------------------------------------
// Lock contention, contrast case: a *stale* lock (older than the 30-minute
// default timeout) is reclaimed and the upgrade proceeds to commit — this
// is what distinguishes "refused" above as genuine contention rather than
// mere file presence.
// ---------------------------------------------------------------------
#[test]
fn stale_lock_is_reclaimed_and_upgrade_proceeds() {
    let (run, _tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    let config = txn_config(&home, &codex_home);
    write_lock(&config.lock_path(), 999_999, 0); // unix epoch: far past any stale timeout

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_COMMITTED),
        "text: {}",
        combined(&output)
    );
    assert!(
        !config.lock_path().exists(),
        "the reclaimed-then-released lock should be cleaned up on commit"
    );
}

// ---------------------------------------------------------------------
// Breadcrumb detection: a valid crash breadcrumb left by a previous
// (simulated) crashed attempt is detected on the next invocation, recovered
// (restoring the referenced snapshot) before any fresh step runs, and exits
// EXIT_ROLLED_BACK (20).
// ---------------------------------------------------------------------
#[test]
fn existing_valid_breadcrumb_triggers_recovery_and_exits_rolled_back() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    let config = txn_config(&home, &codex_home);
    let live_binary = config.live_binary_path();
    fs::create_dir_all(live_binary.parent().unwrap()).unwrap();
    fs::write(&live_binary, "crashed-partial-binary").unwrap();

    let snapshot_root = home.join(".dvandva/upgrade-snapshots/fake-crash");
    fs::create_dir_all(&snapshot_root).unwrap();
    let backup = snapshot_root.join("0");
    fs::write(&backup, "old-committed-binary").unwrap();
    write_breadcrumb(
        &config.breadcrumb_path(),
        &snapshot_root,
        &live_binary,
        &backup,
    );

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLED_BACK),
        "text: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        contains(&text, "previous upgrade attempt did not commit"),
        "text: {text}"
    );
    assert_eq!(
        fs::read_to_string(&live_binary).unwrap(),
        "old-committed-binary",
        "recovery must restore the snapshotted content"
    );
    assert!(
        !config.breadcrumb_path().exists(),
        "a clean recovery removes the crash breadcrumb"
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    assert!(
        log.is_empty(),
        "breadcrumb recovery must short-circuit before any fresh subprocess runs; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// Breadcrumb detection, unrecoverable case: a corrupt breadcrumb (as if the
// crash happened mid-write) cannot be parsed, so the run refuses to guess
// and exits EXIT_ROLLBACK_INCOMPLETE (21), leaving the breadcrumb in place
// for manual inspection.
// ---------------------------------------------------------------------
#[test]
fn corrupt_breadcrumb_exits_rollback_incomplete_and_preserves_it_for_inspection() {
    let (run, tmp, marketplace, home, codex_home) = base_fixture(FAKE_CODEX_MODERN);
    let config = txn_config(&home, &codex_home);
    fs::create_dir_all(config.breadcrumb_path().parent().unwrap()).unwrap();
    fs::write(config.breadcrumb_path(), "not json").unwrap();

    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_ROLLBACK_INCOMPLETE),
        "text: {}",
        combined(&output)
    );
    assert!(
        config.breadcrumb_path().exists(),
        "an unparseable breadcrumb must remain for residual inspection"
    );

    let log = fs::read_to_string(tmp.join("upgrade.log")).unwrap_or_default();
    assert!(
        log.is_empty(),
        "no fresh subprocess should run when the breadcrumb can't be read; log:\n{log}"
    );
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
    assert!(stdout.contains("Exit codes:"), "stdout: {stdout}");
    assert!(stdout.contains("0  committed"), "stdout: {stdout}");
    assert!(stdout.contains("20 failed"), "stdout: {stdout}");
    assert!(
        stdout.contains("21 rollback incomplete"),
        "stdout: {stdout}"
    );
}
