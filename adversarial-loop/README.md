# Adversarial-loop gate

This directory is a small, self-contained proof of an artifact-bound review
gate. Runtime state lives in the ignored `.adversarial-loop/` directory at a
repository root:

- `goal.json` records the goal, its owning session, review-separation mode, and
  artifact-bound steps.
- `evidence/<goal_id>/<step_id>/<attempt-N>.json` is append-only review
  evidence. The latest attempt for every step must pass and match both the
  stamped step revision and artifact digest.

`lib/predicate.sh` is the single sourceable decision function. Two adapters use
that same predicate:

- `hooks/gate.sh` is the Claude Code Stop-hook adapter. A block is exit 0 plus
  one Stop decision object on stdout; an allow is exit 0 with no stdout.
- `hooks/gate-cli.sh` is the stdin-free CLI and Git pre-commit adapter. A block
  is a nonzero exit plus its reason on stderr; an allow is exit 0.

## Goal modes and stamping

`goal.json.mode` selects the independence check:

- `"cross-vendor"` requires the latest evidence `reviewer_family` to differ
  from the step's `author_family`. Omitting `mode` means `cross-vendor` for
  compatibility with older goals.
- `"cross-context"` requires a non-empty `steps[].author_agent_id` and a
  non-empty evidence `reviewer_agent_id`; the two ids must differ. This is the
  same-vendor fallback and is weaker than cross-vendor review.

A pending step uses `"artifact_digest":""`. After execution and before any
attack lane reads the artifact, one STAMP writer computes
`sha256sum -- <artifact>`, writes the real 64-hex digest and current revision,
and changes the step to `"status":"complete"` in `goal.json`. Evidence binds
to that stamped snapshot. A complete step with an empty or malformed digest is
blocked.

Goal status is fail-closed. `active` and `done` are both enforced identically:
every step must be complete and have passing, mode-correct evidence before the
adapter allows. `done` is therefore earned, not an escape hatch. `abandoned`
is deliberately inert, a missing `goal.json` allows, and every other status
blocks.

## Three deployment configurations

| Configuration | Goal mode | Adapter | Separation |
| --- | --- | --- | --- |
| Claude chair + Codex reviewer | `cross-vendor` (or absent) | Claude Code Stop hook | Claude/GPT family boundary |
| Claude-only chair and fresh reviewers | `cross-context` | Claude Code Stop hook | Distinct Claude agent ids/contexts |
| Codex-only or general automation | `cross-context` | CLI or Git pre-commit | Distinct Codex agent ids/contexts |

For either cross-context configuration, give each fresh author and reviewer
dispatch a stable, non-empty id and write those exact ids to the step and its
evidence. Reusing one id blocks. Cross-context reduces accidental
self-approval but does not provide the vendor-level decorrelation of
cross-vendor mode.

## Claude Code Stop-hook adapter

Do not install this automatically. For this checkout's
`plugins/dvandva/hooks/hooks.json`, add the following Stop entry manually. The
path starts from the Dvandva plugin root and reaches this repository-level
directory through Claude Code's plugin-root variable.

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/../../adversarial-loop/hooks/gate.sh"
          }
        ]
      }
    ]
  }
}
```

The Stop adapter gets the owner identity from the hook payload's `session_id`
and compares it with `goal.json.owner_session_id`. This relative path works
only in this repository checkout. A marketplace install does not have the
same repository-level path, so do not reuse the snippet there as-is.

## CLI and Git pre-commit adapter

The CLI adapter reads no hook JSON. Run it from anywhere inside the target Git
worktree and provide the owner session explicitly or through the environment:

```bash
adversarial-loop/hooks/gate-cli.sh --session codex-run-42
ADVERSARIAL_LOOP_SESSION=codex-run-42 adversarial-loop/hooks/gate-cli.sh
```

The supplied id must exactly equal `goal.json.owner_session_id`. **Warning:**
a stale or mistyped `--session` / `ADVERSARIAL_LOOP_SESSION` value that differs
from `goal.json.owner_session_id` makes `gate-cli.sh` silently allow (exit 0)
with no signal: the predicate treats a non-owner session as “not our goal”. It
fails closed only when the session is absent. Always pass the exact same value
used when the goal was created. To use the same adapter as a repository-local
pre-commit hook, install a small wrapper whose absolute path points at this
checkout:

```bash
#!/usr/bin/env bash
exec /absolute/path/to/Dvandva/adversarial-loop/hooks/gate-cli.sh
```

Then make the wrapper executable and preserve the owner id in the commit
environment:

```bash
ADVERSARIAL_LOOP_SESSION=codex-run-42 git commit
```

Missing or failing evidence makes `gate-cli.sh` print the predicate reason to
stderr and exit nonzero, so Git rejects the commit.

## What this proves — and what it cannot

Within a configured adapter, the predicate deterministically checks evidence
presence and shape, the selected cross-review identity rule, append-only latest
attempt ordering, artifact digest binding, revision binding, and verdicts. The
Stop adapter rechecks state on every Stop re-fire, including
`stop_hook_active: true`, rather than treating that flag as an escape hatch.
Claude Code force-ends after eight consecutive Stop blocks, so that adapter is
honestly a bounded nudge rather than an unbounded enforcement daemon.

The predicate cannot prove that a transcript came from a genuine provider
call. A malicious chair can edit or delete `goal.json` and disarm the gate;
that attack is outside this lazy-chair proof. The runtime files are not a
separate authority system.

## Verification

Run the hermetic regression suite from the repository root:

```bash
bash adversarial-loop/tests/gate_test.sh
```

For a manual Stop integration check, configure an owned `active` goal with a
complete step but no evidence and try to finish. Claude Code should show the
block reason, including on a `stop_hook_active: true` re-fire. Add valid review
evidence and confirm the Stop is allowed. Repeat after changing the goal status
to `done`: valid evidence still allows, while removing it blocks.

For a manual CLI check, run `gate-cli.sh --session <owner>` against the same
missing-evidence fixture and confirm a nonzero exit plus the reason on stderr;
add valid evidence and confirm exit 0.
