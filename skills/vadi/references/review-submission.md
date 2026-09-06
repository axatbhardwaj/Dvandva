# Formal GitHub review submission

Read this before preparing or submitting a verdict in persistent `review` or
legacy `pr_review` runs. Apply it separately to every PR in a batch.

1. Prepare the exact adjudicated body as UTF-8 with fewer than 400 lines
   (maximum 399, including blank lines; count with Python `len(body.splitlines())`).
   Keep all actionable blockers in that body; put extended evidence in the
   review artifact. If it is too long, revise it through prativadi before
   submission. Never truncate or rewrite the adjudicated body at write time.
2. After the existing approval and fresh PR/head/base/actor/authority checks,
   submit a **formal GitHub review** with event `APPROVE` or `REQUEST_CHANGES`.
   Use the create-review API with the verified `commit_id`, or
   `gh pr review <PR> --approve|--request-changes --body-file <FILE>` against the
   verified repository. CLI flags verified locally with `gh pr review --help`.
   A PR comment, `COMMENT` review, pending draft, or local report cannot satisfy
   this step. An authorization or platform failure remains an explicit blocker;
   do not substitute a comment. Continue other authorized batch work.
3. Query GitHub after the write and verify the server review ID, PR, actor,
   reviewed commit, exact body digest, and submitted verdict. GitHub returns
   `APPROVED` for `APPROVE` and `CHANGES_REQUESTED` for `REQUEST_CHANGES`;
   normalize only those states to the corresponding artifact verdict. Require
   a submitted review, not a pending draft. Drift or ambiguous evidence is not
   success. Persist the receipt, and let prativadi independently verify it using
   the existing receipt-bearing checkpoint and explainer gates.

Before retrying an uncertain write, query for the exact existing receipt to
avoid duplicates. Legacy `pr_review` still completes after confirmed
`REQUEST_CHANGES`; persistent Review continues until its readiness conditions
are met. This submission rule grants no merge authority.
