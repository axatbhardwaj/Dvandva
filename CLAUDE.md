# Dvandva — project instructions

## Model discipline

- **Fable never writes code.** When this session runs on a Fable-class model, all code — implementation, tests, even one-line fixes — is dispatched to subagents. Dispatch code to `gpt-5.6-terra` for routine work, `gpt-5.6-sol` for hard bounded work, and `gpt-5.6-luna` only for mechanically proven task classes; `gpt-5.5` is the fallback. Fable's job in the chair is judgment and taste only: decisions, plans, reviews, human-facing artifacts, and coordination writes (memory, todos, goal.json stamps). "Too small to dispatch" is the rationalization this rule exists to override.
- **Never invoke `codex exec` directly from the chair.** Every codex invocation rides a sonnet low-effort wrapper agent (standalone agent for one-offs, workflow lane for fan-outs), per `plugins/dvandva/skills/delegating-to-codex/`. Long runs are held by the wrapper's background-Bash + completion notification, never a sleep-poll.
- Model casting guidance lives in `docs/model-selection.md` (advisory scored table; `intelligence > taste > cost`; never haiku).

## The product (since 2.0.0)

The adversarial loop: skill + hook + workflow template, no binary. Dev home `adversarial-loop/` (tests, schemas, design README); shipped product `plugins/dvandva/` (skills, `hooks/adversarial/`, workflow template). The v3 Rust engine was removed on 2026-07-14; `dvandva 3.4.1` stays published on crates.io as the final binary release (history in git tags).

## Release checklist

- A release that touches plugin content (skills, hooks, references) must bump the plugin version in **all three** manifests: `plugins/dvandva/.claude-plugin/plugin.json`, `plugins/dvandva/.codex-plugin/plugin.json`, `.claude-plugin/marketplace.json` — plugin caches are version-keyed and will silently serve stale content otherwise.
- Before publishing, sweep user-facing version references by hand across **every** user-facing document (`grep -rnE '[0-9]+\.[0-9]+\.[0-9]+' README.md AGENTS.md CLAUDE.md product.md docs/ plugins/ --include='*.md' --include='*.json' --include='*.html'`), deriving the live plugin version from the manifests and checking the retired-binary references separately (final binary = `dvandva 3.4.1`) — the old `dvandva lint stale-version-ref` died with the binary; nothing automated fail-closes this anymore.
- Verification before any release claim: `bash adversarial-loop/tests/gate_test.sh` (44/44), `bash adversarial-loop/tests/template_smoke_test.sh`, `dvandva lint skills` on each shipped SKILL.md **only if** a 3.x binary is still installed locally (optional — the suite is the required gate).
- After publishing: `claude plugin update dvandva@dvandva`, and on the Codex side `codex plugin marketplace upgrade dvandva` (refreshes the Git marketplace snapshot — re-adding alone does not) followed by `codex plugin add dvandva@dvandva`; restart sessions to re-arm hooks.
