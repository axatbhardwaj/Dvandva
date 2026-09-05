# Dvandva skills 0.4.0

This release adds Freeflow as the fifth active workflow and expands Review to
one explicitly scoped set of pull requests from the same repository.

- Freeflow supports paired investigations, reports, diagnostics,
  real-interface testing, and scoped test-and-fix work through one current
  plan and one immutable reviewed delivery.
- Review accepts an exact canonical PR set, freezes every member into the run
  scope, and produces one atomic run-level verdict. Legacy one-shot
  `pr_review` behavior remains unchanged.
- Review evidence now binds each member to the selected repository and records
  complete actor, timestamp, review, check, and terminal-disposition identity.
  Mixed-case reference kinds and malformed or ambiguous discovery candidates
  fail closed.
- GitHub-facing parsing and workflow validation are centralized in the shared
  role guard, while the public shell facades remain harness-independent.
- CI covers checkpoint guards, packaging, automatic discovery, browser
  isolation, two-role canaries, polling behavior, and the deterministic
  multi-PR lifecycle fixture.

Kernel **0.4.0** writes **dvandva.run.v2** with **role API 2**. Linux x86_64
only. The private kernel remains outside `PATH` and unpublished on crates.io;
the archived v3 plugin and sources remain unchanged.

Update all four released skills together, then run setup-dvandva update for
0.4.0. Existing run histories and older kernel versions are preserved. Start
fresh host sessions to load the new skill instructions.
