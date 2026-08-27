# Dvandva v0.2 Run Protocol Hardening Implementation Plan

> **Required subskill:** Execute this plan with
> `superpowers:subagent-driven-development`. Use
> `superpowers:test-driven-development` for every behavior change,
> `superpowers:writing-skills` and `writing-for-agents` for role-skill changes,
> `superpowers:requesting-code-review` for the final review, and
> `superpowers:verification-before-completion` before commits or completion
> claims.

**Spec:**
`docs/superpowers/specs/2026-08-27-v0-2-run-protocol-hardening-design.md`

**Goal:** Release a migration-safe v0.2 skill/kernel contract that binds one
complete checkpoint, canonical scope, Codex Sites publication, and Claude
explainer review at every run handoff while keeping both harnesses autonomous.

**Architecture:** Introduce a real `dvandva.run.v2` write epoch and role API 2.
Keep the Rust kernel authoritative for all state, bindings, ownership, and next
actions. Upgrade v1 through one explicit CAS migration that fences old claims.
Keep the three skills thin: they follow kernel snapshots and never manage
harness goals or invoke the peer harness.

**Technology:** Rust 2021, Clap, Serde, SHA-256, Notify, POSIX shell, GitHub
Actions, Agent Skills CLI, Codex Sites as an externally recorded deployment.

---

## Global constraints

- Work only in `v4/`, root `skills/`, active v4 docs, and v4 release tests.
  Do not modify `rust/`, `plugins/dvandva/`, or archived v3 behavior.
- Use `dvandva.run.v2`, kernel version `0.2.0`, facade API `2`, and release tag
  `skills-v0.2.0` exactly.
- Preserve every existing v1 history byte; migration appends one v2 revision.
- V1 is read-only in v0.2 except the dedicated upgrade CAS. Ordinary claim,
  heartbeat, wait, and semantic apply must return `migration_required`.
- Every role-facing command requires facade API 2 before reading or mutating a
  run. The probe's compatible write schema is v2 even though the kernel can
  inspect/migrate v1.
- Neither harness invokes the other. Dvandva never creates, updates, pauses,
  replaces, completes, or clears a harness goal.
- Default publication is one owner-only Codex Site per run, published by the
  Codex-harness participant and reviewed by the Claude-harness participant,
  regardless of semantic role.
- No Claude Artifact or generic local/hosted fallback satisfies publication.
- All new semantic bindings are kernel-derived, trim-validated, and SHA-256
  bound. A caller never supplies a trusted digest, scope revision, or harness.
- Use failing public/black-box tests before production changes. Verify each
  regression test red against the pre-fix behavior and green after the fix.
- Keep commits semantic and granular, targeting roughly 200 changed lines when
  practical. Use `gh stack`; do not push, open a PR, merge, or publish without
  separate user authorization.

## Shared interfaces

Later tasks consume these names; change them only with a ledger ruling against
the spec.

```rust
pub const LEGACY_SCHEMA: &str = "dvandva.run.v1";
pub const SCHEMA: &str = "dvandva.run.v2";
pub const ROLE_API: u64 = 2;

pub struct CheckpointSubmission {
    pub kind: String,
    pub identity: String,
    pub deliverables: Vec<CheckpointDeliverable>,
    pub verification: Vec<String>,
}

pub struct DeliverableRequirement {
    pub id: String,
    pub description: String,
}

pub struct CheckpointDeliverable {
    pub id: String,
    pub artifact: ExternalRef,
}

pub struct CheckpointBinding {
    pub identity: String,
    pub manifest_digest: String,
    pub scope_revision: u64,
}

pub struct PendingSupersession {
    pub checkpoint: CheckpointBinding,
    pub reason: String,
    pub requested_at_revision: u64,
}

pub struct PublicationBinding {
    pub handoff_revision: u64,
    pub handoff_kind: String,
    pub scope_revision: u64,
    pub checkpoint: Option<CheckpointBinding>,
}
```

Every v2 run has a non-empty `scope_deliverables: Vec<DeliverableRequirement>`
with unique IDs. Checkpoint manifest IDs must cover that set exactly.

`Publication` must deserialize both the legacy numeric object and the v2
contract without treating them as interchangeable. A v2 state holds policy,
pending binding, optional exact deployment, and optional exact Claude review.

The common role output is a `RoleSnapshot` containing the full baton plus
`next_actions` and optional blocking reason. `StartedRole` additionally
contains disposition, run path, credential path when claimed, and a peer
prompt. Exact-run non-start outcomes carry the same canonical run summary.

---

## Task 1: Prove the current skill failure under pressure

**Files:**

- Create (git-ignored SDD evidence): task report and baseline transcripts only
- Do not modify production or skill files

### Step 1: Run fresh-agent baseline scenarios

Give a fresh agent the current `skills/vadi/` and `skills/prativadi/` contract
plus each scenario, without the desired answer:

The preserved baseline transcript records the two role responses for scenario
1 only. Keep that evidence unchanged, and run scenarios 2--4 during this task
so the Task 1 report contains one prompt/response/invariant record for each
of all four scenarios. Do not represent the preserved two-response transcript
as evidence for the other three scenarios.

1. A selected exact run is `reviewing/reviewer`; checkpoint A omits a newly
   emphasized deliverable B; the vadi has drafted B; the user asks whether to
   launch prativadi.
2. An exact run ID exists with canonical objective A, while the invocation
   supplies objective B.
3. Semantic roles are reversed: Claude vadi and Codex prativadi must decide
   who publishes and who reviews the explainer.
4. Both prompts contain a user-created harness goal; the role is asked to join
   without changing that goal.

Record each prompt, agent response, and the violated/preserved invariant in the
Task 1 report. Expected baseline failures are stale-checkpoint approval risk,
silent exact-run scope adoption, worker-owned publication, or ambiguous goal
interference. A scenario may pass; record evidence rather than forcing failure.

### Step 2: Preserve the baseline

Do not edit the current skill during this task. Commit only the design and
implementation-plan documents so the baseline remains external evidence.

```bash
git add docs/superpowers/specs/2026-08-27-v0-2-run-protocol-hardening-design.md \
  docs/superpowers/plans/2026-08-27-v0-2-run-protocol-hardening.md
git commit -m "docs(v4): specify run protocol hardening"
```

## Task 2: Add the v2 epoch and one-way migration

**Files:**

- Modify: `v4/src/model.rs`
- Modify: `v4/src/store.rs`
- Modify: `v4/src/claim.rs`
- Modify: `v4/src/wait.rs`
- Modify: `v4/src/transition.rs`
- Modify: `v4/src/discovery.rs`
- Modify: `v4/src/credential.rs`
- Modify: `v4/src/role_session.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/src/main.rs`
- Modify: `v4/tests/run_channel.rs`
- Modify: `v4/tests/discovery.rs`
- Modify: `v4/tests/credential.rs`
- Modify: `v4/tests/role_session.rs`

### Step 1: Write migration RED tests

Add black-box coverage proving:

- fresh initialization writes v2;
- v2 initialization rejects missing, blank, or duplicate required deliverable
  declarations;
- probe reports `write_schema`, `read_schemas`, `role_api`, version, and a v1
  migration capability; a mismatched expected schema/API exits nonzero;
- a v1 run is classified `upgrade_required`, not corrupt or matchable;
- every ordinary v1 role mutation is `migration_required`;
- upgrade appends v2 without changing old history bytes, clears checkpoint,
  review, publication, human decision, and both claims, and records migration
  evidence;
- terminal v1 refuses upgrade;
- a live same-role claim owned by another session makes upgrade busy;
- an already-written v2 migration history head is recoverable after a missing
  or corrupt baton head;
- v1→v2→v2 history validates, while v2→v1, multiple crossings, and recovery
  from a pre-upgrade revision after v2 are rejected;
- stale credentials cannot authorize, but the same session can claim after
  the upgrade fence.

Run the focused test filters against the current kernel and preserve their
failing output in the task report.

### Step 2: Implement schema-aware persistence

Add explicit legacy/current schema constants and `ROLE_API`. Validate supported
schemas on read. Make CAS allow only same-schema writes or exactly one
v1→v2 upgrade. Validate monotonic history and forbid recovery across the
migration boundary. Keep `write_history` before `install`.

### Step 3: Implement dedicated upgrade

Add an atomic upgrade operation rather than routing migration through ordinary
semantic actions. Validate role/harness/session ownership, reject a live
foreign same-role claim, clear both claims and all active v1 semantic state,
preserve a typed legacy-state digest/provenance, create a
`protocol_upgraded` pending publication binding, and route to
`revising/worker`. Initialize canonical v2 scope with exactly one required
`legacy_objective` deliverable whose non-blank description is the canonical
objective summary; require a later scope amendment before separately votable
legacy outcomes are represented.

Make stale credential replacement conditional on the current baton proving
that the stored credential no longer matches an active claim.

### Step 4: Fence all other v1 paths and role APIs

Before an ordinary claim, reclaim, heartbeat, wait, or transition writes,
require v2. Require facade API 2 on every nested `role` command and before run
I/O. Exact v1 start returns migration metadata without a credential.

### Step 5: Verify and commit in granular slices

```bash
cargo test --manifest-path v4/Cargo.toml --test run_channel migration
cargo test --manifest-path v4/Cargo.toml --test discovery upgrade
cargo test --manifest-path v4/Cargo.toml --test role_session migration
cargo test --manifest-path v4/Cargo.toml --test credential stale
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
```

Use separate semantic commits for model/store epoch, upgrade/fencing, and
role-facing migration output if each slice is independently green.

## Task 3: Bind scope, complete checkpoints, and supersession

**Files:**

- Modify: `v4/src/model.rs`
- Modify: `v4/src/action.rs`
- Modify: `v4/src/transition.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

### Step 1: Write transition RED tests

Cover:

- empty deliverable manifests and blank manifest entries are rejected;
- the kernel computes a deterministic manifest digest and stamps the current
  scope revision;
- semantic review must bind identity, digest, and scope revision;
- duplicate checkpoint identity is rejected across rounds;
- human scope amendment increments scope revision, updates objective/refs,
  clears checkpoint/review/supersession, and routes to `revising/worker`;
- a pending supersession blocks approval;
- reviewer acceptance returns ownership without mutating the old checkpoint
  history;
- approval-first/request-first CAS races have the specified outcomes;
- worker withdrawal from `finalizing` reopens revision;
- terminal mutations remain rejected.

Run focused tests and capture the RED evidence before implementation.

### Step 2: Add canonical bindings

Introduce checkpoint submission/binding types and a canonical digest helper.
Trim persisted manifest and verification values once at the transition
boundary. Store the complete checkpoint plus kernel-derived digest/scope.
Extend review receipts and validation to bind all coordinates.

### Step 3: Add scope amendment and supersession actions

Extend `ResumeHumanDecision` with an optional objective/reference/required-
deliverable amendment and optional publication-policy override, both accepted
only through the human contact path. Add request/accept supersession and
approval withdrawal actions with non-blank reasons and exact ownership checks.

### Step 4: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test run_channel checkpoint
cargo test --manifest-path v4/Cargo.toml --test run_channel scope
cargo test --manifest-path v4/Cargo.toml --test run_channel supersession
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
```

Commit checkpoint/scope and supersession as separate recoverable commits when
the test boundaries allow it.

## Task 4: Replace numeric publication with the exact Sites/Claude gate

**Files:**

- Modify: `v4/src/model.rs`
- Modify: `v4/src/action.rs`
- Modify: `v4/src/transition.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

### Step 1: Write publication RED tests

Add a black-box matrix proving:

- new runs begin with a pending `run_started` binding;
- first checkpoint, semantic review, revised checkpoint, and finalization are
  each blocked until their current rolling obligation is approved;
- each semantic handoff produces the correct kind/revision/scope/checkpoint
  binding without making publication actions recursively stale;
- only the Codex-harness participant can record the default deployment;
- only the Claude-harness participant can review it, in normal and reversed
  semantic casting;
- publication requires a valid source SHA-256, stable Site ID, non-blank Site
  version and URL, `codex_sites`, and `owner_only`;
- deployment/review payloads with any stale binding coordinate are rejected;
- republishing clears Claude approval;
- changes requested require findings and do not reopen semantic review;
- a mutable URL or legacy `record_publication` action cannot satisfy v2;
- a human policy override names two distinct existing harnesses and creates a
  fresh obligation;
- finalization requires current checkpoint, semantic approval, deployment,
  and Claude approval all bound exactly.

### Step 2: Implement publication state and helpers

Deserialize legacy and v2 publication objects distinctly. Add default policy,
obligation, deployment, and explainer-review receipt types. Implement helpers
to create a handoff binding, compare full bindings, derive the caller harness,
validate digests/refs, and decide whether the rolling gate is approved.

### Step 3: Apply the gate to transitions

Create new obligations at the transition's resulting revision for every kind
listed in the spec. Do not create a new obligation when recording deployment
or explainer review. Require gate approval before the next cross-handoff
semantic mutation. Preserve the Site ID across deployments under the default
policy.

### Step 4: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test run_channel publication
cargo test --manifest-path v4/Cargo.toml --test run_channel reverse_casting
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
```

Commit the data model/helpers before the transition-gate commit if both are
independently testable.

## Task 5: Return canonical role snapshots and fail fast on exact runs

**Files:**

- Create: `v4/src/next_action.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/src/discovery.rs`
- Modify: `v4/src/role_session.rs`
- Modify: `v4/src/wait.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/discovery.rs`
- Modify: `v4/tests/role_session.rs`
- Modify: `v4/tests/skill_flow.rs`

### Step 1: Write snapshot/discovery RED tests

Prove:

- vadi creation output immediately includes run ID, canonical objective,
  scope, status, assignee, next actions, and exact copyable prativadi prompt;
- exact join needs no caller objective and returns the canonical one;
- a differing supplied objective returns `scope_mismatch` without claim or
  revision change;
- exact missing and exact busy return immediately even when `--wait` is set;
- start/read/apply/wait expose the same current `next_actions` vocabulary;
- next actions combine semantic and harness duties under both castings;
- a publication-blocked semantic action has a concise blocking reason;
- wait returns immediately when an action is already legal at the current
  revision, but continues waiting when the only action is `wait`;
- the stale-checkpoint incident routes to supersession/withdrawal and never
  advertises a legal stale approval.

### Step 2: Centralize action derivation

Implement one pure `next_action` module. Give it the baton, semantic role, and
derived participant harness. Keep advisory domain actions (`work`,
`review_checkpoint`) distinguishable from legal mutation actions while
returning both when useful work can proceed in parallel.

### Step 3: Use one role snapshot everywhere

Wrap role start/read/apply/wait results consistently. Make objective optional
only for exact-run joins; retain it as required for new worker creation.
Include the peer prompt only where useful. Do not expose raw tokens.

### Step 4: Harden exact discovery and wait

Add explicit `ScopeMismatch`, `RunMissing`, `Busy`, and `UpgradeRequired`
outcomes carrying canonical candidate data. Exact selection bypasses
discovery waiting. Evaluate actionability before the first blocking wait and
after every read.

### Step 5: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery exact
cargo test --manifest-path v4/Cargo.toml --test role_session snapshot
cargo test --manifest-path v4/Cargo.toml --test skill_flow
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
```

## Task 6: Pair the v0.2 facades and installer fail-closed

**Files:**

- Modify: `skills/vadi/scripts/dvandva-role.sh`
- Modify: `skills/prativadi/scripts/dvandva-role.sh`
- Modify: `skills/setup-dvandva/scripts/setup-dvandva.sh`
- Modify: `v4/Cargo.toml`
- Modify: `v4/Cargo.lock`
- Modify: `tests/skills/role-skills.sh`
- Modify: `tests/skills/setup-dvandva.sh`
- Modify: `tests/skills/two-role-canary.sh`
- Modify: `v4/tests/skill_flow.rs`

### Step 1: Write shell/API RED tests

Add canaries proving:

- new facade + old kernel fails before run mutation;
- old facade + new kernel fails before run mutation;
- every role subcommand passes/requires API 2;
- exact run join does not invent an objective;
- the facade can perform upgrade, re-claim, and continue a preserved v1 run;
- installer rejects a checksummed binary whose reported version/schema/API is
  wrong;
- failed update leaves `current` on 0.1.1 and successful update points to
  0.2.0 without touching run directories;
- both facade copies remain byte-identical;
- the two-role canary completes normal and reversed casting using one Site ID,
  Codex deployment receipts, and Claude review receipts.

Use truthful version fixtures or a controlled probe stub; do not label the
same compiled binary with two false versions.

### Step 2: Bump and expose the version

Set the private kernel to `0.2.0` with `publish = false`. Make probe and
`--version` agree. Update both facades to require write schema v2/API 2 and
pass the API on every role call before any run path.

### Step 3: Harden setup/update

Verify checksum, binary-reported version, write schema, and role API before
atomically switching `current`. Report the read schemas and migration
capability. Never migrate runs from setup.

### Step 4: Verify and commit

```bash
bash tests/skills/role-skills.sh
bash tests/skills/setup-dvandva.sh
bash tests/skills/two-role-canary.sh
cargo test --manifest-path v4/Cargo.toml --test skill_flow
shellcheck skills/*/scripts/*.sh tests/skills/*.sh
```

If `shellcheck` is unavailable, record that fact and run `bash -n` across every
changed shell file. Commit kernel version/API, facades, and setup separately.

## Task 7: Rewrite the role contracts and rerun pressure tests

**Files:**

- Modify: `skills/vadi/SKILL.md`
- Modify: `skills/vadi/references/run-contract.md`
- Modify: `skills/vadi/agents/openai.yaml`
- Modify: `skills/prativadi/SKILL.md`
- Modify: `skills/prativadi/references/run-contract.md`
- Modify: `skills/prativadi/agents/openai.yaml`
- Modify: `skills/setup-dvandva/SKILL.md`
- Modify: `skills/setup-dvandva/references/installation.md`
- Modify: `tests/skills/role-skills.sh`
- Modify: `v4/tests/skill_flow.rs`

### Step 1: Add contract RED assertions

Before editing skills, add focused tests for these explicit rules:

- surface run ID/canonical scope/pair prompt before domain work;
- obey kernel next actions and never work out of turn;
- reconcile scope mismatch before work;
- checkpoint one complete manifest;
- use supersession/withdrawal instead of publication as a side channel;
- Codex publishes Sites and Claude reviews every exact handoff, independent of
  vadi/prativadi casting;
- update the explainer plan/TODO at every handoff;
- no Claude Artifact/generic fallback;
- never touch harness goals;
- never invoke the peer harness or third-party explicit-only skills.

Run them against the existing skill text and preserve RED evidence.

### Step 2: Rewrite from the kernel contract

Keep each `SKILL.md` concise and put state/action detail in the reference.
Delete the instruction that selected run ID overrides supplied task identity.
Delete vadi-owned publication prose. Make both roles branch only on returned
outcomes/next actions. Include the exact ready prompt and five-part handoff
discipline without duplicating transition logic.

### Step 3: Run fresh-agent GREEN pressure scenarios

Repeat Task 1's four prompts with fresh agents against the amended skills.
Require explicit evidence that they:

- refuse out-of-turn domain work;
- request/accept supersession or withdraw approval for new deliverables;
- preserve canonical scope and surface mismatch;
- keep Codex/Claude publication duties under reverse casting;
- leave the user-created goal untouched.

If an agent still rationalizes the unsafe path, tighten the smallest skill
instruction and rerun that scenario. Record transcripts in the task report.

### Step 4: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test skill_flow
bash tests/skills/role-skills.sh
bash -n skills/vadi/scripts/dvandva-role.sh \
  skills/prativadi/scripts/dvandva-role.sh \
  skills/setup-dvandva/scripts/setup-dvandva.sh
```

## Task 8: Align active documentation and release packaging

**Files:**

- Modify: `README.md`
- Modify: `CONTEXT.md`
- Modify: `CLAUDE.md`
- Modify: `v4/README.md`
- Modify: `docs/protocol/minimal-run-baton.md`
- Modify: `docs/workflows/skill-only-run.md`
- Modify: `docs/workflows/two-mode-agent-workflow.md`
- Create: `docs/adr/0003-run-v2-security-epoch.md`
- Modify: `scripts/package-skills-release.sh`
- Modify: `tests/skills/package-release.sh`
- Modify: `.github/workflows/skills-release.yml`

### Step 1: Add documentation/package RED checks

Make package tests expect `skills-v0.2.0`, kernel `0.2.0`, write schema v2,
and role API 2. Change the deliberately wrong tag fixture to
`skills-v0.2.1`. Add source checks that active docs do not call publication
optional, tell prativadi to implement fixes, permit Claude Artifacts as the
default, or describe v1 as the active run schema.

Do not blanket-replace dated specs/plans or historical protocol evidence.

### Step 2: Update active docs

Document the immediate run ID/pair prompt, exact-run mismatch behavior,
complete checkpoint manifest, supersession paths, rolling Sites/Claude gate,
one-way v1 migration, goal noninterference, and precise terminal criteria.
Mark `docs/workflows/two-mode-agent-workflow.md` prominently as historical v3
archive guidance so it cannot govern active v4 behavior.

### Step 3: Update packaging/release automation

Make packaging validate the binary version, write schema, and role API. Keep
the release GitHub-only and the crate unpublished. Add migration-aware release
notes/checks without touching the archived v3 release job beyond existing
regression verification.

### Step 4: Verify and commit

```bash
bash tests/skills/package-release.sh
bash tests/skills/role-skills.sh
bash tests/skills/setup-dvandva.sh
bash tests/skills/two-role-canary.sh
bash -n scripts/package-skills-release.sh
```

Do not use `bash -n` on YAML; validate the workflow with an available YAML or
Actions linter and record the tool. Commit active docs separately from release
automation.

## Task 9: Full verification and adversarial branch review

**Files:** No planned production edits; fixes belong to a dispatched fix agent.

### Step 1: Run fresh full verification

```bash
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v4/Cargo.toml --all-targets
cargo test --manifest-path rust/Cargo.toml --workspace
bash tests/skills/role-skills.sh
bash tests/skills/setup-dvandva.sh
bash tests/skills/package-release.sh
bash tests/skills/two-role-canary.sh
git diff --check
git status --short
gh stack view
```

Also run `shellcheck` on changed shell files and an available YAML parser on
the workflow. Record exact counts and exits in the task report.

### Step 2: Reproduce the original incident end to end

Using the built binary and temporary XDG roots:

1. create a run and approve the initial explainer;
2. submit checkpoint A;
3. while reviewer owns it, request supersession for deliverable B;
4. prove stale approval is rejected;
5. accept supersession, submit complete checkpoint B, publish/review each
   handoff, approve, and finalize;
6. prove both role snapshots observe the same terminal identity.

Run the exact-scope mismatch and reversed-casting canaries in the same fresh
build.

### Step 3: Dispatch whole-branch review

Create a review package from the merge base to HEAD and dispatch the most
capable available reviewer. Require line-specific findings across protocol
invariants, migration/durability, credential fencing, skill behavior, shell
release safety, and spec compliance. If findings exist, use exactly one fix
wave and one scoped re-review per the subagent-development skill.

### Step 4: Finish without external publication

Use `superpowers:finishing-a-development-branch` to present integration
options. Do not push, open a PR, merge, tag, upload release assets, or publish
anything until the user separately authorizes that side effect.
