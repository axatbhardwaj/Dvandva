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

Then explicitly ask one session:

```text
$setup-dvandva install Dvandva.
```

The intended `skills-v0.3.0` GitHub release provides
`dvandva-kernel-linux-x86_64` and `SHA256SUMS`. Setup becomes usable only after
that tag and asset exist. It verifies the digest and the complete kernel 0.3.0,
`dvandva.run.v2`, role API 2 probe before installing under
`${XDG_DATA_HOME:-$HOME/.local/share}/dvandva/`, outside `PATH`. The crate is
non-publishable and no plugin or marketplace package is involved.

Updates are explicit: update the three skills, then invoke
`$setup-dvandva update`. Uninstall removes only manifest-owned binary data and
preserves run history unless the user separately confirms a purge. Install,
update, and uninstall never migrate run state.

## Run one ticket

The default implementation casting is Codex vadi and Claude prativadi:

```text
Codex session A: Act as vadi and implement DEF-123 with deliverable implementation.
```

Either harness may host either role when the user explicitly reverses the
casting, but one run must use different harness families.

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

Each handoff opens an obligation. The Codex-harness participant stages the
explainer's bytes for the current obligation with `stage_explainer`, and the
Claude-harness participant reads those exact bytes back through
`dvandva-role.sh explainer` and reviews them, regardless of vadi/prativadi
casting. A new handoff replaces the current obligation, so what the gate
requires is that the run's current obligation is staged and reviewed — not that
every obligation the run ever opened was. The explainer carries canonical
scope, complete manifest, findings and decisions, and a current plan/TODO list.

The gate binds a sha256 digest, not a URL, so both harnesses can always reach
the artifact. Staging different bytes invalidates the earlier review.
A Codex Sites deployment is an optional human-facing rendering of the
already-staged bytes; it must name the same digest and never satisfies the gate. Recording an
unread approval, or substituting a Claude Artifact, mutable URL, or generic
hosting, cannot satisfy the gate. The page never coordinates wake-up.

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
and approved Claude review bound together. A Sites deployment is optional and is
not required at `done`. A Human Decision pauses the pair rather than completing
it.

## Explicit-only companion skills

Matt Pocock workflow skills can operate inside either joined role session only
when the human explicitly invokes the specific skill there. Dvandva never
selects one implicitly. User-created harness goals remain untouched throughout
the run.
