# Skill-Only Run Interface Design

**Status:** Approved in conversation on 2026-08-27; written revision awaiting review

## Objective

Make Dvandva usable entirely through three agent skills:

- `setup-dvandva` performs one-time installation and diagnostics;
- `vadi` creates or resumes a run and owns implementation;
- `prativadi` discovers and joins a run and owns adversarial review.

The human starts ordinary Claude and Codex sessions in T3 Code and speaks in
role language. The normal implementation pairing is Codex as vadi and Claude
as prativadi:

```text
Codex: Act as vadi and implement DEF-123.
Claude: Act as prativadi for the current run.
```

Natural-language activation is the primary interface. Explicit `$vadi` and
`$prativadi` invocation remains the deterministic fallback. The human never
needs to call the kernel CLI, copy a run ID, manipulate a Baton, or launch one
harness from the other.

Both role skills are model-invocable: their descriptions explicitly trigger
on "act as vadi", "implement as vadi", "act as prativadi", and "join the
current run as prativadi". `setup-dvandva` remains explicit-only because setup,
update, and uninstall change user-level installation state.

## Boundary and non-goals

The reviewed v4 Rust kernel remains the sole authority for claims, state
transitions, persistence, history, recovery, and local waiting. Skills are
thin role adapters over that kernel; they do not reimplement protocol rules in
prompt prose or shell.

This layer does not add:

- a daemon or background service;
- Git hooks or commit interception;
- Claude-to-Codex or Codex-to-Claude invocation;
- T3 wake-up or transport semantics;
- GitHub, Linear, or Sites polling;
- a project-level coordination Baton;
- implicit invocation of Matt Pocock's explicit-only skills;
- any dependency on the archived v3.5.1 plugin, installer, or state machine.

The helper binary is an internal skill dependency. It is deliberately not put
on the user's `PATH`, presented as the product interface, or registered as a
Claude/Codex plugin command.

## Roles

Protocol role is independent of harness family:

| Skill | Kernel role | Responsibility |
|---|---|---|
| `vadi` | `worker` | implement, verify, submit checkpoints, revise, publish projections, finalize |
| `prativadi` | `reviewer` | independently review the exact checkpoint, request changes, approve |

Either Claude or Codex may host either skill, but the participant families for
one run must differ. The default ticket implementation casting is Codex vadi
and Claude prativadi because implementation consumes most compute and Claude
provides the adversarial review gate. Reversing the roles requires an explicit
user instruction; it never happens because one family is unavailable.

Fable is not a third run participant. A Claude-hosted prativadi may use Fable
only for a genuine unresolved disagreement, then serialize the decision into
the existing two-role Baton or raise a Human Decision. Ordinary findings and
revision requests do not trigger adjudication.

## Installation and updates

The distributable repository exposes `skills/setup-dvandva/`,
`skills/vadi/`, and `skills/prativadi/` in the standard agent-skills layout.
The one-time bootstrap is:

```text
npx skills add axatbhardwaj/Dvandva
```

The user selects Claude Code and Codex as targets, then invokes
`$setup-dvandva` once. That skill:

1. detects Linux architecture and the XDG directories;
2. downloads the matching versioned kernel release plus checksum manifest;
3. verifies SHA-256 before installation;
4. stores the binary privately at
   `$XDG_DATA_HOME/dvandva/bin/<version>/dvandva-kernel`;
5. atomically switches `$XDG_DATA_HOME/dvandva/bin/current` only after
   verification;
6. creates `$XDG_STATE_HOME/dvandva/runs/` and the private credential root;
7. runs a kernel compatibility probe and reports skill/kernel versions.

The standard Skills CLI owns installation, update, conflict handling, and
removal for `setup-dvandva`, `vadi`, and `prativadi`. The setup skill begins
only after those skills are installed and owns no engine skill paths. Its
manifest covers only the private kernel data it creates, so it cannot overwrite
or remove an unowned or archived-v3 skill.

There is no post-install daemon. A skill resolves the private binary through
the `current` link on each activation and rejects an incompatible schema or
major version before touching run state.

Updates are explicit: update the skills, then invoke `$setup-dvandva update`.
An active run stays pinned to the kernel major/schema it was created with;
setup never changes its Baton. Uninstall removes only marker-owned kernel
versions and its installation manifest. Run history is preserved by
default and is deleted only through a separately confirmed purge.

The first development canary may use the repository-built release binary, but
that path is test evidence, not the final installation UX.

## Local data layout

```text
$XDG_DATA_HOME/dvandva/
  installation.json
  bin/<version>/dvandva-kernel
  bin/current -> <version>

$XDG_STATE_HOME/dvandva/
  runs/<run-id>/
    baton.json
    .baton.lock
    history/<revision>.json
  credentials/<session-id>/<run-id>/<role>.json
```

The runs directory is a discovery registry only because discovery is a scan of
independent run directories. There is no mutable global index, global lock, or
project scheduler. Every state-changing decision still occurs under the
selected run's lock and compare-and-swap revision.

Credential files contain the raw participant token and are created mode 0600;
tokens never enter the Baton, history, repository, prompt output, published
explainer, or logs. The threat model is accidental disclosure between normal
same-user sessions, not isolation from a malicious process running as the same
OS user. A replacement session waits for expiry and reclaims with a new epoch;
it does not reuse another session's credential.

## Role-session identity

Each skill activation needs a stable non-secret `session_id` and a private
credential locator. The adapter uses a T3-provided stable session identifier
when available. Otherwise it generates a UUID at the first role activation
and retains it in the harness session's private runtime context for that
session's lifetime. It never derives identity solely from repository, branch,
PID, or role because those values can be shared or reused.

Subsequent turns in the same harness session reuse the credential whose stored
session ID matches the participant claim in the Baton. A newly launched
harness session receives a new ID, cannot adopt the old raw token, and must
wait for expiry before `reclaim`. If the host cannot provide or retain a
session-private locator, the skill fails before claiming rather than storing a
credential in the repository or shared run directory.

## Repository and task identity

Automatic discovery requires stable metadata in `dvandva.run.v1`. A run adds:

```json
{
  "workspace": {
    "repository_id": "normalized-origin-or-local-fingerprint",
    "origin": "git@github.com:owner/repo.git",
    "worktree": "/absolute/advisory/path"
  },
  "task": {
    "reference": "DEF-123",
    "summary": "Implement DEF-123"
  }
}
```

`repository_id` is the normalized Git remote identity when available; a
canonical local repository fingerprint is used only for repositories without
a remote. It intentionally does not include the worktree path, so two T3
sessions in different worktrees of the same repository can pair. The worktree
path is advisory evidence for implementation and review, not a discovery key.

The vadi extracts a task reference from the user's request when one is
present. Absence of a tracker ID is allowed; summary matching may narrow
results but never silently resolves ambiguity. Run IDs are safe slugs plus a
random suffix, so concurrent runs for one ticket remain distinct.

## Vadi activation

When asked to act as vadi, the skill:

1. reads repository instructions and identifies the current repository;
2. checks for a resumable vadi-owned run matching repository and task;
3. resumes exactly one match, asks on several matches, or creates a new run;
4. binds worker family to the current harness and reviewer family to the
   opposite default unless the user explicitly supplied a valid pairing;
5. claims `worker`, stores the credential privately, and reads the objective;
6. performs only the task authorized by the Baton;
7. submits an immutable Git or artifact checkpoint with verification;
8. foreground-waits whenever prativadi owns the turn;
9. fixes findings and resubmits a new identity until approved;
10. synchronizes required publication and finalizes the unchanged approved
    identity.

Creation is idempotent at the run-directory boundary. The skill must not
silently create a second run when one unambiguously matches the same task.
The human may explicitly request a separate run for the same task; that
instruction bypasses resume selection but still creates a distinct random
run ID and independent directory.

## Prativadi automatic discovery

When asked to act as prativadi, the skill computes the current
`repository_id`, extracts any task reference in the prompt, and scans
`$XDG_STATE_HOME/dvandva/runs/*/baton.json`.

A candidate must be:

- valid `dvandva.run.v1` state;
- non-terminal;
- for the current repository;
- bound to the current harness family as reviewer;
- unclaimed by a live reviewer session, or reclaimable after expiry;
- consistent with an explicit task reference when one was provided.

Outcomes are fail-closed:

- exactly one candidate: claim it and begin review;
- no candidate: foreground-watch the runs directory, with polling fallback,
  until one appears or the human interrupts;
- several candidates: narrow by explicit task reference; if several still
  match, present concise run ID/task/status choices and ask the human;
- corrupt candidate: report it separately and do not treat it as a match;
- terminal candidate: ignore it for joining and never reopen it.

The skill never chooses "newest" or "most recently modified" as a hidden
tie-breaker. Starting prativadi before vadi is supported: directory waiting is
a blocking helper operation and consumes no model turns. Concurrent
prativadi sessions may discover the same run, but the kernel claim CAS permits
only one winner; the loser resumes discovery rather than joining as a third
participant.

## Active review loop

Prativadi independently reads the exact checkpoint identity and verification
evidence, reviews the relevant diff/artifact against the ticket and repository
standards, then records one of:

- `changes_requested` with non-empty actionable findings bound to that
  checkpoint; or
- `approved` with no blocking findings, bound to that checkpoint.

After writing a review, prativadi foreground-waits. Vadi wakes, revises, and
submits a new checkpoint; prativadi wakes and reviews again. Neither role
exits merely because its current turn ended.

Matt Pocock's user-invoked workflow skills remain human-invoked. Once vadi has
finished the whole canonical implementation and submitted a complete Git
delivery candidate, prativadi automatically invokes the separately
model-invocable `code-review` once for that immutable checkpoint. It supplies
the exact checkpoint as `HEAD`, a pinned fixed-point SHA, and the canonical task
or spec. No Matt review runs against implementation-in-progress. Its Standards
and Spec reports are evidence; prativadi still adjudicates and binds the
Dvandva verdict. A changes-requested revision is reviewed only after vadi
submits another complete candidate. Analysis checkpoints stay native. If the
companion is unavailable, prativadi performs both axes natively and discloses
the fallback, so the walkaway loop remains autonomous.
The invocation-policy evidence is recorded in
[`docs/workflows/skill-only-run.md`](../../workflows/skill-only-run.md).

## Human Decisions and termination

Either active role may raise a Human Decision when intent, authority, or a
genuine disagreement cannot be resolved autonomously. The designated contact
session surfaces the question once; the other remains in passive wait. The
answer restores the declared state and assignee.

`done` and `abandoned` stop both skills. A terminal run is never resumed or
selected by discovery. Continuing the objective creates a new run with a
predecessor reference. Recovery restores only from validated history, clears
both claims, and never reopens a terminal history head.

## Error behavior

- Missing/incompatible helper: stop before run mutation and instruct the user
  to invoke `$setup-dvandva`.
- Missing run: prativadi waits; vadi creates after checking for matches.
- Ambiguous discovery: ask; never guess.
- Claim contention: reread and resume discovery or waiting.
- Expired own claim: reclaim only after kernel-confirmed expiry.
- Corrupt Baton/history: fail closed and surface the recovery command through
  the skill; never edit JSON directly.
- Filesystem notification failure: use bounded interval polling.
- Lost peer: lease expiry permits replacement; an unresolved prolonged stall
  becomes a Human Decision rather than a third participant.
- Required publication failure: remain in finalizing with evidence of the
  failure; never disable the gate.

## Implementation slices

This design should be implemented after PR #4 as a stacked follow-up:

1. extend the Baton with workspace/task identity and add repository-scoped
   discovery plus discovery waiting to the kernel;
2. add private session credential management and role-oriented helper
   commands that keep raw tokens out of skill prompts;
3. author minimal `vadi`, `prativadi`, and `setup-dvandva` skills with shared
   references rather than copying protocol prose;
4. add release packaging, checksums, compatibility probing, explicit update,
   doctor, and manifest-owned uninstall;
5. run black-box kernel tests, isolated installer tests, skill pressure tests,
   and the real two-session T3 canary.

The first four slices may be separate granular commits on one follow-up PR.
Publishing a release, installing into the user's live skill directories, and
running the real canary require their own explicit authorization.

## Acceptance criteria

The layer is ready when all of the following are demonstrated:

1. A Codex session given "Act as vadi and implement DEF-123" creates and
   claims one repository-scoped run without manual CLI use.
2. A Claude session started first as prativadi blocks without model polling,
   wakes after vadi creates the run, and joins it automatically.
3. A Claude session started after creation auto-joins the only matching run.
4. Multiple matching runs produce a human choice and no claim mutation.
5. Two racing prativadi sessions yield exactly one reviewer claim.
6. A complete checkpoint, findings, revision, approval, publication, and
   finalization cycle finishes without either harness invoking the other.
7. Role reversal works when explicitly requested.
8. Tokens appear only in mode-0600 private credential files and are fenced on
   replacement or recovery.
9. Setup, update, doctor, and uninstall pass in isolated Claude/Codex homes;
   uninstall preserves run history unless purge is separately confirmed.
10. Existing v4 kernel tests and the archived v3 suite remain green, with no
    v3 plugin or distribution changes.
