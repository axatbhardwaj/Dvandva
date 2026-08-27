use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{claim::Role, model::RunBaton};

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("unsafe credential path component")]
    UnsafePath,
    #[error("credential path is not a private directory")]
    UnsafeDirectory,
    #[error("credential already exists for this session and role")]
    Exists,
    #[error("credential does not exist for this session and role")]
    Missing,
    #[error("credential file is not private")]
    UnsafeFile,
    #[error("credential path is not owned by the current user")]
    UnsafeOwner,
    #[error("credential identity does not match the requested role session")]
    Mismatch,
    #[error("credential I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid credential JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credential {
    pub run_dir: PathBuf,
    pub run_id: String,
    pub role: Role,
    pub session_id: String,
    pub epoch: u64,
    pub token: String,
}

pub struct ClaimCredentialLock {
    _lock: File,
    root: PathBuf,
    session_id: String,
    role: Role,
}

impl ClaimCredentialLock {
    pub fn prepare(&self, baton: &RunBaton) -> Result<PathBuf, CredentialError> {
        prepare_for_claim(&self.root, &self.session_id, baton, self.role)
    }

    pub fn store(&self, credential: &Credential) -> Result<PathBuf, CredentialError> {
        store(&self.root, credential)
    }
}

pub fn lock_for_claim(
    root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<ClaimCredentialLock, CredentialError> {
    let target = path(root, session_id, run_id, role)?;
    ensure_private_dir(root)?;
    ensure_private_dir(&root.join(session_id))?;
    let directory = root.join(session_id).join(run_id);
    ensure_private_dir(&directory)?;
    let lock_path = target.with_file_name(format!(".{}.claim.lock", role_name(role)));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path)?;
    let metadata = lock.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(CredentialError::UnsafeFile);
    }
    ensure_current_owner(&metadata)?;
    lock.lock_exclusive()?;
    Ok(ClaimCredentialLock {
        _lock: lock,
        root: root.to_owned(),
        session_id: session_id.to_owned(),
        role,
    })
}

pub fn path(
    root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<PathBuf, CredentialError> {
    if !safe_segment(session_id) || !safe_segment(run_id) {
        return Err(CredentialError::UnsafePath);
    }
    Ok(root
        .join(session_id)
        .join(run_id)
        .join(format!("{}.json", role_name(role))))
}

pub fn store(root: &Path, credential: &Credential) -> Result<PathBuf, CredentialError> {
    let target = prepare(
        root,
        &credential.session_id,
        &credential.run_id,
        credential.role,
    )?;
    let directory = root.join(&credential.session_id).join(&credential.run_id);

    let temporary = directory.join(format!(".credential.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(credential)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::hard_link(&temporary, &target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CredentialError::Exists
            } else {
                CredentialError::Io(error)
            }
        })?;
        fs::remove_file(&temporary)?;
        File::open(&directory)?.sync_all()?;
        Ok::<(), CredentialError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(target)
}

pub fn prepare(
    root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<PathBuf, CredentialError> {
    let target = path(root, session_id, run_id, role)?;
    ensure_private_dir(root)?;
    ensure_private_dir(&root.join(session_id))?;
    let directory = root.join(session_id).join(run_id);
    ensure_private_dir(&directory)?;
    if target.exists() {
        return Err(CredentialError::Exists);
    }
    Ok(target)
}

pub fn prepare_for_claim(
    root: &Path,
    session_id: &str,
    baton: &RunBaton,
    role: Role,
) -> Result<PathBuf, CredentialError> {
    let target = path(root, session_id, &baton.run_id, role)?;
    if target.exists() {
        let stored = load(root, session_id, &baton.run_id, role)?;
        let claim = match role {
            Role::Worker => baton.participants.worker.claim.as_ref(),
            Role::Reviewer => baton.participants.reviewer.claim.as_ref(),
        };
        let matches_active = claim.is_some_and(|claim| {
            let live = OffsetDateTime::parse(&claim.lease_expires_at, &Rfc3339)
                .is_ok_and(|expiry| expiry > OffsetDateTime::now_utc());
            live && claim.session_id == stored.session_id
                && claim.epoch == stored.epoch
                && claim.token_digest == digest(&stored.token)
        });
        if matches_active {
            return Err(CredentialError::Exists);
        }
        fs::remove_file(&target)?;
        File::open(target.parent().ok_or(CredentialError::UnsafeDirectory)?)?.sync_all()?;
    }
    prepare(root, session_id, &baton.run_id, role)
}

pub fn load(
    root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<Credential, CredentialError> {
    let target = path(root, session_id, run_id, role)?;
    ensure_private_dir(root)?;
    ensure_private_dir(&root.join(session_id))?;
    ensure_private_dir(&root.join(session_id).join(run_id))?;
    let metadata = fs::symlink_metadata(&target).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CredentialError::Missing
        } else {
            CredentialError::Io(error)
        }
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(CredentialError::UnsafeFile);
    }
    ensure_current_owner(&metadata)?;
    let credential: Credential = serde_json::from_slice(&fs::read(target)?)?;
    if credential.session_id != session_id || credential.run_id != run_id || credential.role != role
    {
        return Err(CredentialError::Mismatch);
    }
    Ok(credential)
}

fn ensure_private_dir(path: &Path) -> Result<(), CredentialError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_private_dir(&metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = DirBuilder::new();
            builder.mode(0o700);
            match builder.create(path) {
                Ok(()) => {
                    let parent = path.parent().ok_or(CredentialError::UnsafeDirectory)?;
                    File::open(parent)?.sync_all()?;
                    Ok(())
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    validate_private_dir(&fs::symlink_metadata(path)?)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_private_dir(metadata: &fs::Metadata) -> Result<(), CredentialError> {
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(CredentialError::UnsafeDirectory);
    }
    ensure_current_owner(metadata)
}

fn ensure_current_owner(metadata: &fs::Metadata) -> Result<(), CredentialError> {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    let current_uid = unsafe { libc::geteuid() };
    if metadata.uid() != current_uid {
        return Err(CredentialError::UnsafeOwner);
    }
    Ok(())
}

fn safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Worker => "worker",
        Role::Reviewer => "reviewer",
    }
}

fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
