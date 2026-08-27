use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::model::WorkspaceIdentity;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("workspace is not inside a Git repository")]
    RepositoryMissing,
    #[error("repository origin is missing or unsupported")]
    InvalidOrigin,
    #[error("Git returned invalid UTF-8")]
    InvalidUtf8,
}

pub fn identify(workspace: &Path) -> Result<WorkspaceIdentity, IdentityError> {
    let top_level = git_output(workspace, &["rev-parse", "--show-toplevel"])
        .map_err(|_| IdentityError::RepositoryMissing)?;
    let origin = git_output(workspace, &["config", "--get", "remote.origin.url"]).ok();
    let repository_id = match origin.as_deref() {
        Some(origin) => normalize_origin(origin).ok_or(IdentityError::InvalidOrigin)?,
        None => local_fingerprint(workspace)?,
    };

    Ok(WorkspaceIdentity {
        repository_id,
        origin,
        worktree: Some(top_level),
    })
}

fn local_fingerprint(workspace: &Path) -> Result<String, IdentityError> {
    let common_dir = git_output(
        workspace,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let canonical =
        std::fs::canonicalize(common_dir).map_err(|_| IdentityError::RepositoryMissing)?;
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    Ok(format!("local:{digest:x}"))
}

fn git_output(workspace: &Path, args: &[&str]) -> Result<String, IdentityError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(PathBuf::from(workspace))
        .args(args)
        .output()
        .map_err(|_| IdentityError::RepositoryMissing)?;
    if !output.status.success() {
        return Err(IdentityError::RepositoryMissing);
    }
    let value = String::from_utf8(output.stdout).map_err(|_| IdentityError::InvalidUtf8)?;
    Ok(value.trim().to_owned())
}

fn normalize_origin(origin: &str) -> Option<String> {
    let origin = origin.trim();
    let host_and_path = if let Some((_, remainder)) = origin.split_once("://") {
        let (authority, path) = remainder.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        format!("{host}/{path}")
    } else {
        let (authority, path) = origin.split_once(':')?;
        let host = authority.rsplit('@').next()?;
        format!("{host}/{path}")
    };
    let normalized = host_and_path
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or(host_and_path.trim_matches('/'))
        .to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}
