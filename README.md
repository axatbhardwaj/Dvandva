# Dvandva

Dvandva is a coordination protocol and protocol-level orchestrator for paired AI coding agents. There is no daemon, no launcher, and no hidden process that owns the control loop. Two independently running agent sessions follow a shared state machine through a local baton file: one role, `vadi`, proposes plans and implements phases; the other, `prativadi`, adversarially reviews, applies narrow fixups from a strict allowlist, and hands control back through the baton. Legacy runs use `.dvandva/baton.json`; named runs can use `.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in one worktree. `run_id` must be one safe path segment: letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`.

Because the protocol is just files and shell helpers, it needs zero infrastructure, is crash-tolerant by construction (all state lives on disk, so either session can be killed and rejoin at preflight), and is engine-agnostic. The canonical dogfood setup is Claude Code as vadi and Codex as prativadi — the cross-vendor pairing is the point: different models have systematically different blind spots, so the reviewer catches what the implementer cannot see. Either engine can host either role. Single-engine supervised runs are supported; full walkaway mode needs two persistent sessions.

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each turn: using skills before action, brainstorming before design, TDD before implementation, verification before completion, skill-writing discipline when skills change, and subagent-driven execution when parallel tracks are available. If the engine running a Dvandva role cannot see the Superpowers skills, that role must stop and surface setup instructions instead of continuing with a weakened workflow.

Dvandva ships as an installable plugin for both engines. The repo lives at https://github.com/axatbhardwaj/Dvandva.

## Quickstart

Install the marketplace in each engine you want to use:

```bash
claude plugin marketplace add axatbhardwaj/Dvandva
claude plugin install dvandva@dvandva

bash scripts/install-codex.sh
```

For Codex, `scripts/install-codex.sh` registers the marketplace and then runs
`codex plugin add dvandva@dvandva` non-interactively — no TUI navigation
required. Older Codex builds without `plugin add` fall back to the legacy
app-server RPC path. The script accepts an optional local-path argument for
development against a checkout:

```bash
bash scripts/install-codex.sh /path/to/your/Dvandva
```

See `docs/research/2026-05-16-codex-install.md` for the install-history note:
Codex `0.130.0` required app-server RPC, while current Codex exposes
`codex plugin add`.

After install, `/skills` should list `dvandva:vadi` and `dvandva:prativadi`,
and `/dvandva:vadi` / `/dvandva:prativadi` should appear as slash commands.

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

Codex slash commands:

```text
/dvandva:vadi
/dvandva:prativadi
```

Each command starts a walkaway run for that role by injecting the canonical `/goal` block from the corresponding skill. Codex auto-discovers the command files from `plugins/dvandva/commands/<role>.md` — no extra wiring needed.

The `$vadi` / `$prativadi` skill-fallback syntax still works for direct skill invocation when you don't want to start a `/goal` loop:

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
| Superpowers plugin on every engine running a Dvandva role, hard runtime dependency | `/skills` lists `superpowers:using-superpowers`, `superpowers:brainstorming`, `superpowers:test-driven-development`, and `superpowers:verification-before-completion` |
| Work happens on a feature branch | `git branch --show-current` is not `main` or `master` |
| `jq` installed | `jq --version` |
| inotify-tools, optional — instant wake on baton handoff instead of interval polling | `inotifywait --help` |

## Usage

In walkaway mode, the assigned-away session blocks in:

```bash
${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --file "$BATON_FILE" --interval 60 --max-wait 540
```

The active baton is selected in this order: `DVANDVA_BATON_FILE`, then `DVANDVA_RUN_DIR/baton.json`, then safe `DVANDVA_RUN_ID` mapped to `.dvandva/runs/<run_id>/baton.json`, then legacy `.dvandva/baton.json`. Set the same safe `DVANDVA_RUN_ID` in both sessions to run more than one Dvandva loop in one worktree.

That is shell waiting, not model polling. The agent resumes when the baton assigns its role again, or stops if the baton reaches `done`, `human_question`, or `human_decision`.

On Claude Code, invoke the helper with an explicit 600000 ms Bash-tool timeout; the 540 s default max-wait fits the 600 s tool maximum, and exit 20 is just a heartbeat to re-run unless interrupted. Codex-hosted sessions can use `--persist` so the shell keeps waiting across heartbeat intervals; `--persist-max <seconds>` adds a total wall-clock cap and exits 23 when reached. When `inotifywait` is installed the helper wakes the moment the baton changes instead of sleeping the full interval.

The prativadi can also be launched *before* the vadi has scaffolded the baton. Its preflight detects the missing baton, runs the wait helper with `--allow-missing`, and resumes once the vadi writes the file (or exits 20 after `--max-wait` if the vadi never appears). Simultaneous-launch dogfooding is therefore safe — no need to order the two starts.

For one-engine use, set `run_mode: "supervised"` in the active baton and invoke `vadi` and `prativadi` serially in that engine. Supervised mode exits on assigned-away states so one CLI session cannot deadlock itself. Setting `DVANDVA_NO_WAIT=1` in the prativadi's environment also opts out of the missing-baton wait so a serial-supervised user gets the original "no baton — vadi has not started" message immediately.

Agents should make regular local checkpoint commits after a verified logical slice when `allow_commit` is true and the dirty paths match the baton's `changed_paths` union. Checkpoint commits are local only: pushing waits until both `vadi_final_approval` and `prativadi_final_approval` are true and `allow_push` is true. Dvandva must never create a PR.

## History

Every baton write is installed by the bundled `dvandva-write.sh` helper (validated, atomic), which also snapshots to `<baton-dir>/history/<checkpoint>-<status>-<assignee>.json` via `dvandva-snapshot.sh`. Terminal writes (status `done`, `human_decision`, or `human_question`) additionally produce `baton.<sanitized-branch>-<checkpoint>-<status>.json` beside the active baton, so terminal records survive subsequent runs without manual archiving. Branch names containing `/` (e.g. `feature/foo`) are sanitized to `-` so the archive stays a single file.

The `.dvandva/` directory is gitignored. Inspect history with `ls <baton-dir>/history/` and `diff <baton-dir>/history/<a>.json <baton-dir>/history/<b>.json` to see how a baton evolved across handoffs.

## Development Install

Marketplace install is the public path. For local development against a checkout, install the checkout as a local Codex marketplace and, if needed, a local Claude marketplace:

```bash
git clone https://github.com/axatbhardwaj/Dvandva.git
cd Dvandva

# Codex
bash scripts/install-codex.sh "$(pwd)"

# Claude
claude plugin marketplace add "$(pwd)"
claude plugin install dvandva@dvandva
```

For direct skill-development work where you deliberately want live symlinks instead of plugin cache copies, link the skill directories directly:

```bash
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
bash scripts/lint-protocol-phase1.sh
bash scripts/lint-skill-phase3.sh
bash scripts/lint-phase4-research.sh
bash scripts/lint-artifacts.sh
bash scripts/test-lint-artifacts.sh
bash scripts/test-lint-skills.sh
for skill in vadi prativadi research testing understanding worktree-setup; do
  bash scripts/lint-skills.sh "plugins/dvandva/skills/$skill/SKILL.md"
done
bash scripts/test-dvandva-wait.sh
bash scripts/test-dvandva-write.sh
bash scripts/test-dvandva-snapshot.sh
bash scripts/test-install-codex.sh
bash scripts/smoke-plugin-install.sh
claude plugin validate plugins/dvandva
claude plugin validate .
```

The smoke script builds a temp marketplace, validates the Claude plugin path,
adds and installs the marketplace in Codex with `codex plugin add` under an
isolated `CODEX_HOME`, checks that Codex renders both Dvandva skills, runs both
bundled wait helpers, and checks standalone development copies.

## Release Checklist

1. Bump `.claude-plugin/marketplace.json`, `plugins/dvandva/.claude-plugin/plugin.json`, and `plugins/dvandva/.codex-plugin/plugin.json` together.
2. Run the validation commands above.
3. Run `codex plugin marketplace add <repo-or-path>` and `codex plugin add dvandva@dvandva` from an isolated `CODEX_HOME`, then verify `/skills` exposes both Dvandva skills.
4. Run `claude plugin marketplace add <repo-or-path>` and `claude plugin install dvandva@dvandva` from an isolated `HOME`.
5. Tag the release, for example `v0.1.0`.
6. Push the branch and tag only after both Dvandva roles approve the final diff.

## Reading Order

1. `product.md` - product specification and acceptance criteria
2. `plugins/dvandva/references/local-baton-channel.md` - bundled baton protocol
3. `plugins/dvandva/references/state-transition-table.md` - bundled transition reference
4. `docs/case-studies/pr-353.md` - sanitized case study that motivated the design

## Non-Goals

- No runtime daemon, hidden central process, or process launcher.
- No GitHub API integration.
- No PR creation.
- No npm-first distribution path.
