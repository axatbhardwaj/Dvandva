# Interrupted-poll evidence

Evidence for the tool-dependent claims in the role contracts: a killed
foreground `poll` force-ends the harness turn, and the harness reopens with a
synthetic resume message that a role must not treat as a stop.

## Local observations, run `axatbhardwaj-dvandva-22-86aa41ed`

- The human killed a foreground `dvandva-role.sh poll` three times from the
  T3 Code client. Each time the Claude Code Bash tool returned `Exit code 137`
  with no stdout, and the turn ended immediately.
- Each time, the next turn opened with the harness message
  `Continue from where you left off.` and no human question.
- Each time, the vadi role emitted an empty reply, rendered by the client as
  `No response requested.` The lease kept ticking and the peer was stalled.
- The human's client dropped every assistant message emitted after the first
  one in each interrupted turn, so the printed `peer_prompt` was never seen.
- A `poll` launched as a background Bash command survived the end of the turn,
  renewed the lease across the wait, and re-invoked the role on exit.

## Verification commands

- `gh issue view 22 --repo axatbhardwaj/Dvandva --comments` shows the owner
  refinement of 2026-09-04T08:04:38Z recording the reproduction.
- `bash skills/vadi/scripts/dvandva-role.sh poll SESSION RUN_DIR REV 540000`
  under a Bash tool timeout, then a client interrupt, reproduces exit 137.

## Authoritative routes

- Spec: [issue #22](https://github.com/axatbhardwaj/Dvandva/issues/22).
- Exit status 128+9 for a SIGKILLed child: [Bash manual, exit status](https://www.gnu.org/software/bash/manual/html_node/Exit-Status.html).
