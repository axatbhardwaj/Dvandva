//! Doc-contract assertions ported from `scripts/test-dvandva-skill-preflight.sh`
//! (336 ln), re-keyed to the post-port `dvandva <subcommand>` grammar
//! (design doc §3: `dvandva preflight --role <role>`, `dvandva wait --role
//! <role>`, `dvandva state --compact`, `dvandva install-hooks`).
//!
//! These tests read the *live* repo tree (SKILL.md, README, command files,
//! references) rather than fixtures, so they assert the target contract for
//! the post-port docs, not today's pre-rewrite wording. The doc rewrite (Wave
//! C) has landed, so every live-tree case here runs by default alongside the
//! `require_match`/`reject_match` matcher self-tests (`matcher_engine`
//! module).
//!
//! Case naming mirrors the shell script's `for file in ...` loop bodies: one
//! Rust test per distinct contract, asserted against every file the shell
//! loop iterated over (so a single test may check 2-3 files internally,
//! exactly as the shell loop did in one pass).
//!
//! Dropped vs. the shell source (see task report for rationale):
//! - The `[[ ! -f "$ROOT_DIR/scripts/dvandva-preflight.sh" ]]` file-existence
//!   check (script line ~289): the entire `scripts/` tree is deleted
//!   repo-wide by the port's Wave D, so a single-file existence probe here
//!   doesn't carry independent contract value.
//! - No "SKILL.md byte-identical-copy between vadi/prativadi scripts dirs"
//!   assertion was found in this shell source to drop; it does not appear in
//!   `scripts/test-dvandva-skill-preflight.sh` or
//!   `scripts/smoke-plugin-install.sh` as read for this port.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must resolve from CARGO_MANIFEST_DIR/../..")
}

fn vadi_skill() -> PathBuf {
    repo_root().join("plugins/dvandva/skills/vadi/SKILL.md")
}
fn prativadi_skill() -> PathBuf {
    repo_root().join("plugins/dvandva/skills/prativadi/SKILL.md")
}
fn role_skills() -> [(PathBuf, &'static str); 2] {
    [(vadi_skill(), "vadi"), (prativadi_skill(), "prativadi")]
}

fn command_vadi() -> PathBuf {
    repo_root().join("plugins/dvandva/commands/vadi.md")
}
fn command_prativadi() -> PathBuf {
    repo_root().join("plugins/dvandva/commands/prativadi.md")
}
fn command_files() -> [(PathBuf, &'static str); 2] {
    [(command_vadi(), "vadi"), (command_prativadi(), "prativadi")]
}

fn state_ref() -> PathBuf {
    repo_root().join("plugins/dvandva/references/state-transition-table.md")
}
fn research_skill() -> PathBuf {
    repo_root().join("plugins/dvandva/skills/research/SKILL.md")
}
fn product_md() -> PathBuf {
    repo_root().join("product.md")
}
fn readme() -> PathBuf {
    repo_root().join("README.md")
}
fn local_baton_channel_docs() -> PathBuf {
    repo_root().join("docs/protocol/local-baton-channel.md")
}
fn local_baton_channel_plugin() -> PathBuf {
    repo_root().join("plugins/dvandva/references/local-baton-channel.md")
}
fn local_baton_channel_files() -> [(PathBuf, &'static str); 2] {
    [
        (
            local_baton_channel_docs(),
            "docs/protocol/local-baton-channel.md",
        ),
        (
            local_baton_channel_plugin(),
            "plugins/dvandva/references/local-baton-channel.md",
        ),
    ]
}
fn codex_goal_notes() -> PathBuf {
    repo_root().join("docs/research/codex-goal-notes.md")
}
fn claude_code_goal() -> PathBuf {
    repo_root().join("docs/research/claude-code-goal.md")
}
fn docs_research_files() -> [(PathBuf, &'static str); 2] {
    [
        (codex_goal_notes(), "docs/research/codex-goal-notes.md"),
        (claude_code_goal(), "docs/research/claude-code-goal.md"),
    ]
}
fn final_triplet_files() -> [(PathBuf, &'static str); 3] {
    [
        (product_md(), "product.md"),
        (
            local_baton_channel_docs(),
            "docs/protocol/local-baton-channel.md",
        ),
        (
            local_baton_channel_plugin(),
            "plugins/dvandva/references/local-baton-channel.md",
        ),
    ]
}

/// Read a file and flatten newlines to spaces, mirroring the shell helpers'
/// `tr '\n' ' '` before a single-line `grep -Eiq` pass.
fn read_flat(path: &Path) -> String {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    text.replace('\n', " ")
}

fn compile(pattern: &str) -> Regex {
    Regex::new(&format!("(?i){pattern}"))
        .unwrap_or_else(|error| panic!("invalid pattern {pattern:?}: {error}"))
}

/// Require `pattern` (case-insensitive extended regex) to match somewhere in
/// `path`'s flattened content. Mirrors the shell `require_match`.
fn require_match(path: &Path, pattern: &str, message: &str) {
    let text = read_flat(path);
    assert!(
        compile(pattern).is_match(&text),
        "FAIL: {message} (pattern: {pattern:?}, file: {})",
        path.display()
    );
}

/// Require `pattern` to NOT match anywhere in `path`'s flattened content.
/// Mirrors the shell `reject_match`.
fn reject_match(path: &Path, pattern: &str, message: &str) {
    let text = read_flat(path);
    assert!(
        !compile(pattern).is_match(&text),
        "FAIL: {message} (pattern: {pattern:?}, file: {})",
        path.display()
    );
}

macro_rules! live_tree_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            $body
        }
    };
}

/// Self-tests for the `require_match`/`reject_match` matcher engine itself,
/// using self-contained fixtures rather than the live repo tree. The
/// original shell suite had no self-contained cases to port as-is; these
/// exist to validate the matcher engine before layering the ~82 live-tree
/// contract cases on top of it.
mod matcher_engine {
    use super::*;

    fn fixture(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.md");
        fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn require_match_passes_case_insensitively_across_newlines() {
        let (_dir, path) =
            fixture("Baton creation/resume\ndiscovery IS mandatory before active work.");
        require_match(
            &path,
            r"baton creation/resume discovery is mandatory before active work",
            "case-insensitive newline-flattened match",
        );
    }

    #[test]
    #[should_panic(expected = "FAIL: expected pattern")]
    fn require_match_panics_when_pattern_absent() {
        let (_dir, path) = fixture("nothing relevant here");
        require_match(&path, "totally-absent-pattern", "expected pattern");
    }

    #[test]
    fn reject_match_passes_when_pattern_absent() {
        let (_dir, path) = fixture("clean content");
        reject_match(&path, "forbidden-phrase", "reject absent phrase");
    }

    #[test]
    #[should_panic(expected = "FAIL: forbidden phrase present")]
    fn reject_match_panics_when_pattern_present() {
        let (_dir, path) = fixture("this has a forbidden-phrase in it");
        reject_match(&path, "forbidden-phrase", "forbidden phrase present");
    }
}

/// Role SKILL.md contract (`plugins/dvandva/skills/{vadi,prativadi}/SKILL.md`).
/// Source: shell lines 47-121 (`for file in "$VADI" "$PRATIVADI"`).
mod role_skill_contract {
    use super::*;

    live_tree_test!(gates_baton_discovery, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"Baton creation/resume discovery is mandatory before active work",
                &format!("{role} skill makes baton discovery a hard preflight gate"),
            );
        }
    });

    live_tree_test!(resolves_baton_before_reads_writes, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"Resolve the active baton path before reading or writing",
                &format!("{role} skill resolves baton before reads/writes"),
            );
        }
    });

    live_tree_test!(hook_preflight_only_after_baton_resolution, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"before active non-wait work",
                &format!("{role} skill runs hook preflight only after baton resolution"),
            );
        }
    });

    live_tree_test!(exports_dvandva_role, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                &format!(r"export\s+DVANDVA_ROLE={role}"),
                &format!("{role} skill exports DVANDVA_ROLE={role}"),
            );
        }
    });

    live_tree_test!(asserts_dvandva_role, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                &format!(r"asserts?\s+`?DVANDVA_ROLE={role}`?"),
                &format!("{role} skill asserts DVANDVA_ROLE={role}"),
            );
        }
    });

    live_tree_test!(detects_hook_adoption_instead_of_forcing_it, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"detects?\s+Dvandva hook adoption|hook adoption status",
                &format!("{role} skill detects hook adoption instead of forcing it"),
            );
        }
    });

    live_tree_test!(records_prior_hooks_path, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"dvandva\.priorHooksPath",
                &format!("{role} skill records prior hooksPath as dvandva.priorHooksPath and restores on uninstall"),
            );
        }
    });

    live_tree_test!(gates_checkpoint_commits_on_adopted_hooks, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"Checkpoint commits require Dvandva hook adoption",
                &format!("{role} skill gates checkpoint commits on adopted hooks"),
            );
        }
    });

    live_tree_test!(gates_final_commits_on_adopted_hooks, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"Final commits require Dvandva hook adoption",
                &format!("{role} skill gates final commits on adopted hooks"),
            );
        }
    });

    // Re-keyed: the bundled `scripts/install-dvandva-hooks.sh` invocation no
    // longer exists; the equivalent direct-usage instruction to reject is an
    // imperative "run/bash dvandva install-hooks" instruction (the ported
    // subcommand). Preflight owns calling it in-process. Narrowed from a
    // bare `dvandva\s+install-hooks` match (which banned any mention,
    // including a CLI-reference table row) to imperative framing only.
    live_tree_test!(does_not_require_direct_install_hooks_invocation, {
        for (path, role) in role_skills() {
            reject_match(
                &path,
                r"run\s+.{0,12}dvandva\s+install-hooks|bash\s+.{0,12}dvandva\s+install-hooks",
                &format!("{role} skill does not require directly invoking dvandva install-hooks"),
            );
        }
    });

    live_tree_test!(documents_multipart_termination_review, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"termination_review",
                &format!("{role} skill documents the multipart termination review state"),
            );
        }
    });

    live_tree_test!(terminal_stop_is_shared_two_role_decision, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"both roles.*stop|stop.*both roles|shared termination|multipart termination",
                &format!("{role} skill says terminal stop is a shared two-role decision"),
            );
        }
    });

    live_tree_test!(surfaces_and_gates_run_explainer_reviews, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"run_explainer_reviews",
                &format!("{role} skill surfaces and gates final run explainer reviews"),
            );
        }
    });

    live_tree_test!(helper_enforced_explainer_review_ownership, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"approval and explainer-review ownership|explainer-review and approval ownership",
                &format!("{role} skill says helper-enforced ownership covers explainer reviews"),
            );
        }
    });

    live_tree_test!(human_intervention_states_are_paired_run_pauses, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r#"human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)"#,
                &format!("{role} skill says human_question and human_decision are paired run pauses that stop both roles together"),
            );
        }
    });

    live_tree_test!(requires_newer_sibling_propagation, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)",
                &format!("{role} skill requires newer sibling human-intervention propagation"),
            );
        }
    });

    live_tree_test!(preserves_sibling_human_question_metadata, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"sibling.{0,160}human_question.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|human_question.{0,160}sibling.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|question.{0,160}resume_assignee.{0,160}resume_status.{0,160}sibling.{0,160}human_question",
                &format!(
                    "{role} skill preserves sibling human_question question and resume metadata"
                ),
            );
        }
    });

    live_tree_test!(uses_checkpoint_gated_wait_after_handoff, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"--since-checkpoint",
                &format!("{role} skill uses checkpoint-gated wait after handoff"),
            );
        }
    });

    live_tree_test!(uses_action_aware_wait_for_team_owned_states, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"--until-actionable",
                &format!("{role} skill uses action-aware waiting for team-owned states"),
            );
        }
    });

    live_tree_test!(requires_compact_baton_state_surfacing, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT",
                &format!("{role} skill requires compact baton-state surfacing"),
            );
        }
    });

    // Re-keyed: `dvandva-state.sh --compact` -> `dvandva state --compact`.
    live_tree_test!(names_compact_state_subcommand, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"dvandva\s+state\s+--compact",
                &format!("{role} skill names the compact state subcommand"),
            );
        }
    });

    live_tree_test!(compact_field_list_includes_schema_and_active_roles, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT.{0,240}schema.{0,240}active_roles|BATON_STATE_COMPACT.{0,240}active_roles.{0,240}schema",
                &format!("{role} skill compact field list includes schema and active_roles"),
            );
        }
    });

    live_tree_test!(requires_full_baton_reads_before_state_changing_decisions, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"read .*authoritative full .*baton\.json.*state-changing|state-changing.*read .*authoritative full .*baton\.json",
                &format!("{role} skill requires full baton reads before state-changing decisions"),
            );
        }
    });

    live_tree_test!(no_stale_baton_state_surfacing_wording, {
        for (path, role) in role_skills() {
            reject_match(
                &path,
                r"Surface( the new)?\s+`?BATON_STATE([^_A-Z0-9]|$)",
                &format!("{role} skill has no stale BATON_STATE surfacing wording"),
            );
        }
    });

    // New (re-keyed grammar, design doc §3): the bundled per-role
    // `${CLAUDE_SKILL_DIR}/scripts/dvandva-preflight.sh --role <role>`
    // invocation is now the single canonical `dvandva preflight --role
    // <role>` subcommand.
    live_tree_test!(invokes_preflight_with_role, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                &format!(r"dvandva\s+preflight\s+--role\s+{role}"),
                &format!("{role} skill invokes dvandva preflight --role {role}"),
            );
        }
    });
}

/// Command file contract (`plugins/dvandva/commands/{vadi,prativadi}.md`).
/// Source: shell lines 123-164.
mod command_contract {
    use super::*;

    live_tree_test!(distinguishes_post_handshake_done_from_final_approval, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r#"post-handshake "done"|post-handshake done"#,
                &format!("{role} command distinguishes post-handshake done from final approval"),
            );
        }
    });

    live_tree_test!(documents_termination_review_as_active, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"termination_review",
                &format!("{role} command documents termination_review as active"),
            );
        }
    });

    live_tree_test!(roles_stop_together_only_after_shared_approval, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"keep polling or stop together|both roles keep polling|both approve",
                &format!("{role} command says roles stop together only after shared approval"),
            );
        }
    });

    live_tree_test!(requires_both_explainer_reviews_before_done, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"run_explainer_reviews",
                &format!("{role} command requires both explainer reviews before done"),
            );
        }
    });

    live_tree_test!(rejects_one_step_terminal_stop_wording, {
        for (path, role) in command_files() {
            reject_match(
                &path,
                r#"Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision""#,
                &format!("{role} command rejects one-step terminal stop wording"),
            );
        }
    });

    live_tree_test!(human_intervention_states_are_paired_run_pauses, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r#"human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)"#,
                &format!("{role} command says human_question and human_decision are paired run pauses that stop both roles together"),
            );
        }
    });

    live_tree_test!(requires_newer_sibling_propagation, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)",
                &format!("{role} command requires newer sibling human-intervention propagation"),
            );
        }
    });

    live_tree_test!(uses_checkpoint_gated_wait_after_handoff, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"--since-checkpoint",
                &format!("{role} command uses checkpoint-gated wait after handoff"),
            );
        }
    });

    live_tree_test!(uses_action_aware_wait_for_team_owned_states, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"--until-actionable",
                &format!("{role} command uses action-aware waiting for team-owned states"),
            );
        }
    });

    live_tree_test!(requires_compact_baton_state_surfacing, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT",
                &format!("{role} command requires compact baton-state surfacing"),
            );
        }
    });

    // Re-keyed: `dvandva-state.sh --compact` -> `dvandva state --compact`.
    live_tree_test!(names_compact_state_subcommand, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"dvandva\s+state\s+--compact",
                &format!("{role} command names the compact state subcommand"),
            );
        }
    });

    live_tree_test!(compact_field_list_includes_schema_and_active_roles, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT.{0,240}schema.{0,240}active_roles|BATON_STATE_COMPACT.{0,240}active_roles.{0,240}schema",
                &format!("{role} command compact field list includes schema and active_roles"),
            );
        }
    });

    live_tree_test!(requires_full_baton_reads_before_state_changing_decision, {
        for (path, role) in command_files() {
            require_match(
                &path,
                r"full .*baton\.json.*state-changing decision|state-changing decision.*full .*baton\.json",
                &format!(
                    "{role} command requires full baton reads before state-changing decisions"
                ),
            );
        }
    });
}

/// Local-baton-channel reference contract. Source: shell lines 166-174.
mod local_baton_channel_contract {
    use super::*;

    live_tree_test!(requires_compact_baton_state_surfacing, {
        for (path, label) in local_baton_channel_files() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT",
                &format!("{label} requires compact baton-state surfacing"),
            );
        }
    });

    // Re-keyed: `dvandva-state.sh --compact` -> `dvandva state --compact`.
    live_tree_test!(names_compact_state_subcommand, {
        for (path, label) in local_baton_channel_files() {
            require_match(
                &path,
                r"dvandva\s+state\s+--compact",
                &format!("{label} names the compact state subcommand"),
            );
        }
    });
}

/// Research skill contract. Source: shell lines 176-184.
mod research_skill_contract {
    use super::*;

    live_tree_test!(requires_compact_baton_state_surfacing, {
        require_match(
            &research_skill(),
            r"BATON_STATE_COMPACT",
            "research skill requires compact baton-state surfacing",
        );
    });

    // Re-keyed: `dvandva-state.sh --compact` -> `dvandva state --compact`.
    live_tree_test!(names_compact_state_subcommand, {
        require_match(
            &research_skill(),
            r"dvandva\s+state\s+--compact",
            "research skill names the compact state subcommand",
        );
    });

    live_tree_test!(requires_full_baton_reads_before_state_changing_writes, {
        require_match(
            &research_skill(),
            r"read .*authoritative full .*baton\.json.*state-changing|state-changing.*read .*authoritative full .*baton\.json",
            "research skill requires full baton reads before state-changing writes",
        );
    });
}

/// `product.md` contract. Source: shell lines 186-192.
mod product_md_contract {
    use super::*;

    live_tree_test!(documents_compact_baton_state_surfacing, {
        require_match(
            &product_md(),
            r"BATON_STATE_COMPACT",
            "product.md documents compact baton-state surfacing",
        );
    });

    live_tree_test!(no_stale_baton_state_regex_surface, {
        reject_match(
            &product_md(),
            r"BATON_STATE:\s*\{",
            "product.md does not document stale BATON_STATE regex surface",
        );
    });
}

/// Dated research-notes contract. Source: shell lines 194-202.
mod docs_research_contract {
    use super::*;

    live_tree_test!(documents_compact_baton_state_surfacing, {
        for (path, label) in docs_research_files() {
            require_match(
                &path,
                r"BATON_STATE_COMPACT",
                &format!("{label} documents compact baton-state surfacing"),
            );
        }
    });

    live_tree_test!(no_stale_surface_baton_state_wording, {
        for (path, label) in docs_research_files() {
            reject_match(
                &path,
                r"surface\s+BATON_STATE([^_A-Z0-9]|$)",
                &format!("{label} has no stale surface BATON_STATE wording"),
            );
        }
    });
}

/// vadi/SKILL.md-only assertions. Source: shell lines 204-218.
mod vadi_specific_contract {
    use super::*;

    live_tree_test!(treats_human_question_as_resumable_during_discovery, {
        require_match(
            &vadi_skill(),
            r"human_question.*resumable for discovery|resumable for discovery.*human_question",
            "vadi skill treats human_question as resumable during discovery",
        );
    });

    live_tree_test!(does_not_classify_human_question_as_terminal_archive_only, {
        reject_match(
            &vadi_skill(),
            r"only terminal `done`/`human_decision`/`human_question` archives remain, auto-create",
            "vadi skill does not classify human_question as only a terminal archive",
        );
    });

    live_tree_test!(emits_helper_valid_research_phase_fixing_fixbacks, {
        require_match(
            &vadi_skill(),
            r#"Research fixbacks set .*`phase: "research"`.*`status: "research_review"`.*`assignee: "prativadi"`.*`review_target: "research"`"#,
            "vadi skill emits helper-valid research phase_fixing fixbacks",
        );
    });

    live_tree_test!(emits_helper_valid_review_phase_fixing_fixbacks, {
        require_match(
            &vadi_skill(),
            r#"Review fixbacks set .*`phase: "review"`.*`status: "deep_review"`.*`assignee: "prativadi"`.*`review_target: null`"#,
            "vadi skill emits helper-valid review phase_fixing fixbacks",
        );
    });

    live_tree_test!(does_not_keep_phase_spec_for_research_review_fixbacks, {
        reject_match(
            &vadi_skill(),
            r#"Keep the current mode phase \(`<current N>`, `"spec"`, or `"review"`\)"#,
            "vadi skill does not keep phase=spec for research_review fixbacks",
        );
    });
}

/// prativadi/SKILL.md-only assertions. Source: shell lines 220-234.
mod prativadi_specific_contract {
    use super::*;

    live_tree_test!(termination_review_reads_mode_appropriate_artifact, {
        require_match(
            &prativadi_skill(),
            r"mode-appropriate terminal artifact \(`run_explainer_ref`, `research_ref` plus conditional `plan_ref`, or `review_ref`\)",
            "prativadi skill termination review reads the mode-appropriate artifact",
        );
    });

    live_tree_test!(final_ship_rule_is_mode_conditional, {
        require_match(
            &prativadi_skill(),
            r"Development runs require .*run_explainer_ref.*research runs require .*research_ref.*plan_ref.*review runs require .*review_ref",
            "prativadi final ship rule is mode-conditional",
        );
    });

    live_tree_test!(final_ship_rule_requires_both_explainer_reviews, {
        require_match(
            &prativadi_skill(),
            r"run_explainer_reviews.*vadi.*prativadi|vadi.*prativadi.*run_explainer_reviews",
            "prativadi final ship rule requires both explainer reviews",
        );
    });

    live_tree_test!(documents_one_date_explainer_convention, {
        require_match(
            &prativadi_skill(),
            r"one-date run explainer.*YYYY-MM-DD-<run_id>-explainer\.html.*<run_id>-explainer\.html.*never add a second date prefix",
            "prativadi final ship rule documents the one-date explainer convention",
        );
    });

    live_tree_test!(final_ship_rule_is_not_development_artifact_only, {
        reject_match(
            &prativadi_skill(),
            r"A final dark self-contained run explainer exists at `\./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer\.html`",
            "prativadi final ship rule is not development-artifact-only",
        );
    });
}

/// `references/state-transition-table.md` contract. Source: shell lines 236-271.
mod state_ref_contract {
    use super::*;

    // Re-keyed: the old sentence named `scripts/install-dvandva-hooks.sh` as
    // the unconditional installer; that script no longer exists, so the
    // stale phrase to reject now names the ported `dvandva install-hooks`
    // subcommand instead. `core.hooksPath=.githooks` is still the stale
    // value to reject (the post-port target is `.dvandva/githooks`).
    live_tree_test!(no_longer_documents_unconditional_hook_install, {
        reject_match(
            &state_ref(),
            r"role preflight exports and asserts `DVANDVA_ROLE=<role>`,\s*`dvandva install-hooks` sets and verifies `core\.hooksPath=\.githooks`",
            "state reference no longer documents unconditional target-repo hook install",
        );
    });

    live_tree_test!(gates_checkpoint_commits_on_adopted_hooks, {
        require_match(
            &state_ref(),
            r"Checkpoint commits require Dvandva hook adoption",
            "state reference gates checkpoint commits on adopted hooks",
        );
    });

    live_tree_test!(documents_termination_review, {
        require_match(
            &state_ref(),
            r"termination_review",
            "state reference documents termination_review",
        );
    });

    live_tree_test!(routes_final_done_through_termination_review, {
        require_match(
            &state_ref(),
            r"deslop.*termination_review|termination_review.*done",
            "state reference routes final done through termination_review",
        );
    });

    live_tree_test!(requires_both_final_explainer_reviews, {
        require_match(
            &state_ref(),
            r"run_explainer_reviews.*vadi.*prativadi|vadi.*prativadi.*run_explainer_reviews",
            "state reference requires both final explainer reviews",
        );
    });

    live_tree_test!(documents_dvandva_role_ownership_for_explainer_reviews, {
        require_match(
            &state_ref(),
            r"approval and explainer-review ownership|explainer-review ownership|run_explainer_reviews.{0,120}DVANDVA_ROLE.{0,120}ownership|DVANDVA_ROLE.{0,120}run_explainer_reviews.{0,120}ownership",
            "state reference documents DVANDVA_ROLE ownership for explainer reviews",
        );
    });

    live_tree_test!(human_intervention_states_are_paired_run_pauses, {
        require_match(
            &state_ref(),
            r#"human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)"#,
            "state reference says human_question and human_decision are paired run pauses that stop both roles together",
        );
    });

    live_tree_test!(requires_newer_sibling_propagation, {
        require_match(
            &state_ref(),
            r"newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)",
            "state reference requires newer sibling human-intervention propagation",
        );
    });

    live_tree_test!(preserves_sibling_human_question_metadata, {
        require_match(
            &state_ref(),
            r"sibling.{0,160}human_question.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|human_question.{0,160}sibling.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|question.{0,160}resume_assignee.{0,160}resume_status.{0,160}sibling.{0,160}human_question",
            "state reference preserves sibling human_question question and resume metadata",
        );
    });

    live_tree_test!(termination_review_active_and_non_terminal, {
        require_match(
            &state_ref(),
            r"termination_review.*active|active.*termination_review",
            "state reference preserves termination_review as active and non-terminal",
        );
    });

    live_tree_test!(documents_checkpoint_gated_handoff_waits, {
        require_match(
            &state_ref(),
            r"--since-checkpoint",
            "state reference documents checkpoint-gated handoff waits",
        );
    });

    live_tree_test!(documents_action_aware_waits, {
        require_match(
            &state_ref(),
            r"--until-actionable",
            "state reference documents action-aware waits",
        );
    });
}

/// `README.md` contract. Source: shell lines 273-308 (the repo-root
/// `scripts/dvandva-preflight.sh` file-existence check at ~line 289 is
/// dropped; see module doc comment for rationale).
mod readme_contract {
    use super::*;

    // Re-keyed: README must not tell users to run `dvandva install-hooks`
    // directly — the preflight subcommand owns calling it in-process.
    // Narrowed to imperative "run/bash dvandva install-hooks" framing so a
    // CLI-reference table row that merely mentions the subcommand passes.
    live_tree_test!(does_not_instruct_direct_install_hooks_usage, {
        reject_match(
            &readme(),
            r"run\s+.{0,12}dvandva\s+install-hooks|bash\s+.{0,12}dvandva\s+install-hooks",
            "README does not document running dvandva install-hooks directly as a user instruction",
        );
    });

    live_tree_test!(does_not_document_stale_hookspath_value, {
        reject_match(
            &readme(),
            r"core\.hooksPath=\.githooks",
            "README does not document core.hooksPath=.githooks as the adoption target",
        );
    });

    live_tree_test!(does_not_point_at_nonexistent_root_preflight_script, {
        reject_match(
            &readme(),
            r"bash\s+scripts/dvandva-preflight\.sh",
            "README does not point users at a nonexistent root scripts/dvandva-preflight.sh",
        );
    });

    // Re-keyed: the per-role bundled-script path documentation requirement
    // becomes documenting the single canonical `dvandva preflight --role
    // <role>` subcommand invocation.
    live_tree_test!(documents_preflight_role_invocation, {
        require_match(
            &readme(),
            r"dvandva\s+preflight\s+--role\s+(<role>|vadi|prativadi)",
            "README documents the dvandva preflight --role <role> invocation",
        );
    });

    live_tree_test!(documents_multipart_termination_review, {
        require_match(
            &readme(),
            r"termination_review",
            "README documents multipart termination review",
        );
    });

    live_tree_test!(documents_dvandva_role_ownership_for_explainer_reviews, {
        require_match(
            &readme(),
            r"approval and explainer-review ownership|explainer-review ownership|run_explainer_reviews.{0,120}DVANDVA_ROLE.{0,120}ownership|DVANDVA_ROLE.{0,120}run_explainer_reviews.{0,120}ownership",
            "README documents DVANDVA_ROLE ownership for explainer reviews",
        );
    });

    live_tree_test!(documents_action_aware_waits, {
        require_match(
            &readme(),
            r"--until-actionable",
            "README documents action-aware waits",
        );
    });
}

/// Final `product.md` + both `local-baton-channel.md` copies triplet.
/// Source: shell lines 310-330.
mod final_triplet_contract {
    use super::*;

    // Re-keyed: reject direct `dvandva install-hooks` usage instructions
    // (preflight owns calling it in-process), not the deleted shell script.
    // Narrowed to imperative "run/bash dvandva install-hooks" framing so a
    // CLI-reference table row that merely mentions the subcommand passes.
    live_tree_test!(does_not_document_direct_install_hooks_as_role_preflight, {
        for (path, label) in final_triplet_files() {
            reject_match(
                &path,
                r"run\s+.{0,12}dvandva\s+install-hooks|bash\s+.{0,12}dvandva\s+install-hooks",
                &format!("{label} does not document running dvandva install-hooks directly as the role preflight"),
            );
        }
    });

    live_tree_test!(does_not_document_stale_hookspath_value, {
        for (path, label) in final_triplet_files() {
            reject_match(
                &path,
                r"core\.hooksPath=\.githooks",
                &format!(
                    "{label} does not document core.hooksPath=.githooks as the adoption target"
                ),
            );
        }
    });

    live_tree_test!(documents_delegating_wrapper, {
        for (path, label) in final_triplet_files() {
            require_match(
                &path,
                r"\.dvandva/githooks",
                &format!("{label} documents the .dvandva/githooks delegating wrapper"),
            );
        }
    });

    live_tree_test!(documents_final_explainer_review_evidence, {
        for (path, label) in final_triplet_files() {
            require_match(
                &path,
                r"run_explainer_reviews",
                &format!("{label} documents final explainer review evidence"),
            );
        }
    });

    live_tree_test!(documents_checkpoint_gated_handoff_waits, {
        for (path, label) in final_triplet_files() {
            require_match(
                &path,
                r"--since-checkpoint",
                &format!("{label} documents checkpoint-gated handoff waits"),
            );
        }
    });

    live_tree_test!(documents_action_aware_waits, {
        for (path, label) in final_triplet_files() {
            require_match(
                &path,
                r"--until-actionable",
                &format!("{label} documents action-aware waits"),
            );
        }
    });
}

/// F5 human-intervention surfacing contract: the Claude Code-hosted session
/// owns surfacing `human_question`/`human_decision` to the human. The canonical
/// rule sentence is pinned verbatim (no backticks on the two status tokens) so a
/// single substring survives newline flattening in every enforced file — the
/// same needle the `skill-phase3` lint requires in each role skill.
mod f5_human_surfacing_contract {
    use super::*;

    const F5_NEEDLE: &str = r"The Claude Code-hosted session owns surfacing human_question and human_decision to the human";

    live_tree_test!(role_skills_carry_the_f5_needle, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                F5_NEEDLE,
                &format!("{role} skill carries the canonical F5 human-intervention needle"),
            );
        }
    });

    live_tree_test!(command_files_carry_the_f5_needle, {
        for (path, label) in command_files() {
            require_match(
                &path,
                F5_NEEDLE,
                &format!("{label} carries the canonical F5 human-intervention needle"),
            );
        }
    });

    live_tree_test!(readme_documents_the_f5_rule, {
        require_match(
            &readme(),
            F5_NEEDLE,
            "README documents the F5 human-intervention surfacing rule",
        );
    });

    live_tree_test!(product_md_documents_the_f5_rule, {
        require_match(
            &product_md(),
            F5_NEEDLE,
            "product.md documents the F5 human-intervention surfacing rule",
        );
    });

    live_tree_test!(local_baton_channel_documents_the_f5_rule, {
        for (path, label) in local_baton_channel_files() {
            require_match(
                &path,
                F5_NEEDLE,
                &format!("{label} documents the F5 human-intervention surfacing rule"),
            );
        }
    });
}

/// `--through-human` flag contract: Codex-hosted sessions may append it to a
/// wait for zero-touch resumption through a human_question/human_decision
/// pause; Claude Code-hosted sessions must never use it because F5 makes
/// them own surfacing to the human.
mod through_human_flag_contract {
    use super::*;

    live_tree_test!(documents_through_human_zero_touch_resumption, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"--through-human.{0,400}zero-touch resumption|zero-touch resumption.{0,400}--through-human",
                &format!("{role} skill documents --through-human zero-touch resumption"),
            );
        }
    });

    live_tree_test!(claude_hosted_sessions_must_not_use_through_human, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"Claude Code-hosted sessions MUST NOT use `--through-human`",
                &format!(
                    "{role} skill says Claude Code-hosted sessions must not use --through-human"
                ),
            );
        }
    });
}

/// Never-silent-stop contract: a walkaway session never ends its turn
/// mid-run without one of a baton write, an active wait, or a surfaced
/// `human_decision`. The canonical rule sentence is pinned verbatim (no
/// backticks on the status token) so a single substring survives newline
/// flattening — the same needle the `skill-phase3` lint requires in each
/// role skill, mirroring the F5 needle treatment above. Also covers the
/// companion hardening the design calls out alongside it: `--stall-max`
/// becomes required (not optional) in walkaway waits.
mod never_silent_stop_contract {
    use super::*;

    const NEVER_SILENT_STOP_NEEDLE: &str = r"A walkaway session never ends its turn mid-run without one of: a baton write, an active wait, or a surfaced human_decision";

    live_tree_test!(role_skills_carry_the_needle, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                NEVER_SILENT_STOP_NEEDLE,
                &format!("{role} skill carries the never-silent-stop needle"),
            );
        }
    });

    live_tree_test!(product_md_carries_the_needle, {
        require_match(
            &product_md(),
            NEVER_SILENT_STOP_NEEDLE,
            "product.md carries the never-silent-stop needle",
        );
    });

    live_tree_test!(role_skills_require_stall_max_in_walkaway_waits, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"is required in every walkaway wait to arm the dead-peer watchdog",
                &format!("{role} skill makes --stall-max required in walkaway waits"),
            );
        }
    });

    live_tree_test!(role_skills_document_native_notification_channel, {
        for (path, role) in role_skills() {
            require_match(
                &path,
                r"The native Claude Code remote session is the human notification channel",
                &format!("{role} skill documents the native Claude Code remote session as the human notification channel"),
            );
        }
    });

    live_tree_test!(readme_documents_the_watchdog_subcommand, {
        require_match(
            &readme(),
            r"dvandva watchdog",
            "README documents the dvandva watchdog subcommand",
        );
    });
}
