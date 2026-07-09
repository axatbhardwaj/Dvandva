//! `lint phase4-research` — research/subagent workflow contract, and the
//! in-process aggregator.
//!
//! RE-KEYED:
//! * The README "full validation" command list (`bash scripts/lint-*.sh`,
//!   `bash scripts/test-*.sh`, `bash scripts/smoke-*.sh`) becomes the Rust
//!   definition-of-done gate `cargo fmt --check && cargo clippy --all-targets
//!   -- -D warnings && cargo test`; the `claude plugin validate` steps survive.
//! * `scripts/smoke-plugin-install.sh` -> `rust/dvandva/src/smoke.rs`.
//! * `install-codex.sh` -> `dvandva install-codex` in the product smoke-probe
//!   sentence.
//! * The shell aggregator ran sibling test suites via `bash`; the Rust
//!   aggregator instead invokes the sibling lint `run()` functions in-process
//!   (protocol-phase1, skill-phase3, artifacts). It does NOT run `cargo test`
//!   or the smoke — those are separate definition-of-done gates.

use std::path::Path;

use crate::lint::{
    count_lines_matching, file_contains, file_has_exact_line, file_slurp_matches_ci, list_md,
    output_contract_contains, resolve_root, Report, MODEL_POLICY_CLAUDE_MAPPING,
    MODEL_POLICY_CODEX_EFFORT, MODEL_POLICY_CODEX_MAPPING, MODEL_POLICY_NO_HAIKU_COMMANDS,
    MODEL_POLICY_NO_HAIKU_SUBAGENTS, MODEL_POLICY_OPUS_ROUTING, MODEL_POLICY_SONNET_ROUTING,
    MODEL_POLICY_STALE_CANONICAL_COMPAT_MAPPING, MODEL_POLICY_STALE_CODEX_MAPPING,
    MODEL_POLICY_STALE_OPUS_ROUTING, MODEL_POLICY_STALE_SONNET_ROUTING,
    MODEL_POLICY_VENDOR_NEUTRAL_COMMANDS, MODEL_POLICY_VENDOR_NEUTRAL_DOCS,
};

const ALL_AGENTS: [&str; 15] = [
    "researcher",
    "architect",
    "implementer",
    "test-creator",
    "cross-reviewer",
    "adversarial-analyst",
    "deep-reviewer",
    "deslopper",
    "sandbox-verifier",
    "baton-auditor",
    "security-auditor",
    "integration-checker",
    "debugger",
    "doc-verifier",
    "pattern-mapper",
];
const NEW_AGENTS: [&str; 5] = [
    "security-auditor",
    "integration-checker",
    "debugger",
    "doc-verifier",
    "pattern-mapper",
];
/// The enforced model-class vocabulary (D2): every agent frontmatter `model:`
/// line must be one of these four vendor-neutral classes. `opus`/`sonnet` are
/// the seed classes; `fable` (frontier) and `gpt` (bulk-mechanical) are legal
/// for future agents. Seed re-tiering is separately blocked by the exact-value
/// pins in the OPUS_AGENTS/SONNET_AGENTS loops.
const VALID_MODEL_CLASSES: [&str; 4] = ["opus", "sonnet", "fable", "gpt"];
const OPUS_AGENTS: [&str; 7] = [
    "adversarial-analyst",
    "architect",
    "baton-auditor",
    "deep-reviewer",
    "doc-verifier",
    "integration-checker",
    "security-auditor",
];
const SONNET_AGENTS: [&str; 8] = [
    "cross-reviewer",
    "debugger",
    "deslopper",
    "implementer",
    "pattern-mapper",
    "researcher",
    "sandbox-verifier",
    "test-creator",
];
const DOWNSTREAM: [&str; 6] = [
    "researcher",
    "architect",
    "implementer",
    "test-creator",
    "deslopper",
    "pattern-mapper",
];
const ADVERSARIAL: [&str; 8] = [
    "cross-reviewer",
    "adversarial-analyst",
    "deep-reviewer",
    "sandbox-verifier",
    "baton-auditor",
    "security-auditor",
    "integration-checker",
    "doc-verifier",
];
const MODEL_POLICY_SEED_LEGACY_CAVEAT: &str = "Seed-roster class vocabulary keeps these legacy routing needles, but they are not the concrete ring dispatch rule";
const MODEL_POLICY_RING_DEFAULTS: &str =
    "Implementation, tests, and fixes default to gpt-class dispatch";
const MODEL_POLICY_GPT_SELF_REVIEW_NO_CREDIT: &str =
    "GPT self-review is hygiene only and earns no review credit";
const MODEL_POLICY_GROK_UNCREDITED: &str =
    "A Grok lane may run only as read-only, uncredited triage for live-world/plan-pulse or first-pass review leads";
const MODEL_POLICY_FABLE_NO_CODE: &str =
    "Fable-class owns plan authorship and terminal adjudication only, never code";

fn req(r: &mut Report, root: &Path, rel: &str, needle: &str, msg: impl Into<String>) {
    r.add(file_contains(root, rel, needle), msg);
}

fn rej(r: &mut Report, root: &Path, rel: &str, needle: &str, msg: impl Into<String>) {
    r.add(!file_contains(root, rel, needle), msg);
}

fn req_model_policy_routing(r: &mut Report, root: &Path, rel: &str) {
    req(
        r,
        root,
        rel,
        MODEL_POLICY_CODEX_EFFORT,
        format!("{rel} documents Codex effort-tier guidance"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_OPUS_ROUTING,
        format!("{rel} documents opus workload routing"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_SONNET_ROUTING,
        format!("{rel} documents sonnet workload routing"),
    );
    rej(
        r,
        root,
        rel,
        MODEL_POLICY_STALE_OPUS_ROUTING,
        format!("{rel} avoids stale broad opus workload wording"),
    );
    rej(
        r,
        root,
        rel,
        MODEL_POLICY_STALE_SONNET_ROUTING,
        format!("{rel} avoids stale broad sonnet workload wording"),
    );
    rej(
        r,
        root,
        rel,
        MODEL_POLICY_STALE_CODEX_MAPPING,
        format!("{rel} avoids retired Codex gpt-5.4 mapping"),
    );
    rej(
        r,
        root,
        rel,
        MODEL_POLICY_STALE_CANONICAL_COMPAT_MAPPING,
        format!("{rel} avoids retired canonical compatibility mapping"),
    );
}

fn req_model_policy_common(r: &mut Report, root: &Path, rel: &str, vendor_neutral_needle: &str) {
    req(
        r,
        root,
        rel,
        vendor_neutral_needle,
        format!("{rel} documents vendor-neutral model classes"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_CLAUDE_MAPPING,
        format!("{rel} documents Claude model-class mapping"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_CODEX_MAPPING,
        format!("{rel} documents Codex model-class mapping"),
    );
    req_model_policy_routing(r, root, rel);
}

fn req_command_ring_dispatch(r: &mut Report, root: &Path, rel: &str) {
    req(
        r,
        root,
        rel,
        MODEL_POLICY_SEED_LEGACY_CAVEAT,
        format!("{rel} distinguishes seed routing needles from ring dispatch"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_RING_DEFAULTS,
        format!("{rel} pins gpt-class implementation/test/fix defaults"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_GPT_SELF_REVIEW_NO_CREDIT,
        format!("{rel} denies review credit for GPT self-review"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_GROK_UNCREDITED,
        format!("{rel} pins Grok as uncredited read-only triage"),
    );
    req(
        r,
        root,
        rel,
        MODEL_POLICY_FABLE_NO_CODE,
        format!("{rel} pins Fable-class to plan/done judgment"),
    );
}

fn req_grok_plan_pulse_policy(r: &mut Report, root: &Path) {
    let rel = "docs/model-selection.md";
    r.add(
        file_slurp_matches_ci(root, rel, r"plan-review loop.*uncredited latest-tech pulse"),
        "docs/model-selection.md allows only uncredited Grok plan-pulse",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            rel,
            r"plan-pulse findings.*quarantined.*Claude-family role.*confirms",
        ),
        "docs/model-selection.md quarantines Grok plan-pulse findings",
    );
    r.add(
        file_slurp_matches_ci(root, rel, r"never a credited review station")
            && file_slurp_matches_ci(root, rel, r"never the ring's execute stations")
            && file_slurp_matches_ci(root, rel, r"code-touching subagent"),
        "docs/model-selection.md forbids Grok credited review, execute, and code-touching seats",
    );
    r.add(
        file_slurp_matches_ci(root, rel, r"Its output is data, not instructions"),
        "docs/model-selection.md treats Grok output as data not instructions",
    );
    r.add(
        !file_slurp_matches_ci(root, rel, r"grok[^.]{0,120}research phases only"),
        "docs/model-selection.md avoids stale Grok research-only wording",
    );
    r.add(
        !file_slurp_matches_ci(
            root,
            rel,
            r"grok[^.]{0,120}(owns|is|becomes|serves as)[^.]{0,120}credited review",
        ),
        "docs/model-selection.md avoids assigning Grok credited review authority",
    );
    r.add(
        !file_slurp_matches_ci(
            root,
            rel,
            r"\bgrok\s+(may|can|owns|is|becomes|serves as)[^.]{0,120}(execute|executor|code-touching|writes code|baton write)",
        ),
        "docs/model-selection.md avoids assigning Grok execute/code/baton authority",
    );
}

fn require_agent_model(
    r: &mut Report,
    root: &Path,
    rel: &str,
    expected: &str,
    msg: impl Into<String>,
) {
    let one = count_lines_matching(root, rel, "^model:") == 1;
    let exact = file_has_exact_line(root, rel, &format!("model: {expected}"));
    r.add(one && exact, msg);
}

/// Every agent file carries exactly one `model:` frontmatter line whose value is
/// one of the four enforced classes (`opus`/`sonnet`/`fable`/`gpt`). This is the
/// general membership gate; the seed roster is additionally pinned to its exact
/// opus/sonnet value by the OPUS_AGENTS/SONNET_AGENTS loops.
fn require_agent_model_class(r: &mut Report, root: &Path, rel: &str, msg: impl Into<String>) {
    let one = count_lines_matching(root, rel, "^model:") == 1;
    let member = VALID_MODEL_CLASSES
        .iter()
        .any(|class| file_has_exact_line(root, rel, &format!("model: {class}")));
    r.add(one && member, msg);
}

/// phase4-research's OWN content checks for a repo root (no chaining).
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    let research = "plugins/dvandva/skills/research/SKILL.md";
    req_grok_plan_pulse_policy(&mut r, root);
    req(
        &mut r,
        root,
        research,
        "name: research",
        "research skill has plugin-local name",
    );
    req(
        &mut r,
        root,
        research,
        "description: Use when",
        "research skill has trigger-only description",
    );
    req(
        &mut r,
        root,
        research,
        "original_ask",
        "research skill preserves original ask",
    );
    req(
        &mut r,
        root,
        research,
        "research_ref",
        "research skill produces research_ref",
    );
    req(
        &mut r,
        root,
        research,
        "run_explainer_reviews",
        "research skill preserves final explainer review records",
    );
    req(
        &mut r,
        root,
        research,
        "./superpowers/research/YYYY-MM-DD-<topic>.html",
        "research skill writes generated HTML research artifact",
    );
    req(
        &mut r,
        root,
        research,
        "work_split",
        "research skill records work split",
    );
    req(
        &mut r,
        root,
        research,
        "verification_matrix",
        "research skill records verification matrix",
    );
    req(
        &mut r,
        root,
        research,
        "100% test coverage",
        "research skill requires full coverage planning",
    );
    req(
        &mut r,
        root,
        research,
        "test creation is separate from review",
        "research skill separates testing and review",
    );
    req(
        &mut r,
        root,
        research,
        "deep_review",
        "research skill includes deep review loop",
    );
    req(
        &mut r,
        root,
        research,
        "deslop",
        "research skill includes de-slop pass",
    );
    req(
        &mut r,
        root,
        research,
        "parallel subagents",
        "research skill requires parallel subagents",
    );
    req(
        &mut r,
        root,
        research,
        "research_review",
        "research skill documents prativadi review",
    );
    req(
        &mut r,
        root,
        research,
        "research_revision",
        "research skill documents revision loop",
    );
    req(
        &mut r,
        root,
        research,
        "generated user-facing HTML artifact",
        "research skill follows HTML artifact policy",
    );
    req(
        &mut r,
        root,
        research,
        "dark self-contained HTML",
        "research skill requires dark HTML",
    );
    req(
        &mut r,
        root,
        research,
        "machine-readable metadata",
        "research skill requires machine-readable metadata",
    );
    req(&mut r, root, research, "If no subagent tool is available, do the same exploration directly and record the fallback in work_split.", "research skill records subagent fallback");
    req(
        &mut r,
        root,
        research,
        "Do not rely solely on the vadi's research_ref",
        "research skill requires independent prativadi review",
    );
    rej(
        &mut r,
        root,
        research,
        "./superpowers/research/YYYY-MM-DD-<topic>.md",
        "research skill rejects generated markdown research artifacts",
    );

    for file in [
        "README.md",
        "product.md",
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/local-baton-channel.md",
        "plugins/dvandva/skills/vadi/SKILL.md",
        "plugins/dvandva/skills/prativadi/SKILL.md",
        "plugins/dvandva/commands/vadi.md",
        "plugins/dvandva/commands/prativadi.md",
    ] {
        req(
            &mut r,
            root,
            file,
            "Superpowers is a hard runtime dependency",
            format!("{file} requires Superpowers at runtime"),
        );
        req(
            &mut r,
            root,
            file,
            "Dvandva owns baton state",
            format!("{file} separates Dvandva coordination from Superpowers work discipline"),
        );
    }

    for role in ["vadi", "prativadi"] {
        let skill = format!("plugins/dvandva/skills/{role}/SKILL.md");
        req(
            &mut r,
            root,
            &skill,
            "Invoke `dvandva:research`",
            format!("{role} invokes shared research skill"),
        );
        req(
            &mut r,
            root,
            &skill,
            "clarifying_questions_drafting",
            format!("{role} handles clarifying_questions_drafting before research"),
        );
        req(
            &mut r,
            root,
            &skill,
            "research_drafting",
            format!("{role} handles research_drafting"),
        );
        req(
            &mut r,
            root,
            &skill,
            "research_review",
            format!("{role} handles research_review"),
        );
        req(
            &mut r,
            root,
            &skill,
            "research_revision",
            format!("{role} handles research_revision"),
        );
        req(
            &mut r,
            root,
            &skill,
            "work_split",
            format!("{role} surfaces work split"),
        );
        req(
            &mut r,
            root,
            &skill,
            "verification_matrix",
            format!("{role} surfaces verification matrix"),
        );
        req(
            &mut r,
            root,
            &skill,
            "100% test coverage",
            format!("{role} requires full coverage planning"),
        );
        req(
            &mut r,
            root,
            &skill,
            "test_creation",
            format!("{role} separates test creation"),
        );
        req(
            &mut r,
            root,
            &skill,
            "deep_review",
            format!("{role} includes deep review"),
        );
        req(
            &mut r,
            root,
            &skill,
            "deslop",
            format!("{role} includes de-slop pass"),
        );
        req(
            &mut r,
            root,
            &skill,
            "parallel subagents",
            format!("{role} requires parallel subagents"),
        );
        req(
            &mut r,
            root,
            &skill,
            "canonical Dvandva subagent roster",
            format!("{role} uses canonical subagent roster"),
        );
        req(
            &mut r,
            root,
            &skill,
            "all phases are subagent-driven",
            format!("{role} makes all phases subagent-driven"),
        );
        req(
            &mut r,
            root,
            &skill,
            "independent research review",
            format!("{role} supports independent research review"),
        );
        req(
            &mut r,
            root,
            &skill,
            "BATON_BROKEN_FILE=\"$BATON_DIR/baton.broken.json\"",
            format!("{role} defines broken-baton path"),
        );
        req(
            &mut r,
            root,
            &skill,
            "Write `$BATON_BROKEN_FILE` preserving",
            format!("{role} uses broken-baton path"),
        );
        req(
            &mut r,
            root,
            &skill,
            "write-helper validation exit 23",
            format!("{role} disambiguates write exit 23"),
        );
        req(
            &mut r,
            root,
            &skill,
            "wait-helper persist cap exit 23",
            format!("{role} disambiguates wait exit 23"),
        );
        req(
            &mut r,
            root,
            &skill,
            "dvandva.baton.v1` or `dvandva.baton.v2",
            format!("{role} accepts supported v1/v2 baton schemas"),
        );
        req(
            &mut r,
            root,
            &skill,
            "Regular checkpoint commits",
            format!("{role} documents regular checkpoint commits"),
        );
        req(
            &mut r,
            root,
            &skill,
            "conditional parallelism",
            format!("{role} documents conditional parallelism"),
        );
        req(
            &mut r,
            root,
            &skill,
            "parallelize only genuinely disjoint tracks",
            format!("{role} avoids fake subagent parallelism"),
        );
        req(
            &mut r,
            root,
            &skill,
            "record what was not parallelized and why",
            format!("{role} records non-parallelized work"),
        );
        req(
            &mut r,
            root,
            &skill,
            "two-team parallel implementation",
            format!("{role} requires two-team implementation"),
        );
        req(
            &mut r,
            root,
            &skill,
            "cross-review",
            format!("{role} requires cross-review"),
        );
        req(
            &mut r,
            root,
            &skill,
            "implementation-phase parallelism is mandatory",
            format!("{role} requires mandatory implementation parallelism"),
        );
        req(
            &mut r,
            root,
            &skill,
            "Phase convention: implementation-chunk",
            format!("{role} documents subagent track phase convention"),
        );
        req(
            &mut r,
            root,
            &skill,
            "same-status sync checkpoints",
            format!("{role} documents team sync checkpoints"),
        );
        req(
            &mut r,
            root,
            &skill,
            "subagent_tracks",
            format!("{role} records subagent tracks in baton evidence"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "until the v2 write-helper phase lands",
            format!("{role} does not reference dangling v2 phase"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "equals `dvandva.baton.v1` in this helper version",
            format!("{role} does not reject live v2 schema"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "Phase 6 includes v2 write-helper enforcement; until then",
            format!("{role} does not describe v2 enforcement as future-only"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "once Phase 6 includes v2 write-helper enforcement",
            format!("{role} does not describe v2 enforcement as future-only (2)"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "21/22/23: fix the candidate file and re-run",
            format!("{role} does not group exit 23 ambiguously"),
        );
        rej(
            &mut r,
            root,
            &skill,
            "proceed even without superpowers",
            format!("{role} does not allow Superpowers-free active work"),
        );
    }

    let vadi = "plugins/dvandva/skills/vadi/SKILL.md";
    req(
        &mut r,
        root,
        vadi,
        "BATON_BROKEN_FILE",
        "vadi defines broken-baton path symmetrically",
    );
    req(
        &mut r,
        root,
        vadi,
        "Existing baton discovery",
        "vadi documents existing-baton discovery",
    );
    req(
        &mut r,
        root,
        vadi,
        ".dvandva/runs/*/baton.json",
        "vadi scans named run batons",
    );
    req(
        &mut r,
        root,
        vadi,
        "auto-create a new named run",
        "vadi auto-creates new run when only terminal batons exist",
    );
    req(
        &mut r,
        root,
        vadi,
        "ask the user whether to continue",
        "vadi asks before reusing active batons",
    );
    rej(
        &mut r,
        root,
        vadi,
        "Write `$BATON_DIR/baton.broken.json`",
        "vadi avoids hardcoded broken-baton path",
    );

    for command in ["vadi", "prativadi"] {
        let path = format!("plugins/dvandva/commands/{command}.md");
        let name = format!("commands/{command}.md");
        req(
            &mut r,
            root,
            &path,
            "research_ref",
            format!("{name} goal includes research_ref"),
        );
        req(
            &mut r,
            root,
            &path,
            "work_split",
            format!("{name} goal includes work_split"),
        );
        req(
            &mut r,
            root,
            &path,
            "verification_matrix",
            format!("{name} goal includes verification_matrix"),
        );
        req(
            &mut r,
            root,
            &path,
            "test_creation",
            format!("{name} goal separates test creation"),
        );
        req(
            &mut r,
            root,
            &path,
            "deep_review",
            format!("{name} goal includes deep review"),
        );
        req(
            &mut r,
            root,
            &path,
            "deslop",
            format!("{name} goal includes de-slop pass"),
        );
        req(
            &mut r,
            root,
            &path,
            "parallel subagents",
            format!("{name} goal includes subagent parallelism"),
        );
        req(
            &mut r,
            root,
            &path,
            "conditional parallelism",
            format!("{name} goal includes conditional parallelism"),
        );
        req(
            &mut r,
            root,
            &path,
            "subagent_tracks",
            format!("{name} goal records subagent tracks"),
        );
        req(
            &mut r,
            root,
            &path,
            "Invoke `dvandva:research`",
            format!("{name} goal invokes research skill"),
        );
        req(
            &mut r,
            root,
            &path,
            "regular local checkpoint commits",
            format!("{name} goal includes regular checkpoint commits"),
        );
    }

    for file in [
        "product.md",
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/local-baton-channel.md",
        "plugins/dvandva/references/state-transition-table.md",
    ] {
        req(
            &mut r,
            root,
            file,
            "clarifying_questions_drafting",
            format!("{file} documents clarifying questions before research"),
        );
        req(
            &mut r,
            root,
            file,
            "work_split",
            format!("{file} documents work split"),
        );
        req(
            &mut r,
            root,
            file,
            "verification_matrix",
            format!("{file} documents verification matrix"),
        );
        req(
            &mut r,
            root,
            file,
            "100% test coverage",
            format!("{file} documents full coverage target"),
        );
        req(
            &mut r,
            root,
            file,
            "test_creation",
            format!("{file} documents separate test creation"),
        );
        req(
            &mut r,
            root,
            file,
            "deep_review",
            format!("{file} documents deep review loop"),
        );
        req(
            &mut r,
            root,
            file,
            "deslop",
            format!("{file} documents de-slop pass"),
        );
        req(
            &mut r,
            root,
            file,
            "Regular checkpoint commits",
            format!("{file} documents regular checkpoint commits"),
        );
        req(
            &mut r,
            root,
            file,
            "conditional parallelism",
            format!("{file} documents conditional parallelism"),
        );
        req(
            &mut r,
            root,
            file,
            "two-team parallel implementation",
            format!("{file} documents two-team implementation"),
        );
        req(
            &mut r,
            root,
            file,
            "cross-review",
            format!("{file} documents cross-review"),
        );
        req(
            &mut r,
            root,
            file,
            "implementation-phase parallelism is mandatory",
            format!("{file} documents mandatory implementation parallelism"),
        );
        req(
            &mut r,
            root,
            file,
            "Phase convention: implementation-chunk",
            format!("{file} documents subagent track phase convention"),
        );
        req(
            &mut r,
            root,
            file,
            "same-status sync checkpoints",
            format!("{file} documents team sync checkpoints"),
        );
        req(
            &mut r,
            root,
            file,
            "subagent_tracks",
            format!("{file} documents subagent track evidence"),
        );
        req(
            &mut r,
            root,
            file,
            "run_explainer_ref",
            format!("{file} documents final run explainer"),
        );
        req(
            &mut r,
            root,
            file,
            "run_explainer_reviews",
            format!("{file} documents final run explainer reviews"),
        );
        req(
            &mut r,
            root,
            file,
            "v2 write-helper enforcement",
            format!("{file} documents v2 enforcement"),
        );
        req(
            &mut r,
            root,
            file,
            "wait-helper persist cap exit 23",
            format!("{file} disambiguates wait exit 23"),
        );
        req(
            &mut r,
            root,
            file,
            "write-helper validation exit 23",
            format!("{file} disambiguates write exit 23"),
        );
    }

    let readme = "README.md";
    req(
        &mut r,
        root,
        readme,
        "regular local checkpoint commits",
        "README documents regular checkpoint commits",
    );
    req(&mut r, root, readme, "dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup", "README documents all installed Dvandva skills");
    req(
        &mut r,
        root,
        readme,
        "all six Dvandva skills",
        "README validation describes all six Dvandva skills",
    );
    rej(
        &mut r,
        root,
        readme,
        "all five Dvandva skills",
        "README avoids stale five-skill validation wording",
    );
    rej(
        &mut r,
        root,
        readme,
        "both Dvandva skills",
        "README avoids stale two-skill install wording",
    );
    rej(
        &mut r,
        root,
        readme,
        "Agents may commit and push only after both",
        "README no longer says commits are final-only",
    );
    // RE-KEYED: shell `bash scripts/*.sh` validation list -> Rust DoD gate.
    req(
        &mut r,
        root,
        readme,
        "cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test",
        "README documents the Rust definition-of-done gate",
    );
    req(
        &mut r,
        root,
        readme,
        "claude plugin validate plugins/dvandva",
        "README full validation includes claude plugin validate plugins/dvandva",
    );
    req(
        &mut r,
        root,
        readme,
        "claude plugin validate .",
        "README full validation includes claude plugin validate .",
    );

    let schema = "plugins/dvandva/references/baton-schema-v2.json";
    req(
        &mut r,
        root,
        schema,
        "\"work_split\"",
        "v2 schema includes work_split",
    );
    req(
        &mut r,
        root,
        schema,
        "\"verification_matrix\"",
        "v2 schema includes verification_matrix",
    );
    req(
        &mut r,
        root,
        schema,
        "\"run_explainer_ref\"",
        "v2 schema includes final explainer ref",
    );
    req(
        &mut r,
        root,
        schema,
        "\"run_explainer_reviews\"",
        "v2 schema includes final explainer reviews",
    );
    req(
        &mut r,
        root,
        schema,
        "\"active_roles\"",
        "v2 schema includes active roles",
    );
    req(
        &mut r,
        root,
        schema,
        "\"parallel_implementing\"",
        "v2 schema includes parallel implementation status",
    );
    req(
        &mut r,
        root,
        schema,
        "\"test_creation\"",
        "v2 schema includes test creation status",
    );
    req(
        &mut r,
        root,
        schema,
        "\"cross_review\"",
        "v2 schema includes cross-review status",
    );
    req(
        &mut r,
        root,
        schema,
        "\"cross_fixing\"",
        "v2 schema includes cross-fixing status",
    );
    req(
        &mut r,
        root,
        schema,
        "\"deep_review\"",
        "v2 schema includes deep review status",
    );
    req(
        &mut r,
        root,
        schema,
        "\"deslop\"",
        "v2 schema includes de-slop status",
    );
    rej(
        &mut r,
        root,
        schema,
        "\"id\": \"deep_review-security\"",
        "v2 seed does not make security auditor mandatory",
    );
    rej(
        &mut r,
        root,
        schema,
        "\"id\": \"deep_review-integration\"",
        "v2 seed does not make integration checker mandatory",
    );
    rej(
        &mut r,
        root,
        schema,
        "\"id\": \"deep_review-doc-verification\"",
        "v2 seed does not make doc verifier mandatory",
    );
    rej(
        &mut r,
        root,
        schema,
        "\"id\": \"phase_fixing-debug\"",
        "v2 seed does not make debugger mandatory",
    );
    rej(
        &mut r,
        root,
        schema,
        "\"id\": \"research-pattern-mapping\"",
        "v2 seed does not make pattern mapper mandatory",
    );

    for agent in ALL_AGENTS {
        let file = format!("plugins/dvandva/agents/{agent}.md");
        req(
            &mut r,
            root,
            &file,
            &format!("name: dvandva-{agent}"),
            format!("agent {agent} has Dvandva name"),
        );
        req(
            &mut r,
            root,
            &file,
            "description: Use",
            format!("agent {agent} has trigger-focused description"),
        );
        rej(
            &mut r,
            root,
            &file,
            "model: haiku",
            format!("agent {agent} rejects haiku model class"),
        );
        req(
            &mut r,
            root,
            &file,
            "phase:",
            format!("agent {agent} declares phase ownership"),
        );
        req(
            &mut r,
            root,
            &file,
            "tools:",
            format!("agent {agent} declares explicit tool scope"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Mission",
            format!("agent {agent} declares a mission"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Use When",
            format!("agent {agent} declares triggers"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Required Inputs",
            format!("agent {agent} declares required inputs"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Operating Loop",
            format!("agent {agent} declares operating loop"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Output Contract",
            format!("agent {agent} declares output contract"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Evidence Rules",
            format!("agent {agent} declares evidence rules"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Guardrails",
            format!("agent {agent} declares guardrails"),
        );
        req(
            &mut r,
            root,
            &file,
            "## Common Failures",
            format!("agent {agent} declares common failures"),
        );
        req(
            &mut r,
            root,
            &file,
            "work_split",
            format!("agent {agent} reports work_split"),
        );
        req(
            &mut r,
            root,
            &file,
            "verification_matrix",
            format!("agent {agent} reports verification_matrix"),
        );
        req(
            &mut r,
            root,
            &file,
            "subagent_tracks",
            format!("agent {agent} reports subagent track evidence"),
        );
        rej(
            &mut r,
            root,
            &file,
            "not an orchestrator",
            format!("agent {agent} avoids old no-orchestrator framing"),
        );
    }

    for agent in NEW_AGENTS {
        let file = format!("plugins/dvandva/agents/{agent}.md");
        let oc = |r: &mut Report, needle: &str, msg: String| {
            r.add(output_contract_contains(root, &file, needle), msg);
        };
        oc(
            &mut r,
            "id:",
            format!("new agent {agent} outputs schema-valid track id"),
        );
        oc(
            &mut r,
            "phase:",
            format!("new agent {agent} outputs schema-valid track phase"),
        );
        oc(
            &mut r,
            "status: completed|blocked",
            format!("new agent {agent} outputs schema-valid track status"),
        );
        oc(
            &mut r,
            "track:",
            format!("new agent {agent} outputs schema-valid track name"),
        );
        oc(
            &mut r,
            &format!("owner: dvandva-{agent}"),
            format!("new agent {agent} outputs schema-valid track owner"),
        );
        oc(
            &mut r,
            "parallelized:",
            format!("new agent {agent} outputs schema-valid parallelized flag"),
        );
        oc(
            &mut r,
            "rationale:",
            format!("new agent {agent} outputs schema-valid rationale"),
        );
        oc(
            &mut r,
            "inputs:",
            format!("new agent {agent} outputs schema-valid inputs"),
        );
        oc(
            &mut r,
            "outputs:",
            format!("new agent {agent} outputs schema-valid outputs"),
        );
        oc(
            &mut r,
            "evidence_refs:",
            format!("new agent {agent} outputs schema-valid evidence refs"),
        );
        oc(
            &mut r,
            "result: approved|findings|blocked",
            format!("new agent {agent} outputs schema-valid result"),
        );
    }

    // General 4-class membership: every agent file (including future non-seed
    // agents) must declare exactly one valid model class.
    for path in list_md(root, "plugins/dvandva/agents") {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("agent")
            .to_string();
        require_agent_model_class(
            &mut r,
            root,
            &format!("plugins/dvandva/agents/{name}.md"),
            format!("agent {name} declares a single valid model class"),
        );
    }
    for agent in OPUS_AGENTS {
        require_agent_model(
            &mut r,
            root,
            &format!("plugins/dvandva/agents/{agent}.md"),
            "opus",
            format!("agent {agent} uses opus-class model for hard reasoning"),
        );
    }
    for agent in SONNET_AGENTS {
        require_agent_model(
            &mut r,
            root,
            &format!("plugins/dvandva/agents/{agent}.md"),
            "sonnet",
            format!("agent {agent} uses sonnet-class model for bounded execution"),
        );
    }
    for agent in DOWNSTREAM {
        req(
            &mut r,
            root,
            &format!("plugins/dvandva/agents/{agent}.md"),
            "## Downstream Consumer",
            format!("agent {agent} names downstream consumer"),
        );
    }
    for agent in ADVERSARIAL {
        let file = format!("plugins/dvandva/agents/{agent}.md");
        req(
            &mut r,
            root,
            &file,
            "## Adversarial Stance",
            format!("agent {agent} declares adversarial stance"),
        );
        req(
            &mut r,
            root,
            &file,
            "If you cannot verify a claim",
            format!("agent {agent} uses correct proof standard"),
        );
        rej(
            &mut r,
            root,
            &file,
            "If you cannot disprove a claim",
            format!("agent {agent} avoids inverted proof standard"),
        );
    }

    let ad = "plugins/dvandva/agents";
    req(
        &mut r,
        root,
        &format!("{ad}/researcher.md"),
        "tools: Read, Glob, Grep, WebFetch",
        "researcher stays read-only plus WebFetch",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/architect.md"),
        "tools: Read, Glob, Grep",
        "architect stays read-only",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/architect.md"),
        "must_not_do:",
        "architect work split carries must-not-do boundary",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/implementer.md"),
        "phase: parallel_implementing",
        "implementer maps to parallel implementation",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/cross-reviewer.md"),
        "phase: cross_review",
        "cross reviewer maps to cross_review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/adversarial-analyst.md"),
        "phase: deep_review",
        "adversarial analyst maps to deep_review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/deep-reviewer.md"),
        "tools: Read, Glob, Grep, Bash",
        "deep reviewer can verify without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/adversarial-analyst.md"),
        "tools: Read, Glob, Grep, Bash",
        "adversarial analyst can inspect and run probes without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/baton-auditor.md"),
        "tools: Read, Glob, Grep, Bash",
        "baton auditor can inspect without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/sandbox-verifier.md"),
        "tools: Read, Glob, Grep, Bash",
        "sandbox verifier can run probes without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/implementer.md"),
        "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write",
        "implementer declares edit tools",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/test-creator.md"),
        "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write",
        "test creator declares edit tools",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/deslopper.md"),
        "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write",
        "deslopper declares edit tools",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/cross-reviewer.md"),
        "tools: Read, Glob, Grep, Bash",
        "cross reviewer can verify without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/architect.md"),
        "two-team parallel implementation",
        "architect plans two-team implementation",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/architect.md"),
        "implementation-phase parallelism is mandatory",
        "architect enforces mandatory implementation parallelism",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/architect.md"),
        "cross-review",
        "architect plans cross-review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/adversarial-analyst.md"),
        "Attack Hypothesis",
        "adversarial analyst emits attack hypotheses",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/deep-reviewer.md"),
        "at least three angle-specific reviewers",
        "deep reviewer requires multi-angle review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/baton-auditor.md"),
        "active_roles",
        "baton auditor checks active_roles",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/security-auditor.md"),
        "tools: Read, Glob, Grep, Bash",
        "security auditor can verify without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/security-auditor.md"),
        "phase: deep_review",
        "security auditor maps to deep_review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/security-auditor.md"),
        "threat_category",
        "security auditor classifies by threat category",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/integration-checker.md"),
        "tools: Read, Glob, Grep, Bash",
        "integration checker can verify without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/integration-checker.md"),
        "phase: deep_review",
        "integration checker maps to deep_review",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/integration-checker.md"),
        "chunk_boundaries_reviewed",
        "integration checker reviews chunk boundaries",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/debugger.md"),
        "tools: Read, Glob, Grep, Bash",
        "debugger can inspect without editing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/debugger.md"),
        "phase: phase_fixing",
        "debugger maps to phase_fixing",
    );
    req(
        &mut r,
        root,
        &format!("{ad}/debugger.md"),
        "root_cause_confirmed",
        "debugger confirms root cause",
    );

    let product = "product.md";
    req(
        &mut r,
        root,
        product,
        "GSD-style fresh-context subagents",
        "product cites GSD-style subagent pattern",
    );
    req(
        &mut r,
        root,
        product,
        "OMO-style team roles",
        "product cites OMO-style team role pattern",
    );
    req(
        &mut r,
        root,
        product,
        "canonical Dvandva subagent roster",
        "product declares canonical Dvandva agent roster",
    );
    req(
        &mut r,
        root,
        product,
        "dvandva-adversarial-analyst",
        "product includes adversarial analyst",
    );
    for agent in NEW_AGENTS {
        req(
            &mut r,
            root,
            product,
            &format!("dvandva-{agent}"),
            format!("product includes {agent}"),
        );
        req(
            &mut r,
            root,
            vadi,
            &format!("dvandva-{agent}"),
            format!("vadi skill includes {agent}"),
        );
        req(
            &mut r,
            root,
            "plugins/dvandva/skills/prativadi/SKILL.md",
            &format!("dvandva-{agent}"),
            format!("prativadi skill includes {agent}"),
        );
        req(
            &mut r,
            root,
            research,
            &format!("dvandva-{agent}"),
            format!("research skill includes {agent}"),
        );
    }
    for file in [
        "README.md",
        "product.md",
        "plugins/dvandva/skills/vadi/SKILL.md",
        "plugins/dvandva/skills/prativadi/SKILL.md",
        research,
    ] {
        req_model_policy_common(&mut r, root, file, MODEL_POLICY_VENDOR_NEUTRAL_DOCS);
        req(
            &mut r,
            root,
            file,
            MODEL_POLICY_NO_HAIKU_SUBAGENTS,
            format!("{file} forbids haiku-class Dvandva subagents"),
        );
    }
    for file in [
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/local-baton-channel.md",
        "plugins/dvandva/references/state-transition-table.md",
    ] {
        req(
            &mut r,
            root,
            file,
            MODEL_POLICY_CLAUDE_MAPPING,
            format!("{file} documents Claude model-class mapping"),
        );
        req(
            &mut r,
            root,
            file,
            MODEL_POLICY_CODEX_MAPPING,
            format!("{file} documents Codex model-class mapping"),
        );
        req_model_policy_routing(&mut r, root, file);
    }
    for file in [
        "plugins/dvandva/commands/vadi.md",
        "plugins/dvandva/commands/prativadi.md",
    ] {
        req_model_policy_common(&mut r, root, file, MODEL_POLICY_VENDOR_NEUTRAL_COMMANDS);
        req_command_ring_dispatch(&mut r, root, file);
        req(
            &mut r,
            root,
            file,
            MODEL_POLICY_NO_HAIKU_COMMANDS,
            format!("{file} forbids haiku-class Dvandva subagents"),
        );
    }
    req(
        &mut r,
        root,
        product,
        "adversarial-analyst.md",
        "product layout includes adversarial analyst agent file",
    );
    req(
        &mut r,
        root,
        product,
        "at least three angle-specific reviewers",
        "product requires multi-angle deep review",
    );
    req(
        &mut r,
        root,
        product,
        "one-date explainer under `./superpowers/run-reports/`",
        "product documents final explainer path",
    );
    req(
        &mut r,
        root,
        product,
        "never add a second date prefix",
        "product documents date-prefixed run_id explainer convention",
    );
    // RE-KEYED: `install-codex.sh` -> `dvandva install-codex` in the smoke-probe sentence.
    req(&mut r, root, product, "direct Codex plugin install, dual installer install, and dvandva install-codex helper install", "product documents all smoke install runtime probes");
    req(&mut r, root, product, "dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup", "product documents all smoke-verified Dvandva skills");
    // RE-KEYED: `scripts/smoke-plugin-install.sh` -> `rust/dvandva/src/smoke.rs`.
    req(
        &mut r,
        root,
        "rust/dvandva/src/smoke.rs",
        "dvandva:research",
        "smoke port requires research skill runtime surface",
    );
    rej(
        &mut r,
        root,
        product,
        "then write baton with `status: deep_review, assignee: prativadi",
        "product avoids stale direct test_creation-to-deep_review mode wording",
    );
    rej(
        &mut r,
        root,
        product,
        "| `test_creation` | `deep_review, review_target: implementation`",
        "product avoids stale direct test_creation-to-deep_review transition row",
    );
    req(
        &mut r,
        root,
        research,
        "canonical Dvandva subagent roster",
        "research skill declares canonical Dvandva agent roster",
    );
    req(
        &mut r,
        root,
        research,
        "dvandva-adversarial-analyst",
        "research skill includes adversarial analyst",
    );
    req(&mut r, root, "plugins/dvandva/skills/prativadi/SKILL.md", "Add `dvandva-adversarial-analyst` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses", "prativadi deep review invokes adversarial analyst");

    for absorbed in ["testing", "understanding", "worktree-setup"] {
        let file = format!("plugins/dvandva/skills/{absorbed}/SKILL.md");
        req(
            &mut r,
            root,
            &file,
            &format!("name: {absorbed}"),
            format!("absorbed skill {absorbed} has plugin-local name"),
        );
        req(
            &mut r,
            root,
            &file,
            "Dvandva",
            format!("absorbed skill {absorbed} is rewritten for Dvandva"),
        );
        req(
            &mut r,
            root,
            &file,
            "BATON_STATE",
            format!("absorbed skill {absorbed} surfaces baton state"),
        );
    }
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/testing/SKILL.md",
        "100% test coverage",
        "testing skill requires full coverage",
    );
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/testing/SKILL.md",
        "test_creation",
        "testing skill maps to test_creation",
    );
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/testing/SKILL.md",
        "verification_matrix",
        "testing skill updates verification matrix",
    );
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/understanding/SKILL.md",
        "./superpowers/understanding/YYYY-MM-DD-<topic>.html",
        "understanding skill writes HTML checklist",
    );
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/worktree-setup/SKILL.md",
        "BRANCH-NOTES.md",
        "worktree skill preserves branch notes",
    );
    req(
        &mut r,
        root,
        "plugins/dvandva/skills/worktree-setup/SKILL.md",
        "~/ACTIVE-WORK.md",
        "worktree skill updates active work",
    );

    r
}

/// CLI entry: run phase4's own content checks, then chain the sibling lints
/// in-process (protocol-phase1, skill-phase3, artifacts). Aggregate exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let own = report(&root);
    own.print();
    let mut failed = !own.passed();
    if crate::lint::protocol_phase1::run(args) != 0 {
        failed = true;
    }
    if crate::lint::skill_phase3::run(args) != 0 {
        failed = true;
    }
    // The shell aggregator chained lint-artifacts with its DEFAULT target
    // (`<root>/superpowers`), never the repo root itself — forwarding the raw
    // root would reject the repo's own README.md as a generated artifact.
    let artifacts_target = root.join("superpowers").display().to_string();
    if crate::lint::artifacts::run(&[artifacts_target]) != 0 {
        failed = true;
    }
    if failed {
        1
    } else {
        0
    }
}
