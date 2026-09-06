# Persistent Review workflow

Read this reference for every new `workflow=review` run. Review covers one or
more PRs in one canonical repository. It reuses one complete manifest and one
atomic checkpoint verdict for the run; a member is not a child run, kernel
state, or independently finalizable checkpoint.

## Frozen member scope

Resolve the requested selection criteria into a concrete initial set. Do not
guess between materially different eligible sets. Deduplicate every full
canonical `https://github.com/OWNER/REPO/pull/NUMBER` URL, freeze membership,
and declare each URL as an objective ref `review_member=<URL>` plus one stable
distinct deliverable ID whose description repeats that URL. New PRs do not
enter automatically. The scalar task reference is null unless a real parent
task exists; never put a list or arbitrary first member there.

Initial batch Review is limited to one canonical repository. Independently
verify every member after exact join. Existing single-PR `workflow=review`
runs keep their exact scope. Legacy `pr_review` remains one-shot and one PR per
run; it is not discoverable as persistent Review. Scope expansion requires an
explicit amendment and invalidates the prior global binding.

## Complete review-round record

For each member record canonical identity; author and acting reviewer
identities; full head and base revisions; dependency relationship; review
basis; findings; proposed and adjudicated verdict; exact body and body digest;
formal review receipt coordinates; required checks; unresolved blocking
feedback; timestamps; disposition; and next action. Keep raw private exports
outside public repositories.
Encode the dependency relationship as `dependencies`, a deduplicated array of
in-batch PR numbers (empty when independent); self and out-of-batch references
are invalid current-basis evidence.

Finish the complete initial review of every active member before submitting the
first candidate. Pending CI and an adjudicated `REQUEST_CHANGES` are valid
complete-round evidence; an uninspected member is not. Prativadi independently
investigates every member and adjudicates every finding. The Baton receives one
atomic checkpoint verdict; per-member APPROVE or REQUEST_CHANGES values are
artifact content only. Revisions always replace the complete batch candidate.

## External writes and receipts

Before preparing or submitting each external verdict, read and follow
[formal review submission](review-submission.md).

After semantic approval of a complete round, vadi may submit the approved
external reviews during Finalizing. Immediately before each write, recheck the
exact PR, head/base, actor versus author, authority, and adjudicated exact body.
Self-review, missing authority, or drift blocks only that write. Never patch or
rebase another author's branch and Never merge.

Persist each write result immediately in private run evidence. Before retrying
an interrupted or uncertain write, query existing reviews and require an exact
actor, PR, head, state, and body digest match; confirmed unchanged verdicts have
zero duplicate submissions. After all authorized writes, withdraw approval,
stage a complete receipt-bearing replacement, and have prativadi independently
re-query every receipt before the next approval. The artifact distinguishes
review-round work, writes still authorized, writes already confirmed, and
readiness verification. Ambiguous receipt evidence is not success.

Deterministic GitHub fixtures and live operation use the same cases: mixed
APPROVE/REQUEST_CHANGES, pending-to-green checks, head drift immediately before
a write, base/dependency drift, new blocking feedback, author repair, merged or
closed disposition, and interruption immediately before or after a write.

## Drift, waiting, and completion

On head/base movement or new blocking findings, request supersession during
Reviewing or withdraw approval during Finalizing. Recheck the changed member
and affected dependent PRs. Preserve unaffected evidence only after checking
that it remains applicable; every replacement still covers all frozen members.

Finalizing permits bounded read-only GitHub readiness observations and only the
exact previously adjudicated review submissions. Pending author or CI activity
uses the existing 30-second external wait, then fresh facade and GitHub reads
plus an own heartbeat; GitHub never wakes the Baton. Continue runnable work on
other members without partial checkpoints or sub-Batons.

Persistent Review remains active after REQUEST_CHANGES. Every open member must
have the current adjudicated APPROVE, a verified exact formal receipt, required
checks passing, and no unresolved blocking findings or requested changes. A
merged or closed member instead carries its verified actual disposition; do
not invent green CI or current approval. Finalize only after both roles verify
the complete current readiness/disposition manifest, semantic approval, and
the existing explainer/private-Site gate. Review grants no merge authority.
For member-scoped persistent Review, the public facade enforces this final boundary from the credential-checked role
snapshot: it materializes every checkpoint analysis digest through the pinned
kernel `role analysis` interface and validates the documented per-member record
and exact receipt coordinates before passing the same copied finalize action to
the kernel's atomic revision check. It never reads Baton or artifact files
directly.
Every new non-exact Review start requires at least one `review_member`; use an
exact run ID to resume legacy member-less Review state.
The analysis checkpoint kind is intentional: these deliverables are immutable
non-code review, receipt, and readiness records, not changes to a PR branch.
The facade requires each frozen member exactly once and checks canonical URL
and disposition/current-evidence status. Every member carries identity, full
revisions, timestamp, review basis, findings/verdict fields, receipts/checks,
and a next action; an open member additionally needs green checks, no blocking
feedback, adjudicated approval, exact body digest, and a matching formal
receipt. This is a role-contract gate around existing kernel artifacts, not a
new kernel schema.
Scalar legacy Review runs without `review_member` references retain their
original kernel-only finalization behavior.
