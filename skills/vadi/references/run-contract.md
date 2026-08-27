# Vadi run contract

Use only `scripts/dvandva-role.sh`; the helper remains private and outside
`PATH`. First run `probe`. Resolve the session with `session-id`; if unavailable,
run `session-id --generate` once and retain that value in this harness session.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE OBJECTIVE [TASK] [--new-run|--run-id ID]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_JSON
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
```

`TASK` is a copied identity field. When the human supplies an explicit ticket
ID or URL, trim its surrounding whitespace and pass the remaining string
verbatim. When neither appears, omit `TASK`. Keep the same copied identity on
later starts; a human-selected `--run-id` is authoritative if a supplied task
identity differs from that run.

`start` returns `created`, `resumed`, `claimed`, or `reclaimed`, or a fail-closed
discovery result. `task_mismatch` returns compatible candidates immediately;
surface them instead of starting another discovery wait. Several matches
require human selection; repeat `start` with the selected exact `--run-id`.
Use `--new-run` only when the human explicitly asks for a separate run, and
never combine it with `--run-id`.

Drive these states:

| Status/assignee | Vadi action |
|---|---|
| `working/worker`, `revising/worker` | Implement, verify, update explainer, submit checkpoint |
| `reviewing/reviewer` | Update explainer, then `wait` |
| `finalizing/worker` | Synchronize publication and finalize |
| `human_decision/human` | Surface the recorded question; do not guess |
| `done`, `abandoned` | Report terminal evidence and stop |

Create action files in a private temporary directory, mode 0600, and delete
them after `apply`. Never include credentials. Supported worker actions:

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"<immutable SHA>","verification":["<exact check>"]}}
{"type":"record_publication","required":true,"desired_revision":6,"published_revision":6,"refs":[{"kind":"explainer","value":"<published URL>"}]}
{"type":"finalize"}
{"type":"request_human_decision","question":"<decision>","evidence":["<fact>"],"options":["<option>"],"contact_role":"worker","resume_status":"working","resume_assignee":"worker"}
```

Publication is required for the per-run explainer. Its plan is the shared TODO
list and must be refreshed before each handoff and after each wake. Use an
available site/artifact publisher to create a new site for the run. Record only
the URL and projection revision. If publishing is unavailable or fails, request
Human Decision from the current state with the matching resume status and
assignee; after approval, remain in `finalizing`. Never set `required` false.

Checkpoint identity is the exact reviewed object, not a branch or mutable
`HEAD`. For Git, prove it is a commit with
`git cat-file -e '<identity>^{commit}'`; for an artifact, compute and record its
cryptographic digest. After any source change, submit a new identity and obtain
a new review. Call `heartbeat` with the current revision before a long
implementation or publication operation. The facade uses a 30-minute claim
lease and maintains it while waiting; timeouts mean a sanitized reread followed
by another wait, not role completion. If the canonical ticket is unavailable
and the Baton objective is insufficient, request Human Decision instead of
inventing scope.
