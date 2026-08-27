# Prativadi run contract

Use only `scripts/dvandva-role.sh`; the helper remains private and outside
`PATH`. First run `probe`. Resolve the session with `session-id`; if unavailable,
run `session-id --generate` once and retain that value in this harness session.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE OBJECTIVE [TASK] --wait
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_JSON
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
```

`start --wait` watches for a matching run without consuming model turns.
Exactly one valid candidate is claimed. None continues waiting; several are
returned for human selection. Corrupt, terminal, wrong-repository,
wrong-family, and live-claimed candidates are never silently selected. A lost
claim race returns to discovery.

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
```

Before submission, confirm the inspected object still equals
`checkpoint.identity` and use the current Baton revision for CAS. On a stale
revision, reread and review the newly assigned identity; never force the old
verdict. After submission, `wait` again. A timeout means wait again after a
sanitized reread, not role completion.

Review the published explainer as evidence, but do not maintain it: vadi owns
the one-site-per-run projection and its shared TODO list. Publication does not
replace checkpoint-bound source review.
