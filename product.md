# Dvandva — Product Specification v1

Status: rewritten 2026-05-14 for richer flow (spec phase + phased implementation + mutual review + disagreement loop + `/goal` autonomy). Owner: axatbhardwaj. Supersedes the prompt-template-first approach in `templates/prompts/` and the single-shot doer→reviewer flow in the previous draft.

> **Spec rev 2026-06-11:** §3.1 adds `dvandva-write.sh` (validated atomic baton install + auto-snapshot, bundled byte-identical in both skill script dirs) and `scripts/test-dvandva-write.sh`. The wait helper's default `--max-wait` drops 900→540 so one foreground invocation fits Claude Code's 600 s Bash-tool cap (§7.2, §8.2, §12); it wakes early on baton-directory inotify events and retries once on torn reads. This pulls §16's deterministic validator forward to script level (a PreToolUse hook remains future work).
>
> **Spec rev 2026-06-27:** v2 design adds named run directories, a first-class research phase, generated user-facing HTML artifacts, and persistent shell waiting. Legacy v1 still uses `.dvandva/baton.json`; v2 runs use `.dvandva/runs/<run_id>/baton.json` with `schema: "dvandva.baton.v2"`, `run_id`, `original_ask`, `research_ref`, `work_split`, and `verification_matrix`.

## 1. What it is

Dvandva v1 is a pair of agent skills, written to the [agentskills.io](https://agentskills.io) open standard, that encode a disciplined two-agent collaboration protocol:

- `vadi` — the proposer/implementer skill. Runs in either Claude Code or Codex. Drives research, spec/plan creation, and phase implementation, then reviews any narrow fixups the prativadi makes.
- `prativadi` — the responder/reviewer skill. Runs in either Claude Code or Codex. Reviews research, Q&As during the spec phase, reviews each implementation phase, applies narrow fixups within an allowlist, and reviews the vadi's counter-changes when there is a disagreement.

Both skills share a baton file as the coordination channel. Legacy v1 uses `.dvandva/baton.json`. v2 adds named runs under `.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in one git worktree or directory as long as the human gives both sessions the same safe `run_id` or explicit `DVANDVA_BATON_FILE`. A safe `run_id` is one path segment: letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`; once a v2 baton exists, its `run_id` is immutable for that run. The default run mode is `walkaway`: the human gives an initial goal, starts or joins the two agent sessions once, and the skills use a cheap foreground wait helper when the baton assigns work to the other role. `supervised` mode is the serial fallback for one-engine runs: assigned-away agents exit and the human invokes the next role manually.

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each role turn: `superpowers:using-superpowers` before action, `superpowers:brainstorming` before design, `superpowers:test-driven-development` before implementation, `superpowers:verification-before-completion` before success claims, `superpowers:writing-skills` when skills change, and `superpowers:dispatching-parallel-agents` / `superpowers:subagent-driven-development` when parallel tracks exist. If the active engine cannot invoke the relevant Superpowers skills, the Dvandva role must stop, surface setup instructions, and avoid writing a success or advancement baton.

The v2 flow has seven lifecycle segments:

1. **Research phase** — vadi writes `research_ref`, a generated user-facing HTML artifact with machine-readable metadata, after parallel subagents or direct parallel-track exploration cover codebase, docs, tests, risks, and work distribution. The baton records `work_split` and `verification_matrix`. Prativadi independently reviews it and either approves or sends findings back.
2. **Master-planning phase** — collaborative plan creation. Vadi drafts; prativadi Q&As; vadi revises. Either role may ask the user questions while the plan is still unlocked. Loop until plan converges. The generated `plan_ref` HTML declares N implementation phases.
3. **Implementation phase** — vadi implements phase N without silently mixing implementation, testing, and review responsibilities.
4. **Test-creation phase** — vadi creates or updates tests for every new behavior and records a 100% test coverage target in `verification_matrix`; source-only docs/skills get lint/review coverage with rationale.
5. **Deep-review phase** — prativadi performs independent deep review after implementation and test creation. Review is separate from test creation and must inspect code, tests, docs, baton fields, and claims.
6. **De-slop phase** — vadi/prativadi loop on nits, low/minor bugs, stale wording, vague instructions, duplicated logic, and generated-looking clutter until no findings remain except items explicitly accepted in `deferred`.
7. **Phase advancement or completion** — on agreement, make a regular local checkpoint commit for the verified logical slice when `allow_commit` permits it, then advance to phase N+1. On completion of the final phase, require both final approvals, optionally push, then transition to `done`.

Legacy enforcement starts with the agent checklist embedded in each SKILL.md and `/goal` evaluator transcript checks. The bundled write helper now enforces the supported v1/v2 schema strings, required fields, checkpoint arithmetic, safe run IDs, v2 status-owner pairs, and transition subset. A future standalone CLI validator backed by a full JSON Schema file can replace the remaining checklist-only validation.

The product is the `dvandva` plugin, its bundled protocol/orchestration skills, plugin-local baton references, bundled wait helpers, an install/usage doc, and a pilot case study. It coordinates work through baton state and skill checklists; it does not add an agent launcher, daemon, or GitHub integration.

Subagent execution uses the canonical Dvandva subagent roster under `plugins/dvandva/agents/`. This roster replaces the earlier personal `claude-skills/agents` roles for Dvandva work. The design takes GSD-style fresh-context subagents for bounded heavy work and OMO-style team roles for specialization, but preserves Dvandva's core constraint: the baton remains the only coordinator.

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
- `plugins/dvandva/skills/testing/SKILL.md` — Dvandva-native test-creation and test-gap workflow absorbed from the previous standalone testing skill.
- `plugins/dvandva/skills/understanding/SKILL.md` — Dvandva-native deep-understanding teaching workflow that produces HTML mastery checklists.
- `plugins/dvandva/skills/worktree-setup/SKILL.md` — Dvandva-native worktree preparation workflow with generic and DeFi profile conventions.
- `plugins/dvandva/skills/*/scripts/dvandva-wait.sh` — foreground shell wait helper bundled as a real executable in each runtime skill directory. It polls `.dvandva/baton.json` cheaply, without spending model turns, until the baton returns to the role or reaches a terminal human/done state.
- `plugins/dvandva/skills/*/scripts/dvandva-write.sh` — validated atomic baton installer; rejects illegal v1 transitions, installs via tmp+mv, auto-snapshots. Tested by `scripts/test-dvandva-write.sh`.
- `scripts/test-dvandva-wait.sh` — focused shell tests for the helper's exit-code contract.
- `scripts/lint-artifacts.sh` — generated-artifact policy lint. It rejects generated Markdown under `./superpowers/` and requires generated HTML artifacts to be dark, self-contained, offline-renderable, and backed by a machine-readable Dvandva metadata block.
- `plugins/dvandva/references/baton-schema.json` — bundled legacy v1 schema seed with all required keys (`run_mode`, `phase`, `total_phases`, `plan_ref`, `master_plan_locked`, `question`, `resume_assignee`, `resume_status`, `disagreement_round`, `disagreement_cap`, `turn_cap`, `review_target`, `current_engine`, final-approval fields, etc.).
- `README.md` install section covering: marketplace install (primary), development symlink/copy install (fallback), and the Superpowers hard-dependency check for every engine running a Dvandva role.
- One pilot writeup at `docs/case-studies/pilot-01.md` after the workflow ship.

### 3.1a In v2 design

- `dvandva.baton.v2` — run-scoped baton schema for `.dvandva/runs/<run_id>/baton.json`. Required v2 fields include safe `run_id`, `original_ask`, `research_ref`, `work_split`, and `verification_matrix`. v1 remains valid only for the legacy `.dvandva/baton.json` fallback. The live v2 write-helper enforcement covers v2-only fields, schema continuity for existing runs, v2 status-owner pairs, and v2 lifecycle transitions intentionally instead of by convention.
- Research lifecycle states before spec lock: `phase: "research", status: "research_drafting"` for vadi research synthesis, `research_review` for prativadi independent review, and `research_revision` for vadi response to research findings. v2 scaffolds new named runs at `research_drafting`; legacy v1 scaffolds at `spec_drafting`.
- Test and review lifecycle states are separate in v2: `test_creation` records the doer's tests and coverage evidence, `deep_review` records independent prativadi review after tests exist, and `deslop` records cleanup loops for nits, low/minor bugs, stale wording, and unclear instructions. A phase does not advance while unresolved `deep_review` or `deslop` findings remain unless explicitly accepted in `deferred`.
- The generated user-facing artifacts are HTML: plans, research reports, evaluations, reviews, pilot write-ups, and run reports. Platform/source Markdown such as `SKILL.md`, command files, README/source docs, and prompt templates stays in its native format.
- `dvandva-wait.sh --persist` is the v2 shell-level wait mode. It loops inside one shell process until the selected baton assigns the role, reaches `done`/`human_question`/`human_decision`, the user interrupts, or the optional `--persist-max <seconds>` wall-clock cap is reached. The wait-helper persist cap exit 23 means that persistent cap was reached. The write-helper validation exit 23 means a candidate failed schema/required-key/status validation. Finite exit 20 remains a compatibility heartbeat, not a terminal agent decision.
- Claude Code cannot rely on one unbounded `--persist` call because its Bash tool is capped around 600 seconds. Claude-hosted sessions should use finite `--max-wait 540` re-loops or `--persist --persist-max <600`; Codex-hosted sessions may use unbounded `--persist` when the shell budget supports it.

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
      implementer.md
      test-creator.md
      deep-reviewer.md
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

The v2 flow has seven segments and an end state: research, master planning, implementation, test_creation, deep_review, deslop, and phase advancement/completion. Every arrow in the diagram is a baton write by the active agent. In default walkaway mode, the other persistent session is already blocked in `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh`; the helper returns when the baton assigns that role, and the agent re-enters preflight.

```
                  ┌──────────────────────────────────┐
                  │ RESEARCH PHASE                   │
                  │  phase: "research"               │
                  │                                  │
   start ───▶ Vadi (research_drafting)                │
                  │   Invoke dvandva:research         │
                  │   uses parallel subagents         │
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
                  │    or → phase 1 implementing      │
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
                     │ baton phase: 1, status: implementing
                     ▼
   ┌─── PER-PHASE LOOP (for phase N in 1..total_phases) ───┐
   │                                                       │
   │   Vadi (implementing phase N)                         │
   │     uses superpowers:test-driven-development          │
   │     baton → test_creation                             │
   │       ▼                                               │
   │   Vadi (test_creation)                                │
   │     creates/updates tests and coverage evidence       │
   │     targets 100% test coverage for new behavior       │
   │     baton → deep_review                               │
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
   └─── on phase N+1 ──▶ Vadi (implementing phase N+1)  │ │
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
2. **Preflight (all modes)** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If the baton is absent, the vadi scaffolds the resolved directory and writes the seed baton through `dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` with `original_ask` preserved in initial context. If the baton is assigned away and `run_mode: "walkaway"`, wait on `"$BATON_FILE"`; Claude uses finite 540-second re-loops, while Codex may use `--persist` when the shell budget supports it. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role.
3. **Mode R1: research drafting** — when `phase: "research", status: "research_drafting"`. Invoke `dvandva:research`, preserve `original_ask`, use parallel subagents when available, write the generated HTML `research_ref`, populate `work_split` and `verification_matrix`, and hand to prativadi with `status: research_review, review_target: research`.
4. **Mode R2: research revision** — when `phase: "research", status: "research_revision"`. Invoke `dvandva:research`, address prativadi research findings, update `research_ref`, `work_split`, and `verification_matrix`, clear resolved findings, and hand back to `research_review`.
5. **Mode A: spec drafting** — when `phase: "spec", status: "spec_drafting"`. Read `research_ref`, `work_split`, and `verification_matrix` first. Invoke `superpowers:brainstorming` skill flow without rediscovering already-settled research. The vadi may ask the user questions if required before the master plan is useful. Produce a gitignored dark self-contained HTML plan under `./superpowers/plans/YYYY-MM-DD-<topic>.html` with declared `total_phases` and a per-phase scope list. Set `plan_ref`, `total_phases`, and `master_plan_locked: false` on the baton. Write baton with `status: spec_review, assignee: prativadi, review_target: spec`.
6. **Mode B: spec revision** — when `phase: "spec", status: "spec_revision"`. Read the baton's `findings` array (prativadi's Q&A), respond in the `plan_ref` plan, update affected `total_phases` if scope changed. Always write baton with `status: spec_review, assignee: prativadi, review_target: spec`; the prativadi is the only actor that can advance the spec to phase 1. Follow the stop/wait rule.
7. **Mode C: phase implementation** — when `phase: 1..N, status: "implementing"`. Read the corresponding phase scope from the `plan_ref` plan and the relevant `work_split` / `verification_matrix` entries. Invoke `superpowers:test-driven-development` when applicable. Implement only the phase scope; do not bleed into adjacent phases. Write baton with `status: test_creation, assignee: vadi, review_target: null`.
8. **Mode T: test creation** — when `phase: 1..N, status: "test_creation"`. Create or update tests for every new behavior, record 100% test coverage evidence or source-only rationale in `verification_matrix`, run motivating tests and cheap checks, then write baton with `status: deep_review, assignee: prativadi, review_target: implementation`. Test creation is separate from review.
9. **Mode D: phase fixing** — when `phase: 1..N, status: "phase_fixing"`. Read `findings` from prativadi. Fix only listed items, update tests if behavior changed, and return through `test_creation` rather than directly to review. Follow the stop/wait rule.
10. **Mode S: deslop** — when `phase: 1..N, status: "deslop"`. Remove nits, low/minor bugs, stale wording, duplicated instructions, and generated-looking clutter found by deep review. If no unresolved issues remain except explicitly accepted `deferred` items, advance to the next phase or final completion.
11. **Mode E: prativadi-fixup review** — when `status: "review_of_review", review_target: "prativadi_fixups", assignee: vadi`. Read the prativadi's `narrow_fixups` array and inspect the diff the prativadi applied. Decide: approve or disapprove.
   - On approve: write baton with `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` (advance), or `status: done` after final approval/ship if N was the final phase. Follow the stop/wait rule.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write counter-changes inline, write baton `status: counter_review, review_target: vadi_counter, assignee: prativadi`. Follow the stop/wait rule.
12. **Regular checkpoint commits** — after any active mode changes files and the relevant verification commands pass, make a local checkpoint commit when `allow_commit == true`. Commit only the intended `changed_paths` union, excluding `.dvandva/` and `superpowers/`, and only when `git status --short` has no unrelated dirty paths. Use one logical change per commit, semantic prefix, and a subject of 50 characters or fewer. Record the commit hash in `verification` or `summary` as `checkpoint_commit=<hash>`. Do not push checkpoint commits.
13. **Final ship rule** — before terminal `done`, push only when `allow_pr: false`, `allow_push: true`, `vadi_final_approval: true`, and `prativadi_final_approval: true`. If intended dirty files remain and `allow_commit == true`, make one final local commit first; if no dirty intended files remain because checkpoint commits already captured the work, record `final_commit` as `git rev-parse HEAD`. Unrelated dirty paths force `human_decision`. Record `final_commit` and `pushed_ref`. Never create a PR.
14. **Stop rule (universal)** — in walkaway mode, do not stop on role handoff. Surface BATON_STATE, run the wait helper, and continue from preflight when the baton returns. Stop only for `done`, `human_question`, `human_decision`, user interrupt, or turn-cap escalation. `human_question` and `human_decision` remain legal from early v2 research even before `research_ref` exists, so missing setup can be surfaced before the first research artifact is written.
15. **`/goal` condition** — embedded in the skill body verbatim, centered on continuing until `done`, `human_question`, or `human_decision`; if assigned away, block in the wait helper instead of spending model turns.
16. **Failure modes** — section 12.

## 8. prativadi skill design

### 8.1 Frontmatter

- `name: prativadi`
- `description:` front-loaded triggers: *review*, *spec Q&A*, *prativadi pass*, *narrow fixups*, *adversarial verification*, *check the baton*, *review the vadi's counter-change*, *review claude's counter-change*. Covers all three prativadi modes.

### 8.2 Body sections

1. **Role one-liner** — "You are the Dvandva prativadi. You Q&A on plans, review implementation phases, apply narrow fixups, and review the vadi's counter-changes."
2. **Preflight** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If the baton is missing, prativadi waits on `"$BATON_FILE"` with `--allow-missing` unless `DVANDVA_NO_WAIT=1` is set. If the baton is assigned away and `run_mode: "walkaway"`, wait on `"$BATON_FILE"`; Claude uses finite 540-second re-loops, while Codex may use `--persist` when the shell budget supports it. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role. **Additionally verify `superpowers:brainstorming` is available in the current session** before spec Q&A; if absent, surface install instructions and exit (per section 4 prerequisites). Do not depend on one fixed filesystem path.
3. **Mode R: research review** — when `phase: "research", status: "research_review", review_target: "research"`. Invoke `dvandva:research` for independent research review. Do not rely solely on the vadi's `research_ref`; inspect relevant sources and use parallel subagents when available. If gaps remain, populate `findings` and write `status: research_revision, assignee: vadi`. If research is sufficient, advance to `phase: "spec", status: "spec_drafting", assignee: "vadi"`, preserving `research_ref`, `work_split`, and `verification_matrix`.
4. **Mode A: spec Q&A** — when `phase: "spec", status: "spec_review", review_target: "spec"`. Invoke `superpowers:brainstorming` skill flow as the questioner. Read the `plan_ref` plan, surface Q&A in the baton's `findings` array, optionally edit the plan directly for narrow improvements (typos, sharper phrasing). The prativadi may ask the user questions if required before the master plan can be approved or handed back. Decide: hand back to vadi (questions remain) or advance. Write baton `status: spec_revision, assignee: vadi` (for more Q&A) or `phase: 1, status: implementing, assignee: vadi, disagreement_round: 0, master_plan_locked: true` (advance to phase 1).
5. **Mode B: deep review** — when `phase: 1..N, status: "deep_review", review_target: "implementation"`. Read diff vs branch baseline only after `test_creation` is complete. Cross-check the vadi's `verification` block and the planned coverage in `verification_matrix` (did the commands actually pass? do they cover the changed paths and risks, and is 100% test coverage for new behavior documented?). Look for bugs, regressions, stale docs, missing tests, claims not matching diff, and deslop opportunities.
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
   - If approve, no changes: write baton with `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` or `status: done` after final approval/ship if final phase. Follow the stop/wait rule.
8. **Mode C: vadi-counter review** — when `status: "counter_review", review_target: "vadi_counter", assignee: prativadi`. Read the vadi's counter-change diff. Decide:
   - On approve: write baton `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` (advance), or `status: done` after final approval/ship if final phase. Follow the stop/wait rule.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write a new narrow fixup and route back to `review_of_review, review_target: prativadi_fixups, assignee: vadi`. Follow the stop/wait rule.
9. **Final ship rule** — same as vadi. The prativadi may commit/push only after both final approvals are true, the current dirty paths match `changed_paths`, and PR creation remains false.
10. **Stop rule** — in walkaway mode, do not stop on role handoff. Surface BATON_STATE, run the wait helper, and continue from preflight when the baton returns. In supervised mode, exit on role handoff.
11. **`/goal` condition** — centered on continuing until `done`, `human_question`, or `human_decision`; if assigned away, block in the wait helper instead of spending model turns.
12. **Failure modes** — section 12.

## 9. Cross-engine portability

Both skills target the agentskills.io open standard. Only the universal frontmatter (`name`, `description`) carries correctness weight. Optional engine-specific fields are avoided in v1:

- **No `allowed-tools` reliance.** The agentskills.io spec treats it as implementation-varying. Skill bodies assume the user's existing permission setup allows git, bash, and the project's test runner. One-time tool prompts are acceptable; the skill does not depend on pre-approval.
- **No `paths` glob.** Skills are workflow-scoped, not file-scoped.
- **No `context: fork`.** Skills run in the main session so `/goal` transcript surfacing works (the goal evaluator only sees what's surfaced).
- **No engine-specific frontmatter extensions.** If forced in a future rev, the SKILL.md forks into engine-specific variants; document the reason explicitly.

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
claude plugin marketplace add axatbhardwaj/Dvandva
claude plugin install dvandva@dvandva

codex plugin marketplace add axatbhardwaj/Dvandva
codex plugin add dvandva@dvandva
```

`scripts/install-codex.sh` wraps the Codex path for users: it registers the marketplace, runs `codex plugin add dvandva@dvandva` on current Codex builds, and keeps the legacy app-server RPC install as a fallback for older builds. The authoritative preflight is whether the current agent session can see and invoke the required Superpowers skills.

### 11.2 Development install fallback

For local development against a checkout, prefer marketplace install from the checkout:

```bash
bash scripts/install-codex.sh "$(pwd)"
claude plugin marketplace add "$(pwd)"
claude plugin install dvandva@dvandva
```

For live skill-development work where plugin cache copies are too indirect, symlink or copy `plugins/dvandva/skills/vadi/` and `plugins/dvandva/skills/prativadi/` into the engine skill directories. Remove old pre-plugin `dvandva-*` symlinks first because root `skills/` no longer exists.

### 11.3 Project-level adoption

Consumer repos may check the plugin into their own tree or use project-scoped marketplace declarations. Project-level skills can carry tool-permission frontmatter; review `SKILL.md` the same way you would any other `.claude/` or `.agents/` config.

## 12. Failure modes the skills must handle

| Failure | Required behavior |
|---|---|
| `.dvandva/baton.json` missing | Vadi in spec mode: scaffold from template, set `phase: "spec", status: "spec_drafting"`. Prativadi: surface "no baton — vadi has not started" and exit. |
| Baton present but malformed JSON | Both: do not overwrite. Surface parse error verbatim. Write `.dvandva/baton.broken.json` with the unparseable bytes preserved. Surface in-memory next state as `human_decision`. |
| `schema` field is not `dvandva.baton.v1` or `dvandva.baton.v2` | Both: refuse to operate. Surface schema mismatch. Exit. |
| `assignee` does not match this agent's role | In `run_mode: "walkaway"`, run the wait helper for this role. Outside walkaway, surface "wrong actor for this state" and exit. Never silently overwrite the assignee. |
| Superpowers absent on an engine running a Dvandva role | Surface install instructions referencing section 4 and the relevant plugin marketplace/install path. Do not continue with active work; if the baton exists and the role owns it, route to `human_decision` rather than writing a success or advancement baton. |
| `status: human_question` | Stop and surface the one concrete `question` plus `resume_assignee` and `resume_status`. If the user answers in the current prompt, restore `assignee` and `status` from those fields, clear the question fields, and continue. This is valid only before `master_plan_locked: true`; after that, use `human_decision`. |
| `plan_ref` missing, or referenced plan file missing during a phase mode | Doer: surface "spec phase did not complete; cannot start phase implementation." Exit. Reviewer: same. |
| `total_phases` is 0 or unset during phase mode | Both: surface schema integrity error, exit. The spec phase is responsible for setting this. |
| `disagreement_round >= disagreement_cap` | Whichever agent next writes the baton: set `status: human_decision, assignee: human`. Do not write further counter-changes. |
| `/goal` active-work turn cap hit before exit condition | Agent: surface current baton state and a "still owe work" summary, set `status: human_decision`, exit. The default active-work cap is 60; shell wait heartbeats do not count. |
| Wait helper exits 20 (`timeout`) | Surface the still-waiting state and run it again unless the user interrupts. The timeout is only a visibility heartbeat. |
| Wait helper exits 23 (`persist_max`) | This is the wait-helper persist cap exit 23. Surface the still-waiting state and either re-enter the wait helper with a fresh cap or escalate if the peer appears dead or on a different run path. The cap protects shell budgets; it is not a baton terminal state. |
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
- **Plugin smoke test** (`scripts/smoke-plugin-install.sh`): copies the plugin into a temp marketplace, validates Claude plugin/marketplace metadata, runs Codex marketplace add with isolated `CODEX_HOME`, verifies both wait helpers, and checks standalone development copies.
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
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs are enforced",
  "current_engine": "optional; \"claude\" | \"codex\" | null. Records which CLI wrote the most recent baton; for traceability only, not used for correctness.",
  "review_target": "spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, status, and artifact refs",
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

**Spec phase transitions:**

| From | To | Trigger |
|---|---|---|
| (no baton) | `phase: "spec", status: "spec_drafting"` | Vadi first run |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi |
| `spec_review` | `phase: 1, status: implementing` | Prativadi accepts plan and freezes `total_phases` on the baton |
| `spec_revision` | `spec_review` | Vadi answers Q&A, hands back |
| any spec state while `master_plan_locked: false` | `human_question` | Either agent needs one human answer before master plan lock |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields |
| any spec state | `human_decision` | Either agent escalates |

**Implementation phase transitions (per phase N):**

| From | To | Trigger |
|---|---|---|
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Vadi completes phase, hands to prativadi |
| `phase: N, implementing` | `human_decision` | Vadi blocked |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups, mutual review owed |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Prativadi approves, no changes |
| `phase_review (impl)` | `human_decision` | Prativadi escalates |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings, re-hands |
| `phase_fixing` | `human_decision` | Vadi blocked during fix |
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Vadi approves prativadi fixups |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter (disagreement_round +=1) |
| `review_of_review (prativadi_fixups)` | `human_decision` | disagreement_round ≥ cap |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done` after dual final approval and optional commit/push) | Prativadi approves counter |
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
