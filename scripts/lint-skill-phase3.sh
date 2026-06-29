#!/usr/bin/env bash
# Phase 3 contract lint for Dvandva skill loop text.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=0

require_text() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  if grep -Fq -- "$pattern" "$file"; then
    echo "PASS: $label"
  else
    echo "FAIL: $label"
    echo "  missing pattern: $pattern"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
}

reject_text() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  if grep -Fq -- "$pattern" "$file"; then
    echo "FAIL: $label"
    echo "  rejected pattern: $pattern"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  else
    echo "PASS: $label"
  fi
}

for role in vadi prativadi; do
  skill="$ROOT_DIR/plugins/dvandva/skills/$role/SKILL.md"
  require_text "$skill" "Resolve the active baton path before reading or writing" "$role skill resolves active baton path"
  require_text "$skill" "DVANDVA_BATON_FILE" "$role skill supports explicit baton file"
  require_text "$skill" "DVANDVA_RUN_DIR" "$role skill supports explicit run directory"
  require_text "$skill" "DVANDVA_RUN_ID" "$role skill supports run id"
  require_text "$skill" ".dvandva/runs/<run_id>/baton.json" "$role skill documents run-scoped baton path"
  require_text "$skill" "BATON_FILE" "$role skill names BATON_FILE variable"
  require_text "$skill" "BATON_NEXT_FILE" "$role skill names BATON_NEXT_FILE variable"
  require_text "$skill" 'dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"' "$role skill writes through resolved baton path"
  require_text "$skill" "original_ask" "$role skill surfaces original ask"
  require_text "$skill" "run_id" "$role skill surfaces run id"
  require_text "$skill" "research_ref" "$role skill surfaces research ref"
  require_text "$skill" "plan_ref" "$role skill surfaces plan ref"
  require_text "$skill" "turn_cap" "$role skill surfaces active turn cap"
  require_text "$skill" "BATON_STATE: { mode, phase, status, assignee:" "$role BATON_STATE remains structured with mode"
  require_text "$skill" "--persist-max <600" "$role skill documents Claude wait cap"
  require_text "$skill" 'Codex-hosted sessions may use `--persist`' "$role skill documents Codex persistent wait"
  require_text "$skill" "Exit 23" "$role skill documents persistent cap exit"
  require_text "$skill" "Continuous polling is the hard rule" "$role skill makes continuous polling mandatory"
  require_text "$skill" "Phase convention: implementation-chunk" "$role skill documents subagent track phase convention"
done

vadi_skill="$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md"
require_text "$vadi_skill" "Record the user's original ask in the initial baton context" "vadi seeds original ask"
require_text "$vadi_skill" "./superpowers/plans/YYYY-MM-DD-<topic>.html" "vadi writes HTML plan refs"
reject_text "$vadi_skill" "./superpowers/plans/YYYY-MM-DD-<topic>.md" "vadi no longer directs generated plans to markdown"

for command in "$ROOT_DIR/plugins/dvandva/commands/vadi.md" "$ROOT_DIR/plugins/dvandva/commands/prativadi.md"; do
  name="${command#$ROOT_DIR/}"
  require_text "$command" "resolved Dvandva baton" "$name goal refers to resolved baton"
  require_text "$command" "DVANDVA_RUN_ID" "$name goal mentions run id"
  require_text "$command" "turn_cap" "$name goal keeps active turn cap"
  require_text "$command" "do not count shell wait heartbeats as turns" "$name goal separates waits from active turns"
  require_text "$command" "continuous polling is the hard rule" "$name goal makes continuous polling mandatory"
done

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
