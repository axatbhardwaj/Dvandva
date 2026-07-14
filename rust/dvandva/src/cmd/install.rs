//! CLI wrapper for `dvandva install` — ports `scripts/install.sh`.

use dvandva::installers::{self, InstallTargets};

const USAGE: &str = "\
Usage: dvandva install [--claude-only|--codex-only] [<marketplace-path-or-repo>]

Installs the dvandva@dvandva plugin into Claude Code and Codex by default.

Options:
  --claude-only   Install only the Claude Code plugin.
  --codex-only    Install only the Codex plugin.
  -h, --help      Show this help.

Default marketplace: axatbhardwaj/Dvandva";

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallArgs {
    targets: InstallTargets,
    marketplace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseOutcome {
    Help,
    Run(InstallArgs),
}

/// Run the `install` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    match parse_args(args) {
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
            0
        }
        Ok(ParseOutcome::Run(parsed)) => {
            installers::run_install(parsed.targets, &parsed.marketplace)
        }
        Err(message) => {
            eprintln!("ERROR: {message}");
            eprintln!("{USAGE}");
            2
        }
    }
}

/// Mirrors `install.sh`'s `while [[ $# -gt 0 ]]; do case "$1" in ...`: flags
/// must precede the single positional marketplace argument — any argument
/// (flag or not) following the positional is rejected.
fn parse_args(args: &[String]) -> Result<ParseOutcome, String> {
    let mut install_claude = true;
    let mut install_codex = true;
    let mut marketplace = installers::DEFAULT_MARKETPLACE.to_string();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--claude-only" => {
                install_codex = false;
                index += 1;
            }
            "--codex-only" => {
                install_claude = false;
                index += 1;
            }
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            positional => {
                marketplace = positional.to_string();
                index += 1;
                if index < args.len() {
                    return Err("expected at most one marketplace argument".to_string());
                }
            }
        }
    }

    if !install_claude && !install_codex {
        return Err("--claude-only and --codex-only cannot be combined".to_string());
    }

    Ok(ParseOutcome::Run(InstallArgs {
        targets: InstallTargets {
            claude: install_claude,
            codex: install_codex,
        },
        marketplace,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_defaults_to_both_engines_and_default_marketplace() {
        assert_eq!(
            parse_args(&args(&[])).unwrap(),
            ParseOutcome::Run(InstallArgs {
                targets: InstallTargets {
                    claude: true,
                    codex: true
                },
                marketplace: installers::DEFAULT_MARKETPLACE.to_string(),
            })
        );
    }

    #[test]
    fn claude_only_disables_codex() {
        assert_eq!(
            parse_args(&args(&["--claude-only"])).unwrap(),
            ParseOutcome::Run(InstallArgs {
                targets: InstallTargets {
                    claude: true,
                    codex: false
                },
                marketplace: installers::DEFAULT_MARKETPLACE.to_string(),
            })
        );
    }

    #[test]
    fn codex_only_disables_claude() {
        assert_eq!(
            parse_args(&args(&["--codex-only"])).unwrap(),
            ParseOutcome::Run(InstallArgs {
                targets: InstallTargets {
                    claude: false,
                    codex: true
                },
                marketplace: installers::DEFAULT_MARKETPLACE.to_string(),
            })
        );
    }

    #[test]
    fn conflicting_flags_are_rejected_after_parsing() {
        let err = parse_args(&args(&["--claude-only", "--codex-only"])).unwrap_err();
        assert!(err.contains("cannot be combined"), "got: {err}");
    }

    #[test]
    fn positional_marketplace_overrides_default() {
        assert_eq!(
            parse_args(&args(&["/tmp/local-marketplace"])).unwrap(),
            ParseOutcome::Run(InstallArgs {
                targets: InstallTargets {
                    claude: true,
                    codex: true
                },
                marketplace: "/tmp/local-marketplace".to_string(),
            })
        );
    }

    #[test]
    fn flag_after_positional_is_rejected() {
        let err = parse_args(&args(&["/tmp/local-marketplace", "--claude-only"])).unwrap_err();
        assert!(
            err.contains("at most one marketplace argument"),
            "got: {err}"
        );
    }

    #[test]
    fn second_positional_is_rejected() {
        let err = parse_args(&args(&["one", "two"])).unwrap_err();
        assert!(
            err.contains("at most one marketplace argument"),
            "got: {err}"
        );
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let err = parse_args(&args(&["--bogus"])).unwrap_err();
        assert!(err.contains("unknown option: --bogus"), "got: {err}");
    }

    #[test]
    fn help_flag_short_and_long_are_recognized() {
        assert_eq!(parse_args(&args(&["-h"])).unwrap(), ParseOutcome::Help);
        assert_eq!(parse_args(&args(&["--help"])).unwrap(), ParseOutcome::Help);
    }
}
