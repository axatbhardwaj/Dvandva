# Dvandva skills 0.3.8

This release restores the HTML house-format skill and separates planning from
implementation without weakening v4 checkpoint or publication checks.

- Run implementation sessions with **Sol/high as Codex vadi** and **Opus as
  Claude prativadi**. Use Astra/Fable in separate planning sessions; hand off a
  concise approved plan. They remain optional advisers for design questions.
- Install the restored **html-deliverables** skill alongside setup-dvandva,
  vadi, and prativadi. It includes the shared template, standalone validation,
  and desktop/mobile visual-review guidance. Sol/medium authors HTML; Opus reviews; Codex
  publishes the exact approved bytes through the existing owner-only Sites gate.
- Preserve failed poll output and exit status, reject malformed responses, and
  distinguish tool failures from human cancellation. Recovery observes fresh
  state and uses bounded retries, with explicit environment-blocker reporting.
- Allow genuine intent and authority questions in autonomous runs. Scope
  decisions still require concrete proposals; recorded options, repeat-question
  protection, deterministic recovery, and history validation remain enforced.
- Report approved delivery separately from pending publication during a Sites
  outage. Publication is still required for `done` when Codex participates.

CI retains the archived runtime suite and excludes only six v3 README prose
assertions superseded by the earlier v4-only README change (`ee66a47`).

Kernel **0.3.8** writes **dvandva.run.v2** with **role API 2**. Linux x86_64 only.
The private kernel remains outside PATH and unpublished on crates.io. The v3
plugin and historical sources remain retired and unchanged. Setup does not
migrate runs or alter harness goals; existing versions remain installed.

Update all four skills and run the released setup skill's update operation.
Start fresh host sessions to load the updated instructions. Neither harness
launches the other, and optional model advice adds no Baton participant.
