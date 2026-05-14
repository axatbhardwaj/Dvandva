# Dvandva — Product Specification v1

Status: rewritten 2026-05-14 for richer flow (spec phase + phased implementation + mutual review + disagreement loop + `/goal` autonomy). Owner: axatbhardwaj. Supersedes the prompt-template-first approach in `templates/prompts/` and the single-shot doer→reviewer flow in the previous draft.

## 1. What it is

Dvandva v1 is a pair of agent skills, written to the [agentskills.io](https://agentskills.io) open standard, that encode a disciplined two-agent collaboration protocol:

- `dvandva-doer` — auto-activates in Claude Code. Drives the spec/plan phase using `superpowers:brainstorming` and `superpowers:writing-plans`, then implements the plan phase-by-phase, then reviews any narrow fixups Codex makes.
- `dvandva-reviewer` — auto-activates in Codex. Q&As during the spec phase, reviews each implementation phase, applies narrow fixups within an allowlist, and reviews Claude's counter-changes when there is a disagreement.

Both skills share `.dvandva/baton.json` as the coordination channel. Both agents run autonomously via `/goal` within each invocation, exiting only when the baton-condition transfers ownership to the other agent or to a human.

The flow has three lifecycle segments:

1. **Spec phase** — collaborative plan creation. Claude drafts; Codex Q&As; Claude revises. Loop until plan converges. The plan declares N implementation phases.
2. **Per-phase implementation loop** — Claude implements phase N; Codex reviews; if Codex applies code changes, Claude reviews those changes (mutual review); on disagreement, Claude makes counter-changes and Codex reviews; up to 3 disagreement rounds before forced human escalation. A phase advances only after the agent responsible for the current review approves.
3. **Phase advancement or completion** — on agreement, advance to phase N+1; on completion of the final phase, transition to `done`.

Enforcement in v1 is by agent checklist embedded in each SKILL.md and by `/goal` evaluator transcript-checks. Deterministic schema and transition validation is deferred to v2 (a CLI validator backed by a real JSON Schema file).

The product is the two skills, the baton template, an install/usage doc, and a pilot case study. No CLI binary, no daemon, no GitHub integration.

**PR 353 provenance.** PR 353 proved the need for a durable handoff surface, explicit ack/ownership flips, reviewer findings that can become fixes, and a cheaper alternative to agent-to-agent PR comment traffic. The v1 mutual-review loop, disagreement cap, turn cap, `human_decision` terminal, and baton transition table are product design responses to that evidence; they were not themselves fully exercised as named states in PR 353. The pilot exists to validate those new protocol pieces.

## 2. Audience and success criteria

**Primary audience:** the spec owner and any teammate using Claude Code + Codex 0.130+, both with the superpowers plugin installed.

**v1 ships successfully when all five hold:**

1. The repo contains `skills/dvandva-doer/SKILL.md` and `skills/dvandva-reviewer/SKILL.md` written to the agentskills.io standard, plus a baton template and an install/usage README that covers superpowers prerequisites.
2. A teammate can follow the README — including the superpowers install step for Codex — and run a Claude+Codex pilot on a low-risk real PR without DM-ing the owner.
3. One pilot is completed: spec phase converges, ≥2 implementation phases run, ≥1 mutual-review loop triggers, and one disagreement-loop event occurs and resolves (or terminates correctly at human escalation). Metrics — turn count per agent, agent-to-agent PR comment count, wall-clock, real issues caught — are written up as `docs/case-studies/pilot-01.md` against the PR 353 baseline.
4. In the pilot, both skills auto-activate from natural workflow language at least once each. Explicit invocation (`/dvandva-doer`, `$dvandva-reviewer`) stays as documented fallback.
5. No runaway loops. The disagreement-round cap (default 3) triggers a forced `human_decision` correctly when exercised, and `/goal` exits cleanly at every baton-state transition.

If criterion #5 fails (any runaway loop observed during pilot), v1 does not ship — the cap mechanism is the operational safety floor and has to work.

## 3. Scope

### 3.1 In v1

- `skills/dvandva-doer/SKILL.md` — frontmatter (portable `name` + `description`), body covering the doer's five modes (spec drafting, spec revision, phase implementation, phase fixing, codex-fixup review), the baton schema, the `/goal` exit conditions, and the disagreement-cap behavior.
- `skills/dvandva-reviewer/SKILL.md` — same shape, covering reviewer's three modes (spec Q&A, phase review, claude-counter review), narrow-fix allowlist, handback conditions, baton schema, `/goal` exit conditions.
- `templates/channel/baton.json` — must be updated from the current v0 seed into the v1 extended schema seed with the new fields (`phase`, `total_phases`, `plan_ref`, `disagreement_round`, `review_target`) before the pilot.
- `README.md` install section covering: user-level symlink install (primary), project-level install (secondary), and the superpowers prerequisite check for both engines.
- One pilot writeup at `docs/case-studies/pilot-01.md` after the workflow ship.

### 3.2 Out of v1 (non-goals)

- No CLI, no `dvandva` binary, no schema-validator script. v1 enforcement is checklist-gate inside the skill body + `/goal` transcript surfacing. The deterministic validator and the real JSON Schema file are v2.
- No runner / daemon. Human still starts each agent invocation. Inside the invocation, `/goal` runs many turns autonomously; between invocations, the human types one command. v2 introduces a file watcher.
- No GitHub integration. No PR comment posting. Skills tell the agent what to surface in transcript; humans write any PR comments using the baton as source material.
- No generic doer/reviewer role abstraction. v1 hardcodes `claude` and `codex` in the skill bodies. The baton `assignee` field is an unconstrained string so a third role is not blocked at the schema level.
- No separate `dvandva-init` skill. The doer skill scaffolds `.dvandva/` inline on first run.
- No plugin packaging for distribution. Manual symlink install is the v1 distribution story. Plugin packaging is v2.
- No multi-baton-per-repo support. One active baton per worktree. Parallel branches each get their own worktree and own baton.

## 4. Prerequisites (hard requirement before pilot)

Both prerequisites must be verified by the user before the pilot can run. The skill bodies must check the most-likely-to-fail prerequisite (superpowers on Codex) at preflight.

| Prerequisite | Why | How to verify |
|---|---|---|
| Claude Code installed | Hosts the doer skill | `which claude && claude --version` |
| Codex CLI ≥ 0.130 | Hosts the reviewer skill; supports skills + `/goal` | `codex --version` |
| superpowers plugin installed on Claude Code | doer uses `superpowers:brainstorming` and `superpowers:writing-plans` in spec phase | `ls ~/.claude/plugins/cache/*/superpowers/` (varies by install method) or `claude` then `/skills` |
| superpowers plugin installed on Codex | reviewer uses `superpowers:brainstorming` for spec Q&A | Capability check, not a fixed path: Codex must show `superpowers:brainstorming` in its available skills, or the reviewer must be able to invoke it successfully. Local installs may live under `~/.codex/plugins/cache/...` or `~/.agents/skills/...`. |
| Git repo with a feature branch | The dvandva flow assumes a branch | `git rev-parse --abbrev-ref HEAD` returns something other than the main branch |

The reviewer skill's preflight refuses to run and surfaces a clear install hint if `superpowers:brainstorming` is not available to the current Codex session. It must not hardcode a single filesystem path.

## 5. Repo layout

```
skills/
  dvandva-doer/
    SKILL.md
  dvandva-reviewer/
    SKILL.md
templates/
  channel/
    baton.json          # to be updated to extended-schema initial seed
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

The existing `templates/prompts/claude-doer-goal.md` and `templates/prompts/codex-reviewer-goal.md` are demoted from active templates to historical artifacts (a README note explains they were the v0 form of what the skills now are; files stay in-tree as reference).

## 6. Flow overview

The flow has three segments and an end state. Every arrow in the diagram is a baton write by the *exiting* agent that triggers the *next* agent. The human dispatches the next agent between invocations; inside each invocation `/goal` drives many turns until the exit condition.

```
                  ┌──────────────────────────────────┐
                  │ SPEC PHASE                       │
                  │  phase: "spec"                   │
                  │                                  │
   start ───▶ Claude (drafting)                       │
                  │   uses superpowers:brainstorming  │
                  │   + superpowers:writing-plans     │
                  │   writes ./superpowers/plans/...  │
                  │   stores plan_ref on baton        │
                  │   baton → spec_review             │
                  ▼                                  │
              Codex (Q&A / revision proposals)        │
                  │   uses superpowers:brainstorming  │
                  │   may edit plan_ref plan          │
                  │   baton → spec_revision (claude)  │
                  │    or → phase 1 implementing      │
                  ▼                                  │
              Claude (revision)                       │
                  │   addresses Codex Q&A             │
                  │   baton → spec_review (loop)      │
                  └──┬───────────────────────────────┘
                     │
                     │ plan_ref plan converged
                     │ total_phases set
                     │ baton phase: 1, status: implementing
                     ▼
   ┌─── PER-PHASE LOOP (for phase N in 1..total_phases) ───┐
   │                                                       │
   │   Claude (implementing phase N)                       │
   │     uses superpowers:test-driven-development          │
   │     baton → phase_review (review_target: impl)        │
   │       ▼                                               │
   │   Codex (reviewing phase N)                           │
   │     decides: approve / fix narrowly / hand back       │
   │       │                                               │
   │       ├─ approve, no changes ──▶ phase N+1 (or done)  │
   │       │                                               │
   │       ├─ apply narrow fixup ──▶ MUTUAL REVIEW         │
   │       │     baton → review_of_review                  │
   │       │     review_target: codex_fixups               │
   │       │     assignee: claude                          │
   │       │       ▼                                       │
   │       │   Claude (reviewing Codex fixups)             │
   │       │       │                                       │
   │       │       ├─ approve ──▶ phase N+1 (or done)      │
   │       │       │                                       │
   │       │       └─ disapprove ──▶ DISAGREEMENT LOOP     │
   │       │             disagreement_round += 1           │
   │       │             Claude writes counter-change       │
   │       │             baton → counter_review            │
   │       │             review_target: claude_counter     │
   │       │               ▼                               │
   │       │           Codex reviews counter-change        │
   │       │               │                               │
   │       │               ├─ approve ──▶ phase N+1        │
   │       │               │                               │
   │       │               ├─ disapprove, propose new fix ─┘
   │       │               │    (loop back to mutual review)│
   │       │               │                               │
   │       │               └─ disagreement_round ≥ 3 ────┐ │
   │       │                   baton → human_decision   │ │
   │       │                                            │ │
   │       └─ hand back (substantive issues) ──▶ Claude │ │
   │             baton → phase_fixing                   │ │
   │             findings array populated                │ │
   │             Claude fixes, hands back to Codex       │ │
   │             (re-enters Codex review at top of loop) │ │
   │                                                     │ │
   └─── on phase N+1 ──▶ Claude (implementing phase N+1) │ │
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

   Final phase complete → status: done → cycle ends
```

Phase advancement invariant: Claude never advances a phase directly after implementation or fixing. Advancement is legal only when Codex approves Claude's implementation with no changes, Claude approves Codex's narrow fixups, or Codex approves Claude's counter-change in the disagreement loop. The agent writing the first baton for the next phase must set `disagreement_round: 0`.

Three caps the spec enforces operationally:

- **Disagreement round cap (default 3).** Counter resets at the start of each phase. On the 3rd mutual-review disapproval, the writing agent must set `status: human_decision` and exit. Tunable per-phase via a `disagreement_cap` field on the baton (set during spec phase by either agent).
- **Per-invocation turn cap (default 20).** Each agent's `/goal` invocation must stop after 20 turns even if the baton condition has not been hit, and surface its current state for human review. Tunable per-invocation via a goal-prompt argument.
- **No phase count cap.** Plans declare `total_phases` during the spec phase; the protocol does not constrain how many phases are reasonable. The spec phase itself is responsible for sane phase scoping.

## 7. dvandva-doer skill design

### 7.1 Frontmatter

- `name: dvandva-doer`
- `description:` one paragraph, front-loaded with trigger words: *implement*, *doer*, *spec*, *plan with codex*, *phased implementation*, *hand off for review*, *review codex fixups*. Must list both spec-phase triggers and implementation-phase triggers since one skill handles both. Under the 1,536-char listing cap.

No `allowed-tools` reliance (see section 9). Optional Claude-only `argument-hint: "[task description]"` for UX.

### 7.2 Body sections (target < 500 lines)

1. **Role one-liner** — "You are the Dvandva doer. You draft plans, implement them phase by phase, and review Codex's narrow fixups."
2. **Preflight (all modes)** — read `AGENTS.md`, read `.dvandva/baton.json` if present (scaffold from template if absent), determine current mode from `phase` + `status` + `review_target`, verify `assignee == claude`, refuse otherwise.
3. **Mode A: spec drafting** — when `phase: "spec", status: "spec_drafting"`. Invoke `superpowers:brainstorming` skill flow. Produce a gitignored Superpowers plan under `./superpowers/plans/YYYY-MM-DD-<topic>.md` with declared `total_phases` and a per-phase scope list. Use `superpowers:writing-plans` to convert the spec into a phase-by-phase plan. Set `plan_ref` and `total_phases` on the baton. Write baton with `status: spec_review, assignee: codex, review_target: spec`. Exit.
4. **Mode B: spec revision** — when `phase: "spec", status: "spec_revision"`. Read the baton's `findings` array (Codex's Q&A), respond in the `plan_ref` plan, update affected `total_phases` if scope changed. Always write baton with `status: spec_review, assignee: codex, review_target: spec`; Codex is the only actor that can advance the spec to phase 1. Exit.
5. **Mode C: phase implementation** — when `phase: 1..N, status: "implementing"`. Read the corresponding phase scope from the `plan_ref` plan. Invoke `superpowers:test-driven-development` when applicable. Implement only the phase scope; do not bleed into adjacent phases. Run motivating tests and cheap checks. Surface all commands + results. Write baton with `status: phase_review, assignee: codex, review_target: implementation`. Exit.
6. **Mode D: phase fixing** — when `phase: 1..N, status: "phase_fixing"`. Read `findings` from Codex. Fix only listed items. Run verification. Write baton with `status: phase_review, assignee: codex, review_target: implementation` (re-entering Codex review). Exit.
7. **Mode E: codex-fixup review** — when `status: "review_of_review", review_target: "codex_fixups", assignee: claude`. Read Codex's `narrow_fixups` array and inspect the diff Codex applied. Decide: approve or disapprove.
   - On approve: write baton with `phase: N+1, status: implementing, assignee: claude, disagreement_round: 0` (advance), or `status: done` if N was the final phase. Exit.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write counter-changes inline, write baton `status: counter_review, review_target: claude_counter, assignee: codex`. Exit.
8. **Stop rule (universal)** — exit after writing any baton that assigns away from claude. Inside `/goal`, this happens by the goal evaluator detecting the baton-state surface line and stopping the loop.
9. **`/goal` condition** — embedded in the skill body verbatim, e.g., *"Work until `.dvandva/baton.json` has `assignee` not equal to `claude` or `status` is `done` or `human_decision`. Before stopping, read the baton back into the transcript, list changed files, list verification commands and outcomes, and do not modify files outside the requested scope."*
10. **Failure modes** — section 12.

## 8. dvandva-reviewer skill design

### 8.1 Frontmatter

- `name: dvandva-reviewer`
- `description:` front-loaded triggers: *review*, *codex review*, *spec Q&A*, *reviewer pass*, *narrow fixups*, *adversarial verification*, *check the baton*, *review claude counter-changes*. Covers all three reviewer modes.

### 8.2 Body sections

1. **Role one-liner** — "You are the Dvandva reviewer. You Q&A on plans, review implementation phases, apply narrow fixups, and review Claude's counter-changes."
2. **Preflight** — read `AGENTS.md`, read `.dvandva/baton.json`, verify `assignee == codex`. Refuse and exit if not. **Additionally verify `superpowers:brainstorming` is available in the current Codex session**; if absent, surface install instructions and exit (per section 4 prerequisites). Do not depend on one fixed filesystem path.
3. **Mode A: spec Q&A** — when `phase: "spec", status: "spec_review", review_target: "spec"`. Invoke `superpowers:brainstorming` skill flow as the questioner. Read the `plan_ref` plan, surface Q&A in the baton's `findings` array, optionally edit the plan directly for narrow improvements (typos, sharper phrasing). Decide: hand back to Claude (questions remain) or advance. Write baton `status: spec_revision, assignee: claude` (for more Q&A) or `phase: 1, status: implementing, assignee: claude, disagreement_round: 0` (advance to phase 1). Exit.
4. **Mode B: phase implementation review** — when `phase: 1..N, status: "phase_review", review_target: "implementation"`. Read diff vs branch baseline. Cross-check doer's `verification` block (did the commands actually pass? do they cover the changed paths?). Look for bugs, regressions, stale docs, missing tests, claims not matching diff.
5. **Narrow-fix allowlist** (verbatim from `docs/workflows/two-mode-agent-workflow.md:41-47`):
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
   - If only handback issues: populate `findings`, write baton `status: phase_fixing, assignee: claude`. Exit.
   - If narrow fixups apply AND no handback issues: apply fixups inline, run verification, populate `narrow_fixups` array. Write baton `status: review_of_review, review_target: codex_fixups, assignee: claude` (route to mutual review). Exit.
   - If narrow fixups apply AND handback issues: populate both `findings` and `narrow_fixups`; route to `phase_fixing` first; mutual review of the narrow fix happens on the next Codex pass after Claude's fix.
   - If approve, no changes: write baton with `phase: N+1, status: implementing, assignee: claude, disagreement_round: 0` or `status: done` if final phase. Exit.
8. **Mode C: claude-counter review** — when `status: "counter_review", review_target: "claude_counter", assignee: codex`. Read Claude's counter-change diff. Decide:
   - On approve: write baton `phase: N+1, status: implementing, assignee: claude, disagreement_round: 0` (advance), or `status: done` if final phase. Exit.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write a new narrow fixup and route back to `review_of_review, review_target: codex_fixups, assignee: claude`. Exit.
9. **Stop rule** — exit after writing the baton.
10. **`/goal` condition** — *"Review the branch using `.dvandva/baton.json` as the handoff. Apply only narrow fixups within the allowlist. Stop when the baton has `assignee` not equal to `codex` or `status` is `done` or `human_decision`. Before stopping, surface findings, verification commands and outcomes, the final baton contents."*
11. **Failure modes** — section 12.

## 9. Cross-engine portability

Both skills target the agentskills.io open standard. Only the universal frontmatter (`name`, `description`) carries correctness weight. Optional engine-specific fields are avoided in v1:

- **No `allowed-tools` reliance.** The agentskills.io spec treats it as implementation-varying. Skill bodies assume the user's existing permission setup allows git, bash, and the project's test runner. One-time tool prompts are acceptable; the skill does not depend on pre-approval.
- **No `paths` glob.** Skills are workflow-scoped, not file-scoped.
- **No `context: fork`.** Skills run in the main session so `/goal` transcript surfacing works (the goal evaluator only sees what's surfaced).
- **No engine-specific frontmatter extensions.** If forced in a future rev, the SKILL.md forks into engine-specific variants; document the reason explicitly.

**superpowers compatibility note:** both engines must have superpowers installed at runtime. The doer relies on `superpowers:brainstorming` + `superpowers:writing-plans` + `superpowers:test-driven-development`; the reviewer relies on `superpowers:brainstorming`. Skills invoke these via the engine's native `Skill` tool. If superpowers is absent, the reviewer's preflight (section 8.2 step 2) refuses to run; the doer's spec phase fails on the first `Skill` call with a clear error.

## 10. Description tuning strategy

Auto-activation depends entirely on `description`. Tuning rules:

- **Front-load trigger phrases.** Doer description starts: *"Use when the user asks Claude to draft a plan or implement code as part of a Claude+Codex pair via the Dvandva protocol."* Reviewer description starts: *"Use when the user asks Codex to Q&A on a plan, review a Claude implementation, or review Claude's counter-changes via the Dvandva protocol."*
- **At least three paraphrase variants** per description so partial matches still hit.
- **Explicit anti-trigger** in each: *"Do not use this skill for solo work not paired with the other agent."*
- **Calibration during pilot.** If a skill mis-fires or fails to fire, the pilot writeup records user phrasing → activation outcome, and the description gets one edit pass.

## 11. Distribution and install

### 11.1 Primary install (user level, pilot setup)

```bash
# from this repo's root
ln -s "$(pwd)/skills/dvandva-doer"     ~/.claude/skills/dvandva-doer
ln -s "$(pwd)/skills/dvandva-reviewer" ~/.agents/skills/dvandva-reviewer

# verify superpowers prerequisites
ls ~/.claude/plugins/cache/*/superpowers/ 2>/dev/null || echo "ERROR: install superpowers on Claude Code first"
test -n "$(find ~/.codex ~/.agents -maxdepth 6 -path '*superpowers*' 2>/dev/null | head -1)" || echo "ERROR: install superpowers for Codex"
```

README spells these commands out for macOS / Linux symlink and Windows copy / `mklink /D`. The filesystem check is only a convenience check; the authoritative preflight is whether the current agent session can see and invoke the required Superpowers skills.

### 11.2 Secondary install (team adoption in consumer repos)

Consumer repos check skills under `.claude/skills/` and `.agents/skills/`. Both engines walk from cwd up to repo root. The README includes the trust warning verbatim: *"Project-level skills can carry tool-permission frontmatter. Review the SKILL.md contents the same way you would any other `.claude/` or `.agents/` config the repo ships before trusting it."*

### 11.3 Plugin packaging (deferred)

Both engines support plugin distribution ([Claude](https://code.claude.com/docs/en/plugins), [`codex plugin marketplace`](https://developers.openai.com/codex/plugins)). v1 stays at manual symlink. v2 considers publishing dvandva as a marketplace plugin once pilot data informs whether the friction reduction is worth the release surface.

## 12. Failure modes the skills must handle

| Failure | Required behavior |
|---|---|
| `.dvandva/baton.json` missing | Doer in spec mode: scaffold from template, set `phase: "spec", status: "spec_drafting"`. Reviewer: surface "no baton — doer has not started" and exit. |
| Baton present but malformed JSON | Both: do not overwrite. Surface parse error verbatim. Write `.dvandva/baton.broken.json` with the unparseable bytes preserved. Surface in-memory next state as `human_decision`. |
| `schema` field is not `dvandva.baton.v1` | Both: refuse to operate. Surface schema mismatch. Exit. |
| `assignee` does not match this agent's role | Surface "wrong actor for this state" and exit. Never silently overwrite the assignee. |
| `superpowers` absent on Codex (reviewer preflight) | Surface install instructions referencing section 4 and `codex plugin marketplace`. Exit. |
| `plan_ref` missing, or referenced plan file missing during a phase mode | Doer: surface "spec phase did not complete; cannot start phase implementation." Exit. Reviewer: same. |
| `total_phases` is 0 or unset during phase mode | Both: surface schema integrity error, exit. The spec phase is responsible for setting this. |
| `disagreement_round >= disagreement_cap` | Whichever agent next writes the baton: set `status: human_decision, assignee: human`. Do not write further counter-changes. |
| `/goal` turn limit (default 20) hit before exit condition | Agent: surface current baton state and a "still owe work" summary, set `status: human_decision`, exit. |
| Reviewer finds no diff vs baseline (after Claude said implementation done) | Write `findings: ["doer claimed implementation but produced no diff"]`, route to `human_decision`. |
| Both agents accidentally started concurrently | v1 cannot detect. Skill body warns in preflight; deterministic detection is v2. |
| Git working tree dirty before spec phase starts | Doer: surface dirty state in baton `summary`, proceed only if user's prompt indicates intent. |
| `plan_ref` plan edited by Codex during spec Q&A and `total_phases` changed | Doer's spec-revision mode reads the new `total_phases` from the plan and updates the baton to match. **The plan referenced by `plan_ref` is authoritative during the spec phase; the baton is authoritative during implementation phases.** Once `phase: 1` is set, `total_phases` is frozen on the baton and the plan is treated as reference. |

## 13. Testing strategy

v1 has no automated test surface for skill behavior. What can be tested:

- **Frontmatter linter** (a small script committed to the repo): parses both SKILL.md files, confirms required frontmatter, checks `description` ≤ 1,536 chars, checks body ≤ 500 lines. Suggested pre-commit hook.
- **Schema key-presence check** (same script): the inlined `dvandva.baton.v1` JSON in each SKILL.md must parse as valid JSON and contain the required keys from Appendix A. Not a JSON Schema check — that's v2.
- **Smoke test (manual)**: symlink both skills + verify both auto-list in `claude` / `codex` `/skills` menus. Verify the reviewer preflight gate triggers in a clean Codex profile where `superpowers:brainstorming` is not available.
- **Pilot as integration test:** the pilot is the v1 integration test. Success criteria #1–#5 in section 2 are the acceptance gate.

## 14. Risks and open questions

Named risks, ordered by severity:

1. **Disagreement-cap mechanism untested.** If the cap doesn't fire correctly, two agents can lock into infinite counter-change loops. Mitigation: success criterion #5 is the gate; pilot must exercise the loop at least once and confirm it caps correctly.
2. **`/goal` evaluator misjudges baton state surface.** If Claude or Codex surfaces a baton-state line in a way the evaluator misreads, the loop may stop prematurely or fail to stop. Mitigation: skill bodies require a structured "BATON_STATE: {...}" line at every checkpoint, parseable by simple regex.
3. **superpowers parity drift between Claude Code and Codex.** Superpowers is one codebase but ships through two distribution channels (Claude Code plugins, Codex plugin marketplace). Version drift could mean a brainstorming skill that exists on one but not the other. Mitigation: the skill bodies invoke only well-established superpowers skills (brainstorming, writing-plans, test-driven-development); pilot writeup records each agent's superpowers version.
4. **Human-as-dispatcher cost.** Same as the previous draft: every agent-to-agent transition still requires a human typing the next command. v2 introduces a runner. Accepted for v1.
5. **The `plan_ref` plan becomes a contested file.** Both agents may write to it during the spec phase. Conflict resolution is currently "whoever writes last wins, baton acknowledges via `summary`." If both agents wanted to edit the same line in a single round, behavior is unspecified. Mitigation: in v1 the spec phase is strictly serial (Claude writes, then Codex Q&As, then Claude revises); concurrent edits should not happen. Document but don't enforce in v1.
6. **Mutual-review can re-introduce a regression Codex thought it fixed.** Claude disapproves Codex's fix, writes a counter, Codex reviews the counter — but Codex may now be checking against its *own* prior view, not the original Claude implementation. Mitigation: the baton's `narrow_fixups` and `claude_counter` arrays preserve diff context across the loop; reviewer's mode C re-reads from the baton not from session memory.
7. **Same-GitHub-identity attribution unsolved.** PR 353's pain point. v1 stays out of GitHub entirely. Postponed, not solved.

Open questions to revisit after the pilot:

- Does the per-phase turn cap (default 20) need to scale with phase scope? Larger phases may legitimately need more turns.
- Should the spec phase have its own turn cap separate from the per-phase one?
- How does `claude --resume` behave when the baton has advanced past the paused session's state? Likely fine since skill preflight re-reads the baton, but the first occurrence should be documented.

## 15. Versioning policy

- **Spec version:** this document is v1's source of truth. Changes to in-scope behavior require a spec rev with a `docs:` commit prefix and a section number reference.
- **Schema version:** baton field is `dvandva.baton.v1`. Breaking changes increment to `v2`; both skills must update in lockstep. Skills refuse to operate on a mismatched schema string (section 12).
- **Schema maintenance.** The schema lives in three places: Appendix A (canonical), `templates/channel/baton.json` (operational seed for `.dvandva/`), inlined in each SKILL.md (agent in-context contract). On changes: update Appendix A first, then template file, then both SKILL.md inlines. The v2 deterministic validator (section 16) will make Appendix A machine-checkable.
- **Skill versions:** each SKILL.md may carry a `# Skill version: <semver>` comment in the body. Bumped on body changes that alter agent behavior.

## 16. Future work (v2 and beyond)

In priority order:

- **Deterministic validator script** + real JSON Schema at `templates/channel/baton.schema.json`. Skills invoke it as a pre-write gate. Rejects malformed batons and illegal transitions per the table in Appendix A. Closes the "enforcement is just prompt text" gap.
- **Runner / launcher.** File watcher that detects baton transitions and starts the next agent via `claude -p` / `codex exec`. Closes the human-as-dispatcher gap. Must preserve human visibility (likely a "tail the baton" dashboard).
- **Plugin packaging.** Ship dvandva as Claude Code plugin + Codex plugin with manifests. Install becomes one command instead of two symlinks.
- **Generic role abstraction.** Promote `doer` / `reviewer` to first-class abstract roles with Claude/Codex as canonical instantiations. Largest portability risk currently.
- **GitHub PR summary integration.** Skill-side helper that turns the final baton state into a one-shot PR summary the human pastes in. Solves attribution if and only if it is the *only* PR comment.
- **Concurrent-agent detection.** Lock file or PID file with stale-detection so v2 can refuse to start a second agent against a baton already in use.
- **Per-phase scope refinement.** v2 could auto-suggest phase boundaries based on file-graph or churn analysis during the spec phase.

## Appendix A — `dvandva.baton.v1` canonical schema and transitions

This appendix is the spec-level authoritative reference for the schema (including reviewer-only fields) and the v1 state-transition table. In v1, the template file at `templates/channel/baton.json` must be a minimal initial baton seed used by `dvandva-doer` on first run; it carries the same schema shape but only the always-present fields.

The current repository template is still the v0 seed until implementation work updates it. That update is required before pilot acceptance; otherwise the doer skill would scaffold a baton that immediately violates this appendix.

### Schema

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "feature-pr | campaign",
  "phase": "spec | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | implementing | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are claude | codex | human",
  "review_target": "spec | implementation | codex_fixups | claude_counter | null",
  "plan_ref": "path to gitignored Superpowers plan file under ./superpowers/plans/, set during spec phase",
  "disagreement_round": "integer, set to 0 by the agent that writes the first baton of each new phase; incremented by the agent that disagrees during mutual review",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "turn_cap": "integer, default 20, applied to each /goal invocation",
  "branch": "git branch name",
  "checkpoint": "integer, bumped by the writer",
  "summary": "one-paragraph human-readable summary of this checkpoint",
  "changed_paths": ["array of paths touched in this checkpoint"],
  "verification": [
    { "command": "exact shell command run", "result": "passed | failed | skipped", "notes": "optional one-liner" }
  ],
  "findings": ["reviewer or counter-reviewer: bullets describing issues found"],
  "narrow_fixups": ["reviewer: bullets describing narrow fixes applied directly"],
  "claude_counter": ["doer-as-reviewer: bullets describing counter-changes proposed during mutual review"],
  "deferred": ["reviewer: items deferred with one-line rationale and next-recommended-action"],
  "blockers": ["bullets describing what is blocking forward progress"],
  "next_action": "exact one-sentence instruction for the next actor"
}
```

### Allowed state transitions (v1, authoritative)

This spec is authoritative for v1 transitions and supersedes `docs/protocol/local-baton-channel.md:46-68`. The protocol doc will be updated in a follow-up commit to match.

**Spec phase transitions:**

| From | To | Trigger |
|---|---|---|
| (no baton) | `phase: "spec", status: "spec_drafting"` | Doer first run |
| `spec_drafting` | `spec_review` | Doer hands plan to reviewer for Q&A |
| `spec_review` | `spec_revision` | Reviewer surfaces Q&A back to doer |
| `spec_review` | `phase: 1, status: implementing` | Reviewer accepts plan and freezes `total_phases` on the baton |
| `spec_revision` | `spec_review` | Doer answers Q&A, hands back |
| any spec state | `human_decision` | Either agent escalates |

**Implementation phase transitions (per phase N):**

| From | To | Trigger |
|---|---|---|
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Doer completes phase, hands to reviewer |
| `phase: N, implementing` | `human_decision` | Doer blocked |
| `phase_review (impl)` | `phase_fixing` | Reviewer hands back substantive findings |
| `phase_review (impl)` | `review_of_review, review_target: codex_fixups` | Reviewer applied narrow fixups, mutual review owed |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done`) | Reviewer approves, no changes |
| `phase_review (impl)` | `human_decision` | Reviewer escalates |
| `phase_fixing` | `phase_review (impl)` | Doer addressed findings, re-hands |
| `phase_fixing` | `human_decision` | Doer blocked during fix |
| `review_of_review (codex_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done`) | Doer approves Codex fixups |
| `review_of_review (codex_fixups)` | `counter_review, review_target: claude_counter` | Doer disapproves, writes counter (disagreement_round +=1) |
| `review_of_review (codex_fixups)` | `human_decision` | disagreement_round ≥ cap |
| `counter_review (claude_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` (or final `done`) | Reviewer approves counter |
| `counter_review (claude_counter)` | `review_of_review, review_target: codex_fixups` | Reviewer disapproves counter, applies a different fix (disagreement_round +=1) |
| `counter_review (claude_counter)` | `human_decision` | disagreement_round ≥ cap |
| any state | `human_decision` | escalation (`disagreement_round >= cap`, `turn_cap` hit, blocker, malformed input) |
| `human_decision` | any state | After human edits baton or prompts an agent |

Any other transition is illegal in v1 and must be rejected by the writing agent.

## Appendix B — pilot acceptance scaffold

Filled in for `docs/case-studies/pilot-01.md` after the pilot completes. Comparison to the PR 353 baseline from `docs/case-studies/pr-353.md`.

| Metric | PR 353 baseline | Pilot result | Delta |
|---|---|---|---|
| Total commits in the PR | 116 | | |
| Agent-to-agent PR comments | 186 | | |
| Wall-clock from first doer turn to `status: done` | (not recorded) | | n/a |
| Doer turns (across all phases) | (not recorded) | | n/a |
| Reviewer turns (across all phases) | (not recorded) | | n/a |
| Spec phase rounds (Claude draft + Codex Q&A + revision) | n/a | | n/a |
| Implementation phases declared | n/a | | n/a |
| Mutual-review loops triggered | n/a | | n/a |
| Disagreement-cap fires | n/a | | n/a |
| Real issues caught by reviewer | qualitative (see PR 353 "What Worked") | | |
| Auto-activation rate (doer) | n/a | X / Y attempts | n/a |
| Auto-activation rate (reviewer) | n/a | X / Y attempts | n/a |
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
