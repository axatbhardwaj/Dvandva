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
  require_text "$skill" "run_explainer_reviews" "$role skill surfaces final explainer reviews"
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
require_text "$vadi_skill" 'Full-profile v2 writes `status: "test_creation"`; fast/standard-profile v2 writes `status: "phase_review"`' "vadi handoff branches by development profile"
reject_text "$vadi_skill" 'status: "phase_review" for the legacy v1 helper. In v2, use `status: "test_creation"` first' "vadi no longer routes compact profile handoff through full-only gates"
require_text "$vadi_skill" 'Development/full fixbacks keep the numeric implementation phase, set `status: "test_creation"`' "vadi full fixbacks return through test_creation"
require_text "$vadi_skill" 'Development/fast and development/standard fixbacks keep the numeric implementation phase, set `status: "phase_review"`' "vadi compact fixbacks return to phase_review"
reject_text "$vadi_skill" 'If a fix changes behavior, return through test_creation; do not skip directly to review.' "vadi phase fixing instructions are profile-aware"
require_text "$vadi_skill" 'fast` is allowlisted prose-only work with an optional `research_drafting -> research_review -> implementing` prelude' "vadi fast profile documents optional research prelude"
require_text "$vadi_skill" 'For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.' "vadi review_of_review approval uses full-profile deslop"
reject_text "$vadi_skill" '`status: "implementing"` (advance) **or** `"done"` (terminal)' "vadi review_of_review approval avoids stale v1 direct advance"
reject_text "$vadi_skill" 'Approve to advance, or counter-propose.' "vadi counter handoff avoids stale approve-to-advance wording"

prativadi_skill="$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md"
require_text "$prativadi_skill" 'Full-profile v2: `status: "parallel_implementing"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`' "prativadi full spec approval ownership is valid"
require_text "$prativadi_skill" 'Fast/standard-profile v2: `status: "implementing"`, `assignee: "vadi"`, `active_roles: []`' "prativadi compact spec approval ownership is valid"
reject_text "$prativadi_skill" 'assignee: "team" for v2, with `active_roles: ["vadi", "prativadi"]`; legacy v1 uses `"vadi"`' "prativadi compact spec approval does not use team owner"
reject_text "$prativadi_skill" '`assignee: "team"` for v2, with `active_roles: ["vadi", "prativadi"]`; legacy v1 uses `"vadi"`' "prativadi compact spec approval does not use backticked team owner"
reject_text "$prativadi_skill" 'Spec approved. Advancing to phase 1 parallel implementation. <total_phases> phases planned.' "prativadi compact spec approval summary is profile-aware"
reject_text "$vadi_skill" 'legacy v1 phase implementation' "vadi mode table does not label compact implementing as legacy-only"
require_text "$prativadi_skill" 'Fast/standard profiles do not use `review_of_review` narrow-fix branches' "prativadi compact review avoids unsupported narrow-fix branch"
reject_text "$prativadi_skill" 'for development, both explainer review entries present' "prativadi final done gate is profile-aware"
require_text "$prativadi_skill" 'Development/fast: write `phase: 1`, `status: "implementing"`, `assignee: "vadi"`, and `active_roles: []` so the allowlisted fast path skips spec planning.' "prativadi fast research approval skips spec planning"
reject_text "$prativadi_skill" 'Development or legacy `feature-pr`: write `phase: "spec", status: "spec_drafting"`' "prativadi research approval is profile-aware"
require_text "$prativadi_skill" 'Full-profile development no-change approval routes to `deslop`; fast/standard compact no-change approval routes through `phase_review -> termination_review` on the final phase or `phase_review -> implementing` for additional work.' "prativadi no-change approval is profile-aware"
reject_text "$prativadi_skill" 'route through deslop before advancement when v2 states are available' "prativadi no-change approval avoids full-only deslop guidance"
require_text "$prativadi_skill" 'Re-read the final diff, verification, and the mode/profile-appropriate terminal evidence' "prativadi termination review starts profile-aware"
reject_text "$prativadi_skill" 'the mode-appropriate terminal artifact (`run_explainer_ref`, `research_ref` plus conditional `plan_ref`, or `review_ref`)' "prativadi termination review does not imply compact run explainer"
require_text "$prativadi_skill" 'For full-profile v2, approval routes to `deslop`; do not advance directly to `implementing` or `done`.' "prativadi counter approval uses full-profile deslop"
reject_text "$prativadi_skill" '`status: "implementing"` on advance, or `"done"` on terminal' "prativadi counter approval avoids stale v1 direct advance"
reject_text "$prativadi_skill" 'Approve to advance, or counter.' "prativadi fixup handoff avoids stale approve-to-advance wording"
reject_text "$prativadi_skill" 'Approve to advance, or counter again.' "prativadi counter loop handoff avoids stale approve-to-advance wording"

for command in "$ROOT_DIR/plugins/dvandva/commands/vadi.md" "$ROOT_DIR/plugins/dvandva/commands/prativadi.md"; do
  name="${command#$ROOT_DIR/}"
  require_text "$command" "resolved Dvandva baton" "$name goal refers to resolved baton"
  require_text "$command" "DVANDVA_RUN_ID" "$name goal mentions run id"
  require_text "$command" "turn_cap" "$name goal keeps active turn cap"
  require_text "$command" "do not count shell wait heartbeats as turns" "$name goal separates waits from active turns"
  require_text "$command" "continuous polling is the hard rule" "$name goal makes continuous polling mandatory"
  require_text "$command" "run_explainer_reviews" "$name goal requires final explainer reviews"
done

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
