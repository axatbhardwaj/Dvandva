# Task 7B report: active role contracts

## Status

DONE

## RED evidence preserved before prose changes

Only `tests/skills/role-skills.sh` and `v4/tests/skill_flow.rs` had changed when
these failures were recorded. No role skill, reference, entry prompt, or setup
document had been edited.

The focused role-source contract failed at the first missing activation
guarantee:

```text
running 1 test

thread 'active_role_skill_sources_define_the_complete_v2_contract' ... panicked at tests/skill_flow.rs:69:9:
role contract omitted "first user-visible protocol output"
test active_role_skill_sources_define_the_complete_v2_contract ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out
```

The focused setup-source contract proved the released documentation was still
pinned to v0.1.1:

```text
running 1 test

thread 'setup_skill_sources_pin_v2_without_implicit_run_migration' ... panicked at tests/skill_flow.rs:136:9:
setup contract omitted "0.2.0"
test setup_skill_sources_pin_v2_without_implicit_run_migration ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out
```

After adapting the existing shell fixture to Task 7A's minimal v2 Human
Decision payload, the wrapper behavior completed through migration and then
the new source contract failed on the same first missing phrase:

```text
+ grep -Fq '"status": "human_decision"'
+ grep -Fq '"outcome": "upgrade_required"'
+ grep -Fq '"schema": "dvandva.run.v2"'
+ grep -Fq '"status": "revising"'
+ grep -Fq 'first user-visible protocol output'
SHELL_STATUS=1
```

The fixture adaptation removed the obsolete caller-supplied `contact_role`,
`resume_status`, and `resume_assignee` fields. Before that adaptation, the
current v2 kernel correctly rejected the old payload with:

```text
{"error":"invalid_baton","message":"invalid baton JSON: unknown field `contact_role`, expected one of `question`, `evidence`, `options`"}
```

## GREEN evidence

- `cargo test --manifest-path v4/Cargo.toml --test skill_flow` — 8 passed.
- `bash tests/skills/role-skills.sh` — `role skill wrappers: ok`.
- `bash -n skills/vadi/scripts/dvandva-role.sh \
  skills/prativadi/scripts/dvandva-role.sh \
  skills/setup-dvandva/scripts/setup-dvandva.sh` — passed.
- `cargo test --manifest-path v4/Cargo.toml --all-targets` — 165 passed,
  0 failed.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings`
  — passed.
- `git diff --check` — passed.

The role skills now keep only activation, the authoritative snapshot loop, and
boundaries. Their references own the complete v2 action shapes and state
details. The setup skill and installation reference name release
`skills-v0.2.0`, kernel `0.2.0`, write schema `dvandva.run.v2`, facade API 2,
explicit-only v1 migration, and setup's non-migration boundary.

## Pressure tests

Four initial agents were fresh `fork_turns: none` contexts. Each was instructed
to read only one amended `SKILL.md`, its `references/run-contract.md`, and one
hypothetical-snapshot prompt. All work was read-only. Prompts and verbatim
responses are ignored under `private-artifacts/pressure-tests/`:

1. Stale checkpoint/new work:
   `task-7b-scenario-1-prompt.md` and
   `task-7b-scenario-1-response.md` — PASS. The agent froze B, refused
   publication as a substitute, applied `request_checkpoint_supersession`,
   assigned acceptance to the reviewer, and deferred work until a fresh
   snapshot authorized `work`.
2. Exact-run scope conflict:
   `task-7b-scenario-2-prompt.md` and
   `task-7b-scenario-2-response-first.md` — NEEDS TIGHTENING. The agent
   correctly refused claim/review, but its requested-B path said verbatim,
   "Start without `--run-id` ... and `--new-run`," leaving the acting
   prativadi's authority ambiguous.
3. Reverse casting/publication:
   `task-7b-scenario-3-prompt.md` and
   `task-7b-scenario-3-response.md` — PASS. Codex remained publisher, Claude
   remained exact-deployment reviewer, the Claude Artifact was rejected, and
   semantic approval stayed blocked on the exact Site review.
4. Existing user-created goals:
   `task-7b-scenario-4-prompt.md` and
   `task-7b-scenario-4-response.md` — PASS. Both goals remained unchanged,
   the explicitly invoked third-party skill was scoped to authorized work,
   and the first protocol output and five-part handoff were complete.

For scenario 2, a new source assertion failed before the wording fix:

```text
thread 'active_role_skill_sources_define_the_complete_v2_contract' ... panicked at tests/skill_flow.rs:77:5:
assertion failed: prativadi.contains("Prativadi never creates a run.")
test active_role_skill_sources_define_the_complete_v2_contract ... FAILED
```

The smallest fix states that prativadi never creates a run and returns a
separate-run choice to the human so vadi/worker can create it. Only scenario 2
was rerun, with another fresh read-only agent. Its verbatim response is
`task-7b-scenario-2-response-rerun.md`; it passed by stating:

```text
Requested B: prativadi does not create the run. The human must start a
separate vadi/worker harness session ...
```

No further rationalization appeared, so scenarios 1, 3, and 4 were not rerun.

## Commits

- `e48fb83 test(v4): define active role contract`
- `48422d6 docs(v4): harden role run contracts`
- `7f78bbb docs(setup): pin v0.2 skill release`
- `57eab12 fix(v4): keep run creation worker-owned`

## Boundary and next action

No kernel, facade, installer implementation, workflow documentation, package
automation, v3 archive, or harness goal changed. No blocker remains. The
reviewer owns the next action. Run:

```bash
git diff 0e5b695..HEAD -- \
  skills/vadi/SKILL.md skills/vadi/references/run-contract.md \
  skills/vadi/agents/openai.yaml \
  skills/prativadi/SKILL.md skills/prativadi/references/run-contract.md \
  skills/prativadi/agents/openai.yaml \
  skills/setup-dvandva/SKILL.md \
  skills/setup-dvandva/references/installation.md \
  tests/skills/role-skills.sh v4/tests/skill_flow.rs \
  .superpowers/sdd/2026-08-27-v0-2-run-protocol-hardening/task-7b-report.md
```
