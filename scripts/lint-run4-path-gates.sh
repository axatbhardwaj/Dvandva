#!/usr/bin/env bash
# Lint the Run4 path-gate and git work-gate documentation/script contracts.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${1:-$ROOT_DIR}"
FAILURES=0

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

pass() {
  printf 'PASS: %s\n' "$*"
}

file_path() {
  printf '%s/%s\n' "$TARGET_ROOT" "$1"
}

require_file() {
  local rel="$1"
  if [[ -f "$(file_path "$rel")" ]]; then
    pass "$rel exists"
  else
    fail "$rel is missing"
    return 1
  fi
}

require_match() {
  local rel="$1" regex="$2" message="$3"
  local path
  path="$(file_path "$rel")"
  if [[ ! -f "$path" ]]; then
    fail "$rel is missing"
    return
  fi
  if grep -Eiq -- "$regex" "$path"; then
    pass "$message"
  else
    fail "$message"
  fi
}

require_slurp_match() {
  local rel="$1" regex="$2" message="$3"
  local path text
  path="$(file_path "$rel")"
  if [[ ! -f "$path" ]]; then
    fail "$rel is missing"
    return
  fi
  text="$(tr '\n' ' ' < "$path")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    pass "$message"
  else
    fail "$message"
  fi
}

required_files=(
  README.md
  product.md
  docs/protocol/local-baton-channel.md
  plugins/dvandva/references/state-transition-table.md
  plugins/dvandva/references/baton-schema-v2.json
  plugins/dvandva/skills/vadi/scripts/dvandva-write.sh
  plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh
  plugins/dvandva/skills/vadi/SKILL.md
  plugins/dvandva/skills/prativadi/SKILL.md
  .githooks/pre-commit
  .githooks/prepare-commit-msg
  scripts/dvandva-commit-gate.sh
  scripts/dvandva-drift-lint.sh
  scripts/install-dvandva-hooks.sh
)

for rel in "${required_files[@]}"; do
  require_file "$rel"
done

require_slurp_match \
  README.md \
  'work_split.*write_paths|write_paths.*work_split' \
  'README.md must document work_split write_paths'

require_slurp_match \
  product.md \
  'safe_rel_path.*work_split|work_split.*safe_rel_path' \
  'product.md must document safe_rel_path work_split path validation'

require_slurp_match \
  docs/protocol/local-baton-channel.md \
  'cross_review.*read-only.*write_paths' \
  'local-baton-channel.md must document cross_review read-only semantics'

require_slurp_match \
  docs/protocol/local-baton-channel.md \
  'write_paths.*supplements.*paths|effective write set.*union' \
  'local-baton-channel.md must document write_paths cannot narrow write-capable paths'

require_slurp_match \
  docs/protocol/local-baton-channel.md \
  'conflict_group.*depends_on|depends_on.*conflict_group' \
  'local-baton-channel.md must document conflict_group/depends_on serialization'

require_slurp_match \
  plugins/dvandva/references/state-transition-table.md \
  'conflict_group.*depends_on|depends_on.*conflict_group' \
  'state-transition-table.md must document conflict_group/depends_on serialization'

require_slurp_match \
  plugins/dvandva/references/state-transition-table.md \
  'terminal historical.*reuse|base_checkpoint.*wave model' \
  'state-transition-table.md must document terminal work_split reuse rationale'

require_slurp_match \
  plugins/dvandva/references/baton-schema-v2.json \
  'write_paths.*conflict_group.*depends_on|depends_on.*conflict_group.*write_paths' \
  'baton-schema-v2.json must expose write_paths/conflict_group/depends_on'

require_slurp_match \
  plugins/dvandva/skills/vadi/scripts/dvandva-write.sh \
  'safe_rel_path.*work_split|work_split.*safe_rel_path' \
  'vadi dvandva-write.sh must validate work_split paths with safe_rel_path'

require_slurp_match \
  plugins/dvandva/skills/vadi/scripts/dvandva-write.sh \
  'paths.*write_paths.*unique|write_paths.*paths.*unique' \
  'vadi dvandva-write.sh must union write-capable paths and write_paths'

require_slurp_match \
  plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh \
  'safe_rel_path.*work_split|work_split.*safe_rel_path' \
  'prativadi dvandva-write.sh must validate work_split paths with safe_rel_path'

require_slurp_match \
  plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh \
  'paths.*write_paths.*unique|write_paths.*paths.*unique' \
  'prativadi dvandva-write.sh must union write-capable paths and write_paths'

require_match \
  .githooks/pre-commit \
  'dvandva-commit-gate\.sh' \
  '.githooks/pre-commit must delegate to dvandva-commit-gate.sh'

require_match \
  .githooks/prepare-commit-msg \
  'Dvandva-Checkpoint' \
  '.githooks/prepare-commit-msg must stamp Dvandva-Checkpoint'

require_match \
  scripts/dvandva-commit-gate.sh \
  'DVANDVA_ROLE' \
  'dvandva-commit-gate.sh must enforce DVANDVA_ROLE'

require_match \
  scripts/dvandva-drift-lint.sh \
  'Dvandva-Checkpoint' \
  'dvandva-drift-lint.sh must inspect Dvandva-Checkpoint trailers'

require_slurp_match \
  scripts/install-dvandva-hooks.sh \
  'core\.hooksPath.*\.githooks|\.githooks.*core\.hooksPath' \
  'install-dvandva-hooks.sh must install repo-local .githooks via core.hooksPath'

require_slurp_match \
  scripts/install-dvandva-hooks.sh \
  'dvandva\.hooksAdoptedAt' \
  'install-dvandva-hooks.sh must record hook-adoption baseline'

require_slurp_match \
  scripts/dvandva-drift-lint.sh \
  'dvandva\.hooksAdoptedAt' \
  'dvandva-drift-lint.sh must honor hook-adoption baseline'

require_slurp_match \
  plugins/dvandva/skills/vadi/SKILL.md \
  'install-dvandva-hooks\.sh.*core\.hooksPath=.*\.githooks|core\.hooksPath=.*\.githooks.*install-dvandva-hooks\.sh' \
  'vadi skill preflight must enforce repo-local Dvandva hooks'

require_slurp_match \
  plugins/dvandva/skills/prativadi/SKILL.md \
  'install-dvandva-hooks\.sh.*core\.hooksPath=.*\.githooks|core\.hooksPath=.*\.githooks.*install-dvandva-hooks\.sh' \
  'prativadi skill preflight must enforce repo-local Dvandva hooks'

for rel in scripts/dvandva-commit-gate.sh scripts/dvandva-drift-lint.sh .githooks/prepare-commit-msg; do
  require_slurp_match \
    "$rel" \
    '\.dvandva/runs.*baton\.json|baton\.json.*\.dvandva/runs' \
    "$rel must scan run-scoped baton paths"
  require_slurp_match \
    "$rel" \
    'done.*human_question.*human_decision|human_question.*human_decision.*done' \
    "$rel must share terminal baton statuses"
  require_slurp_match \
    "$rel" \
    'jq empty' \
    "$rel must fail closed on malformed baton JSON"
done

require_slurp_match \
  product.md \
  'no daemon.*hidden|hidden.*no daemon|no hidden.*daemon' \
  'product.md must preserve no-daemon/no-hidden-orchestrator contract'

if [[ "$FAILURES" -gt 0 ]]; then
  exit 1
fi

exit 0
