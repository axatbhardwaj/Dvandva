# Vadi run contract

Use only `scripts/dvandva-role.sh`; the helper remains private and outside
`PATH`. First run `probe`. Resolve the session with `session-id`; if unavailable,
run `session-id --generate` once and retain that value in this harness session.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE OBJECTIVE [TASK] [--new-run]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_JSON
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
```

`start` returns `created`, `resumed`, `claimed`, or `reclaimed`, or a fail-closed
discovery result. Several matches require human selection. Use `--new-run` only
when the human explicitly asks for a separate run.

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
```

Publication is required for the per-run explainer. Its plan is the shared TODO
list and must be refreshed before each handoff and after each wake. Use an
available site/artifact publisher to create a new site for the run. Record only
the URL and projection revision. If publishing is unavailable or fails, remain
in `finalizing` and request Human Decision; never set `required` false.

Checkpoint identity is the exact reviewed object, not a branch or mutable
`HEAD`. After any source change, submit a new identity and obtain a new review.
The facade maintains the lease while waiting; timeouts mean wait again after a
sanitized reread, not role completion.
