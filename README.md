# Dvandva

Dvandva is **a governed-loop protocol for adversarial AI pairs** — orchestration for paired AI coding agents without an orchestrator. There is no daemon, no launcher, and no hidden process that owns the control loop — two independently running agent sessions follow a shared state machine through a local baton file. One role, `vadi`, proposes plans and implements phases; the other, `prativadi`, adversarially reviews, applies narrow fixups from a strict allowlist, and hands control back through the baton. Because the protocol is just files plus the `dvandva` binary, it needs zero infrastructure and is crash-tolerant by construction: all state lives on disk, so either session can be killed and rejoin at preflight. The canonical dogfood pairing is Claude Code as `vadi` and Codex as `prativadi` — the cross-vendor split is the point, because different models have systematically different blind spots.

**At a glance**

- **Never solo** — every run has two decorrelated roles, and the reviewer is never the engine that produced the work.
- **Governed loops** — every review→fix cycle is a typed, capped state-machine edge; the validator mandates counter increments, and a loop that hits its cap routes to the human instead of spinning.
- **Walkaway or supervised** — two autonomous sessions polling the baton, or one human invoking each role in turn.
- **3 modes × 3 profiles** — `development` / `research` / `review` runs, each development run tuned `fast` / `standard` / `full`.
- **Evidence-gated `done`** — no run reaches post-handshake `done` until both roles independently approve with recorded verification evidence.
- **Crash-tolerant** — state is on disk; kill a session and it rejoins at preflight.

**Superpowers is a hard runtime dependency.** Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each turn — skills before action, brainstorming before design, TDD before implementation, verification before completion, and subagent-driven execution when parallel tracks exist. If the engine running a Dvandva role cannot see the Superpowers skills, that role must stop and surface setup instructions instead of continuing with a weakened workflow.

Dvandva ships as an installable plugin (version `1.5.2`) for both engines. The repo lives at https://github.com/axatbhardwaj/Dvandva.

## Quickstart

The `dvandva` binary IS the Dvandva runtime — the read path, the write path, waiting, preflight, git work-gating, the installers, and the lints, all in one multicall binary. It is published on crates.io as `dvandva 3.2.0`.

**1. Install the binary.** From crates.io, or from a checkout:

```bash
cargo install dvandva --version 3.2.0
# or, from a checkout: cargo install --path rust/dvandva
```

The binary must be on `PATH` for the installed skills to run — the plugin bundles no executables.

**2. Install the plugin into both engines.**

```bash
dvandva install
```

`dvandva install` registers the Dvandva marketplace and installs `dvandva@dvandva` into Claude Code and Codex; it adds the skills, commands, agents, and references, and is separate from installing the binary. Use `--claude-only` or `--codex-only` for a single engine, and pass a local path to develop against a checkout:

```bash
dvandva install /path/to/your/Dvandva
```

For Codex, `dvandva install` delegates to `dvandva install-codex`, which runs `codex plugin add dvandva@dvandva` non-interactively so no TUI navigation is required; older Codex builds without `plugin add` fall back to the legacy app-server RPC path. See `docs/research/2026-05-16-codex-install.md` for that history.

**3. Verify.** After install, `/skills` should list `dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup`, and the slash commands `/dvandva:vadi` and `/dvandva:prativadi` should appear.

**4. Start a run.** On a feature-branch worktree, open both sessions:

```text
Claude: Implement <small feature> with Codex review. Start a Dvandva development run in walkaway mode.
Codex:  Join the same Dvandva development run and review the baton.
```

Research and review runs use the same two-session shape:

```text
Claude: Research <topic> with Codex review. Start a Dvandva research run.
Claude: Review <diff or artifact> with Codex cross-checking. Start a Dvandva review run.
```

The `/dvandva:vadi` and `/dvandva:prativadi` slash commands start a walkaway run for that role by injecting the canonical `/goal` block from the corresponding skill (Codex auto-discovers them from `plugins/dvandva/commands/<role>.md`). The `$vadi` / `$prativadi` fallback invokes the skill directly when you do not want to start a `/goal` loop.

**Upgrading.** `dvandva upgrade` refreshes the whole stack — binary + both engine plugin caches — as one all-or-nothing transaction: it lands fully on the new version or fully restores the prior one, never a partial mix. Exit `0` means committed and verified, `20` means it failed with the prior state intact (rolled back, or nothing had been changed yet), and `21` means rollback itself was incomplete (a precise residual report prints in that case). An advisory lock at `~/.dvandva/upgrade.lock` blocks concurrent upgrades, and a crash mid-upgrade leaves a breadcrumb; the next run detects it, restores the prior state, and exits (code `20`) so you can re-run the upgrade cleanly.

### Prerequisites

| Prerequisite | Verify |
|---|---|
| `dvandva` binary on `PATH` (hard runtime dependency) | `dvandva --version` |
| Claude Code and/or Codex CLI | `claude --version` / `codex --version` |
| Superpowers on every engine running a role (hard runtime dependency) | `/skills` lists `superpowers:using-superpowers`, `superpowers:brainstorming`, `superpowers:test-driven-development`, `superpowers:verification-before-completion` |
| Work happens on a feature branch | `git branch --show-current` is not `main` or `master` |

## How it works

The baton is a single JSON file the two roles pass back and forth. Each turn a role runs `dvandva preflight --role <role>` (which asserts `DVANDVA_ROLE` and adopts the git hooks), reads the baton with `dvandva state` / `dvandva resolve`, does its bounded work, scaffolds the next baton with `dvandva next`, installs it atomically with `dvandva write` (which snapshots history), and blocks in `dvandva wait --until-actionable` until the baton assigns its role again. The assigned-away session is not model-polling — it is foreground-blocked in the wait helper, which wakes the instant the baton changes.

The full state machine, the 26-status catalog, and every legal transition live in `plugins/dvandva/references/state-transition-table.md`, with the acceptance spec in `product.md`. `done` and `abandoned` are the two terminal states; `abandoned` is a human bailout enterable only from `human_question` / `human_decision`, and `dvandva wait` exits 13 on it.

The v1 and v2 baton schemas are **retired on the write path**: a `dvandva.baton.v1` write candidate (or current baton still on v1) is rejected with `schema_retired` plus a migration hint to `dvandva.baton.v2`; a `dvandva.baton.v2` write candidate is then rejected with the live migration hint to `dvandva.baton.v3`. The read path (`state` / `resolve` / `wait` / `brief`) stays lenient so old v1/v2 batons remain observable and resumable-for-read.

Legacy runs use `.dvandva/baton.json`; named runs use `.dvandva/runs/<run_id>/baton.json` so multiple runs coexist in one worktree. `run_id` must be one safe path segment — letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`. The active baton is selected in order: `DVANDVA_BATON_FILE`, then `DVANDVA_RUN_DIR/baton.json`, then a safe `DVANDVA_RUN_ID` mapped to `.dvandva/runs/<run_id>/baton.json`, then discovery over `.dvandva/runs/*/baton.json` and legacy `.dvandva/baton.json`. If an active baton exists and the prompt does not choose one, the vadi asks whether to continue or start a new named run; if only terminal batons exist, it auto-creates a new named run instead of overwriting old state. Set the same safe `DVANDVA_RUN_ID` in both sessions to run more than one loop in one worktree. The prativadi can launch *before* the vadi has scaffolded the baton: its preflight detects the missing file, waits with `--allow-missing`, and resumes when the vadi writes it (set `DVANDVA_NO_WAIT=1` to opt out for serial-supervised use).

## Modes & profiles

`mode` answers what kind of run this is; `profile` answers how much development lifecycle is required.

| Mode | Purpose | Terminal evidence |
|---|---|---|
| `development` | plan and implement a change | passing verification, completed `verification_matrix`, approved `phase-review` |
| `research` | produce shared research | `research_ref` (plus `plan_ref` iff `research_outcome == seed_development`) |
| `review` | cross-check a diff or artifact | `review_ref` |

`feature-pr` remains a legacy alias for `development` on older batons; `campaign` is no longer a current mode enum.

Development runs also carry an orthogonal flow `profile`:

| Profile | Lifecycle |
|---|---|
| `fast` | allowlisted prose-only changes with positive allowlist evidence |
| `standard` | default for new development scaffolds — the compact `implementing → phase_review → termination_review → done` path |
| `full` | the eight-segment lifecycle (research, planning, implementation, test_creation, cross_review, deep_review, deslop, advancement); default for existing profile-less batons |

Hard-risk paths — product specs, baton schemas, role skills, helper scripts, protocol docs, hooks, top-level scripts, dependency manifests, secret/env surfaces, external API clients, or artifact/history formats — force `profile_floor: "full"`. Profile downgrades below `profile_floor` route to `human_decision`.

**Dvandva model classes are vendor-neutral workload-routing labels.** Agent frontmatter uses `model: opus` and `model: sonnet` as durable class labels, not Anthropic-only product IDs or a ranked model table. Claude Code maps `opus` to Opus-class, `sonnet` to Sonnet-class, `fable` to Fable-class, and `gpt` to a Sonnet-class wrapper that shells to Codex where available. Codex maps `opus` and `fable` to `gpt-5.5` with `xhigh` reasoning and `sonnet` and `gpt` to `gpt-5.5` with `high` reasoning. Codex should request `xhigh` reasoning effort for opus-class and fable-class work and `high` reasoning effort for sonnet-class and gpt-class work where the active surface exposes it. Use `opus` for architecture, planning, deep review, adversarial/security/integration/doc-verification, and baton-audit work. Use `sonnet` for bounded implementation, documentation, research, verification, routine cross-review, debugging, test creation, sandbox probes, and deslop. Do not use `haiku` for Dvandva subagents.

## The human rail

Walkaway is autonomous, but three baton states are human-intervention pauses, not completion: `human_question`, `human_decision`, and the `abandoned` bailout reachable from them. `human_question` and `human_decision` pause the loop; on them a role stops working only to surface, never to quit.

**The native Claude Code remote session is the human notification channel, reachable from mobile.** The Claude Code-hosted session owns surfacing human_question and human_decision to the human: whichever role Claude Code hosts asks the human in-session on writing a pause or on a wait exit 11/12, while the Codex-hosted role stops silently unless it is the only session; if no Claude session is in the run, the writer surfaces.

**Zero-touch resumption.** Add `--through-human` to a wait so that, instead of exiting 11/12 on a pause, it keeps polling through it — noting once per pause episode (deduped across a shell-budget re-invocation via a marker file beside the baton, with the stall watchdog suspended) and resuming normal semantics the moment the pause clears. Per the surfacing rule, only the non-surfacing session ever passes this flag; the Claude Code-hosted session still exits 11/12 to ask the human.

**Never silent.** A walkaway session never ends its turn mid-run without one of: a baton write, an active wait, or a surfaced human_decision.

**Termination is shared.** `termination_review` is the multipart termination state: it keeps both roles active so they either keep polling or stop together after both approve. No run reaches post-handshake `done` until both `vadi_final_approval` and `prativadi_final_approval` are true. Approval and explainer-review ownership are helper-enforced — `DVANDVA_ROLE=vadi` may raise only `vadi_final_approval` and add or change only `run_explainer_reviews` entries with `role: "vadi"`, and symmetrically for the prativadi.

Disputes run through bounded findings→fixing loops (`deep_review → phase_fixing`, `cross_review → cross_fixing`, `phase_review → phase_fixing`) capped by `loop_counts` at `disagreement_cap`. The `review_of_review` / `counter_review` vadi-counter loop is a retained but rarely-exercised safety valve — it has never fired across the ~24 recorded runs.

## The runtime

The `dvandva` binary is one multicall runtime. Invoked through a git-hook symlink (`pre-commit`, `prepare-commit-msg`, ...) it takes the hook name from `argv[0]`; `dvandva --version` prints the version line (`dvandva 3.2.0`).

| Group | Subcommands |
|---|---|
| Read | `state --compact --file <baton> --role <r>`, `resolve --role <r> --cwd <repo>`, `brief --role <r> --file <baton>` |
| Transitions | `next --file <baton> [--role <r>]` (list legal transitions), `next … --to <status> --summary <t> --next-action <t> [--out <f>]` (scaffold + validate a candidate) |
| Write | `write "$BATON_FILE" "$BATON_NEXT_FILE"` (validated, atomic; snapshots history) |
| Wait | `wait --role <r> --until-actionable [--interval N --max-wait N --stall-max N --through-human]` |
| Preflight & gates | `preflight --role <role>`, `commit-gate`, `drift-lint --warn`, `baton-guard` (PreToolUse hook) |
| Install | `install [path]`, `install-codex [path]`, `retire-agents`, `smoke-install` |
| Lints | `lint <check>`, `watchdog <roots…>` |

- `dvandva next` lists the legal transitions from the current baton and, in generate mode, scaffolds a `baton.next.json` candidate that it validates through the full write pipeline before emitting (it never writes the baton itself) — run it before `dvandva write` instead of hand-building ~44-key candidates.
- `dvandva brief --role <r>` prints a baton-native fresh-context pack (run header and effective profile, artifact refs, this role's current-phase work, open findings, verification matrix, the last five history entries, and `next_action`) for late phases running on degraded context.
- `dvandva baton-guard` is a Claude Code PreToolUse hook, registered by `plugins/dvandva/hooks/hooks.json`. It still blocks direct edits of `baton.json` under `.dvandva/` or anything under `.dvandva/**/history/` (exit 2), but the baton-creation SLA is warn-only: vadi `resolve`/`preflight` `CREATE` arms `.dvandva/.session-baton-pending.vadi`, `DVANDVA_BATON_SLA_SECONDS` sets the threshold (default 120s), and after the deadline the hook injects a model-visible warning while allowing the tool call. Codex has no hook surface, so Codex sees the SLA through the `DVANDVA_SLA armed ...` deadline line printed by resolve/preflight rather than through hook context. The guard fails open (exit 0) on unparseable stdin so a guard bug never bricks unrelated tools.

Subcommands use exit codes to signal baton state (for example `wait` exits 11/12 on human pauses, 13 on `abandoned`, and 23 on a persist cap); `product.md` and the role skills carry the full convention.

## Operating headless (VPS)

In walkaway mode the assigned-away session blocks in:

```bash
dvandva wait --role <vadi|prativadi> --file "$BATON_FILE" --interval 60 --max-wait 540 --until-actionable
```

`--max-wait` is a heartbeat interval, not permission to stop. Continuous polling is the default hard rule: the helper keeps polling until the baton assigns the role, reaches post-handshake `done`, enters `human_question` / `human_decision`, or the user interrupts. Use `--until-actionable` so team-owned `active_roles` states do not wake a role until it has dependency-unblocked actionable work; after a handoff write, combine it with `--since-checkpoint <written_checkpoint>`. A built-in directory watcher wakes the helper the moment the baton changes, falling back to interval polling if it cannot start. `--persist` is accepted for older call sites and is now redundant; `--persist-max <seconds>` adds a total wall-clock cap (exit 23) that in walkaway mode is a shell-budget heartbeat, so the role must immediately re-enter the wait unless the user interrupts. `--finite` is compatibility-only and is not valid for normal walkaway loops.

**Stall watchdog.** `--stall-max` arms the in-protocol dead-peer watchdog that fires when the other role goes silent mid-work.

**Out-of-band liveness monitor.** `dvandva watchdog` covers the case the in-protocol watchdog cannot — both roles' sessions dying at once (VPS reboot, OOM sweep, network loss) with nothing alive to write `human_decision`. Run it from cron or systemd, not from inside a session:

```bash
*/10 * * * * dvandva watchdog /srv/<repos...> >> /var/log/dvandva-watchdog.log 2>&1
```

It scans every baton under the given roots (default: git toplevel of cwd, else cwd) and prints one `DVANDVA_WATCHDOG watchdog_stale` line per mid-work baton unmoved past `--stale-max` (default 1800s), one `DVANDVA_WATCHDOG watchdog_paused` line per `human_question` / `human_decision` baton unmoved past `--remind-paused` (default 0 = off), plus a `DVANDVA_WATCHDOG summary` line at the end. It is a stateless scanner — findings print on every scan that finds them, with no dedup or pacing; cron logs are the record. Garbage or unreadable baton files are skipped and counted, never crash the scan. It always exits 0 — a monitor, not a gate. The human channel is still the Claude Code remote session, not this monitor.

**Commit gate.** Role preflight enforces git work-gating in each clone via the single turn-entry gate `dvandva preflight --role <role>`. The hook stage records the prior `core.hooksPath` as `dvandva.priorHooksPath`, sets `core.hooksPath` to `.dvandva/githooks` (delegating hooks that are symlinks to the `dvandva` binary and exec the prior hook chain), and records `dvandva.hooksAdoptedAt` as the drift-lint baseline; on uninstall the prior path is restored. While a baton is active, commits require `DVANDVA_ROLE=vadi` or `DVANDVA_ROLE=prativadi`: the in-binary `dvandva commit-gate` (via the `pre-commit` symlink) allows a commit only when that role owns the baton turn or appears in `active_roles`, the `prepare-commit-msg` symlink appends `Dvandva-Checkpoint: <N>`, and `dvandva drift-lint --warn` reports off-protocol commits from the adoption baseline floor so a stamped checkpoint cannot hide an unstamped bypass. `git commit --no-verify` can bypass the pre-commit gate (and stamps no trailer if the role is also unset); treat drift lint as the backstop. Terminal `done` / `human_question` / `human_decision` batons are inactive for this gate. This is local git-hook enforcement, not a daemon or hidden central process.

The commit gate also crosschecks staged paths: a commit whose staged paths fall outside `changed_paths ∪ role-visible work_split paths/write_paths` is blocked (`.dvandva/` and `superpowers/` are always exempt). Set `DVANDVA_COMMIT_GATE_PATHS=warn` to print offenders without blocking, or `=off` to skip the crosscheck; a baton that declares no scope at all is exempt (fail-open), while one that declares an empty scope blocks all non-exempt staged paths.

**Checkpoints and push.** Agents make regular local checkpoint commits after a verified logical slice when `allow_commit` is true and the dirty paths match the baton's `changed_paths` union. Commits are local only: pushing waits until the final `termination_review` handoff has completed, both `vadi_final_approval` and `prativadi_final_approval` are true, and `allow_push` is true. **Dvandva never creates a PR.**

**Terminal `done` gate.** Before post-handshake `done`, a run must satisfy the mode/profile-conditional terminal artifact gate. Full-profile development runs write a one-date run explainer HTML under `./superpowers/run-reports/`, set `run_explainer_ref`, and require both roles to record completed approved `run_explainer_reviews` for that exact artifact (role-owned; `DVANDVA_ROLE` must match the entry role). Use `YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or `<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`; never add a second date prefix. Fast and standard runs skip the explainer gate but still require `profile_decision`, passing final verification, a completed `verification_matrix`, approved prativadi `phase-review` evidence with a current-cycle `review_checkpoint`, a shared `termination_review`, and both role-owned final approvals.

## Development

Rust definition-of-done gate (run in `rust/`):

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

The full suite covers the read path, write path, wait, preflight, git work-gating, installers, and every lint. Run the doc/artifact/schema lints, drift lint, the install smoke test, and plugin validation with:

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
dvandva lint schema-parity
dvandva lint stale-version-ref
for skill in vadi prativadi research testing understanding worktree-setup; do
  dvandva lint skills "plugins/dvandva/skills/$skill/SKILL.md"
done
dvandva drift-lint --warn
dvandva smoke-install
claude plugin validate plugins/dvandva
claude plugin validate .
```

`dvandva lint schema-parity` keeps the status catalog, the required-key list, the two byte-identical channel-doc copies, and the HISTORICAL v1 references in parity. `dvandva lint stale-version-ref` checks user-facing version references (READMEs, SKILL install hints, plugin manifests) against the Cargo.toml crate version and the shared plugin version, fail-closed, with tests/fixtures, product/agent config (`product.md`, `CLAUDE.md`), and dated planning artifacts under `superpowers/` allowlisted. `dvandva smoke-install` builds a temp marketplace, validates the Claude plugin path, adds and installs the marketplace in Codex under an isolated `CODEX_HOME`, runs the dual Claude/Codex installer and Codex-only helper under isolated homes, checks that Codex renders all six Dvandva skills, checks the installed cache version, and checks exact 15-agent roster parity in the installed copies.

For direct skill-development work where you deliberately want live symlinks instead of plugin-cache copies, link the skill directories directly:

```bash
mkdir -p ~/.claude/skills ~/.agents/skills
rm -f ~/.claude/skills/dvandva-vadi ~/.claude/skills/dvandva-prativadi \
      ~/.agents/skills/dvandva-vadi ~/.agents/skills/dvandva-prativadi
ln -sfn "$(pwd)/plugins/dvandva/skills/vadi"      ~/.claude/skills/vadi
ln -sfn "$(pwd)/plugins/dvandva/skills/prativadi" ~/.claude/skills/prativadi
ln -sfn "$(pwd)/plugins/dvandva/skills/vadi"      ~/.agents/skills/vadi
ln -sfn "$(pwd)/plugins/dvandva/skills/prativadi" ~/.agents/skills/prativadi
```

Old pre-plugin installs used `dvandva-vadi` and `dvandva-prativadi` symlinks pointing at deleted root `skills/` paths after the plugin migration — remove those before re-linking. Codex contributors read `AGENTS.md` for the routing Claude Code gets from the slash commands.

**Release checklist**

1. Bump `.claude-plugin/marketplace.json`, `plugins/dvandva/.claude-plugin/plugin.json`, and `plugins/dvandva/.codex-plugin/plugin.json` together.
2. Run the validation commands above.
3. Run `dvandva install <repo-or-path>` from an isolated `HOME` and `CODEX_HOME`, then verify `/skills` exposes all six Dvandva skills in the installed engines. To test the Codex-only fallback, run `dvandva install-codex <repo-or-path>` from an isolated `CODEX_HOME` and `HOME`.
4. Tag the release, for example `vX.Y.Z`.
5. Push the branch and tag only after both Dvandva roles approve the final diff.

## Repo map & history

```
docs/
  dvandva-explainer.html            # visual product explainer (dark, self-contained; open via file://)
  case-studies/pr-353.md            # sanitized case study that motivated the design
  protocol/local-baton-channel.md   # baton protocol spec
  workflows/two-mode-agent-workflow.md
  research/2026-05-16-codex-install.md
plugins/dvandva/                    # the installable plugin: skills, commands, agents, references, hooks
rust/dvandva/                       # the dvandva binary + tests
templates/prompts/                  # historical v0 goal prompts (see note below)
product.md                          # product specification and acceptance criteria
README.md                           # this file
```

**What ships.** Dvandva is one `dvandva` plugin containing the `vadi`, `prativadi`, `research`, `testing`, `understanding`, and `worktree-setup` skills — the last three are Dvandva-native replacements for the standalone `testing` / `understanding` / `worktree-setup` skills during Dvandva work. Alongside them ship a **seed roster** of 15 canonical subagent roles under `plugins/dvandva/agents/` (researcher, architect, pattern-mapper, implementer, test-creator, debugger, cross-reviewer, adversarial-analyst, deep-reviewer, security-auditor, integration-checker, doc-verifier, deslopper, sandbox-verifier, baton-auditor; seed model classes remain the vendor-neutral `opus` / `sonnet`, while generated non-seed instances may use the expanded `opus` / `sonnet` / `fable` / `gpt` class vocabulary), plugin-local protocol references in `plugins/dvandva/references/` (the live v3 contract is `baton-schema-v3.json`; `baton-schema-v2.json` is the HISTORICAL `dvandva.baton.v2` read-path reference; `baton-schema.json` and `templates/channel/baton.json` are HISTORICAL `dvandva.baton.v1` references only, each carrying a `HISTORICAL: dvandva.baton.v1` marker and never written by the retired v1 path), and marketplace metadata in `.agents/plugins/marketplace.json` (Codex) and `.claude-plugin/marketplace.json` (root). The `dvandva` binary is the protocol runtime; the plugin bundles no executables.

**Run 3 — run-scoped dynamic agents.** Run 3 turns the seed roster into a foundation for run-scoped dynamic agent generation: parent roles generate additional named instances on demand, each recorded in `agent_instances` on the baton with its identity, parent role, model/permission class, read/write paths, base checkpoint, lifecycle state, output/evidence refs, and close result. Generated agents observe single-writer merge (they never own baton `assignee`, phase transitions, or final approval), explicit closure (every handle must be closed before its track counts complete), and dynamic write-path disjointness (write-path overlaps for live `planned` / `running` instances, or for instances sharing a `base_checkpoint`, are rejected unless a shared `conflict_group` with an explicit `depends_on` edge serializes the work). Generated instances are run-scoped and ephemeral; there is no roster sprawl, and it adds no daemon, mailbox, or hidden central process — the baton and the foreground wait remain the only coordination channel.

**Run 4 — path gates, git work-gating, and Dvandva-only retirement.** Run 4 generalizes `work_split` path gates (`write_paths` carries write intent and supplements rather than narrows `paths`; `cross_review` chunks are read-only unless they declare `write_paths`; live overlaps are rejected unless a `conflict_group` plus a `depends_on` edge serializes the work), adds repo-local git work-gating (above), and lands **Dvandva-only** standalone-agent **retirement**. `dvandva retire-agents` can retire only the five Claude Code symlink agents whose Dvandva-covered workflows the seed roster replaces: `adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and `sandbox-executor`. The scope is intentionally narrow — functional parity rests on equivalent-or-better empirical usage across Runs 1-4 plus 1.1.0 cache/roster parity and reversibility, Codex agent-axis retirement is a no-op, and skill directories are never touched. The helper is dry-run first, writes a backup manifest, and supports restore.

**Historical templates.** `templates/prompts/claude-doer-goal.md` and `templates/prompts/codex-reviewer-goal.md` are historical v0 artifacts — the v0 form of what the `vadi` and `prativadi` skills now are. They stay in-tree as reference only and are no longer active templates.

**History storage.** Every baton write is installed by `dvandva write` (validated, atomic), which also snapshots to `<baton-dir>/history/<checkpoint>-<status>-<assignee>.json` via `dvandva snapshot`. Terminal writes (`done`, `human_decision`, or `human_question`) additionally produce `baton.<sanitized-branch>-<checkpoint>-<status>.json` beside the active baton, so terminal records survive later runs without manual archiving; branch names containing `/` are sanitized to `-`. The `.dvandva/` directory is gitignored — inspect history with `ls <baton-dir>/history/` and `diff` between two snapshots.

### Reading order

1. `product.md` — product specification and acceptance criteria
2. `plugins/dvandva/references/local-baton-channel.md` — bundled baton protocol
3. `plugins/dvandva/references/state-transition-table.md` — bundled transition reference
4. `docs/case-studies/pr-353.md` — sanitized case study that motivated the design

### Non-goals

- No runtime daemon, hidden central process, or process launcher.
- No GitHub API integration.
- No PR creation.
- No npm-first distribution path.
