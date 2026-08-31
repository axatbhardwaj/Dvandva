# Prativadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--run-id ID] [--autonomous]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_FILE
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
poll  SESSION RUN_DIR AFTER_REVISION [MAX_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
explainer SESSION RUN_DIR
analysis SESSION RUN_DIR DIGEST
upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
repair-policy SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
claim SESSION RUN_DIR EXPECTED_REVISION
reclaim SESSION RUN_DIR EXPECTED_REVISION
```

## Start and snapshot contract

Exact joins pass only `--run-id` unless the human explicitly supplied objective,
reference, task, or deliverable coordinates to compare. Never
invent an objective for an exact join. Exact run ID selects state but never
amends or overrides scope: surface `scope_mismatch` without claiming or
working. Without an exact run, use `--wait`; surface multiple matches for human
selection rather than choosing newest.

Prativadi never creates a run. If requested scope belongs in a separate run,
return that choice to the human so vadi/worker can create it.

Surface `ambiguous`, `busy`, `run_missing`, and `upgrade_required` rather than
guessing. Surface the start outcome and canonical snapshot before domain-tool
work.

For `publication_unreadable`, run `repair-policy` with the exact returned run
directory and revision; it installs the readable channel and clears the current
obligation's receipts so the publisher restages.

For `upgrade_required`, run `upgrade` with the exact returned run directory,
harnesses, and revision. Upgrade clears claims: use its returned revision with
`claim`, then read a fresh v2 snapshot. For ordinary expired-claim recovery,
exact `start --run-id` automatically reclaims a claim owned by the same
session. Reserve direct `reclaim` for a revision explicitly returned by the
facade, and follow it immediately with `read`. Never route migration through
an ordinary action payload.

After every facade operation, use the fresh facade snapshot. `next_actions`
combines `advisory_actions` and ordinary `legal_actions`; semantic work happens
only when the returned advisory action authorizes it. Apply only a returned
legal action. `request_human_decision` may be selected directly from
`legal_actions` solely for a decision that is the human's alone — `scope`,
`intent`, or `authority` — and never for protocol approval; it is never an
ordinary wake or action.

Protocol-internal problems never block on human approval, because the human may
be absent. Every one has a deterministic recovery to take instead:
`publication_unreadable` takes `repair-policy`, `upgrade_required` takes
`upgrade`, an expired own claim takes exact `start --run-id`, a
changes-requested explainer takes `stage_explainer` again, and a wait timeout
(`wait_outcome: idle_timeout`) takes a fresh snapshot and another wait. The
kernel never leaves `request_human_decision` as the only way forward.

## Checkpoint and review bindings

For every `apply` action, the role writes its JSON to a private temporary file
with mode 0600, passes its path as `ACTION_FILE`, and deletes it after `apply`.

Review only when `advisory_actions` includes `review_checkpoint`. Materialize
the exact immutable checkpoint, whose complete deliverable manifest covers the
canonical deliverable IDs exactly once. A `git` checkpoint materializes from its
commit object names; an `analysis` checkpoint materializes through
`dvandva-role.sh analysis SESSION RUN_DIR DIGEST`, which verifies each cited
digest against the staged bytes before returning them. Never select a review
target from a moving branch `HEAD` or the vadi's mutable worktree. Do not apply
worker-owned `submit_checkpoint`,
`request_checkpoint_supersession`, `withdraw_approval`, or `finalize` actions.
The authorized checkpoint is a post-implementation delivery candidate. Never
invoke a companion against partial work, an implementation-in-progress, or a
mutable branch; vadi does not submit those as checkpoints.

Capture the authorized snapshot's checkpoint identity, manifest digest, and
scope revision before reviewing. For the Spec axis, materialize one immutable
spec snapshot from the canonical objective, references, task, deliverables, and
any exact referenced issue/spec bytes. Store it in a private file, compute its
sha256, and pass that file rather than a mutable URL or live issue. Record the
original reference and `spec_sha256`; if there is no external spec, the
canonical scope snapshot itself is the spec.

**REQUIRED WHEN AVAILABLE:** Invoke `code-review` once for each newly authorized
complete `git` delivery candidate. It is available only when advertised in the
current host session with model invocation enabled. Materialize the exact
checkpoint as `HEAD` in an isolated checkout and verify `git rev-parse HEAD`
equals the captured checkpoint identity. Give `code-review` an immutable
fixed-point commit SHA and the immutable spec snapshot. Prefer a comparison
base explicitly named by canonical scope; otherwise pin the merge-base with the
repository's remote default branch. Never pass a symbolic branch, mutable spec,
or mutable worktree state.

Treat the Standards and Spec reports as review evidence, not as the Dvandva
verdict. Verify their findings against the checkpoint and adjudicate every one
before recording the bound review. An `analysis` checkpoint does not invoke
`code-review`; inspect its facade-verified bytes on both axes natively. A
companion is unavailable when it is absent, hidden, user-only, unreadable,
rejected by the host, or fails to return both reports. In that case, complete
the same two axes as a native fallback without installing or changing skills.

Put this compact block under `What was verified` in the current session's
five-part handoff:

```text
review_mode: <matt-code-review|native-analysis|native-fallback>
reviewed_checkpoint_identity: <captured checkpoint identity>
reviewed_manifest_digest: <captured manifest digest>
reviewed_scope_revision: <captured scope revision>
fixed_point_sha: <full commit SHA|null for analysis>
spec_reference: <canonical reference|canonical-scope>
spec_sha256: <sha256 of exact spec snapshot bytes>
axis_results: <Standards result>; <Spec result>
finding_adjudication: <each finding accepted/rejected with checkpoint evidence>
fallback_reason: <reason|null>
```

This is operator-visible session evidence, not Baton state or a peer transport.
Do not claim that raw companion reports or rejected findings were staged in the
explainer. `record_review` durably stores only the checkpoint-bound verdict and
accepted actionable findings. Those bound fields are all the peer may rely on
through the facade. Always disclose a native fallback; companion availability
never blocks an authorized review. Remove the private spec snapshot after
adjudication.

This selection relies on the hosts' documented default invocation policy:
[Codex skills](https://learn.chatgpt.com/docs/build-skills#how-chatgpt-and-codex-use-skills)
are implicitly invocable unless `allow_implicit_invocation` is false, and
[Claude Code skills](https://code.claude.com/docs/en/skills#control-who-invokes-a-skill)
are model-invocable unless `disable-model-invocation` is true. Missing, hidden,
or user-only installations still take the native fallback above.

Before a verdict, read or claim a fresh snapshot. Compare all three captured
coordinates with the current checkpoint and, for Git, verify `git rev-parse
HEAD` still equals its identity. If any value differs, discard the reports and
restart from the newly authorized checkpoint; never rebind old evidence by
copying fresh coordinates onto it. Otherwise copy the matching current
coordinates into the action. Never type, increment, or reuse them from an older
snapshot. Bind every verdict to all three:

```json
{"type":"record_review","verdict":"changes_requested","checkpoint_identity":"<snapshot.checkpoint.identity>","manifest_digest":"<snapshot.checkpoint.manifest_digest>","scope_revision":<snapshot.checkpoint.scope_revision>,"findings":["<actionable finding>"]}
```

When a pending supersession is returned in `reviewing`, accept it only through
the reviewer-owned action below. The worker owns requesting supersession and
withdrawing approval.

Publication never substitutes for supersession or withdrawal.

```json
{"type":"accept_checkpoint_supersession"}
```

## Human Decision

Every request declares what it asks for: `scope` for what the work should cover,
`intent` for which reading of the request is meant, or `authority` for
permission that is the human's alone to give. There is deliberately
no approval kind: a protocol-internal problem has a deterministic recovery, and
the human may be absent.

The options are the decision. The human answers by choosing one of them, and
the kernel refuses any other answer. A `scope` decision resolves through the
chosen `proposals` entry (one concrete scope per option) or an explicit scope
amendment; an `intent` or `authority` answer is recorded as an objective
reference of that kind. A pause that would change nothing about the run cannot
be resolved, and the decision just answered cannot be asked again.

A run started with `--autonomous` admits a decision only as a choice among
scope proposals, so when the human may be absent there is no admissible shape
for "please approve": every pause is a set of concrete scopes the kernel
applies itself.

Use only the minimal request. The kernel derives contact and resume routing:

```json
{"type":"request_human_decision","kind":"scope","question":"<one decision>","evidence":["<verified fact>"],"options":["<concrete option A>","<concrete option B>"],"proposals":[{"objective":"<scope if A>","objective_refs":[],"task_reference":null,"scope_deliverables":[{"id":"<deliverable ID>","description":"<deliverable description>"}]},{"objective":"<scope if B>","objective_refs":[],"task_reference":null,"scope_deliverables":[{"id":"<deliverable ID>","description":"<deliverable description>"}]}]}
{"type":"resume_human_decision","answer":"<one of the recorded options>"}
```

`answer_human` maps to `resume_human_decision`; copy the human's answer. If the
human changes scope, populate every field below only from explicit
human-approved values. `task_reference` must be a JSON string or `null`; it is
not returned by the Human Decision object and must never be inferred:

```json
{"type":"resume_human_decision","answer":"<human-approved answer>","scope_amendment":{"objective":"<human-approved objective>","objective_refs":[{"kind":"<human-approved ref kind>","value":"<human-approved ref value>"}],"task_reference":"<human-approved task reference>","scope_deliverables":[{"id":"<human-approved deliverable ID>","description":"<human-approved deliverable description>"}]}}
```

## Explainer obligation

Each semantic handoff opens an obligation. For the current one, the
Codex harness stages the explainer's bytes into the run directory and the
Claude harness reviews those exact bytes, regardless of semantic casting. Staging is first: the gate binds a digest, not a URL.
A new handoff replaces the current obligation, so the gate requires the current
obligation to be staged and reviewed, not every obligation the run has opened.
The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.

For `stage_explainer`, write the explainer HTML to a private path and
copy `publication_binding.obligation` unchanged from the fresh snapshot. The kernel
hashes the bytes, stores them at `explainer/<source_digest>.html` inside the run
directory, and binds that digest to the obligation. Staging different bytes
discards any earlier rendering and review of the obligation:

```json
{"type":"stage_explainer","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_path":"<absolute path to the explainer HTML>"}
```

Every receipt carries `after_seq`, copied from
`publication_binding.receipt_seq` in the same fresh snapshot. Only receipts
advance it, so an unrelated peer heartbeat or progress report never invalidates
a prepared write, while a delayed or out-of-order receipt is refused instead of
overwriting newer state. Re-applying an identical receipt is a no-op.

For `review_explainer`, read the staged bytes through the facade with
`dvandva-role.sh explainer`, which verifies the digest for you, then copy the
same obligation and `publication_binding.artifact.source_digest` unchanged:

```json
{"type":"record_explainer_review","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","verdict":"approved","findings":[]}
```

`publish_explainer` is optional and never gates the run. It records a
human-facing Codex Site that renders the already-staged bytes; its
`source_digest` must equal the staged digest. Reuse one stable Site ID for the
run and record a new Site version for each deployment:

```json
{"type":"record_explainer_publication","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

Never record a verdict on bytes you did not read. Recording an unread approval,
or substituting a Claude Artifact, generic publisher, or any other
silent fallback, cannot satisfy the gate.

A run whose `publication_policy` names a channel the reviewer cannot read is
refused at `start` with `publication_unreadable`; repair it with `repair-policy`
rather than working around it.

## Liveness

Before and during long authorized work, publish progress so the peer can tell
slow from dead. `report_progress` also renews your own lease, and is never a
reason for the peer to wake:

```json
{"type":"report_progress","phase":"publishing_explainer","detail":"<current step>"}
```

Phases are `working`, `publishing_explainer`, `reviewing_explainer`,
`reviewing_checkpoint`, and `waiting`. Read the peer's phase, claim state, and
lease expiry from the snapshot's `peer` block; never infer a dead peer from an
expired lease alone.

## Run boundaries and handoff

The human starts the peer session with the returned prompt. Neither role
invokes or wakes the other harness. User-created harness goals remain
unchanged. Third-party user-invoked workflow skills run only when the human
explicitly invokes them in this session. The required model-invocable
`code-review` companion above is local to prativadi and is not a peer-harness
invocation.

After each handoff, report these exact fields and, in the same turn, continue
in a foreground local wait until terminal state or human stop:

- What changed
- What was verified
- What is blocked
- Who owns the next action
- Exact command or prompt

The foreground wait is `poll`, which re-enters the kernel wait on every
`idle_timeout` until a real wake, a terminal run, or its budget. When `poll`
returns with `wait_outcome: idle_timeout`, call it again immediately; on any
other outcome, read a fresh snapshot and act. Ending the turn is not a wait: it
stops the poll, lets the lease lapse, and stalls the peer. Heartbeat before long
authorized work. Keep action files private (mode 0600), exclude credentials,
and delete them after `apply`.
