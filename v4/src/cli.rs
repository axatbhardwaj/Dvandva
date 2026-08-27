use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;
use thiserror::Error;

use crate::action::Action;
use crate::claim::{self, ClaimError, Role};
use crate::discovery::{self, DiscoveryError, DiscoveryQuery};
use crate::identity::{self, IdentityError};
use crate::model::{RunBaton, TaskIdentity, WorkspaceIdentity, SCHEMA};
use crate::role_session::{self, RoleSessionError, RoleStartRequest};
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
        session_id: Option<String>,
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
        session_id: Option<String>,
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
        objective: String,
        #[arg(long)]
        task_reference: Option<String>,
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
        #[arg(long)]
        credentials_root: PathBuf,
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
        #[arg(long)]
        credentials_root: PathBuf,
    },
    Apply {
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
    schema: &'a str,
    compatible: bool,
}

pub fn run() -> Result<(), CliError> {
    match Cli::parse().command {
        Command::Probe { expected_schema } => {
            let probe = Probe {
                package: env!("CARGO_PKG_NAME"),
                version: env!("CARGO_PKG_VERSION"),
                schema: SCHEMA,
                compatible: expected_schema == SCHEMA,
            };
            println!("{}", serde_json::to_string_pretty(&probe)?);
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
            session_id,
        } => {
            let outcome = discovery::discover(
                &runs_dir,
                DiscoveryQuery {
                    repository_id: &repository_id,
                    role,
                    participant_harness: &reviewer_harness,
                    task_reference: task_reference.as_deref(),
                    session_id: session_id.as_deref(),
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
            session_id,
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
                    session_id: session_id.as_deref(),
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
                workspace,
                runs_dir,
                credentials_root,
                role,
                session_id,
                current_harness,
                peer_harness,
                objective,
                task_reference,
                lease_seconds,
                wait,
                poll_interval_ms,
                timeout_ms,
                new_run,
            } => {
                let result = role_session::start(RoleStartRequest {
                    workspace: &workspace,
                    runs_dir: &runs_dir,
                    credentials_root: &credentials_root,
                    role,
                    session_id: &session_id,
                    current_harness: &current_harness,
                    peer_harness: &peer_harness,
                    objective: &objective,
                    task_reference: task_reference.as_deref(),
                    lease_seconds,
                    wait,
                    poll_interval: std::time::Duration::from_millis(poll_interval_ms.max(1)),
                    timeout: std::time::Duration::from_millis(timeout_ms),
                    new_run,
                })?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            RoleCommand::Claim {
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
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
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
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
                run_dir,
                role,
                session_id,
                expected_revision,
                credentials_root,
                action,
            } => {
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
                run_dir,
                role,
                session_id,
                credentials_root,
            } => {
                let baton = role_session::read(&run_dir, &credentials_root, role, &session_id)?;
                println!("{}", serde_json::to_string_pretty(&baton)?);
                Ok(())
            }
            RoleCommand::Heartbeat {
                run_dir,
                role,
                session_id,
                lease_seconds,
                expected_revision,
                credentials_root,
            } => {
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
                run_dir,
                role,
                session_id,
                credentials_root,
                after_revision,
                poll_interval_ms,
                timeout_ms,
            } => {
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
        } => {
            validate_init(&run_id, &objective, &worker, &reviewer, &repository_id)?;
            validate_optional("origin", origin.as_deref())?;
            validate_optional("worktree", worktree.as_deref())?;
            validate_optional("task reference", task_reference.as_deref())?;
            let objective = objective.trim();
            let baton = RunBaton::new(run_id, objective, worker.trim(), reviewer.trim())
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
            let baton = wait::wait(
                &RunChannel::open(run_dir),
                role,
                &session_id,
                &token,
                after_revision,
                std::time::Duration::from_millis(poll_interval_ms.max(1)),
                std::time::Duration::from_millis(timeout_ms),
            )?;
            println!("{}", serde_json::to_string_pretty(&baton)?);
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
        CliError::Identity(IdentityError::RepositoryMissing) => "repository_missing",
        CliError::Identity(IdentityError::InvalidOrigin) => "invalid_origin",
        CliError::Identity(IdentityError::InvalidUtf8) => "invalid_git_output",
        CliError::Discovery(DiscoveryError::Io(_)) => "discovery_io",
        CliError::RoleSession(RoleSessionError::Store(StoreError::RunExists)) => "run_exists",
        CliError::RoleSession(RoleSessionError::Store(StoreError::RunMissing)) => "run_missing",
        CliError::RoleSession(RoleSessionError::Store(StoreError::RevisionConflict { .. })) => {
            "revision_conflict"
        }
        CliError::RoleSession(RoleSessionError::Store(StoreError::Io(_))) => "io_error",
        CliError::RoleSession(RoleSessionError::Store(StoreError::Json(_))) => "invalid_baton",
        CliError::RoleSession(RoleSessionError::Store(StoreError::InvalidHistory)) => {
            "invalid_history"
        }
        CliError::RoleSession(RoleSessionError::Store(StoreError::TerminalState)) => {
            "terminal_state"
        }
        CliError::RoleSession(RoleSessionError::Claim(_)) => "claim_error",
        CliError::RoleSession(RoleSessionError::Credential(_)) => "credential_error",
        CliError::RoleSession(RoleSessionError::Transition(_)) => "transition_error",
        CliError::RoleSession(RoleSessionError::Wait(_)) => "wait_error",
        CliError::RoleSession(RoleSessionError::Identity(_)) => "repository_error",
        CliError::RoleSession(RoleSessionError::Discovery(_)) => "discovery_error",
        CliError::RoleSession(RoleSessionError::Invalid(_)) => "invalid_input",
        CliError::Store(StoreError::RunExists) => "run_exists",
        CliError::Store(StoreError::RunMissing) => "run_missing",
        CliError::Store(StoreError::RevisionConflict { .. }) => "revision_conflict",
        CliError::Store(StoreError::Io(_)) => "io_error",
        CliError::Store(StoreError::Json(_)) => "invalid_baton",
        CliError::Store(StoreError::InvalidHistory) => "invalid_history",
        CliError::Store(StoreError::TerminalState) => "terminal_state",
        CliError::Json(_) => "invalid_baton",
        CliError::Claim(ClaimError::Store(StoreError::RevisionConflict { .. })) => {
            "revision_conflict"
        }
        CliError::Claim(ClaimError::Store(StoreError::RunExists)) => "run_exists",
        CliError::Claim(ClaimError::Store(StoreError::RunMissing)) => "run_missing",
        CliError::Claim(ClaimError::Store(StoreError::Io(_))) => "io_error",
        CliError::Claim(ClaimError::Store(StoreError::Json(_))) => "invalid_baton",
        CliError::Claim(ClaimError::Store(StoreError::InvalidHistory)) => "invalid_history",
        CliError::Claim(ClaimError::Store(StoreError::TerminalState)) => "terminal_state",
        CliError::Claim(ClaimError::Active) => "claim_active",
        CliError::Claim(ClaimError::NotExpired) => "claim_not_expired",
        CliError::Claim(ClaimError::Missing) => "claim_missing",
        CliError::Claim(ClaimError::Fenced) => "claim_fenced",
        CliError::Claim(ClaimError::InvalidLease | ClaimError::InvalidSession) => "invalid_input",
        CliError::Claim(ClaimError::Terminal) => "terminal_state",
        CliError::Claim(ClaimError::InvalidTimestamp) => "invalid_baton",
        CliError::Transition(TransitionError::Store(StoreError::RevisionConflict { .. })) => {
            "revision_conflict"
        }
        CliError::Transition(TransitionError::Store(StoreError::RunExists)) => "run_exists",
        CliError::Transition(TransitionError::Store(StoreError::RunMissing)) => "run_missing",
        CliError::Transition(TransitionError::Store(StoreError::Io(_))) => "io_error",
        CliError::Transition(TransitionError::Store(StoreError::Json(_))) => "invalid_baton",
        CliError::Transition(TransitionError::Store(StoreError::InvalidHistory)) => {
            "invalid_history"
        }
        CliError::Transition(TransitionError::Store(StoreError::TerminalState)) => "terminal_state",
        CliError::Transition(TransitionError::Claim(_)) => "claim_fenced",
        CliError::Transition(TransitionError::WrongOwner) => "wrong_owner",
        CliError::Transition(TransitionError::IllegalState) => "invalid_transition",
        CliError::Transition(TransitionError::InvalidCheckpoint) => "invalid_checkpoint",
        CliError::Transition(TransitionError::MissingVerification) => "missing_verification",
        CliError::Transition(TransitionError::StaleReview) => "stale_review",
        CliError::Transition(TransitionError::MissingFindings) => "missing_findings",
        CliError::Transition(TransitionError::BlockingFindings) => "blocking_findings",
        CliError::Transition(TransitionError::PublicationStale) => "publication_stale",
        CliError::Transition(TransitionError::Terminal) => "terminal_state",
        CliError::Transition(TransitionError::InvalidHumanDecision) => "invalid_human_decision",
        CliError::Transition(TransitionError::WrongContact) => "wrong_contact",
        CliError::Transition(TransitionError::PublicationRegression) => "publication_regression",
        CliError::Transition(TransitionError::MissingReason) => "missing_reason",
        CliError::Wait(WaitError::Store(StoreError::RunMissing)) => "run_missing",
        CliError::Wait(WaitError::Store(StoreError::Json(_))) => "invalid_baton",
        CliError::Wait(WaitError::Store(StoreError::Io(_))) => "io_error",
        CliError::Wait(WaitError::Store(StoreError::RunExists)) => "run_exists",
        CliError::Wait(WaitError::Store(StoreError::RevisionConflict { .. })) => {
            "revision_conflict"
        }
        CliError::Wait(WaitError::Store(StoreError::InvalidHistory)) => "invalid_history",
        CliError::Wait(WaitError::Store(StoreError::TerminalState)) => "terminal_state",
        CliError::Wait(WaitError::Claim(_)) => "claim_fenced",
        CliError::Wait(WaitError::Timeout) => "timeout",
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
    if worker.trim().is_empty() || reviewer.trim().is_empty() {
        return Err(CliError::Invalid("harness must not be blank".to_owned()));
    }
    if worker.trim().eq_ignore_ascii_case(reviewer.trim()) {
        return Err(CliError::Invalid(
            "worker and reviewer must use different harness families".to_owned(),
        ));
    }
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
