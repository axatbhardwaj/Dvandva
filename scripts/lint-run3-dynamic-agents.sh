#!/usr/bin/env bash
# Lint the Run 3 dynamic-agent documentation contract across the Dvandva surface.
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR" || exit 1

failures=0

SURFACE_PATHS=(
  README.md
  product.md
  docs/protocol
  docs/workflows
  plugins/dvandva/agents
  plugins/dvandva/commands
  plugins/dvandva/references
  plugins/dvandva/skills
)

if ! command -v rg >/dev/null 2>&1; then
  echo "FAIL: ripgrep is required for Run 3 dynamic-agent lint"
  exit 1
fi

search_fixed() {
  local pattern="$1"
  rg -n -F -m1 -- "$pattern" "${SURFACE_PATHS[@]}" 2>/dev/null | head -n1 || true
}

search_regex() {
  local pattern="$1"
  rg -n -m1 -- "$pattern" "${SURFACE_PATHS[@]}" 2>/dev/null | head -n1 || true
}

pass_hit() {
  local label="$1"
  local hit="$2"
  echo "PASS: $label"
  echo "  $hit"
}

fail_missing() {
  local label="$1"
  local detail="$2"
  echo "FAIL: $label"
  echo "  missing contract: $detail"
  failures=$((failures + 1))
}

require_fixed() {
  local pattern="$1"
  local label="$2"
  local hit

  hit="$(search_fixed "$pattern")"
  if [[ -n "$hit" ]]; then
    pass_hit "$label" "$hit"
  else
    fail_missing "$label" "$pattern"
  fi
}

require_regex() {
  local pattern="$1"
  local label="$2"
  local hit

  hit="$(search_regex "$pattern")"
  if [[ -n "$hit" ]]; then
    pass_hit "$label" "$hit"
  else
    fail_missing "$label" "$pattern"
  fi
}

require_fixed "agent_instances" "surface names Run 3 agent_instances"
require_regex 'seed roster|static roster[^[:alnum:]]+as seed|static roster.*seed|seed.*static roster' "surface treats the roster as a seed/static roster"
require_regex 'run-scoped.*dynamic (agents|agent|instances|instance)|dynamic (agents|agent|instances|instance).*run-scoped' "surface documents run-scoped dynamic agents or instances"
require_regex 'explicit (Codex )?subagent handle closure|subagent handle closure|explicit closure|every generated handle must be explicitly closed|close[sd]?.*subagent handle|close[sd]?.*generated handle' "surface requires explicit subagent handle closure"
require_regex 'write-path disjoint|write path disjoint|dynamic write-path|conflict_group|serializ(e|ation).*conflict_group' "surface documents write-path disjointness or conflict_group serialization"
require_regex 'no daemon|There is no daemon|without adding a daemon' "surface rejects a runtime daemon"
require_regex 'no mailbox|without adding a daemon, mailbox, or central runtime process|mailbox, or central runtime process' "surface rejects a runtime mailbox"
require_regex 'hidden scheduler|hidden central process|hidden process that owns the control loop' "surface rejects a hidden scheduler or central owner"
require_fixed 'Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models' "surface documents Anthropic opus/sonnet model-class mapping"
require_fixed 'Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`' "surface documents Codex gpt-5.5/gpt-5.4 model-class mapping"
require_regex 'generated agents?.*(do not|must not|never).*(own|set|mutate).*(assignee|active_roles|transitions)|(assignee|active_roles|transitions).*(do not|must not|never).*(belong to|owned by).*(generated agents?)' "surface says generated agents do not own assignee, active_roles, or transitions"

if [[ "$failures" -gt 0 ]]; then
  echo "Run 3 dynamic-agent lint failed with $failures contract gap(s)."
  exit 1
fi

echo "Run 3 dynamic-agent lint passed."
exit 0
