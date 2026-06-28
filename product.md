# Dvandva — Product Specification v1

Status: rewritten 2026-05-14 for richer flow (spec phase + phased implementation + mutual review + disagreement loop + `/goal` autonomy). Owner: axatbhardwaj. Supersedes the prompt-template-first approach in `templates/prompts/` and the single-shot doer→reviewer flow in the previous draft.

> **Spec rev 2026-06-11:** §3.1 adds `dvandva-write.sh` (validated atomic baton install + auto-snapshot, bundled byte-identical in both skill script dirs) and `scripts/test-dvandva-write.sh`. The wait helper's default `--max-wait` drops 900→540 so one foreground invocation fits Claude Code's 600 s Bash-tool cap (§7.2, §8.2, §12); it wakes early on baton-directory inotify events and retries once on torn reads. This pulls §16's deterministic validator forward to script level (a PreToolUse hook remains future work).
>
> **Spec rev 2026-06-27:** v2 design adds named run directories, a first-class research phase, generated user-facing HTML artifacts, and persistent shell waiting. Legacy v1 still uses `.dvandva/baton.json`; v2 runs use `.dvandva/runs/<run_id>/baton.json` with `schema: "dvandva.baton.v2"`, `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `active_roles`, `work_split`, `agent_instances`, `subagent_tracks`, and `verification_matrix`.
>
> **Spec rev 2026-06-28:** Run 4 adds generalized `work_split` path gates, repo-local git work-gating, and safe Dvandva-only retirement of replaced standalone user agents. The write helper applies `safe_rel_path` to `work_split.paths`, `work_split.read_paths`, and `work_split.write_paths`; for write-capable chunks, `write_paths` supplements rather than narrows `paths`, so the effective write set is their union; live write-capable chunks collide unless they share a `conflict_group` and an explicit `depends_on` serialization edge; `cross_review` remains read-only unless explicit `write_paths` are present. The git gate is local shell/git-hook enforcement (`core.hooksPath=.githooks`, `DVANDVA_ROLE`, `Dvandva-Checkpoint`, drift lint), not a daemon or hidden central process. Retirement is limited to Dvandva-covered workflows: the five Claude symlink agents `adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and `sandbox-executor`; functional parity is justified by equivalent-or-better usage across Runs 1-4 plus 0.4.0 cache/roster parity and reversibility. Codex agent-axis retirement is a no-op, skills are out of scope, and the helper writes a backup manifest with restore support.

## 1. What it is

Dvandva v1 is a pair of agent skills, written to the [agentskills.io](https://agentskills.io) open standard, that encode a disciplined two-agent collaboration protocol:

- `vadi` — the proposer/implementer skill. Runs in either Claude Code or Codex. Drives research, spec/plan creation, and phase implementation, then reviews any narrow fixups the prativadi makes.
- `prativadi` — the responder/reviewer skill. Runs in either Claude Code or Codex. Reviews research, Q&As during the spec phase, reviews each implementation phase, applies narrow fixups within an allowlist, and reviews the vadi's counter-changes when there is a disagreement.

Both skills share a baton file as the coordination channel. Legacy v1 uses `.dvandva/baton.json`. v2 adds named runs under `.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in one git worktree or directory as long as the human gives both sessions the same safe `run_id` or explicit `DVANDVA_BATON_FILE`. A safe `run_id` is one path segment: letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`; once a v2 baton exists, its `run_id` is immutable for that run. The default run mode is `walkaway`: the human gives an initial goal, starts or joins the two agent sessions once, and the skills use a cheap foreground wait helper when the baton assigns work to the other role. `supervised` mode is the serial fallback for one-engine runs: assigned-away agents exit and the human invokes the next role manually.

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each role turn: `superpowers:using-superpowers` before action, `superpowers:brainstorming` before design, `superpowers:test-driven-development` before implementation, `superpowers:verification-before-completion` before success claims, `superpowers:writing-skills` when skills change, and `superpowers:dispatching-parallel-agents` / `superpowers:subagent-driven-development` when parallel tracks exist. If the active engine cannot invoke the relevant Superpowers skills, the Dvandva role must stop, surface setup instructions, and avoid writing a success or advancement baton.

The v2 flow has eight lifecycle segments:

1. **Research phase** — vadi writes `research_ref`, a generated user-facing HTML artifact with machine-readable metadata, after conditional parallelism covers codebase, docs, tests, risks, and work distribution. The baton records `work_split`, `subagent_tracks`, and `verification_matrix`. Parallelize only genuinely disjoint tracks; when a track is not parallelized, record what was not parallelized and why.
2. **Master-planning phase** — collaborative plan creation. Vadi drafts; prativadi Q&As; vadi revises. Either role may ask the user questions while the plan is still unlocked. Loop until plan converges. The generated `plan_ref` HTML declares N implementation phases.
3. **Implementation phase** — vadi and prativadi implement phase N chunks in team-owned `parallel_implementing` without silently mixing implementation, testing, and review responsibilities.
4. **Test-creation phase** — vadi creates or updates tests for every new behavior and records a 100% test coverage target in `verification_matrix`; source-only docs/skills get lint/review coverage with rationale.
5. **Cross-review phase** — both roles review peer-owned chunks and record `cross-review` subagent tracks before any deep review begins.
6. **Deep-review phase** — prativadi performs independent deep review after implementation, test creation, and reciprocal cross-review. Review is separate from test creation and must inspect code, tests, docs, baton fields, and claims. When subagent tooling is available, use at least three angle-specific reviewers: correctness/regression, test/evidence, and protocol/handoff.
7. **De-slop phase** — vadi/prativadi loop on nits, low/minor bugs, stale wording, vague instructions, duplicated logic, and generated-looking clutter until no findings remain except items explicitly accepted in `deferred`.
8. **Phase advancement or completion** — on agreement, make a regular local checkpoint commit for the verified logical slice when `allow_commit` permits it, then advance to phase N+1. On completion of the final phase, write `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`, set `run_explainer_ref`, require both final approvals, optionally push, then transition to `done`.

implementation-phase parallelism is mandatory in v2. Spec approval enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the phase `work_split` must contain at least five implementation chunks split across both roles for two-team parallel implementation. Every implementation chunk names reciprocal `cross_review_by`, and `test_creation` routes to `cross_review` before `deep_review`. If cross-review finds peer-chunk defects, the phase routes through `cross_fixing` and then back to `test_creation` before review continues.

Run 4 extends that phase split into an enforceable path contract. `work_split.paths` describes the human-readable file surface; `read_paths` can narrow read-only review surfaces; `write_paths` adds explicit write intent but does not narrow `paths` for write-capable chunks. For backward compatibility, bare `paths` still imply write intent only for implementation and cross-fixing chunks, and the helper unions `paths` with `write_paths` for those chunks. Cross-review chunks are read-only by default unless they declare `write_paths`. The write helper rejects unsafe relative paths, absolute paths, parent traversals, and live write overlaps; the only allowed overlap is serialized with a shared `conflict_group` and an explicit `depends_on` edge. Terminal work_split chunks are completed historical work and do not block later path reuse because work_split has no `base_checkpoint` wave model.

Legacy enforcement starts with the agent checklist embedded in each SKILL.md and `/goal` evaluator transcript checks. The bundled write helper now enforces the supported v1/v2 schema strings, required fields, checkpoint arithmetic, safe run IDs, v2 status-owner pairs, and transition subset. A future standalone CLI validator backed by a full JSON Schema file can replace the remaining checklist-only validation.

The product is the `dvandva` plugin, its bundled protocol/orchestration skills, plugin-local baton references, bundled wait helpers, an install/usage doc, and a pilot case study. It coordinates work through baton state and skill checklists; it does not add an agent launcher, daemon, or GitHub integration.

Subagent execution uses the canonical Dvandva subagent roster (15 agents) under `plugins/dvandva/agents/`. This roster replaces the earlier personal `claude-skills/agents` roles for Dvandva work and includes `dvandva-researcher`, `dvandva-architect`, `dvandva-pattern-mapper`, `dvandva-implementer`, `dvandva-test-creator`, `dvandva-debugger`, `dvandva-cross-reviewer`, `dvandva-adversarial-analyst`, `dvandva-deep-reviewer`, `dvandva-security-auditor`, `dvandva-integration-checker`, `dvandva-doc-verifier`, `dvandva-deslopper`, `dvandva-sandbox-verifier`, and `dvandva-baton-auditor`. The design takes GSD-style fresh-context subagents for bounded heavy work and OMO-style team roles for specialization, but preserves Dvandva's core constraint: the baton remains the only coordinator. There is no two-subagent ceiling; each phase uses conditional parallelism and may spawn as many independent tracks as the harness can safely run without shared-state conflicts. Specialist agents are trigger-gated, not mandatory seed work: use security, integration, doc-verification, debugger, and pattern-mapping agents when their phase/risk trigger applies. Codex-specific lifecycle note: completed subagents can remain open and keep counting against the thread/concurrency limit, so the controller must explicitly close each subagent handle after consuming its result.

Run 3 turns this canonical roster into a **seed roster** for run-scoped dynamic agent generation. Parent roles may generate additional named agent instances on demand; each instance is recorded in `agent_instances` on the baton — a first-class array separate from the post-hoc `subagent_tracks` record. Every `agent_instances` entry carries its identity, parent role, seed agent, model/permission class, read/write paths, base checkpoint, lifecycle state (`planned`/`running`/`closed`/`rejected`/`collapsed`), output refs, evidence refs, and close result. Generated agents observe three invariants: (1) **single-writer merge** — they never write the baton directly and never own `assignee`, `active_roles`, phase transitions, or final approvals; the parent role waits for all handles to close and serializes evidence into one monotonic checkpoint; (2) **explicit closure** — every generated handle must be explicitly closed after result consumption, harness-specific closure proof (e.g. `closed:<handle>`) is required before a track counts as complete, and the instance must carry non-empty `work_item_ids`; (3) **dynamic write-path disjointness** — write-path overlaps between generated instances are rejected when they share the same `base_checkpoint` or when both instances are live (`planned`/`running`) unless they share a `conflict_group` with explicit dependency serialization; closed historical instances from earlier base checkpoints do not block later sequential path reuse. `seed_agent` is advisory provenance for humans; executable ownership comes from `spawned_by`, the generated instance `id`, and the parent role. There is no daemon, no background scheduler, and no hidden orchestrator outside the baton and foreground wait helper. Generated instances are run-scoped and ephemeral; the seed roster is never modified at runtime, and a pattern may be promoted to the seed roster only through a later reviewed source change.

Dvandva model classes are vendor-neutral. Agent frontmatter uses `model: opus` and `model: sonnet` as class labels, not Anthropic-only product IDs. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Do not use `haiku` for Dvandva subagents.

**PR 353 provenance.** PR 353 proved the need for a durable handoff surface, explicit ack/ownership flips, reviewer findings that can become fixes, and a cheaper alternative to agent-to-agent PR comment traffic. The v1 mutual-review loop, disagreement cap, turn cap, `human_decision` terminal, and baton transition table are product design responses to that evidence; they were not themselves fully exercised as named states in PR 353. The pilot exists to validate those new protocol pieces.

## 2. Audience and success criteria

**Primary audience:** the spec owner and any teammate using Claude Code + Codex 0.130+, both with the Superpowers plugin installed.

**v1 ships successfully when all five hold:**

1. The repo contains `plugins/dvandva/skills/vadi/SKILL.md` and `plugins/dvandva/skills/prativadi/SKILL.md` written to the agentskills.io standard, plus a baton template and an install/usage README that covers Superpowers prerequisites.
2. A teammate can follow the README — including the Superpowers install step — and run a Dvandva pilot on a low-risk real PR without DM-ing the owner. Two persistent sessions are the default; one engine playing both roles serially is a fallback.
3. One pilot is completed: spec phase converges, ≥2 implementation phases run, ≥1 mutual-review loop triggers, and one disagreement-loop event occurs and resolves (or terminates correctly at human escalation). Metrics — turn count per agent, agent-to-agent PR comment count, wall-clock, real issues caught — are written up as `docs/case-studies/pilot-01.md` against the PR 353 baseline.
4. In the pilot, both skills auto-activate from natural workflow language at least once each. Explicit invocation (`/vadi`, `$prativadi`) stays as documented fallback.
5. No runaway loops. The disagreement-round cap (default 3) triggers a forced `human_decision` correctly when exercised, and the wait helper wakes/stops cleanly at every baton-state transition.

If criterion #5 fails (any runaway loop observed during pilot), v1 does not ship — the cap mechanism is the operational safety floor and has to work.

## 3. Scope

### 3.1 In v1

- `plugins/dvandva/skills/vadi/SKILL.md` — frontmatter (portable `name` + `description`), body covering research drafting/revision, spec drafting, spec revision, phase implementation, phase fixing, prativadi-fixup review, the baton schema, the `/goal` exit conditions, and the disagreement-cap behavior.
- `plugins/dvandva/skills/prativadi/SKILL.md` — same shape, covering research review, spec Q&A, phase review, vadi-counter review, narrow-fix allowlist, handback conditions, baton schema, `/goal` exit conditions.
- `plugins/dvandva/skills/research/SKILL.md` — shared Dvandva research workflow used by both roles to produce `research_ref`, `work_split`, and `verification_matrix`.
- `plugins/dvandva/skills/testing/SKILL.md` — Dvandva-native test-creation and test-gap workflow, invoked during `test_creation` and review sandbox steps. Absorbed from the standalone `testing` skill; replaces it for all Dvandva work.
- `plugins/dvandva/skills/understanding/SKILL.md` — Dvandva-native mastery-gated teaching of the run, code, and decisions; grounded in baton/diff/`research_ref`/`plan_ref`; exports an HTML mastery checklist. Absorbed from the standalone `understanding` skill; replaces it for all Dvandva work.
- `plugins/dvandva/skills/worktree-setup/SKILL.md` — Dvandva-native worktree preparation with generic and optional DeFi profile conventions. Absorbed from the standalone `worktree-setup` skill; replaces it for all Dvandva work.
- `plugins/dvandva/skills/*/scripts/dvandva-wait.sh` — foreground shell wait helper bundled as a real executable in each runtime skill directory. It polls `.dvandva/baton.json` cheaply, without spending model turns, until the baton returns to the role or reaches a terminal human/done state.
- `plugins/dvandva/skills/*/scripts/dvandva-write.sh` — validated atomic baton installer; rejects illegal v1 transitions, installs via tmp+mv, auto-snapshots. Tested by `scripts/test-dvandva-write.sh`.
- `scripts/test-dvandva-wait.sh` — focused shell tests for the helper's exit-code contract.
- `scripts/lint-artifacts.sh` — generated-artifact policy lint. It rejects generated Markdown under `./superpowers/` and requires generated HTML artifacts to be dark, self-contained, offline-renderable, and backed by a machine-readable Dvandva metadata block.
- `plugins/dvandva/references/baton-schema.json` — bundled legacy v1 schema seed with all required keys (`run_mode`, `phase`, `total_phases`, `plan_ref`, `master_plan_locked`, `question`, `resume_assignee`, `resume_status`, `disagreement_round`, `disagreement_cap`, `turn_cap`, `review_target`, `current_engine`, final-approval fields, etc.).
- `README.md` install section covering: marketplace install (primary), development symlink/copy install (fallback), and the Superpowers hard-dependency check for every engine running a Dvandva role.
- One pilot writeup at `docs/case-studies/pilot-01.md` after the workflow ship.

### 3.1a In v2 design

- `dvandva.baton.v2` — run-scoped baton schema for `.dvandva/runs/<run_id>/baton.json`. Required v2 fields include safe `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, `active_roles`, and `agent_instances`. v1 remains valid only for the legacy `.dvandva/baton.json` fallback. Run 3 adds `agent_instances` — a first-class baton array for generated run-scoped agent instances recording identity, parent role, seed agent, model/permission class, read/write paths, base checkpoint, lifecycle state, output refs, evidence refs, and close result. `agent_instances` is separate from the post-hoc `subagent_tracks` record and is validated by the Run 3 write helper for: safe ids, no duplicates or reserved owner-name collisions, supported model/permission classes, matching closed registry records for any dynamic `subagent_tracks` owner not in the seed roster, closure evidence plus non-empty `work_item_ids` before a track counts complete, and dynamic write-path disjointness among generated instances sharing the same `base_checkpoint` or among any two live (`planned`/`running`) instances regardless of base checkpoint. The live v2 write-helper enforcement covers v2-only fields, schema continuity for existing runs, v2 status-owner pairs, honest `subagent_tracks`, and v2 lifecycle transitions intentionally instead of by convention.
- Run 4 fields and conventions for `work_split`: write-capable chunks should declare `write_paths`; read-only review chunks may declare `read_paths`; overlapping writers require `conflict_group` plus `depends_on`; `cross_review` has no write intent unless explicit `write_paths` are present.
- Research lifecycle states before spec lock: `phase: "research", status: "research_drafting"` for vadi research synthesis, `research_review` for prativadi independent review, and `research_revision` for vadi response to research findings. v2 scaffolds new named runs at `research_drafting`; legacy v1 scaffolds at `spec_drafting`.
- Test and review lifecycle states are separate in v2: `test_creation` records the doer's tests and coverage evidence, `deep_review` records independent prativadi review after tests exist, and `deslop` records cleanup loops for nits, low/minor bugs, stale wording, and unclear instructions. A phase does not advance while unresolved `deep_review` or `deslop` findings remain unless explicitly accepted in `deferred`.
- Team-owned v2 states (`parallel_implementing`, `cross_review`, `cross_fixing`) may write same-status sync checkpoints to record partial completion, task distribution, or peer wait state without pretending the phase is ready to advance. Scalar-owner states still reject same-status rewrites.
- Phase convention: implementation-chunk tracks use the numeric implementation phase, while cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.
- The generated user-facing artifacts are HTML: plans, research reports, evaluations, reviews, pilot write-ups, and run reports. Every completed v2 run must produce `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html` with decisions, development, architecture, verification, and diagrams. Platform/source Markdown such as `SKILL.md`, command files, README/source docs, and prompt templates stays in its native format.
- Continuous polling is the hard rule. `dvandva-wait.sh` treats `--max-wait` as a heartbeat interval by default and keeps polling until the selected baton assigns the role, reaches `done`/`human_question`/`human_decision`, or the user interrupts. `--persist` is accepted for older call sites and is now redundant. `--persist-max <seconds>` is a shell-budget cap; wait-helper persist cap exit 23 is not proof the peer is done and the role must immediately re-enter the wait unless the user interrupts. The write-helper validation exit 23 means a candidate failed schema/required-key/status validation. Finite exit 20 is available only through explicit `--finite` compatibility mode and is not valid for normal walkaway loops.
- Claude Code has a Bash-tool cap around 600 seconds, so Claude-hosted sessions must relaunch the wait if the harness stops the shell before a terminal baton state. Codex-hosted sessions may use unbounded default continuous polling or pass `--persist` for older snippets; both are the same continuous wait contract.

### 3.2 Out of v1 (non-goals)

- No standalone `dvandva` binary and no full JSON Schema validator script. Enforcement is the skill-body checklist plus `/goal` transcript surfacing, the wait helper's exit-code contract, and the write helper's deterministic validation for the supported v1/v2 transition subset. A complete JSON Schema validator remains future work.
- No runtime runner / daemon / process launcher. The user starts the two interactive sessions; in walkaway mode, the skills keep those sessions alive by blocking in the wait helper when assigned away.
- No GitHub integration. No PR comment posting. Skills tell the agent what to surface in transcript; humans write any PR comments using the baton as source material.
- No multi-engine enforcement. v1 does not verify which engine is running a given role. The `current_engine` field on the baton records which CLI wrote each checkpoint for traceability, but the protocol does not require a particular pairing. The canonical pairing (vadi=Claude, prativadi=Codex) is documented but not enforced.
- No separate `dvandva-init` skill. The vadi skill scaffolds `.dvandva/` inline on first run.
- No official marketplace-directory submission and no npm-first distribution. v1 is a GitHub-hosted plugin marketplace package.
- v2 supports multiple named batons per repo/worktree via safe run directories. It does not isolate the git index; overlapping `changed_paths` between active runs must route to `human_decision` before final ship.
- No PR creation. Walkaway mode may create local checkpoint commits after verified logical slices and may push only after dual final approval, but it must not raise a PR.

## 4. Prerequisites (hard requirement before pilot)

At least one prerequisite engine must be verified before the pilot can run. The skill bodies must check the most-likely-to-fail prerequisite (Superpowers availability) at preflight.

You need at least one of {Claude Code, Codex} with the Superpowers plugin installed. If you have both, the canonical setup pairs them (vadi=Claude Code, prativadi=Codex), and both engines must have Superpowers. If you have only one, both roles run in that one engine sequentially with Superpowers installed.

| Prerequisite | Why | How to verify |
|---|---|---|
| Claude Code installed (optional if using Codex for both roles) | Can host either skill | `which claude && claude --version` |
| Codex CLI ≥ 0.130 (optional if using Claude Code for both roles) | Can host either skill; supports skills + `/goal` | `codex --version` |
| Superpowers plugin on every engine running a Dvandva role | Hard runtime dependency: Dvandva coordinates; Superpowers supplies the active-work discipline, including brainstorming, TDD, verification-before-completion, skill-writing discipline, and parallel-agent execution | On Claude Code: `claude` then `/skills`. On Codex: capability check — Codex must show the Superpowers skills in its available skills. |
| Git repo with a feature branch | The dvandva flow assumes a branch | `git rev-parse --abbrev-ref HEAD` returns something other than the main branch |
| `jq` installed | The wait helper reads baton fields with jq | `jq --version` |

Each role's preflight refuses active work and surfaces a clear install hint if the current session cannot invoke the relevant Superpowers skills. It must not hardcode a single filesystem path.

## 5. Repo layout

```
.claude-plugin/
  marketplace.json
plugins/
  dvandva/
    .claude-plugin/plugin.json
    .codex-plugin/plugin.json
    skills/
      research/SKILL.md
      testing/SKILL.md
      understanding/SKILL.md
      worktree-setup/SKILL.md
      vadi/SKILL.md
      prativadi/SKILL.md
    agents/
      researcher.md
      architect.md
      pattern-mapper.md
      implementer.md
      test-creator.md
      debugger.md
      cross-reviewer.md
      adversarial-analyst.md
      deep-reviewer.md
      security-auditor.md
      integration-checker.md
      doc-verifier.md
      deslopper.md
      sandbox-verifier.md
      baton-auditor.md
    references/
      baton-schema.json
      local-baton-channel.md
      state-transition-table.md
docs/
  case-studies/
    pr-353.md           # existing baseline
    pilot-01.md         # written after the pilot
  protocol/
    local-baton-channel.md  # follow-up commit aligns with this spec's transition table
  workflows/
    two-mode-agent-workflow.md  # existing
README.md               # install + usage (covers superpowers prereq)
product.md              # this file
```

The existing `templates/prompts/claude-doer-goal.md` and `templates/prompts/codex-reviewer-goal.md` are demoted from active templates to historical artifacts (a README note explains they were the v0 form of what the skills now are; files stay in-tree as reference). (Note: these template filenames use the old v0 naming and are kept as-is since they are historical reference only.)

## 6. Flow overview

The v2 flow has eight segments and an end state: research, master planning, implementation, test_creation, cross_review, deep_review, deslop, and phase advancement/completion. Every arrow in the diagram is a baton write by the active agent. In default walkaway mode, the other persistent session is already blocked in `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh`; the helper returns when the baton assigns that role, and the agent re-enters preflight.

```
                  ┌──────────────────────────────────┐
                  │ RESEARCH PHASE                   │
                  │  phase: "research"               │
                  │                                  │
   start ───▶ Vadi (research_drafting)                │
                  │   Invoke dvandva:research         │
                  │   conditional parallelism         │
                  │   writes research_ref HTML        │
                  │   records work_split              │
                  │   records verification_matrix     │
                  │   baton → research_review         │
                  ▼                                  │
              Prativadi (research_review)             │
                  │   independent research review     │
                  │   may route findings to vadi      │
                  │   baton → research_revision       │
                  │    or → spec_drafting             │
                  ▼                                  │
              Vadi (research_revision)                │
                  │   updates research_ref / fields   │
                  │   baton → research_review (loop)  │
                  └──┬───────────────────────────────┘
                     │
                     │ research accepted
                     │ research_ref/work_split/
                     │ verification_matrix ready
                     ▼
                  ┌──────────────────────────────────┐
                  │ MASTER-PLANNING PHASE            │
                  │  phase: "spec"                   │
                  │                                  │
   start ───▶ Vadi (drafting — Claude or Codex)       │
                  │   uses superpowers:brainstorming  │
                  │   + superpowers:writing-plans     │
                  │   writes ./superpowers/plans/...  │
                  │   stores plan_ref on baton        │
                  │   baton → spec_review             │
                  ▼                                  │
              Prativadi (Q&A — Claude or Codex)       │
                  │   uses superpowers:brainstorming  │
                  │   may ask human while plan        │
                  │   is unlocked                     │
                  │   may edit plan_ref plan          │
                  │   baton → spec_revision (vadi)    │
                  │    or → phase 1 parallel_implementing │
                  ▼                                  │
              Vadi (revision)                         │
                  │   addresses prativadi Q&A         │
                  │   may ask human while plan        │
                  │   is unlocked                     │
                  │   baton → spec_review (loop)      │
                  └──┬───────────────────────────────┘
                     │
                     │ plan_ref plan converged
                     │ master_plan_locked: true
                     │ total_phases set
                     │ baton phase: 1, status: parallel_implementing
                     ▼
   ┌─── PER-PHASE LOOP (for phase N in 1..total_phases) ───┐
   │                                                       │
  │   Team (parallel_implementing phase N)                │
   │     uses superpowers:test-driven-development          │
   │     baton → test_creation                             │
   │       ▼                                               │
   │   Vadi (test_creation)                                │
   │     creates/updates tests and coverage evidence       │
   │     targets 100% test coverage for new behavior       │
   │     baton → cross_review                              │
   │       ▼                                               │
   │   Vadi + Prativadi (cross_review for phase N)         │
   │     reciprocal review of peer-owned chunks            │
   │     baton → cross_fixing or deep_review               │
   │       ▼                                               │
   │   Prativadi (deep_review for phase N)                 │
   │     independent review after tests exist              │
   │     decides: approve / fix narrowly / hand back       │
   │       │                                               │
   │       ├─ approve, no changes ──▶ deslop               │
   │       │       ▼                                       │
   │       │   de-slop pass fixes nits/low/minor issues    │
   │       │       │                                       │
   │       │       ├─ clean ──▶ phase N+1 (or done)        │
   │       │       └─ findings ──▶ phase_fixing            │
   │       │                                               │
   │       ├─ apply narrow fixup ──▶ MUTUAL REVIEW         │
   │       │     baton → review_of_review                  │
   │       │     review_target: prativadi_fixups           │
   │       │     assignee: vadi                            │
   │       │       ▼                                       │
   │       │   Vadi (reviewing prativadi fixups)           │
   │       │       │                                       │
   │       │       ├─ approve ──▶ phase N+1 (or done)      │
   │       │       │                                       │
   │       │       └─ disapprove ──▶ DISAGREEMENT LOOP     │
   │       │             disagreement_round += 1           │
   │       │             Vadi writes counter-change        │
   │       │             baton → counter_review            │
   │       │             review_target: vadi_counter       │
   │       │               ▼                               │
   │       │           Prativadi reviews counter-change    │
   │       │               │                               │
   │       │               ├─ approve ──▶ phase N+1        │
   │       │               │                               │
   │       │               ├─ disapprove, propose new fix ─┘
   │       │               │    (loop back to mutual review)│
   │       │               │                               │
   │       │               └─ disagreement_round ≥ 3 ────┐ │
   │       │                   baton → human_decision   │ │
   │       │                                            │ │
   │       └─ hand back (substantive issues) ──▶ Vadi  │ │
   │             baton → phase_fixing                   │ │
   │             findings array populated                │ │
   │             Vadi fixes, hands back to Prativadi     │ │
   │             (re-enters prativadi review at top)     │ │
   │                                                     │ │
   └─── on phase N+1 ──▶ Team (parallel_implementing)   │ │
                                                         │ │
                                                         ▼ ▼
                                                  ┌───────────┐
                                                  │  human    │
                                                  │  decision │
                                                  └───────────┘
                                                       │
                                                       ▼
                                                 (human edits
                                                  baton, restarts)

   Final phase complete + both final approvals true
      → optional commit/push if allowed
      → status: done → cycle ends
```

Phase advancement invariant: the vadi never advances a phase directly after implementation or fixing. Advancement is legal only when the prativadi approves the vadi's implementation with no changes, the vadi approves the prativadi's narrow fixups, or the prativadi approves the vadi's counter-change in the disagreement loop. The agent writing the first baton for the next phase must set `disagreement_round: 0`.

Three caps the spec enforces operationally:

- **Disagreement round cap (default 3).** Counter resets at the start of each phase. On the 3rd mutual-review disapproval, the writing agent must set `status: human_decision` and exit. Tunable per-phase via a `disagreement_cap` field on the baton (set during spec phase by either agent).
- **Per-invocation turn cap (default 60).** Each agent's `/goal` invocation must stop after the active model-work turn cap even if the baton condition has not been hit, and surface its current state for human review. Passive shell wait heartbeats do not count against this cap.
- **No phase count cap.** Plans declare `total_phases` during the spec phase; the protocol does not constrain how many phases are reasonable. The spec phase itself is responsible for sane phase scoping.
- **Planning-question boundary.** Before `master_plan_locked: true`, either agent may route to `human_question`. After the master plan is locked, agents should resolve internally or escalate with `human_decision`.

## 7. vadi skill design

### 7.1 Frontmatter

- `name: vadi`
- `description:` one paragraph, front-loaded with trigger words: *implement*, *vadi*, *spec*, *plan with review*, *phased implementation*, *hand off for review*, *review the prativadi's fixups*, *review codex's fixups*. Must list both spec-phase triggers and implementation-phase triggers since one skill handles both. Under the 1,536-char listing cap.

No `allowed-tools` reliance (see section 9). Optional Claude-only `argument-hint: "[task description]"` for UX.

### 7.2 Body sections (target < 500 lines)

1. **Role one-liner** — "You are the Dvandva vadi. You draft plans, implement them phase by phase, and review the prativadi's narrow fixups."
2. **Preflight (all modes)** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then Existing baton discovery across `.dvandva/runs/*/baton.json` plus legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If active batons exist and no selector is explicit, ask the user whether to continue or create a new run; if only terminal batons exist, auto-create a new named run. If the baton is absent, the vadi scaffolds the resolved directory and writes the seed baton through `dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` with `original_ask` preserved in initial context. If the baton is assigned away and `run_mode: "walkaway"`, wait continuously on `"$BATON_FILE"` until a terminal baton state, role ownership, or user interrupt; shell caps must re-enter the wait. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role.
3. **Mode R1: research drafting** — when `phase: "research", status: "research_drafting"`. Invoke `dvandva:research`, preserve `original_ask`, use conditional parallelism when available, write the generated HTML `research_ref`, populate `work_split`, `subagent_tracks`, and `verification_matrix`, and hand to prativadi with `status: research_review, review_target: research`.
4. **Mode R2: research revision** — when `phase: "research", status: "research_revision"`. Invoke `dvandva:research`, address prativadi research findings, update `research_ref`, `work_split`, and `verification_matrix`, clear resolved findings, and hand back to `research_review`.
5. **Mode A: spec drafting** — when `phase: "spec", status: "spec_drafting"`. Read `research_ref`, `work_split`, and `verification_matrix` first. Invoke `superpowers:brainstorming` skill flow without rediscovering already-settled research. The vadi may ask the user questions if required before the master plan is useful. Produce a gitignored dark self-contained HTML plan under `./superpowers/plans/YYYY-MM-DD-<topic>.html` with declared `total_phases` and a per-phase scope list. Set `plan_ref`, `total_phases`, and `master_plan_locked: false` on the baton. Write baton with `status: spec_review, assignee: prativadi, review_target: spec`.
6. **Mode B: spec revision** — when `phase: "spec", status: "spec_revision"`. Read the baton's `findings` array (prativadi's Q&A), respond in the `plan_ref` plan, update affected `total_phases` if scope changed. Always write baton with `status: spec_review, assignee: prativadi, review_target: spec`; the prativadi is the only actor that can advance the spec to phase 1. Follow the stop/wait rule.
7. **Mode C: phase implementation** — when `phase: 1..N, status: "parallel_implementing"` for v2, or `"implementing"` only for an explicitly selected legacy v1 run. Read the corresponding phase scope from the `plan_ref` plan and the relevant `work_split` / `verification_matrix` entries. Invoke `superpowers:test-driven-development` when applicable. Implement only the phase scope; do not bleed into adjacent phases. V2 writes baton with `status: test_creation, assignee: vadi, review_target: null` after both roles record implementation evidence.
8. **Mode T: test creation** — when `phase: 1..N, status: "test_creation"`. Create or update tests for every new behavior, record 100% test coverage evidence or source-only rationale in `verification_matrix`, run motivating tests and cheap checks, then write baton with `status: cross_review, assignee: team, active_roles: ["vadi", "prativadi"], review_target: implementation`. Test creation is separate from review.
9. **Mode D: phase fixing** — when `phase: 1..N, status: "phase_fixing"`. Read `findings` from prativadi. Fix only listed items, update tests if behavior changed, and return through `test_creation` rather than directly to review. Follow the stop/wait rule.
10. **Mode S: deslop** — when `phase: 1..N, status: "deslop"`. Remove nits, low/minor bugs, stale wording, duplicated instructions, and generated-looking clutter found by deep review. Use conditional parallelism for independent style, protocol, and artifact-integrity tracks and record them in `subagent_tracks`. If no unresolved issues remain except explicitly accepted `deferred` items, advance to the next phase or final completion.
11. **Mode E: prativadi-fixup review** — when `status: "review_of_review", review_target: "prativadi_fixups", assignee: vadi`. Read the prativadi's `narrow_fixups` array and inspect the diff the prativadi applied. Decide: approve or disapprove.
   - On approve: legacy v1 explicit runs may write baton with `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` (advance), or `status: done` after final approval/ship if N was the final phase. V2 returns to the review/deslop lifecycle and advances only through the `deslop` gate. Follow the stop/wait rule.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write counter-changes inline, write baton `status: counter_review, review_target: vadi_counter, assignee: prativadi`. Follow the stop/wait rule.
12. **Regular checkpoint commits** — after any active mode changes files and the relevant verification commands pass, make a local checkpoint commit when `allow_commit == true`. Commit only the intended `changed_paths` union, excluding `.dvandva/` and `superpowers/`, and only when `git status --short` has no unrelated dirty paths. Use one logical change per commit, semantic prefix, and a subject of 50 characters or fewer. Record the commit hash in `verification` or `summary` as `checkpoint_commit=<hash>`. Do not push checkpoint commits.
13. **Final ship rule** — before terminal `done`, write the final explainer HTML at `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`, set `run_explainer_ref`, and push only when `allow_pr: false`, `allow_push: true`, `vadi_final_approval: true`, and `prativadi_final_approval: true`. If intended dirty files remain and `allow_commit == true`, make one final local commit first; if no dirty intended files remain because checkpoint commits already captured the work, record `final_commit` as `git rev-parse HEAD`. Unrelated dirty paths force `human_decision`. Record `final_commit` and `pushed_ref`. Never create a PR.
14. **Stop rule (universal)** — in walkaway mode, do not stop on role handoff or slow peer work. Surface BATON_STATE, run the wait helper, and continue from preflight when the baton returns. Continuous shell polling stops only for `done`, `human_question`, `human_decision`, user interrupt, or turn-cap escalation during active model work. `human_question` and `human_decision` remain legal from early v2 research even before `research_ref` exists, so missing setup can be surfaced before the first research artifact is written.
15. **`/goal` condition** — embedded in the skill body verbatim, centered on continuing until `done`, `human_question`, or `human_decision`; if assigned away, block in the wait helper instead of spending model turns.
16. **Failure modes** — section 12.

## 8. prativadi skill design

### 8.1 Frontmatter

- `name: prativadi`
- `description:` front-loaded triggers: *review*, *spec Q&A*, *prativadi pass*, *narrow fixups*, *adversarial verification*, *check the baton*, *review the vadi's counter-change*, *review claude's counter-change*. Covers all three prativadi modes.

### 8.2 Body sections

1. **Role one-liner** — "You are the Dvandva prativadi. You Q&A on plans, review implementation phases, apply narrow fixups, and review the vadi's counter-changes."
2. **Preflight** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then Existing baton discovery across `.dvandva/runs/*/baton.json` plus legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If no explicit selector exists and exactly one active/resumable baton exists, select it; if several exist, surface the candidates and wait for a human choice; if none exists, wait continuously on the selected or would-be named-run baton with `--allow-missing` unless `DVANDVA_NO_WAIT=1` is set. If the baton is assigned away and `run_mode: "walkaway"`, wait continuously on `"$BATON_FILE"` until role ownership, terminal baton state, or user interrupt; shell caps must re-enter the wait. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role. **Additionally verify `superpowers:brainstorming` is available in the current session** before spec Q&A; if absent, surface install instructions and exit (per section 4 prerequisites). Do not depend on one fixed filesystem path.
3. **Mode R: research review** — when `phase: "research", status: "research_review", review_target: "research"`. Invoke `dvandva:research` for independent research review. Do not rely solely on the vadi's `research_ref`; inspect relevant sources and use conditional parallelism when available. If gaps remain, populate `findings` and write `status: research_revision, assignee: vadi`. If research is sufficient, advance to `phase: "spec", status: "spec_drafting", assignee: "vadi"`, preserving `research_ref`, `work_split`, `subagent_tracks`, and `verification_matrix`.
4. **Mode A: spec Q&A** — when `phase: "spec", status: "spec_review", review_target: "spec"`. Invoke `superpowers:brainstorming` skill flow as the questioner. Read the `plan_ref` plan, surface Q&A in the baton's `findings` array, optionally edit the plan directly for narrow improvements (typos, sharper phrasing). The prativadi may ask the user questions if required before the master plan can be approved or handed back. Decide: hand back to vadi (questions remain) or advance. Write baton `status: spec_revision, assignee: vadi` for more Q&A. For v2 phase work, approve by writing `phase: 1, status: parallel_implementing, assignee: team, active_roles: ["vadi", "prativadi"], disagreement_round: 0, master_plan_locked: true`; legacy v1 explicit runs use `phase: 1, status: implementing, assignee: vadi`.
5. **Mode B: deep review** — when `phase: 1..N, status: "deep_review", review_target: "implementation"`. Read diff vs branch baseline only after `test_creation` is complete. Cross-check the vadi's `verification` block and the planned coverage in `verification_matrix` (did the commands actually pass? do they cover the changed paths and risks, and is 100% test coverage for new behavior documented?). Use at least three angle-specific reviewers/tracks in `subagent_tracks`: correctness/regression, test/evidence, and protocol/handoff; add `dvandva-adversarial-analyst` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses. Look for bugs, regressions, stale docs, missing tests, claims not matching diff, and deslop opportunities.
6. **Narrow-fix allowlist** (verbatim from `docs/workflows/two-mode-agent-workflow.md:41-47`):
   - Typographical and docs mistakes
   - Stale references in docs or audit rows
   - Small test expectation updates
   - Lint, formatting, or type errors with obvious fixes
   - Small missed edge cases that do not change architecture
6. **Handback conditions** (verbatim from the same doc):
   - Product behavior changes
   - Architecture changes
   - Schema migrations
   - Shared infra changes
   - Dependency removals or major dependency additions
   - Ambiguous requirements
7. **Decision branching (from Mode B)** —
   - If only handback issues: populate `findings`, write baton `status: phase_fixing, assignee: vadi`. Exit.
   - If narrow fixups apply AND no handback issues: apply fixups inline, run verification, populate `narrow_fixups` array. Write baton `status: review_of_review, review_target: prativadi_fixups, assignee: vadi` (route to mutual review). Exit.
   - If narrow fixups apply AND handback issues: populate both `findings` and `narrow_fixups`; route to `phase_fixing` first; mutual review of the narrow fix happens on the next prativadi pass after the vadi's fix.
   - If approve, no changes: for v2, write `status: deslop, assignee: vadi` so cleanup runs before phase advancement; legacy v1 explicit runs may write `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` or `status: done` after final approval/ship if final phase. Follow the stop/wait rule.
8. **Mode C: vadi-counter review** — when `status: "counter_review", review_target: "vadi_counter", assignee: prativadi`. Read the vadi's counter-change diff. Decide:
   - On approve: v2 counters return to the normal review/deslop lifecycle; legacy v1 explicit runs may write baton `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` (advance), or `status: done` after final approval/ship if final phase. Follow the stop/wait rule.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write a new narrow fixup and route back to `review_of_review, review_target: prativadi_fixups, assignee: vadi`. Follow the stop/wait rule.
9. **Final ship rule** — same as vadi. The prativadi may commit/push only after both final approvals are true, the current dirty paths match `changed_paths`, PR creation remains false, `run_explainer_ref` points to `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`, and the HTML includes decisions, development, architecture, verification, and diagrams.
10. **Stop rule** — in walkaway mode, do not stop on role handoff. Surface BATON_STATE, run the wait helper, and continue from preflight when the baton returns. In supervised mode, exit on role handoff.
11. **`/goal` condition** — centered on continuing until `done`, `human_question`, or `human_decision`; if assigned away, block in the wait helper instead of spending model turns.
12. **Failure modes** — section 12.

## 9. Cross-engine portability

Both skills target the agentskills.io open standard. Only the universal frontmatter (`name`, `description`) carries correctness weight. Optional engine-specific fields are avoided in v1:

- **No `allowed-tools` reliance.** The agentskills.io spec treats it as implementation-varying. Skill bodies assume the user's existing permission setup allows git, bash, and the project's test runner. One-time tool prompts are acceptable; the skill does not depend on pre-approval.
- **No `paths` glob.** Skills are workflow-scoped, not file-scoped.
- **No `context: fork`.** Skills run in the main session so `/goal` transcript surfacing works (the goal evaluator only sees what's surfaced).
- **No engine-specific frontmatter extensions.** If forced in a future rev, the SKILL.md forks into engine-specific variants; document the reason explicitly.

Agent files are separate from `SKILL.md` portability. Their `model:` field is a Dvandva model-class hint, not an engine lock-in: `opus` means the strongest available planning/review/architecture class, and `sonnet` means the implementation/documentation workhorse class. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Dvandva never maps any agent to a `haiku` class.

**Superpowers compatibility note:** both engines must have Superpowers installed at runtime. The vadi and prativadi rely on Superpowers as the active-work discipline, not only as planning helpers: skill-use discipline before action, brainstorming before design, TDD before implementation/fixups, verification before completion, writing-skills discipline for skill edits, and parallel/subagent execution when tracks are independent. Skills invoke these via the engine's native skill tool. If Superpowers is absent, the active Dvandva role must stop, surface setup instructions, and avoid writing a success or advancement baton.

## 10. Description tuning strategy

Auto-activation depends entirely on `description`. Tuning rules:

- **Front-load trigger phrases.** Vadi description starts with draft/implement/phase language. Prativadi description starts with Q&A/review/counter-change language. Both descriptions mention Dvandva and paired-agent context without hardcoding a required engine.
- **At least three paraphrase variants** per description so partial matches still hit.
- **Explicit anti-trigger** in each: *"Do not use this skill for solo work not paired with the other agent."*
- **Calibration during pilot.** If a skill mis-fires or fails to fire, the pilot writeup records user phrasing → activation outcome, and the description gets one edit pass.

## 11. Distribution and install

### 11.1 Primary install (marketplace)

```bash
bash scripts/install.sh
```

`scripts/install.sh` wraps the public install path for users: it registers the Dvandva marketplace and installs `dvandva@dvandva` in both Claude Code and Codex. It accepts `--claude-only` and `--codex-only` for one-engine installs. For Codex, it delegates to `scripts/install-codex.sh`, which runs `codex plugin add dvandva@dvandva` on current Codex builds and keeps the legacy app-server RPC install as a fallback for older builds. The authoritative preflight is whether the current agent session can see and invoke the required Superpowers skills.

### 11.2 Development install fallback

For local development against a checkout, prefer marketplace install from the checkout:

```bash
bash scripts/install.sh "$(pwd)"
```

For live skill-development work where plugin cache copies are too indirect, symlink or copy `plugins/dvandva/skills/vadi/` and `plugins/dvandva/skills/prativadi/` into the engine skill directories. Remove old pre-plugin `dvandva-*` symlinks first because root `skills/` no longer exists.

### 11.3 Project-level adoption

Consumer repos may check the plugin into their own tree or use project-scoped marketplace declarations. Project-level skills can carry tool-permission frontmatter; review `SKILL.md` the same way you would any other `.claude/` or `.agents/` config.

## 12. Failure modes the skills must handle

| Failure | Required behavior |
|---|---|
| No selected/resumable baton after discovery | Vadi creates a named v2 run at `phase: "research", status: "research_drafting"` unless the user explicitly selected legacy `.dvandva/baton.json`. Prativadi waits on the selected or would-be named-run baton with `--allow-missing` unless `DVANDVA_NO_WAIT=1`; it does not create the run. |
| Baton present but malformed JSON | Both: do not overwrite. Surface parse error verbatim. Write `.dvandva/baton.broken.json` with the unparseable bytes preserved. Surface in-memory next state as `human_decision`. |
| `schema` field is not `dvandva.baton.v1` or `dvandva.baton.v2` | Both: refuse to operate. Surface schema mismatch. Exit. |
| `assignee` does not match this agent's role | In `run_mode: "walkaway"`, run the wait helper for this role. Outside walkaway, surface "wrong actor for this state" and exit. Never silently overwrite the assignee. |
| Superpowers absent on an engine running a Dvandva role | Surface install instructions referencing section 4 and the relevant plugin marketplace/install path. Do not continue with active work; if the baton exists and the role owns it, route to `human_decision` rather than writing a success or advancement baton. |
| `status: human_question` | Stop and surface the one concrete `question` plus `resume_assignee` and `resume_status`. If the user answers in the current prompt, restore `assignee` and `status` from those fields, clear the question fields, and continue. This is valid only before `master_plan_locked: true`; after that, use `human_decision`. |
| `plan_ref` missing, or referenced plan file missing during a phase mode | Doer: surface "spec phase did not complete; cannot start phase implementation." Exit. Reviewer: same. |
| `total_phases` is 0 or unset during phase mode | Both: surface schema integrity error, exit. The spec phase is responsible for setting this. |
| `disagreement_round >= disagreement_cap` | Whichever agent next writes the baton: set `status: human_decision, assignee: human`. Do not write further counter-changes. |
| `/goal` active-work turn cap hit before exit condition | Agent: surface current baton state and a "still owe work" summary, set `status: human_decision`, exit. The default active-work cap is 60; shell wait heartbeats do not count. |
| Wait helper exits 20 (`timeout`) | This can only happen in explicit `--finite` compatibility mode. Normal walkaway loops must not use `--finite`; immediately re-enter continuous wait unless the user interrupts. |
| Wait helper exits 23 (`persist_max`) | This is the wait-helper persist cap exit 23. Surface the still-waiting state and immediately re-enter the wait helper with a fresh cap unless the user interrupts. The cap protects shell budgets; it is not a baton terminal state and is not evidence the peer stopped. |
| Write helper exits 23 during candidate install | This is the write-helper validation exit 23. The candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation. Fix the candidate file and rerun `dvandva-write.sh`; never edit the installed baton directly. |
| Final phase approved by only one role | Do not commit or push. Keep routing until both `vadi_final_approval` and `prativadi_final_approval` are true, or escalate. |
| Commit or push fails in walkaway mode | Set `status: human_decision`, keep the working tree as-is, and record the failing command in `blockers`. Never try to create a PR as recovery. |
| Dirty paths outside `changed_paths` at final ship | Set `status: human_decision`; do not commit unrelated work. |
| Prativadi finds no diff vs baseline (after Claude said implementation done) | Write `findings: ["vadi claimed implementation but produced no diff"]`, route to `human_decision`. |
| Both agents accidentally started concurrently | v1 cannot detect. Skill body warns in preflight; deterministic detection is v2. |
| Git working tree dirty before spec phase starts | Doer: surface dirty state in baton `summary`, proceed only if user's prompt indicates intent. |
| `plan_ref` plan edited during spec Q&A and `total_phases` changed | Vadi spec-revision mode reads the new `total_phases` from the plan and updates the baton to match. **The plan referenced by `plan_ref` is authoritative during the spec phase; the baton is authoritative during implementation phases.** Once `phase: 1` is set, `total_phases` is frozen on the baton and the plan is treated as reference. |

## 13. Testing strategy

v1 has no automated test surface for skill behavior. What can be tested:

- **Frontmatter linter** (a small script committed to the repo): parses both SKILL.md files, confirms required frontmatter, checks `description` ≤ 1,536 chars, checks body ≤ 500 lines. Suggested pre-commit hook.
- **Schema key-presence check** (same script): the inlined `dvandva.baton.v1` JSON in each SKILL.md must parse as valid JSON and contain the required keys from Appendix A. Not a JSON Schema check — that's v2.
- **Generated artifact lint** (`scripts/lint-artifacts.sh`): rejects generated Markdown under `./superpowers/`, requires generated HTML artifacts to declare dark color scheme, parses embedded Dvandva artifact metadata, and rejects external script/link references.
- **Wait-helper tests** (`scripts/test-dvandva-wait.sh`): verifies the foreground helper exits 0 when a role is assigned, 10 on `done`, 11 on `human_decision`, 12 on `human_question` with resume fields, and 20 on timeout.
- **Installer tests** (`scripts/test-install.sh`, `scripts/test-install-codex.sh`): verify the dual Claude/Codex installer invokes both engine install paths and the Codex-only helper uses `codex plugin add` when available, with the app-server path preserved only as legacy fallback.
- **Plugin smoke test** (`scripts/smoke-plugin-install.sh`): copies the plugin into a temp marketplace, validates Claude plugin/marketplace metadata, runs Codex marketplace add with isolated `CODEX_HOME`, probes Codex runtime discovery after direct Codex plugin install, dual installer install, and `install-codex.sh` helper install, requires `dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup`, verifies both wait helpers, and checks standalone development copies.
- **Run 4 work-gate and retirement tests**: `scripts/test-dvandva-commit-gate.sh` covers repo-local `.githooks`, `DVANDVA_ROLE`, `active_roles`, `Dvandva-Checkpoint`, installer idempotence, and drift lint; `scripts/test-retire-standalone-agents.sh` covers dry-run, five-symlink apply, manifest restore, cache parity refusal, Codex agent-axis no-op, and no skill touches; `scripts/test-lint-run4-path-gates.sh` plus `scripts/test-lint-run4-standalone-agents.sh` keep source docs, manifests, helper scripts, and the 15-agent roster aligned with Run 4.
- **Pilot as integration test:** the pilot is the v1 integration test. Success criteria #1–#5 in section 2 are the acceptance gate.

## 14. Risks and open questions

Named risks, ordered by severity:

1. **Disagreement-cap mechanism untested.** If the cap doesn't fire correctly, two agents can lock into infinite counter-change loops. Mitigation: success criterion #5 is the gate; pilot must exercise the loop at least once and confirm it caps correctly.
2. **`/goal` evaluator misjudges baton state surface.** If Claude or Codex surfaces a baton-state line in a way the evaluator misreads, the loop may stop prematurely or fail to stop. Mitigation: skill bodies require a structured "BATON_STATE: {...}" line at every checkpoint, parseable by simple regex.
3. **superpowers parity drift between Claude Code and Codex.** Superpowers is one codebase but ships through two distribution channels (Claude Code plugins, Codex plugin marketplace). Version drift could mean a brainstorming skill that exists on one but not the other. Mitigation: the skill bodies invoke only well-established superpowers skills (brainstorming, writing-plans, test-driven-development); pilot writeup records each agent's superpowers version.
4. **Wait-helper integration depends on agents actually running the command.** The helper is deterministic, but the skill still has to invoke it after handoff. Mitigation: skill bodies make the wait command part of the universal stop rule; smoke test should verify both roles block and resume.
5. **The `plan_ref` plan becomes a contested file.** Both agents may write to it during the spec phase. Conflict resolution is currently "whoever writes last wins, baton acknowledges via `summary`." If both agents wanted to edit the same line in a single round, behavior is unspecified. Mitigation: in v1 the spec phase is strictly serial (Claude writes, then Codex Q&As, then Claude revises); concurrent edits should not happen. Document but don't enforce in v1.
6. **Mutual-review can re-introduce a regression the prativadi thought it fixed.** The vadi disapproves the prativadi's fix, writes a counter, the prativadi reviews the counter — but the prativadi may now be checking against its *own* prior view, not the original vadi implementation. Mitigation: the baton's `narrow_fixups` and `vadi_counter` arrays preserve diff context across the loop; the prativadi's Mode C re-reads from the baton not from session memory.
7. **Same-GitHub-identity attribution unsolved.** PR 353's pain point. v1 stays out of GitHub entirely. Postponed, not solved.

Open questions to revisit after the pilot:

- Does the per-phase active-work turn cap need to scale with phase scope? Larger phases may legitimately need more than the v2 default of 60 active model-work turns, while shell wait heartbeats remain outside the cap.
- Should the spec phase have its own turn cap separate from the per-phase one?
- How does `claude --resume` behave when the baton has advanced past the paused session's state? Likely fine since skill preflight re-reads the baton, but the first occurrence should be documented.

## 15. Versioning policy

- **Spec version:** this document is v1's source of truth. Changes to in-scope behavior require a spec rev with a `docs:` commit prefix and a section number reference.
- **Schema version:** baton field is `dvandva.baton.v1` for legacy runs and `dvandva.baton.v2` for named runs. Breaking changes increment the schema version; both skills must update in lockstep. Skills refuse to operate on a schema string outside the supported set (section 12).
- **Schema maintenance.** The schema lives in three places: Appendix A (canonical), `templates/channel/baton.json` (operational seed for `.dvandva/`), inlined in each SKILL.md (agent in-context contract). On changes: update Appendix A first, then template file, then both SKILL.md inlines. The v2 deterministic validator (section 16) will make Appendix A machine-checkable.
- **Policy fields in baton.** `allow_commit`, `allow_push`, and `allow_pr` intentionally live in the baton for v1 so every agent and transcript sees the run authority in the same file as state. `allow_commit` authorizes regular local checkpoint commits after verified logical slices; `allow_push` is still final-ship only. A separate `.dvandva/policy.json` is a v2 option if policy grows beyond these booleans.
- **Skill versions:** each SKILL.md may carry a `# Skill version: <semver>` comment in the body. Bumped on body changes that alter agent behavior.

## 16. Future work (v2 and beyond)

In priority order:

- **Deterministic validator script** + real JSON Schema at `templates/channel/baton.schema.json`. Skills invoke it as a pre-write gate. Rejects malformed batons and illegal transitions per the table in Appendix A. Closes the remaining schema-depth gap beyond the helper-level v1/v2 validation already in `dvandva-write.sh`.
- **Runner / launcher.** Optional file watcher that starts fresh agent processes via engine-specific commands. v1 walkaway stays session-based and uses persistent sessions plus the wait helper. A future runner must preserve human visibility and avoid expensive non-interactive loops.
- **Official marketplace submission.** Submit the GitHub-hosted plugin to official marketplace directories after public install smoke and pilot data.
- **Generic role abstraction.** Promote `vadi` / `prativadi` to first-class abstract roles with Claude/Codex as canonical instantiations. Largest portability risk currently.
- **GitHub PR summary integration.** Skill-side helper that turns the final baton state into a one-shot PR summary the human pastes in. Solves attribution if and only if it is the *only* PR comment.
- **Concurrent-agent detection.** Lock file or PID file with stale-detection so v2 can refuse to start a second agent against a baton already in use.
- **Explicit human-answer field.** Replace the v1 judgment call ("did the current prompt answer the question?") with a deterministic `human_answer` field or helper command that resumes `human_question` states only when populated.
- **Per-phase scope refinement.** v2 could auto-suggest phase boundaries based on file-graph or churn analysis during the spec phase.

## 17. Roadmap and deferred scope

In design-run order:

- **Run 3 — super-parallel dynamic agent generation.** The static 15-agent roster is now the seed roster: parent roles generate additional named agent instances on demand during a run. `agent_instances` is a first-class baton array (separate from `subagent_tracks`) recording each generated instance's identity, parent role, seed agent, model class (`opus-class|gpt-5.5` for planning/review, `sonnet-class|gpt-5.4` for implementation/docs), permission class (`readonly`, `verify-only`, `edit-scoped`, or `write-artifact-only`), read/write paths, base checkpoint, lifecycle, output refs, evidence refs, and close result. Three mandatory invariants: single-writer merge (generated agents never own baton `assignee`, phase transitions, or final approval; the parent role serializes evidence into one monotonic checkpoint), explicit closure (every generated handle must be explicitly closed with non-empty `work_item_ids` before its track counts as complete), and dynamic write-path disjointness (write-path overlaps for generated instances sharing the same `base_checkpoint`, or for any two live instances, are rejected unless sharing a `conflict_group` with explicit dependency serialization). No daemon, no hidden orchestrator. Generated instances are run-scoped and ephemeral; seed roster changes require a reviewed source commit.
- **Run 4 — generalized path-gate + retire standalones + work-gating.** Off-protocol commits are guarded by local git hooks, generalized `work_split` path-gate enforcement extends beyond the Run 3 dynamic disjointness check, and the user's standalone agent fleet is retired only for Dvandva-covered workflows with functional parity via Runs 1-4 usage, backup-manifest reversibility, Codex agent-axis no-op behavior, and no skill touches.

## Appendix A — `dvandva.baton.v1` canonical schema and transitions

This appendix is the spec-level authoritative reference for the schema (including prativadi-only fields) and the v1 state-transition table. The template file at `templates/channel/baton.json` is a v1-aligned reference artifact that mirrors the schema shape but holds only the always-present fields; `vadi` does not depend on it at runtime (see section 7.2 preflight), so the template is reference-only for humans inspecting the repo.

### Schema

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "feature-pr | campaign",
  "run_mode": "walkaway | supervised",
  "phase": "spec | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | parallel_implementing | test_creation | cross_review | cross_fixing | deep_review | deslop | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs include team for concurrent states",
  "active_roles": "v2 concurrent roles array, usually [] or [\"vadi\", \"prativadi\"]",
  "current_engine": "optional; \"claude\" | \"codex\" | null. Records which CLI wrote the most recent baton; for traceability only, not used for correctness.",
  "review_target": "spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "run_explainer_ref": "v2 path to gitignored final run explainer HTML under ./superpowers/run-reports/, required before terminal done",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, status, and artifact refs",
  "agent_instances": "v2 array recording generated run-scoped agent instances, provenance, model/permission class, read/write paths, closure evidence, output refs, and validation state",
  "subagent_tracks": "v2 array recording actual conditional parallelism tracks, owner, evidence refs, fallback rationale, and result",
  "verification_matrix": "v2 array/object mapping claims and risks to planned checks, owners, expected evidence, result, and evidence_ref",
  "master_plan_locked": "boolean; false during planning, true once prativadi advances to phase 1",
  "question": "string | null; one concrete user question when status is human_question",
  "resume_assignee": "vadi | prativadi | null; role to resume after a human_question answer",
  "resume_status": "spec_drafting | spec_review | spec_revision | null; status to restore after a human_question answer",
  "disagreement_round": "integer, set to 0 by the agent that writes the first baton of each new phase; incremented by the agent that disagrees during mutual review",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "turn_cap": "integer, default 60; passive shell wait heartbeats do not count",
  "branch": "git branch name",
  "checkpoint": "integer, bumped by the writer",
  "allow_commit": "boolean; default true in walkaway mode",
  "allow_push": "boolean; default true in walkaway mode",
  "allow_pr": "boolean; default false; v1 skills must never create a PR",
  "vadi_final_approval": "boolean; true only when vadi approves final diff",
  "prativadi_final_approval": "boolean; true only when prativadi approves final diff",
  "final_commit": "git commit hash | null; set after final commit",
  "pushed_ref": "git ref | null; set after final push",
  "summary": "one-paragraph human-readable summary of this checkpoint",
  "changed_paths": ["run-level union of intended files touched so far; excludes .dvandva/ and superpowers/"],
  "verification": [
    { "command": "exact shell command run", "result": "passed | failed | skipped", "notes": "optional one-liner" }
  ],
  "findings": ["reviewer or counter-reviewer: bullets describing issues found"],
  "narrow_fixups": ["reviewer: bullets describing narrow fixes applied directly"],
  "vadi_counter": ["vadi-as-reviewer: bullets describing counter-changes proposed during mutual review"],
  "deferred": ["reviewer: items deferred with one-line rationale and next-recommended-action"],
  "blockers": ["bullets describing what is blocking forward progress"],
  "next_action": "exact one-sentence instruction for the next actor"
}
```

### Allowed state transitions (v1 plus enforced v2 subset)

This spec is authoritative for v1 transitions and the enforced v2 research, test, review, and de-slop subset. The protocol doc and plugin-local transition table carry the same runtime contract.

**Research phase transitions (v2 named runs):**

| From | To | Trigger |
|---|---|---|
| (no named-run baton) | `phase: "research", status: "research_drafting"` | Vadi creates a new v2 named run and preserves `original_ask` |
| `research_drafting` | `research_review` | Vadi writes `research_ref`, `work_split`, `subagent_tracks`, and `verification_matrix` |
| `research_review` | `research_revision` | Prativadi finds source, coverage, risk, or work-distribution gaps |
| `research_revision` | `research_review` | Vadi updates the research artifact and baton fields |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Prativadi approves the research package |
| any research state while `master_plan_locked: false` | `human_question` | Either agent needs one human answer before master plan lock |
| any research state | `human_decision` | Either agent escalates |

**Spec phase transitions:**

| From | To | Trigger |
|---|---|---|
| (legacy v1 explicit selection only, no baton) | `phase: "spec", status: "spec_drafting"` | Vadi creates the legacy v1 seed |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi |
| `spec_review` | `phase: 1, status: parallel_implementing` | v2: prativadi accepts plan, freezes `total_phases`, and activates `assignee: "team"` with `active_roles: ["vadi", "prativadi"]` |
| `spec_review` | `phase: 1, status: implementing` | Legacy v1 explicit path only: prativadi accepts plan and freezes `total_phases` on the baton |
| `spec_revision` | `spec_review` | Vadi answers Q&A, hands back |
| any spec state while `master_plan_locked: false` | `human_question` | Either agent needs one human answer before master plan lock |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields |
| any spec state | `human_decision` | Either agent escalates |

**Implementation phase transitions (per phase N):**

| From | To | Trigger |
|---|---|---|
| `phase: N, parallel_implementing` | `phase: N, status: test_creation` | v2: both roles completed implementation chunks and recorded implementation-chunk subagent evidence |
| `test_creation` | `cross_review, review_target: implementation` | Vadi records tests/coverage evidence and hands to both roles for reciprocal peer-chunk review |
| `cross_review` | `cross_fixing` | Either role finds peer-owned chunk defects that must be fixed before deep review |
| `cross_fixing` | `test_creation` | Cross-review findings are fixed and tests/evidence must be refreshed before another cross-review |
| `cross_review` | `deep_review, review_target: implementation` | Both roles record completed approved cross-review tracks with evidence |
| `deep_review (impl)` | `deslop` | Prativadi substantively accepts implementation/test evidence and records completed `correctness-regression`, `test-evidence`, and `protocol-handoff` subagent tracks with evidence |
| `deep_review (impl)` | `phase_fixing` | Prativadi finds bugs, missing tests, verification gaps, or substantive protocol blockers |
| `phase_fixing` | `test_creation` | Vadi addressed findings and must refresh tests/evidence before another deep review |
| `deslop` | `phase_fixing` | Cleanup finds behavior, test, or review blockers |
| `deslop` | `phase: N+1, status: parallel_implementing, disagreement_round: 0` | v2: no unresolved nits, low/minor bugs, stale wording, or unclear instructions remain except explicitly accepted `deferred` items; next phase starts team-owned with `active_roles: ["vadi", "prativadi"]` |
| final `deslop` | `done` | Same as above, plus terminal `done` uses a coordinator assignee (`human`, `team`, `vadi`, or `prativadi`), the final explainer exists, `run_explainer_ref` matches `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`, and both final approvals are true |
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Legacy v1 direct review path only |
| `phase: N, implementing` | `human_decision` | Vadi blocked |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups, mutual review owed |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Legacy v1 direct-review path only: prativadi approves, no changes |
| `phase_review (impl)` | `human_decision` | Prativadi escalates |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings, re-hands |
| `phase_fixing` | `human_decision` | Vadi blocked during fix |
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Legacy v1 direct-review path only: vadi approves prativadi fixups |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter (disagreement_round +=1) |
| `review_of_review (prativadi_fixups)` | `human_decision` | disagreement_round ≥ cap |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Legacy v1 direct-review path only: prativadi approves counter |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves counter, applies a different fix (disagreement_round +=1) |
| `counter_review (vadi_counter)` | `human_decision` | disagreement_round ≥ cap |
| any state | `human_decision` | escalation (`disagreement_round >= cap`, `turn_cap` hit, blocker, malformed input) |
| `human_decision` | any state | After human edits baton or prompts an agent |

Any other transition is illegal in v1 or v2 and must be rejected by the writing agent.

## Appendix B — pilot acceptance scaffold

Filled in for `docs/case-studies/pilot-01.md` after the pilot completes. Comparison to the PR 353 baseline from `docs/case-studies/pr-353.md`.

| Metric | PR 353 baseline | Pilot result | Delta |
|---|---|---|---|
| Total commits in the PR | 116 | | |
| Agent-to-agent PR comments | 186 | | |
| Wall-clock from first vadi turn to `status: done` | (not recorded) | | n/a |
| Doer turns (across all phases) | (not recorded) | | n/a |
| Reviewer turns (across all phases) | (not recorded) | | n/a |
| Spec phase rounds (Claude draft + Codex Q&A + revision) | n/a | | n/a |
| Implementation phases declared | n/a | | n/a |
| Mutual-review loops triggered | n/a | | n/a |
| Disagreement-cap fires | n/a | | n/a |
| Real issues caught by reviewer | qualitative (see PR 353 "What Worked") | | |
| Auto-activation rate (vadi) | n/a | X / Y attempts | n/a |
| Auto-activation rate (prativadi) | n/a | X / Y attempts | n/a |
| Description edits required mid-pilot | n/a | | n/a |
| Runaway loops observed | n/a | should be 0 | |

A pilot is "ship-passing" when:
- Agent-to-agent comments column is dramatically lower than 186 (target: < 20).
- At least one real issue was caught by the reviewer.
- The disagreement cap fired correctly when exercised.
- No runaway loops occurred.

Anything else is data for the v2 rev.

## Sources

- agentskills.io open standard — https://agentskills.io
- Claude Code skills — https://code.claude.com/docs/en/skills
- Claude Code plugins — https://code.claude.com/docs/en/plugins
- Claude Code `/goal` — https://code.claude.com/docs/en/goal
- Codex skills — https://developers.openai.com/codex/skills
- Codex plugins — https://developers.openai.com/codex/plugins/build
- Codex AGENTS.md guide — https://developers.openai.com/codex/guides/agents-md
- superpowers framework — https://github.com/obra/superpowers
- superpowers Codex install guide — https://deepwiki.com/obra/superpowers/2.4-installing-on-codex
- PR 353 baseline (this repo) — `docs/case-studies/pr-353.md`
- Two-mode agent workflow (this repo) — `docs/workflows/two-mode-agent-workflow.md`
- Local baton channel protocol (this repo) — `docs/protocol/local-baton-channel.md`
