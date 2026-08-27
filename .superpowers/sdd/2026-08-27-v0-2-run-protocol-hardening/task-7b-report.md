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
3. Reverse casting/publication: the original result is **RETRACTED**, not a
   PASS. `task-7b-scenario-3-prompt.md` fabricated an impossible checkpoint
   binding with `identity` where `CheckpointBinding` requires
   `checkpoint_identity`; `task-7b-scenario-3-response.md` therefore is not
   valid pressure evidence. Both ignored files remain preserved as historical
   evidence of the invalid exercise. A corrected fresh rerun is recorded
   below.
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

No further rationalization appeared in the scenario 2 fix, so scenarios 1 and
4 were not rerun. Scenario 3 was rerun later for the independent invalid-field
finding recorded below.

## Review fix round 1 — executable role contracts

The review correctly found that source-member grep had accepted incomplete and
invalid payload examples. Before changing role/setup prose, focused per-role
and executable tests produced these RED results.

Both per-role source contracts lacked required executable detail:

```text
vadi contract omitted "canonical deliverable IDs exactly once"
prativadi contract omitted "upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION"
test result: FAILED. 0 passed; 2 failed
```

The action-map test proved both references omitted `resume_human_decision` and
`finalize` payloads:

```text
assertion `left == right` failed: vadi action map is incomplete
left: {"accept_checkpoint_supersession", "record_explainer_publication",
"record_explainer_review", "record_review", "request_checkpoint_supersession",
"request_human_decision", "submit_checkpoint", "withdraw_approval"}
right: {"accept_checkpoint_supersession", "finalize",
"record_explainer_publication", "record_explainer_review", "record_review",
"request_checkpoint_supersession", "request_human_decision",
"resume_human_decision", "submit_checkpoint", "withdraw_approval"}
```

The per-role kernel-transition test rejected the documented one-option Human
Decision payload before mutation:

```text
assertion failed: action["options"].as_array().unwrap().len() >= 2
test documented_human_decision_payload_transitions_for_each_role ... FAILED
```

The setup source test also failed because the docs presented an unpublished
target as an available release:

```text
setup contract omitted "source and planned release target"
test setup_skill_sources_pin_v2_without_implicit_run_migration ... FAILED
```

Finally, the shell source contract stopped independently on vadi's missing
goal boundary:

```text
grep -Fq 'Dvandva never creates, replaces, pauses, completes, or clears any harness goal.' skills/vadi/SKILL.md
STATUS=1
```

The smallest source changes then made every advertised route executable:

- each role copies current checkpoint, obligation, and deployment coordinates
  from the facade rather than hardcoding an identity, kind, revision, or scope;
- both Human Decision examples provide two options and map `answer_human` to
  `resume_human_decision`;
- `finalize`, explainer publication, and explainer review map to their exact v2
  actions;
- `upgrade_required` documents the facade upgrade invocation, fresh claim, and
  later expired-claim recovery route; and
- each role independently forbids creating, replacing, pausing, completing, or
  clearing harness goals. Goals supplied by the user at launch remain outside
  the protocol.

The executable tests deserialize normalized forms of every documented action,
transition both roles' Human Decision examples, and materialize the documented
checkpoint, publication, explainer-review, semantic-review, and finalize
templates through the kernel flow. This catches schema member errors such as
`identity` in a `CheckpointBinding`, which text grep alone did not catch.

### Review-fix GREEN evidence

- `cargo test --manifest-path v4/Cargo.toml --test skill_flow` — 11 passed.
- `bash tests/skills/role-skills.sh` — `role skill wrappers: ok`.
- `cargo test --manifest-path v4/Cargo.toml --all-targets` — 168 passed,
  0 failed.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings`
  — passed.
- `bash -n skills/vadi/scripts/dvandva-role.sh \
  skills/prativadi/scripts/dvandva-role.sh \
  skills/setup-dvandva/scripts/setup-dvandva.sh` — passed.
- `git diff --check` — passed.

Setup now states durable release-boundary truth: `0.2.0` is this source's
release target. At invocation, the installer resolves the requested tag and
asset and fails closed until both exist; the docs do not claim that the
unpublished release is currently available.

### Corrected scenario 3 pressure evidence

A fresh `fork_turns: none`, read-only agent received only the amended
prativadi skill/reference and
`task-7b-scenario-3-corrected-prompt.md`. Its verbatim response is
`task-7b-scenario-3-corrected-response.md` — PASS. The corrected snapshot used
`checkpoint_identity` in its exact receipt. The agent kept Codex as publisher,
Claude as exact-deployment reviewer, rejected a Claude Artifact, copied the
revision 27/scope 4 obligation exactly, and blocked semantic approval until a
fresh legal action follows Claude's recorded deployment review.

## Review fix round 2 — role-owned actions and scope resume

Before editing either role contract, the new focused source and transition
tests failed together:

```text
prativadi contract omitted "new human scope, ambiguity, or unavailable mandated publication/review capability"
vadi contract omitted "new human scope, ambiguity, or unavailable mandated publication/review capability"
assertion failed: vadi action map is not role-specific
vadi omitted documented scope-amending resume payload
test result: FAILED. 0 passed; 4 failed
RUST_STATUS=101 SHELL_STATUS=1
```

The tests now reject either old contradictory Human Decision restriction even
if the new sentence is also present. Both concise skills and both references
allow the exception only for new human scope, ambiguity, or unavailable
mandated publication/review capability.

Each reference retains a plain-answer resume and adds an executable
`resume_human_decision` with `scope_amendment`. Its objective, objective refs,
nullable task reference, and deliverables must come only from explicit
human-approved values; the Human Decision object is not described as returning
them. Per-role flow tests request and resume the decision, then verify the
canonical amended scope, revision increment, `revising`/worker routing, and
fresh `scope_amended` obligation.

Documented JSON inventories are now exact and role-owned. Vadi documents its
worker actions; prativadi documents its reviewer actions; both retain the
Codex/Claude harness-specific explainer actions. Peer-owned payload examples
were removed, and prativadi's start synopsis no longer advertises
`--new-run`.

### Round-2 GREEN evidence

- `cargo test --manifest-path v4/Cargo.toml --test skill_flow` — 12 passed.
- `bash tests/skills/role-skills.sh` — `role skill wrappers: ok`.
- `cargo test --manifest-path v4/Cargo.toml --all-targets` — 169 passed,
  0 failed.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check` — passed after the
  formatter identified and corrected one wrapped test expression.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings`
  — passed.
- `bash -n skills/vadi/scripts/dvandva-role.sh \
  skills/prativadi/scripts/dvandva-role.sh \
  skills/setup-dvandva/scripts/setup-dvandva.sh` — passed.
- `git diff --check 0e5b695..HEAD` — passed.

The private kernel intentionally does not resolve Git objects or call the
Codex Sites API. Its accepted v0.2 trust boundary is typed external references,
structural exact-binding checks, and two harness-derived attestations. No live
deployment is claimed. Task 8 should make this trust boundary explicit in its
ADR/release documentation; it is not a Task 7B kernel or facade change.
Independent role references remain near-duplicates by design so each skill is
self-contained when installed.

## Commits

- `e48fb83 test(v4): define active role contract`
- `48422d6 docs(v4): harden role run contracts`
- `7f78bbb docs(setup): pin v0.2 skill release`
- `57eab12 fix(v4): keep run creation worker-owned`
- `6da048b docs(sdd): record task 7b evidence`
- `f997ded test(v4): execute documented role actions`
- `709b51a fix(v4): make role action contracts executable`
- `ffd4331 docs(setup): mark v0.2 as release target`
- `docs(sdd): record task 7b review fixes` (this report commit)
- `3e70336 test(v4): enforce role-owned action contracts`
- `a179b74 fix(v4): scope role action contracts`
- `12c7018 style(v4): format role contract test`
- `docs(sdd): record task 7b review fix round 2` (this report commit)

## Boundary and next action

No kernel, facade, installer implementation, workflow documentation, package
automation, v3 archive, or harness goal changed. No push, tag, or release was
performed. The JSON examples intentionally retain facade-copy placeholders;
tests normalize those placeholders before deserialization and transition.
No live Codex Sites deployment was performed or claimed. The external-reference
trust boundary above remains for Task 8 documentation; it is not a release
blocker under the accepted design. No blocker remains. The reviewer owns the
next action. Run:

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
