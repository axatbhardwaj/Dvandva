# Dvandva

Dvandva packages a two-agent coding workflow as an installable plugin. One role, `vadi`, proposes plans and implements phases. The other, `prativadi`, reviews, applies narrow fixups, and hands control back through a local `.dvandva/baton.json` file.

The canonical dogfood setup is Claude Code as vadi and Codex as prativadi, but either engine can host either role. Single-engine supervised runs are supported; full walkaway mode needs two persistent sessions.

The repo lives at https://github.com/axatbhardwaj/Dvandva.

## Quickstart

Install the marketplace in each engine you want to use:

```bash
claude plugin marketplace add axatbhardwaj/Dvandva
claude plugin install dvandva@dvandva

codex plugin marketplace add axatbhardwaj/Dvandva
```

For Codex, `marketplace add` registers the marketplace. Then restart Codex,
open the plugin directory, select the Dvandva marketplace, and install the
`dvandva` plugin. After install, `/skills` should list `dvandva:vadi` and
`dvandva:prativadi`.

Then start a feature-branch worktree and open both sessions:

```text
Claude: Implement <small feature> with Codex review. Use Dvandva walkaway.
Codex:  Review the Dvandva baton.
```

Claude plugin invocation fallback:

```text
/dvandva:vadi
/dvandva:prativadi
```

Codex skill fallback:

```text
$vadi
$prativadi
```

## Current State

v0.1.0 ships as one `dvandva` plugin with:

- `plugins/dvandva/skills/vadi/SKILL.md`
- `plugins/dvandva/skills/prativadi/SKILL.md`
- bundled `dvandva-wait.sh` helpers in both skill directories
- plugin-local protocol references in `plugins/dvandva/references/`
- Codex marketplace metadata in `.agents/plugins/marketplace.json`
- root marketplace metadata in `.claude-plugin/marketplace.json`

The default `run_mode` is `walkaway`: start both sessions once, then let the baton decide which role works next.

## Prerequisites

| Prerequisite | Verify |
|---|---|
| Claude Code, if using Claude | `claude --version` |
| Codex CLI, if using Codex | `codex --version` |
| superpowers plugin on the engine(s) used for planning | `/skills` lists `superpowers:brainstorming` |
| Work happens on a feature branch | `git branch --show-current` is not `main` or `master` |
| `jq` installed | `jq --version` |

## Usage

In walkaway mode, the assigned-away session blocks in:

```bash
${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --interval 60 --max-wait 900
```

That is shell waiting, not model polling. The agent resumes when the baton assigns its role again, or stops if the baton reaches `done`, `human_question`, or `human_decision`.

For one-engine use, set `run_mode: "supervised"` in `.dvandva/baton.json` and invoke `vadi` and `prativadi` serially in that engine. Supervised mode exits on assigned-away states so one CLI session cannot deadlock itself.

Agents may commit and push only after both `vadi_final_approval` and `prativadi_final_approval` are true. Dvandva must never create a PR.

## Development Install

Marketplace install is the public path. For local development against a checkout, symlink the plugin skill directories directly:

```bash
git clone https://github.com/axatbhardwaj/Dvandva.git
cd Dvandva

mkdir -p ~/.claude/skills ~/.agents/skills
rm -f \
  ~/.claude/skills/dvandva-vadi \
  ~/.claude/skills/dvandva-prativadi \
  ~/.agents/skills/dvandva-vadi \
  ~/.agents/skills/dvandva-prativadi

ln -sfn "$(pwd)/plugins/dvandva/skills/vadi"      ~/.claude/skills/vadi
ln -sfn "$(pwd)/plugins/dvandva/skills/prativadi" ~/.claude/skills/prativadi
ln -sfn "$(pwd)/plugins/dvandva/skills/vadi"      ~/.agents/skills/vadi
ln -sfn "$(pwd)/plugins/dvandva/skills/prativadi" ~/.agents/skills/prativadi
```

Old pre-plugin installs used `dvandva-vadi` and `dvandva-prativadi` symlinks. Remove those before re-linking; they point at deleted root `skills/` paths after the plugin migration.

## Validation

```bash
bash scripts/lint-skills.sh plugins/dvandva/skills/vadi/SKILL.md
bash scripts/lint-skills.sh plugins/dvandva/skills/prativadi/SKILL.md
bash scripts/test-dvandva-wait.sh
bash scripts/smoke-plugin-install.sh
claude plugin validate plugins/dvandva
claude plugin validate .
```

The smoke script builds a temp marketplace, validates the Claude plugin path,
adds and installs the marketplace in Codex with an isolated `CODEX_HOME`, checks
that Codex renders both Dvandva skills, runs both bundled wait helpers, and
checks standalone development copies.

## Release Checklist

1. Bump `.claude-plugin/marketplace.json`, `plugins/dvandva/.claude-plugin/plugin.json`, and `plugins/dvandva/.codex-plugin/plugin.json` together.
2. Run the validation commands above.
3. Run `codex plugin marketplace add <repo-or-path>` from an isolated `CODEX_HOME`, then verify app-server plugin install exposes both skills.
4. Run `claude plugin marketplace add <repo-or-path>` and `claude plugin install dvandva@dvandva` from an isolated `HOME`.
5. Tag the release, for example `v0.1.0`.
6. Push the branch and tag only after both Dvandva roles approve the final diff.

## Reading Order

1. `product.md` - product specification and acceptance criteria
2. `plugins/dvandva/references/local-baton-channel.md` - bundled baton protocol
3. `plugins/dvandva/references/state-transition-table.md` - bundled transition reference
4. `docs/case-studies/pr-353.md` - sanitized case study that motivated the design

## Non-Goals

- No daemon or process launcher in v0.1.0.
- No GitHub API integration.
- No PR creation.
- No npm-first distribution path.
