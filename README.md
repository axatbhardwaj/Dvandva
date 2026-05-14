# Dvandva

Dvandva is an orchestration framework for pairing two coding agents into a disciplined collaboration protocol. Coordination happens through a local baton file; PR comments are reserved for human-facing summaries. The canonical setup pairs Claude Code as vadi and Codex as prativadi, but either engine can host either role — including single-engine setups where one CLI plays both roles serially.

vadi (वादी) is Sanskrit for "proposer" and prativadi (प्रतिवादी) for "responder" — terms from classical Indian philosophical debate, which is the duo's working metaphor.

## Current State

v1 ships as a pair of [agentskills.io](https://agentskills.io)-standard skills (`dvandva-vadi` for Claude Code, `dvandva-prativadi` for Codex) that encode a phased spec-then-implementation flow with mutual review and a disagreement cap.

The full product spec is in `product.md`. A sanitized case study of the internal PR 353 research run that motivated the design lives at `docs/case-studies/pr-353.md`.

## Core Design

Dvandva treats agent collaboration as a state machine with three lifecycle segments:

1. **Spec phase** — the vadi drafts a plan using `superpowers:brainstorming` and `superpowers:writing-plans`. The prativadi Q&As. The vadi revises. Loop until the plan converges.
2. **Per-phase implementation loop** — the vadi implements each phase. The prativadi reviews. If the prativadi applies narrow fixups, the vadi reviews them (mutual review). On disagreement, the vadi counter-changes and the prativadi reviews; up to 3 rounds before forced human escalation.
3. **Phase advancement or completion** — on agreement, advance to phase N+1; on the final phase, transition to `done`.

Both agents run autonomously via `/goal` within each invocation. The human dispatches between invocations.

## Prerequisites

At least one engine must have superpowers installed. The canonical setup pairs Claude Code as vadi and Codex as prativadi; single-engine setups (one CLI playing both roles sequentially) also work.

| Prerequisite | Verify |
|---|---|
| Claude Code installed (optional if using Codex for both roles) | `claude --version` |
| Codex CLI ≥ 0.130 (optional if using Claude Code for both roles) | `codex --version` |
| superpowers plugin on the engine(s) you will use | `claude` then `/skills` lists `superpowers:brainstorming`; or `codex` then `/skills` lists it. Install via `codex plugin marketplace` or upstream symlink per https://deepwiki.com/obra/superpowers/2.4-installing-on-codex |
| Working directory is a git repo on a feature branch | `git rev-parse --abbrev-ref HEAD` returns something other than `main` / `master` |

The `dvandva-prativadi` skill refuses to run if `superpowers:brainstorming` is not available in the current session (Mode A only — phase reviews and counter reviews proceed without superpowers).

The `dvandva-vadi` skill's spec phase also requires superpowers — it invokes `superpowers:brainstorming` and `superpowers:writing-plans` and fails immediately if either is unavailable.

## Install

### Primary: user-level symlink (pilot setup)

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

Consumer repos that intentionally adopt Dvandva check the skills under their own `.claude/skills/` and `.agents/skills/`. Both engines walk from cwd up to the repo root looking for these directories.

**Trust warning:** Project-level skills can carry tool-permission frontmatter (Claude `allowed-tools`, Codex skill metadata). Review the `SKILL.md` contents the same way you would any other `.claude/` or `.agents/` config the repo ships before trusting it. The in-repo skill bodies are at `skills/dvandva-vadi/SKILL.md` and `skills/dvandva-prativadi/SKILL.md`.

## Usage

In a feature-branch worktree, prompt your vadi engine with natural language:

> "Implement the X feature with Codex review. Use dvandva."

`dvandva-vadi` auto-activates from the description. It scaffolds `.dvandva/baton.json`, drives the spec phase, and writes a handoff. When the baton's `assignee` flips to `prativadi`, start your prativadi engine:

> "Review the dvandva baton."

`dvandva-prativadi` auto-activates, Q&As during spec or reviews the implementation, writes a handoff, exits. Repeat the cycle until the baton reaches `status: "done"` or `human_decision`.

Explicit invocation (`/dvandva-vadi`, `$dvandva-vadi`, `/dvandva-prativadi`, `$dvandva-prativadi`) is documented fallback if auto-activation misfires.

**Single-engine workflow:** if you have only Claude Code (or only Codex) installed, both skills run in that one engine. The first invocation kicks off the vadi flow; when the baton flips to `assignee: "prativadi"`, exit the current session and start a new session in the same engine, then invoke `/dvandva-prativadi` (or describe what you want in natural language). Run sequentially until the baton reaches `done` or `human_decision`.

## Linting and Validation

A small Bash+jq linter at `scripts/lint-skills.sh` validates SKILL.md frontmatter and the inlined baton schema:

```bash
bash scripts/lint-skills.sh skills/dvandva-vadi/SKILL.md
bash scripts/lint-skills.sh skills/dvandva-prativadi/SKILL.md
```

A future v2 will add a deterministic baton schema and transition validator (see `product.md` section 16).

## Historical Templates

The files at `templates/prompts/claude-doer-goal.md` and `templates/prompts/codex-reviewer-goal.md` are the v0 form of the protocol — pre-skill prompt templates pasted into `/goal`. They are kept in-tree as reference but are superseded by the SKILL.md files in `skills/`. Do not use the templates for new work.

## Non-Goals

- Dvandva is not trying to make agents chat endlessly.
- It is not trying to replace human approval for risky changes.
- It does not assume GitHub PR comments are the main coordination channel.
- It does not require both agents to run at the same time.
- v1 does not include a CLI binary, a daemon, or a GitHub integration. Those are tracked as v2 work in `product.md` section 16.

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
