# Workflow-mode GitHub evidence

## Local CLI evidence

- `gh --version` returned `2.98.0 (2026-08-20)`.
- `gh pr edit --help` says `--add-reviewer` can add or re-request a reviewer.
- `gh pr view --help` exposes `--json` and `headRefOid`.

## Authoritative routes

- Re-request: [GitHub CLI edit](https://cli.github.com/manual/gh_pr_edit) and [GitHub review requests](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/getting-involved-with-pull-requests/requesting-a-pull-request-review).
- PR/head query: [GitHub CLI view](https://cli.github.com/manual/gh_pr_view).
- Formal reviews: [REST pull-request reviews](https://docs.github.com/en/rest/pulls/reviews?apiVersion=2022-11-28) or `gh api graphql` over [PullRequestReview](https://docs.github.com/en/graphql/reference/objects#pullrequestreview); use the latter when a receipt needs review ID, author, state, commit, and body while the PR query supplies `headRefOid`.
- Fable is advisory, not a Baton participant: [skill-only interface design](../superpowers/specs/2026-08-27-skill-only-run-interface-design.md) and [model selection](../model-selection.md).
