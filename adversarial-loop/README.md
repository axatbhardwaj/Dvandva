# Adversarial-loop Stop gate

This is a small, self-contained Claude Code Stop-hook proof for Dvandva's
cross-family review gate. Runtime state lives in the ignored
`.adversarial-loop/` directory at a repository root:

- `goal.json` records the active goal, its owning Claude Code session, and its
  artifact-bound steps.
- `evidence/<goal_id>/<step_id>/<attempt_id>.json` is append-only review
  evidence. The latest attempt for every step must be a passing review from a
  different model family and must match both the step revision and artifact
  digest.

`lib/predicate.sh` is the sourceable decision function. `hooks/gate.sh` is its
Claude Code Stop adapter: block decisions use the Stop contract of exit 0 plus
one JSON object on stdout; allow decisions use exit 0 and no stdout.

## What this proves — and what it cannot

Within a hook-enabled Claude Code session, it deterministically blocks an
answer from ending as `done` while active, cross-family evidence is absent or
failing. It rechecks state on every Stop re-fire, including
`stop_hook_active: true`, rather than treating that flag as an escape hatch.
Claude Code force-ends after **eight consecutive blocks**, so this is honestly
a bounded nudge that makes the easy path “run the review”; it is not an
unbounded enforcement daemon.

The predicate verifies evidence presence and shape, cross-family labels,
artifact digest binding, and verdicts. It cannot prove that a transcript came
from a genuine provider call. A malicious chair who edits or deletes
`goal.json` can disarm the gate; that attack is explicitly **out of scope** for
this lazy-chair proof. The runtime files are not a separate authority system.

## Manual Claude Code wiring

Do not install this automatically. For this checkout's
`plugins/dvandva/hooks/hooks.json`, add the following Stop entry manually. The
path starts from the Dvandva plugin root and reaches this repository-level
`adversarial-loop/` directory through Claude Code's plugin-root variable.

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

This relative wiring works only in this repository checkout. In a marketplace
install, `${CLAUDE_PLUGIN_ROOT}/../../adversarial-loop` does not resolve to this
repository-level directory, so the snippet must not be reused as-is.

## Verification and a real-Claude-Code check

Run the hermetic regression suite from the repository root:

```bash
bash adversarial-loop/tests/gate_test.sh
```

For a manual integration check, configure an active goal owned by the current
Claude Code `session_id`, leave one step without valid cross-family evidence,
and try to finish. Claude Code should show the block reason. Trigger the Stop
hook again with `stop_hook_active: true`; it must issue the same state-based
block until the evidence is fixed. Finally, observe that Claude Code stops
blocking after its platform-enforced eighth consecutive block—the documented
ceiling of this bounded nudge, not a condition this hook attempts to defeat.
