//! CLI wrapper for `dvandva retire-agents` — reversible retirement of the 5
//! standalone Claude user agents superseded by the dvandva-* roster.
//! Ports `scripts/retire-standalone-agents.sh`.

use dvandva::retire::{self, RetirePaths};

const USAGE: &str = "\
Usage: dvandva retire-agents [--dry-run|--apply|--restore <backup-dir>]

Reversibly retire the 5 standalone Claude user agents now superseded by the
dvandva-* roster (bundled in the dvandva plugin).

Modes:
  (no flags)          Dry-run: print what WOULD be retired; touch nothing. (default)
  --dry-run           Same as above (explicit).
  --apply             Execute retirement after the parity gate passes.
  --restore <dir>     Reverse a prior --apply run using its manifest.json.
  -h, --help          Show this help.

Safety:
  \u{2022} Only the 5 allowlisted symlinks are ever moved.
  \u{2022} --apply refuses unless the dvandva cache at DVANDVA_EXPECTED_VERSION
    (default: 1.5.2) contains all 15 required dvandva-* agent files.
  \u{2022} Haoshoku source targets are never touched; only the symlink pointers move.
  \u{2022} Skills, non-allowlisted agents, and Codex dirs are never modified.

Environment:
  HOME                       Overridable home dir (used in tests).
  CODEX_HOME                 Codex home dir (default: $HOME/.codex).
  DVANDVA_EXPECTED_VERSION   Required dvandva cache version (default: 1.5.2).
";
// NOTE: keep the two \"(default: ...)\" values above in sync with the
// authoritative default (DEFAULT_EXPECTED_VERSION / PLUGIN_VERSION) defined
// in src/retire.rs (or src/versions.rs).

enum Mode {
    DryRun,
    Apply,
    Restore(String),
}

enum ParseError {
    Help,
    Usage(String),
    Die(String),
}

/// Run the `retire-agents` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mode = match parse_args(args) {
        Ok(mode) => mode,
        Err(ParseError::Help) => {
            print!("{USAGE}");
            return 0;
        }
        Err(ParseError::Usage(message)) => {
            eprintln!("ERROR: {message}");
            eprint!("{USAGE}");
            return 2;
        }
        Err(ParseError::Die(message)) => {
            eprintln!("ERROR: {message}");
            return 1;
        }
    };

    let home = std::env::var("HOME").unwrap_or_default();
    let codex_home_env = std::env::var("CODEX_HOME").ok();
    let expected_version_env = std::env::var("DVANDVA_EXPECTED_VERSION").ok();
    let paths = RetirePaths::from_env(
        &home,
        codex_home_env.as_deref(),
        expected_version_env.as_deref(),
    );

    match mode {
        Mode::DryRun => {
            print!("{}", retire::dry_run_report(&paths));
            0
        }
        Mode::Apply => {
            let (stdout, stderr, code) = retire::run_apply(&paths);
            print!("{stdout}");
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }
            code
        }
        Mode::Restore(dir) => {
            if dir.is_empty() {
                eprintln!("ERROR: --restore requires a backup directory");
                return 1;
            }
            let (stdout, stderr, code) = retire::run_restore(&paths, &dir);
            print!("{stdout}");
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }
            code
        }
    }
}

fn parse_args(args: &[String]) -> Result<Mode, ParseError> {
    let mut mode = Mode::DryRun;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => {
                mode = Mode::DryRun;
                index += 1;
            }
            "--apply" => {
                mode = Mode::Apply;
                index += 1;
            }
            "--restore" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    ParseError::Die("--restore requires a backup directory argument".to_string())
                })?;
                mode = Mode::Restore(value.clone());
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::Usage(format!("unknown option: {other}"))),
        }
    }

    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    // USAGE embeds the "(default: ...)" version as a literal (see the NOTE
    // above USAGE), because a `const` cannot be interpolated into another
    // `const &str` at compile time. This test is the drift guard the NOTE
    // promises: it fails the moment USAGE's literal falls out of sync with
    // the compiled `versions::PLUGIN_VERSION`, instead of silently going
    // stale on the next version bump.
    #[test]
    fn usage_default_version_matches_plugin_version_constant() {
        assert!(
            USAGE.contains(dvandva::versions::PLUGIN_VERSION),
            "USAGE help text must reference versions::PLUGIN_VERSION ({}); got:\n{USAGE}",
            dvandva::versions::PLUGIN_VERSION
        );
    }

    #[test]
    fn parse_args_defaults_to_dry_run() {
        assert!(matches!(parse_args(&[]), Ok(Mode::DryRun)));
    }

    #[test]
    fn parse_args_accepts_dry_run_and_apply() {
        assert!(matches!(
            parse_args(&["--dry-run".to_string()]),
            Ok(Mode::DryRun)
        ));
        assert!(matches!(
            parse_args(&["--apply".to_string()]),
            Ok(Mode::Apply)
        ));
    }

    #[test]
    fn parse_args_accepts_restore_with_directory() {
        match parse_args(&["--restore".to_string(), "/backup/dir".to_string()]) {
            Ok(Mode::Restore(dir)) => assert_eq!(dir, "/backup/dir"),
            _ => panic!("expected Mode::Restore"),
        }
    }

    #[test]
    fn parse_args_rejects_restore_without_argument() {
        assert!(matches!(
            parse_args(&["--restore".to_string()]),
            Err(ParseError::Die(_))
        ));
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        assert!(matches!(
            parse_args(&["--bogus".to_string()]),
            Err(ParseError::Usage(_))
        ));
    }

    #[test]
    fn parse_args_recognizes_help() {
        assert!(matches!(
            parse_args(&["-h".to_string()]),
            Err(ParseError::Help)
        ));
        assert!(matches!(
            parse_args(&["--help".to_string()]),
            Err(ParseError::Help)
        ));
    }
}
