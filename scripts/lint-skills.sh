#!/usr/bin/env bash
# Lint a SKILL.md file: validate frontmatter, body length, and inlined dvandva.baton.v1 schema.
# Usage: bash scripts/lint-skills.sh <path/to/SKILL.md>
# Exit codes: 0 = ok; 1 = lint failure; 2 = usage error.
set -u

if [[ $# -ne 1 ]]; then
  echo "Usage: bash $0 <path/to/SKILL.md>" >&2
  exit 2
fi

FILE="$1"

if [[ ! -f "$FILE" ]]; then
  echo "ERROR: file not found: $FILE" >&2
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

# Body length: total lines minus frontmatter block
TOTAL_LINES=$(wc -l < "$FILE")
FRONTMATTER_LINES=$(awk '/^---$/{c++} c<=2{n++} END{print n}' "$FILE")
BODY_LINES=$(( TOTAL_LINES - FRONTMATTER_LINES ))
if [[ $BODY_LINES -gt 500 ]]; then
  echo "FAIL: body is $BODY_LINES lines (max 500) in $FILE" >&2
  exit 1
fi

# Inlined schema check: find a fenced JSON block whose first key is "schema"
# Extract the first JSON block and parse with jq, then check required keys
JSON_BLOCK=$(awk '/^```json$/{flag=1; next} /^```$/{flag=0} flag' "$FILE" | head -100)
if [[ -z "$JSON_BLOCK" ]]; then
  echo "FAIL: no fenced JSON block found in body of $FILE" >&2
  exit 1
fi

# Parse with jq and verify required v1 keys exist
if ! echo "$JSON_BLOCK" | jq -e '.schema == "dvandva.baton.v1"' >/dev/null 2>&1; then
  echo "FAIL: inlined JSON block does not have schema=dvandva.baton.v1 in $FILE" >&2
  exit 1
fi

REQUIRED_KEYS=(schema updated_at mode phase total_phases status assignee review_target plan_ref disagreement_round disagreement_cap turn_cap branch checkpoint summary changed_paths verification findings narrow_fixups claude_counter deferred blockers next_action)
for key in "${REQUIRED_KEYS[@]}"; do
  if ! echo "$JSON_BLOCK" | jq -e "has(\"$key\")" >/dev/null 2>&1; then
    echo "FAIL: inlined JSON block missing required key '$key' in $FILE" >&2
    exit 1
  fi
done

echo "OK: $FILE"
exit 0
