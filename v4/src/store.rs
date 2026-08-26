use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use thiserror::Error;
use uuid::Uuid;

use crate::model::RunBaton;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("run already exists")]
    RunExists,
    #[error("run does not exist")]
    RunMissing,
    #[error("revision conflict: expected {expected}, found {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid baton JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct RunChannel {
    directory: PathBuf,
}

impl RunChannel {
    pub fn open(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn create(&self, initial: &RunBaton) -> Result<RunBaton, StoreError> {
        fs::create_dir_all(&self.directory)?;
        self.with_lock(|| {
            if self.baton_path().exists() {
                return Err(StoreError::RunExists);
            }
            self.install(initial)?;
            self.write_history(initial)?;
            Ok(initial.clone())
        })
    }

    pub fn read(&self) -> Result<RunBaton, StoreError> {
        let path = self.baton_path();
        if !path.exists() {
            return Err(StoreError::RunMissing);
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn compare_and_swap(
        &self,
        expected_revision: u64,
        next: &RunBaton,
    ) -> Result<RunBaton, StoreError> {
        self.with_lock(|| {
            let current = self.read()?;
            if current.revision != expected_revision {
                return Err(StoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            if next.revision != expected_revision + 1 {
                return Err(StoreError::RevisionConflict {
                    expected: expected_revision + 1,
                    actual: next.revision,
                });
            }
            self.install(next)?;
            self.write_history(next)?;
            Ok(next.clone())
        })
    }

    fn baton_path(&self) -> PathBuf {
        self.directory.join("baton.json")
    }

    fn with_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.directory.join(".baton.lock"))?;
        lock.lock_exclusive()?;
        let result = operation();
        FileExt::unlock(&lock)?;
        result
    }

    fn install(&self, baton: &RunBaton) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(baton)?;
        let temporary = self
            .directory
            .join(format!(".baton.{}.tmp", Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, self.baton_path())?;
        sync_directory(&self.directory)?;
        Ok(())
    }

    fn write_history(&self, baton: &RunBaton) -> Result<(), StoreError> {
        let directory = self.directory.join("history");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{:020}.json", baton.revision));
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        file.write_all(&serde_json::to_vec_pretty(baton)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        sync_directory(&directory)?;
        Ok(())
    }
}

fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    File::open(path)?.sync_all()
}
