# Discovery: spec, then tickets

Use this only for `workflow=discovery`. Read initiation.md first. Discovery is
two separately terminal runs, normally in the same Fable-vadi/Astra-prativadi
sessions. Spec and ticket review are native analysis review, not Git code-review.

## Run 1: approved spec

Start with `--objective-ref workflow=discovery --objective-ref discovery_stage=spec`
and `--required-deliverable spec="Reviewed specification"`. The exploratory
objective describes the problem to resolve, not an invented implementation.

1. Complete automatic source discovery and both independent startup
   investigations from initiation.md. Record source revisions, disagreements,
   settled facts and remaining human decisions. Reconcile facts through code,
   primary sources or a bounded experiment. Every consequential uncertainty
   must be resolved or explicitly accepted before spec production.
2. When the human explicitly invokes `/grill-with-docs` (or their selected
   grilling skill), vadi conducts it. Consolidate prativadi's questions; ask
   only questions whose prerequisites are settled. The human owns product
   decisions. Record glossary/ADRs as the invoked skill requires. Apply actual
   scope changes through Human Decisions, advancing `scope_revision`; keep
   answers and rejected alternatives so resumption does not repeat the interview.
3. Once questions are settled, present scope, exclusions, decisions and remaining
   accepted uncertainty. Enter `waiting_for_skill` for `/to-spec` if it has not
   been explicitly invoked in this session for this work. Never manufacture
   an invocation from agreement, a generic continue or starting discovery.
4. On `/to-spec`, follow the installed skill unchanged: confirm test seams with
   the human, synthesize and publish to the configured tracker. Its
   `ready-for-agent` label alone is not Dvandva approval. Tell the user that
   paired review is pending; downstream implementation requires this run's
   approved checkpoint, not a tracker label.
5. Vadi captures the complete published spec bytes and source/decision evidence,
   stages an analysis artifact, and submits the complete spec checkpoint.
   Prativadi reviews the exact digest for scope, evidence, contradictions,
   testability, exclusions and accepted human decisions. Both compare the
   tracker body against the captured spec. Request concrete changes; vadi
   revises the existing spec in the authorized cycle. New human decisions use
   the existing Human Decision path; another user-only skill invocation is
   never inferred. Re-capture changed bytes and submit a complete replacement.
6. Finalize only after exact spec approval, live tracker-content verification
   and the existing explainer/publication gate. Record the run ID, checkpoint
   identity, manifest digest, scope revision, spec digest and tracker ID.

## Run 2: verified tickets

The human's `/to-tickets` invocation requesting the next stage authorizes a new
linked run; use `--new-run`, `workflow=discovery`, `discovery_stage=tickets`,
`predecessor_run=<run-1 ID>`, `spec_digest=<approved digest>` objective refs and
`--required-deliverable tickets="Reviewed published ticket graph"`.
Exact-join the new run in the same peer session using its new peer prompt.
Never silently reuse run 1's terminal Baton or migrate an active run's scope.

1. Verify run 1 is done and its approved spec digest matches the full source
   read for ticketing, including relevant tracker comments. A changed spec must
   be reconciled and reapproved in a separate spec revision run before ticketing;
   terminal state stays immutable. Comments are evidence, not silent spec edits.
2. Vadi follows `/to-tickets`: draft tracer-bullet slices and blocking edges.
   The human approves granularity, dependencies and splits before publication.
   Preserve any installed skill's wide-refactor handling and parent-issue rules.
3. Publish the approved breakdown in dependency order. Retain returned ticket
   IDs; on interruption discover existing outputs before retrying creation.
   Capture ticket bodies, IDs, native blocking edges (or declared local edges),
   spec-to-ticket coverage and the human's breakdown approval into one complete
   analysis artifact. Submit it as the tickets checkpoint.
4. Prativadi reads the actual tracker tickets and compares them with the staged
   bytes and approved spec: no missing requirements or extra scope, no cycles,
   real blocking edges, verifiable acceptance criteria and coherent slices.
   Return findings through record_review. Vadi updates the same tickets and
   re-checkpoints. A requested split/merge or changed dependency returns to the
   human approval required by the skill; peers cannot approve on the human's behalf.
5. Finalize after verified ticket bodies/edges, exact checkpoint approval and
   the existing explainer/publication gate. Hand off spec revision, ticket IDs,
   decisions, dependencies and test seams to fresh implementation sessions.

The kernel binds artifact bytes and review coordinates. Agents own source
coverage, independent investigation and tracker semantics; do not claim those
are schema-enforced. Progress distinguishes working, waiting for the peer,
waiting for a human answer and waiting for a skill invocation.
