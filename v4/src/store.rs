use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use thiserror::Error;
use uuid::Uuid;

use crate::model::{RecoveryProvenance, RunBaton, Status, LEGACY_SCHEMA, SCHEMA};

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
    #[error("history is missing, incomplete, or inconsistent")]
    InvalidHistory,
    #[error("terminal state cannot be recovered into an active state")]
    TerminalState,
    #[error("unsupported baton schema: {0}")]
    UnsupportedSchema(String),
    #[error("v1 runs require a dedicated protocol upgrade")]
    MigrationRequired,
    #[error("schema transition is not monotonic")]
    InvalidSchemaTransition,
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
        if initial.schema != SCHEMA {
            return Err(StoreError::InvalidSchemaTransition);
        }
        fs::create_dir_all(&self.directory)?;
        self.with_lock(|| {
            if self.baton_path().exists() {
                return Err(StoreError::RunExists);
            }
            self.write_history(initial)?;
            self.install(initial)?;
            Ok(initial.clone())
        })
    }

    pub fn read(&self) -> Result<RunBaton, StoreError> {
        let path = self.baton_path();
        if !path.exists() {
            return Err(StoreError::RunMissing);
        }
        let baton: RunBaton = serde_json::from_slice(&fs::read(path)?)?;
        validate_supported_schema(&baton.schema)?;
        Ok(baton)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
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
            let valid_schema_edge = current.schema == next.schema
                || (current.schema == LEGACY_SCHEMA && next.schema == SCHEMA);
            if !valid_schema_edge {
                return Err(StoreError::InvalidSchemaTransition);
            }
            self.write_history(next)?;
            self.install(next)?;
            Ok(next.clone())
        })
    }

    pub fn recover(&self, from_revision: u64) -> Result<RunBaton, StoreError> {
        self.with_lock(|| {
            let history_dir = self.directory.join("history");
            let mut revisions = fs::read_dir(&history_dir)?
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    entry
                        .path()
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .and_then(|stem| stem.parse::<u64>().ok())
                })
                .collect::<Vec<_>>();
            revisions.sort_unstable();
            let high = *revisions.last().ok_or(StoreError::InvalidHistory)?;
            if revisions != (0..=high).collect::<Vec<_>>() || from_revision != high {
                return Err(StoreError::InvalidHistory);
            }

            let mut run_id = None;
            let mut selected = None;
            let mut terminal_head = false;
            let mut previous_schema: Option<String> = None;
            let mut crossed = false;
            for revision in 0..=high {
                let path = history_dir.join(format!("{revision:020}.json"));
                let baton: RunBaton = serde_json::from_slice(&fs::read(path)?)?;
                validate_supported_schema(&baton.schema).map_err(|_| StoreError::InvalidHistory)?;
                if baton.revision != revision {
                    return Err(StoreError::InvalidHistory);
                }
                if let Some(previous) = previous_schema.as_deref() {
                    match (previous, baton.schema.as_str()) {
                        (SCHEMA, LEGACY_SCHEMA) => return Err(StoreError::InvalidHistory),
                        (LEGACY_SCHEMA, SCHEMA) if crossed => {
                            return Err(StoreError::InvalidHistory)
                        }
                        (LEGACY_SCHEMA, SCHEMA) => crossed = true,
                        (left, right) if left != right => return Err(StoreError::InvalidHistory),
                        _ => {}
                    }
                }
                previous_schema = Some(baton.schema.clone());
                match &run_id {
                    Some(expected) if expected != &baton.run_id => {
                        return Err(StoreError::InvalidHistory)
                    }
                    None => run_id = Some(baton.run_id.clone()),
                    _ => {}
                }
                if revision == high {
                    terminal_head = matches!(&baton.status, Status::Done | Status::Abandoned);
                }
                if revision == from_revision {
                    selected = Some(baton);
                }
            }
            if terminal_head {
                return Err(StoreError::TerminalState);
            }
            let mut recovered = selected.ok_or(StoreError::InvalidHistory)?;
            if recovered.schema != SCHEMA {
                return Err(StoreError::MigrationRequired);
            }
            recovered.revision = high + 1;
            recovered.participants.worker.claim = None;
            recovered.participants.reviewer.claim = None;
            recovered.recovery = Some(RecoveryProvenance {
                from_revision,
                previous_high_revision: high,
            });
            self.write_history(&recovered)?;
            self.install(&recovered)?;
            Ok(recovered)
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
            .truncate(false)
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

pub fn require_current_schema(baton: &RunBaton) -> Result<(), StoreError> {
    match baton.schema.as_str() {
        SCHEMA => Ok(()),
        LEGACY_SCHEMA => Err(StoreError::MigrationRequired),
        other => Err(StoreError::UnsupportedSchema(other.to_owned())),
    }
}

fn validate_supported_schema(schema: &str) -> Result<(), StoreError> {
    if matches!(schema, SCHEMA | LEGACY_SCHEMA) {
        Ok(())
    } else {
        Err(StoreError::UnsupportedSchema(schema.to_owned()))
    }
}

fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    File::open(path)?.sync_all()
}
