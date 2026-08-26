use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use serde::Serialize;
use thiserror::Error;

use crate::model::RunBaton;

#[derive(Debug, Parser)]
#[command(name = "dvandva-v4")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
    },
    Read {
        #[arg(long)]
        run_dir: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Invalid(String),
    #[error("run already exists")]
    RunExists,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid baton JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Serialize)]
struct Diagnostic<'a> {
    error: &'a str,
    message: String,
}

pub fn run() -> Result<(), CliError> {
    match Cli::parse().command {
        Command::Init {
            run_dir,
            run_id,
            objective,
            worker,
            reviewer,
        } => {
            validate_init(&run_id, &objective, &worker, &reviewer)?;
            fs::create_dir_all(&run_dir)?;
            let path = run_dir.join("baton.json");
            if path.exists() {
                return Err(CliError::RunExists);
            }
            let baton = RunBaton::new(run_id, objective.trim(), worker, reviewer);
            fs::write(path, serde_json::to_vec_pretty(&baton)?)?;
            Ok(())
        }
        Command::Read { run_dir } => {
            let bytes = fs::read(run_dir.join("baton.json"))?;
            let baton: RunBaton = serde_json::from_slice(&bytes)?;
            println!("{}", serde_json::to_string_pretty(&baton)?);
            Ok(())
        }
    }
}

pub fn print_error(error: &CliError) {
    let code = match error {
        CliError::Invalid(_) => "invalid_input",
        CliError::RunExists => "run_exists",
        CliError::Io(_) => "io_error",
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

fn validate_init(
    run_id: &str,
    objective: &str,
    worker: &str,
    reviewer: &str,
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
    Ok(())
}
