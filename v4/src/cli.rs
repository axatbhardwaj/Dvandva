use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;
use thiserror::Error;

use crate::action::Action;
use crate::claim::{self, ClaimError, Role};
use crate::discovery::{self, DiscoveryError, DiscoveryQuery};
use crate::identity::{self, IdentityError};
use crate::model::{
    normalize_participants, DeliverableRequirement, ExternalRef, ModelError, RunBaton,
    TaskIdentity, WorkspaceIdentity, LEGACY_SCHEMA, ROLE_API, SCHEMA,
};
use crate::role_session::{self, RoleSessionError, RoleStartRequest, UpgradeError};
use crate::store::{RunChannel, StoreError};
use crate::transition::{self, TransitionError};
use crate::wait::{self, WaitError};

#[derive(Debug, Parser)]
#[command(name = "dvandva-v4", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Probe {
        #[arg(long)]
        expected_schema: String,
        #[arg(long)]
        expected_role_api: u32,
    },
    Identify {
        #[arg(long)]
        workspace: PathBuf,
    },
    Discover {
        #[arg(long)]
        runs_dir: PathBuf,
        #[arg(long)]
        repository_id: String,
        #[arg(long, alias = "harness")]
        reviewer_harness: String,
        #[arg(long, default_value = "reviewer")]
        role: Role,
        #[arg(long)]
        task_reference: Option<String>,
        #[arg(long)]
        objective: Option<String>,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        stale_after_days: Option<u64>,
    },
    DiscoverWait {
        #[arg(long)]
        runs_dir: PathBuf,
        #[arg(long)]
        repository_id: String,
        #[arg(long, alias = "harness")]
        reviewer_harness: String,
        #[arg(long, default_value = "reviewer")]
        role: Role,
        #[arg(long)]
        task_reference: Option<String>,
        #[arg(long)]
        objective: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        stale_after_days: Option<u64>,
        #[arg(long, default_value_t = 1000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 300_000)]
        timeout_ms: u64,
        #[arg(long)]
        poll_only: bool,
    },
    Role {
        #[command(subcommand)]
        command: RoleCommand,
    },
    /// Report, and optionally archive, unclaimed runs whose head has not moved
    /// for `--older-than-days`. Archiving moves a run aside; it never deletes.
    RunsGc {
        #[arg(long)]
        runs_dir: PathBuf,
        #[arg(long, default_value_t = 14)]
        older_than_days: u64,
        #[arg(long)]
        archive: bool,
    },
    Init {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        objective: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        repository_id: String,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        worktree: Option<String>,
        #[arg(long)]
        task_reference: Option<String>,
        #[arg(long = "required-deliverable")]
        required_deliverables: Vec<String>,
    },
    Read {
        #[arg(long)]
        run_dir: PathBuf,
    },
    Claim {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
    },
    Reclaim {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
    },
    Heartbeat {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
    },
    Apply {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        action: PathBuf,
    },
    Wait {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        after_revision: u64,
        #[arg(long, default_value_t = 1000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 300_000)]
        timeout_ms: u64,
    },
    Recover {
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        from_revision: u64,
    },
}

#[derive(Debug, Subcommand)]
enum RoleCommand {
    Start {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        runs_dir: PathBuf,
        #[arg(long)]
        credentials_root: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        current_harness: String,
        #[arg(long)]
        peer_harness: String,
        #[arg(long)]
        objective: Option<String>,
        #[arg(long = "objective-ref")]
        objective_refs: Vec<String>,
        #[arg(long)]
        task_reference: Option<String>,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long, default_value_t = 300)]
        lease_seconds: u64,
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value_t = 1000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 300_000)]
        timeout_ms: u64,
        #[arg(long)]
        new_run: bool,
        #[arg(long = "required-deliverable")]
        required_deliverables: Vec<String>,
    },
    Claim {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        credentials_root: PathBuf,
    },
    Reclaim {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        credentials_root: PathBuf,
    },
    Apply {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        credentials_root: PathBuf,
        #[arg(long)]
        action: PathBuf,
    },
    Read {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        credentials_root: PathBuf,
    },
    Heartbeat {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        lease_seconds: u64,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        credentials_root: PathBuf,
    },
    Wait {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        credentials_root: PathBuf,
        #[arg(long)]
        after_revision: u64,
        #[arg(long, default_value_t = 1000)]
        poll_interval_ms: u64,
        #[arg(long, default_value_t = 300_000)]
        timeout_ms: u64,
    },
    Upgrade {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        current_harness: String,
        #[arg(long)]
        peer_harness: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        credentials_root: PathBuf,
    },
    /// Replace an unreadable publication policy with the canonical channel both
    /// harnesses can read.
    RepairPolicy {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        current_harness: String,
        #[arg(long)]
        peer_harness: String,
        #[arg(long)]
        expected_revision: u64,
    },
    /// Materialize the bytes behind a staged analysis checkpoint artifact.
    Analysis {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        credentials_root: PathBuf,
        #[arg(long)]
        digest: String,
    },
    /// Read the digest-bound explainer bytes staged for the current obligation.
    Explainer {
        #[arg(long)]
        api: u32,
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long)]
        role: Role,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        credentials_root: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("invalid baton JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Claim(#[from] ClaimError),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error(transparent)]
    Wait(#[from] WaitError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    RoleSession(#[from] RoleSessionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Upgrade(#[from] UpgradeError),
}

#[derive(Serialize)]
struct Diagnostic<'a> {
    error: &'a str,
    message: String,
}

#[derive(Serialize)]
struct Probe<'a> {
    package: &'a str,
    version: &'a str,
    publish: bool,
    write_schema: &'a str,
    read_schemas: [&'a str; 2],
    role_api: u32,
    capabilities: ProbeCapabilities,
    compatible: bool,
}

#[derive(Serialize)]
struct ProbeCapabilities {
    upgrade_from_v1: bool,
}

pub fn run() -> Result<(), CliError> {
    match Cli::parse().command {
        Command::Probe {
            expected_schema,
            expected_role_api,
        } => {
            let compatible = expected_schema == SCHEMA && expected_role_api == ROLE_API;
            let probe = Probe {
                package: env!("CARGO_PKG_NAME"),
                version: env!("CARGO_PKG_VERSION"),
                publish: false,
                write_schema: SCHEMA,
                read_schemas: [SCHEMA, LEGACY_SCHEMA],
                role_api: ROLE_API,
                capabilities: ProbeCapabilities {
                    upgrade_from_v1: true,
                },
                compatible,
            };
            println!("{}", serde_json::to_string_pretty(&probe)?);
            if compatible {
                Ok(())
            } else {
                Err(CliError::Invalid(
                    "kernel compatibility mismatch".to_owned(),
                ))
            }
        }
        Command::RunsGc {
            runs_dir,
            older_than_days,
            archive,
        } => {
            let stale = discovery::stale_runs(&runs_dir, older_than_days)?;
            let mut archived = Vec::new();
            let mut skipped = Vec::new();
            if archive {
                for run in &stale {
                    match discovery::archive_stale_run(&runs_dir, &run.run_dir, older_than_days)? {
                        Some(destination) => archived.push(destination),
                        // Came back to life, or the archive root was unsafe.
                        None => skipped.push(run.run_id.clone()),
                    }
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "older_than_days": older_than_days,
                    "stale": stale,
                    "archived": archived,
                    "skipped": skipped,
                }))?
            );
            Ok(())
        }
        Command::Identify { workspace } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&identity::identify(&workspace)?)?
            );
            Ok(())
        }
        Command::Discover {
            runs_dir,
            repository_id,
            reviewer_harness,
            role,
            task_reference,
            objective,
            run_id,
            session_id,
            stale_after_days,
        } => {
            let outcome = discovery::discover(
                &runs_dir,
                DiscoveryQuery {
                    repository_id: &repository_id,
                    role,
                    participant_harness: &reviewer_harness,
                    task_reference: task_reference.as_deref(),
                    objective: objective.as_deref(),
                    run_id: run_id.as_deref(),
                    session_id: session_id.as_deref(),
                    stale_after_days: stale_after_days.filter(|days| *days > 0),
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::DiscoverWait {
            runs_dir,
            repository_id,
            reviewer_harness,
            role,
            task_reference,
            objective,
            session_id,
            stale_after_days,
            poll_interval_ms,
            timeout_ms,
            poll_only,
        } => {
            let outcome = discovery::wait_for_match(
                &runs_dir,
                DiscoveryQuery {
                    repository_id: &repository_id,
                    role,
                    participant_harness: &reviewer_harness,
                    task_reference: task_reference.as_deref(),
                    objective: objective.as_deref(),
                    run_id: None,
                    session_id: session_id.as_deref(),
                    stale_after_days: stale_after_days.filter(|days| *days > 0),
                },
                std::time::Duration::from_millis(poll_interval_ms.max(1)),
                std::time::Duration::from_millis(timeout_ms),
                !poll_only,
            )?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Command::Role { command } => match command {
            RoleCommand::Start {
                api,
                workspace,
                runs_dir,
                credentials_root,
                role,
                session_id,
                current_harness,
                peer_harness,
                objective,
                objective_refs,
                task_reference,
                run_id,
                lease_seconds,
                wait,
                poll_interval_ms,
                timeout_ms,
                new_run,
                required_deliverables,
            } => {
                require_role_api(api)?;
                let required_deliverables = parse_deliverables(required_deliverables)?;
                let objective_refs = parse_objective_refs(objective_refs)?;
                let result = role_session::start(RoleStartRequest {
                    workspace: &workspace,
                    runs_dir: &runs_dir,
                    credentials_root: &credentials_root,
                    role,
                    session_id: &session_id,
                    current_harness: &current_harness,
                    peer_harness: &peer_harness,
                    objective: objective.as_deref(),
                    objective_refs: &objective_refs,
                    task_reference: task_reference.as_deref(),
                    run_id: run_id.as_deref(),
                    lease_seconds,
                    wait,
                    poll_interval: std::time::Duration::from_millis(poll_interval_ms.max(1)),
                    timeout: std::time::Duration::from_millis(timeout_ms),
                    new_run,
                    required_deliverables: &required_deliverables,
                })?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            RoleCommand::Claim {
                api,
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
                require_role_api(api)?;
                let result = role_session::claim(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    lease_seconds,
                    expected_revision,
                )?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            RoleCommand::Reclaim {
                api,
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
                require_role_api(api)?;
                let result = role_session::reclaim(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    lease_seconds,
                    expected_revision,
                )?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            RoleCommand::Apply {
                api,
                run_dir,
                role,
                session_id,
                expected_revision,
                credentials_root,
                action,
            } => {
                require_role_api(api)?;
                let action: Action =
                    serde_json::from_slice(&std::fs::read(action).map_err(StoreError::Io)?)?;
                let baton = role_session::apply(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    expected_revision,
                    action,
                )?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::Read {
                api,
                run_dir,
                role,
                session_id,
                credentials_root,
            } => {
                require_role_api(api)?;
                let baton = role_session::read(&run_dir, &credentials_root, role, &session_id)?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::Heartbeat {
                api,
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
                require_role_api(api)?;
                let revision = role_session::heartbeat(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    lease_seconds,
                    expected_revision,
                )?;
                println!(r#"{{"revision":{revision}}}"#);
                Ok(())
            }
            RoleCommand::Wait {
                api,
                run_dir,
                role,
                session_id,
                credentials_root,
                after_revision,
                poll_interval_ms,
                timeout_ms,
            } => {
                require_role_api(api)?;
                let baton = role_session::wait(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    after_revision,
                    std::time::Duration::from_millis(poll_interval_ms.max(1)),
                    std::time::Duration::from_millis(timeout_ms),
                )?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::Upgrade {
                api,
                run_dir,
                role,
                session_id,
                current_harness,
                peer_harness,
                expected_revision,
                credentials_root,
            } => {
                require_role_api(api)?;
                let baton = role_session::upgrade(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    &current_harness,
                    &peer_harness,
                    expected_revision,
                )?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::RepairPolicy {
                api,
                run_dir,
                role,
                session_id,
                current_harness,
                peer_harness,
                expected_revision,
            } => {
                require_role_api(api)?;
                let baton = role_session::repair_publication_policy(
                    &run_dir,
                    role,
                    &session_id,
                    &current_harness,
                    &peer_harness,
                    expected_revision,
                )?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::Analysis {
                api,
                run_dir,
                role,
                session_id,
                credentials_root,
                digest,
            } => {
                require_role_api(api)?;
                let staged = role_session::read_analysis(
                    &run_dir,
                    &credentials_root,
                    role,
                    &session_id,
                    &digest,
                )?;
                println!("{}", serde_json::to_string_pretty(&staged)?);
                Ok(())
            }
            RoleCommand::Explainer {
                api,
                run_dir,
                role,
                session_id,
                credentials_root,
            } => {
                require_role_api(api)?;
                let staged =
                    role_session::read_explainer(&run_dir, &credentials_root, role, &session_id)?;
                println!("{}", serde_json::to_string_pretty(&staged)?);
                Ok(())
            }
        },
        Command::Init {
            run_dir,
            run_id,
            objective,
            worker,
            reviewer,
            repository_id,
            origin,
            worktree,
            task_reference,
            required_deliverables,
        } => {
            validate_init(&run_id, &objective, &worker, &reviewer, &repository_id)?;
            validate_optional("origin", origin.as_deref())?;
            validate_optional("worktree", worktree.as_deref())?;
            validate_optional("task reference", task_reference.as_deref())?;
            let objective = objective.trim();
            let required_deliverables = parse_deliverables(required_deliverables)?;
            let baton = RunBaton::new(
                run_id,
                objective,
                worker.trim(),
                reviewer.trim(),
                required_deliverables,
            )?
            .with_discovery_identity(
                WorkspaceIdentity {
                    repository_id: repository_id.trim().to_owned(),
                    origin: trim_optional(origin),
                    worktree: trim_optional(worktree),
                },
                TaskIdentity {
                    reference: trim_optional(task_reference),
                    summary: objective.to_owned(),
                },
            );
            RunChannel::open(run_dir).create(&baton)?;
            Ok(())
        }
        Command::Read { run_dir } => {
            let baton = RunChannel::open(run_dir).read()?;
            println!("{}", serde_json::to_string_pretty(&baton)?);
            Ok(())
        }
        Command::Claim {
            run_dir,
            role,
            session_id,
            lease_seconds,
            expected_revision,
        } => {
            let grant = claim::claim(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                lease_seconds,
                expected_revision,
            )?;
            println!("{}", serde_json::to_string(&grant)?);
            Ok(())
        }
        Command::Reclaim {
            run_dir,
            role,
            session_id,
            lease_seconds,
            expected_revision,
        } => {
            let grant = claim::reclaim(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                lease_seconds,
                expected_revision,
            )?;
            println!("{}", serde_json::to_string(&grant)?);
            Ok(())
        }
        Command::Heartbeat {
            run_dir,
            role,
            session_id,
            token,
            lease_seconds,
            expected_revision,
        } => {
            let revision = claim::heartbeat(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                &token,
                lease_seconds,
                expected_revision,
            )?;
            println!(r#"{{"revision":{revision}}}"#);
            Ok(())
        }
        Command::Apply {
            run_dir,
            role,
            session_id,
            token,
            expected_revision,
            action,
        } => {
            let action: Action =
                serde_json::from_slice(&std::fs::read(action).map_err(StoreError::Io)?)?;
            let baton = transition::apply(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                &token,
                expected_revision,
                action,
            )?;
            println!("{}", serde_json::to_string_pretty(&baton)?);
            Ok(())
        }
        Command::Wait {
            run_dir,
            role,
            session_id,
            token,
            after_revision,
            poll_interval_ms,
            timeout_ms,
        } => {
            let (baton, outcome) = wait::wait(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                &token,
                after_revision,
                std::time::Duration::from_millis(poll_interval_ms.max(1)),
                std::time::Duration::from_millis(timeout_ms),
            )?;
            let mut rendered = serde_json::to_value(&baton)?;
            if let Some(object) = rendered.as_object_mut() {
                object.insert("wait_outcome".to_owned(), serde_json::to_value(outcome)?);
            }
            println!("{}", serde_json::to_string_pretty(&rendered)?);
            Ok(())
        }
        Command::Recover {
            run_dir,
            from_revision,
        } => {
            let baton = RunChannel::open(run_dir).recover(from_revision)?;
            println!("{}", serde_json::to_string_pretty(&baton)?);
            Ok(())
        }
    }
}

pub fn print_error(error: &CliError) {
    let code = match error {
        CliError::Invalid(_) => "invalid_input",
        CliError::Identity(error) | CliError::RoleSession(RoleSessionError::Identity(error)) => {
            identity_error_code(error)
        }
        CliError::Discovery(DiscoveryError::Io(_)) => "discovery_io",
        CliError::RoleSession(RoleSessionError::Discovery(DiscoveryError::Io(_))) => "discovery_io",
        CliError::Store(error) | CliError::RoleSession(RoleSessionError::Store(error)) => {
            store_error_code(error)
        }
        CliError::Claim(error) | CliError::RoleSession(RoleSessionError::Claim(error)) => {
            claim_error_code(error)
        }
        CliError::RoleSession(RoleSessionError::Credential(_)) => "credential_error",
        CliError::Model(_) | CliError::RoleSession(RoleSessionError::Model(_)) => "invalid_input",
        CliError::Upgrade(error) | CliError::RoleSession(RoleSessionError::Upgrade(error)) => {
            upgrade_error_code(error)
        }
        CliError::Transition(error)
        | CliError::RoleSession(RoleSessionError::Transition(error)) => {
            transition_error_code(error)
        }
        CliError::Wait(error) | CliError::RoleSession(RoleSessionError::Wait(error)) => {
            wait_error_code(error)
        }
        CliError::RoleSession(RoleSessionError::Invalid(_)) => "invalid_input",
        CliError::Json(_) => "invalid_baton",
    };
    let diagnostic = Diagnostic {
        error: code,
        message: error.to_string(),
    };
    eprintln!(
        "{}",
        serde_json::to_string(&diagnostic).expect("diagnostic is serializable")
    );
}

fn identity_error_code(error: &IdentityError) -> &'static str {
    match error {
        IdentityError::RepositoryMissing => "repository_missing",
        IdentityError::InvalidOrigin => "invalid_origin",
        IdentityError::InvalidUtf8 => "invalid_git_output",
    }
}

fn store_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::RunExists => "run_exists",
        StoreError::RunMissing => "run_missing",
        StoreError::RevisionConflict { .. } => "revision_conflict",
        StoreError::Io(_) => "io_error",
        StoreError::Json(_) => "invalid_baton",
        StoreError::InvalidHistory => "invalid_history",
        StoreError::TerminalState => "terminal_state",
        StoreError::UnsupportedSchema(_) => "unsupported_schema",
        StoreError::MigrationRequired => "migration_required",
        StoreError::InvalidSchemaTransition => "invalid_schema_transition",
        StoreError::LegacyClaimLive => "busy",
        StoreError::InvalidLeaseTimestamp => "invalid_timestamp",
        StoreError::InvalidBaton(_) => "invalid_baton",
    }
}

fn upgrade_error_code(error: &UpgradeError) -> &'static str {
    match error {
        UpgradeError::Store(error) => store_error_code(error),
        UpgradeError::Terminal => "terminal_state",
        UpgradeError::Busy => "busy",
        UpgradeError::Credential(_) => "credential_error",
        UpgradeError::Claim(error) => claim_error_code(error),
        UpgradeError::InvalidSession => "invalid_session",
        UpgradeError::InvalidObjective => "invalid_objective",
        UpgradeError::InvalidTopology => "invalid_participants",
        UpgradeError::InvalidSchema => "invalid_schema_transition",
        UpgradeError::InvalidTimestamp => "invalid_timestamp",
    }
}

fn claim_error_code(error: &ClaimError) -> &'static str {
    match error {
        ClaimError::Store(error) => store_error_code(error),
        ClaimError::Active => "claim_active",
        ClaimError::NotExpired => "claim_not_expired",
        ClaimError::Missing => "claim_missing",
        ClaimError::Fenced => "claim_fenced",
        ClaimError::InvalidLease | ClaimError::InvalidSession => "invalid_input",
        ClaimError::Terminal => "terminal_state",
        ClaimError::InvalidTimestamp => "invalid_baton",
    }
}

fn transition_error_code(error: &TransitionError) -> &'static str {
    match error {
        TransitionError::Store(error) => store_error_code(error),
        TransitionError::Claim(_) => "claim_fenced",
        TransitionError::WrongOwner => "wrong_owner",
        TransitionError::IllegalState => "invalid_transition",
        TransitionError::InvalidCheckpoint => "invalid_checkpoint",
        TransitionError::MissingVerification => "missing_verification",
        TransitionError::StaleReview => "stale_review",
        TransitionError::SupersessionPending => "supersession_pending",
        TransitionError::MissingFindings => "missing_findings",
        TransitionError::BlockingFindings => "blocking_findings",
        TransitionError::PublicationStale => "publication_stale",
        TransitionError::Terminal => "terminal_state",
        TransitionError::InvalidHumanDecision => "invalid_human_decision",
        TransitionError::WrongContact => "wrong_contact",
        TransitionError::LegacyPublicationUnsupported => "legacy_publication_unsupported",
        TransitionError::WrongPublisherHarness => "wrong_publisher_harness",
        TransitionError::WrongReviewerHarness => "wrong_reviewer_harness",
        TransitionError::InvalidExplainerPublication => "invalid_explainer_publication",
        TransitionError::StalePublicationBinding => "stale_publication_binding",
        TransitionError::SiteIdMismatch => "site_id_mismatch",
        TransitionError::MissingReason => "missing_reason",
        TransitionError::InvalidExplainerSource => "invalid_explainer_source",
        TransitionError::ExplainerNotStaged => "explainer_not_staged",
        TransitionError::InvalidProgress => "invalid_progress",
        TransitionError::InvalidCheckpointArtifact => "invalid_checkpoint_artifact",
        TransitionError::ExplainerBytesMissing => "explainer_bytes_missing",
        TransitionError::AnalysisNotStaged => "analysis_not_staged",
        TransitionError::AutonomousRecoveryAvailable => "autonomous_recovery_available",
    }
}

fn wait_error_code(error: &WaitError) -> &'static str {
    match error {
        WaitError::Store(error) => store_error_code(error),
        WaitError::Claim(_) => "claim_fenced",
    }
}

fn validate_init(
    run_id: &str,
    objective: &str,
    worker: &str,
    reviewer: &str,
    repository_id: &str,
) -> Result<(), CliError> {
    let safe_id = !run_id.is_empty()
        && run_id != "."
        && run_id != ".."
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !safe_id || run_id.contains("..") {
        return Err(CliError::Invalid("unsafe run id".to_owned()));
    }
    if objective.trim().is_empty() {
        return Err(CliError::Invalid("objective must not be blank".to_owned()));
    }
    normalize_participants(worker.to_owned(), reviewer.to_owned())?;
    if repository_id.trim().is_empty() {
        return Err(CliError::Invalid(
            "repository id must not be blank".to_owned(),
        ));
    }
    Ok(())
}

fn validate_optional(name: &str, value: Option<&str>) -> Result<(), CliError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(CliError::Invalid(format!("{name} must not be blank")));
    }
    Ok(())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_owned())
}

fn require_role_api(api: u32) -> Result<(), CliError> {
    if api == ROLE_API {
        Ok(())
    } else {
        Err(CliError::Invalid(format!(
            "role_api_mismatch: expected {ROLE_API}, found {api}"
        )))
    }
}

fn parse_deliverables(declarations: Vec<String>) -> Result<Vec<DeliverableRequirement>, CliError> {
    declarations
        .into_iter()
        .map(|declaration| {
            let (id, description) = declaration.split_once('=').ok_or_else(|| {
                CliError::Invalid("required deliverable must use id=description syntax".to_owned())
            })?;
            Ok(DeliverableRequirement {
                id: id.trim().to_owned(),
                description: description.trim().to_owned(),
            })
        })
        .collect()
}

fn parse_objective_refs(declarations: Vec<String>) -> Result<Vec<ExternalRef>, CliError> {
    declarations
        .into_iter()
        .map(|declaration| {
            let (kind, value) = declaration.split_once('=').ok_or_else(|| {
                CliError::Invalid("objective reference must use kind=value syntax".to_owned())
            })?;
            let kind = kind.trim().to_owned();
            let value = value.trim().to_owned();
            if kind.is_empty() || value.is_empty() {
                return Err(CliError::Invalid(
                    "objective reference must not be blank".to_owned(),
                ));
            }
            Ok(ExternalRef { kind, value })
        })
        .collect()
}
