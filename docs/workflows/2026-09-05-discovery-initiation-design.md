# Discovery and coordination initiation

Status: implementation authorized in conversation on 2026-09-05.

## Contract

Expose four workflows: Discovery, Implementation, Babysitting, Review.
Discovery uses two linked runs in the same sessions: approved spec, then
verified tickets. Claude Fable 5.1/high is vadi and Codex Astra/high is
prativadi. Implementation uses fresh Codex Sol/high and Claude Opus sessions.
User-only Matt Pocock skills remain explicit human entry points, unchanged.

Auto-discover relevant repository instructions, domain docs, ADRs, specs,
tracker discussions and code. Record sources and their revisions. Both roles
investigate independently before comparing conclusions. Consolidate questions
for the human; facts are the agents' responsibility. Scope begins with an
exploratory objective and is refined through recorded human decisions.

Reuse the existing run-start explainer gate for scope verification and the
reviewer's research receipt. Its initial source manifest excludes vadi's
research conclusions. The reviewer records its independent findings before
comparison. Preserve kernel/schema, complete checkpoints and publication gates.
This is skill-enforced research provenance, not a new kernel attestation.

Explicit invocation waits yield with the run ID and next command, then exact
resume on the next user message. They do not masquerade as terminal states,
protocol failures or meaningless Human Decisions. Product decisions still use
the existing scope/intent/authority path.

Run 1: research, invoked grilling, invoked to-spec, exact spec review and
revision, verified publication, terminal approval. Run 2: invoked to-tickets,
reviewed decomposition, human granularity approval, publication and independent
verification of IDs, bodies and dependency edges. Bind it to run 1's approved
spec digest. User approval requirements inside upstream skills remain intact.

New Review runs persist across REQUEST_CHANGES and re-review updated heads
until the exact approved head satisfies required checks. Authors own fixes.
Persisted pr_review runs retain their previous one-shot semantics. Babysitting
retains scoped repairs and separate human merge authority; babysit is its
compatible stored alias. New workflow values use discovery, implementation,
babysitting and review.

## Validation

Source-contract tests check distributed role reference parity, startup routing,
independence, explicit invocation waits, linked runs and readiness distinctions.
Run existing Rust and shell role/poll/canary tests to protect protocol behavior.
No release, installation or archived v3 change is part of this implementation.
