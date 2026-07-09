//! Transactional backbone for `dvandva upgrade`.
//!
//! This module intentionally owns only the side-effect ordering and rollback
//! contract. The CLI orchestration can plug in subprocess-backed upgrade steps
//! without duplicating the invariants.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Upgrade committed and final verification passed.
pub const EXIT_COMMITTED: i32 = 0;
/// Upgrade failed, and all reachable snapshots were restored cleanly.
pub const EXIT_ROLLED_BACK: i32 = 20;
/// Upgrade failed, and rollback was incomplete or precise recovery was not reachable.
pub const EXIT_ROLLBACK_INCOMPLETE: i32 = 21;

/// Default stale timeout for `~/.dvandva/upgrade.lock`.
pub const DEFAULT_STALE_LOCK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const LOCK_FILE: &str = "upgrade.lock";
const BREADCRUMB_FILE: &str = "upgrade.breadcrumb.json";

/// Filesystem and marketplace inputs for one transactional upgrade attempt.
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    marketplace: String,
    home: PathBuf,
    codex_home: PathBuf,
    state_dir: PathBuf,
    stale_lock_timeout: Duration,
}

impl TransactionConfig {
    pub fn new(
        marketplace: impl Into<String>,
        home: impl AsRef<Path>,
        codex_home: impl AsRef<Path>,
        state_dir: impl AsRef<Path>,
    ) -> Self {
        Self {
            marketplace: marketplace.into(),
            home: home.as_ref().to_path_buf(),
            codex_home: codex_home.as_ref().to_path_buf(),
            state_dir: state_dir.as_ref().to_path_buf(),
            stale_lock_timeout: DEFAULT_STALE_LOCK_TIMEOUT,
        }
    }

    pub fn with_stale_lock_timeout(mut self, timeout: Duration) -> Self {
        self.stale_lock_timeout = timeout;
        self
    }

    pub fn marketplace(&self) -> &str {
        &self.marketplace
    }

    pub fn live_binary_path(&self) -> PathBuf {
        self.home.join(".cargo/bin/dvandva")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.state_dir.join(LOCK_FILE)
    }

    pub fn breadcrumb_path(&self) -> PathBuf {
        self.state_dir.join(BREADCRUMB_FILE)
    }

    fn snapshot_paths(&self) -> Vec<PathBuf> {
        let engine_targets = crate::upgrade_txn_engines::engine_state_targets(
            &self.home,
            &self.codex_home,
            &self.marketplace,
        );
        vec![
            self.live_binary_path(),
            engine_targets.claude_installed_plugins,
            engine_targets.claude_cache_base,
            engine_targets.codex_marketplace_tmp,
            engine_targets.codex_config,
            engine_targets.codex_cache_base,
        ]
    }

    fn snapshot_root(&self, tx_id: &str) -> PathBuf {
        self.state_dir.join("upgrade-snapshots").join(tx_id)
    }

    fn stage_root(&self, tx_id: &str) -> PathBuf {
        self.state_dir.join("upgrade-staging").join(tx_id)
    }
}

/// The concrete upgrade steps supplied by CLI orchestration.
pub trait UpgradeExecutor {
    fn stage_binary(&mut self, stage_root: &Path) -> Result<PathBuf, UpgradeStepError>;
    fn verify_binary(&mut self, binary: &Path) -> Result<(), UpgradeStepError>;
    fn upgrade_plugins(&mut self, marketplace: &str) -> Result<(), UpgradeStepError>;
    fn verify_committed(&mut self, live_binary: &Path) -> Result<(), UpgradeStepError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeStepError {
    pub stage: &'static str,
    pub message: String,
}

impl UpgradeStepError {
    pub fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

/// Run one all-or-nothing upgrade attempt.
pub fn run_transactional_upgrade(
    config: &TransactionConfig,
    executor: &mut impl UpgradeExecutor,
) -> i32 {
    match run_transactional_upgrade_inner(config, executor) {
        Ok(()) => EXIT_COMMITTED,
        Err(TxnFailure::RolledBack) => EXIT_ROLLED_BACK,
        Err(TxnFailure::RollbackIncomplete) => EXIT_ROLLBACK_INCOMPLETE,
    }
}

fn run_transactional_upgrade_inner(
    config: &TransactionConfig,
    executor: &mut impl UpgradeExecutor,
) -> Result<(), TxnFailure> {
    fs::create_dir_all(&config.state_dir).map_err(|err| {
        eprintln!(
            "ERROR: could not create upgrade state dir {}: {err}",
            config.state_dir.display()
        );
        TxnFailure::RolledBack
    })?;

    let _lock = match UpgradeLock::acquire(&config.lock_path(), config.stale_lock_timeout) {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!("ERROR: upgrade lock unavailable: {err}");
            if config.breadcrumb_path().exists() {
                eprintln!(
                    "ERROR: upgrade breadcrumb exists at {}; recovery cannot safely run until the lock is released or reclaimed.",
                    config.breadcrumb_path().display()
                );
                return Err(TxnFailure::RollbackIncomplete);
            }
            return Err(TxnFailure::RolledBack);
        }
    };

    if config.breadcrumb_path().exists() {
        return recover_previous_attempt(config);
    }

    let tx_id = transaction_id();
    let snapshot = match Snapshot::create(&config.snapshot_root(&tx_id), &config.snapshot_paths()) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            eprintln!("ERROR: could not create upgrade snapshot: {err}");
            return Err(TxnFailure::RolledBack);
        }
    };
    let stage_root = config.stage_root(&tx_id);
    let breadcrumb = Breadcrumb::new(snapshot.records.clone(), snapshot.root.clone());
    if let Err(err) = write_breadcrumb(&config.breadcrumb_path(), &breadcrumb) {
        eprintln!("ERROR: could not write upgrade breadcrumb: {err}");
        return Err(TxnFailure::RolledBack);
    }

    let staged_binary = match executor.stage_binary(&stage_root) {
        Ok(path) => path,
        Err(err) => return rollback_after_error(config, &snapshot, &stage_root, err),
    };
    if let Err(err) = executor.verify_binary(&staged_binary) {
        return rollback_after_error(config, &snapshot, &stage_root, err);
    }
    if let Err(err) = executor.upgrade_plugins(config.marketplace()) {
        return rollback_after_error(config, &snapshot, &stage_root, err);
    }
    if let Err(err) = install_staged_binary_last(&staged_binary, &config.live_binary_path()) {
        return rollback_after_error(
            config,
            &snapshot,
            &stage_root,
            UpgradeStepError::new("binary-commit", err.to_string()),
        );
    }
    if let Err(err) = executor.verify_committed(&config.live_binary_path()) {
        return rollback_after_error(config, &snapshot, &stage_root, err);
    }

    let _ = fs::remove_file(config.breadcrumb_path());
    let _ = fs::remove_dir_all(snapshot.root);
    let _ = fs::remove_dir_all(stage_root);
    Ok(())
}

fn rollback_after_error(
    config: &TransactionConfig,
    snapshot: &Snapshot,
    stage_root: &Path,
    err: UpgradeStepError,
) -> Result<(), TxnFailure> {
    eprintln!(
        "ERROR: upgrade step '{}' failed: {}",
        err.stage, err.message
    );
    finish_rollback(
        config,
        &snapshot.records,
        Some(&snapshot.root),
        Some(stage_root),
    )
}

fn recover_previous_attempt(config: &TransactionConfig) -> Result<(), TxnFailure> {
    let breadcrumb = match read_breadcrumb(&config.breadcrumb_path()) {
        Ok(breadcrumb) => breadcrumb,
        Err(err) => {
            eprintln!(
                "ERROR: found upgrade breadcrumb at {}, but could not read it: {err}",
                config.breadcrumb_path().display()
            );
            return Err(TxnFailure::RollbackIncomplete);
        }
    };
    eprintln!(
        "ERROR: previous upgrade attempt did not commit; restoring snapshot from {}",
        breadcrumb.snapshot_root.display()
    );
    finish_rollback(
        config,
        &breadcrumb.targets,
        Some(&breadcrumb.snapshot_root),
        None,
    )
}

fn finish_rollback(
    config: &TransactionConfig,
    records: &[SnapshotRecord],
    snapshot_root: Option<&Path>,
    stage_root: Option<&Path>,
) -> Result<(), TxnFailure> {
    let report = restore_snapshots(records);
    if report.residuals.is_empty() {
        let _ = fs::remove_file(config.breadcrumb_path());
        if let Some(root) = snapshot_root {
            let _ = fs::remove_dir_all(root);
        }
        if let Some(root) = stage_root {
            let _ = fs::remove_dir_all(root);
        }
        eprintln!("Upgrade failed; rollback restored all reachable snapshots.");
        Err(TxnFailure::RolledBack)
    } else {
        eprintln!("ERROR: rollback incomplete; residual state:");
        for residual in &report.residuals {
            eprintln!("  - {residual}");
        }
        Err(TxnFailure::RollbackIncomplete)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxnFailure {
    RolledBack,
    RollbackIncomplete,
}

#[derive(Debug)]
struct UpgradeLock {
    path: PathBuf,
    token: String,
}

#[derive(Debug)]
struct StaleReclaimGuard {
    path: PathBuf,
    token: String,
}

impl UpgradeLock {
    fn acquire(path: &Path, stale_timeout: Duration) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let token = format!("{}:{}", std::process::id(), unix_timestamp_secs());
        let content = lock_content(&token);
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                file.write_all(content.as_bytes())
                    .map_err(|err| err.to_string())?;
                Ok(Self {
                    path: path.to_path_buf(),
                    token,
                })
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if !lock_is_stale(path, stale_timeout)? {
                    return Err(format!("{} is held by another upgrade", path.display()));
                }
                let _reclaim = StaleReclaimGuard::acquire(path, stale_timeout)?;
                fs::remove_file(path).map_err(|err| err.to_string())?;
                Self::acquire(path, stale_timeout)
            }
            Err(err) => Err(err.to_string()),
        }
    }
}

impl Drop for UpgradeLock {
    fn drop(&mut self) {
        let Ok(content) = fs::read_to_string(&self.path) else {
            return;
        };
        if content.contains(&format!("token={}", self.token)) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl StaleReclaimGuard {
    fn acquire(lock_path: &Path, stale_timeout: Duration) -> Result<Self, String> {
        let path = stale_reclaim_path(lock_path);
        let token = format!("{}:{}", std::process::id(), unix_timestamp_secs());
        let content = lock_content(&token);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(content.as_bytes())
                    .map_err(|err| err.to_string())?;
                Ok(Self { path, token })
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&path, stale_timeout)? {
                    fs::remove_file(&path).map_err(|err| err.to_string())?;
                    return Self::acquire(lock_path, stale_timeout);
                }
                Err(format!(
                    "{} is held by another stale-lock reclaimer",
                    path.display()
                ))
            }
            Err(err) => Err(err.to_string()),
        }
    }
}

impl Drop for StaleReclaimGuard {
    fn drop(&mut self) {
        let Ok(content) = fs::read_to_string(&self.path) else {
            return;
        };
        if content.contains(&format!("token={}", self.token)) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn lock_content(token: &str) -> String {
    format!(
        "pid={}\ntimestamp={}\ntoken={token}\n",
        std::process::id(),
        unix_timestamp_secs()
    )
}

fn stale_reclaim_path(lock_path: &Path) -> PathBuf {
    let Some(file_name) = lock_path.file_name() else {
        return lock_path.with_extension("reclaim");
    };
    lock_path.with_file_name(format!("{}.reclaim", file_name.to_string_lossy()))
}

fn lock_is_stale(path: &Path, stale_timeout: Duration) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let Some(timestamp) = content.lines().find_map(|line| {
        line.strip_prefix("timestamp=")
            .and_then(|value| value.parse::<u64>().ok())
    }) else {
        return Ok(true);
    };
    Ok(unix_timestamp_secs().saturating_sub(timestamp) > stale_timeout.as_secs())
}

#[derive(Debug)]
struct Snapshot {
    root: PathBuf,
    records: Vec<SnapshotRecord>,
}

impl Snapshot {
    fn create(root: &Path, targets: &[PathBuf]) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let mut records = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            let backup = root.join(index.to_string());
            let record = SnapshotRecord::capture(target, &backup)?;
            records.push(record);
        }
        Ok(Self {
            root: root.to_path_buf(),
            records,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotRecord {
    target: PathBuf,
    backup: Option<PathBuf>,
    existed: bool,
    was_dir: bool,
}

impl SnapshotRecord {
    fn capture(target: &Path, backup: &Path) -> io::Result<Self> {
        let meta = match fs::metadata(target) {
            Ok(meta) => meta,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    target: target.to_path_buf(),
                    backup: None,
                    existed: false,
                    was_dir: false,
                });
            }
            Err(err) => return Err(err),
        };

        if meta.is_dir() {
            copy_dir_all(target, backup)?;
            Ok(Self {
                target: target.to_path_buf(),
                backup: Some(backup.to_path_buf()),
                existed: true,
                was_dir: true,
            })
        } else {
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(target, backup)?;
            fs::set_permissions(backup, meta.permissions())?;
            Ok(Self {
                target: target.to_path_buf(),
                backup: Some(backup.to_path_buf()),
                existed: true,
                was_dir: false,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Breadcrumb {
    pid: u32,
    timestamp: u64,
    snapshot_root: PathBuf,
    targets: Vec<SnapshotRecord>,
}

impl Breadcrumb {
    fn new(targets: Vec<SnapshotRecord>, snapshot_root: PathBuf) -> Self {
        Self {
            pid: std::process::id(),
            timestamp: unix_timestamp_secs(),
            snapshot_root,
            targets,
        }
    }
}

fn write_breadcrumb(path: &Path, breadcrumb: &Breadcrumb) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(breadcrumb)?;
    fs::write(path, body)
}

fn read_breadcrumb(path: &Path) -> io::Result<Breadcrumb> {
    let body = fs::read(path)?;
    serde_json::from_slice(&body).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[derive(Debug, Default)]
struct RollbackReport {
    residuals: Vec<String>,
}

fn restore_snapshots(records: &[SnapshotRecord]) -> RollbackReport {
    let mut report = RollbackReport::default();
    for record in records {
        if let Err(err) = restore_snapshot(record) {
            report
                .residuals
                .push(format!("{}: {err}", record.target.display()));
        }
    }
    report
}

fn restore_snapshot(record: &SnapshotRecord) -> io::Result<()> {
    remove_path_if_exists(&record.target)?;
    if !record.existed {
        return Ok(());
    }
    let Some(backup) = &record.backup else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "snapshot metadata says path existed but backup is missing",
        ));
    };
    if !backup.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("snapshot backup missing: {}", backup.display()),
        ));
    }
    if let Some(parent) = record.target.parent() {
        fs::create_dir_all(parent)?;
    }
    if record.was_dir {
        copy_dir_all(backup, &record.target)
    } else {
        fs::copy(backup, &record.target)?;
        fs::set_permissions(&record.target, fs::metadata(backup)?.permissions())
    }
}

fn install_staged_binary_last(staged_binary: &Path, live_binary: &Path) -> io::Result<()> {
    let parent = live_binary.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("live binary path has no parent: {}", live_binary.display()),
        )
    })?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".dvandva-upgrade-{}.tmp", transaction_id()));
    fs::copy(staged_binary, &tmp)?;
    fs::set_permissions(&tmp, fs::metadata(staged_binary)?.permissions())?;
    fs::rename(&tmp, live_binary)
}

fn remove_path_if_exists(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    fs::set_permissions(dst, fs::metadata(src)?.permissions())?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
            fs::set_permissions(&dst_path, fs::metadata(&src_path)?.permissions())?;
        }
    }
    Ok(())
}

fn transaction_id() -> String {
    format!("{}-{}", std::process::id(), unix_timestamp_secs())
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct FailingPluginUpgrade {
        cache_marker: PathBuf,
    }

    impl UpgradeExecutor for FailingPluginUpgrade {
        fn stage_binary(&mut self, stage_root: &Path) -> Result<PathBuf, UpgradeStepError> {
            let staged = stage_root.join("bin/dvandva");
            fs::create_dir_all(staged.parent().unwrap()).unwrap();
            fs::write(&staged, "new staged binary").unwrap();
            Ok(staged)
        }

        fn verify_binary(&mut self, binary: &Path) -> Result<(), UpgradeStepError> {
            if binary.is_file() {
                Ok(())
            } else {
                Err(UpgradeStepError::new(
                    "verify-staged",
                    "missing staged binary",
                ))
            }
        }

        fn upgrade_plugins(&mut self, _marketplace: &str) -> Result<(), UpgradeStepError> {
            fs::write(&self.cache_marker, "new cache").unwrap();
            Err(UpgradeStepError::new("plugins", "simulated plugin failure"))
        }

        fn verify_committed(&mut self, _live_binary: &Path) -> Result<(), UpgradeStepError> {
            panic!("commit verification must not run after a plugin failure");
        }
    }

    struct SuccessfulUpgrade {
        live_binary: PathBuf,
        live_seen_during_plugins: Option<String>,
    }

    impl UpgradeExecutor for SuccessfulUpgrade {
        fn stage_binary(&mut self, stage_root: &Path) -> Result<PathBuf, UpgradeStepError> {
            let staged = stage_root.join("bin/dvandva");
            fs::create_dir_all(staged.parent().unwrap()).unwrap();
            fs::write(&staged, "new binary").unwrap();
            Ok(staged)
        }

        fn verify_binary(&mut self, binary: &Path) -> Result<(), UpgradeStepError> {
            assert_eq!(fs::read_to_string(binary).unwrap(), "new binary");
            Ok(())
        }

        fn upgrade_plugins(&mut self, _marketplace: &str) -> Result<(), UpgradeStepError> {
            self.live_seen_during_plugins = Some(fs::read_to_string(&self.live_binary).unwrap());
            Ok(())
        }

        fn verify_committed(&mut self, live_binary: &Path) -> Result<(), UpgradeStepError> {
            assert_eq!(live_binary, self.live_binary);
            assert_eq!(fs::read_to_string(live_binary).unwrap(), "new binary");
            Ok(())
        }
    }

    struct FailingEngineStateMutation {
        claude_pointer: PathBuf,
        codex_config: PathBuf,
        codex_marketplace_file: PathBuf,
        codex_cache_marker: PathBuf,
    }

    impl UpgradeExecutor for FailingEngineStateMutation {
        fn stage_binary(&mut self, stage_root: &Path) -> Result<PathBuf, UpgradeStepError> {
            let staged = stage_root.join("bin/dvandva");
            fs::create_dir_all(staged.parent().unwrap()).unwrap();
            fs::write(&staged, "new staged binary").unwrap();
            Ok(staged)
        }

        fn verify_binary(&mut self, binary: &Path) -> Result<(), UpgradeStepError> {
            if binary.is_file() {
                Ok(())
            } else {
                Err(UpgradeStepError::new(
                    "verify-staged",
                    "missing staged binary",
                ))
            }
        }

        fn upgrade_plugins(&mut self, _marketplace: &str) -> Result<(), UpgradeStepError> {
            fs::write(
                &self.claude_pointer,
                r#"{"plugins":{"dvandva":{"version":"new"}}}"#,
            )
            .unwrap();
            fs::write(
                &self.codex_config,
                "[marketplaces.dvandva]\nsource = \"new\"\n",
            )
            .unwrap();
            fs::write(&self.codex_marketplace_file, "new marketplace").unwrap();
            fs::write(&self.codex_cache_marker, "new cache").unwrap();
            Err(UpgradeStepError::new(
                "plugins",
                "simulated engine state failure",
            ))
        }

        fn verify_committed(&mut self, _live_binary: &Path) -> Result<(), UpgradeStepError> {
            panic!("commit verification must not run after a plugin failure");
        }
    }

    #[test]
    fn rollback_restores_snapshots_and_exits_20_on_plugin_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let codex_home = tmp.path().join("codex-home");
        let state_dir = tmp.path().join("state");
        let live_binary = home.join(".cargo/bin/dvandva");
        let claude_cache = home.join(".claude/plugins/cache/dvandva/dvandva");
        let cache_marker = claude_cache.join("version.txt");

        fs::create_dir_all(live_binary.parent().unwrap()).unwrap();
        fs::create_dir_all(&claude_cache).unwrap();
        fs::write(&live_binary, "old binary").unwrap();
        fs::write(&cache_marker, "old cache").unwrap();

        let config = TransactionConfig::new("local-marketplace", &home, &codex_home, &state_dir);
        let mut executor = FailingPluginUpgrade {
            cache_marker: cache_marker.clone(),
        };

        let code = run_transactional_upgrade(&config, &mut executor);

        assert_eq!(code, EXIT_ROLLED_BACK);
        assert_eq!(fs::read_to_string(&live_binary).unwrap(), "old binary");
        assert_eq!(fs::read_to_string(&cache_marker).unwrap(), "old cache");
        assert!(
            !config.breadcrumb_path().exists(),
            "clean rollback should remove the crash breadcrumb"
        );
    }

    #[test]
    fn successful_transaction_swaps_binary_last_deletes_breadcrumb_and_exits_0() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let codex_home = tmp.path().join("codex-home");
        let state_dir = tmp.path().join("state");
        let live_binary = home.join(".cargo/bin/dvandva");

        fs::create_dir_all(live_binary.parent().unwrap()).unwrap();
        fs::write(&live_binary, "old binary").unwrap();

        let config = TransactionConfig::new("local-marketplace", &home, &codex_home, &state_dir);
        let mut executor = SuccessfulUpgrade {
            live_binary: live_binary.clone(),
            live_seen_during_plugins: None,
        };

        let code = run_transactional_upgrade(&config, &mut executor);

        assert_eq!(code, EXIT_COMMITTED);
        assert_eq!(
            executor.live_seen_during_plugins.as_deref(),
            Some("old binary"),
            "plugins must run before the live binary is swapped"
        );
        assert_eq!(fs::read_to_string(&live_binary).unwrap(), "new binary");
        assert!(!config.breadcrumb_path().exists());
        assert!(!config.lock_path().exists());
    }

    #[test]
    fn invalid_existing_breadcrumb_exits_21_without_starting_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let codex_home = tmp.path().join("codex-home");
        let state_dir = tmp.path().join("state");
        let config = TransactionConfig::new("local-marketplace", &home, &codex_home, &state_dir);
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(config.breadcrumb_path(), "not json").unwrap();

        let mut executor = SuccessfulUpgrade {
            live_binary: home.join(".cargo/bin/dvandva"),
            live_seen_during_plugins: None,
        };

        let code = run_transactional_upgrade(&config, &mut executor);

        assert_eq!(code, EXIT_ROLLBACK_INCOMPLETE);
        assert_eq!(executor.live_seen_during_plugins, None);
        assert!(
            config.breadcrumb_path().exists(),
            "unrecoverable breadcrumb should remain for residual inspection"
        );
        assert!(!config.lock_path().exists());
    }

    #[test]
    fn live_lock_with_breadcrumb_exits_21_without_claiming_clean_rollback() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let codex_home = tmp.path().join("codex-home");
        let state_dir = tmp.path().join("state");
        let config = TransactionConfig::new("local-marketplace", &home, &codex_home, &state_dir);
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            config.lock_path(),
            format!(
                "pid=999999999\ntimestamp={}\ntoken=999999999:{}\n",
                unix_timestamp_secs(),
                unix_timestamp_secs()
            ),
        )
        .unwrap();
        write_breadcrumb(
            &config.breadcrumb_path(),
            &Breadcrumb::new(Vec::new(), state_dir.join("upgrade-snapshots/previous")),
        )
        .unwrap();

        let mut executor = SuccessfulUpgrade {
            live_binary: home.join(".cargo/bin/dvandva"),
            live_seen_during_plugins: None,
        };

        let code = run_transactional_upgrade(&config, &mut executor);

        assert_eq!(code, EXIT_ROLLBACK_INCOMPLETE);
        assert!(
            config.breadcrumb_path().exists(),
            "recovery was not attempted, so the breadcrumb must remain"
        );
        assert_eq!(executor.live_seen_during_plugins, None);
    }

    #[test]
    fn stale_lock_reclaim_guard_blocks_competing_reclaimer() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join("upgrade.lock");
        fs::write(&lock_path, "pid=1\ntimestamp=0\ntoken=1:0\n").unwrap();
        fs::write(
            tmp.path().join("upgrade.lock.reclaim"),
            format!(
                "pid=2\ntimestamp={}\ntoken=2:{}\n",
                unix_timestamp_secs(),
                unix_timestamp_secs()
            ),
        )
        .unwrap();

        let err = UpgradeLock::acquire(&lock_path, DEFAULT_STALE_LOCK_TIMEOUT)
            .expect_err("reclaim sentinel should block a competing stale-lock owner");

        assert!(
            err.contains("reclaim"),
            "error should name the reclaim guard; got: {err}"
        );
        assert_eq!(
            fs::read_to_string(&lock_path).unwrap(),
            "pid=1\ntimestamp=0\ntoken=1:0\n",
            "competing reclaimer must not remove the stale lock while a guard exists"
        );
    }

    #[test]
    fn stale_reclaim_guard_is_reclaimed() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join("upgrade.lock");
        let reclaim_path = tmp.path().join("upgrade.lock.reclaim");
        fs::write(&lock_path, "pid=1\ntimestamp=0\ntoken=1:0\n").unwrap();
        fs::write(&reclaim_path, "pid=2\ntimestamp=0\ntoken=2:0\n").unwrap();

        let lock = UpgradeLock::acquire(&lock_path, DEFAULT_STALE_LOCK_TIMEOUT)
            .expect("stale reclaim guard should be replaceable");

        assert!(lock_path.exists());
        assert!(
            !reclaim_path.exists(),
            "successful stale-lock reclaim must clean up its reclaim guard"
        );
        drop(lock);
    }

    #[test]
    fn rollback_restores_engine_specific_plugin_state() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let codex_home = tmp.path().join("codex-home");
        let state_dir = tmp.path().join("state");
        let live_binary = home.join(".cargo/bin/dvandva");
        let claude_pointer = home.join(".claude/plugins/installed_plugins.json");
        let codex_config = codex_home.join("config.toml");
        let codex_marketplace_file = codex_home
            .join(".tmp/marketplaces/dvandva")
            .join("checkout.txt");
        let codex_cache_marker = codex_home
            .join("plugins/cache/dvandva/dvandva")
            .join("version.txt");

        for path in [
            &live_binary,
            &claude_pointer,
            &codex_config,
            &codex_marketplace_file,
            &codex_cache_marker,
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        fs::write(&live_binary, "old binary").unwrap();
        fs::write(
            &claude_pointer,
            r#"{"plugins":{"dvandva":{"version":"old"}}}"#,
        )
        .unwrap();
        fs::write(&codex_config, "[marketplaces.dvandva]\nsource = \"old\"\n").unwrap();
        fs::write(&codex_marketplace_file, "old marketplace").unwrap();
        fs::write(&codex_cache_marker, "old cache").unwrap();

        let config = TransactionConfig::new("axatbhardwaj/Dvandva", &home, &codex_home, &state_dir);
        let mut executor = FailingEngineStateMutation {
            claude_pointer: claude_pointer.clone(),
            codex_config: codex_config.clone(),
            codex_marketplace_file: codex_marketplace_file.clone(),
            codex_cache_marker: codex_cache_marker.clone(),
        };

        let code = run_transactional_upgrade(&config, &mut executor);

        assert_eq!(code, EXIT_ROLLED_BACK);
        assert_eq!(
            fs::read_to_string(&claude_pointer).unwrap(),
            r#"{"plugins":{"dvandva":{"version":"old"}}}"#
        );
        assert_eq!(
            fs::read_to_string(&codex_config).unwrap(),
            "[marketplaces.dvandva]\nsource = \"old\"\n"
        );
        assert_eq!(
            fs::read_to_string(&codex_marketplace_file).unwrap(),
            "old marketplace"
        );
        assert_eq!(
            fs::read_to_string(&codex_cache_marker).unwrap(),
            "old cache"
        );
    }
}
