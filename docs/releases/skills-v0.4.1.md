# Dvandva skills 0.4.1

- Require a dedicated worktree for each new repository task across all five
  workflows. Resume the same task in its own worktree; preserve unrelated work
  and isolate writable reviewer checkouts.
- Require formal GitHub Approve or Request changes submissions in Review and
  legacy PR review. Comments and pending drafts do not count as submissions.
- Keep each adjudicated review body under 400 lines, including blank lines.
  Revise oversized bodies before submission; retain detailed evidence in the
  review artifact. Persistent Review finalization rejects oversized approvals.
- Preserve exact review receipts, independent verification, autonomous waiting,
  and existing merge-authority boundaries.

The private kernel is version 0.4.1; run schema `dvandva.run.v2` and role API 2
are unchanged. This release targets Linux x86_64. Update the four active skills
and run setup-dvandva to install the matching private kernel.
