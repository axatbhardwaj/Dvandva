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

Setup downloads `dvandva-kernel-linux-x86_64` and `SHA256SUMS` from the
`skills-v0.1.0` GitHub release, verifies the exact digest and schema probe, and
installs the helper under `${XDG_DATA_HOME:-$HOME/.local/share}/dvandva/`.
It creates private run and credential roots under
`${XDG_STATE_HOME:-$HOME/.local/state}/dvandva/`. It never puts the helper on
`PATH`.

Updates are explicit: update the three skills, then invoke
`$setup-dvandva update`. Uninstall removes only manifest-owned binary data and
preserves run history unless the user separately confirms a purge.

## Run one ticket

The default implementation casting is Codex vadi and Claude prativadi:

```text
Codex session A: Act as vadi and implement DEF-123.
Claude session B: Join DEF-123 as prativadi.
```

Either harness may host either role when the user explicitly reverses the
casting, but one run must use different harness families.

Vadi discovers or creates exactly one repository/task-matched run, implements
only its objective, verifies an immutable checkpoint, and hands it off.
If another live vadi already owns that repository/task run, discovery reports
it as busy instead of silently creating a duplicate; only an explicit request
may create a separate run.
Prativadi can start first: its local watcher waits until exactly one valid
candidate exists, then claims and independently reviews that exact checkpoint.
Several matches are surfaced; neither role secretly chooses the newest.

After every handoff, the assigned-away role foreground-waits. Findings cause a
new checkpoint and a new review. Approval remains bound to one immutable
identity. Both sessions stay attached until `done`, `abandoned`, an explicit
human stop, or a surfaced Human Decision.

## Published explainer

Every run has one newly published explainer. Vadi updates it before handoff and
after waking; its plan is the shared, directly viewable TODO list. The Baton
records its URL and projection revision. A stale or failed required publication
keeps the run in `finalizing`; it cannot be made optional to bypass the gate.

The publisher is a host capability, not a coordination transport. Codex may use
Sites and Claude may use an available artifact publisher, but neither polls the
published page to wake the other.

## Explicit-only companion skills

Matt Pocock workflow skills can operate inside either joined role session only
when the human explicitly invokes the specific skill there. Dvandva never
selects one implicitly. Fable may adjudicate a genuine unresolved Claude-side
disagreement, but it is not a third Baton participant; unclear authority becomes
Human Decision.
