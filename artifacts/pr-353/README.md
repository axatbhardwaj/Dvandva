# PR 353 Local Archive

This directory stores a local copy of PR 353 history so Dvandva research does not require repeated GitHub visits.

## Files

- `pr-353-full-history.json` - combined archive containing PR object, comments, commits, changed files, and reviews.
- `comments-timeline.md` - readable issue-comment timeline.
- `commits-timeline.md` - readable commit timeline.
- `files.md` - changed-file table.
- `raw/pr.json` - raw pull request object from the GitHub API.
- `raw/comments.json` - raw issue comments.
- `raw/commits.json` - raw pull commits.
- `raw/files.json` - raw changed files.
- `raw/reviews.json` - raw pull reviews.

## Refresh Commands

Run from repo root:

```bash
gh api repos/defi-com/monorepo/pulls/353 > artifacts/pr-353/raw/pr.json
gh api --paginate --slurp repos/defi-com/monorepo/issues/353/comments | jq 'add' > artifacts/pr-353/raw/comments.json
gh api --paginate --slurp repos/defi-com/monorepo/pulls/353/commits | jq 'add' > artifacts/pr-353/raw/commits.json
gh api --paginate --slurp repos/defi-com/monorepo/pulls/353/files | jq 'add' > artifacts/pr-353/raw/files.json
gh api --paginate --slurp repos/defi-com/monorepo/pulls/353/reviews | jq 'add' > artifacts/pr-353/raw/reviews.json
```

The current archive was exported on 2026-05-12.

