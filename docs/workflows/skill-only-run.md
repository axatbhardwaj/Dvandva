# Skill-only Dvandva run

## Boundary

Dvandva v4 is three agent skills backed by one private local kernel. It is not
a Claude/Codex plugin, daemon, launcher, Git hook, or peer-invocation service.
The human starts two ordinary T3 Code sessions. The sessions coordinate only
through one run-scoped Baton under the XDG state directory.

The retired v3.5.1 crate and v1.7.0 plugin remain historical evidence. None of
their installation, commands, hooks, state, or marketplace metadata is part of
v4.

## Install

```bash
npx --yes skills add axatbhardwaj/Dvandva --global \
  --agent claude-code codex \
  --skill setup-dvandva vadi prativadi
```

Dvandva setup installs only the three Dvandva skills above and their private
kernel. It does not install or update Matt Pocock's separate skills. When the
host advertises Matt's model-invocable `code-review`, prativadi must use it once
for each newly authorized complete Git delivery candidate; otherwise the
declared native fallback keeps Dvandva standalone.

Then explicitly ask one session:

```text
$setup-dvandva install Dvandva.
```

The `skills-v0.3.3` GitHub release provides `dvandva-kernel-linux-x86_64` and
`SHA256SUMS`. Setup verifies the digest and the complete kernel 0.3.3,
`dvandva.run.v2`, role API 2 probe before installing under
`${XDG_DATA_HOME:-$HOME/.local/share}/dvandva/`, outside `PATH`. The crate is
non-publishable and no plugin or marketplace package is involved.

**Linux x86_64 only, for now.** That is the only asset the release carries and
the only host the installer accepts; on macOS, native Windows, or another
architecture it fails closed before downloading anything. The kernel itself
uses Linux-only system calls, so there is no other build to substitute. On
Windows, run both sessions inside WSL2.

Updates are explicit: update the three skills, then invoke
`$setup-dvandva update`. Uninstall removes only manifest-owned binary data and
preserves run history unless the user separately confirms a purge. Install,
update, and uninstall never migrate run state.

## Run one ticket

The default implementation casting is Codex vadi and Claude prativadi:

```text
Codex session A: Act as vadi and implement DEF-123 with deliverable implementation.
```

Any two distinct, non-blank harness names may fill the two roles; the user may
explicitly reverse the usual Codex/Claude casting.

Before domain work, vadi surfaces the returned run ID, canonical objective and
scope, status/assignee, actions, and this exact prompt:

```text
Act as prativadi and join Dvandva run <run-id>.
```

The human pastes it into session B. Neither role invokes or wakes the other
harness. If prativadi starts first, local discovery waits for exactly one valid
candidate. Several matches are surfaced rather than guessed.

An exact `--run-id` selects state, not scope. Any explicitly supplied objective,
reference, task, or deliverable coordinate is compared with the Baton;
`scope_mismatch` returns the canonical and supplied coordinates without claim,
wait, or work. `run_missing` and a live foreign claim also return immediately.
If another live vadi already owns that repository/task run, discovery reports
it as busy instead of silently creating a duplicate; only an explicit request
may create a separate run.

V1 selection returns `upgrade_required` without a claim. The designated role
uses the facade's explicit migration action, then both sessions claim the new
v2 epoch. Ordinary actions never upgrade, history never downgrades, and setup
does not touch runs.

## Complete checkpoint loop

Vadi works only when `advisory_actions` authorizes `work`. It submits one
checkpoint whose manifest covers every canonical deliverable ID exactly once,
uses immutable external references, and includes verification. The kernel
derives its manifest digest and scope revision. Prativadi first reconciles the
declared scope with the human objective, then binds its verdict to checkpoint
identity, manifest digest, and scope revision.

The checkpoint is submitted only after the whole canonical implementation and
its verification are complete. Partial or work-in-progress implementation is
not checkpointed for Matt review. If prativadi requests changes, vadi fixes
them and submits a new complete candidate; prativadi then runs `code-review`
once against that new immutable `HEAD` and the original pinned fixed point.

New required work has two stale-work paths:

- During `reviewing`, vadi applies `request_checkpoint_supersession`; approval
  is blocked until prativadi accepts and returns ownership.
- During `finalizing`, vadi applies `withdraw_approval` and produces a new
  complete checkpoint.

Human Decision is for what only a human can settle: `scope` for what the work
should cover, `intent` for which reading of the request is meant, and
`authority` for permission that is theirs alone to give. A missing capability is
not one of these — it has a deterministic recovery, and the kernel refuses to
park a run while it holds one. Only the designated contact resumes a decision,
and only a human-approved scope amendment can change canonical scope.

## Published explainer

Each handoff opens an obligation. Vadi stages the explainer's bytes for the
current obligation with `stage_explainer`; prativadi reads those exact bytes
back through `dvandva-role.sh explainer` and reviews them. At `run_started`,
vadi proposes this first HTML before continuing domain work and revises it until
prativadi approves. A new handoff replaces the current obligation, so the gate
checks the current artifact and receipts rather than every historical one. The
explainer carries canonical scope, complete manifest, findings and decisions,
and a current plan/TODO list.

The gate binds a sha256 digest, not a URL, so both roles can always reach the
artifact. Staging different bytes invalidates the earlier review. After
prativadi approval, whichever participant is Codex mechanically deploys that
digest to one stable, owner-only ChatGPT Site per run. It is the user's status
page and later approved handoffs update the same URL. When Codex participates,
finalization requires both receipts; without Codex, publication is skipped and
the local approval is sufficient. Substituting a mutable URL or generic host is
invalid. The page never coordinates wake-up.

Only `finalize` waits on this gate. A finished deliverable can always be
checkpointed, and the recovery paths — supersession and approval withdrawal —
are never blocked by it.

After every handoff the assigned-away role foreground-waits with
`dvandva-role.sh poll`, in the same turn as its handoff report. `poll` re-enters
the kernel wait on every `idle_timeout` until a real wake, a terminal run, or
its budget; on an idle return the role calls it again immediately. Ending the
turn is not a wait — it lets the lease lapse and stalls the peer. Both roles
refresh the facade snapshot after waking. A role stops only for explicit human
stop, `abandoned`, or after observing `done` with the current scope, complete
checkpoint, exact semantic approval, and the current obligation's staged bytes
and approved prativadi review bound together, plus the matching owner-only Sites
deployment when the pairing contains Codex. A Human Decision pauses the pair
rather than completing it.

## Companion skills

Matt Pocock's user-invoked workflow skills can operate inside either joined
role session only when the human explicitly invokes the specific skill there.
The separately installed `code-review` is mandatory when the host advertises it
as model-invocable, but only for a complete Git delivery candidate after the
whole scoped implementation is ready. Prativadi supplies a pinned fixed-point
SHA, the checkpoint as `HEAD`, and a sha256-bound local snapshot of the
canonical task/spec bytes.
Its Standards and Spec reports are evidence for prativadi's independently
adjudicated, checkpoint-bound verdict. Prativadi summarizes provenance under
`What was verified`; the Baton durably records only the accepted findings and
verdict bound to the checkpoint. Raw companion reports and rejected findings
are session evidence, not protocol state or a peer transport. Analysis
checkpoints stay native. An absent, hidden, user-only, unreadable, rejected, or
incomplete companion takes the disclosed native fallback. Prativadi never
installs or reconfigures it mid-run. User-created harness goals remain untouched
throughout the run.

The supplied snapshot is the sole authorized Spec source. Prativadi tells the
companion not to discover or fetch issue references or other specs and requires
the Spec report to attest the supplied digest. Any other source or missing
attestation invalidates both companion reports and triggers the disclosed native
fallback against the immutable snapshot.

The host behavior is documented by
[OpenAI's Codex skill invocation policy](https://learn.chatgpt.com/docs/build-skills#how-chatgpt-and-codex-use-skills)
and [Claude Code's skill invocation policy](https://code.claude.com/docs/en/skills#control-who-invokes-a-skill).
The installed Matt skill was rechecked on 2026-09-01 with:

```bash
task_skill_root="$HOME/.agents/skills/code-review"
test -f "$task_skill_root/SKILL.md"
test -f "$task_skill_root/agents/openai.yaml"
! rg -n '^disable-model-invocation:[[:space:]]*true$' "$task_skill_root/SKILL.md"
! rg -n 'allow_implicit_invocation:[[:space:]]*false' "$task_skill_root/agents/openai.yaml"
printf 'installed policy check: model invocation allowed\n'
```

The RED/GREEN fresh-agent scenarios are recorded in
[`docs/case-studies/2026-09-01-prativadi-code-review-pressure-test.md`](../case-studies/2026-09-01-prativadi-code-review-pressure-test.md).
