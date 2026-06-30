#!/usr/bin/env bash
# Tests for the bundled Dvandva compact state helpers.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

write_full_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "token-efficient-runs",
  "mode": "development",
  "run_mode": "walkaway",
  "phase": 1,
  "status": "parallel_implementing",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "checkpoint": 42,
  "refs": {
    "branch": "token-efficient-runs",
    "base": "main",
    "plan": "superpowers/plans/token-efficient-runs.html"
  },
  "work_split": [
    {
      "id": "implementation-state-helper-tests",
      "phase": 1,
      "chunk_type": "implementation",
      "owner_role": "vadi",
      "status": "ready",
      "depends_on": ["state-helper-contract"],
      "paths": ["scripts/test-dvandva-state.sh"],
      "cross_review_by": "prativadi",
      "acceptance_criteria": [
        "compact JSON validates",
        "full dynamic arrays are omitted"
      ],
      "notes": "This intentionally large field must not be copied into current_role_work."
    },
    {
      "id": "implementation-state-helper-core",
      "phase": 1,
      "chunk_type": "implementation",
      "owner_role": "vadi",
      "status": "pending",
      "depends_on": ["implementation-state-helper-tests"],
      "paths": [
        "plugins/dvandva/skills/vadi/scripts/dvandva-state.sh",
        "plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"
      ],
      "cross_review_by": "prativadi",
      "acceptance_criteria": ["passes the RED contract tests"],
      "notes": "Another verbose field that belongs only in the source baton."
    },
    {
      "id": "review-state-helper-core",
      "phase": 1,
      "chunk_type": "implementation",
      "owner_role": "prativadi",
      "status": "pending",
      "depends_on": ["implementation-state-helper-core"],
      "paths": ["plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"],
      "cross_review_by": "vadi",
      "acceptance_criteria": ["review the compact state helper"],
      "notes": "Prativadi-only verbose detail."
    }
  ],
  "subagent_tracks": [
    {"id": "writer", "owner_role": "vadi", "status": "complete"},
    {"id": "reviewer", "owner_role": "prativadi", "status": "pending"}
  ],
  "verification_matrix": [
    {"id": "state-test-red", "status": "red"},
    {"id": "state-test-green", "status": "pending"}
  ],
  "findings": [
    {"id": "F-1", "status": "open", "severity": "medium", "summary": "open finding"},
    {"id": "F-2", "status": "resolved", "severity": "low", "summary": "closed finding"}
  ],
  "blockers": [
    {"id": "B-1", "status": "open", "summary": "helper missing"}
  ],
  "changed_paths": [
    "scripts/test-dvandva-state.sh",
    "plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"
  ],
  "verification_latest": {
    "command": "scripts/test-dvandva-state.sh",
    "status": "red",
    "updated_at": "2026-07-01T00:00:00Z"
  },
  "next_action": {
    "owner_role": "vadi",
    "prompt": "Implement the compact state helper."
  }
}
JSON
}

write_missing_refs_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "minimal-run",
  "mode": "development",
  "run_mode": "supervised",
  "phase": 1,
  "status": "implementing",
  "assignee": "vadi",
  "active_roles": ["vadi"],
  "checkpoint": 3,
  "work_split": [],
  "subagent_tracks": [],
  "verification_matrix": [],
  "findings": [],
  "blockers": [],
  "changed_paths": []
}
JSON
}

write_malformed_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  printf '{"schema":"dvandva.baton.v2","run_id":"broken",\n' > "$file"
}

run_compact() {
  local script="$1"
  local file="$2"
  "$script" --compact --file "$file"
}

assert_compact_state() {
  local name="$1"
  local script="$2"
  local baton="$3"
  local expected_role="$4"
  local expected_work_count="$5"

  local output actual_exit
  output="$(run_compact "$script" "$baton" 2>&1)"
  actual_exit=$?
  if [[ "$actual_exit" -ne 0 ]]; then
    echo "FAIL: $name expected exit 0, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return
  fi

  if ! printf '%s' "$output" | jq -e . >/dev/null 2>&1; then
    echo "FAIL: $name emitted invalid JSON"
    echo "$output"
    failures=$((failures + 1))
    return
  fi

  if ! printf '%s' "$output" | jq -e --arg role "$expected_role" --argjson work_count "$expected_work_count" '
    (.kind // .label) == "BATON_STATE_COMPACT"
    and .run_id == "token-efficient-runs"
    and .mode == "development"
    and .run_mode == "walkaway"
    and .phase == 1
    and .status == "parallel_implementing"
    and .assignee == "team"
    and .active_roles == ["vadi", "prativadi"]
    and .checkpoint == 42
    and (.refs | type) == "object"
    and .counts.work_split == 3
    and .counts.subagent_tracks == 2
    and .counts.verification_matrix == 2
    and .counts.findings == 2
    and .counts.blockers == 1
    and .counts.changed_paths == 2
    and (.current_role_work | type) == "array"
    and (.current_role_work | length) == $work_count
    and all(.current_role_work[]; .owner_role == $role)
    and all(.current_role_work[]; has("id") and has("status") and has("chunk_type"))
    and all(.current_role_work[]; (has("depends_on") or has("acceptance_criteria") or has("notes") or has("cross_review_by")) | not)
    and (.open_findings | type) == "array"
    and (.open_findings | length) == 1
    and .open_findings[0].id == "F-1"
    and .verification_latest.command == "scripts/test-dvandva-state.sh"
    and .next_action.owner_role == "vadi"
    and (has("work_split") | not)
    and (has("subagent_tracks") | not)
    and (has("verification_matrix") | not)
  ' >/dev/null 2>&1; then
    echo "FAIL: $name compact JSON did not match expected contract"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi

  echo "PASS: $name"
}

assert_missing_optional_refs() {
  local name="$1"
  local script="$2"
  local baton="$3"

  local output actual_exit
  output="$(run_compact "$script" "$baton" 2>&1)"
  actual_exit=$?
  if [[ "$actual_exit" -ne 0 ]]; then
    echo "FAIL: $name expected exit 0, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return
  fi

  if ! printf '%s' "$output" | jq -e '
    (.kind // .label) == "BATON_STATE_COMPACT"
    and .run_id == "minimal-run"
    and (.refs == null or .refs == {})
    and .counts.work_split == 0
    and .counts.subagent_tracks == 0
    and .counts.verification_matrix == 0
    and .counts.findings == 0
    and .counts.blockers == 0
    and .counts.changed_paths == 0
    and .current_role_work == []
    and .open_findings == []
    and (.verification_latest == null or .verification_latest == {})
    and (.next_action == null or .next_action == {})
  ' >/dev/null 2>&1; then
    echo "FAIL: $name missing optional refs were not serialized as null/empty values"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi

  echo "PASS: $name"
}

assert_malformed_fails() {
  local name="$1"
  local script="$2"
  local baton="$3"

  local output actual_exit
  output="$(run_compact "$script" "$baton" 2>&1)"
  actual_exit=$?
  if [[ "$actual_exit" -eq 0 ]]; then
    echo "FAIL: $name expected nonzero exit for malformed JSON"
    echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

FULL_BATON="$TMP_DIR/full-baton.json"
MISSING_REFS_BATON="$TMP_DIR/missing-refs-baton.json"
MALFORMED_BATON="$TMP_DIR/malformed-baton.json"
write_full_baton "$FULL_BATON"
write_missing_refs_baton "$MISSING_REFS_BATON"
write_malformed_baton "$MALFORMED_BATON"

assert_compact_state "vadi --compact emits bounded compact baton state JSON" "$SCRIPT" "$FULL_BATON" "vadi" 2
assert_compact_state "prativadi --compact emits bounded compact baton state JSON" "$PRATIVADI_SCRIPT" "$FULL_BATON" "prativadi" 1

assert_missing_optional_refs "missing optional refs serialize as null/empty" "$SCRIPT" "$MISSING_REFS_BATON"

assert_malformed_fails "malformed JSON fails nonzero" "$SCRIPT" "$MALFORMED_BATON"

if cmp -s "$SCRIPT" "$PRATIVADI_SCRIPT"; then
  echo "PASS: plugin state helpers are byte-identical"
else
  echo "FAIL: plugin state helpers drifted"
  failures=$((failures + 1))
fi

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
exit 0
