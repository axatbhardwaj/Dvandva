//! CLI wrapper for `dvandva install-hooks [<repo-root>] [--uninstall]`.
//!
//! Parses the optional positional repo-root, the `--uninstall` flag, and the
//! deprecated `--force` no-op; resolves the repo root (default: the git
//! toplevel of the process cwd); then delegates to
//! [`dvandva::install_hooks::run_install`]. Exit codes mirror the shell
//! installer: 2 usage · 1 not-a-git-repo · otherwise the installer's code.

use std::path::{Path, PathBuf};

use dvandva::gitcfg::repo_toplevel;
use dvandva::install_hooks::run_install;

const USAGE: &str = "Usage: install-dvandva-hooks.sh [<repo-root>] [--uninstall]";

/// Run the `install-hooks` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut uninstall = false;
    let mut repo_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "--uninstall" => uninstall = true,
            "--force" => {} // deprecated no-op: coexistence never clobbers
            other if other.starts_with('-') => {
                eprintln!("install-dvandva-hooks: unknown option: {other}");
                eprintln!("{USAGE}");
                return 2;
            }
            other => {
                if repo_arg.is_some() {
                    eprintln!("install-dvandva-hooks: too many positional arguments");
                    return 2;
                }
                repo_arg = Some(other);
            }
        }
    }

    let root = match repo_arg {
        Some(arg) => match repo_toplevel(Path::new(arg)) {
            Some(root) => root,
            None => {
                eprintln!("install-dvandva-hooks: not a git repository: {arg}");
                return 1;
            }
        },
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match repo_toplevel(&cwd) {
                Some(root) => root,
                None => {
                    eprintln!("install-dvandva-hooks: not inside a git repository");
                    return 1;
                }
            }
        }
    };

    run_install(&root, uninstall)
}
