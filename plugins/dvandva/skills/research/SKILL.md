---
name: research
description: Use when a Dvandva run is in research_drafting, research_review, or research_revision, or when a vadi/prativadi needs shared research, work distribution, verification planning, or independent research review before spec drafting.
---

# Dvandva Research

## Overview

Use this skill to turn the user's `original_ask` into source-backed preparation before planning or implementation. The output is a generated user-facing HTML artifact at `research_ref`, plus baton fields that let both agents work without rediscovering the same context.

## Research Artifact

Write research output as a generated user-facing HTML artifact:

- Path: `./superpowers/research/YYYY-MM-DD-<topic>.html`
- Format: dark self-contained HTML that renders offline and includes machine-readable metadata.
- Metadata: include `schema`, `run_id`, `original_ask`, `research_ref`, `work_split`, `verification_matrix`, source inventory, generated timestamp, and open questions.
- Source/platform Markdown files such as `SKILL.md`, command files, README/source docs, and protocol references remain Markdown; generated research reports do not.

## Baton Fields

Carry these fields forward on every baton:

| Field | Meaning |
|---|---|
| `original_ask` | Full user request and constraints, preserved from the first baton. |
| `research_ref` | Path to the generated HTML research artifact. |
| `work_split` | Planned responsibilities for vadi, prativadi, human, or subagents; include owner, scope, paths, status, and artifact refs. |
| `verification_matrix` | Planned evidence for claims and risks; include owner, phase, command or inspection, expected result, current result, evidence refs, and the 100% test coverage target for newly created behavior. |

`verification` remains the command log. `verification_matrix` is the coverage plan and evidence map. Test creation is separate from review: the doer creates or updates tests, then the reviewer independently evaluates sufficiency.

## Parallel Tracks

Use parallel subagents aggressively when tools are available. Default tracks:

- Codebase map: files, scripts, tests, and existing local conventions.
- Protocol/docs map: relevant product, protocol, README, skill, and command constraints.
- Verification map: tests/lints/manual checks needed to prove the work.
- Risk map: edge cases, conflicting requirements, stale references, and likely review failures.
- Work distribution: proposed owner and scope for each track or phase.
- Test creation: every new behavior, helper, schema path, or generated workflow needs an explicit test or lint entry. Source-only documentation gets a lint/review entry rather than executable coverage.
- Deep review: plan a `deep_review` pass after implementation, test_creation, and cross_review to hunt correctness bugs, stale wording, missed invariants, and low/minor issues.
- De-slop: plan a `deslop` pass to remove fuzzy wording, duplicated instructions, overbroad abstractions, stale examples, and generated-looking clutter before final approval.

If no subagent tool is available, do the same exploration directly and record the fallback in work_split.

Subagents are read-only during research by default. The main agent synthesizes the artifact, writes baton fields, and owns the handoff.

## Dvandva Agent Roster

Use the canonical Dvandva subagent roster under `plugins/dvandva/agents/` when the harness supports named subagents. These local roles are the source of truth for Dvandva; retired personal agent definitions from external skill repos should not be required.

Dvandva model classes are vendor-neutral. Agent frontmatter uses `model: opus` and `model: sonnet` as class labels, not Anthropic-only product IDs. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Do not use `haiku` for Dvandva subagents.

| Phase | Agent |
|---|---|
| `research_drafting` | `dvandva-researcher`, `dvandva-pattern-mapper`, `dvandva-architect`, `dvandva-baton-auditor` |
| `spec_drafting` | `dvandva-architect`, `dvandva-baton-auditor` |
| `parallel_implementing` / `implementing` | `dvandva-implementer`, optionally `dvandva-sandbox-verifier` |
| `test_creation` | `dvandva-test-creator`, `dvandva-sandbox-verifier` |
| `cross_review` / `cross_fixing` | `dvandva-cross-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| `deep_review` | `dvandva-deep-reviewer`, `dvandva-adversarial-analyst`, trigger-gated `dvandva-security-auditor`, `dvandva-integration-checker`, `dvandva-doc-verifier`, `dvandva-baton-auditor`, optionally `dvandva-sandbox-verifier` |
| `phase_fixing` | `dvandva-debugger` when root cause is unclear, `dvandva-implementer`, `dvandva-test-creator` |
| `deslop` | `dvandva-deslopper`, `dvandva-baton-auditor` |

This borrows the useful part of GSD-style fresh-context subagents and OMO-style team roles without adding a daemon, mailbox, or central runtime process. The baton still owns coordination.

## Dynamic agents (seed roster)

The seed roster in `plugins/dvandva/agents/` is the canonical source for generated run-scoped agent instances during research. When additional parallel tracks are needed, plan them in `work_split`, generate each brief from the seed roster (each brief satisfies the same agent contract as the static seed), record the instance in `agent_instances` on the baton, dispatch the harness subagent, and apply explicit closure: close the handle and record closure evidence in `agent_instances[].evidence_refs` and `agent_instances[].closed_at` before the track counts as completed. A closed generated instance must also carry non-empty `work_item_ids`. Outputs are serialized into one baton checkpoint via the single-writer rule. `seed_agent` is advisory provenance; executable validation is based on the generated instance id, `spawned_by`, parent role, lifecycle evidence, and track ownership.

Generated instances are run-scoped and ephemeral — no additive roster sprawl unless a later reviewed source change promotes the pattern into the seed roster.

Mandatory invariants:
- Coordination invariant: no daemon, no hidden orchestrator — the baton is the only coordinator.
- Single-writer: generated agents never own `assignee`, `active_roles`, phase transitions, or final approval.
- Path invariant: dynamic write-path disjointness — generated instances with non-empty `write_paths` sharing the same `base_checkpoint`, or any two live (`planned`/`running`) instances regardless of base_checkpoint, must be pairwise disjoint unless explicitly serialized through `depends_on` within a shared `conflict_group`; closed instances from an earlier base_checkpoint do not block later sequential reuse.
- Model-class mapping: use `opus-class|gpt-5.5` for review, planning, and architecture seeds; use `sonnet-class|gpt-5.4` for implementation and documentation seeds. Never use `haiku`.

## Absorbed Dvandva skills

These skills are available within the Dvandva run context. Use each only when its trigger applies; none is mandatory on every run.

- **`dvandva:testing`** — use during `test_creation` track planning to define coverage targets and populate `verification_matrix` with required test entries for new behavior.
- **`dvandva:understanding`** — invoke when the human asks to understand the run, its code, or its decisions during any phase. Teaching is mastery-gated and grounded in the active baton, diff, `research_ref`, and `plan_ref`.
- **`dvandva:worktree-setup`** — invoke when a run needs an isolated git worktree before starting implementation. Uses the generic core profile by default; apply the DeFi profile when working in defi-com repos.

## Mode Contracts

### research_drafting

The vadi runs research first for a named v2 run:

1. Re-read `original_ask` and repo instructions.
2. Dispatch parallel subagents or perform the same tracks directly.
3. Create or update `research_ref` as the HTML artifact.
4. Populate `work_split` and `verification_matrix`, including `test_creation`, `deep_review`, and `deslop` entries.
5. Hand to prativadi with `phase: "research"`, `status: "research_review"`, `assignee: "prativadi"`, `review_target: "research"`.

### research_review

The prativadi performs independent research review. Do not rely solely on the vadi's research_ref.

1. Re-read `original_ask`.
2. Open `research_ref`.
3. Independently inspect relevant code, docs, tests, and local commands.
4. Use parallel subagents when available.
5. Compare independent findings against `research_ref`, `work_split`, `subagent_tracks`, and `verification_matrix`.
6. Confirm test creation is separate from review and that new code/behavior has a 100% test coverage plan or an explicit documented reason why executable coverage is impossible.
7. If gaps remain, write `findings` and route to `research_revision`.
8. If research is sufficient, advance to `phase: "spec", status: "spec_drafting"`, `assignee: "vadi"`, preserving `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref`.

### research_revision

The vadi addresses prativadi research findings:

1. Read every finding.
2. Re-run targeted research tracks or subagents as needed.
3. Update the HTML artifact, `work_split`, `subagent_tracks`, and `verification_matrix`.
4. Clear resolved findings and hand back to `research_review`.

## Common Mistakes

| Mistake | Correction |
|---|---|
| Treating research as prose in `summary` | Write `research_ref`, `work_split`, and `verification_matrix`. |
| Letting prativadi only rubber-stamp the artifact | Require independent research review against sources. |
| Claiming unavailable subagents were used | Record the direct fallback in `work_split`. |
| Writing generated research as Markdown | Generated human-facing research is HTML; source/platform docs remain Markdown. |
| Starting implementation from research | Research must feed spec drafting and verification planning before implementation. |
| Combining tests and review | Keep `test_creation` and `deep_review` as separate responsibilities. |
| Shipping low-quality residue | Run `deslop` until nits, low/minor bugs, and stale wording are fixed or explicitly accepted in `deferred`. |
