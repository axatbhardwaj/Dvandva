//! The baton-directory mkdir lock with fencing token, ported from
//! `dvandva-write.sh`.
//!
//! mkdir is the atomic primitive (chosen over flock for macOS portability in
//! the shell era; kept for on-disk compatibility). The lock is a directory
//! `<baton-dir>/.baton.lock.d` holding `started_at` (acquisition wall clock,
//! lets a waiter age out a crashed holder) and `owner` (the fencing token; a
//! stealer provably replaces it so the original holder detects the theft
//! before its irreversible install `mv`).

use std::io::Read;
use std::path::{Path, PathBuf};

/// Name of the lock directory beside the baton file.
pub const LOCK_DIR_NAME: &str = ".baton.lock.d";

/// Outcome of a lock acquisition attempt.
#[derive(Debug)]
pub enum Acquire {
    /// Lock held; the fencing token was stamped into `owner`.
    Held(String),
    /// Environmental failure: the lock dir could not be created and does not
    /// exist (unwritable baton dir). There is no race to lose — the caller
    /// proceeds unlocked and lets the real install fail with its own code.
    NoDir,
    /// A non-directory squats the lock path. Never a real lock; proceeding
    /// unlocked would reopen the write race. Callers must fail closed
    /// (write helper: exit 28).
    SquattedNonDir,
}

fn lock_dir(baton_dir: &Path) -> PathBuf {
    baton_dir.join(LOCK_DIR_NAME)
}

/// Generate a fencing token: pid, wall clock, and urandom entropy.
pub fn fencing_token() -> String {
    let pid = std::process::id();
    let epoch = crate::util::now_epoch_nanos();
    let mut entropy = [0u8; 8];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut entropy);
    }
    let hex: String = entropy.iter().map(|b| format!("{b:02x}")).collect();
    format!("{pid}.{epoch}.{hex}")
}

/// Acquire the lock, aging out an abandoned holder after `timeout_secs`.
///
/// Mirrors `acquire_lock()` in `dvandva-write.sh`: spin on contention with a
/// 100ms sleep; steal by atomic rename-aside + remove when the holder's
/// `started_at` (or, if missing, our own first observation) is older than
/// the timeout.
pub fn acquire(baton_dir: &Path, timeout_secs: u64) -> Acquire {
    let dir = lock_dir(baton_dir);
    let mut first_seen = crate::util::now_epoch();
    loop {
        if std::fs::create_dir(&dir).is_ok() {
            let token = fencing_token();
            let _ = std::fs::write(dir.join("started_at"), crate::util::now_epoch().to_string());
            let _ = std::fs::write(dir.join("owner"), &token);
            return Acquire::Held(token);
        }
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if !meta.is_dir() => return Acquire::SquattedNonDir,
            Err(_) => return Acquire::NoDir,
            Ok(_) => {}
        }
        let now = crate::util::now_epoch();
        let started: Option<u64> = std::fs::read_to_string(dir.join("started_at"))
            .ok()
            .and_then(|s| s.trim().parse().ok());
        let age = now.saturating_sub(started.unwrap_or(first_seen));
        if age >= timeout_secs {
            let stale = dir.with_extension(format!("stale.{}", std::process::id()));
            if std::fs::rename(&dir, &stale).is_ok() {
                let _ = std::fs::remove_dir_all(&stale);
            }
            first_seen = crate::util::now_epoch();
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// True while the `owner` file still carries our fencing token.
pub fn holds(baton_dir: &Path, token: &str) -> bool {
    std::fs::read_to_string(lock_dir(baton_dir).join("owner"))
        .map(|owner| owner == token)
        .unwrap_or(false)
}

/// Release the lock, but only if we still own it (never delete a thief's
/// lock — mirrors the shell's `LOCK_ACQUIRED=0` guard after a theft).
pub fn release(baton_dir: &Path, token: &str) {
    if holds(baton_dir, token) {
        let _ = std::fs::remove_dir_all(lock_dir(baton_dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let Acquire::Held(token) = acquire(dir.path(), 30) else {
            panic!("expected Held");
        };
        assert!(holds(dir.path(), &token));
        release(dir.path(), &token);
        assert!(!lock_dir(dir.path()).exists());
    }

    #[test]
    fn missing_baton_dir_reports_nodir() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("absent");
        assert!(matches!(acquire(&gone, 1), Acquire::NoDir));
    }

    #[test]
    fn non_directory_squat_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(lock_dir(dir.path()), "squatter").unwrap();
        assert!(matches!(acquire(dir.path(), 30), Acquire::SquattedNonDir));
    }

    #[test]
    fn stale_lock_is_aged_out_and_stolen() {
        let dir = tempfile::tempdir().unwrap();
        let ld = lock_dir(dir.path());
        std::fs::create_dir(&ld).unwrap();
        std::fs::write(ld.join("started_at"), "0").unwrap();
        std::fs::write(ld.join("owner"), "dead-holder").unwrap();
        let Acquire::Held(token) = acquire(dir.path(), 30) else {
            panic!("expected steal to succeed");
        };
        assert!(holds(dir.path(), &token));
        release(dir.path(), &token);
    }

    #[test]
    fn release_never_deletes_a_thiefs_lock() {
        let dir = tempfile::tempdir().unwrap();
        let Acquire::Held(token) = acquire(dir.path(), 30) else {
            panic!("expected Held");
        };
        std::fs::write(lock_dir(dir.path()).join("owner"), "thief").unwrap();
        assert!(!holds(dir.path(), &token));
        release(dir.path(), &token);
        assert!(lock_dir(dir.path()).exists(), "thief's lock must survive");
    }
}
