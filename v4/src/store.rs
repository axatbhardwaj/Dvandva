use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::model::{
    create_handoff_obligation, normalize_deliverables, normalize_participants,
    DeliverableRequirement, HandoffKind, MigrationProvenance, Publication, PublicationPolicy,
    RecoveryProvenance, RunBaton, Status, LEGACY_SCHEMA, SCHEMA,
};

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
    #[error("baton violates the {0} schema invariants")]
    InvalidBaton(String),
}

#[derive(Deserialize)]
struct SchemaEnvelope {
    schema: String,
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
        validate_baton(initial)?;
        if initial.schema != SCHEMA
            || initial.revision != 0
            || initial.migration.is_some()
            || initial.participants.worker.claim.is_some()
            || initial.participants.reviewer.claim.is_some()
            || initial.publication_binding
                != Some(create_handoff_obligation(HandoffKind::RunStarted, 0, 0))
        {
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
        decode_baton(&fs::read(path)?)
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
            validate_baton(next)?;
            match (current.schema.as_str(), next.schema.as_str()) {
                (left, right) if left == right => {}
                (LEGACY_SCHEMA, SCHEMA) => validate_migration_edge(&current, next)?,
                _ => return Err(StoreError::InvalidSchemaTransition),
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
            let mut previous: Option<RunBaton> = None;
            let mut crossed = false;
            for revision in 0..=high {
                let path = history_dir.join(format!("{revision:020}.json"));
                let baton =
                    decode_baton(&fs::read(path)?).map_err(|_| StoreError::InvalidHistory)?;
                if baton.revision != revision {
                    return Err(StoreError::InvalidHistory);
                }
                if let Some(previous_baton) = previous.as_ref() {
                    match (previous_baton.schema.as_str(), baton.schema.as_str()) {
                        (SCHEMA, LEGACY_SCHEMA) => return Err(StoreError::InvalidHistory),
                        (LEGACY_SCHEMA, SCHEMA) if crossed => {
                            return Err(StoreError::InvalidHistory)
                        }
                        (LEGACY_SCHEMA, SCHEMA) => {
                            validate_migration_edge(previous_baton, &baton)
                                .map_err(|_| StoreError::InvalidHistory)?;
                            crossed = true;
                        }
                        (left, right) if left != right => return Err(StoreError::InvalidHistory),
                        _ => {}
                    }
                }
                previous = Some(baton.clone());
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
            validate_baton(&recovered)?;
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

pub fn migrate_legacy_baton(current: &RunBaton) -> Result<RunBaton, StoreError> {
    validate_baton(current)?;
    if current.schema != LEGACY_SCHEMA || current.objective.summary.trim().is_empty() {
        return Err(StoreError::InvalidSchemaTransition);
    }
    let (worker, reviewer) = normalize_participants(
        current.participants.worker.harness.clone(),
        current.participants.reviewer.harness.clone(),
    )
    .map_err(|_| StoreError::InvalidSchemaTransition)?;
    let mut next = current.clone();
    let legacy_state_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(current).expect("baton serializes"))
    );
    next.schema = SCHEMA.to_owned();
    next.participants.worker.harness = worker;
    next.participants.reviewer.harness = reviewer;
    next.participants.worker.claim = None;
    next.participants.reviewer.claim = None;
    next.status = Status::Revising;
    next.assignee = crate::model::Assignee::Worker;
    next.revision += 1;
    next.scope_revision = 0;
    next.scope_deliverables = vec![DeliverableRequirement {
        id: "legacy_objective".to_owned(),
        description: current.objective.summary.trim().to_owned(),
    }];
    next.checkpoint = None;
    next.review = None;
    next.publication = Publication::default();
    next.publication_policy = Some(PublicationPolicy::fixed());
    next.publication_binding = Some(create_handoff_obligation(
        HandoffKind::ProtocolUpgraded,
        next.revision,
        0,
    ));
    next.human_decision = None;
    next.terminal = None;
    next.recovery = None;
    next.migration = Some(MigrationProvenance {
        from_schema: LEGACY_SCHEMA.to_owned(),
        from_revision: current.revision,
        legacy_state_digest,
        legacy_checkpoint: current.checkpoint.clone(),
    });
    Ok(next)
}

fn decode_baton(bytes: &[u8]) -> Result<RunBaton, StoreError> {
    let envelope: SchemaEnvelope = serde_json::from_slice(bytes)?;
    validate_supported_schema(&envelope.schema)?;
    let baton: RunBaton = serde_json::from_slice(bytes)?;
    validate_baton(&baton)?;
    Ok(baton)
}

fn validate_baton(baton: &RunBaton) -> Result<(), StoreError> {
    validate_supported_schema(&baton.schema)?;
    if baton.schema == LEGACY_SCHEMA {
        if baton.scope_revision != 0
            || !baton.scope_deliverables.is_empty()
            || baton.publication_policy.is_some()
            || baton.publication_binding.is_some()
            || baton.migration.is_some()
        {
            return Err(StoreError::InvalidBaton(baton.schema.clone()));
        }
        return Ok(());
    }

    let normalized_participants = normalize_participants(
        baton.participants.worker.harness.clone(),
        baton.participants.reviewer.harness.clone(),
    )
    .map_err(|_| StoreError::InvalidBaton(baton.schema.clone()))?;
    let normalized_deliverables = normalize_deliverables(baton.scope_deliverables.clone())
        .map_err(|_| StoreError::InvalidBaton(baton.schema.clone()))?;
    let binding = baton
        .publication_binding
        .as_ref()
        .ok_or_else(|| StoreError::InvalidBaton(baton.schema.clone()))?;
    if normalized_participants
        != (
            baton.participants.worker.harness.clone(),
            baton.participants.reviewer.harness.clone(),
        )
        || normalized_deliverables != baton.scope_deliverables
        || baton.publication_policy.as_ref() != Some(&PublicationPolicy::fixed())
        || !baton.publication.required
        || binding.obligation.scope_revision != baton.scope_revision
        || binding.obligation.handoff_revision > baton.revision
    {
        return Err(StoreError::InvalidBaton(baton.schema.clone()));
    }
    if let Some(migration) = baton.migration.as_ref() {
        if migration.from_schema != LEGACY_SCHEMA
            || migration.from_revision >= baton.revision
            || migration.legacy_state_digest.len() != 64
            || !migration
                .legacy_state_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(StoreError::InvalidBaton(baton.schema.clone()));
        }
    }
    Ok(())
}

fn validate_migration_edge(current: &RunBaton, next: &RunBaton) -> Result<(), StoreError> {
    if migrate_legacy_baton(current)? == *next {
        Ok(())
    } else {
        Err(StoreError::InvalidSchemaTransition)
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
