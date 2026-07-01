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
  "profile": "standard",
  "profile_floor": "standard",
  "profile_decision": {
    "selected_profile": "standard",
    "floor": "standard",
    "reason": "state helper profile surfacing test",
    "decided_by": "test-suite",
    "decided_at": "2026-07-01T00:00:00Z",
    "risk_inputs": [],
    "hard_triggers": [],
    "allowlist_match": false,
    "allowlist_refs": [],
    "evidence_refs": ["test:state-profile"]
  },
  "profile_history": [],
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
    "result": "red",
    "notes": "short note",
    "extra": "object-form extra detail must not be surfaced"
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

write_non_object_json() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  printf '[]\n' > "$file"
}

write_phase_less_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<'JSON'
{
  "schema": "dvandva.baton.v2",
  "run_id": "phase-less-run",
  "mode": "development",
  "run_mode": "walkaway",
  "status": "implementing",
  "assignee": "vadi",
  "active_roles": [],
  "checkpoint": 7,
  "work_split": [
    {
      "id": "phase-less-work",
      "chunk_type": "implementation",
      "owner_role": "vadi",
      "status": "ready",
      "paths": ["plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"]
    }
  ],
  "subagent_tracks": [],
  "verification_matrix": [],
  "findings": [],
  "blockers": [],
  "changed_paths": []
}
JSON
}

write_legacy_string_verification_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<'JSON'
{
  "schema": "dvandva.baton.v2",
  "run_id": "legacy-string-verification",
  "mode": "development",
  "profile": "fast",
  "profile_floor": "fast",
  "run_mode": "walkaway",
  "phase": 1,
  "status": "implementing",
  "assignee": "vadi",
  "active_roles": [],
  "checkpoint": 11,
  "work_split": [],
  "subagent_tracks": [],
  "verification_matrix": [],
  "verification": ["legacy verification string"],
  "findings": [],
  "blockers": [],
  "changed_paths": []
}
JSON
}

write_legacy_string_findings_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<'JSON'
{
  "schema": "dvandva.baton.v2",
  "run_id": "legacy-string-findings",
  "mode": "development",
  "profile": "standard",
  "profile_floor": "standard",
  "run_mode": "walkaway",
  "phase": 1,
  "status": "cross_fixing",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "checkpoint": 12,
  "work_split": [],
  "subagent_tracks": [],
  "verification_matrix": [],
  "verification": [],
  "findings": ["legacy finding string"],
  "blockers": [],
  "changed_paths": []
}
JSON
}

write_large_baton() {
  local file="$1"
  local long
  long="$(printf 'x%.0s' $(seq 1 1500))"
  mkdir -p "$(dirname "$file")"
  jq -n --arg long "$long" '
    {
      schema: "dvandva.baton.v2",
      run_id: "large-run",
      mode: "development",
      run_mode: "walkaway",
      phase: 1,
      status: "implementing",
      assignee: "vadi",
      active_roles: [],
      checkpoint: 9,
      refs: {
        huge: $long,
        branch: ("branch-" + $long),
        plan: "superpowers/plans/large-run.html"
      },
      research_ref: ("./superpowers/research/" + $long + ".html"),
      plan_ref: "./superpowers/plans/large-run.html",
      work_split: [
        range(0; 15) as $i |
        {
          id: ("work-" + ($i | tostring)),
          phase: 1,
          chunk_type: "implementation",
          owner_role: "vadi",
          status: "ready",
          paths: ["a", "b"],
          write_paths: ["a"],
          depends_on: ["root"]
        }
      ],
      subagent_tracks: [],
      verification_matrix: [],
      findings: [
        range(0; 15) as $i |
        {
          id: ("F-" + ($i | tostring)),
          severity: "low",
          status: "open",
          summary: $long
        }
      ],
      blockers: [],
      changed_paths: [],
      verification_latest: {
        command: $long,
        result: "passed",
        notes: $long,
        extra: $long
      },
      next_action: $long
    }
  ' > "$file"
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
    and .profile == "standard"
    and .profile_floor == "standard"
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
    and .verification_latest.result == "red"
    and .verification_latest.notes == "short note"
    and (.verification_latest | has("extra") | not)
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
	    and .profile == "full"
	    and .profile_floor == "full"
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

assert_non_object_fails_cleanly() {
  local name="$1"
  local script="$2"
  local baton="$3"

  local output actual_exit
  output="$(run_compact "$script" "$baton" 2>&1)"
  actual_exit=$?
  if [[ "$actual_exit" -ne 22 ]]; then
    echo "FAIL: $name expected exit 22 for valid non-object JSON, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return
  fi
  if ! printf '%s' "$output" | grep -Fq "baton JSON root must be object"; then
    echo "FAIL: $name expected clean non-object error"
    echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

assert_phase_less_work_is_not_dropped() {
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
    .run_id == "phase-less-run"
    and (.phase == null)
    and (.current_role_work | length) == 1
    and .current_role_work[0].id == "phase-less-work"
  ' >/dev/null 2>&1; then
    echo "FAIL: $name dropped phase-less role work"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

assert_legacy_string_verification_is_bounded() {
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
    .run_id == "legacy-string-verification"
    and .profile == "fast"
    and .profile_floor == "fast"
    and .verification_latest.command == "legacy verification string"
    and .verification_latest.result == "legacy"
  ' >/dev/null 2>&1; then
    echo "FAIL: $name did not compact a legacy string verification entry"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

assert_legacy_string_findings_do_not_crash() {
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
    .run_id == "legacy-string-findings"
    and .profile == "standard"
    and .profile_floor == "standard"
    and (.open_findings | length) == 1
    and .open_findings[0].status == "open"
    and .open_findings[0].summary == "legacy finding string"
  ' >/dev/null 2>&1; then
    echo "FAIL: $name did not compact a legacy string finding entry"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

assert_large_state_is_bounded() {
  local name="$1"
  local script="$2"
  local baton="$3"

  local output actual_exit bytes
  output="$(run_compact "$script" "$baton" 2>&1)"
  actual_exit=$?
  if [[ "$actual_exit" -ne 0 ]]; then
    echo "FAIL: $name expected exit 0, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return
  fi
  bytes="$(printf '%s' "$output" | wc -c)"
  if [[ "$bytes" -gt 12000 ]]; then
    echo "FAIL: $name expected compact output <= 12000 bytes, got $bytes"
    failures=$((failures + 1))
    return
  fi
  if ! printf '%s' "$output" | jq -e '
    (.refs | has("huge") | not)
    and ((.refs.research_ref // "" | length) <= 260)
    and ((.verification_latest.command // "" | length) <= 260)
    and ((.verification_latest.notes // "" | length) <= 260)
    and (.verification_latest | has("extra") | not)
    and ((.next_action // "" | length) <= 520)
    and (.current_role_work | length) == 11
    and .current_role_work[10].more_count == 5
    and (.open_findings | length) == 11
    and .open_findings[10].more_count == 5
  ' >/dev/null 2>&1; then
    echo "FAIL: $name compact JSON was not bounded as expected"
    printf '%s\n' "$output" | jq . 2>/dev/null || echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

FULL_BATON="$TMP_DIR/full-baton.json"
MISSING_REFS_BATON="$TMP_DIR/missing-refs-baton.json"
MALFORMED_BATON="$TMP_DIR/malformed-baton.json"
NON_OBJECT_BATON="$TMP_DIR/non-object-baton.json"
PHASE_LESS_BATON="$TMP_DIR/phase-less-baton.json"
LEGACY_STRING_VERIFICATION_BATON="$TMP_DIR/legacy-string-verification-baton.json"
LEGACY_STRING_FINDINGS_BATON="$TMP_DIR/legacy-string-findings-baton.json"
LARGE_BATON="$TMP_DIR/large-baton.json"
write_full_baton "$FULL_BATON"
write_missing_refs_baton "$MISSING_REFS_BATON"
write_malformed_baton "$MALFORMED_BATON"
write_non_object_json "$NON_OBJECT_BATON"
write_phase_less_baton "$PHASE_LESS_BATON"
write_legacy_string_verification_baton "$LEGACY_STRING_VERIFICATION_BATON"
write_legacy_string_findings_baton "$LEGACY_STRING_FINDINGS_BATON"
write_large_baton "$LARGE_BATON"

assert_compact_state "vadi --compact emits bounded compact baton state JSON" "$SCRIPT" "$FULL_BATON" "vadi" 2
assert_compact_state "prativadi --compact emits bounded compact baton state JSON" "$PRATIVADI_SCRIPT" "$FULL_BATON" "prativadi" 1

assert_missing_optional_refs "missing optional refs serialize as null/empty" "$SCRIPT" "$MISSING_REFS_BATON"

assert_malformed_fails "malformed JSON fails nonzero" "$SCRIPT" "$MALFORMED_BATON"
assert_non_object_fails_cleanly "valid non-object JSON fails cleanly" "$SCRIPT" "$NON_OBJECT_BATON"
assert_phase_less_work_is_not_dropped "phase-less baton keeps phase-less role work" "$SCRIPT" "$PHASE_LESS_BATON"
assert_legacy_string_verification_is_bounded "legacy string verification stays bounded" "$SCRIPT" "$LEGACY_STRING_VERIFICATION_BATON"
assert_legacy_string_findings_do_not_crash "legacy string findings stay bounded" "$SCRIPT" "$LEGACY_STRING_FINDINGS_BATON"
assert_large_state_is_bounded "large compact state stays bounded" "$SCRIPT" "$LARGE_BATON"

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
