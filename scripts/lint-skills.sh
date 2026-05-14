#!/usr/bin/env bash
# Lint a SKILL.md file: validate frontmatter, body length, and inlined dvandva.baton.v1 schema.
# Usage: scripts/lint-skills.sh <path/to/SKILL.md>
# Exit codes: 0 = ok; 1 = lint failure; 2 = usage error.
# set -e is intentionally omitted; this script uses explicit 'if !' guards
# and emits structured FAIL messages on each failure path.
set -uo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: bash $0 <path/to/SKILL.md>" >&2
  exit 2
fi

FILE="$1"

if [[ ! -f "$FILE" ]]; then
  echo "FAIL: file not found: $FILE" >&2
  exit 1
fi

# Reject if frontmatter is not closed (need at least two '---' lines)
DASH_COUNT=$(grep -c '^---$' "$FILE")
if [[ $DASH_COUNT -lt 2 ]]; then
  echo "FAIL: frontmatter block not closed (need two '---' lines) in $FILE" >&2
  exit 1
fi

# Extract frontmatter (lines between first two '---' lines)
FRONTMATTER=$(awk '/^---$/{c++; next} c==1' "$FILE")
if [[ -z "$FRONTMATTER" ]]; then
  echo "FAIL: no frontmatter block found in $FILE" >&2
  exit 1
fi

# Required: name and description
if ! grep -qE '^name: ' <<< "$FRONTMATTER"; then
  echo "FAIL: missing required frontmatter field 'name' in $FILE" >&2
  exit 1
fi
if ! grep -qE '^description: ' <<< "$FRONTMATTER"; then
  echo "FAIL: missing required frontmatter field 'description' in $FILE" >&2
  exit 1
fi

# description length
DESC=$(grep -E '^description: ' <<< "$FRONTMATTER" | sed 's/^description: //')
DESC_LEN=${#DESC}
if [[ $DESC_LEN -gt 1536 ]]; then
  echo "FAIL: description is $DESC_LEN chars (max 1536) in $FILE" >&2
  exit 1
fi

# Body length: count lines after the second '---'
BODY_LINES=$(awk '/^---$/{c++; next} c>=2{n++} END{print n+0}' "$FILE")
if [[ $BODY_LINES -gt 500 ]]; then
  echo "FAIL: body is $BODY_LINES lines (max 500) in $FILE" >&2
  exit 1
fi

# Inlined schema check: find a fenced JSON block whose first key is "schema"
# Only scan body lines (after the second '---') to ignore any ```json in frontmatter.
# The awk terminates at the closing fence so no truncation limit is needed.
JSON_BLOCK=$(awk '/^---$/{c++; next} c>=2 && /^```json$/{flag=1; next} c>=2 && /^```$/{flag=0} flag' "$FILE")
if [[ -z "$JSON_BLOCK" ]]; then
  echo "FAIL: no fenced JSON block found in body of $FILE" >&2
  exit 1
fi

# Parse with jq and verify required v1 keys exist
if ! echo "$JSON_BLOCK" | jq -e '.schema == "dvandva.baton.v1"' >/dev/null 2>&1; then
  echo "FAIL: inlined JSON block does not have schema=dvandva.baton.v1 in $FILE" >&2
  exit 1
fi

REQUIRED_KEYS=(schema updated_at mode run_mode phase total_phases status assignee current_engine review_target plan_ref master_plan_locked question resume_assignee resume_status disagreement_round disagreement_cap turn_cap branch checkpoint allow_commit allow_push allow_pr vadi_final_approval prativadi_final_approval final_commit pushed_ref summary changed_paths verification findings narrow_fixups vadi_counter deferred blockers next_action)
for key in "${REQUIRED_KEYS[@]}"; do
  if ! echo "$JSON_BLOCK" | jq -e "has(\"$key\")" >/dev/null 2>&1; then
    echo "FAIL: inlined JSON block missing required key '$key' in $FILE" >&2
    exit 1
  fi
done

echo "OK: $FILE"
exit 0
