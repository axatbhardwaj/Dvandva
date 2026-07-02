# Dvandva

Dvandva is a coordination protocol and protocol-level orchestrator for paired AI coding agents. There is no daemon, no launcher, and no hidden process that owns the control loop. Two independently running agent sessions follow a shared state machine through a local baton file: one role, `vadi`, proposes plans and implements phases; the other, `prativadi`, adversarially reviews, applies narrow fixups from a strict allowlist, and hands control back through the baton. Legacy runs use `.dvandva/baton.json`; named runs can use `.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in one worktree. `run_id` must be one safe path segment: letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`.

Because the protocol is just files plus the `dvandva` binary, it needs zero infrastructure, is crash-tolerant by construction (all state lives on disk, so either session can be killed and rejoin at preflight), and is engine-agnostic. The canonical dogfood setup is Claude Code as vadi and Codex as prativadi — the cross-vendor pairing is the point: different models have systematically different blind spots, so the reviewer catches what the implementer cannot see. Either engine can host either role — but **Dvandva never runs solo**. Every run has two decorrelated roles, and the reviewer is never the engine that produced the work; that separation is the whole point. The two `run_mode`s differ only in handoff style, never in role count: `walkaway` is two autonomous sessions polling the baton, `supervised` is a human invoking each of the two roles in turn. The termination gate enforces it — no run reaches post-handshake `done` until both roles have independently approved, and every `*_final_approval` and `run_explainer_reviews` entry is `DVANDVA_ROLE`-bound, so one engine can never stand in for both.

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each turn: using skills before action, brainstorming before design, TDD before implementation, verification before completion, skill-writing discipline when skills change, and subagent-driven execution when parallel tracks are available. Dvandva uses conditional parallelism: parallelize only genuinely disjoint tracks, record actual work in `subagent_tracks`, and record what was not parallelized and why when a direct pass is safer. Codex subagent handles must be closed explicitly after their results are consumed, because completed agents can remain open and keep counting against the thread limit. If the engine running a Dvandva role cannot see the Superpowers skills, that role must stop and surface setup instructions instead of continuing with a weakened workflow.

Accepted v2 baton modes are `development`, `research`, and `review`.
`feature-pr` remains a legacy alias for `development` on older batons. Public
docs no longer treat `campaign` as the current mode enum.

Development runs also carry an orthogonal flow `profile`: `fast`, `standard`,
or `full`. `mode` answers what kind of run this is; `profile` answers how much
development lifecycle is required. New development scaffolds default to
`standard`, existing development batons with no profile are treated as effective
`full`, and hard-risk paths such as product specs, baton schemas, role skills,
helper scripts, protocol docs, hooks, top-level scripts, dependency
manifests, secret/env surfaces, external API clients, or artifact/history formats force
`profile_floor: "full"`. `fast` is only for
allowlisted prose-only changes with positive allowlist evidence. Profile
downgrades below `profile_floor` route to `human_decision`.

Dvandva model classes are vendor-neutral. Agent frontmatter uses `model: opus` and `model: sonnet` as class labels, not Anthropic-only product IDs. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Do not use `haiku` for Dvandva subagents.

Dvandva ships as an installable plugin for both engines. The repo lives at https://github.com/axatbhardwaj/Dvandva.

The `dvandva` binary IS the Dvandva runtime: read path, write path, waiting,
preflight, git work-gating, installers, and lints. It ships from this repo;
crates.io currently carries only the older `2.0.0-alpha.1` read-path
prerelease. Install it from a checkout with `cargo install --path
rust/dvandva` before installing the plugin; the plugin no longer bundles
executables.

## Quickstart

Install the `dvandva` binary, then the marketplace in both Claude Code and Codex:

```bash
cargo install --path rust/dvandva
# or, once 2.0.0-alpha.2 is published: cargo install dvandva --version 2.0.0-alpha.2
dvandva install
```

`dvandva install` registers the Dvandva marketplace and installs
`dvandva@dvandva` into both engines. It accepts an optional local-path argument
for development against a checkout:

```bash
dvandva install /path/to/your/Dvandva
```

Use `--claude-only` or `--codex-only` when you are installing just one engine.
For Codex, `dvandva install` delegates to `dvandva install-codex`, which runs
`codex plugin add dvandva@dvandva` non-interactively so no TUI navigation is
required. Older Codex builds without `plugin add` fall back to the legacy
app-server RPC path.

`dvandva install` is separate from installing the binary itself. `dvandva install`
adds the Dvandva skills, commands, agents, and references to Claude Code and/or
Codex; `cargo install --path rust/dvandva` (or, once published, `cargo install
dvandva --version 2.0.0-alpha.2`) installs only the `dvandva` binary. The binary
must be on `PATH` for the installed skills to run — the plugin no longer
bundles executables.

See `docs/research/2026-05-16-codex-install.md` for the install-history note:
Codex `0.130.0` required app-server RPC, while current Codex exposes
`codex plugin add`.

After install, `/skills` should list `dvandva:vadi`, `dvandva:prativadi`,
`dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and
`dvandva:worktree-setup`.
The role slash commands `/dvandva:vadi` and `/dvandva:prativadi` should also
appear.

Then start a feature-branch worktree and open both sessions:

```text
Claude: Implement <small feature> with Codex review. Start a Dvandva development run in walkaway mode.
Codex:  Join the same Dvandva development run and review the baton.
```

Other accepted v2 run-mode prompts:

```text
Claude: Research <topic> with Codex review. Start a Dvandva research run.
Claude: Review <diff or artifact> with Codex cross-checking. Start a Dvandva review run.
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

## Rust Crate

The `dvandva` binary IS the Dvandva runtime — the read path, the write path,
waiting, preflight, git work-gating, and the installers, all in one multicall
binary. Install it from a checkout of this repo (crates.io currently carries
only the older `2.0.0-alpha.1` read-path prerelease):

```bash
cargo install --path rust/dvandva
# or, once 2.0.0-alpha.2 is published: cargo install dvandva --version 2.0.0-alpha.2
```

The binary must be on `PATH` for the Dvandva skills to run. `cargo install`
installs only the binary; run `dvandva install` afterward to add the engine
plugins. Common subcommands:

```bash
dvandva state --compact --file <baton> --role <vadi|prativadi|team|human>
dvandva resolve --role <vadi|prativadi> --cwd <repo>
dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"
dvandva wait --role <vadi|prativadi> --until-actionable
```

Invoked through a git-hook symlink (`pre-commit`, `prepare-commit-msg`, ...) the
binary takes the hook name from `argv[0]`. `dvandva --version` prints the
version line (`dvandva 2.0.0-alpha.2`).

## Current State

Dvandva ships as one `dvandva` plugin with:

- `plugins/dvandva/skills/vadi/SKILL.md`
- `plugins/dvandva/skills/prativadi/SKILL.md`
- `plugins/dvandva/skills/research/SKILL.md`
- `plugins/dvandva/skills/testing/SKILL.md` — Dvandva-native test discipline invoked during `test_creation` and review sandbox steps; replaces standalone `testing` for Dvandva work.
- `plugins/dvandva/skills/understanding/SKILL.md` — mastery-gated teaching grounded in baton/diff/`research_ref`/`plan_ref`; exports an HTML checklist; replaces standalone `understanding` for Dvandva work.
- `plugins/dvandva/skills/worktree-setup/SKILL.md` — isolated-worktree prep with an optional DeFi profile; replaces standalone `worktree-setup` for Dvandva work.
- 15 canonical subagent roles forming the **seed roster** under `plugins/dvandva/agents/` (researcher, architect, pattern-mapper, implementer, test-creator, debugger, cross-reviewer, adversarial-analyst, deep-reviewer, security-auditor, integration-checker, doc-verifier, deslopper, sandbox-verifier, baton-auditor); model classes are vendor-neutral (`opus`/`sonnet`). Run 3 turns this seed roster into a foundation for run-scoped dynamic agent generation: parent roles generate additional named instances on demand; each is recorded in `agent_instances` on the baton with its identity, parent role, model/permission class, read/write paths, base checkpoint, lifecycle state, output refs, evidence refs, and close result. Generated agents observe single-writer merge (they never own baton `assignee`, phase transitions, or final approval), explicit closure (every handle must be closed before its track counts as complete), and dynamic write-path disjointness (write-path overlaps for generated instances sharing the same `base_checkpoint`, or for any two live (`planned`/`running`) instances regardless of base_checkpoint, are rejected unless sharing a `conflict_group` with explicit dependency serialization; closed instances from an earlier base_checkpoint are not part of the collision set). Generated instances are run-scoped and ephemeral; no roster sprawl occurs unless a later reviewed run promotes a pattern into the seed roster.
- Run 4 generalized path gates: `work_split` items now expose `write_paths` for write intent; bare `paths` remain backward-compatible write intent only for implementation and cross-fixing chunks. For write-capable chunks, `write_paths` supplements rather than narrows `paths`; the collision check uses their union so `write_paths: []` cannot mask a declared write surface. `cross_review` chunks are read-only unless they declare explicit `write_paths`. Live overlaps are rejected unless the chunks share a `conflict_group` and an explicit `depends_on` edge serializes the work. Terminal work_split chunks are historical and do not block later path reuse because work_split does not carry the generated-agent `base_checkpoint` wave model.
- Run 4 git work-gating: role preflight exports and asserts `DVANDVA_ROLE=<role>`, then runs the hook stage via `dvandva preflight --role <role>`; the hook stage records the prior `core.hooksPath` as `dvandva.priorHooksPath`, sets `core.hooksPath` to `.dvandva/githooks` (delegating hooks that are symlinks to the `dvandva` binary and exec the prior hook chain), and records `dvandva.hooksAdoptedAt` as the local drift-lint baseline; on uninstall the prior `core.hooksPath` is restored from `dvandva.priorHooksPath`. The pre-commit hook runs the in-binary `dvandva commit-gate`, which requires `DVANDVA_ROLE` to match baton ownership or `active_roles`; the prepare-commit-msg hook stamps `Dvandva-Checkpoint`; and `dvandva drift-lint` checks for off-protocol commits from the hook-adoption baseline floor, so a later stamped checkpoint cannot hide an unstamped bypass. This is local git-hook enforcement, not a daemon or hidden central process.
  Terminal `done`, `human_question`, and `human_decision` batons are inactive for this git gate: commits are not blocked by terminal batons, and drift lint only reports off-protocol commits while a non-terminal baton is active or checkpoint history gives it a scan floor.
- Run 4 Dvandva-only retirement: `dvandva retire-agents` can retire only the five Claude Code symlink agents whose Dvandva-covered workflows are replaced by the seed roster: `adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and `sandbox-executor`. Functional parity is based on equivalent-or-better empirical usage across Runs 1-4, plus 1.1.0 cache/roster parity and reversibility. Codex agent-axis retirement is a no-op; skills are out of scope and no skill files are touched. The helper is dry-run first, writes a backup manifest, and supports restore.
- the `dvandva` binary as the protocol runtime (`state`, `resolve`, `write`, `wait`, `snapshot`, `preflight`, the git work-gate, installers, and lints); the plugin bundles no executables
- plugin-local protocol references in `plugins/dvandva/references/`
- Codex marketplace metadata in `.agents/plugins/marketplace.json`
- root marketplace metadata in `.claude-plugin/marketplace.json`

The default `run_mode` is `walkaway`: start both sessions once, then let the baton decide which role works next.

## Prerequisites

| Prerequisite | Verify |
|---|---|
| `dvandva` binary on `PATH`, hard runtime dependency | `dvandva --version` (install with `cargo install --path rust/dvandva`, or `cargo install dvandva --version 2.0.0-alpha.2` once published) |
| Claude Code, if using Claude | `claude --version` |
| Codex CLI, if using Codex | `codex --version` |
| Superpowers plugin on every engine running a Dvandva role, hard runtime dependency | `/skills` lists `superpowers:using-superpowers`, `superpowers:brainstorming`, `superpowers:test-driven-development`, and `superpowers:verification-before-completion` |
| Work happens on a feature branch | `git branch --show-current` is not `main` or `master` |
| none extra for instant wake — the binary's file watcher is built in, with interval polling as fallback | `dvandva wait --role vadi --allow-missing --finite --max-wait 1` |

## Usage

In walkaway mode, the assigned-away session blocks in:

```bash
dvandva wait --role <vadi|prativadi> --file "$BATON_FILE" --interval 60 --max-wait 540 --until-actionable
```

The active baton is selected in this order: `DVANDVA_BATON_FILE`, then `DVANDVA_RUN_DIR/baton.json`, then safe `DVANDVA_RUN_ID` mapped to `.dvandva/runs/<run_id>/baton.json`, then Existing baton discovery over `.dvandva/runs/*/baton.json` and legacy `.dvandva/baton.json`. If an active baton exists and the prompt does not choose one, the vadi asks whether to continue or start a new named run. If only terminal batons exist, the vadi auto-creates a new named run instead of overwriting old state. Set the same safe `DVANDVA_RUN_ID` in both sessions to run more than one Dvandva loop in one worktree.

That is foreground waiting, not model polling. The agent resumes when the baton assigns its role again, or stops for completion only when the baton reaches post-handshake `done`. `human_question` and `human_decision` are human-intervention pauses, not completion.

Continuous polling is the default hard rule: `--max-wait` is a heartbeat interval, not permission to stop. The helper keeps polling until the baton assigns the role, reaches post-handshake `done`, enters `human_question` / `human_decision`, or the user interrupts. Use `--until-actionable` for normal walkaway waits so team-owned `active_roles` states do not wake a role until that role has dependency-unblocked actionable work; after a handoff write, combine it with `--since-checkpoint <written_checkpoint>`. `termination_review` is the shared multipart termination state: it keeps both roles active so they either keep polling or stop together after both approve. Final approval and development explainer-review ownership are helper-enforced: `DVANDVA_ROLE=vadi` may raise only `vadi_final_approval` and may add/change only `run_explainer_reviews` entries with `role: "vadi"`; `DVANDVA_ROLE=prativadi` may raise only `prativadi_final_approval` and may add/change only entries with `role: "prativadi"`. `--persist` is accepted for older call sites and is now redundant. `--persist-max <seconds>` adds a total wall-clock cap and exits 23 when reached; in walkaway mode that cap is a shell-budget heartbeat, so the role must immediately re-enter the wait unless the user interrupts. `--finite` is compatibility-only and is not valid for normal walkaway loops. The helper's built-in directory watcher wakes it the moment the baton changes instead of sleeping the full interval; if the watcher cannot start, it falls back to interval polling.

The prativadi can also be launched *before* the vadi has scaffolded the baton. Its preflight detects the missing baton, runs the wait helper with `--allow-missing`, and resumes once the vadi writes the file. Simultaneous-launch dogfooding is therefore safe — no need to order the two starts.

For one-engine use, set `run_mode: "supervised"` in the active baton and invoke `vadi` and `prativadi` serially in that engine. Supervised mode exits on assigned-away states so one CLI session cannot deadlock itself. Setting `DVANDVA_NO_WAIT=1` in the prativadi's environment also opts out of the missing-baton wait so a serial-supervised user gets the original "no baton — vadi has not started" message immediately.

Agents should make regular local checkpoint commits after a verified logical slice when `allow_commit` is true and the dirty paths match the baton's `changed_paths` union. Checkpoint commits are local only: pushing waits until the final `termination_review` handoff has completed, both `vadi_final_approval` and `prativadi_final_approval` are true on the installed baton, and `allow_push` is true. Dvandva must never create a PR.

Run 4 role preflight enforces git work-gating automatically in each clone via
the single turn-entry gate `dvandva preflight --role <role>`, invoked by the role
skill at turn entry. Invoked directly it is:

```bash
export DVANDVA_ROLE=vadi      # or: export DVANDVA_ROLE=prativadi
dvandva preflight --role vadi
```

The preflight asserts `DVANDVA_ROLE=<role>` then runs the hook stage in-process.
The hook stage records the prior `core.hooksPath` as `dvandva.priorHooksPath`,
sets `core.hooksPath` to `.dvandva/githooks` (delegating hooks that are symlinks
to the `dvandva` binary), and execs the prior hook chain on every commit so any
existing hooks configuration keeps firing. On uninstall, the prior
`core.hooksPath` is restored from `dvandva.priorHooksPath`. The hook stage also
records the current `HEAD` as `dvandva.hooksAdoptedAt` as the local drift-lint
baseline. While a Dvandva baton is active, commits require `DVANDVA_ROLE=vadi` or
`DVANDVA_ROLE=prativadi`; the in-binary `dvandva commit-gate` (dispatched by the
`pre-commit` symlink) allows the commit only when that role owns the baton turn
or appears in `active_roles`. The `prepare-commit-msg` symlink appends
`Dvandva-Checkpoint: <N>` so `dvandva drift-lint --warn` can report off-protocol
commits. Drift lint floors the scan at `dvandva.hooksAdoptedAt` when present, so
`checkpoint -> --no-verify -> checkpoint` sandwiches stay visible while
pre-adoption history is ignored. The gate is local git-hook enforcement, not a
daemon or hidden central process.
Git `commit --no-verify` can bypass the pre-commit gate; if the role environment
is also unset, no `Dvandva-Checkpoint` trailer is stamped. Treat drift lint as
the backstop for that explicit bypass. While a baton is active, drift lint also
reports unstamped commits when no earlier checkpoint baseline exists, so a first
bypass commit is still visible.

Before post-handshake terminal `done`, a v2 run must satisfy the
mode/profile-conditional terminal artifact gate. Full-profile development runs
write one-date run explainer HTML under `./superpowers/run-reports/`, set
`run_explainer_ref`, and require both roles to record completed approved
entries in `run_explainer_reviews` for that exact artifact; those entries are
role-owned, and `DVANDVA_ROLE` must match the entry role for additions or
changes. Use `YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or
`<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`;
never add a second date prefix. Fast and standard development runs skip the
run-explainer gate but still require `profile_decision`, passing final
verification, completed `verification_matrix` evidence, completed approved
prativadi `phase-review` evidence with current-cycle `review_checkpoint`,
shared `termination_review`, and both role-owned final approvals. Research runs
require `research_ref` and additionally `plan_ref` iff `research_outcome ==
seed_development`; review runs require `review_ref`.

## History

Every baton write is installed by `dvandva write` (validated, atomic), which also snapshots to `<baton-dir>/history/<checkpoint>-<status>-<assignee>.json` via `dvandva snapshot`. Terminal writes (status `done`, `human_decision`, or `human_question`) additionally produce `baton.<sanitized-branch>-<checkpoint>-<status>.json` beside the active baton, so terminal records survive subsequent runs without manual archiving. Branch names containing `/` (e.g. `feature/foo`) are sanitized to `-` so the archive stays a single file.

The `.dvandva/` directory is gitignored. Inspect history with `ls <baton-dir>/history/` and `diff <baton-dir>/history/<a>.json <baton-dir>/history/<b>.json` to see how a baton evolved across handoffs.

## Development Install

Marketplace install is the public path. For local development against a checkout, install the checkout as a local marketplace in both engines:

```bash
git clone https://github.com/axatbhardwaj/Dvandva.git
cd Dvandva

dvandva install "$(pwd)"
```

For engine-specific development, pass `--claude-only` or `--codex-only`.

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

Rust definition-of-done gate (run in `rust/`):

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Doc and artifact lints, drift lint, the install smoke test, and plugin validation:

```bash
dvandva lint protocol-phase1
dvandva lint skill-phase3
dvandva lint phase4-research
dvandva lint artifacts
dvandva lint artifacts superpowers/plans
dvandva lint artifacts superpowers/research
dvandva lint run3-dynamic-agents
dvandva lint run4-path-gates
dvandva lint run4-standalone-agents
for skill in vadi prativadi research testing understanding worktree-setup; do
  dvandva lint skills "plugins/dvandva/skills/$skill/SKILL.md"
done
dvandva drift-lint --warn
dvandva smoke-install
claude plugin validate plugins/dvandva
claude plugin validate .
```

The `dvandva smoke-install` step builds a temp marketplace, validates the Claude
plugin path, adds and installs the marketplace in Codex with `codex plugin add`
under an isolated `CODEX_HOME`, runs the dual Claude/Codex installer and
Codex-only helper under isolated homes, checks that Codex renders all six Dvandva skills,
checks the installed cache version, and checks exact 15-agent roster parity in the
installed development copies.

## Release Checklist

1. Bump `.claude-plugin/marketplace.json`, `plugins/dvandva/.claude-plugin/plugin.json`, and `plugins/dvandva/.codex-plugin/plugin.json` together.
2. Run the validation commands above.
3. Run `dvandva install <repo-or-path>` from isolated `HOME` and `CODEX_HOME`, then verify `/skills` exposes `dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup` in the installed engines.
4. If testing engine-specific fallback paths, run `dvandva install-codex <repo-or-path>` from an isolated `CODEX_HOME` and `HOME`.
5. Tag the release, for example `vX.Y.Z`.
6. Push the branch and tag only after both Dvandva roles approve the final diff.

## Reading Order

1. `product.md` - product specification and acceptance criteria
2. `plugins/dvandva/references/local-baton-channel.md` - bundled baton protocol
3. `plugins/dvandva/references/state-transition-table.md` - bundled transition reference
4. `docs/case-studies/pr-353.md` - sanitized case study that motivated the design

## Roadmap

- **Run 3** — super-parallel dynamic agent generation: the static 15-agent seed roster expands on demand via run-scoped `agent_instances`; single-writer merge ensures generated agents never own baton transitions; dynamic write-path disjointness rejects write collisions among generated instances sharing the same `base_checkpoint` or among any two live (`planned`/`running`) instances regardless of base_checkpoint; explicit closure of every generated handle is required before its track counts as complete. No daemon, no hidden orchestrator — the baton and foreground wait helper remain the only coordination channel.
- **Run 4** — generalized `work_split` path-gate enforcement + repo-local git work-gating + Dvandva-only standalone-agent retirement. The retirement scope is intentionally narrow: only Dvandva-covered workflows with functional parity via Runs 1-4 usage are eligible, Codex agent-axis cleanup is a no-op, and skill directories are never touched.

## Non-Goals

- No runtime daemon, hidden central process, or process launcher.
- No GitHub API integration.
- No PR creation.
- No npm-first distribution path.
