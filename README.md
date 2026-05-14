# Dvandva

Dvandva is an orchestration framework for pairing two coding agents into a disciplined collaboration protocol. Coordination happens through a local baton file; PR comments are reserved for human-facing summaries. The canonical setup pairs Claude Code as vadi and Codex as prativadi, but either engine can host either role — including single-engine setups where one CLI plays both roles serially.

vadi (वादी) is Sanskrit for "proposer" and prativadi (प्रतिवादी) for "responder" — terms from classical Indian philosophical debate, which is the duo's working metaphor.

The repo lives at https://github.com/axatbhardwaj/Dvandva.

## Quickstart

Two commands plus one prompt:

```bash
# Clone and install
git clone https://github.com/axatbhardwaj/Dvandva.git && cd Dvandva
mkdir -p ~/.claude/skills ~/.agents/skills
for engine_dir in ~/.claude/skills ~/.agents/skills; do
  ln -sf "$(pwd)/skills/dvandva-vadi"      "$engine_dir/dvandva-vadi"
  ln -sf "$(pwd)/skills/dvandva-prativadi" "$engine_dir/dvandva-prativadi"
done
```

Then in any feature-branch worktree, open Claude Code and type:

> *Implement \<small feature\> with codex review. Use dvandva walkaway.*

The vadi skill auto-activates from the description, scaffolds `.dvandva/baton.json`, drafts a plan, and hands off to the prativadi by writing the baton. Open a Codex session in the same worktree:

> *Review the dvandva baton.*

The prativadi skill auto-activates, Q&As during planning or reviews the implementation, hands back to the vadi via the baton, and the cycle repeats until the baton reaches `done`.

If you only have one engine (Claude Code OR Codex), set `run_mode: "supervised"` instead and invoke the two skills serially in the same session — see [Single-engine workflow](#usage) below.

A typical handoff in transcript looks roughly like:

```
[vadi session — Claude Code]
> BATON_STATE: { phase: 1, status: phase_review, assignee: prativadi, ... }
> [vadi blocks in dvandva-wait.sh — no model turns spent here]

[prativadi session — Codex, picks up the baton]
> BATON_STATE: { phase: 1, status: phase_review, assignee: prativadi, ... }
> [reviewing diff, running tests, applying narrow fixups]
> BATON_STATE: { phase: 1, status: review_of_review, assignee: vadi, ... }
> [prativadi now blocks; vadi wakes from its wait]
```

## Current State

v1 ships as a pair of [agentskills.io](https://agentskills.io)-standard skills (`dvandva-vadi` for Claude Code, `dvandva-prativadi` for Codex) plus a small foreground wait helper. The default `run_mode` is `walkaway`: start both sessions once, then let the baton decide which role works next.

The full product spec is in `product.md`. A sanitized case study of the internal PR 353 research run that motivated the design lives at `docs/case-studies/pr-353.md`.

## Core Design

Dvandva treats agent collaboration as a state machine with three lifecycle segments:

1. **Master planning** — the vadi drafts a plan using `superpowers:brainstorming` and `superpowers:writing-plans`. The prativadi Q&As. Either role may ask the user questions until the master plan is locked.
2. **Per-phase implementation loop** — the vadi implements each phase. The prativadi reviews. If the prativadi applies narrow fixups, the vadi reviews them (mutual review). On disagreement, the vadi counter-changes and the prativadi reviews; up to 3 rounds before forced human escalation.
3. **Walkaway completion** — after the master plan is locked, the agents keep handing off through `.dvandva/baton.json` until `done`, `human_question`, or `human_decision`. On final agreement, both roles must set final approval before commit/push. PR creation is forbidden.

The human can watch `.dvandva/baton.json`, the transcript, or the wait-helper output, but involvement is voluntary unless the baton asks a planning question or escalates to `human_decision`.

## Prerequisites

At least one engine must have superpowers installed. The canonical setup pairs Claude Code as vadi and Codex as prativadi; single-engine setups (one CLI playing both roles sequentially) also work.

| Prerequisite | Verify |
|---|---|
| Claude Code installed (optional if using Codex for both roles) | `claude --version` |
| Codex CLI ≥ 0.130 (optional if using Claude Code for both roles) | `codex --version` |
| superpowers plugin on the engine(s) you will use | `claude` then `/skills` lists `superpowers:brainstorming`; or `codex` then `/skills` lists it. Install via `codex plugin marketplace` or upstream symlink per https://deepwiki.com/obra/superpowers/2.4-installing-on-codex |
| Working directory is a git repo on a feature branch | `git rev-parse --abbrev-ref HEAD` returns something other than `main` / `master` |
| `jq` installed | `jq --version` |

The `dvandva-prativadi` skill refuses to run if `superpowers:brainstorming` is not available in the current session (Mode A only — phase reviews and counter reviews proceed without superpowers).

The `dvandva-vadi` skill's spec phase also requires superpowers — it invokes `superpowers:brainstorming` and `superpowers:writing-plans` and fails immediately if either is unavailable.

## Install

### Primary: user-level symlink

From this repo's root:

```bash
# Install both skills into both engines' skill directories.
# Either engine can host either role; redundant symlinks are harmless.
mkdir -p ~/.claude/skills ~/.agents/skills
for engine_dir in ~/.claude/skills ~/.agents/skills; do
  ln -sf "$(pwd)/skills/dvandva-vadi"      "$engine_dir/dvandva-vadi"
  ln -sf "$(pwd)/skills/dvandva-prativadi" "$engine_dir/dvandva-prativadi"
done
```

Then verify:

```bash
ls ~/.claude/skills/dvandva-vadi/SKILL.md
ls ~/.claude/skills/dvandva-prativadi/SKILL.md
ls ~/.agents/skills/dvandva-vadi/SKILL.md
ls ~/.agents/skills/dvandva-prativadi/SKILL.md
```

Open `claude` and run `/skills`. Both `dvandva-vadi` and `dvandva-prativadi` should be listed. Open `codex` and run `/skills`. Both should be listed there too.

**On Windows:** use `mklink /D` instead of `ln -s` from PowerShell (run as Administrator). Replace the symlink loop above with:

```cmd
mkdir "%USERPROFILE%\.claude\skills" 2>nul
mkdir "%USERPROFILE%\.agents\skills" 2>nul
mklink /D "%USERPROFILE%\.claude\skills\dvandva-vadi"      "<full-path-to-repo>\skills\dvandva-vadi"
mklink /D "%USERPROFILE%\.claude\skills\dvandva-prativadi" "<full-path-to-repo>\skills\dvandva-prativadi"
mklink /D "%USERPROFILE%\.agents\skills\dvandva-vadi"      "<full-path-to-repo>\skills\dvandva-vadi"
mklink /D "%USERPROFILE%\.agents\skills\dvandva-prativadi" "<full-path-to-repo>\skills\dvandva-prativadi"
```

If `mklink` is not available, copy the directories instead.

### Secondary: project-level adoption

A **consumer repo** is any repo where you want to use Dvandva for your feature work (i.e., not the Dvandva source repo itself). Consumer repos that intentionally adopt Dvandva can check the skills directly under their own `.claude/skills/` and `.agents/skills/` directories instead of relying on user-level symlinks. Both engines walk from cwd up to the repo root looking for these directories.

**Trust warning:** Project-level skills can carry tool-permission frontmatter (Claude `allowed-tools`, Codex skill metadata). Review the `SKILL.md` contents the same way you would any other `.claude/` or `.agents/` config the repo ships before trusting it. The in-repo skill bodies are at `skills/dvandva-vadi/SKILL.md` and `skills/dvandva-prativadi/SKILL.md`.

## Usage

In a feature-branch worktree, start both agent sessions once.

In the vadi session, prompt natural language:

> "Implement the X feature with Codex review. Use dvandva."

`dvandva-vadi` auto-activates from the description. It scaffolds `.dvandva/baton.json`, drives master planning, and writes a handoff.

In the prativadi session, prompt:

> "Review the dvandva baton."

`dvandva-prativadi` auto-activates, Q&As during master planning or reviews implementation phases. When either role is not assigned, the skill runs:

```bash
${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --interval 60 --max-wait 900
```

`${CLAUDE_SKILL_DIR}` resolves to the directory containing the SKILL.md file (e.g., `~/.claude/skills/dvandva-vadi`). Claude Code auto-substitutes it; in Codex the LLM resolves it from the load path. The helper is bundled inside each skill dir as a symlink, so it travels with the install symlink.

That command blocks cheaply in the shell until the baton returns to the role, reaches `done`, reaches `human_question`, or reaches `human_decision`. The agent should re-read the baton and continue when the wait returns ready. The 15-minute max wait is only a heartbeat; timeout means "still waiting", not failure.

Explicit invocation (`/dvandva-vadi`, `$dvandva-vadi`, `/dvandva-prativadi`, `$dvandva-prativadi`) is documented fallback if auto-activation misfires.

**Planning questions:** before `master_plan_locked: true`, either role may set `status: "human_question"`. The wait helper prints the question plus `resume_assignee` and `resume_status`. Answer in the agent session; the skill records the answer, restores `assignee`/`status` from those resume fields, clears the question fields, and continues.

**Single-engine workflow:** if you have only Claude Code (or only Codex) installed, run `run_mode: "supervised"`. In supervised mode the assigned-away role exits instead of blocking in the wait helper, so you can invoke the other role in the same session without deadlock. Full walkaway requires two persistent sessions.

**Commit/push rule:** in default walkaway mode, agents may commit and push only after `vadi_final_approval` and `prativadi_final_approval` are both true. The baton defaults to `allow_commit: true`, `allow_push: true`, and `allow_pr: false`. Agents must never open a PR.

## Linting and Validation

Small Bash+jq scripts validate the skill bodies and wait helper:

```bash
bash scripts/lint-skills.sh skills/dvandva-vadi/SKILL.md
bash scripts/lint-skills.sh skills/dvandva-prativadi/SKILL.md
bash scripts/test-dvandva-wait.sh
```

A future v2 will add a deterministic baton schema and transition validator (see `product.md` section 16).

## Historical Templates

The files at `templates/prompts/claude-doer-goal.md` and `templates/prompts/codex-reviewer-goal.md` are the v0 form of the protocol — pre-skill prompt templates pasted into `/goal`. They are kept in-tree as reference but are superseded by the SKILL.md files in `skills/`. Do not use the templates for new work.

## Non-Goals

- Dvandva is not trying to make agents chat endlessly.
- It is not trying to replace human approval for risky or ambiguous changes; those route to `human_question` during planning or `human_decision` after the plan is locked.
- It does not assume GitHub PR comments are the main coordination channel.
- It does not create PRs.
- v1 does not include a daemon, process launcher, schema validator, or GitHub integration. Those are tracked as future work in `product.md` section 16.

## Reading Order

1. `product.md` — v1 product specification (authoritative)
2. `docs/workflows/two-mode-agent-workflow.md` — Feature PR vs Campaign mode
3. `docs/protocol/local-baton-channel.md` — baton state machine (aligned with product.md Appendix A)
4. `docs/research/claude-code-goal.md` — `/goal` research notes
5. `docs/research/codex-goal-notes.md` — Codex `/goal` notes
6. `docs/case-studies/pr-353.md` — sanitized case study of the failure-mode dataset that motivated Dvandva

## Research Sources

- Claude Code skills: https://code.claude.com/docs/en/skills
- Claude Code `/goal`: https://code.claude.com/docs/en/goal
- Codex skills: https://developers.openai.com/codex/skills
- Codex plugins: https://developers.openai.com/codex/plugins/build
- Codex AGENTS.md guide: https://developers.openai.com/codex/guides/agents-md
- agentskills.io open standard: https://agentskills.io
- Superpowers framework: https://github.com/obra/superpowers
- Superpowers Codex install guide: https://deepwiki.com/obra/superpowers/2.4-installing-on-codex
