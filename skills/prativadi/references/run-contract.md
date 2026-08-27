# Prativadi run contract

Use only `scripts/dvandva-role.sh`; the helper remains private and outside
`PATH`. First run `probe`. Resolve the session with `session-id`; if unavailable,
run `session-id --generate` once and retain that value in this harness session.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE OBJECTIVE [TASK] --wait [--run-id ID]
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

`start --wait` watches for a matching run without consuming model turns.
Exactly one valid candidate is claimed. None continues waiting; several are
returned for human selection, after which `start --wait` is repeated with the
selected exact `--run-id`. `task_mismatch` returns compatible candidates
immediately; surface them instead of starting another discovery window.
Corrupt, terminal, wrong-repository, wrong-family, and live-claimed candidates
are never silently selected. A lost claim race returns to discovery.

Drive these states:

| Status/assignee | Prativadi action |
|---|---|
| `working/worker`, `revising/worker` | `wait` |
| `reviewing/reviewer` | Review exact checkpoint, record verdict |
| `finalizing/worker` | `wait`; approval is not completion |
| `human_decision/human` | Surface the recorded question; do not guess |
| `done`, `abandoned` | Report terminal evidence and stop |

Create action files in a private temporary directory, mode 0600, and delete
them after `apply`. Never include credentials. Supported reviewer actions:

```json
{"type":"record_review","verdict":"changes_requested","checkpoint_identity":"<exact identity>","findings":["<actionable finding>"]}
{"type":"record_review","verdict":"approved","checkpoint_identity":"<exact identity>","findings":[]}
{"type":"request_human_decision","question":"<decision>","evidence":["<fact>"],"options":["<option>"],"contact_role":"reviewer","resume_status":"reviewing","resume_assignee":"reviewer"}
```

Before submission, confirm the inspected object still equals
`checkpoint.identity` and use the current Baton revision for CAS. On a stale
revision, reread and review the newly assigned identity; never force the old
verdict. After submission, `wait` again. A timeout means wait again after a
sanitized reread, not role completion; a pre-claim timeout means repeat
discovery. Call `heartbeat` with the current revision before a long review. The
facade uses a 30-minute claim lease.

For Git, prove the identity is a commit with
`git cat-file -e '<identity>^{commit}'`, inspect that exact object and its
relevant base diff, and recheck it before submission. For an artifact, verify
the recorded cryptographic digest against the materialized artifact. If exact
materialization is impossible, request Human Decision.

Review the published explainer as evidence, but do not maintain it: vadi owns
the one-site-per-run projection and its shared TODO list. Publication does not
replace checkpoint-bound source review.
