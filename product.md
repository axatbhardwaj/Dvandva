# Dvandva — Product Specification v1

Status: rewritten 2026-05-14 for richer flow (spec phase + phased implementation + mutual review + disagreement loop + `/goal` autonomy). Owner: axatbhardwaj. Supersedes the prompt-template-first approach in `templates/prompts/` and the single-shot doer→reviewer flow in the previous draft.

> **Spec rev 2026-06-11:** §3.1 adds `dvandva-write.sh` (validated atomic baton install + auto-snapshot, bundled byte-identical in both skill script dirs) and `scripts/test-dvandva-write.sh`. The wait helper's default `--max-wait` drops 900→540 so one foreground invocation fits Claude Code's 600 s Bash-tool cap (§7.2, §8.2, §12); it wakes early on baton-directory inotify events and retries once on torn reads. This pulls §16's deterministic validator forward to script level (a PreToolUse hook remains future work).
>
> **Spec rev 2026-06-27:** v2 design adds named run directories, a first-class research phase, generated user-facing HTML artifacts, and persistent shell waiting. Legacy v1 still uses `.dvandva/baton.json`; v2 runs use `.dvandva/runs/<run_id>/baton.json` with `schema: "dvandva.baton.v2"`, `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `run_explainer_reviews`, `active_roles`, `work_split`, `agent_instances`, `subagent_tracks`, and `verification_matrix`.
>
> **Spec rev 2026-06-28:** Run 4 adds generalized `work_split` path gates, repo-local git work-gating, and safe Dvandva-only retirement of replaced standalone user agents. The write helper applies `safe_rel_path` to `work_split.paths`, `work_split.read_paths`, and `work_split.write_paths`; for write-capable chunks, `write_paths` supplements rather than narrows `paths`, so the effective write set is their union; live write-capable chunks collide unless they share a `conflict_group` and an explicit `depends_on` serialization edge; `cross_review` remains read-only unless explicit `write_paths` are present. The git gate is local shell/git-hook enforcement (`DVANDVA_ROLE`, `core.hooksPath=.dvandva/githooks`, `Dvandva-Checkpoint`, drift lint), not a daemon or hidden central process; role preflight exports/asserts the role, installs a `.dvandva/githooks` delegating wrapper, preserves the prior hook chain, and records `dvandva.hooksAdoptedAt` as the local drift-lint baseline. Drift lint scans from the hook-adoption baseline floor when present so later stamped checkpoints cannot hide unstamped sandwich bypasses; terminal `done`, `human_question`, and `human_decision` batons are inactive for commit gating and active-baton drift detection. Retirement is limited to Dvandva-covered workflows: the five Claude symlink agents `adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and `sandbox-executor`; functional parity is justified by equivalent-or-better usage across Runs 1-4 plus 1.1.0 cache/roster parity and reversibility. Codex agent-axis retirement is a no-op, skills are out of scope, and the helper writes a backup manifest with restore support.
>
> **Spec rev 2026-06-29:** Accepted v2 run modes are now `development`, `research`, and `review`; `feature-pr` remains a legacy alias for `development`. Public docs now describe mode-conditional terminal artifact gates, later refined by the 2026-07-01 profile split: full-profile development requires `run_explainer_ref` plus completed approved `run_explainer_reviews` from both roles for that exact artifact, research requires `research_ref` and additionally `plan_ref` iff `research_outcome == seed_development`, and review requires `review_ref`. `termination_review` plus both final approvals are shared across all three modes.
>
> **Spec rev 2026-07-01:** Development mode now has orthogonal flow profiles: `fast`, `standard`, and `full`. New development scaffolds default to `standard`; legacy development batons without a profile remain effective `full`; hard-risk coordination/helper/schema/skill/path/terminal-artifact work forces `profile_floor: "full"`; and only full-profile development requires the final run explainer plus both `run_explainer_reviews`.
>
> **Spec rev 2026-07-01:** Wait-helper human intervention is now a stop-together invariant. `human_question` and `human_decision` are paired run pauses that stop both roles together. During any selected non-terminal wait, a newer sibling run's `human_question` or `human_decision` is propagated to the selected waiter unless `DVANDVA_CONCURRENT=1`; older sibling human-intervention batons stay parked, and sibling `human_question` output preserves `question`, `resume_assignee`, and `resume_status`. Post-handoff waits use `--since-checkpoint <written_checkpoint> --until-actionable` so a team-owned `active_roles` state cannot bounce the writer back to ready on the same checkpoint or wake a role before it has dependency-unblocked work.
>
> **Spec rev 2026-07-02:** The Dvandva helpers are ported into the `dvandva`
> multicall binary (crate `dvandva 2.0.0-alpha.2`), which replaces every bundled
> shell helper. Helper invocations are now `dvandva <sub>` subcommands —
> `dvandva state --compact`, `dvandva resolve`, `dvandva write`, `dvandva wait`,
> `dvandva snapshot`, `dvandva preflight`, `dvandva hook-preflight`,
> `dvandva commit-gate`, `dvandva drift-lint`, `dvandva install`,
> `dvandva install-codex`, `dvandva retire-agents`, `dvandva smoke-install`, and
> `dvandva lint <target>` (git-hook installation is owned by `dvandva preflight`).
> The plugin no longer bundles executables; the `dvandva` binary must be on
> `PATH` (installed via `cargo install dvandva --version 2.0.0-alpha.2`, or
> `cargo install --path rust/dvandva` from a checkout). This supersedes §3.2's
> "No standalone `dvandva` binary" non-goal — the binary now IS the runtime and
> deterministic validator. The write helper's hard-risk path floor set is
> re-expressed against the port: the coordination/helper triggers now cover
> `rust/dvandva/src/**` and `rust/dvandva/tests/**` in place of the deleted
> `scripts/*.sh`, `plugins/dvandva/scripts/*.sh`, and
> `plugins/dvandva/skills/*/scripts/dvandva-*.sh` patterns. The
> regression suite is cargo-based: `cargo fmt --check && cargo clippy
> --all-targets -- -D warnings && cargo test` in `rust/`, plus the `dvandva lint`
> family.
>
> **Spec rev 2026-07-02 (flow patches):** The `dvandva` binary gains four
> subcommands and the transition engine gains six protocol-graph patches; crate →
> `2.0.0-alpha.3`, plugin manifests → `1.3.0`. New subcommands: `dvandva next`
> lists the legal transitions from the current baton and scaffolds a validated
> `baton.next.json` candidate before `dvandva write` (never writes the baton
> itself); `dvandva brief --role <r>` prints a baton-native fresh-context pack
> (run header + effective profile, artifact refs, this role's current-phase
> work, open findings, verification matrix, last five history entries,
> next_action) for late phases; `dvandva baton-guard` is a PreToolUse hook wired
> via `plugins/dvandva/hooks/hooks.json` that blocks direct
> `Write`/`Edit`/`MultiEdit`/`NotebookEdit` of `baton.json` under `.dvandva/` or
> anything under `.dvandva/**/history/` and fails open on unparseable stdin (the
> plugin ships the hook; the `dvandva` binary must be on `PATH`); and `dvandva
> wait --notify <url>` / `DVANDVA_NOTIFY_URL` sends a best-effort ntfy-style POST
> on `human_question`/`human_decision`/`done`/`split_brain`/`stalled` (3s
> timeout, never changes exit codes or timing). Engine patches (v2 development):
> **(F7)** a capped plan-amendment loop — full `deslop -> spec_revision` /
> standard `phase_review -> spec_revision` sets the additive nullable
> `amendment_from_phase` to the current numeric phase, and the loop key
> `plan_amendment:<from-phase>` is capped by `disagreement_cap` per-episode (the
> exit resets `loop_counts`, so the cap is per-episode); `total_phases` is now
> engine-frozen once `master_plan_locked` (reason `bad_amendment
> total_phases_frozen`), and any human-authority `total_phases`/scope change
> happens on the write INTO `human_decision`, not on the resume. **(F8)**
> `test_creation` is team-owned in full profile (`assignee: "team"`, both
> `active_roles`, team-sync same-status) — the vadi authors the coverage track
> owned by `dvandva-test-creator` and the prativadi MAY record an optional
> adversarial-test track (recommended, not mandated). **(F9)** per-phase ceremony
> via the additive nullable `phase_profiles` `{"<numeric phase>":
> "standard"|"full"}` — effective profile of numeric phase N = `phase_profiles[N]`
> // run profile, mutated only in spec states with a per-phase hard-path floor;
> new cross-profile edges full `deslop -> implementing` and standard `phase_review
> -> parallel_implementing`, with the entry state chosen by the target phase's
> effective profile and terminal done-gate selection following the run profile.
> **(F6)** risk-triggered deep-review angles — a security angle when
> changed_paths ∪ current-phase work_split paths hit the `.env*`/secret/credential
> and api/client submatchers, an integration angle when ≥2 distinct-owner
> write-capable chunks share a cross-owner `depends_on` or `conflict_group`
> (reason `bad_deep_review_angles`); non-triggering runs pay nothing. **(F10)**
> full-profile `done` additionally requires a completed `explainer-verification`
> subagent track owned by `dvandva-doc-verifier` (reason
> `bad_explainer_verification`); the two role-owned `run_explainer_reviews` stay.
> **(F5, user requirement)** the Claude Code-hosted session owns surfacing
> `human_question` and `human_decision` to the human — a mobile-reachable
> in-session ask on writing a pause or on wait exit 11/12, regardless of role;
> the Codex-hosted role writes/observes the pause and stops silently, and if no
> Claude Code session is in the run the writer surfaces. Counter demotion (docs
> only): `review_of_review`/`counter_review` is reframed as a rarely-exercised
> safety valve (never fired across ~24 recorded runs); findings→fixing loops with
> `loop_counts` caps are the primary dispute mechanism, and the counter machinery
> is retained. All new fields are additive nullable; absent = pre-patch behavior.
>
> **Spec rev 2026-07-02 (hardening S2/S4/S5/S6):** Protocol-hardening slices
> re-keyed to the Rust engine; crate → `2.0.0-alpha.4`, plugin manifests →
> `1.4.0`. **S2-T1/T3:** a new terminal status `abandoned` (enterable only from
> `human_question`/`human_decision`, `assignee: "human"`, `active_roles: []`, no
> artifact/approval/loop gates, snapshot-archived like `done`); `dvandva wait`
> exits `13`; the resolver resumable-set, commit gate, and drift lint treat
> `{done, abandoned}` as terminal; `dvandva preflight` adds an `invalid_baton`
> sanity check (owner/status mismatch, or a team status with empty `active_roles`
> → exit 1). **S4:** the done gate now also requires every required ref to be an
> existing non-empty file (`missing_artifact`), a complete-and-fresh
> `verification_matrix` (`stale_verification_matrix`, anchored at the last
> implementation-family checkpoint), and candidate id-sets/findings that are a
> superset of the installed team-owned baton (`lost_update`); spec entry requires
> candidate `master_plan_locked == true` and unlock is forbidden except into
> `human_decision` (`bad_master_plan_locked`); `human_question` widens post-lock
> (D1); review mode gains `deep_review -> phase_fixing` (loop-capped); the commit
> gate crosschecks staged paths against `changed_paths ∪ work_split`
> (`DVANDVA_COMMIT_GATE_PATHS=warn|off`); the install path re-verifies the lock
> after the rename (`lock_lost_post_install`, exit 29). **S5:** the standard
> profile gains the capped mutual-review edges (D4); the v1 WRITE path is retired
> (`schema_retired`; the READ path stays lenient); the five-chunk parallel floor
> becomes `>=2` write-capable chunks per role AND (`>=5` total OR a reviewed
> `work_split_waiver`, `bad_work_split_waiver` when malformed); research-mode
> exploratory terminals label phase `"research"` (seed paths keep `"spec"`, with
> current-side leniency for old labels). **S6:** `dvandva lint schema-parity`
> holds the status catalog, required-key list, channel-doc copies, and HISTORICAL
> markers in parity. All new fields are additive nullable; absent = pre-hardening
> behavior.

## 1. What it is

Dvandva v1 is a pair of agent skills, written to the [agentskills.io](https://agentskills.io) open standard, that encode a disciplined two-agent collaboration protocol:

- `vadi` — the proposer/implementer skill. Runs in either Claude Code or Codex. Drives research, spec/plan creation, and phase implementation, then reviews any narrow fixups the prativadi makes.
- `prativadi` — the responder/reviewer skill. Runs in either Claude Code or Codex. Reviews research, Q&As during the spec phase, reviews each implementation phase, applies narrow fixups within an allowlist, and reviews the vadi's counter-changes when there is a disagreement.

Both skills share a baton file as the coordination channel. Legacy v1 uses `.dvandva/baton.json`. v2 adds named runs under `.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in one git worktree or directory as long as the human gives both sessions the same safe `run_id` or explicit `DVANDVA_BATON_FILE`. A safe `run_id` is one path segment: letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`; once a v2 baton exists, its `run_id` is immutable for that run. The default run mode is `walkaway`: the human gives an initial goal, starts or joins the two agent sessions once, and the skills use a cheap foreground wait helper when the baton assigns work to the other role. `supervised` mode is the serial fallback for one-engine runs: assigned-away agents exit and the human invokes the next role manually.

Accepted v2 baton modes are `development`, `research`, and `review`.
`feature-pr` remains a legacy alias for `development` on older batons.
`development` is the delivery run; its separate `profile` field selects the
full lifecycle or a compact fast/standard lifecycle.
`research` ends as research, optionally emitting a seed-development plan when
`research_outcome == seed_development`. `review` is a review-only run driven by
`review_intake`, `review_target`, and the generated `review_ref`.

Development mode also carries a separate flow `profile`: `fast`, `standard`,
or `full`. The profile is not a mode and must not be stored in `mode`.
`standard` is the default for new development scaffolds, while existing
development batons with no profile are effective `full` for compatibility.
`fast` is allowlisted prose-only work with positive allowlist evidence and no
hard-risk paths. Any product spec, baton schema/template, role skill,
write/wait/state/preflight helper, transition table, protocol doc,
top-level script, dependency manifest, secret/env surface,
external API client, artifact/history
format, or ambiguous/high-risk behavior raises `profile_floor` to `full`.
Escalation to a stricter profile is legal; lowering below `profile_floor`
requires `human_decision`.

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each role turn: `superpowers:using-superpowers` before action, `superpowers:brainstorming` before design, `superpowers:test-driven-development` before implementation, `superpowers:verification-before-completion` before success claims, `superpowers:writing-skills` when skills change, and `superpowers:dispatching-parallel-agents` / `superpowers:subagent-driven-development` when parallel tracks exist. If the active engine cannot invoke the relevant Superpowers skills, the Dvandva role must stop, surface setup instructions, and avoid writing a success or advancement baton.

The full-profile v2 development flow has eight lifecycle segments:

1. **Research phase** — vadi writes `research_ref`, a generated user-facing HTML artifact with machine-readable metadata, after conditional parallelism covers codebase, docs, tests, risks, and work distribution. The baton records `work_split`, `subagent_tracks`, and `verification_matrix`. Parallelize only genuinely disjoint tracks; when a track is not parallelized, record what was not parallelized and why.
2. **Master-planning phase** — collaborative plan creation. Vadi drafts; prativadi Q&As; vadi revises. Either role may ask the user questions while the plan is still unlocked. Loop until plan converges. The generated `plan_ref` HTML declares N implementation phases.
3. **Implementation phase** — full-profile vadi/prativadi work implements phase N chunks in team-owned `parallel_implementing` without silently mixing implementation, testing, and review responsibilities; fast/standard profiles use the compact `implementing` state and still preserve verification and independent review evidence.
4. **Test-creation phase** — vadi creates or updates tests for every new behavior and records a 100% test coverage target in `verification_matrix`; source-only docs/skills get lint/review coverage with rationale.
5. **Cross-review phase** — both roles review peer-owned chunks and record `cross-review` subagent tracks before any deep review begins.
6. **Deep-review phase** — prativadi performs independent deep review after implementation, test creation, and reciprocal cross-review. Review is separate from test creation and must inspect code, tests, docs, baton fields, and claims. When subagent tooling is available, use at least three angle-specific reviewers: correctness/regression, test/evidence, and protocol/handoff.
7. **De-slop phase** — vadi/prativadi loop on nits, low/minor bugs, stale wording, vague instructions, duplicated logic, and generated-looking clutter until no findings remain except items explicitly accepted in `deferred`.
8. **Phase advancement or completion** — on agreement, make a regular local checkpoint commit for the verified logical slice when `allow_commit` permits it, then advance to phase N+1. On completion of a final full-profile phase, write a one-date run explainer under `./superpowers/run-reports/` (`YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or `<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`), set `run_explainer_ref`, require both roles to review that exact explainer through `run_explainer_reviews`, require both final approvals, optionally push, then transition to `done`. Fast/standard final phases skip the explainer and use the compact verification/review evidence gate instead.

Full-profile implementation-phase parallelism is mandatory in v2. Spec approval enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the phase `work_split` must contain at least five implementation chunks split across both roles for two-team parallel implementation. Every implementation chunk names reciprocal `cross_review_by`, and `test_creation` routes to `cross_review` before `deep_review`. If cross-review finds peer-chunk defects, the phase routes through `cross_fixing` and then back to `test_creation` before review continues. Fast and standard profiles use the compact `implementing -> phase_review -> termination_review -> done` path, still with `profile_decision`, passing final verification, completed `verification_matrix` evidence, completed approved prativadi `phase-review` evidence with current-cycle `review_checkpoint`, shared termination, and role-owned final approvals.

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
- `dvandva wait` (`rust/dvandva/src/wait.rs`) — foreground wait subcommand of the `dvandva` binary. It polls the resolved baton cheaply, without spending model turns, until the baton returns to the role or reaches a terminal human/done state.
- `dvandva write` (`rust/dvandva/src/write.rs`) — validated atomic baton installer; rejects illegal v1/v2 transitions, installs via tmp+rename, auto-snapshots. Covered by the write port's `cargo test` suite.
- The wait exit-code contract is covered by the wait port's `cargo test` suite (`rust/dvandva/src/wait.rs`).
- `dvandva lint artifacts` (`rust/dvandva/src/lint/artifacts.rs`) — generated-artifact policy lint. It rejects generated Markdown under `./superpowers/` and requires generated HTML artifacts to be dark, self-contained, offline-renderable, and backed by a machine-readable Dvandva metadata block.
- `plugins/dvandva/references/baton-schema.json` — bundled legacy v1 schema seed with all required keys (`run_mode`, `phase`, `total_phases`, `plan_ref`, `master_plan_locked`, `question`, `resume_assignee`, `resume_status`, `disagreement_round`, `disagreement_cap`, `turn_cap`, `review_target`, `current_engine`, final-approval fields, etc.).
- `README.md` install section covering: marketplace install (primary), development symlink/copy install (fallback), and the Superpowers hard-dependency check for every engine running a Dvandva role.
- One pilot writeup at `docs/case-studies/pilot-01.md` after the workflow ship.

### 3.1a In v2 design

- `dvandva.baton.v2` — run-scoped baton schema for `.dvandva/runs/<run_id>/baton.json`. Accepted public modes are `development`, `research`, and `review`; older batons may still serialize `feature-pr` as a legacy alias for `development`. Development runs add `profile`, `profile_floor`, `profile_decision`, and `profile_history` for the orthogonal `fast | standard | full` lifecycle-depth selector. Required v2 fields include safe `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, `active_roles`, and `agent_instances`; full-profile development terminal `done` additionally requires `run_explainer_reviews` entries from both roles for the exact `run_explainer_ref`, while fast/standard development terminal `done` requires `profile_decision`, passing final verification, completed `verification_matrix` evidence, completed approved prativadi `phase-review` evidence with current-cycle `review_checkpoint`, and both role-owned final approvals without the explainer gate. Nullable v2 additions for accepted run modes are `research_outcome`, `review_ref`, and `review_intake`; `review_target` remains the existing selector field. v1 remains valid only for the legacy `.dvandva/baton.json` fallback. Run 3 adds `agent_instances` — a first-class baton array for generated run-scoped agent instances recording identity, parent role, seed agent, model/permission class, read/write paths, base checkpoint, lifecycle state, output refs, evidence refs, and close result. `agent_instances` is separate from the post-hoc `subagent_tracks` record and is validated by the Run 3 write helper for: safe ids, no duplicates or reserved owner-name collisions, supported model/permission classes, matching closed registry records for any dynamic `subagent_tracks` owner not in the seed roster, closure evidence plus non-empty `work_item_ids` before a track counts complete, and dynamic write-path disjointness among generated instances sharing the same `base_checkpoint` or among any two live (`planned`/`running`) instances regardless of base checkpoint. The live v2 write-helper enforcement covers v2-only fields, schema continuity for existing runs, v2 status-owner pairs, honest `subagent_tracks`, and v2 lifecycle transitions intentionally instead of by convention.
- Run 4 fields and conventions for `work_split`: write-capable chunks should declare `write_paths`; read-only review chunks may declare `read_paths`; overlapping writers require `conflict_group` plus `depends_on`; `cross_review` has no write intent unless explicit `write_paths` are present.
- Research lifecycle states before spec lock: `phase: "research", status: "research_drafting"` for vadi research synthesis, `research_review` for prativadi independent review, and `research_revision` for vadi response to research findings. v2 scaffolds new named runs at `research_drafting`; legacy v1 scaffolds at `spec_drafting`.
- Test and review lifecycle states are separate in v2: `test_creation` records the doer's tests and coverage evidence, `deep_review` records independent prativadi review after tests exist, and `deslop` records cleanup loops for nits, low/minor bugs, stale wording, and unclear instructions. A phase does not advance while unresolved `deep_review` or `deslop` findings remain unless explicitly accepted in `deferred`.
- Team-owned v2 states (`parallel_implementing`, `cross_review`, `cross_fixing`, `termination_review`) may write same-status sync checkpoints to record partial completion, task distribution, peer wait state, or shared stop-review evidence without pretending the phase is ready to advance. Scalar-owner states still reject same-status rewrites.
- Phase convention: implementation-chunk tracks use the numeric implementation phase, while cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.
- The generated user-facing artifacts are HTML: plans, research reports, evaluations, reviews, pilot write-ups, and run reports. Every completed full-profile v2 development run must produce a one-date explainer under `./superpowers/run-reports/`: use `YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or `<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`; never add a second date prefix. The explainer includes decisions, development, architecture, verification, and diagrams. Fast/standard v2 development runs skip the explainer and instead require `profile_decision`, passing final verification, completed `verification_matrix` evidence, and completed approved prativadi `phase-review` evidence with current-cycle `review_checkpoint`. Platform/source Markdown such as `SKILL.md`, command files, README/source docs, and prompt templates stays in its native format.
- Continuous polling is the hard rule. `dvandva wait` treats `--max-wait` as a heartbeat interval by default and keeps polling until the selected baton assigns the role, reaches `done`/`human_question`/`human_decision`, or the user interrupts. After a handoff write, roles pass `--since-checkpoint <written_checkpoint> --until-actionable` so the helper keeps polling while the selected baton remains at or below that checkpoint, even if team-owned `active_roles` still names the writer; action-aware waiting then wakes only when the baton advances and the role has dependency-unblocked work. `human_question` and `human_decision` are paired run pauses that stop both roles together. During any selected non-terminal wait, a newer sibling run's `human_question` or `human_decision` is propagated to the selected waiter unless `DVANDVA_CONCURRENT=1`; older sibling human-intervention batons are ignored, and sibling `human_question` output preserves `question`, `resume_assignee`, and `resume_status`. `--persist` is accepted for older call sites and is now redundant. `--persist-max <seconds>` is a shell-budget cap; wait-helper persist cap exit 23 is not proof the peer is done and the role must immediately re-enter the wait unless the user interrupts. The write-helper validation exit 23 means a candidate failed schema/required-key/status validation. Finite exit 20 is available only through explicit `--finite` compatibility mode and is not valid for normal walkaway loops.
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
| `dvandva` binary on `PATH` | The Dvandva runtime subcommands (`state`, `resolve`, `write`, `wait`, ...) read and write baton fields | `dvandva --version` |

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
      baton-schema-v2.json
      local-baton-channel.md
      state-transition-table.md
rust/
  dvandva/                # the dvandva multicall binary
    Cargo.toml
    src/                  # runtime, preflight, git work-gate, install, lint subcommands
    tests/
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

The full-profile v2 flow has eight segments and an end state: research, master planning, implementation, test_creation, cross_review, deep_review, deslop, and phase advancement/completion. Fast and standard profiles use the compact `implementing -> phase_review -> termination_review -> done` path after the optional research/spec prelude. Every arrow in the diagram is a baton write by the active agent. In default walkaway mode, the other persistent session is already blocked in `dvandva wait`; the helper returns when the baton assigns that role, and the agent re-enters preflight.

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
                  │    or → phase 1 profile path       │
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
                     │ full baton: phase 1, parallel_implementing
                     │ compact baton: phase 1, implementing
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
   │       │       ├─ clean ──▶ next phase / stop review   │
   │       │       └─ findings ──▶ phase_fixing            │
   │       │                                               │
   │       ├─ apply narrow fixup ──▶ MUTUAL REVIEW         │
   │       │     baton → review_of_review                  │
   │       │     review_target: prativadi_fixups           │
   │       │     assignee: vadi                            │
   │       │       ▼                                       │
   │       │   Vadi (reviewing prativadi fixups)           │
   │       │       │                                       │
   │       │       ├─ approve ──▶ deslop                   │
   │       │       │                                       │
   │       │       └─ disapprove ──▶ DISAGREEMENT LOOP     │
   │       │             disagreement_round += 1           │
   │       │             Vadi writes counter-change        │
   │       │             baton → counter_review            │
   │       │             review_target: vadi_counter       │
   │       │               ▼                               │
   │       │           Prativadi reviews counter-change    │
   │       │               │                               │
   │       │               ├─ approve ──▶ deslop           │
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

   Final phase clean → termination_review → both final approvals true
      → optional commit/push if allowed
      → status: done → cycle ends
```

Phase advancement invariant: the vadi never advances a phase directly after implementation or fixing. Full-profile v2 approvals route back through `deslop`; only a clean non-final `deslop` checkpoint starts the next phase, and only a clean final `deslop` checkpoint enters shared `termination_review`. Legacy v1 direct advancement remains valid only for explicitly legacy runs. The agent writing the first baton for the next phase must set `disagreement_round: 0`.

Three caps the spec enforces operationally:

- **Disagreement round cap (default 3).** Counter resets at the start of each phase. On the 3rd mutual-review disapproval, the writing agent must set `status: human_decision` and exit. Tunable per-phase via a `disagreement_cap` field on the baton (set during spec phase by either agent).
- **Per-invocation turn cap (default 60).** Each agent's `/goal` invocation must stop after the active model-work turn cap even if the baton condition has not been hit, and surface its current state for human review. Passive shell wait heartbeats do not count against this cap.
- **No phase count cap.** Plans declare `total_phases` during the spec phase; the protocol does not constrain how many phases are reasonable. The spec phase itself is responsible for sane phase scoping.
- **Planning-question boundary (S4-T5/D1).** `human_question` may be entered pre-lock (`master_plan_locked: false`) from any planning state (`research_*`/`spec_*`), AND post-lock from the working states `implementing`, `parallel_implementing`, `test_creation`, `cross_fixing`, and `phase_fixing`. The rule: when a genuine requirement ambiguity — not a design or scope decision — blocks progress, route one concrete question to the human instead of guessing; `resume_status`/`resume_assignee` restore the exact prior state on the answer. Entering `human_question` is not a loop edge (it is a human-bounded stop-together pause). `human_decision` stays the re-routing and scope-escalation state. The Claude Code-hosted session owns surfacing both to the human (F5, see §12).
- **Counter demotion.** The `review_of_review`/`counter_review` vadi-counter mutual-review loop is a rarely-exercised safety valve — it has never fired across the ~24 recorded runs. The primary dispute mechanism is the findings→fixing loops (`deep_review->phase_fixing`, `cross_review->cross_fixing`, `phase_review->phase_fixing`) bounded by `loop_counts` caps at `disagreement_cap`. The counter machinery is retained (not removed) for the rare case a reviewer's own inline fixup is itself disputed.

## 7. vadi skill design

### 7.1 Frontmatter

- `name: vadi`
- `description:` one paragraph, front-loaded with trigger words: *implement*, *vadi*, *spec*, *plan with review*, *phased implementation*, *hand off for review*, *review the prativadi's fixups*, *review codex's fixups*. Must list both spec-phase triggers and implementation-phase triggers since one skill handles both. Under the 1,536-char listing cap.

No `allowed-tools` reliance (see section 9). Optional Claude-only `argument-hint: "[task description]"` for UX.

### 7.2 Body sections (target < 500 lines)

1. **Role one-liner** — "You are the Dvandva vadi. You draft plans, implement them phase by phase, and review the prativadi's narrow fixups."
2. **Preflight (all modes)** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then Existing baton discovery across `.dvandva/runs/*/baton.json` plus legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If active batons exist and no selector is explicit, ask the user whether to continue or create a new run; if only terminal batons exist, auto-create a new named run. If the baton is absent, the vadi scaffolds the resolved directory and writes the seed baton through `dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"` with `original_ask` preserved in initial context. If the baton is assigned away and `run_mode: "walkaway"`, wait continuously on `"$BATON_FILE"` until a terminal baton state, role ownership, or user interrupt; shell caps must re-enter the wait. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role.
3. **Mode R1: research drafting** — when `status: "research_drafting"` in any v2 mode. Development/research modes use `phase: "research"`; review mode uses `phase: "review"`. Invoke `dvandva:research`, preserve `original_ask`, use conditional parallelism when available, write the generated HTML `research_ref`, populate `work_split`, `subagent_tracks`, and `verification_matrix`, and hand to prativadi with mode-correct phase plus `status: research_review`.
4. **Mode R2: research revision** — when `status: "research_revision"` in any v2 mode. Invoke `dvandva:research`, address prativadi research findings, update `research_ref`, `work_split`, and `verification_matrix`, clear resolved findings, and hand back to `research_review` with the same mode-correct phase.
5. **Mode A: spec drafting** — when `phase: "spec", status: "spec_drafting"`. Read `research_ref`, `work_split`, and `verification_matrix` first. Invoke `superpowers:brainstorming` skill flow without rediscovering already-settled research. The vadi may ask the user questions if required before the master plan is useful. Produce a gitignored dark self-contained HTML plan under `./superpowers/plans/YYYY-MM-DD-<topic>.html` with declared `total_phases` and a per-phase scope list. Set `plan_ref`, `total_phases`, and `master_plan_locked: false` on the baton. Write baton with `status: spec_review, assignee: prativadi, review_target: spec`.
6. **Mode B: spec revision** — when `phase: "spec", status: "spec_revision"`. Read the baton's `findings` array (prativadi's Q&A), respond in the `plan_ref` plan, update affected `total_phases` if scope changed. Always write baton with `status: spec_review, assignee: prativadi, review_target: spec`; the prativadi is the only actor that can advance the spec to phase 1. Follow the stop/wait rule.
7. **Mode C: phase implementation** — when `phase: 1..N, status: "parallel_implementing"` for full-profile v2, or `"implementing"` for fast/standard-profile v2 and explicitly selected legacy v1 runs. Read the corresponding phase scope from the `plan_ref` plan and the relevant `work_split` / `verification_matrix` entries. Invoke `superpowers:test-driven-development` when applicable. Implement only the phase scope; do not bleed into adjacent phases. Full-profile v2 writes baton with `status: test_creation, assignee: team, active_roles: ["vadi", "prativadi"], review_target: null` (F8: `test_creation` is team-owned) after both roles record implementation evidence; fast/standard-profile v2 writes `status: phase_review, assignee: prativadi, review_target: implementation` after implementation and verification evidence are ready.
8. **Mode T: test creation** — when `phase: 1..N, status: "test_creation"`. F8: `test_creation` is team-owned in full profile (`assignee: team`, `active_roles: ["vadi", "prativadi"]`) — the vadi authors the coverage track owned by `dvandva-test-creator`, and the prativadi MAY record an optional adversarial-test track (recommended for decorrelated coverage, not mandated). Tests-first stays a Superpowers mandate during implementation; `test_creation` is the team coverage-evidence gate, not "write tests now". Create or update tests for every new behavior, record 100% test coverage evidence or source-only rationale in `verification_matrix`, run motivating tests and cheap checks, then write baton with `status: cross_review, assignee: team, active_roles: ["vadi", "prativadi"], review_target: implementation`. Test creation is separate from review.
9. **Mode D: phase fixing** — when `phase: 1..N, status: "phase_fixing"`. Read `findings` from prativadi. Fix only listed items, update tests if behavior changed, and return through the profile-appropriate review path: full-profile development runs go through `test_creation`, while fast/standard compact runs return to `phase_review` with refreshed verification evidence. Follow the stop/wait rule.
10. **Mode S: deslop** — when `phase: 1..N, status: "deslop"`. Remove nits, low/minor bugs, stale wording, duplicated instructions, and generated-looking clutter found by deep review. Use conditional parallelism for independent style, protocol, and artifact-integrity tracks and record them in `subagent_tracks`. If no unresolved issues remain except explicitly accepted `deferred` items, advance to the next phase or, on the final phase, shared `termination_review`. If instead a post-lock plan/scope change is needed, open a capped plan-amendment episode (F7): full `deslop -> spec_revision` (standard runs use `phase_review -> spec_revision`) sets `amendment_from_phase` to the current numeric phase and increments loop key `plan_amendment:<from-phase>` (cap = `disagreement_cap`, `loop_counts` reset on exit); exit re-enters implementation at a numeric phase ≥ `amendment_from_phase`.
11. **Mode E: prativadi-fixup review** — when `status: "review_of_review", review_target: "prativadi_fixups", assignee: vadi`. Read the prativadi's `narrow_fixups` array and inspect the diff the prativadi applied. Decide: approve or disapprove.
   - On approve: legacy v1 explicit runs may write baton with `phase: N+1, status: implementing, assignee: vadi, disagreement_round: 0` (advance), or `status: done` after final approval/ship if N was the final phase. V2 returns to the review/deslop lifecycle and advances only through the `deslop` gate. Follow the stop/wait rule.
   - On disapprove: increment `disagreement_round`. If `disagreement_round >= cap`, write baton `status: human_decision, assignee: human`. Otherwise, write counter-changes inline, write baton `status: counter_review, review_target: vadi_counter, assignee: prativadi`. Follow the stop/wait rule.
12. **Regular checkpoint commits** — after any active mode changes files and the relevant verification commands pass, make a local checkpoint commit when `allow_commit == true`. Commit only the intended `changed_paths` union, excluding `.dvandva/` and `superpowers/`, and only when `git status --short` has no unrelated dirty paths. Role preflight must already have exported and asserted `DVANDVA_ROLE=<role>`, run the `dvandva preflight --role <role>` hook stage, verified `core.hooksPath=.dvandva/githooks`, and recorded the hook-adoption baseline before checkpoint commits begin. The `.dvandva/githooks` delegating hooks (symlinks to the `dvandva` binary) preserve the prior hook chain. Use one logical change per commit, semantic prefix, and a subject of 50 characters or fewer. Record the commit hash in `verification` or `summary` as `checkpoint_commit=<hash>`. Do not push checkpoint commits.
13. **Final ship rule** — before terminal `done`, satisfy the mode/profile-conditional terminal artifact gate: full-profile development runs write the one-date run explainer under `./superpowers/run-reports/` (`YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or `<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`), set `run_explainer_ref`, and record completed approved `run_explainer_reviews` from both vadi and prativadi for that exact path; fast/standard development runs skip the explainer but still require `profile_decision`, passing final verification, completed `verification_matrix` evidence, completed approved prativadi `phase-review` evidence with current-cycle `review_checkpoint`, shared `termination_review`, and both role-owned final approvals; research runs require `research_ref` plus `plan_ref` iff `research_outcome == seed_development`; review runs require `review_ref`. Push only when `allow_pr: false`, `allow_push: true`, `vadi_final_approval: true`, and `prativadi_final_approval: true`. If intended dirty files remain and `allow_commit == true`, make one final local commit first; if no dirty intended files remain because checkpoint commits already captured the work, record `final_commit` as `git rev-parse HEAD`. Unrelated dirty paths force `human_decision`. Record `final_commit` and `pushed_ref`. Never create a PR.
14. **Stop rule (universal)** — in walkaway mode, do not stop on role handoff or slow peer work. Surface BATON_STATE_COMPACT via `dvandva state --compact`, run the wait helper, and continue from preflight when the baton returns. Continuous shell polling stops only for `done`, `human_question`, `human_decision`, user interrupt, or turn-cap escalation during active model work. `human_question` and `human_decision` remain legal from early v2 research even before `research_ref` exists, so missing setup can be surfaced before the first research artifact is written. F5 human-intervention handling: the Claude Code-hosted session owns surfacing `human_question`/`human_decision` to the human in-session (mobile-reachable) on writing a pause or on wait exit 11/12; the Codex-hosted role stops silently unless it is the only session; pair the wait with `dvandva wait --notify <url>` (or `DVANDVA_NOTIFY_URL`) so the pause also pings your phone.
15. **`/goal` condition** — embedded in the skill body verbatim, centered on continuing until `done`, `human_question`, or `human_decision`; if assigned away, block in the wait helper instead of spending model turns.
16. **Failure modes** — section 12.

## 8. prativadi skill design

### 8.1 Frontmatter

- `name: prativadi`
- `description:` front-loaded triggers: *review*, *spec Q&A*, *prativadi pass*, *narrow fixups*, *adversarial verification*, *check the baton*, *review the vadi's counter-change*, *review claude's counter-change*. Covers all three prativadi modes.

### 8.2 Body sections

1. **Role one-liner** — "You are the Dvandva prativadi. You Q&A on plans, review implementation phases, apply narrow fixups, and review the vadi's counter-changes."
2. **Preflight** — read `AGENTS.md`, resolve the baton path from `DVANDVA_BATON_FILE`, `DVANDVA_RUN_DIR`, safe `DVANDVA_RUN_ID`, then Existing baton discovery across `.dvandva/runs/*/baton.json` plus legacy `.dvandva/baton.json`, and set `BATON_FILE` plus `BATON_NEXT_FILE`. If no explicit selector exists and exactly one active/resumable baton exists, select it; if several exist, surface the candidates and wait for a human choice; if none exists, wait continuously on the selected or would-be named-run baton with `--allow-missing` unless `DVANDVA_NO_WAIT=1` is set. If the baton is assigned away and `run_mode: "walkaway"`, wait continuously on `"$BATON_FILE"` until role ownership, terminal baton state, or user interrupt; shell caps must re-enter the wait. If `run_mode: "supervised"`, exit on assigned-away states so the human can invoke the next role. **Additionally verify `superpowers:brainstorming` is available in the current session** before spec Q&A; if absent, surface install instructions and exit (per section 4 prerequisites). Do not depend on one fixed filesystem path.
3. **Mode R: research review** — when `status: "research_review"` with `phase: "research"` in development/research modes or `phase: "review"` in review mode. Invoke `dvandva:research` for independent research review. Do not rely solely on the vadi's `research_ref`; inspect relevant sources and use conditional parallelism when available. If gaps remain, populate `findings` and write `status: research_revision, assignee: vadi` with the same mode-correct phase. If research is sufficient, branch by mode: development advances to `phase: "spec", status: "spec_drafting"`; research seed-development advances to `spec_drafting` before stop review; exploratory research advances to shared `termination_review`; review mode advances to `phase: "review", status: "deep_review"` without writing `review_ref` during intake.
4. **Mode A: spec Q&A** — when `phase: "spec", status: "spec_review", review_target: "spec"`. Invoke `superpowers:brainstorming` skill flow as the questioner. Read the `plan_ref` plan, surface Q&A in the baton's `findings` array, optionally edit the plan directly for narrow improvements (typos, sharper phrasing). The prativadi may ask the user questions if required before the master plan can be approved or handed back. Decide: hand back to vadi (questions remain) or advance. Write baton `status: spec_revision, assignee: vadi` for more Q&A. For v2 full-profile phase work, approve by writing `phase: 1, status: parallel_implementing, assignee: team, active_roles: ["vadi", "prativadi"], disagreement_round: 0, master_plan_locked: true`; For v2 fast/standard-profile phase work, approve by writing `phase: 1, status: implementing, assignee: vadi, active_roles: [], disagreement_round: 0, master_plan_locked: true`; legacy v1 explicit runs use `phase: 1, status: implementing, assignee: vadi`.
5. **Mode B: deep review** — when `phase: 1..N, status: "deep_review", review_target: "implementation"` in development runs, or `phase: "review", status: "deep_review"` in review runs. Read diff vs branch baseline only after the mode-appropriate intake/test evidence is complete. Full-profile `test_creation` is team-owned (F8): confirm the vadi's coverage track owned by `dvandva-test-creator` and note any optional prativadi adversarial-test track. Cross-check the peer's `verification` block and the planned coverage in `verification_matrix` (did the commands actually pass? do they cover the changed paths and risks, and is 100% test coverage for new behavior documented?). Use at least three angle-specific reviewers/tracks in `subagent_tracks`: correctness/regression, test/evidence, and protocol/handoff; add `dvandva-adversarial-analyst` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses. F6: for a full-effective-profile phase the `deep_review -> {deslop, review_of_review}` gate additionally requires a completed `security` angle (owner `dvandva-security-auditor`) when changed_paths ∪ current-phase work_split paths hit the `.env*`/secret/credential or api/client submatchers, and a completed `integration` angle (owner `dvandva-integration-checker`) when ≥2 distinct-owner write-capable chunks share a cross-owner `depends_on` or `conflict_group` (reason `bad_deep_review_angles`). Look for bugs, regressions, stale docs, missing tests, claims not matching diff, and deslop opportunities.
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
9. **Final ship rule** — same as vadi. The prativadi may commit/push only after both final approvals are true, the current dirty paths match `changed_paths`, PR creation remains false, and the mode/profile-conditional terminal artifact or evidence exists: `run_explainer_ref` plus both roles' `run_explainer_reviews` for development/full, `profile_decision` plus passing final verification, completed `verification_matrix` evidence, and completed approved prativadi `phase-review` evidence with current-cycle `review_checkpoint` for development/fast and development/standard, `research_ref` plus conditional `plan_ref` for research runs, or `review_ref` for review runs. Full-profile development explainers must include decisions, development, architecture, verification, and diagrams. F10: full-profile `done` additionally requires a completed current-cycle `explainer-verification` subagent track owned by `dvandva-doc-verifier` (result approved/passed, non-empty outputs and evidence_refs) that checks the explainer's claims against the code, beyond the two role-owned `run_explainer_reviews` (reason `bad_explainer_verification`).
10. **Stop rule** — in walkaway mode, do not stop on role handoff. Surface BATON_STATE_COMPACT via `dvandva state --compact`, run the wait helper, and continue from preflight when the baton returns. In supervised mode, exit on role handoff. F5 human-intervention handling: the Claude Code-hosted session owns surfacing `human_question`/`human_decision` to the human in-session (mobile-reachable) on writing a pause or on wait exit 11/12; the Codex-hosted role stops silently unless it is the only session; pair the wait with `dvandva wait --notify <url>` (or `DVANDVA_NOTIFY_URL`).
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
dvandva install
```

`dvandva install` wraps the public install path for users: it registers the Dvandva marketplace and installs `dvandva@dvandva` in both Claude Code and Codex. It accepts `--claude-only` and `--codex-only` for one-engine installs. For Codex, it delegates to `dvandva install-codex`, which runs `codex plugin add dvandva@dvandva` on current Codex builds and keeps the legacy app-server RPC install as a fallback for older builds. The `dvandva` binary must already be on `PATH` (`cargo install --path rust/dvandva` from a checkout is the primary form and builds the current `2.0.0-alpha.4`; `cargo install dvandva --version 2.0.0-alpha.3` installs the latest published crate, and `--version 2.0.0-alpha.4` works once that version is published). The authoritative preflight is whether the current agent session can see and invoke the required Superpowers skills.

### 11.2 Development install fallback

For local development against a checkout, prefer marketplace install from the checkout:

```bash
dvandva install "$(pwd)"
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
| `status: human_question` | Stop and surface the one concrete `question` plus `resume_assignee` and `resume_status`. If the user answers in the current prompt, restore `assignee` and `status` from those fields, clear the question fields, and continue. Enterable pre-lock from any planning state and post-lock from the working states `implementing`/`parallel_implementing`/`test_creation`/`cross_fixing`/`phase_fixing` (S4-T5/D1); `human_decision` remains the re-routing and scope escalation. |
| `status: human_question` or `human_decision` written or observed (F5) | The Claude Code-hosted session owns surfacing human_question and human_decision to the human. Whichever role the Claude Code session hosts asks the human directly in-session (question, options, resume fields) — on writing the pause or on wait exit 11/12 (including sibling propagation) — and stays available for the answer via Claude Code's mobile/remote surface. The Codex-hosted role writes/observes the pause and stops silently; it must not compete to consume the answer. If no Claude Code session is part of the run, the writer of the pause surfaces it. `current_engine` still records the writer for traceability. Recommended pairing: run waits with `dvandva wait --notify <url>` (or `DVANDVA_NOTIFY_URL`) so the pause also pings your phone. |
| `plan_ref` missing, or referenced plan file missing during a phase mode | Doer: surface "spec phase did not complete; cannot start phase implementation." Exit. Reviewer: same. |
| `total_phases` is 0 or unset during phase mode | Both: surface schema integrity error, exit. The spec phase is responsible for setting this. |
| `disagreement_round >= disagreement_cap` | Whichever agent next writes the baton: set `status: human_decision, assignee: human`. Do not write further counter-changes. |
| `/goal` active-work turn cap hit before exit condition | Agent: surface current baton state and a "still owe work" summary, set `status: human_decision`, exit. The default active-work cap is 60; shell wait heartbeats do not count. |
| Wait helper exits 20 (`timeout`) | This can only happen in explicit `--finite` compatibility mode. Normal walkaway loops must not use `--finite`; immediately re-enter continuous wait unless the user interrupts. |
| Wait helper exits 23 (`persist_max`) | This is the wait-helper persist cap exit 23. Surface the still-waiting state and immediately re-enter the wait helper with a fresh cap unless the user interrupts. The cap protects shell budgets; it is not a baton terminal state and is not evidence the peer stopped. |
| Write helper exits 23 during candidate install | This is the write-helper validation exit 23. The candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation. Fix the candidate file and rerun `dvandva write`; never edit the installed baton directly. |
| Write helper exits 23 `schema_retired` | The candidate (or the current baton) carries `schema: "dvandva.baton.v1"`; the v1 WRITE path is retired (S5-T2/D5). Migrate the candidate to `dvandva.baton.v2` and rerun. Old v1 batons stay readable on the `state`/`resolve`/`wait` path. |
| Write helper exits 23 `missing_artifact` / `stale_verification_matrix` / `lost_update` (done gate) | At a `done` candidate: each required ref (`research_ref`, plus `plan_ref`/`run_explainer_ref`/`review_ref` when the mode requires it) must be an existing non-empty file (`missing_artifact`, S4-T1); every `verification_matrix` row must be complete with a numeric `evidence_checkpoint`/`review_checkpoint` at or after the last implementation-family checkpoint (`stale_verification_matrix`, S4-T6); and a team-owned candidate's `subagent_tracks`/`agent_instances`/`work_split` ids and `findings` must be a superset of the installed baton's (`lost_update`, S4-T4). Fix the candidate and rerun. |
| Write helper exits 23 `bad_master_plan_locked` | Spec entry (`spec_review -> implementing`/`parallel_implementing`) requires candidate `master_plan_locked == true` (S4-T2/D2), and `master_plan_locked` `true->false` is rejected on every development edge except a write whose `new_status` is `human_decision`. Set the lock, or route to `human_decision` to unlock, then rerun. |
| Write helper exits 23 `bad_work_split_waiver` | The `work_split_waiver` object is malformed (S5-T3); a valid waiver is `{reason: <non-blank string>, approved_by: "prativadi", checkpoint: <number>}`. The per-role `>=2` write-capable-chunk floor is never waivable; only the `>=5`-total floor is. |
| Write helper exits 29 `lock_lost_post_install` | The install renamed the baton, then the fencing token was found overwritten by a peer that age-stole the lock (S4-T10). The install DID happen and may be superseded; re-read the installed baton before deciding anything (`caller_must_reread=true`). |
| `status: abandoned` / wait exit 13 | Terminal (S2-T1): the run was abandoned from `human_question`/`human_decision`. Surface it and stop; do not scaffold or advance. Reopen only via a hand-authored `human_decision` write. |
| Preflight `invalid_baton` (exit 1) | After resolve, `dvandva preflight` read-only-validates the installed baton (S2-T3): an assignee/status owner mismatch, or a team-owned status with empty `active_roles`, prints `reason=invalid_baton detail=<violation>` and exits 1 without mutating. Fix the baton owner/roles, or route via `human_decision`. |
| Commit gate blocks on staged paths (S4-T9) | While a baton is active, the pre-commit `dvandva commit-gate` blocks a commit whose staged paths fall outside `changed_paths ∪ role-visible work_split paths/write_paths` (`.dvandva/` and `superpowers/` are always exempt). Keep `changed_paths` honest, or set `DVANDVA_COMMIT_GATE_PATHS=warn` (print offenders, allow) or `=off` (skip). Declared-empty scope (`changed_paths: []`/empty role-visible `work_split` present) blocks all non-exempt staged paths; a baton that declares no scope key at all is exempt (fail-open) for legacy compatibility. |
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
- **Rust definition-of-done gate** (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` in `rust/`): the port's regression suite, exercising the read-path, write-path, wait, preflight, hook, install, and lint modules.
- **Generated artifact lint** (`dvandva lint artifacts`, `rust/dvandva/src/lint/artifacts.rs`): rejects generated Markdown under `./superpowers/`, requires generated HTML artifacts to declare dark color scheme, parses embedded Dvandva artifact metadata, and rejects external script/link references.
- **Wait-helper tests** (the wait port's `cargo test` suite): verifies `dvandva wait` exits 0 when a role is assigned, 10 on `done`, 11 on `human_decision`, 12 on `human_question` with resume fields, and 20 on timeout.
- **Installer tests** (the install and install-codex ports' `cargo test` suites): verify the dual Claude/Codex installer invokes both engine install paths and the Codex-only helper uses `codex plugin add` when available, with the app-server path preserved only as legacy fallback.
- **Plugin smoke test** (`dvandva smoke-install`, `rust/dvandva/src/smoke.rs`): copies the plugin into a temp marketplace, validates Claude plugin/marketplace metadata, runs Codex marketplace add with isolated `CODEX_HOME`, probes Codex runtime discovery after direct Codex plugin install, dual installer install, and dvandva install-codex helper install, requires `dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup`, checks the installed cache version, and checks standalone development copies.
- **Run 4 work-gate and retirement tests**: the commit-gate port's `cargo test` suite covers the repo-local `.dvandva/githooks` delegating hooks, `DVANDVA_ROLE`, `active_roles`, `Dvandva-Checkpoint`, installer idempotence, and drift lint; the retire port's `cargo test` suite (`rust/dvandva/src/retire.rs`) covers dry-run, five-symlink apply, manifest restore, cache parity refusal, Codex agent-axis no-op, and no skill touches; `dvandva lint run4-path-gates` plus `dvandva lint run4-standalone-agents` keep source docs, manifests, rust sources, and the 15-agent roster aligned with Run 4.
- **Pilot as integration test:** the pilot is the v1 integration test. Success criteria #1–#5 in section 2 are the acceptance gate.

## 14. Risks and open questions

Named risks, ordered by severity:

1. **Disagreement-cap mechanism untested.** If the cap doesn't fire correctly, two agents can lock into infinite counter-change loops. Mitigation: success criterion #5 is the gate; pilot must exercise the loop at least once and confirm it caps correctly.
2. **`/goal` evaluator misjudges baton state surface.** If Claude or Codex surfaces a baton-state line in a way the evaluator misreads, the loop may stop prematurely or fail to stop. Mitigation: skill bodies require the bounded `BATON_STATE_COMPACT` JSON summary from `dvandva state --compact` at every checkpoint, instead of full dynamic baton arrays.
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
- **Schema maintenance.** The v2 contract is copied across many hand-maintained files, all held in parity by `dvandva lint schema-parity` (S6-T1): the engine's own `dvandva.baton.v2` status enum and `v2_required_keys()` (`rust/dvandva/src/write.rs`); `plugins/dvandva/references/baton-schema-v2.json` (its `status_catalog`); the `Status catalog (22)` marker line in this spec (Appendix A) and in `plugins/dvandva/references/state-transition-table.md`; the inline `json` seed block in each of `plugins/dvandva/skills/vadi/SKILL.md` and `plugins/dvandva/skills/prativadi/SKILL.md` (top-level keys ≡ `v2_required_keys()`, one `json` fence per file); the byte-identical channel-doc copies `docs/protocol/local-baton-channel.md` and `plugins/dvandva/references/local-baton-channel.md`; and the HISTORICAL v1 references `plugins/dvandva/references/baton-schema.json` and `templates/channel/baton.json` (each carrying a `HISTORICAL: dvandva.baton.v1` marker). On a schema change, update the engine first, then every copy above, and run `dvandva lint schema-parity` and `dvandva lint skills` until green.
- **Policy fields in baton.** `allow_commit`, `allow_push`, and `allow_pr` intentionally live in the baton for v1 so every agent and transcript sees the run authority in the same file as state. `allow_commit` authorizes regular local checkpoint commits after verified logical slices; `allow_push` is still final-ship only. A separate `.dvandva/policy.json` is a v2 option if policy grows beyond these booleans.
- **Skill versions:** each SKILL.md may carry a `# Skill version: <semver>` comment in the body. Bumped on body changes that alter agent behavior.

## 16. Future work (v2 and beyond)

In priority order:

- **Deterministic validator script** + real JSON Schema at `templates/channel/baton.schema.json`. Skills invoke it as a pre-write gate. Rejects malformed batons and illegal transitions per the table in Appendix A. Closes the remaining schema-depth gap beyond the helper-level v1/v2 validation already in `dvandva write`.
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
- **Run 4 — generalized path-gate + retire standalones + work-gating.** Off-protocol commits are guarded by role-preflight-exported `DVANDVA_ROLE`, local git hooks, and a `dvandva.hooksAdoptedAt` drift-lint baseline that keeps stamped-sandwich bypasses visible. Generalized `work_split` path-gate enforcement extends beyond the Run 3 dynamic disjointness check, and the user's standalone agent fleet is retired only for Dvandva-covered workflows with functional parity via Runs 1-4 usage, backup-manifest reversibility, Codex agent-axis no-op behavior, and no skill touches.

## Appendix A — `dvandva.baton.v1` canonical schema and transitions

This appendix is the spec-level authoritative reference for the schema (including prativadi-only fields) and the v1 state-transition table. The template file at `templates/channel/baton.json` is a v1-aligned reference artifact that mirrors the schema shape but holds only the always-present fields; `vadi` does not depend on it at runtime (see section 7.2 preflight), so the template is reference-only for humans inspecting the repo.

### Schema

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "development | research | review | feature-pr (legacy alias for development)",
  "profile": "development-only lifecycle depth: fast | standard | full; missing existing development profiles are effective full",
  "profile_floor": "minimum allowed profile computed from risk inputs; downgrades below this require human_decision",
  "profile_decision": "object recording selected_profile, floor, reason, decided_by, decided_at, risk_inputs, hard_triggers, allowlist_match, allowlist_refs, and evidence_refs",
  "profile_history": "append-only array of profile change records: from, to, floor, checkpoint, actor_role, reason, evidence_refs",
  "run_mode": "walkaway | supervised",
  "phase": "research | spec | review | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase; engine-frozen once master_plan_locked is true (reason bad_amendment total_phases_frozen), changeable only inside an F7 amendment loop (amendment_from_phase non-null) or on a write into human_decision",
  "phase_profiles": "additive nullable object {\"<numeric phase>\": \"standard\" | \"full\"} (F9); effective profile of numeric phase N = phase_profiles[N] // run profile; set/changed only in spec states, and never below the per-phase hard-path floor (reason bad_phase_profiles); absent = every phase uses the run profile",
  "status": "research_drafting | research_review | research_revision | spec_drafting | spec_review | spec_revision | human_question | implementing | parallel_implementing | test_creation | cross_review | cross_fixing | deep_review | deslop | termination_review | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done | abandoned (22-token v2 catalog; see the Status catalog line above; abandoned is the S2-T1 terminal enterable only from human_question/human_decision)",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs include team for concurrent states",
  "active_roles": "v2 concurrent roles array, usually [] or [\"vadi\", \"prativadi\"]",
  "current_engine": "optional; \"claude\" | \"codex\" | null. Records which CLI wrote the most recent baton; for traceability only, not used for correctness.",
  "review_target": "research | spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "run_explainer_ref": "v2 path to gitignored final run explainer HTML under ./superpowers/run-reports/, required before terminal done for full-profile development mode",
  "run_explainer_reviews": "v2 array of role review records for the final run explainer; full-profile development done requires completed approved vadi and prativadi entries whose artifact_ref exactly equals run_explainer_ref, with non-empty summary and evidence_refs",
  "research_outcome": "nullable v2 field; accepted research result, including seed_development when a research run seeds a future development run",
  "review_ref": "nullable v2 path to gitignored generated HTML review artifact under ./superpowers/reviews/",
  "review_intake": "nullable v2 field carrying review-mode intake scope or selector",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, status, and artifact refs",
  "agent_instances": "v2 array recording generated run-scoped agent instances, provenance, model/permission class, read/write paths, closure evidence, output refs, and validation state",
  "subagent_tracks": "v2 array recording actual conditional parallelism tracks, owner, evidence refs, fallback rationale, and result",
  "verification_matrix": "v2 array/object mapping claims and risks to planned checks, owners, expected evidence, result, and evidence_ref",
  "master_plan_locked": "boolean; false during planning, true once prativadi advances to phase 1",
  "amendment_from_phase": "additive nullable number (F7); when non-null it records the numeric phase an in-progress plan-amendment loop returns to. May become non-null only on an amendment entry edge (full deslop -> spec_revision / standard phase_review -> spec_revision), is unchangeable mid-loop, and MUST be nulled on exit; while non-null the spec loop is legal post-lock and total_phases/phase_profiles may change (reason bad_amendment). Absent = null.",
  "question": "string | null; one concrete user question when status is human_question",
  "resume_assignee": "vadi | prativadi | null; role to resume after a human_question answer",
  "resume_status": "spec_drafting | spec_review | spec_revision | null; status to restore after a human_question answer",
  "disagreement_round": "integer, set to 0 by the agent that writes the first baton of each new phase; incremented by the agent that disagrees during mutual review",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "work_split_waiver": "S5-T3 additive nullable object gating the parallel/test-creation chunk floor: {reason: <non-blank string>, approved_by: \"prativadi\", checkpoint: <number>}. When valid it waives ONLY the >=5-total chunk floor; the per-role >=2 write-capable-chunk floor is never waivable. Any other present shape is rejected (bad_work_split_waiver). Absent = the >=5 floor is in force.",
  "loop_counts": "v2 additive map keyed \"<kind>:<phase>\" to a per-cycle counter for repeated review/fix loops; the write helper mandates increment-by-one on every loop-edge write (an absent counter reads as 0 but the candidate must still write the increment, so the cap cannot be bypassed by omission) and routes a counter that reaches disagreement_cap to human_decision. The counter resets on phase advance.",
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

### Allowed state transitions (accepted v2 modes plus legacy v1 fallback)

This spec is authoritative for the accepted v2 mode tables and the legacy v1
fallback. The protocol doc and plugin-local transition table carry the same
runtime contract.

Status catalog (22): research_drafting, research_review, research_revision, spec_drafting, spec_review, spec_revision, implementing, parallel_implementing, test_creation, cross_review, cross_fixing, deep_review, review_of_review, counter_review, deslop, termination_review, phase_review, phase_fixing, human_question, human_decision, done, abandoned

The engine's `dvandva.baton.v2` status enum, the `baton-schema-v2.json` `status_catalog`, the state-transition-table copy, and this line are held equal by `dvandva lint schema-parity` (S6-T1). `abandoned` (S2-T1) is the new terminal status; `done` and `abandoned` are the two terminal states.

**Development full profile (v2, 28 edges):**

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref` and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi surfaces research findings back to vadi. |
| `research_revision` | `research_review` | Vadi addresses research findings and hands back. |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Prativadi accepts research and starts development planning. |
| `spec_drafting` | `spec_review` | Vadi hands the development plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces planning Q&A back to vadi. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the plan. |
| `spec_review` | `phase: 1, status: "parallel_implementing"` | Prativadi accepts the plan, freezes `total_phases`, and activates both roles. |
| `phase: N, parallel_implementing` | `test_creation` | Both roles completed implementation chunks and recorded implementation evidence. |
| `test_creation` | `cross_review, review_target: implementation` | Vadi records tests/coverage evidence and hands to both roles for reciprocal peer-chunk review. |
| `cross_review` | `cross_fixing` | One or both reciprocal cross-review tracks found peer-owned chunk defects. |
| `cross_fixing` | `test_creation` | Cross-review findings were fixed and tests/evidence must be refreshed. |
| `cross_review` | `deep_review, review_target: implementation` | Both roles recorded completed approved cross-review tracks with evidence. |
| `deep_review` | `phase_fixing` | Prativadi finds bugs, missing tests, verification gaps, or substantive review issues. |
| `deep_review` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups after the required review angles; vadi must inspect the fixup diff before cleanup continues. F6: for a full-effective-profile phase this gate also requires a completed `security` angle (owner `dvandva-security-auditor`) when changed_paths ∪ current-phase work_split paths hit the `.env*`/secret/credential or api/client submatchers, and a completed `integration` angle (owner `dvandva-integration-checker`) when ≥2 distinct-owner write-capable chunks share a cross-owner `depends_on` or `conflict_group` (reason `bad_deep_review_angles`). |
| `deep_review` | `deslop` | Prativadi accepts behavior and tests after the required review angles are complete, including the F6 security/integration angles when their triggers fire (reason `bad_deep_review_angles`). |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves prativadi fixups and writes a counter-change. |
| `review_of_review (prativadi_fixups)` | `deslop` | Vadi approves prativadi fixups; v2 returns to cleanup rather than advancing or terminating directly. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves the counter and applies a different narrow fixup. |
| `counter_review (vadi_counter)` | `deslop` | Prativadi approves the counter; v2 returns to cleanup rather than advancing or terminating directly. |
| `phase_fixing` | `test_creation` | Vadi fixed behavior, tests, or verification gaps and must refresh test evidence. |
| `deslop` | `phase_fixing` | Cleanup finds behavior, test, or review blockers. |
| `deslop` | `phase: N+1, status: "parallel_implementing"` | A non-final phase is clean and the next development phase (whose effective profile is full) begins. |
| `deslop` | `phase: N+1, status: "implementing"` | F9 cross-profile advance: a clean full phase advances into a next phase whose effective profile is `standard` (the entry state is chosen by the target phase's effective profile). |
| `deslop` | `phase: "spec", status: "spec_revision"` | F7 amendment entry: the vadi opens a capped plan-amendment episode for a post-lock scope change — sets `amendment_from_phase` to the current numeric phase, lands `phase: "spec"`, assignee `vadi`, and increments loop key `plan_amendment:<from-phase>` (cap = `disagreement_cap`; at cap only `human_decision` is legal). |
| `deslop` | `termination_review` | The final development phase is clean; the run enters the shared stop-review gate (selected by the final phase's effective profile). |
| `termination_review` | `phase_fixing` | One role rejects final stop because behavior, tests, docs, or run artifacts still need work. |
| `termination_review` | final `done` | Both roles explicitly decide to stop, both approval bits are true, `run_explainer_ref` is set, `run_explainer_reviews` contains completed approved entries from both roles for that exact artifact, and (F10) a completed current-cycle `explainer-verification` track owned by `dvandva-doc-verifier` exists (reason `bad_explainer_verification`). |

F7 amendment loop (full): after the `deslop -> spec_revision` entry above, the existing `spec_revision -> spec_review` and `spec_review -> spec_revision` edges run post-lock while `amendment_from_phase` is non-null, and `total_phases`/`phase_profiles` MAY change during them. The loop exits via `spec_review -> parallel_implementing` (the existing spec→impl entry, chosen by the target phase's effective profile), which MUST null `amendment_from_phase` and re-enter at a numeric phase ≥ `amendment_from_phase`; the exit resets `loop_counts`, so the `plan_amendment:<from>` cap is per-episode.

**Development standard profile (v2 compact edges):**

Standard keeps research and spec review, then uses a compact implementation
path. It is the default for new development scaffolds when no hard-risk trigger
forces `full`.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref`, `profile_decision`, work split, and verification matrix. |
| `research_review` | `research_revision` | Prativadi finds research/profile gaps. |
| `research_revision` | `research_review` | Vadi addresses research/profile gaps. |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Research is accepted and the compact development plan can be drafted. |
| `spec_drafting` | `spec_review` | Vadi hands the plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces planning Q&A. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the plan. |
| `spec_review` | `phase: 1, status: "implementing"` | Prativadi accepts the plan and starts compact implementation. |
| `implementing` | `phase_review` | Vadi records implementation and verification evidence, including a profile-floor recheck. |
| `phase_review` | `phase_fixing` | Prativadi finds substantive issues. |
| `phase_review` | `implementing` | Prativadi requests additional implementation or verification without leaving compact profile. |
| `phase_review` | `review_of_review, review_target: prativadi_fixups` | S5-T1/D4: standard gains the same rarely-exercised mutual-review safety valve `full` has — prativadi applied narrow fixups and mutual review is owed (requires non-empty `narrow_fixups`). Loop-capped. |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | S5-T1: vadi disapproves the fixups and writes a counter-change. Loop-capped (`review_of_review<->counter_review`). |
| `review_of_review (prativadi_fixups)` | `phase_review` | S5-T1: vadi approves the fixups; standard returns to compact phase review rather than advancing directly. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | S5-T1: prativadi disapproves the counter and applies a different narrow fixup. Loop-capped. |
| `counter_review (vadi_counter)` | `phase_review` | S5-T1: prativadi approves the counter; standard returns to compact phase review. |
| `phase_review` | `phase: N+1, status: "parallel_implementing"` | F9 cross-profile advance: a clean standard phase advances into a next phase whose effective profile is `full` (the entry state is chosen by the target phase's effective profile). |
| `phase_review` | `phase: "spec", status: "spec_revision"` | F7 amendment entry (standard equivalent): the writer opens a capped plan-amendment episode for a post-lock scope change — sets `amendment_from_phase` to the current numeric phase, lands `phase: "spec"`, assignee `vadi`, and increments loop key `plan_amendment:<from-phase>` (cap = `disagreement_cap`). The loop exits via `spec_review -> implementing`, which MUST null `amendment_from_phase` and re-enter at a numeric phase ≥ `amendment_from_phase`; the exit resets `loop_counts`. |
| `phase_fixing` | `phase_review` | Vadi fixes and refreshes evidence. |
| `phase_review` | `termination_review` | Prativadi approves compact implementation/review evidence. |
| `termination_review` | `phase_fixing` | Either role rejects final stop. |
| `termination_review` | final `done` | Both approvals are true, `profile_decision` is valid, final verification is passing, `verification_matrix` evidence is complete, and prativadi `phase-review` evidence is completed approved with current-cycle `review_checkpoint`. No run explainer is required for standard. |

**Development fast profile (v2 allowlist edges):**

Fast is available only for allowlisted prose-only changes with positive
allowlist evidence, `profile_floor: "fast"`, and no hard-risk paths.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Optional fast research prelude records `research_ref`, `profile_decision`, allowlist evidence, work split, and verification matrix before compact implementation. |
| `research_review` | `research_revision` | Prativadi requests a research/evidence correction before fast implementation. |
| `research_revision` | `research_review` | Vadi refreshes the fast research package and returns to prativadi. |
| `research_review` | `implementing` | Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation. |
| `implementing` | `phase_review` | Vadi records verification evidence and profile recheck for allowlisted paths. |
| `phase_review` | `phase_fixing` | Prativadi finds issues. |
| `phase_fixing` | `phase_review` | Vadi fixes and refreshes evidence. |
| `phase_review` | `termination_review` | Prativadi approves the compact review and the profile still satisfies the floor. |
| `termination_review` | `phase_fixing` | Either role rejects final stop. |
| `termination_review` | final `done` | Both approvals are true, `profile_decision` is valid, final verification is passing, `verification_matrix` evidence is complete, and prativadi `phase-review` evidence is completed approved with current-cycle `review_checkpoint`. No run explainer is required for fast. |

**Research mode (v2, 12 edges):**

Research mode ends as research. It may optionally produce a seed-development
plan before termination; when it does, set `research_outcome:
"seed_development"` and populate `plan_ref`. Required phases are mode-specific:
`research_*` statuses use `phase: "research"`; seed-plan statuses,
`phase_fixing`, `termination_review`, and terminal `done` use `phase: "spec"`.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref` and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi surfaces research findings back to vadi. |
| `research_revision` | `research_review` | Vadi addresses research findings and hands back. |
| `research_review` | `spec_drafting` | Accepted outcome is `seed_development`; vadi must draft a seed plan before the research run can terminate. |
| `spec_drafting` | `spec_review` | Vadi hands the seed-development plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces seed-plan Q&A back to vadi. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the seed plan. |
| `spec_review` | `termination_review` | The seed-development plan is accepted; `plan_ref` is set and the research run enters shared stop review. |
| `research_review` | `termination_review` | The accepted research outcome is not `seed_development`; `research_ref` is sufficient to enter shared stop review. |
| `termination_review` | `phase_fixing` | Shared stop review finds research or seed-plan gaps and routes to a focused fixing pass. |
| `phase_fixing` | `research_review` | The focused fix is complete and the research package must be re-reviewed. |
| `termination_review` | final `done` | Both roles explicitly decide to stop; `research_ref` exists, and `plan_ref` also exists iff `research_outcome == seed_development`. |

**Review mode (v2, 10 edges):**

Review mode reuses the existing research statuses for intake and the existing
review statuses for the review package. Set `review_intake` and `review_target`
before the first `deep_review`, and produce `review_ref` before shared
termination. Every review-mode status uses `phase: "review"`. S4-T7 adds the
`deep_review -> phase_fixing` hand-back edge (loop-capped), taking the table from
9 to 10 edges.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes review scope/intake research and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi finds intake gaps and routes back to vadi. |
| `research_revision` | `research_review` | Vadi fixes intake gaps and hands back. |
| `research_review` | `deep_review` | Intake is sufficient and the review package can be evaluated. |
| `deep_review` | `phase_fixing` | S4-T7: the reviewer hands back substantive fixes (bugs, missing tests, verification gaps) without lapping the stop gate. Loop-capped (`deep_review->phase_fixing`). |
| `deep_review` | `deslop` | Substantive review passes and only cleanup or wording polish remains. |
| `deslop` | `termination_review` | Review cleanup is complete and `review_ref` is ready for the shared stop gate. |
| `termination_review` | `phase_fixing` | Shared stop review finds review-package work still owed. |
| `phase_fixing` | `deep_review` | Requested fixes or evidence refreshes are complete and the review must be rerun. |
| `termination_review` | final `done` | Both roles explicitly decide to stop and `review_ref` is set. |

**Legacy v1 explicit fallback:**

| From | To | Trigger |
|---|---|---|
| (legacy v1 explicit selection only, no baton) | `phase: "spec", status: "spec_drafting"` | Vadi creates the legacy v1 seed. |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi. |
| `spec_review` | `phase: 1, status: implementing` | Legacy v1 explicit path only: prativadi accepts plan and freezes `total_phases` on the baton. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and hands back. |
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Legacy v1 direct review path only. |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings. |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups and mutual review is owed. |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1 direct-review path only: prativadi approves, no changes. |
| `phase_review (impl)` | final `done` | Legacy v1 final phase approved by both roles; optional commit/push complete. |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings and re-hands. |
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1 direct-review path only: vadi approves prativadi fixups. |
| `review_of_review (prativadi_fixups)` | final `done` | Legacy v1 final phase approved by both roles after vadi approves prativadi fixups. |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter (`disagreement_round += 1`). |
| `review_of_review (prativadi_fixups)` | `human_decision` | `disagreement_round >= disagreement_cap`. |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1 direct-review path only: prativadi approves counter. |
| `counter_review (vadi_counter)` | final `done` | Legacy v1 final phase approved by both roles after prativadi approves counter. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves counter and applies a different fix (`disagreement_round += 1`). |
| `counter_review (vadi_counter)` | `human_decision` | `disagreement_round >= disagreement_cap`. |

**Shared intervention edges:**

| From | To | Trigger |
|---|---|---|
| planning state (`research_*`/`spec_*`) with `master_plan_locked: false`, OR a post-lock working state (`implementing`/`parallel_implementing`/`test_creation`/`cross_fixing`/`phase_fixing`) | `human_question` | S4-T5/D1: either agent routes one concrete requirement question to the human instead of guessing; `resume_status`/`resume_assignee` restore the exact prior state. Not a loop edge. |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields. |
| `human_question` | `abandoned` (`assignee: human`, `active_roles: []`) | S2-T1: the run is abandoned. Terminal, snapshot-archived like `done`, no artifact/approval/loop gates. |
| `human_decision` | `abandoned` (`assignee: human`, `active_roles: []`) | S2-T1: the run is abandoned. Terminal. |
| `abandoned` | — (no outgoing edge) | S2-T1: terminal; `dvandva wait` exits 13; the resolver/commit-gate/drift inactive set is `{done, abandoned}`. Reopen only via a hand-authored `human_decision` write. |
| any state | `human_decision` | Escalation (`disagreement_round >= cap`, `turn_cap` hit, blocker, malformed input). |
| `human_decision` | any mode-owned state chosen by the human | After human edits baton or prompts an agent. |

**Hardening gates layered onto these edges (S4/S5):**

- **Spec-entry lock (S4-T2/D2).** `spec_review -> implementing`/`parallel_implementing` requires candidate `master_plan_locked == true` (`bad_master_plan_locked`, 23). `master_plan_locked` `true->false` is rejected on every development edge except a write whose `new_status` is `human_decision`; amendment loops keep the lock.
- **Done-gate refs, matrix, and superset (S4-T1/T4/T6).** Every `done` candidate must resolve each required ref (`research_ref`, plus `plan_ref`/`run_explainer_ref`/`review_ref` per mode) to an existing non-empty file (`missing_artifact`); the `verification_matrix` must be complete with a numeric `evidence_checkpoint`/`review_checkpoint` at or after the last implementation-family checkpoint (`stale_verification_matrix`); and a team-owned candidate's `subagent_tracks`/`agent_instances`/`work_split` ids and `findings` must be a superset of the installed baton's (`lost_update`).
- **v1 write retirement (S5-T2/D5).** A `dvandva.baton.v1` write candidate — or a current baton still carrying `schema: "dvandva.baton.v1"` — is rejected with `schema_retired` and a migration hint to v2. The lenient READ path (`state`/`resolve`/`wait`/`brief`) is untouched, so old v1 batons stay observable.
- **Parallel-chunk floor + waiver (S5-T3).** `parallel_implementing` entry and `parallel_implementing -> test_creation` require `>=2` write-capable chunks per role AND (`>=5` total OR a valid `work_split_waiver`); a malformed waiver is `bad_work_split_waiver`.
- **Research-mode phase labels (S5-T5).** Research-mode `termination_review`/`phase_fixing`/`done` use phase `"research"` for exploratory runs and phase `"spec"` for seed runs (`research_outcome == seed_development` or `plan_ref` set). Candidates are strict; existing batons carrying the old `"spec"` label are accepted on the CURRENT side of a same-status/relabel transition (current-side leniency).
- **Post-install fence (S4-T10).** After the atomic rename, the write path re-verifies the lock; loss is `lock_lost_post_install` (exit 29) — the install DID happen and may be superseded, so the caller must re-read.

Any other transition is illegal in v1 or v2 and must be rejected by the
writing agent.

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
