# V4 Git discipline

Status: accepted design draft; inactive while the repository's retirement notice remains in force.

This policy is the shared Git discipline for both Dvandva lanes. Active v4
agent instructions should point here rather than restating it.

## Local change policy

- Preserve unrelated and pre-existing work.
- Prefer granular, recoverable commits around 200 changed lines. Treat that as
  a sizing guide, not a reason to split one semantic change or generated output
  artificially.
- Use semantic commit messages.
- Make each checkpoint straightforward to review, cherry-pick, or revert.
- Record the checkpoint commit and verification evidence at every source-code
  handoff.

## Stack policy

- Use the installed `gh stack` extension for stacked branch and pull-request
  workflows.
- Keep parallel track commits independently recoverable before integrating them
  into the run's canonical branch or stack.
- Require credited review from the opposite model family before a track commit
  enters the canonical branch or stack.
- Treat `gh stack push`, `gh stack submit`, `gh stack merge`, and any equivalent
  remote mutation as separately authorized actions. A Dvandva run may perform
  them only when its task explicitly grants that authority.
- A local checkpoint never implies permission to push, open a pull request,
  merge, release, or publish source changes.

## Handoff evidence

Every Git handoff identifies:

- the exact commit;
- its parent or stack position;
- the changed scope;
- verification performed;
- unresolved risks;
- the next owner and action.

The current host was checked with `gh stack --help` on 2026-08-26; the extension
exposed local stack management plus separately visible push, submit, sync,
rebase, and merge operations.
