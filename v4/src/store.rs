use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::model::{
    checkpoint_manifest_digest, create_handoff_obligation, normalize_deliverables,
    normalize_participants, valid_exact_reference, valid_sha256, Assignee, Checkpoint,
    DeliverableRequirement, HandoffKind, LegacyPublication, MigrationProvenance, ParticipantClaim,
    PublicationBinding, PublicationPolicy, RecoveryProvenance, RunBaton, Status,
    TerminalProvenance, EXPLAINER_ACCESS, EXPLAINER_CHANNEL, EXPLAINER_PUBLISHER_HARNESS,
    EXPLAINER_REVIEWER_HARNESS, LEGACY_SCHEMA, SCHEMA,
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
    #[error("legacy participant claim is still live")]
    LegacyClaimLive,
    #[error("legacy participant claim has an invalid lease timestamp")]
    InvalidLeaseTimestamp,
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
        if !valid_v2_creation_root(initial) {
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

    pub fn checkpoint_identity_seen(
        &self,
        identity: &str,
        through_revision: u64,
    ) -> Result<bool, StoreError> {
        for revision in 0..=through_revision {
            let baton = self.read_history_revision(revision)?;
            if baton.schema == SCHEMA
                && (baton
                    .checkpoint
                    .as_ref()
                    .is_some_and(|checkpoint| checkpoint.identity == identity)
                    || baton
                        .checkpoint_history
                        .iter()
                        .any(|checkpoint| checkpoint.checkpoint_identity == identity))
            {
                return Ok(true);
            }
        }
        Ok(false)
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
            if self.read_history_revision(expected_revision)? != current {
                return Err(StoreError::InvalidHistory);
            }
            if current.schema != next.schema {
                return Err(StoreError::InvalidSchemaTransition);
            }
            let claims_changed = current.participants.worker.claim
                != next.participants.worker.claim
                || current.participants.reviewer.claim != next.participants.reviewer.claim;
            if claims_changed {
                return Err(StoreError::InvalidHistory);
            }
            validate_baton(next)?;
            validate_history_edge(&current, next)?;
            self.write_history(next)?;
            self.install(next)?;
            Ok(next.clone())
        })
    }

    pub(crate) fn mutate_locked<T, E>(
        &self,
        expected_revision: u64,
        mutation: impl FnOnce(&mut RunBaton, OffsetDateTime) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        self.with_lock_error(|| {
            let current = self.read().map_err(E::from)?;
            if current.revision != expected_revision {
                return Err(E::from(StoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: current.revision,
                }));
            }
            if self
                .read_history_revision(expected_revision)
                .map_err(E::from)?
                != current
            {
                return Err(E::from(StoreError::InvalidHistory));
            }
            if current.schema != SCHEMA {
                return Err(E::from(StoreError::MigrationRequired));
            }

            let mut next = current.clone();
            let result = mutation(&mut next, OffsetDateTime::now_utc())?;
            if next.revision != expected_revision + 1 {
                return Err(E::from(StoreError::InvalidHistory));
            }
            validate_baton(&next).map_err(E::from)?;
            validate_history_edge(&current, &next).map_err(E::from)?;
            self.write_history(&next).map_err(E::from)?;
            self.install(&next).map_err(E::from)?;
            Ok(result)
        })
    }

    pub(crate) fn upgrade_legacy(&self, expected_revision: u64) -> Result<RunBaton, StoreError> {
        self.with_lock(|| {
            let current = self.read()?;
            if current.revision != expected_revision {
                return Err(StoreError::RevisionConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            if self.read_history_revision(expected_revision)? != current {
                return Err(StoreError::InvalidHistory);
            }
            let next = migrate_legacy_baton_at(&current, OffsetDateTime::now_utc())?;
            validate_baton(&next)?;
            validate_migration_edge(&current, &next)?;
            self.write_history(&next)?;
            self.install(&next)?;
            Ok(next)
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
            for revision in 0..=high {
                let baton = self.read_history_revision(revision)?;
                if revision == 0 && baton.schema == SCHEMA && !valid_v2_creation_root(&baton) {
                    return Err(StoreError::InvalidHistory);
                }
                if let Some(previous_baton) = previous.as_ref() {
                    validate_history_edge(previous_baton, &baton)
                        .map_err(|_| StoreError::InvalidHistory)?;
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
            let source = recovered.clone();
            recovered.revision = high + 1;
            recovered.participants.worker.claim = None;
            recovered.participants.reviewer.claim = None;
            recovered.recovery = Some(RecoveryProvenance {
                from_revision,
                previous_high_revision: high,
            });
            validate_baton(&recovered)?;
            validate_history_edge(&source, &recovered).map_err(|_| StoreError::InvalidHistory)?;
            self.write_history(&recovered)?;
            self.install(&recovered)?;
            Ok(recovered)
        })
    }

    fn baton_path(&self) -> PathBuf {
        self.directory.join("baton.json")
    }

    fn read_history_revision(&self, revision: u64) -> Result<RunBaton, StoreError> {
        let path = self
            .directory
            .join("history")
            .join(format!("{revision:020}.json"));
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::InvalidHistory)
            }
            Err(error) => return Err(StoreError::Io(error)),
        };
        let baton = decode_baton(&bytes).map_err(|_| StoreError::InvalidHistory)?;
        if baton.revision != revision {
            return Err(StoreError::InvalidHistory);
        }
        Ok(baton)
    }

    fn with_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.with_lock_error(operation)
    }

    fn with_lock_error<T, E>(&self, operation: impl FnOnce() -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join(".baton.lock"))
            .map_err(StoreError::from)
            .map_err(E::from)?;
        lock.lock_exclusive()
            .map_err(StoreError::from)
            .map_err(E::from)?;
        let result = operation();
        FileExt::unlock(&lock)
            .map_err(StoreError::from)
            .map_err(E::from)?;
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
    migrate_legacy_baton_at(current, OffsetDateTime::now_utc())
}

fn migrate_legacy_baton_at(
    current: &RunBaton,
    migrated_at: OffsetDateTime,
) -> Result<RunBaton, StoreError> {
    validate_baton(current)?;
    if current.schema != LEGACY_SCHEMA || current.objective.summary.trim().is_empty() {
        return Err(StoreError::InvalidSchemaTransition);
    }
    if matches!(current.status, Status::Done | Status::Abandoned) || current.terminal.is_some() {
        return Err(StoreError::TerminalState);
    }
    for claim in [
        current.participants.worker.claim.as_ref(),
        current.participants.reviewer.claim.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let expires_at = OffsetDateTime::parse(&claim.lease_expires_at, &Rfc3339)
            .map_err(|_| StoreError::InvalidLeaseTimestamp)?;
        if expires_at > migrated_at {
            return Err(StoreError::LegacyClaimLive);
        }
    }
    let migrated_at = migrated_at
        .format(&Rfc3339)
        .map_err(|_| StoreError::InvalidLeaseTimestamp)?;
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
    next.checkpoint_history.clear();
    next.review = None;
    next.pending_checkpoint_supersession = None;
    next.publication = None;
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
        migrated_at,
        legacy_state_digest,
        legacy_checkpoint: current.checkpoint.clone(),
    });
    Ok(next)
}

fn decode_baton(bytes: &[u8]) -> Result<RunBaton, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let envelope: SchemaEnvelope = serde_json::from_value(value.clone())?;
    validate_supported_schema(&envelope.schema)?;
    let publication_present = value
        .as_object()
        .is_some_and(|object| object.contains_key("publication"));
    let mut baton: RunBaton = serde_json::from_value(value)?;
    match envelope.schema.as_str() {
        LEGACY_SCHEMA if !publication_present => {
            baton.publication = Some(LegacyPublication::default());
        }
        LEGACY_SCHEMA if baton.publication.is_none() => {
            return Err(StoreError::InvalidBaton(envelope.schema));
        }
        SCHEMA if publication_present => {
            return Err(StoreError::InvalidBaton(envelope.schema));
        }
        _ => {}
    }
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
            || !baton.checkpoint_history.is_empty()
            || baton.pending_checkpoint_supersession.is_some()
            || baton.migration.is_some()
            || baton.publication.is_none()
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
        || baton.publication.is_some()
        || binding.obligation.scope_revision != baton.scope_revision
        || binding.obligation.handoff_revision > baton.revision
        || binding
            .obligation
            .checkpoint
            .as_ref()
            .is_some_and(|checkpoint| !baton.checkpoint_history.contains(checkpoint))
        || !valid_checkpoint_state(baton)
        || !valid_publication_binding(binding)
        || baton
            .participants
            .worker
            .claim
            .as_ref()
            .is_some_and(|claim| !valid_participant_claim(claim))
        || baton
            .participants
            .reviewer
            .claim
            .as_ref()
            .is_some_and(|claim| !valid_participant_claim(claim))
    {
        return Err(StoreError::InvalidBaton(baton.schema.clone()));
    }
    if let Some(migration) = baton.migration.as_ref() {
        if migration.from_schema != LEGACY_SCHEMA
            || migration.from_revision >= baton.revision
            || OffsetDateTime::parse(&migration.migrated_at, &Rfc3339)
                .ok()
                .and_then(|timestamp| timestamp.format(&Rfc3339).ok())
                .as_deref()
                != Some(migration.migrated_at.as_str())
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

fn valid_v2_creation_root(baton: &RunBaton) -> bool {
    baton.schema == SCHEMA
        && baton.revision == 0
        && baton.status == Status::Working
        && baton.assignee == Assignee::Worker
        && baton.participants.worker.claim.is_none()
        && baton.participants.reviewer.claim.is_none()
        && baton.scope_revision == 0
        && baton.checkpoint.is_none()
        && baton.checkpoint_history.is_empty()
        && baton.review.is_none()
        && baton.pending_checkpoint_supersession.is_none()
        && baton.publication.is_none()
        && baton.publication_policy.as_ref() == Some(&PublicationPolicy::fixed())
        && baton.publication_binding
            == Some(create_handoff_obligation(HandoffKind::RunStarted, 0, 0))
        && baton.human_decision.is_none()
        && baton.terminal.is_none()
        && baton.recovery.is_none()
        && baton.migration.is_none()
}

fn valid_checkpoint_state(baton: &RunBaton) -> bool {
    let mut identities = std::collections::HashSet::new();
    if baton.checkpoint_history.iter().any(|binding| {
        binding.checkpoint_identity.trim() != binding.checkpoint_identity
            || binding.checkpoint_identity.is_empty()
            || !valid_sha256(&binding.manifest_digest)
            || binding.scope_revision > baton.scope_revision
            || !identities.insert(binding.checkpoint_identity.as_str())
    }) {
        return false;
    }
    let Some(checkpoint) = baton.checkpoint.as_ref() else {
        return baton.review.is_none() && baton.pending_checkpoint_supersession.is_none();
    };
    let binding = checkpoint.binding();
    if checkpoint.scope_revision != baton.scope_revision
        || !valid_checkpoint(checkpoint, baton)
        || !baton.checkpoint_history.contains(&binding)
        || baton
            .review
            .as_ref()
            .is_some_and(|review| review.binding() != binding)
        || baton
            .review
            .as_ref()
            .is_some_and(|review| !valid_review(review, baton))
        || baton
            .pending_checkpoint_supersession
            .as_ref()
            .is_some_and(|pending| {
                pending.reason.trim().is_empty()
                    || pending.reason.trim() != pending.reason
                    || pending.checkpoint != binding
            })
    {
        return false;
    }
    true
}

fn valid_review(review: &crate::model::ReviewReceipt, baton: &RunBaton) -> bool {
    let findings_are_normalized = review
        .findings
        .iter()
        .all(|finding| !finding.is_empty() && finding.trim() == finding);
    match review.verdict.as_str() {
        "approved" => review.findings.is_empty() && baton.pending_checkpoint_supersession.is_none(),
        "changes_requested" => !review.findings.is_empty() && findings_are_normalized,
        _ => false,
    }
}

fn valid_publication_binding(binding: &crate::model::PublicationBinding) -> bool {
    if binding
        .site_id
        .as_ref()
        .is_some_and(|site_id| !valid_exact_reference(site_id))
    {
        return false;
    }
    let Some(deployment) = binding.deployment.as_ref() else {
        return binding.review.is_none();
    };
    if binding.site_id.as_ref() != Some(&deployment.site_id)
        || deployment.obligation != binding.obligation
        || !valid_sha256(&deployment.source_digest)
        || !valid_exact_reference(&deployment.site_id)
        || !valid_exact_reference(&deployment.site_version)
        || !valid_exact_reference(&deployment.url)
        || deployment.channel != EXPLAINER_CHANNEL
        || deployment.access != EXPLAINER_ACCESS
        || deployment.publisher_harness != EXPLAINER_PUBLISHER_HARNESS
    {
        return false;
    }
    let Some(review) = binding.review.as_ref() else {
        return true;
    };
    let findings_are_normalized = review
        .findings
        .iter()
        .all(|finding| valid_exact_reference(finding));
    review.obligation == binding.obligation
        && review.source_digest == deployment.source_digest
        && review.site_id == deployment.site_id
        && review.site_version == deployment.site_version
        && review.url == deployment.url
        && review.reviewer_harness == EXPLAINER_REVIEWER_HARNESS
        && match review.verdict.as_str() {
            "approved" => review.findings.is_empty(),
            "changes_requested" => !review.findings.is_empty() && findings_are_normalized,
            _ => false,
        }
}

fn valid_checkpoint(checkpoint: &Checkpoint, baton: &RunBaton) -> bool {
    if checkpoint.kind.trim().is_empty()
        || checkpoint.kind.trim() != checkpoint.kind
        || checkpoint.identity.trim().is_empty()
        || checkpoint.identity.trim() != checkpoint.identity
        || checkpoint.verification.is_empty()
        || checkpoint
            .verification
            .iter()
            .any(|item| item.trim().is_empty() || item.trim() != item)
        || checkpoint.deliverables.is_empty()
        || !checkpoint
            .deliverables
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
        || checkpoint_manifest_digest(checkpoint) != checkpoint.manifest_digest
    {
        return false;
    }
    let required = baton
        .scope_deliverables
        .iter()
        .map(|deliverable| deliverable.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut submitted = std::collections::HashSet::new();
    checkpoint.deliverables.iter().all(|deliverable| {
        !deliverable.id.is_empty()
            && deliverable.id.trim() == deliverable.id
            && submitted.insert(deliverable.id.as_str())
            && !deliverable.artifacts.is_empty()
            && deliverable
                .artifacts
                .windows(2)
                .all(|pair| (&pair[0].kind, &pair[0].value) <= (&pair[1].kind, &pair[1].value))
            && deliverable.artifacts.iter().all(|artifact| {
                !artifact.kind.is_empty()
                    && artifact.kind.trim() == artifact.kind
                    && !artifact.value.is_empty()
                    && artifact.value.trim() == artifact.value
            })
    }) && submitted == required
}

fn validate_migration_edge(current: &RunBaton, next: &RunBaton) -> Result<(), StoreError> {
    let migrated_at = next
        .migration
        .as_ref()
        .ok_or(StoreError::InvalidSchemaTransition)
        .and_then(|migration| {
            OffsetDateTime::parse(&migration.migrated_at, &Rfc3339)
                .map_err(|_| StoreError::InvalidSchemaTransition)
        })?;
    if migrate_legacy_baton_at(current, migrated_at)? == *next {
        Ok(())
    } else {
        Err(StoreError::InvalidSchemaTransition)
    }
}

fn validate_history_edge(current: &RunBaton, next: &RunBaton) -> Result<(), StoreError> {
    if next.revision != current.revision + 1 || next.run_id != current.run_id {
        return Err(StoreError::InvalidHistory);
    }
    match (current.schema.as_str(), next.schema.as_str()) {
        (SCHEMA, SCHEMA) if valid_v2_history_edge(current, next) => Ok(()),
        (SCHEMA, SCHEMA) => Err(StoreError::InvalidHistory),
        (LEGACY_SCHEMA, LEGACY_SCHEMA) => Ok(()),
        (LEGACY_SCHEMA, SCHEMA) => validate_migration_edge(current, next),
        _ => Err(StoreError::InvalidSchemaTransition),
    }
}

fn valid_v2_history_edge(current: &RunBaton, next: &RunBaton) -> bool {
    next.scope_revision >= current.scope_revision
        && next
            .checkpoint_history
            .starts_with(&current.checkpoint_history)
        && valid_v2_edge_kind(current, next)
}

fn valid_v2_edge_kind(current: &RunBaton, next: &RunBaton) -> bool {
    let (Some(current_binding), Some(next_binding)) = (
        current.publication_binding.as_ref(),
        next.publication_binding.as_ref(),
    ) else {
        return false;
    };
    if current_binding
        .site_id
        .as_ref()
        .is_some_and(|site_id| next_binding.site_id.as_ref() != Some(site_id))
    {
        return false;
    }
    if current_binding.obligation != next_binding.obligation {
        return current_binding.site_id == next_binding.site_id
            && next_binding.deployment.is_none()
            && next_binding.review.is_none()
            && valid_new_obligation(current, next, next_binding);
    }
    if current_binding != next_binding {
        return valid_publication_receipt_edge(current, next, current_binding, next_binding);
    }
    valid_claim_edge(current, next)
        || valid_human_decision_request_edge(current, next)
        || valid_plain_human_decision_resume_edge(current, next)
        || valid_checkpoint_supersession_request_edge(current, next)
        || valid_finalize_edge(current, next, current_binding)
        || valid_abandon_edge(current, next)
        || valid_recovery_successor_edge(current, next)
}

fn valid_publication_receipt_edge(
    current: &RunBaton,
    next: &RunBaton,
    current_binding: &PublicationBinding,
    next_binding: &PublicationBinding,
) -> bool {
    if is_terminal(current)
        || !only_fields_changed(current, next, |expected| {
            expected.publication_binding = next.publication_binding.clone();
        })
    {
        return false;
    }
    if current_binding.deployment != next_binding.deployment {
        return next_binding.deployment.is_some() && next_binding.review.is_none();
    }
    current_binding.site_id == next_binding.site_id
        && current_binding.review != next_binding.review
        && next_binding.review.is_some()
}

fn valid_claim_edge(current: &RunBaton, next: &RunBaton) -> bool {
    if is_terminal(current) {
        return false;
    }
    let worker_changed = current.participants.worker.claim != next.participants.worker.claim;
    let reviewer_changed = current.participants.reviewer.claim != next.participants.reviewer.claim;
    if worker_changed == reviewer_changed {
        return false;
    }
    if worker_changed {
        valid_claim_mutation(
            current.participants.worker.claim.as_ref(),
            next.participants.worker.claim.as_ref(),
        ) && only_fields_changed(current, next, |expected| {
            expected.participants.worker.claim = next.participants.worker.claim.clone();
        })
    } else {
        valid_claim_mutation(
            current.participants.reviewer.claim.as_ref(),
            next.participants.reviewer.claim.as_ref(),
        ) && only_fields_changed(current, next, |expected| {
            expected.participants.reviewer.claim = next.participants.reviewer.claim.clone();
        })
    }
}

fn valid_claim_mutation(
    current: Option<&ParticipantClaim>,
    next: Option<&ParticipantClaim>,
) -> bool {
    let Some(next) = next.filter(|claim| valid_participant_claim(claim)) else {
        return false;
    };
    match current {
        None => next.epoch == 1,
        Some(current)
            if next.session_id == current.session_id
                && next.epoch == current.epoch
                && next.token_digest == current.token_digest =>
        {
            let Some((current_start, current_expiry)) = claim_times(current) else {
                return false;
            };
            let Some((next_start, _)) = claim_times(next) else {
                return false;
            };
            next_start >= current_start && next_start < current_expiry
        }
        Some(current) => {
            let Some((_, current_expiry)) = claim_times(current) else {
                return false;
            };
            let Some((next_start, _)) = claim_times(next) else {
                return false;
            };
            current.epoch.checked_add(1) == Some(next.epoch)
                && next.token_digest != current.token_digest
                && next_start >= current_expiry
        }
    }
}

fn valid_participant_claim(claim: &ParticipantClaim) -> bool {
    let Some((started_at, expires_at)) = claim_times(claim) else {
        return false;
    };
    !claim.session_id.trim().is_empty()
        && valid_sha256(&claim.token_digest)
        && claim.lease_seconds > 0
        && claim.lease_seconds <= i64::MAX as u64
        && started_at.checked_add(Duration::seconds(claim.lease_seconds as i64)) == Some(expires_at)
}

fn claim_times(claim: &ParticipantClaim) -> Option<(OffsetDateTime, OffsetDateTime)> {
    let started_value = claim.lease_started_at.as_ref()?;
    let started_at = OffsetDateTime::parse(started_value, &Rfc3339).ok()?;
    let expires_at = OffsetDateTime::parse(&claim.lease_expires_at, &Rfc3339).ok()?;
    (started_at.format(&Rfc3339).ok()?.as_str() == started_value
        && expires_at.format(&Rfc3339).ok()?.as_str() == claim.lease_expires_at)
        .then_some((started_at, expires_at))
}

fn valid_human_decision_request_edge(current: &RunBaton, next: &RunBaton) -> bool {
    let Some(decision) = next.human_decision.as_ref() else {
        return false;
    };
    if is_terminal(current)
        || current
            .human_decision
            .as_ref()
            .is_some_and(|decision| decision.answer.is_none())
        || decision.question.trim().is_empty()
        || decision.evidence.is_empty()
        || decision
            .evidence
            .iter()
            .any(|evidence| evidence.trim().is_empty())
        || decision.options.len() < 2
        || decision
            .options
            .iter()
            .any(|option| option.trim().is_empty())
        || !valid_role_name(&decision.requested_by)
        || !valid_role_name(&decision.contact_role)
        || decision.answer.is_some()
        || !valid_resume_target(&decision.resume_status, &decision.resume_assignee)
        || next.status != Status::HumanDecision
        || next.assignee != Assignee::Human
    {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.human_decision = next.human_decision.clone();
        expected.status = Status::HumanDecision;
        expected.assignee = Assignee::Human;
    })
}

fn valid_plain_human_decision_resume_edge(current: &RunBaton, next: &RunBaton) -> bool {
    let (Some(current_decision), Some(next_decision)) = (
        current.human_decision.as_ref(),
        next.human_decision.as_ref(),
    ) else {
        return false;
    };
    let mut expected_decision = current_decision.clone();
    expected_decision.answer = next_decision.answer.clone();
    if current.status != Status::HumanDecision
        || current.assignee != Assignee::Human
        || current_decision.answer.is_some()
        || !valid_role_name(&current_decision.contact_role)
        || !next_decision
            .answer
            .as_ref()
            .is_some_and(|answer| valid_exact_reference(answer))
        || expected_decision != *next_decision
        || !valid_resume_target(
            &current_decision.resume_status,
            &current_decision.resume_assignee,
        )
        || next.status != current_decision.resume_status
        || next.assignee != current_decision.resume_assignee
    {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.human_decision = next.human_decision.clone();
        expected.status = next.status.clone();
        expected.assignee = next.assignee.clone();
    })
}

fn valid_checkpoint_supersession_request_edge(current: &RunBaton, next: &RunBaton) -> bool {
    let (Some(checkpoint), Some(pending)) = (
        current.checkpoint.as_ref(),
        next.pending_checkpoint_supersession.as_ref(),
    ) else {
        return false;
    };
    if current.status != Status::Reviewing
        || current.assignee != Assignee::Reviewer
        || current.pending_checkpoint_supersession.is_some()
        || !valid_exact_reference(&pending.reason)
        || pending.checkpoint != checkpoint.binding()
    {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.pending_checkpoint_supersession = next.pending_checkpoint_supersession.clone();
    })
}

fn valid_finalize_edge(current: &RunBaton, next: &RunBaton, binding: &PublicationBinding) -> bool {
    let (Some(checkpoint), Some(review)) = (current.checkpoint.as_ref(), current.review.as_ref())
    else {
        return false;
    };
    let checkpoint = checkpoint.binding();
    if current.status != Status::Finalizing
        || current.assignee != Assignee::Worker
        || current.pending_checkpoint_supersession.is_some()
        || review.verdict != "approved"
        || review.binding() != checkpoint
        || !approved_publication_gate(binding, Some((&HandoffKind::ReviewerToWorker, &checkpoint)))
        || next.status != Status::Done
        || next.assignee != Assignee::None
        || next.terminal
            != Some(TerminalProvenance {
                outcome: "done".to_owned(),
                reason: None,
            })
    {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.status = Status::Done;
        expected.assignee = Assignee::None;
        expected.terminal = next.terminal.clone();
    })
}

fn valid_abandon_edge(current: &RunBaton, next: &RunBaton) -> bool {
    let Some(terminal) = next.terminal.as_ref() else {
        return false;
    };
    if is_terminal(current)
        || next.status != Status::Abandoned
        || next.assignee != Assignee::None
        || terminal.outcome != "abandoned"
        || !terminal
            .reason
            .as_ref()
            .is_some_and(|reason| !reason.trim().is_empty())
    {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.status = Status::Abandoned;
        expected.assignee = Assignee::None;
        expected.terminal = next.terminal.clone();
    })
}

fn valid_recovery_successor_edge(current: &RunBaton, next: &RunBaton) -> bool {
    let expected_recovery = RecoveryProvenance {
        from_revision: current.revision,
        previous_high_revision: current.revision,
    };
    if is_terminal(current) || next.recovery.as_ref() != Some(&expected_recovery) {
        return false;
    }
    only_fields_changed(current, next, |expected| {
        expected.participants.worker.claim = None;
        expected.participants.reviewer.claim = None;
        expected.recovery = Some(expected_recovery);
    })
}

fn only_fields_changed(
    current: &RunBaton,
    next: &RunBaton,
    update_expected: impl FnOnce(&mut RunBaton),
) -> bool {
    let mut expected = current.clone();
    expected.revision = next.revision;
    update_expected(&mut expected);
    expected == *next
}

fn valid_role_name(value: &str) -> bool {
    matches!(value, "worker" | "reviewer")
}

fn valid_resume_target(status: &Status, assignee: &Assignee) -> bool {
    matches!(
        (status, assignee),
        (
            Status::Working | Status::Revising | Status::Finalizing,
            Assignee::Worker
        ) | (Status::Reviewing, Assignee::Reviewer)
    )
}

fn is_terminal(baton: &RunBaton) -> bool {
    matches!(baton.status, Status::Done | Status::Abandoned)
}

fn valid_new_obligation(current: &RunBaton, next: &RunBaton, binding: &PublicationBinding) -> bool {
    if binding.obligation.handoff_revision != next.revision {
        return false;
    }
    match binding.obligation.kind {
        HandoffKind::WorkerToReviewer => valid_worker_to_reviewer(current, next, binding),
        HandoffKind::ReviewerToWorker => valid_reviewer_to_worker(current, next, binding),
        HandoffKind::ScopeAmended => valid_scope_amended(current, next, binding),
        HandoffKind::CheckpointSuperseded => valid_checkpoint_superseded(current, next, binding),
        HandoffKind::ApprovalWithdrawn => valid_approval_withdrawn(current, next, binding),
        HandoffKind::RunStarted | HandoffKind::ProtocolUpgraded => false,
    }
}

fn valid_worker_to_reviewer(
    current: &RunBaton,
    next: &RunBaton,
    binding: &PublicationBinding,
) -> bool {
    let Some(checkpoint) = next.checkpoint.as_ref() else {
        return false;
    };
    let checkpoint = checkpoint.binding();
    if !current
        .publication_binding
        .as_ref()
        .is_some_and(|current_binding| approved_publication_gate(current_binding, None))
        || !matches!(current.status, Status::Working | Status::Revising)
        || current.assignee != Assignee::Worker
        || next.status != Status::Reviewing
        || next.assignee != Assignee::Reviewer
        || next.scope_revision != current.scope_revision
        || binding.obligation.checkpoint.as_ref() != Some(&checkpoint)
        || next.checkpoint_history.len() != current.checkpoint_history.len() + 1
        || next.checkpoint_history.last() != Some(&checkpoint)
    {
        return false;
    }
    let mut expected = current.clone();
    expected.revision = next.revision;
    expected.status = Status::Reviewing;
    expected.assignee = Assignee::Reviewer;
    expected.checkpoint = next.checkpoint.clone();
    expected.checkpoint_history = next.checkpoint_history.clone();
    expected.review = None;
    expected.pending_checkpoint_supersession = None;
    expected.publication_binding = next.publication_binding.clone();
    expected == *next
}

fn valid_reviewer_to_worker(
    current: &RunBaton,
    next: &RunBaton,
    binding: &PublicationBinding,
) -> bool {
    let (Some(checkpoint), Some(review)) = (current.checkpoint.as_ref(), next.review.as_ref())
    else {
        return false;
    };
    let checkpoint = checkpoint.binding();
    let expected_gate = Some((&HandoffKind::WorkerToReviewer, &checkpoint));
    let valid_verdict = match review.verdict.as_str() {
        "changes_requested" => next.status == Status::Revising,
        "approved" => {
            next.status == Status::Finalizing && current.pending_checkpoint_supersession.is_none()
        }
        _ => false,
    };
    if !current
        .publication_binding
        .as_ref()
        .is_some_and(|current_binding| approved_publication_gate(current_binding, expected_gate))
        || current.status != Status::Reviewing
        || current.assignee != Assignee::Reviewer
        || next.assignee != Assignee::Worker
        || !valid_verdict
        || next.scope_revision != current.scope_revision
        || next.checkpoint != current.checkpoint
        || next.checkpoint_history != current.checkpoint_history
        || review.binding() != checkpoint
        || binding.obligation.checkpoint.as_ref() != Some(&checkpoint)
    {
        return false;
    }
    let mut expected = current.clone();
    expected.revision = next.revision;
    expected.status = next.status.clone();
    expected.assignee = Assignee::Worker;
    expected.review = next.review.clone();
    expected.pending_checkpoint_supersession = None;
    expected.publication_binding = next.publication_binding.clone();
    expected == *next
}

fn valid_scope_amended(current: &RunBaton, next: &RunBaton, binding: &PublicationBinding) -> bool {
    let (Some(current_decision), Some(next_decision)) = (
        current.human_decision.as_ref(),
        next.human_decision.as_ref(),
    ) else {
        return false;
    };
    let mut expected_decision = current_decision.clone();
    expected_decision.answer = next_decision.answer.clone();
    let valid_scope = valid_exact_reference(&next.objective.summary)
        && next.objective.refs.iter().all(|reference| {
            valid_exact_reference(&reference.kind) && valid_exact_reference(&reference.value)
        })
        && next.task.as_ref().is_none_or(|task| {
            task.summary == next.objective.summary
                && task
                    .reference
                    .as_ref()
                    .is_none_or(|reference| valid_exact_reference(reference))
        });
    if current.status != Status::HumanDecision
        || current.assignee != Assignee::Human
        || current_decision.answer.is_some()
        || !next_decision
            .answer
            .as_ref()
            .is_some_and(|answer| valid_exact_reference(answer))
        || expected_decision != *next_decision
        || !valid_scope
        || next.scope_revision != current.scope_revision + 1
        || next.status != Status::Revising
        || next.assignee != Assignee::Worker
        || binding.obligation.checkpoint.is_some()
        || current.task.is_none() != next.task.is_none()
    {
        return false;
    }
    let mut expected = current.clone();
    expected.revision = next.revision;
    expected.objective = next.objective.clone();
    expected.task = next.task.clone();
    expected.scope_revision = next.scope_revision;
    expected.scope_deliverables = next.scope_deliverables.clone();
    expected.checkpoint = None;
    expected.review = None;
    expected.pending_checkpoint_supersession = None;
    expected.status = Status::Revising;
    expected.assignee = Assignee::Worker;
    expected.publication_binding = next.publication_binding.clone();
    expected.human_decision = next.human_decision.clone();
    expected == *next
}

fn valid_checkpoint_superseded(
    current: &RunBaton,
    next: &RunBaton,
    binding: &PublicationBinding,
) -> bool {
    let (Some(checkpoint), Some(pending)) = (
        current.checkpoint.as_ref(),
        current.pending_checkpoint_supersession.as_ref(),
    ) else {
        return false;
    };
    let checkpoint = checkpoint.binding();
    if current.status != Status::Reviewing
        || current.assignee != Assignee::Reviewer
        || pending.checkpoint != checkpoint
        || next.status != Status::Revising
        || next.assignee != Assignee::Worker
        || next.scope_revision != current.scope_revision
        || binding.obligation.checkpoint.as_ref() != Some(&checkpoint)
    {
        return false;
    }
    expected_checkpoint_clearing_transition(current, next)
}

fn valid_approval_withdrawn(
    current: &RunBaton,
    next: &RunBaton,
    binding: &PublicationBinding,
) -> bool {
    let (Some(checkpoint), Some(review)) = (current.checkpoint.as_ref(), current.review.as_ref())
    else {
        return false;
    };
    let checkpoint = checkpoint.binding();
    if current.status != Status::Finalizing
        || current.assignee != Assignee::Worker
        || review.verdict != "approved"
        || review.binding() != checkpoint
        || next.status != Status::Revising
        || next.assignee != Assignee::Worker
        || next.scope_revision != current.scope_revision
        || binding.obligation.checkpoint.as_ref() != Some(&checkpoint)
    {
        return false;
    }
    expected_checkpoint_clearing_transition(current, next)
}

fn expected_checkpoint_clearing_transition(current: &RunBaton, next: &RunBaton) -> bool {
    let mut expected = current.clone();
    expected.revision = next.revision;
    expected.status = Status::Revising;
    expected.assignee = Assignee::Worker;
    expected.checkpoint = None;
    expected.review = None;
    expected.pending_checkpoint_supersession = None;
    expected.publication_binding = next.publication_binding.clone();
    expected == *next
}

fn approved_publication_gate(
    binding: &PublicationBinding,
    expected: Option<(&HandoffKind, &crate::model::CheckpointBinding)>,
) -> bool {
    expected.is_none_or(|(kind, checkpoint)| {
        &binding.obligation.kind == kind
            && binding.obligation.checkpoint.as_ref() == Some(checkpoint)
    }) && binding
        .review
        .as_ref()
        .is_some_and(|review| review.verdict == "approved" && review.findings.is_empty())
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
