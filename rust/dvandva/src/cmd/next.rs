//! CLI wrapper for `dvandva next` — flow-patch target (design §F1).
//!
//! All logic lives in `dvandva::next` (the library module), where it can reach
//! the `pub(crate)` transition surface `dvandva write` validates with. This
//! wrapper only forwards the subcommand args.

pub fn run(args: &[String]) -> i32 {
    dvandva::next::run(args)
}
