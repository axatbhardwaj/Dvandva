//! Fixture-driven tests for the six repo-content lints, re-keyed to the
//! post-port `dvandva <subcommand>` command grammar.
//!
//! Each test builds a small fixture tree in a tempdir and drives the lint's
//! `report(root)` seam directly, asserting on findings (the in-process analog
//! of the shell meta-tests' `expect_pass` / `expect_fail "<text>"`). These
//! never touch the live repo tree — that is a later verification gate.

use std::fs;
use std::path::Path;

use dvandva::lint::{
    phase4_research, protocol_phase1, run3_dynamic_agents, run4_path_gates, run4_standalone_agents,
    skill_phase3,
};
use tempfile::TempDir;

fn w(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn tmp() -> TempDir {
    tempfile::tempdir().unwrap()
}

// ---------------------------------------------------------------------------
// protocol-phase1
// ---------------------------------------------------------------------------

fn protocol_fixture(root: &Path) {
    w(
        root,
        "product.md",
        r#"dvandva.baton.v2 schema.
run_id field.
original_ask field.
research_ref field.
run_explainer_reviews field.
research_drafting research_review research_revision states.
Persistent wait: dvandva wait --persist keeps polling.
Continuous polling is the hard rule.
This applies to generated user-facing artifacts and HTML migration scope.
Required v2 fields include active_roles and agent_instances.
The full-profile v2 flow has eight segments.
Every completed full-profile v2 development run must produce a one-date explainer.
`development` is the delivery run; its separate `profile` field selects the lifecycle.
For v2 full-profile phase work, approve by writing `phase: 1, status: parallel_implementing`.
For v2 fast/standard-profile phase work, approve by writing `phase: 1, status: implementing`.
Mode C recognizes status: "parallel_implementing"` for full-profile v2, or `"implementing"` for fast/standard-profile v2.
| `review_of_review (prativadi_fixups)` | final `done` | Legacy v1 final phase approved by both roles after vadi approves prativadi fixups. |
| `counter_review (vadi_counter)` | final `done` | Legacy v1 final phase approved by both roles after prativadi approves counter. |
| `research_review` | `implementing` | Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation. |
no daemon and no hidden orchestrator. done human_question human_decision are inactive.
"#,
    );
    let channel = r#"Baton paths use .dvandva/runs/<run_id>/baton.json and DVANDVA_RUN_ID and run_id.
generated user-facing artifacts and HTML policy.
run_explainer_reviews evidence.
Continuous polling is the hard rule.
Phase convention: implementation-chunk N of M.
Legacy v1: `spec_review` → `phase: 1, implementing`.
v2: `deslop` → `phase: N+1, parallel_implementing`.
Fast profile: `research_review` -> `implementing`.
"#;
    w(root, "docs/protocol/local-baton-channel.md", channel);
    w(
        root,
        "plugins/dvandva/references/local-baton-channel.md",
        channel,
    );
    w(
        root,
        "plugins/dvandva/references/baton-schema-v2.json",
        r#"{
  "schema": "dvandva.baton.v2",
  "run_id": "",
  "original_ask": "",
  "research_ref": "",
  "run_explainer_reviews": [],
  "turn_cap": 60
}
"#,
    );
    w(
        root,
        "plugins/dvandva/references/baton-schema.json",
        "{ \"turn_cap\": 60 }\n",
    );
    w(
        root,
        "templates/channel/baton.json",
        "{ \"turn_cap\": 60 }\n",
    );
    w(
        root,
        "plugins/dvandva/references/state-transition-table.md",
        r#"dvandva.baton.v2 transitions.
research_drafting research_review research_revision.
run_explainer_reviews gate.
| `research_review` | `implementing` | Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation. |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: advance. |
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: advance. |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: advance. |
"#,
    );
}

#[test]
fn protocol_phase1_accepts_complete_fixture() {
    let d = tmp();
    protocol_fixture(d.path());
    let r = protocol_phase1::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn protocol_phase1_rejects_missing_original_ask() {
    let d = tmp();
    protocol_fixture(d.path());
    w(d.path(), "product.md", "dvandva.baton.v2 only.\n");
    let r = protocol_phase1::report(d.path());
    assert!(r.fails_with("product spec defines original_ask"));
}

#[test]
fn protocol_phase1_rejects_missing_persist_wait_rekey() {
    let d = tmp();
    protocol_fixture(d.path());
    // Drop the `--persist` flag but keep `dvandva wait`.
    let p = d.path().join("product.md");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("dvandva wait --persist", "dvandva wait");
    fs::write(&p, text).unwrap();
    let r = protocol_phase1::report(d.path());
    assert!(r.fails_with("product spec defines persistent dvandva wait"));
}

#[test]
fn protocol_phase1_rejects_stale_single_baton_wording() {
    let d = tmp();
    protocol_fixture(d.path());
    let p = d.path().join("product.md");
    let text = format!(
        "{}\nOne active baton per worktree.\n",
        fs::read_to_string(&p).unwrap()
    );
    fs::write(&p, text).unwrap();
    let r = protocol_phase1::report(d.path());
    assert!(r.fails_with("product spec no longer excludes multi-run support"));
}

#[test]
fn protocol_phase1_rejects_wrong_turn_cap() {
    let d = tmp();
    protocol_fixture(d.path());
    w(
        d.path(),
        "templates/channel/baton.json",
        "{ \"turn_cap\": 20 }\n",
    );
    let r = protocol_phase1::report(d.path());
    assert!(r.fails_with("channel template seed uses turn_cap 60"));
}

// ---------------------------------------------------------------------------
// skill-phase3
// ---------------------------------------------------------------------------

fn skill_shared_block() -> String {
    r#"Resolve the active baton path before reading or writing.
DVANDVA_BATON_FILE points at an explicit baton.
DVANDVA_RUN_DIR points at an explicit run dir.
DVANDVA_RUN_ID names the run.
Path: .dvandva/runs/<run_id>/baton.json
BATON_FILE and BATON_NEXT_FILE hold the resolved paths.
dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"
original_ask run_id research_ref run_explainer_reviews plan_ref turn_cap
BATON_STATE: { mode, phase, status, assignee: ... }
--persist-max <600 caps Claude waits.
Codex-hosted sessions may use `--persist`.
Exit 23 signals the persistent cap.
Continuous polling is the hard rule.
Phase convention: implementation-chunk N of M.
"#
    .to_string()
}

fn skill_vadi_block() -> String {
    let mut s = skill_shared_block();
    s.push_str(
        r#"Record the user's original ask in the initial baton context.
Plans live at ./superpowers/plans/YYYY-MM-DD-<topic>.html
Full-profile v2 writes `status: "test_creation"`; fast/standard-profile v2 writes `status: "phase_review"`.
Development/full fixbacks keep the numeric implementation phase, set `status: "test_creation"`.
Development/fast and development/standard fixbacks keep the numeric implementation phase, set `status: "phase_review"`.
fast` is allowlisted prose-only work with an optional `research_drafting -> research_review -> implementing` prelude.
For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.
"#,
    );
    s
}

fn skill_prativadi_block() -> String {
    let mut s = skill_shared_block();
    s.push_str(
        r#"Full-profile v2: `status: "parallel_implementing"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`.
Fast/standard-profile v2: `status: "implementing"`, `assignee: "vadi"`, `active_roles: []`.
Fast/standard profiles do not use `review_of_review` narrow-fix branches.
Development/fast: write `phase: 1`, `status: "implementing"`, `assignee: "vadi"`, and `active_roles: []` so the allowlisted fast path skips spec planning.
Full-profile development no-change approval routes to `deslop`; fast/standard compact no-change approval routes through `phase_review -> termination_review` on the final phase or `phase_review -> implementing` for additional work.
Re-read the final diff, verification, and the mode/profile-appropriate terminal evidence.
For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.
"#,
    );
    s
}

fn skill_command_block() -> String {
    r#"Goal: drive the resolved Dvandva baton.
DVANDVA_RUN_ID names the run.
turn_cap is the active cap; do not count wait heartbeats as turns.
continuous polling is the hard rule.
run_explainer_reviews are required at the end.
"#
    .to_string()
}

fn skill_fixture(root: &Path) {
    w(
        root,
        "plugins/dvandva/skills/vadi/SKILL.md",
        &skill_vadi_block(),
    );
    w(
        root,
        "plugins/dvandva/skills/prativadi/SKILL.md",
        &skill_prativadi_block(),
    );
    w(
        root,
        "plugins/dvandva/commands/vadi.md",
        &skill_command_block(),
    );
    w(
        root,
        "plugins/dvandva/commands/prativadi.md",
        &skill_command_block(),
    );
}

#[test]
fn skill_phase3_accepts_complete_fixture() {
    let d = tmp();
    skill_fixture(d.path());
    let r = skill_phase3::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn skill_phase3_rejects_missing_write_rekey() {
    let d = tmp();
    skill_fixture(d.path());
    let p = d.path().join("plugins/dvandva/skills/vadi/SKILL.md");
    let text = fs::read_to_string(&p).unwrap().replace(
        "dvandva write \"$BATON_FILE\" \"$BATON_NEXT_FILE\"",
        "write the baton",
    );
    fs::write(&p, text).unwrap();
    let r = skill_phase3::report(d.path());
    assert!(r.fails_with("vadi skill writes through resolved baton path"));
}

#[test]
fn skill_phase3_rejects_markdown_plan_ref() {
    let d = tmp();
    skill_fixture(d.path());
    let p = d.path().join("plugins/dvandva/skills/vadi/SKILL.md");
    let text = format!(
        "{}\nPlans live at ./superpowers/plans/YYYY-MM-DD-<topic>.md\n",
        fs::read_to_string(&p).unwrap()
    );
    fs::write(&p, text).unwrap();
    let r = skill_phase3::report(d.path());
    assert!(r.fails_with("vadi no longer directs generated plans to markdown"));
}

#[test]
fn skill_phase3_rejects_command_missing_explainer_reviews() {
    let d = tmp();
    skill_fixture(d.path());
    w(d.path(), "plugins/dvandva/commands/prativadi.md", "Goal: drive the resolved Dvandva baton. DVANDVA_RUN_ID turn_cap do not count wait heartbeats as turns. continuous polling is the hard rule.\n");
    let r = skill_phase3::report(d.path());
    assert!(r.fails_with("goal requires final explainer reviews"));
}

// ---------------------------------------------------------------------------
// phase4-research (aggregator: report() = own content checks)
// ---------------------------------------------------------------------------

const BIG_LIST: &str = r#"work_split verification_matrix 100% test coverage
test_creation deep_review deslop
Regular checkpoint commits
conditional parallelism
two-team parallel implementation
cross-review
implementation-phase parallelism is mandatory
Phase convention: implementation-chunk
same-status sync checkpoints
subagent_tracks
run_explainer_ref run_explainer_reviews
v2 write-helper enforcement
wait-helper persist cap exit 23
write-helper validation exit 23
"#;

const MODEL_CLASSES: &str = r#"Dvandva model classes are vendor-neutral.
Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models.
Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`.
Do not use `haiku` for Dvandva subagents.
"#;

const SUPERPOWERS: &str = "Superpowers is a hard runtime dependency.\nDvandva owns baton state.\n";

const NEW5: &str = "dvandva-adversarial-analyst dvandva-security-auditor dvandva-integration-checker dvandva-debugger dvandva-doc-verifier dvandva-pattern-mapper\n";

const AGENTS15: [&str; 15] = [
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

fn phase4_agent(name: &str) -> String {
    let opus = [
        "adversarial-analyst",
        "architect",
        "baton-auditor",
        "cross-reviewer",
        "deep-reviewer",
        "doc-verifier",
        "integration-checker",
        "pattern-mapper",
        "researcher",
        "sandbox-verifier",
        "security-auditor",
    ];
    let model = if opus.contains(&name) {
        "opus"
    } else {
        "sonnet"
    };
    let (tools, phase) = match name {
        "researcher" => ("Read, Glob, Grep, WebFetch", "research"),
        "architect" => ("Read, Glob, Grep", "spec"),
        "implementer" => (
            "Read, Glob, Grep, Bash, Edit, MultiEdit, Write",
            "parallel_implementing",
        ),
        "test-creator" => (
            "Read, Glob, Grep, Bash, Edit, MultiEdit, Write",
            "test_creation",
        ),
        "deslopper" => ("Read, Glob, Grep, Bash, Edit, MultiEdit, Write", "deslop"),
        "cross-reviewer" => ("Read, Glob, Grep, Bash", "cross_review"),
        "debugger" => ("Read, Glob, Grep, Bash", "phase_fixing"),
        "pattern-mapper" => ("Read, Glob, Grep", "research"),
        _ => ("Read, Glob, Grep, Bash", "deep_review"),
    };
    let downstream = [
        "researcher",
        "architect",
        "implementer",
        "test-creator",
        "deslopper",
        "pattern-mapper",
    ];
    let adversarial = [
        "cross-reviewer",
        "adversarial-analyst",
        "deep-reviewer",
        "sandbox-verifier",
        "baton-auditor",
        "security-auditor",
        "integration-checker",
        "doc-verifier",
    ];
    let new_agents = [
        "security-auditor",
        "integration-checker",
        "debugger",
        "doc-verifier",
        "pattern-mapper",
    ];

    let mut s = String::new();
    s.push_str(&format!(
        "---\nname: dvandva-{name}\ndescription: Use when the run needs {name}.\nmodel: {model}\ntools: {tools}\nphase: {phase}\n---\n"
    ));
    s.push_str(&format!("# dvandva-{name}\n"));
    s.push_str("## Mission\nReports work_split, verification_matrix, subagent_tracks.\n");
    s.push_str("## Use When\nUse when the run needs it.\n");
    s.push_str("## Required Inputs\nwork_split and verification_matrix.\n");
    s.push_str("## Operating Loop\nStep through the loop.\n");
    s.push_str("## Output Contract\n");
    if new_agents.contains(&name) {
        s.push_str(&format!(
            "id: {name}-1\nphase: {phase}\nstatus: completed|blocked\ntrack: {name}-track\nowner: dvandva-{name}\nparallelized: true\nrationale: bounded chunk\ninputs: work_split\noutputs: verification_matrix\nevidence_refs: subagent_tracks\nresult: approved|findings|blocked\n"
        ));
    } else {
        s.push_str("Reports structured evidence with work_split and subagent_tracks.\n");
    }
    s.push_str("## Evidence Rules\nEvidence over assertion.\n");
    s.push_str("## Guardrails\nStay in scope.\n");
    s.push_str("## Common Failures\nSkipping evidence.\n");
    if downstream.contains(&name) {
        s.push_str("## Downstream Consumer\nThe next phase consumes this output.\n");
    }
    if adversarial.contains(&name) {
        s.push_str("## Adversarial Stance\nIf you cannot verify a claim, treat it as unproven.\n");
    }
    match name {
        "architect" => s.push_str(
            "must_not_do: overlap.\ntwo-team parallel implementation.\nimplementation-phase parallelism is mandatory.\ncross-review.\n",
        ),
        "adversarial-analyst" => s.push_str("Attack Hypothesis: boundary breach.\n"),
        "deep-reviewer" => s.push_str("Dispatch at least three angle-specific reviewers.\n"),
        "baton-auditor" => s.push_str("Checks active_roles integrity.\n"),
        "security-auditor" => s.push_str("Classifies by threat_category.\n"),
        "integration-checker" => s.push_str("Reports chunk_boundaries_reviewed.\n"),
        "debugger" => s.push_str("Reports root_cause_confirmed.\n"),
        _ => {}
    }
    s
}

fn phase4_fixture(root: &Path) {
    // README
    let mut readme = String::new();
    readme.push_str(SUPERPOWERS);
    readme.push_str("regular local checkpoint commits.\n");
    readme.push_str("`dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup` are installed.\n");
    readme.push_str("Validation exercises all six Dvandva skills.\n");
    readme.push_str(MODEL_CLASSES);
    readme.push_str("Definition of done: cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test\n");
    readme.push_str("claude plugin validate plugins/dvandva\n");
    readme.push_str("claude plugin validate .\n");
    w(root, "README.md", &readme);

    // product.md
    let mut product = String::new();
    product.push_str(SUPERPOWERS);
    product.push_str(BIG_LIST);
    product.push_str("GSD-style fresh-context subagents.\n");
    product.push_str("OMO-style team roles.\n");
    product.push_str("canonical Dvandva subagent roster.\n");
    product.push_str(NEW5);
    product.push_str(MODEL_CLASSES);
    product.push_str("Layout: adversarial-analyst.md and peers.\n");
    product.push_str("Deep review dispatches at least three angle-specific reviewers.\n");
    product.push_str("Produces a one-date explainer under `./superpowers/run-reports/`.\n");
    product.push_str("Reuse the run_id date; never add a second date prefix.\n");
    product.push_str("Smoke probes direct Codex plugin install, dual installer install, and dvandva install-codex helper install.\n");
    product.push_str("`dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup`\n");
    w(root, "product.md", &product);

    // channel docs
    let channel = format!("{SUPERPOWERS}{BIG_LIST}");
    w(root, "docs/protocol/local-baton-channel.md", &channel);
    w(
        root,
        "plugins/dvandva/references/local-baton-channel.md",
        &channel,
    );
    w(
        root,
        "plugins/dvandva/references/state-transition-table.md",
        BIG_LIST,
    );

    // vadi skill
    let mut vadi = String::new();
    vadi.push_str(SUPERPOWERS);
    vadi.push_str(&phase4_role_skill());
    vadi.push_str("Existing baton discovery scans .dvandva/runs/*/baton.json.\n");
    vadi.push_str("When only terminal batons exist, auto-create a new named run.\n");
    vadi.push_str("For active batons, ask the user whether to continue.\n");
    vadi.push_str(MODEL_CLASSES);
    w(root, "plugins/dvandva/skills/vadi/SKILL.md", &vadi);

    // prativadi skill
    let mut prativadi = String::new();
    prativadi.push_str(SUPERPOWERS);
    prativadi.push_str(&phase4_role_skill());
    prativadi.push_str("Add `dvandva-adversarial-analyst` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses.\n");
    prativadi.push_str(MODEL_CLASSES);
    w(
        root,
        "plugins/dvandva/skills/prativadi/SKILL.md",
        &prativadi,
    );

    // research skill
    let mut research = String::new();
    research.push_str("name: research\ndescription: Use when a run needs shared research.\n");
    research.push_str("original_ask research_ref run_explainer_reviews\n");
    research.push_str("Artifact: ./superpowers/research/YYYY-MM-DD-<topic>.html\n");
    research.push_str("work_split verification_matrix 100% test coverage\n");
    research.push_str("test creation is separate from review.\n");
    research.push_str("deep_review deslop parallel subagents research_review research_revision\n");
    research.push_str("generated user-facing HTML artifact, dark self-contained HTML, machine-readable metadata.\n");
    research.push_str("If no subagent tool is available, do the same exploration directly and record the fallback in work_split.\n");
    research.push_str("Do not rely solely on the vadi's research_ref.\n");
    research.push_str("canonical Dvandva subagent roster.\n");
    research.push_str(NEW5);
    research.push_str(MODEL_CLASSES);
    w(root, "plugins/dvandva/skills/research/SKILL.md", &research);

    // commands
    let command = format!(
        "{SUPERPOWERS}research_ref work_split verification_matrix test_creation deep_review deslop\nparallel subagents\nconditional parallelism\nsubagent_tracks\nInvoke `dvandva:research`.\nregular local checkpoint commits\n"
    );
    w(root, "plugins/dvandva/commands/vadi.md", &command);
    w(root, "plugins/dvandva/commands/prativadi.md", &command);

    // v2 schema
    w(
        root,
        "plugins/dvandva/references/baton-schema-v2.json",
        r#"{
  "work_split": {},
  "verification_matrix": {},
  "run_explainer_ref": "",
  "run_explainer_reviews": [],
  "active_roles": [],
  "parallel_implementing": {},
  "test_creation": {},
  "cross_review": {},
  "cross_fixing": {},
  "deep_review": {},
  "deslop": {}
}
"#,
    );

    // absorbed skills
    w(
        root,
        "plugins/dvandva/skills/testing/SKILL.md",
        "name: testing\nDvandva testing skill.\nBATON_STATE surfaced.\n100% test coverage\ntest_creation\nverification_matrix\n",
    );
    w(
        root,
        "plugins/dvandva/skills/understanding/SKILL.md",
        "name: understanding\nDvandva understanding skill.\nBATON_STATE surfaced.\n./superpowers/understanding/YYYY-MM-DD-<topic>.html\n",
    );
    w(
        root,
        "plugins/dvandva/skills/worktree-setup/SKILL.md",
        "name: worktree-setup\nDvandva worktree setup skill.\nBATON_STATE surfaced.\nBRANCH-NOTES.md\n~/ACTIVE-WORK.md\n",
    );

    // agents
    for name in AGENTS15 {
        w(
            root,
            &format!("plugins/dvandva/agents/{name}.md"),
            &phase4_agent(name),
        );
    }

    // smoke port carries the research runtime surface token
    w(
        root,
        "rust/dvandva/src/smoke.rs",
        "// dvandva smoke-install probes the dvandva:research runtime surface.\n",
    );
}

fn phase4_role_skill() -> String {
    let mut s = String::new();
    s.push_str("Invoke `dvandva:research`.\n");
    s.push_str("research_drafting research_review research_revision.\n");
    s.push_str("work_split verification_matrix 100% test coverage\n");
    s.push_str("test_creation deep_review deslop\n");
    s.push_str("parallel subagents\n");
    s.push_str("canonical Dvandva subagent roster\n");
    s.push_str("all phases are subagent-driven\n");
    s.push_str("independent research review\n");
    s.push_str("BATON_BROKEN_FILE=\"$BATON_DIR/baton.broken.json\"\n");
    s.push_str("Write `$BATON_BROKEN_FILE` preserving the last good state.\n");
    s.push_str("write-helper validation exit 23\n");
    s.push_str("wait-helper persist cap exit 23\n");
    s.push_str("`dvandva.baton.v1` or `dvandva.baton.v2`\n");
    s.push_str("Regular checkpoint commits\n");
    s.push_str("conditional parallelism\n");
    s.push_str("parallelize only genuinely disjoint tracks\n");
    s.push_str("record what was not parallelized and why\n");
    s.push_str("two-team parallel implementation\n");
    s.push_str("cross-review\n");
    s.push_str("implementation-phase parallelism is mandatory\n");
    s.push_str("Phase convention: implementation-chunk\n");
    s.push_str("same-status sync checkpoints\n");
    s.push_str("subagent_tracks\n");
    s.push_str(NEW5);
    s
}

#[test]
fn phase4_research_accepts_complete_fixture() {
    let d = tmp();
    phase4_fixture(d.path());
    let r = phase4_research::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn phase4_research_rejects_haiku_agent() {
    let d = tmp();
    phase4_fixture(d.path());
    let p = d.path().join("plugins/dvandva/agents/debugger.md");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("model: sonnet", "model: haiku");
    fs::write(&p, text).unwrap();
    let r = phase4_research::report(d.path());
    assert!(r.fails_with("rejects haiku model class"));
}

#[test]
fn phase4_research_rejects_wrong_model_class() {
    let d = tmp();
    phase4_fixture(d.path());
    // implementer must be sonnet; flip to opus.
    let p = d.path().join("plugins/dvandva/agents/implementer.md");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("model: sonnet", "model: opus");
    fs::write(&p, text).unwrap();
    let r = phase4_research::report(d.path());
    assert!(r.fails_with("uses sonnet-class model"));
}

#[test]
fn phase4_research_rejects_missing_cargo_gate_rekey() {
    let d = tmp();
    phase4_fixture(d.path());
    let p = d.path().join("README.md");
    let text = fs::read_to_string(&p).unwrap().replace(
        "cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test",
        "run the tests",
    );
    fs::write(&p, text).unwrap();
    let r = phase4_research::report(d.path());
    assert!(r.fails_with("Rust definition-of-done gate"));
}

#[test]
fn phase4_research_rejects_missing_new_agent() {
    let d = tmp();
    phase4_fixture(d.path());
    fs::remove_file(d.path().join("plugins/dvandva/agents/security-auditor.md")).unwrap();
    let r = phase4_research::report(d.path());
    assert!(r.failures() > 0);
}

// ---------------------------------------------------------------------------
// run3-dynamic-agents
// ---------------------------------------------------------------------------

const RUN3_SURFACE: &str = r#"Dvandva uses agent_instances for Run 3 dynamic agent records.
The static roster is the seed roster for run-scoped dynamic agents.
Explicit closure is required; every generated handle must be explicitly closed before completion.
Dynamic write-path disjointness is required unless conflict_group serialization applies.
There is no daemon and no mailbox.
There is no hidden scheduler or hidden central process.
Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models.
Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`.
generated agents never own assignee, active_roles, or transitions.
"#;

const RUN3_SEED: &str = r#"Dvandva uses agent_instances for Run 3 dynamic agent records.
The static roster is the seed roster for run-scoped dynamic agents.
This file is a dynamic agent-instance seed.
Generated briefs must satisfy this same seed agent contract.
Explicit closure is required; every generated handle must be explicitly closed before completion and each closed generated instance records non-empty work_item_ids.
Dynamic write-path disjointness is required when instances share base_checkpoint or when both instances are live planned/running, unless conflict_group serialization through depends_on applies.
There is no daemon and no mailbox.
There is no hidden scheduler or hidden central process.
Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models.
Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`.
generated agents never own assignee, active_roles, or transitions.
"#;

fn run3_base(root: &Path) {
    // Surface directories must exist even when empty.
    fs::create_dir_all(root.join("docs/protocol")).unwrap();
    fs::create_dir_all(root.join("docs/workflows")).unwrap();
    fs::create_dir_all(root.join("plugins/dvandva/agents")).unwrap();
    fs::create_dir_all(root.join("plugins/dvandva/commands")).unwrap();
    fs::create_dir_all(root.join("plugins/dvandva/references")).unwrap();
    fs::create_dir_all(root.join("plugins/dvandva/skills/research")).unwrap();
    w(root, "README.md", "");
    w(root, "product.md", "");
}

#[test]
fn run3_accepts_complete_surface() {
    let d = tmp();
    run3_base(d.path());
    w(
        d.path(),
        "plugins/dvandva/skills/research/SKILL.md",
        RUN3_SURFACE,
    );
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn run3_scans_seed_agent_contracts() {
    let d = tmp();
    run3_base(d.path());
    w(d.path(), "plugins/dvandva/agents/implementer.md", RUN3_SEED);
    w(d.path(), "plugins/dvandva/agents/architect.md", RUN3_SEED);
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn run3_rejects_seed_file_missing_work_item_ids() {
    let d = tmp();
    run3_base(d.path());
    w(d.path(), "plugins/dvandva/agents/implementer.md", RUN3_SEED);
    w(
        d.path(),
        "plugins/dvandva/agents/architect.md",
        &RUN3_SEED.replace("work_item_ids", "work items"),
    );
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.fails_with("binds work_item_ids"));
}

#[test]
fn run3_rejects_missing_agent_instances() {
    let d = tmp();
    run3_base(d.path());
    w(
        d.path(),
        "plugins/dvandva/skills/research/SKILL.md",
        &RUN3_SURFACE.replace("agent_instances", "agent registry"),
    );
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.fails_with("surface names Run 3 agent_instances"));
}

#[test]
fn run3_rejects_missing_no_daemon() {
    let d = tmp();
    run3_base(d.path());
    w(
        d.path(),
        "plugins/dvandva/skills/research/SKILL.md",
        &RUN3_SURFACE.replace("no daemon", "no background service"),
    );
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.fails_with("surface rejects a runtime daemon"));
}

#[test]
fn run3_rejects_empty_surface() {
    let d = tmp();
    run3_base(d.path());
    let r = run3_dynamic_agents::report(d.path());
    assert!(r.failures() > 0);
}

// ---------------------------------------------------------------------------
// run4-path-gates (re-keyed to Rust sources)
// ---------------------------------------------------------------------------

fn pathgate_fixture(root: &Path) {
    w(
        root,
        "README.md",
        "work_split write_paths declared.\nGit work-gating treats done human_question human_decision as inactive.\n",
    );
    w(
        root,
        "product.md",
        "safe_rel_path work_split validation.\nno daemon and no hidden orchestrator process.\nGit work-gating treats done human_question human_decision as inactive.\n",
    );
    w(
        root,
        "docs/protocol/local-baton-channel.md",
        "cross_review is read-only unless explicit write_paths are present.\nwrite_paths supplements paths rather than narrowing them; the effective write set is a union.\nOverlaps require a shared conflict_group and explicit depends_on serialization.\nGit work-gating treats done human_question human_decision batons as inactive.\n",
    );
    w(
        root,
        "plugins/dvandva/references/state-transition-table.md",
        "Live overlapping chunks share conflict_group and depends_on serialization.\nClosed terminal historical chunks can reuse paths; there is no base_checkpoint wave model.\nGit work-gating treats done human_question human_decision as inactive.\n",
    );
    w(
        root,
        "plugins/dvandva/references/baton-schema-v2.json",
        "{ \"note\": \"work_split entries carry write_paths, conflict_group, depends_on\" }\n",
    );
    // write port + shared safe_rel_path.
    w(
        root,
        "rust/dvandva/src/write.rs",
        "// validates work_split entries; each path checked with safe_rel_path.\n// unions paths and write_paths into a unique write set.\nfn v() { let _ = (\"work_split\", \"safe_rel_path\", \"paths\", \"write_paths\", \"unique\"); }\n",
    );
    w(
        root,
        "rust/dvandva/src/util.rs",
        "pub fn is_safe_rel_path() {}\n",
    );
    // skills invoke `dvandva preflight --role`.
    w(
        root,
        "plugins/dvandva/skills/vadi/SKILL.md",
        "Preflight: export DVANDVA_ROLE=vadi, then dvandva preflight --role vadi asserts DVANDVA_ROLE=vadi.\n",
    );
    w(
        root,
        "plugins/dvandva/skills/prativadi/SKILL.md",
        "Preflight: export DVANDVA_ROLE=prativadi, then dvandva preflight --role prativadi asserts DVANDVA_ROLE=prativadi.\n",
    );
    // hook installer + materialized hook bodies + gates.
    w(
        root,
        "rust/dvandva/src/install_hooks.rs",
        "// install-hooks sets core.hooksPath to .dvandva/githooks and dispatches pre-commit to dvandva commit-gate.\n// records dvandva.hooksAdoptedAt baseline.\n// records __DVANDVA_ROOT_PENDING__ for unborn repos.\n",
    );
    w(
        root,
        "rust/dvandva/src/hooks.rs",
        "// materialized prepare-commit-msg stamps Dvandva-Checkpoint trailers.\n// delegates to commit_gate::collect_baton_paths and commit_gate::is_gate_terminal.\n// fail closed via read_json_lenient on malformed baton JSON.\n",
    );
    w(
        root,
        "rust/dvandva/src/commit_gate.rs",
        "// dvandva commit-gate enforces DVANDVA_ROLE.\n// scans .dvandva/runs/*/baton.json; terminal done human_question human_decision.\n// fail closed via read_json_lenient.\n",
    );
    w(
        root,
        "rust/dvandva/src/drift_lint.rs",
        "// dvandva drift-lint inspects Dvandva-Checkpoint trailers.\n// honors dvandva.hooksAdoptedAt baseline.\n// __DVANDVA_ROOT_PENDING__ backfilled via rev-list.\n// hooksAdoptedAtInclusive scan_log_shas preserved.\n// delegates to commit_gate::collect_baton_paths and commit_gate::is_gate_terminal.\n// fail closed via read_json_lenient.\n",
    );
}

#[test]
fn pathgate_accepts_complete_fixture() {
    let d = tmp();
    pathgate_fixture(d.path());
    let r = run4_path_gates::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn pathgate_rejects_readme_without_write_paths() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(d.path(), "README.md", "work intent only.\nGit work-gating treats done human_question human_decision as inactive.\n");
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("README.md must document work_split write_paths"));
}

#[test]
fn pathgate_rejects_missing_cross_review_readonly() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "docs/protocol/local-baton-channel.md",
        "write_paths supplements paths rather than narrowing them; the effective write set is a union.\nOverlaps require a shared conflict_group and explicit depends_on serialization.\nGit work-gating treats done human_question human_decision batons as inactive.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("local-baton-channel.md must document cross_review read-only semantics"));
}

#[test]
fn pathgate_rejects_write_port_without_safe_rel_path() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/write.rs",
        "// unions paths and write_paths into a unique write set.\n",
    );
    w(d.path(), "rust/dvandva/src/util.rs", "pub fn other() {}\n");
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("write port must validate work_split paths with safe_rel_path"));
}

#[test]
fn pathgate_rejects_installer_without_commit_gate_dispatch() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/install_hooks.rs",
        "// install-hooks sets core.hooksPath to .dvandva/githooks.\n// records dvandva.hooksAdoptedAt baseline.\n// records __DVANDVA_ROOT_PENDING__ for unborn repos.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("hook installer must dispatch pre-commit to dvandva commit-gate"));
}

#[test]
fn pathgate_rejects_hooks_without_checkpoint_stamp() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/hooks.rs",
        "// delegates to commit_gate::collect_baton_paths and commit_gate::is_gate_terminal.\n// fail closed via read_json_lenient.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("prepare-commit-msg hook must stamp Dvandva-Checkpoint"));
}

#[test]
fn pathgate_rejects_commit_gate_without_role() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/commit_gate.rs",
        "// scans .dvandva/runs/*/baton.json; terminal done human_question human_decision.\n// fail closed via read_json_lenient.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("commit-gate must enforce DVANDVA_ROLE"));
}

#[test]
fn pathgate_rejects_vadi_missing_preflight_rekey() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "plugins/dvandva/skills/vadi/SKILL.md",
        "Preflight: export DVANDVA_ROLE=vadi, then run the preflight tool asserts DVANDVA_ROLE=vadi.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("vadi skill preflight must invoke dvandva preflight"));
}

#[test]
fn pathgate_rejects_resolver_without_run_scope() {
    let d = tmp();
    pathgate_fixture(d.path());
    // `commit_gate.rs` is the sole owner of the run-scan literal; drift_lint.rs
    // and hooks.rs delegate rather than duplicating it (see
    // `pathgate_rejects_drift_lint_without_baton_path_delegation` and
    // `pathgate_rejects_hooks_without_terminal_status_delegation` for the
    // consumer-side delegation checks).
    w(
        d.path(),
        "rust/dvandva/src/commit_gate.rs",
        "// dvandva commit-gate enforces DVANDVA_ROLE.\n// terminal done human_question human_decision.\n// fail closed via read_json_lenient.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("commit_gate.rs must scan run-scoped baton paths"));
}

#[test]
fn pathgate_rejects_owner_without_terminal_statuses() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/commit_gate.rs",
        "// dvandva commit-gate enforces DVANDVA_ROLE.\n// scans .dvandva/runs/*/baton.json.\n// fail closed via read_json_lenient.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("commit_gate.rs must share terminal baton statuses"));
}

#[test]
fn pathgate_rejects_drift_lint_without_baton_path_delegation() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/drift_lint.rs",
        "// dvandva drift-lint inspects Dvandva-Checkpoint trailers.\n// honors dvandva.hooksAdoptedAt baseline.\n// __DVANDVA_ROOT_PENDING__ backfilled via rev-list.\n// hooksAdoptedAtInclusive scan_log_shas preserved.\n// delegates to commit_gate::is_gate_terminal.\n// fail closed via read_json_lenient.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with(
        "drift_lint.rs must delegate baton-path discovery to commit_gate::collect_baton_paths"
    ));
}

#[test]
fn pathgate_rejects_hooks_without_terminal_status_delegation() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/hooks.rs",
        "// materialized prepare-commit-msg stamps Dvandva-Checkpoint trailers.\n// delegates to commit_gate::collect_baton_paths.\n// fail closed via read_json_lenient on malformed baton JSON.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with(
        "hooks.rs must delegate terminal-status classification to commit_gate::is_gate_terminal"
    ));
}

#[test]
fn pathgate_rejects_resolver_without_fail_closed_json() {
    let d = tmp();
    pathgate_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/commit_gate.rs",
        "// dvandva commit-gate enforces DVANDVA_ROLE.\n// scans .dvandva/runs/*/baton.json; terminal done human_question human_decision.\n",
    );
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("must fail closed on malformed baton JSON"));
}

#[test]
fn pathgate_rejects_missing_required_rust_source() {
    let d = tmp();
    pathgate_fixture(d.path());
    fs::remove_file(d.path().join("rust/dvandva/src/hooks.rs")).unwrap();
    let r = run4_path_gates::report(d.path());
    assert!(r.fails_with("rust/dvandva/src/hooks.rs is missing"));
}

// ---------------------------------------------------------------------------
// run4-standalone-agents (re-keyed: version 1.2.0, retire-agents, Rust ports)
// ---------------------------------------------------------------------------

fn standalone_fixture(root: &Path) {
    let mut readme = String::new();
    readme.push_str("Dvandva 1.2.0 ships the canonical Dvandva roster. Run 4 makes Dvandva-only ");
    readme.push_str("retirement available only for Dvandva-covered workflows. The retired Claude ");
    readme.push_str(
        "symlink allowlist is adversarial-analyst, architect, developer, quality-reviewer, ",
    );
    readme.push_str(
        "and sandbox-executor. Functional parity is proven by Runs 1-4 usage, not only by ",
    );
    readme.push_str(
        "file count. Codex agent-axis retirement is a no-op. Skills are out of scope; no ",
    );
    readme.push_str(
        "skill files are touched. The helper writes a backup manifest and supports restore.\n",
    );
    w(root, "README.md", &readme);

    w(
        root,
        "product.md",
        "Run 4 retires only Dvandva-covered standalone agents after version 1.2.0 cache parity and functional parity via Runs 1-4 usage. The Claude allowlist is adversarial-analyst, architect, developer, quality-reviewer, and sandbox-executor. Codex agent-axis cleanup is explicitly no-op. Skills are out of scope. Restore uses the backup manifest.\n",
    );
    w(
        root,
        "docs/protocol/local-baton-channel.md",
        "Run 4 retirement is Dvandva-only and limited to Dvandva-covered workflows. It does not retire Codex agent-axis files, does not touch skills, and is reversible through a backup manifest restore path.\n",
    );
    w(
        root,
        "plugins/dvandva/references/state-transition-table.md",
        "Run 4 records the 1.2.0 Dvandva roster parity, Dvandva-only retirement, Codex agent-axis no-op, and functional parity via Runs 1-4 usage.\n",
    );
    w(
        root,
        "plugins/dvandva/references/baton-schema-v2.json",
        "{ \"description\": \"Run 4 Dvandva-only retirement with backup manifest restore and no skill touches\" }\n",
    );

    // retire helper port (dvandva retire-agents) with test coverage.
    w(
        root,
        "rust/dvandva/src/retire.rs",
        "// dvandva retire-agents: Dvandva-only, Dvandva-covered workflows, functional parity via Runs 1-4 usage.\n// adversarial-analyst architect developer quality-reviewer sandbox-executor allowlist.\n// Codex agent-axis no-op; skills never touched; backup manifest restore path.\n#[cfg(test)]\nmod tests { #[test] fn covered() {} }\n",
    );
    // smoke + installer ports.
    w(
        root,
        "rust/dvandva/src/smoke.rs",
        "// dvandva smoke-install probes dvandva:research.\n",
    );
    w(
        root,
        "rust/dvandva/src/installers.rs",
        "// dvandva install and dvandva install-codex ports; 1.2.0 canonical 15-agent roster.\n",
    );

    // manifests at 1.2.0.
    w(
        root,
        ".claude-plugin/marketplace.json",
        "{\n  \"plugins\": [\n    { \"name\": \"dvandva\", \"source\": \"./plugins/dvandva\", \"version\": \"1.2.0\" }\n  ]\n}\n",
    );
    w(
        root,
        "plugins/dvandva/.claude-plugin/plugin.json",
        "{ \"name\": \"dvandva\", \"version\": \"1.2.0\" }\n",
    );
    w(
        root,
        "plugins/dvandva/.codex-plugin/plugin.json",
        "{ \"name\": \"dvandva\", \"version\": \"1.2.0\" }\n",
    );

    // 15 canonical agents.
    for name in AGENTS15 {
        w(
            root,
            &format!("plugins/dvandva/agents/{name}.md"),
            &format!("---\nname: dvandva-{name}\n---\n# dvandva-{name}\n"),
        );
    }
}

#[test]
fn standalone_accepts_complete_fixture() {
    let d = tmp();
    standalone_fixture(d.path());
    let r = run4_standalone_agents::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn standalone_rejects_readme_without_dvandva_only() {
    let d = tmp();
    standalone_fixture(d.path());
    let p = d.path().join("README.md");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("Dvandva-only", "general");
    fs::write(&p, text).unwrap();
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("README.md must document Dvandva-only retirement"));
}

#[test]
fn standalone_rejects_stale_release_wording() {
    let d = tmp();
    standalone_fixture(d.path());
    let p = d.path().join("README.md");
    let text = format!(
        "{}\nv0.2.0 ships legacy text\nRun 3 (in progress)\n",
        fs::read_to_string(&p).unwrap()
    );
    fs::write(&p, text).unwrap();
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("stale Run 3 or v0.2.0"));
}

#[test]
fn standalone_rejects_version_mismatch() {
    let d = tmp();
    standalone_fixture(d.path());
    w(
        d.path(),
        "plugins/dvandva/.codex-plugin/plugin.json",
        "{ \"name\": \"dvandva\", \"version\": \"0.3.0\" }\n",
    );
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("Dvandva manifest versions must all equal 1.2.0"));
}

#[test]
fn standalone_rejects_missing_canonical_agent() {
    let d = tmp();
    standalone_fixture(d.path());
    fs::remove_file(d.path().join("plugins/dvandva/agents/security-auditor.md")).unwrap();
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("must contain exactly the 15 canonical agents"));
}

#[test]
fn standalone_rejects_bad_frontmatter_name() {
    let d = tmp();
    standalone_fixture(d.path());
    w(
        d.path(),
        "plugins/dvandva/agents/test-creator.md",
        "---\nname: test-creator\n---\n# test-creator\n",
    );
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("agent frontmatter names must use dvandva-*"));
}

#[test]
fn standalone_rejects_retire_port_without_no_skill_touches() {
    let d = tmp();
    standalone_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/retire.rs",
        "// dvandva retire-agents: Dvandva-only, Dvandva-covered workflows, functional parity via Runs 1-4 usage.\n// backup manifest restore path.\n#[cfg(test)]\nmod tests { #[test] fn covered() {} }\n",
    );
    let r = run4_standalone_agents::report(d.path());
    assert!(r.fails_with("retirement helper must document no skill touches"));
}

#[test]
fn standalone_accepts_retire_port_with_two_line_wrapped_no_skill_touches_prose() {
    // The shell's `require_match` slurped the file (`tr '\n' ' '`) before
    // regex-matching, so a multi-token needle like "skills...never" could be
    // satisfied even when a line-length wrap (a real rustdoc wrap, each line
    // still `//`-prefixed) split "skills" and "never" onto different lines.
    // Per-line matching would false-fail this; slurp-style matching restores
    // that fidelity.
    let d = tmp();
    standalone_fixture(d.path());
    w(
        d.path(),
        "rust/dvandva/src/retire.rs",
        "// dvandva retire-agents: Dvandva-only, Dvandva-covered workflows, functional parity via Runs 1-4 usage.\n// adversarial-analyst architect developer quality-reviewer sandbox-executor allowlist.\n// Codex agent-axis no-op; skills are out of scope for this helper\n// since it never touches skill files; backup manifest restore path.\n#[cfg(test)]\nmod tests { #[test] fn covered() {} }\n",
    );
    let r = run4_standalone_agents::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}
