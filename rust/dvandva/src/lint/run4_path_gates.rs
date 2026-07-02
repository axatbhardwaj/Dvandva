//! `lint run4-path-gates` — path-gate and git work-gate contract.
//!
//! RE-KEYED: the shell scripts (`scripts/dvandva-commit-gate.sh`,
//! `scripts/dvandva-drift-lint.sh`, `scripts/install-dvandva-hooks.sh`,
//! `.githooks/*`, both `plugins/.../dvandva-write.sh` copies) are deleted by
//! the port. Their invariants now live in the binary's Rust modules:
//! `rust/dvandva/src/{write,util,hooks,install_hooks,commit_gate,drift_lint}.rs`.
//! The `jq empty` fail-closed JSON check re-keys to `read_json_lenient` (the
//! shared lenient reader), and skills invoke `dvandva preflight --role`.

use std::path::Path;

use crate::lint::protocol_phase1::{
    file_exists, file_matches_ci, file_slurp_matches_ci, resolve_root, union_slurp_matches_ci,
    Report,
};

const WRITE_SOURCES: &[&str] = &["rust/dvandva/src/write.rs", "rust/dvandva/src/util.rs"];

/// Build the run4 path-gate findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    let required = [
        "README.md",
        "product.md",
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/state-transition-table.md",
        "plugins/dvandva/references/baton-schema-v2.json",
        // RE-KEYED: two `dvandva-write.sh` copies -> one write port.
        "rust/dvandva/src/write.rs",
        "plugins/dvandva/skills/vadi/SKILL.md",
        "plugins/dvandva/skills/prativadi/SKILL.md",
        // RE-KEYED: `.githooks/*` + shell hook/gate scripts -> binary modules.
        "rust/dvandva/src/hooks.rs",
        "rust/dvandva/src/install_hooks.rs",
        "rust/dvandva/src/commit_gate.rs",
        "rust/dvandva/src/drift_lint.rs",
    ];
    for rel in required {
        let exists = file_exists(root, rel);
        let msg = if exists {
            format!("{rel} exists")
        } else {
            format!("{rel} is missing")
        };
        r.add(exists, msg);
    }

    // Documented path-gate semantics (docs unchanged by the port).
    r.add(
        file_slurp_matches_ci(
            root,
            "README.md",
            "work_split.*write_paths|write_paths.*work_split",
        ),
        "README.md must document work_split write_paths",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "product.md",
            "safe_rel_path.*work_split|work_split.*safe_rel_path",
        ),
        "product.md must document safe_rel_path work_split path validation",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "docs/protocol/local-baton-channel.md",
            "cross_review.*read-only.*write_paths",
        ),
        "local-baton-channel.md must document cross_review read-only semantics",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "docs/protocol/local-baton-channel.md",
            "write_paths.*supplements.*paths|effective write set.*union",
        ),
        "local-baton-channel.md must document write_paths cannot narrow write-capable paths",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "docs/protocol/local-baton-channel.md",
            "conflict_group.*depends_on|depends_on.*conflict_group",
        ),
        "local-baton-channel.md must document conflict_group/depends_on serialization",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "plugins/dvandva/references/state-transition-table.md",
            "conflict_group.*depends_on|depends_on.*conflict_group",
        ),
        "state-transition-table.md must document conflict_group/depends_on serialization",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "plugins/dvandva/references/state-transition-table.md",
            "terminal historical.*reuse|base_checkpoint.*wave model",
        ),
        "state-transition-table.md must document terminal work_split reuse rationale",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "plugins/dvandva/references/baton-schema-v2.json",
            "write_paths.*conflict_group.*depends_on|depends_on.*conflict_group.*write_paths",
        ),
        "baton-schema-v2.json must expose write_paths/conflict_group/depends_on",
    );

    // RE-KEYED: both write.sh copies validated safe_rel_path + unioned paths;
    // now the single write port does, sharing safe_rel_path from util.rs.
    r.add(
        union_slurp_matches_ci(
            root,
            WRITE_SOURCES,
            "safe_rel_path.*work_split|work_split.*safe_rel_path",
        ),
        "write port must validate work_split paths with safe_rel_path",
    );
    r.add(
        union_slurp_matches_ci(
            root,
            WRITE_SOURCES,
            "paths.*write_paths.*unique|write_paths.*paths.*unique",
        ),
        "write port must union write-capable paths and write_paths",
    );

    // RE-KEYED: `.githooks/pre-commit` delegated to `dvandva-commit-gate.sh`;
    // now the installer materializes a hook dispatching to `dvandva commit-gate`.
    r.add(
        file_matches_ci(root, "rust/dvandva/src/install_hooks.rs", "commit-gate"),
        "hook installer must dispatch pre-commit to dvandva commit-gate",
    );
    // RE-KEYED: `.githooks/prepare-commit-msg` stamped Dvandva-Checkpoint; the
    // materialized hook body lives in hooks.rs.
    r.add(
        file_matches_ci(root, "rust/dvandva/src/hooks.rs", "Dvandva-Checkpoint"),
        "prepare-commit-msg hook must stamp Dvandva-Checkpoint",
    );
    r.add(
        file_matches_ci(root, "rust/dvandva/src/commit_gate.rs", "DVANDVA_ROLE"),
        "commit-gate must enforce DVANDVA_ROLE",
    );
    r.add(
        file_matches_ci(root, "rust/dvandva/src/drift_lint.rs", "Dvandva-Checkpoint"),
        "drift-lint must inspect Dvandva-Checkpoint trailers",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/install_hooks.rs",
            "core\\.hooksPath.*\\.dvandva/githooks|\\.dvandva/githooks.*core\\.hooksPath",
        ),
        "install-hooks must set core.hooksPath to the delegating .dvandva/githooks dir",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/install_hooks.rs",
            "dvandva\\.hooksAdoptedAt",
        ),
        "install-hooks must record hook-adoption baseline",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/drift_lint.rs",
            "dvandva\\.hooksAdoptedAt",
        ),
        "drift-lint must honor hook-adoption baseline",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/install_hooks.rs",
            "__DVANDVA_ROOT_PENDING__",
        ),
        "install-hooks must record pending root baseline for unborn repos",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/drift_lint.rs",
            "__DVANDVA_ROOT_PENDING__.*rev-list|rev-list.*__DVANDVA_ROOT_PENDING__",
        ),
        "drift-lint must backfill pending root baseline",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/drift_lint.rs",
            "hooksAdoptedAtInclusive.*scan_log_shas|scan_log_shas.*hooksAdoptedAtInclusive",
        ),
        "drift-lint must preserve inclusive root-baseline scans",
    );

    // RE-KEYED: skills invoke `dvandva preflight --role` (was `dvandva-preflight.sh`).
    for role in ["vadi", "prativadi"] {
        let skill = format!("plugins/dvandva/skills/{role}/SKILL.md");
        r.add(
            file_slurp_matches_ci(root, &skill, "dvandva preflight"),
            format!("{role} skill preflight must invoke dvandva preflight --role turn-gate"),
        );
        r.add(
            file_slurp_matches_ci(
                root,
                &skill,
                &format!("export[[:space:]]+DVANDVA_ROLE={role}"),
            ),
            format!("{role} skill preflight must export DVANDVA_ROLE={role}"),
        );
        r.add(
            file_slurp_matches_ci(
                root,
                &skill,
                &format!("asserts?[[:space:]]+`?DVANDVA_ROLE={role}`?"),
            ),
            format!("{role} skill preflight must assert DVANDVA_ROLE={role}"),
        );
    }

    // RE-KEYED resolvers: the three shell resolvers (commit-gate, drift-lint,
    // prepare-commit-msg) are now these binary modules. `jq empty` -> the shared
    // lenient reader `read_json_lenient`.
    for rel in [
        "rust/dvandva/src/commit_gate.rs",
        "rust/dvandva/src/drift_lint.rs",
        "rust/dvandva/src/hooks.rs",
    ] {
        r.add(
            file_slurp_matches_ci(
                root,
                rel,
                "\\.dvandva/runs.*baton\\.json|baton\\.json.*\\.dvandva/runs",
            ),
            format!("{rel} must scan run-scoped baton paths"),
        );
        r.add(
            file_slurp_matches_ci(
                root,
                rel,
                "done.*human_question.*human_decision|human_question.*human_decision.*done",
            ),
            format!("{rel} must share terminal baton statuses"),
        );
        r.add(
            file_matches_ci(root, rel, "read_json_lenient"),
            format!("{rel} must fail closed on malformed baton JSON"),
        );
    }

    for rel in [
        "README.md",
        "product.md",
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/state-transition-table.md",
    ] {
        r.add(
            file_slurp_matches_ci(
                root,
                rel,
                "done.*human_question.*human_decision.*inactive|inactive.*done.*human_question.*human_decision",
            ),
            format!("{rel} must document terminal statuses as inactive for git work-gating"),
        );
    }

    r.add(
        file_slurp_matches_ci(
            root,
            "product.md",
            "no daemon.*hidden|hidden.*no daemon|no hidden.*daemon",
        ),
        "product.md must preserve no-daemon/no-hidden-orchestrator contract",
    );

    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}
