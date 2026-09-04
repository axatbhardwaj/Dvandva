# Prativadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.
Read `references/model-selection.md` before selecting or dispatching a model.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--run-id ID] [--autonomous]
read  SESSION RUN_DIR
observe SESSION RUN_DIR
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

`observe` is read-only and claim-independent: it returns the snapshot without
verifying, renewing, or fencing any claim, so a watcher can tell a finished
run from a lapsed claim. It never substitutes for the claim-verified `read`
before a mutation.

Harness identity is protocol data. A Codex participant must use the harness
name `codex` (case-insensitive); aliases such as `codex-cli`, `gpt`, or
`openai` take the no-Codex branch and skip Sites publication.

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
work. The first `poll` is illegal until that start outcome has been shown to
the human.

For `publication_unreadable`, run `repair-policy` with the exact returned run
directory and revision; it installs the readable channel and clears the current
obligation's receipts so vadi restages.

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
changes-requested explainer waits for vadi to `stage_explainer` again and then
reviews the replacement digest, and a wait timeout (`wait_outcome:
idle_timeout`) takes a fresh snapshot and another wait. The kernel never leaves
`request_human_decision` as the only way forward.

## Workflow selection and prativadi lifecycle

Read objective ref `workflow=implementation|babysit|pr_review`; when absent, use
`implementation`. Existing checkpoint, supersession, explainer, Human Decision,
polling, and Matt `code-review` rules remain in force. Parents alone mutate
Baton or GitHub. Prativadi subagents remain read-only and receive only semantic
work authorized by the current snapshot.

In `implementation`, review each newly authorized complete checkpoint under existing immutable two-axis rules; never treat work in progress as a candidate.

In `babysit`, prativadi is an internal sanity filter, not the real reviewer. Independently check each exact fix head and CI evidence before vadi re-requests the existing colleague reviewer. Unresolved findings block that request, but neither internal approval nor thread resolution is colleague acceptance; feedback, changed head, or failed gate reopens the loop. After colleague approval, progress from `merge_ready` to `maintaining_ready` and refresh live GitHub between bounded Baton waits; GitHub does not wake Baton. Head, base, CI, approval, requested-change, or thread drift reopens the fix and review loop. Never merge without fresh human authorization.

In `pr_review`, make an independent first pass without vadi's report, covering diff, spec, standards, regressions, security edges, and practical failures. Then compare and adjudicate every vadi finding into final `APPROVE` or `REQUEST_CHANGES` that vadi submits. After the write, prativadi independently re-queries the same GitHub receipt: review ID, exact PR, actor, state, reviewed commit/head, and body digest. Head drift before both confirmations invalidates the attempt and restarts review. A Dvandva approval in `pr_review` approves the receipt-bearing review artifact; formal `REQUEST_CHANGES` still completes after confirmed submission. Sources: [workflow-mode evidence](../../../docs/research/2026-09-01-workflow-mode-github-evidence.md).

For every workflow, recoverable CI, review, scoped-branch failures, and other uncertainty stay autonomous. Before every actual `request_human_decision` for scope, intent, or authority, exchange evidence, attempt an available scoped fix, and optionally consult local Astra or Fable for a concrete unresolved design question within snapshot-authorized activity. Authority is permission the human alone can grant; unavailable capability is not a Human Decision. Escalate only an action both roles cannot establish as safe or an unavoidable external permission barrier; Astra and Fable are optional advisers and never additional Baton participants. In babysit, design is intent; security and secret-policy are scope or authority, never new kinds.

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

The immutable snapshot is the sole authorized Spec source. Explicitly instruct
`code-review` not to discover or fetch issue references from commit messages,
URLs, branches, or other files, and require its Spec report to identify the
supplied `spec_sha256`. If it uses any other source, omits that attestation, or
cannot honor the restriction, discard both companion reports and perform both
axes natively against the snapshot. Disclose `native-fallback` and the exact
reason; never accept mutable live bytes as review evidence.

Treat the Standards and Spec reports as review evidence, not as the Dvandva
verdict. Verify their findings against the checkpoint and adjudicate every one
before recording the bound review. An `analysis` checkpoint does not invoke
`code-review`; inspect its facade-verified bytes on both axes natively. A
companion is unavailable when it is absent, hidden, user-only, unreadable,
rejected by the host, fails the sole-spec-source rule, or fails to return both
reports. In that case, complete the same two axes as a native fallback without
installing or changing skills.

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

A run started with `--autonomous` requires concrete proposals for a `scope`
decision. Genuine `intent` and `authority` decisions remain available: autonomy
never supplies missing permission or chooses a human answer. Deterministic
protocol recovery, distinct options, and protection against repeated decisions
still apply in both modes. The kernel validates decision structure; the roles
must establish that the question actually requires human judgment.

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

Use the installed `html-deliverables` skill and its `template.html` for status
HTML. Run its standalone validator and inspect desktop/mobile rendering before
staging or approving. If the companion is missing, report the missing skill
and install the release's companion within the user's setup authority; never
invoke archived v3 tooling. The role parent retains every facade mutation.

Each work-carrying handoff opens an obligation. Vadi stages the explainer's bytes
into the run directory and prativadi reviews those exact bytes, regardless of which harness fills either role. Staging is first: the gate binds a digest, not a URL. For `run_started`, review this initial HTML before vadi continues domain
work; request concrete changes and review each replacement digest until clean.
The kernel withholds vadi's `work` advisory until this approval — the join
gate — so review it promptly on joining: the pair forms on your first receipt.
On an upgraded run, a complete pre-`0.3.3` receipt pair remains valid against
the stored fixed policy. If only the legacy author receipt exists, wait for
current vadi to restage before reviewing.
A work-carrying handoff replaces the current obligation; an approval preserves
the obligation and its receipts, so an approved delivery finalizes on the
explainer already staged and reviewed for its checkpoint. The gate requires the
current obligation to be staged and reviewed, not every obligation the run has
opened.
The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.

For peer-owned `stage_explainer`, vadi writes the explainer HTML to a private
path and copies `publication_binding.obligation` unchanged from the fresh
snapshot. The kernel hashes the bytes, stores them at
`explainer/<source_digest>.html` inside the run directory, and binds that digest
to the obligation. Staging different bytes discards any earlier rendering and
review of the obligation:

```json
{"type":"stage_explainer","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_path":"<absolute path to the explainer HTML>"}
```

Every receipt carries `after_seq`, copied from
`publication_binding.receipt_seq` in the same fresh snapshot. Only receipts
advance it, so an unrelated peer heartbeat or progress report never invalidates
a prepared write, while a delayed or out-of-order receipt is refused instead of
overwriting newer state. Re-applying an identical receipt is a no-op.

For prativadi-owned `review_explainer`, read the staged bytes through the facade
with `dvandva-role.sh explainer`, which verifies the digest for you, then copy `publication_binding.obligation` unchanged along with
`publication_binding.artifact.source_digest`:

```json
{"type":"record_explainer_review","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","verdict":"approved","findings":[]}
```

After prativadi records an approved local review, `publish_explainer` is
required work for whichever participant is Codex and wakes that participant.
If neither participant is Codex, skip Sites publication. Otherwise treat it as
a mechanical adapter:

1. Refresh the snapshot, then use `dvandva-role.sh explainer` to materialize the
   verified staged bytes into a private, isolated Sites checkout. Keep the exact
   HTML as the status page's rendered source; packaging may add hosting machinery
   but may not rewrite the reviewed content.
2. Invoke the host's `sites:sites-building` and `sites:sites-hosting`
   capabilities. Reuse `publication_binding.site_id` when present; otherwise
   create one Site exactly once. Push the exact source state, save one version,
   deploy it owner-only with the private deployment operation, and poll that
   deployment to terminal success.
3. Refresh the facade again. Record only the connector-returned Site ID, saved
   version, and production URL, bound to the still-current obligation, receipt
   sequence, and local digest. Reuse one stable Site ID for the run and record a
   new Site version for each approved digest:

```json
{"type":"record_explainer_publication","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

When Codex participates, finalization requires both the approved local digest
and this matching private Sites receipt. Without Codex, local approval is
sufficient. A connector failure remains Codex-owned publication work: report
the exact non-secret failure, publication owner Codex, and preserved source
digest. When the current checkpoint has exact semantic approval, report
`delivery approved; publication pending`; otherwise report the actual delivery
state. Retry only errors identified as temporary, with bounded 5, 15, then 30
second backoff; after each attempt, obtain fresh connector status and refresh
the facade. Leave the run active. A persistent non-transient failure is a
reported environment blocker, not an immediate actionable/poll retry loop.
Never record a guessed receipt, publish to broader access, substitute generic
hosting, or ask prativadi to review through the Site.

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

Before the first `poll`, after all semantic work and the handoff, the full
start outcome is the final protocol output immediately before that first wait.
The foreground wait is `poll`, which re-enters the kernel wait on every
`idle_timeout` until a real wake, a terminal run, or its budget. When `poll`
returns with `wait_outcome: idle_timeout`, call it again immediately; on any
other successful JSON outcome, read a fresh snapshot and act.
A nonzero exit or missing JSON alone does not establish a human interrupt.
Only an explicit human stop or host-reported cancellation establishes one;
exit 130 or 143 alone may also mean a process signal unrelated to the human.
For a failed poll, retain its exit status and non-secret error, take a fresh
read-only `observe` snapshot, and use its exact-run recovery. A fenced claim
requires observation before exact reclaim; a missing run never permits a
replacement run or domain work. Retry transient I/O failures with bounded
backoff (5, 15, then 30 seconds), refreshing state before resuming. Persistent
failure is an environment blocker: report the run, error, owner, and next
recovery action, preserving the active run rather than claiming a human stop.
A malformed or empty successful response is `invalid_poll_response`, not a
wake; inspect the pinned kernel/probe before retrying. After successful
recovery, resume the foreground loop.
Ending the turn is not a wait: it stops the poll, lets the lease
lapse, and stalls the peer. After an interrupt force-ends the turn, the next
turn, unless its message is an explicit human stop, first reads a fresh
snapshot, answers anything the human asked, and re-enters `poll`. A bare
continue or an empty resume is never a stop and never a no-op. Heartbeat before
long authorized work. Keep action files private (mode 0600), exclude
credentials, and delete them after `apply`. Sources: the reproduction recorded
in the [issue #22 owner refinement](https://github.com/axatbhardwaj/Dvandva/issues/22#issuecomment-5537575950),
verified locally with `gh issue view 22 --repo axatbhardwaj/Dvandva --comments`.
