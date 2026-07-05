//! `lint protocol-phase1` — protocol/source-doc contract lint, re-keyed to the
//! post-port `dvandva <subcommand>` grammar.
//!
//! The shared lint scaffolding (`Report`/`Finding` plus the file-predicate
//! helpers) that used to live here now lives in `crate::lint` (`mod.rs`),
//! reused by the sibling repo-content lints. The shell scripts derived
//! `ROOT_DIR` from the script location; the Rust ports take an optional
//! repo-root argument and otherwise fall back to the git toplevel of the
//! current directory.

use std::path::Path;

use crate::lint::{file_matches, json_turn_cap_60, resolve_root, Report};

/// Build the protocol-phase1 findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    // product.md — v2 baton contract, research states, profile-aware flow.
    r.add(
        file_matches(root, "product.md", r"dvandva\.baton\.v2"),
        "product spec defines baton v2",
    );
    r.add(
        file_matches(root, "product.md", "run_id"),
        "product spec defines run_id",
    );
    r.add(
        file_matches(root, "product.md", "original_ask"),
        "product spec defines original_ask",
    );
    r.add(
        file_matches(root, "product.md", "research_ref"),
        "product spec defines research_ref",
    );
    r.add(
        file_matches(root, "product.md", "run_explainer_reviews"),
        "product spec defines final explainer reviews",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "research_drafting|research_review|research_revision",
        ),
        "product spec defines research states",
    );
    r.add(
        file_matches(root, "product.md", "clarifying_questions_drafting")
            && file_matches(root, "product.md", "clarifying_questions_answer")
            && file_matches(root, "product.md", "clarifying_questions_followup")
            && file_matches(root, "product.md", "clarifying_questions_followup_answer"),
        "product spec defines clarifying questions prefix states",
    );
    // RE-KEYED: shell `dvandva-wait.sh --persist` -> binary `dvandva wait` + `--persist`.
    r.add(
        file_matches(root, "product.md", "dvandva wait")
            && file_matches(root, "product.md", "--persist"),
        "product spec defines persistent dvandva wait",
    );
    r.add(
        file_matches(root, "product.md", "Continuous polling is the hard rule"),
        "product spec makes continuous polling mandatory",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "generated user-facing artifacts.*HTML|HTML.*generated user-facing artifacts",
        ),
        "product spec scopes HTML migration to generated user-facing artifacts",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            "No multi-baton-per-repo support|One active baton per worktree",
        ),
        "product spec no longer excludes multi-run support",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "Required v2 fields include.*active_roles.*agent_instances",
        ),
        "product v2 field list includes active_roles and agent_instances",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "The full-profile v2 flow has eight segments",
        ),
        "product flow overview is scoped to full profile",
    );
    r.add(
        !file_matches(root, "product.md", "The v2 flow has eight segments"),
        "product flow overview no longer treats full profile as all v2",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "Every completed full-profile v2 development run must produce a one-date explainer",
        ),
        "product artifact policy scopes run explainer to full profile",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            "Every completed v2 development run must produce a one-date explainer",
        ),
        "product artifact policy no longer requires explainers for compact profiles",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "`development` is the delivery run; its separate `profile` field selects",
        ),
        "product mode summary separates delivery mode from lifecycle profile",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            r"`development` is the full research -> planning -> implementation -> review run\.",
        ),
        "product mode summary does not collapse development into full profile",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "For v2 full-profile phase work, approve by writing `phase: 1, status: parallel_implementing",
        ),
        "product prativadi spec approval branches for full profile",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "For v2 fast/standard-profile phase work, approve by writing `phase: 1, status: implementing",
        ),
        "product prativadi spec approval branches for compact profiles",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            "status: \"parallel_implementing\"` for full-profile v2, or `\"implementing\"` for fast/standard-profile v2",
        ),
        "product vadi Mode C recognizes compact v2 implementing",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            "status: \"parallel_implementing\"` for v2, or `\"implementing\"` only for an explicitly selected legacy v1 run",
        ),
        "product vadi Mode C no longer treats implementing as legacy-only",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            "return through `test_creation` rather than directly to review",
        ),
        "product phase fixing instructions are profile-aware",
    );
    r.add(
        !file_matches(root, "product.md", r"Vadi \(implementing phase N\+1\)"),
        "product flow diagram avoids stale sequential v2 implementation wording",
    );
    r.add(
        !file_matches(root, "product.md", r"clean ──▶ phase N\+1"),
        "product overview deslop clean arrow avoids direct done/phase advance ambiguity",
    );
    r.add(
        !file_matches(root, "product.md", r"approve ──▶ phase N\+1"),
        "product overview mutual-review approval arrows route through deslop",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            r"\| `review_of_review \(prativadi_fixups\)` \| final `done` \| Legacy v1 final phase approved by both roles after vadi approves prativadi fixups\.",
        ),
        "product legacy table keeps review_of_review final done row",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            r"\| `counter_review \(vadi_counter\)` \| final `done` \| Legacy v1 final phase approved by both roles after prativadi approves counter\.",
        ),
        "product legacy table keeps counter_review final done row",
    );

    // Both local-baton-channel copies document run-scoped paths + phase convention.
    for file in [
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/local-baton-channel.md",
    ] {
        r.add(
            file_matches(root, file, r"runs/<run_id>|runs/\$|DVANDVA_RUN_ID|run_id"),
            format!("{file} documents run-scoped baton paths"),
        );
        r.add(
            file_matches(root, file, "generated user-facing artifacts|HTML"),
            format!("{file} documents HTML generated artifact policy"),
        );
        r.add(
            file_matches(root, file, "run_explainer_reviews"),
            format!("{file} documents final explainer review evidence"),
        );
        r.add(
            file_matches(root, file, "Continuous polling is the hard rule"),
            format!("{file} makes continuous polling mandatory"),
        );
        r.add(
            file_matches(root, file, "Phase convention: implementation-chunk"),
            format!("{file} documents subagent track phase convention"),
        );
        r.add(
            file_matches(root, file, "clarifying_questions_drafting")
                && file_matches(root, file, "clarifying_questions_followup_answer"),
            format!("{file} documents clarifying questions prefix"),
        );
        r.add(
            file_matches(
                root,
                file,
                "Legacy v1.*`spec_review` → `phase: 1, implementing`|`spec_review` → `phase: 1, implementing`.*Legacy v1",
            ),
            format!("{file} scopes spec_review->implementing as legacy v1"),
        );
        r.add(
            file_matches(
                root,
                file,
                r"v2: `deslop` → `phase: N\+1, parallel_implementing`",
            ),
            format!("{file} routes v2 deslop to parallel_implementing"),
        );
        r.add(
            !file_matches(root, file, r"v2: `deslop` → `phase: N\+1, implementing`"),
            format!("{file} avoids stale v2 deslop->implementing wording"),
        );
        r.add(
            file_matches(root, file, "`research_review` -> `implementing`"),
            format!("{file} fast profile documents research_review->implementing edge"),
        );
    }

    // v2 schema seed + turn_cap seeds.
    let v2 = "plugins/dvandva/references/baton-schema-v2.json";
    r.add(
        file_matches(root, v2, r#""schema": "dvandva\.baton\.v2""#),
        "v2 schema seed declares dvandva.baton.v2",
    );
    r.add(
        file_matches(root, v2, r#""run_id""#),
        "v2 schema seed includes run_id",
    );
    r.add(
        file_matches(root, v2, r#""original_ask""#),
        "v2 schema seed includes original_ask",
    );
    r.add(
        file_matches(root, v2, r#""research_ref""#),
        "v2 schema seed includes research_ref",
    );
    r.add(
        file_matches(root, v2, r#""clarifying_questions_drafting""#)
            && file_matches(root, v2, r#""clarifying_questions_followup_answer""#),
        "v2 schema seed includes clarifying status catalog entries",
    );
    r.add(
        file_matches(root, v2, r#""run_explainer_reviews""#),
        "v2 schema seed includes final explainer review records",
    );
    r.add(
        json_turn_cap_60(root, "plugins/dvandva/references/baton-schema.json"),
        "v1 plugin schema seed uses turn_cap 60",
    );
    r.add(
        json_turn_cap_60(root, "templates/channel/baton.json"),
        "channel template seed uses turn_cap 60",
    );
    r.add(
        json_turn_cap_60(root, v2),
        "v2 schema seed uses turn_cap 60",
    );
    r.add(
        !file_matches(
            root,
            "product.md",
            "extended v1 seed|legacy v1 default 20|Legacy v1 defaults to 20",
        ),
        "product spec no longer mentions stale v1 turn_cap seed/default wording",
    );

    // state-transition-table.md — v2 + research states + legacy-scoped rows.
    let stt = "plugins/dvandva/references/state-transition-table.md";
    r.add(
        file_matches(root, stt, r"dvandva\.baton\.v2"),
        "transition table documents baton v2",
    );
    r.add(
        file_matches(
            root,
            stt,
            "research_drafting|research_review|research_revision",
        ),
        "transition table documents research states",
    );
    r.add(
        file_matches(root, stt, "clarifying_questions_drafting")
            && file_matches(root, stt, "clarifying_questions_answer")
            && file_matches(root, stt, "clarifying_questions_followup")
            && file_matches(root, stt, "clarifying_questions_followup_answer"),
        "transition table documents clarifying questions prefix states",
    );
    r.add(
        file_matches(root, stt, "run_explainer_reviews"),
        "transition table documents final explainer review gate",
    );
    r.add(
        file_matches(
            root,
            "product.md",
            r"\| `research_review` \| `implementing` \| Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation\.",
        ),
        "product fast profile documents research_review->implementing edge",
    );
    r.add(
        file_matches(
            root,
            stt,
            r"\| `research_review` \| `implementing` \| Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation\.",
        ),
        "transition table fast profile documents research_review->implementing edge",
    );
    r.add(
        file_matches(
            root,
            stt,
            r"\| `phase_review \(impl\)` \| `phase: N\+1, status: implementing, disagreement_round: 0` \| Legacy v1:",
        ),
        "transition table scopes phase_review advancement as legacy v1",
    );
    r.add(
        file_matches(
            root,
            stt,
            r"\| `review_of_review \(prativadi_fixups\)` \| `phase: N\+1, status: implementing, disagreement_round: 0` \| Legacy v1:",
        ),
        "transition table scopes review_of_review advancement as legacy v1",
    );
    r.add(
        file_matches(
            root,
            stt,
            r"\| `counter_review \(vadi_counter\)` \| `phase: N\+1, status: implementing, disagreement_round: 0` \| Legacy v1:",
        ),
        "transition table scopes counter_review advancement as legacy v1",
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
