//! Git-hook entry point (invoked via an `argv[0]` symlink such as `pre-commit`
//! or `prepare-commit-msg`, or via `dvandva git-hook <name>`).
//!
//! All behavior lives in [`dvandva::hooks`]; this wrapper only forwards the
//! resolved hook name and its arguments. The invoked symlink path is read from
//! `argv[0]` inside the hook family, not passed here.

/// Dispatch a git-hook invocation, returning the process exit code.
pub fn run(name: &str, args: &[String]) -> i32 {
    dvandva::hooks::run(name, args)
}
