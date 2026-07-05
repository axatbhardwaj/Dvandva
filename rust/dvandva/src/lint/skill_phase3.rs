//! `lint skill-phase3` — skill/command loop-text contract, re-keyed to the
//! post-port `dvandva <subcommand>` grammar.

use std::path::Path;

use crate::lint::{file_contains, resolve_root, Report};

/// Build the skill-phase3 findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    for role in ["vadi", "prativadi"] {
        let skill = format!("plugins/dvandva/skills/{role}/SKILL.md");
        let mut req = |needle: &str, label: &str| {
            r.add(
                file_contains(root, &skill, needle),
                format!("{role} {label}"),
            );
        };
        req(
            "Resolve the active baton path before reading or writing",
            "skill resolves active baton path",
        );
        req("DVANDVA_BATON_FILE", "skill supports explicit baton file");
        req("DVANDVA_RUN_DIR", "skill supports explicit run directory");
        req("DVANDVA_RUN_ID", "skill supports run id");
        req(
            ".dvandva/runs/<run_id>/baton.json",
            "skill documents run-scoped baton path",
        );
        req("BATON_FILE", "skill names BATON_FILE variable");
        req("BATON_NEXT_FILE", "skill names BATON_NEXT_FILE variable");
        // RE-KEYED: shell `dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"`
        //           -> binary `dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"`.
        req(
            "dvandva write \"$BATON_FILE\" \"$BATON_NEXT_FILE\"",
            "skill writes through resolved baton path",
        );
        req("original_ask", "skill surfaces original ask");
        req("run_id", "skill surfaces run id");
        req("research_ref", "skill surfaces research ref");
        req(
            "run_explainer_reviews",
            "skill surfaces final explainer reviews",
        );
        req("plan_ref", "skill surfaces plan ref");
        req("turn_cap", "skill surfaces active turn cap");
        req(
            "BATON_STATE: { mode, phase, status, assignee:",
            "BATON_STATE remains structured with mode",
        );
        req("--persist-max <600", "skill documents Claude wait cap");
        req(
            "Codex-hosted sessions may use `--persist`",
            "skill documents Codex persistent wait",
        );
        req("Exit 23", "skill documents persistent cap exit");
        req(
            "Continuous polling is the hard rule",
            "skill makes continuous polling mandatory",
        );
        req(
            "Phase convention: implementation-chunk",
            "skill documents subagent track phase convention",
        );
        // F5: the canonical human-intervention surfacing needle (verbatim, no
        // backticks on the two status tokens so the substring match holds).
        req(
            "The Claude Code-hosted session owns surfacing human_question and human_decision to the human",
            "skill carries the F5 human-intervention surfacing needle",
        );
        // Never-silent-stop: the walkaway turn-boundary invariant (verbatim,
        // no backticks on the status token) — mirrors the F5 needle above.
        req(
            "A walkaway session never ends its turn mid-run without one of: a baton write, an active wait, or a surfaced human_decision",
            "skill carries the never-silent-stop needle",
        );
        // Flow patches: skills must point agents at `dvandva next` for
        // candidate scaffolding before `dvandva write`.
        req(
            "dvandva next",
            "skill documents dvandva next candidate scaffolding",
        );
        req(
            "dvandva:clarifying-questions",
            "skill routes clarifying questions phase",
        );
        req(
            "clarifying_questions_drafting",
            "skill handles clarifying_questions_drafting",
        );
        req(
            "clarifying_questions_answer",
            "skill handles clarifying_questions_answer",
        );
        req(
            "clarifying_questions_followup",
            "skill handles clarifying_questions_followup",
        );
        req(
            "clarifying_questions_followup_answer",
            "skill handles clarifying_questions_followup_answer",
        );
    }

    let vadi = "plugins/dvandva/skills/vadi/SKILL.md";
    r.add(
        file_contains(
            root,
            vadi,
            "Record the user's original ask in the initial baton context",
        ),
        "vadi seeds original ask",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "Do not exit this discovery-wait loop while waiting for baton creation",
        ),
        "vadi keeps missing-baton discovery wait indefinite",
    );
    r.add(
        file_contains(root, vadi, "./superpowers/plans/YYYY-MM-DD-<topic>.html"),
        "vadi writes HTML plan refs",
    );
    r.add(
        !file_contains(root, vadi, "./superpowers/plans/YYYY-MM-DD-<topic>.md"),
        "vadi no longer directs generated plans to markdown",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "Full-profile v2 writes `status: \"test_creation\"`; fast/standard-profile v2 writes `status: \"phase_review\"`",
        ),
        "vadi handoff branches by development profile",
    );
    r.add(
        !file_contains(
            root,
            vadi,
            "status: \"phase_review\" for the legacy v1 helper. In v2, use `status: \"test_creation\"` first",
        ),
        "vadi no longer routes compact profile handoff through full-only gates",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "Development/full fixbacks keep the numeric implementation phase, set `status: \"test_creation\"`",
        ),
        "vadi full fixbacks return through test_creation",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "Development/fast and development/standard fixbacks keep the numeric implementation phase, set `status: \"phase_review\"`",
        ),
        "vadi compact fixbacks return to phase_review",
    );
    r.add(
        !file_contains(
            root,
            vadi,
            "If a fix changes behavior, return through test_creation; do not skip directly to review.",
        ),
        "vadi phase fixing instructions are profile-aware",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "fast` is allowlisted prose-only work with a mandatory `clarifying_questions_drafting -> clarifying_questions_answer -> clarifying_questions_followup -> clarifying_questions_followup_answer -> research_drafting -> research_review -> implementing` prelude",
        ),
        "vadi fast profile documents research prelude",
    );
    r.add(
        file_contains(
            root,
            vadi,
            "For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.",
        ),
        "vadi review_of_review approval uses full-profile deslop",
    );
    r.add(
        !file_contains(
            root,
            vadi,
            "`status: \"implementing\"` (advance) **or** `\"done\"` (terminal)",
        ),
        "vadi review_of_review approval avoids stale v1 direct advance",
    );
    r.add(
        !file_contains(root, vadi, "Approve to advance, or counter-propose."),
        "vadi counter handoff avoids stale approve-to-advance wording",
    );
    r.add(
        !file_contains(root, vadi, "legacy v1 phase implementation"),
        "vadi mode table does not label compact implementing as legacy-only",
    );

    let prat = "plugins/dvandva/skills/prativadi/SKILL.md";
    r.add(
        file_contains(
            root,
            prat,
            "Full-profile v2: `status: \"parallel_implementing\"`, `assignee: \"team\"`, `active_roles: [\"vadi\", \"prativadi\"]`",
        ),
        "prativadi full spec approval ownership is valid",
    );
    r.add(
        file_contains(
            root,
            prat,
            "Fast/standard-profile v2: `status: \"implementing\"`, `assignee: \"vadi\"`, `active_roles: []`",
        ),
        "prativadi compact spec approval ownership is valid",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "assignee: \"team\" for v2, with `active_roles: [\"vadi\", \"prativadi\"]`; legacy v1 uses `\"vadi\"`",
        ),
        "prativadi compact spec approval does not use team owner",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "`assignee: \"team\"` for v2, with `active_roles: [\"vadi\", \"prativadi\"]`; legacy v1 uses `\"vadi\"`",
        ),
        "prativadi compact spec approval does not use backticked team owner",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "Spec approved. Advancing to phase 1 parallel implementation. <total_phases> phases planned.",
        ),
        "prativadi compact spec approval summary is profile-aware",
    );
    r.add(
        file_contains(
            root,
            prat,
            "Fast/standard profiles do not use `review_of_review` narrow-fix branches",
        ),
        "prativadi compact review avoids unsupported narrow-fix branch",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "for development, both explainer review entries present",
        ),
        "prativadi final done gate is profile-aware",
    );
    r.add(
        file_contains(
            root,
            prat,
            "Development/fast: write `phase: 1`, `status: \"implementing\"`, `assignee: \"vadi\"`, and `active_roles: []` so the allowlisted fast path skips spec planning.",
        ),
        "prativadi fast research approval skips spec planning",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "Development or legacy `feature-pr`: write `phase: \"spec\", status: \"spec_drafting\"`",
        ),
        "prativadi research approval is profile-aware",
    );
    r.add(
        file_contains(
            root,
            prat,
            "Full-profile development no-change approval routes to `deslop`; fast/standard compact no-change approval routes through `phase_review -> termination_review` on the final phase or `phase_review -> implementing` for additional work.",
        ),
        "prativadi no-change approval is profile-aware",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "route through deslop before advancement when v2 states are available",
        ),
        "prativadi no-change approval avoids full-only deslop guidance",
    );
    r.add(
        file_contains(
            root,
            prat,
            "Re-read the final diff, verification, and the mode/profile-appropriate terminal evidence",
        ),
        "prativadi termination review starts profile-aware",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "the mode-appropriate terminal artifact (`run_explainer_ref`, `research_ref` plus conditional `plan_ref`, or `review_ref`)",
        ),
        "prativadi termination review does not imply compact run explainer",
    );
    r.add(
        file_contains(
            root,
            prat,
            "For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.",
        ),
        "prativadi counter approval uses full-profile deslop",
    );
    r.add(
        !file_contains(
            root,
            prat,
            "`status: \"implementing\"` on advance, or `\"done\"` on terminal",
        ),
        "prativadi counter approval avoids stale v1 direct advance",
    );
    r.add(
        !file_contains(root, prat, "Approve to advance, or counter."),
        "prativadi fixup handoff avoids stale approve-to-advance wording",
    );
    r.add(
        !file_contains(root, prat, "Approve to advance, or counter again."),
        "prativadi counter loop handoff avoids stale approve-to-advance wording",
    );

    for command in ["vadi", "prativadi"] {
        let path = format!("plugins/dvandva/commands/{command}.md");
        let name = format!("commands/{command}.md");
        r.add(
            file_contains(root, &path, "resolved Dvandva baton"),
            format!("{name} goal refers to resolved baton"),
        );
        r.add(
            file_contains(root, &path, "DVANDVA_RUN_ID"),
            format!("{name} goal mentions run id"),
        );
        r.add(
            file_contains(root, &path, "turn_cap"),
            format!("{name} goal keeps active turn cap"),
        );
        // RE-KEYED: shell-era "do not count shell wait heartbeats as turns" ->
        // "do not count wait heartbeats as turns" (the docs rewrite drops only
        // "shell"; the wait is now `dvandva wait`, not shell). The full phrase
        // is required so an inverted directive (e.g. "count wait heartbeats as
        // turns") cannot satisfy this check by substring coincidence.
        r.add(
            file_contains(root, &path, "do not count wait heartbeats as turns"),
            format!("{name} goal separates waits from active turns"),
        );
        r.add(
            file_contains(root, &path, "continuous polling is the hard rule"),
            format!("{name} goal makes continuous polling mandatory"),
        );
        r.add(
            file_contains(root, &path, "run_explainer_reviews"),
            format!("{name} goal requires final explainer reviews"),
        );
    }

    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}
