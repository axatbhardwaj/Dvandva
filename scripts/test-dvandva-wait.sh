#!/usr/bin/env bash
# Tests for the bundled Dvandva wait helpers.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-wait.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-wait.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

write_baton() {
  local file="$1"
  local assignee="$2"
  local status="$3"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v1",
  "assignee": "$assignee",
  "status": "$status",
  "phase": 1,
  "checkpoint": 7
}
JSON
}

write_question_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v1",
  "assignee": "human",
  "status": "human_question",
  "phase": "spec",
  "checkpoint": 8,
  "question": "Which scope should Dvandva choose?",
  "resume_assignee": "prativadi",
  "resume_status": "spec_review"
}
JSON
}

write_observed_baton() {
  local file="$1"
  local assignee="$2"
  local status="$3"
  local updated_at="$4"
  local current_engine="$5"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v1",
  "assignee": "$assignee",
  "status": "$status",
  "phase": 2,
  "checkpoint": 8,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "updated_at": "$updated_at",
  "current_engine": "$current_engine"
}
JSON
}

write_named_observed_baton() {
  local file="$1"
  local run_id="$2"
  local assignee="$3"
  local status="$4"
  local updated_at="$5"
  local current_engine="$6"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "$run_id",
  "assignee": "$assignee",
  "status": "$status",
  "phase": 2,
  "checkpoint": 8,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "updated_at": "$updated_at",
  "current_engine": "$current_engine"
}
JSON
}

write_named_question_baton() {
  local file="$1"
  local run_id="$2"
  local updated_at="$3"
  local current_engine="$4"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "$run_id",
  "assignee": "human",
  "status": "human_question",
  "phase": "spec",
  "checkpoint": 9,
  "question": "Which scope should Dvandva choose?",
  "resume_assignee": "prativadi",
  "resume_status": "spec_review",
  "updated_at": "$updated_at",
  "current_engine": "$current_engine"
}
JSON
}

write_active_roles_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "parallel_implementing",
  "phase": 1,
  "checkpoint": 9,
  "question": null,
  "resume_assignee": null,
  "resume_status": null
}
JSON
}

write_termination_review_baton() {
  local file="$1"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "termination_review",
  "phase": 1,
  "checkpoint": 10,
  "question": null,
  "resume_assignee": null,
  "resume_status": null
}
JSON
}

run_case() {
  local name="$1"
  local expected_exit="$2"
  shift 2

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name expected exit $expected_exit, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

BATON_READY="$TMP_DIR/ready.json"
write_baton "$BATON_READY" "vadi" "implementing"
run_case "returns 0 when role is assigned" 0 \
  "$SCRIPT" --role vadi --file "$BATON_READY" --interval 0 --max-wait 0

BATON_ACTIVE_ROLES="$TMP_DIR/active-roles.json"
write_active_roles_baton "$BATON_ACTIVE_ROLES"
run_case "returns 0 for vadi active_roles concurrent baton" 0 \
  "$SCRIPT" --role vadi --file "$BATON_ACTIVE_ROLES" --interval 0 --max-wait 0
run_case "returns 0 for prativadi active_roles concurrent baton" 0 \
  "$PRATIVADI_SCRIPT" --role prativadi --file "$BATON_ACTIVE_ROLES" --interval 0 --max-wait 0

BATON_TERMINATION_REVIEW="$TMP_DIR/termination-review.json"
write_termination_review_baton "$BATON_TERMINATION_REVIEW"
run_case "returns 0 for vadi termination_review active_roles" 0 \
  "$SCRIPT" --role vadi --file "$BATON_TERMINATION_REVIEW" --interval 0 --max-wait 0
run_case "returns 0 for prativadi termination_review active_roles" 0 \
  "$PRATIVADI_SCRIPT" --role prativadi --file "$BATON_TERMINATION_REVIEW" --interval 0 --max-wait 0

BATON_TERMINATION_REVIEW_WAIT="$TMP_DIR/termination-review-wait.json"
write_baton "$BATON_TERMINATION_REVIEW_WAIT" "team" "termination_review"
run_case "termination_review is not terminal done" 20 \
  "$SCRIPT" --role vadi --file "$BATON_TERMINATION_REVIEW_WAIT" --interval 0 --max-wait 0 --finite

BATON_DONE="$TMP_DIR/done.json"
write_baton "$BATON_DONE" "human" "done"
run_case "returns 10 when run is done" 10 \
  "$SCRIPT" --role vadi --file "$BATON_DONE" --interval 0 --max-wait 0

BATON_HUMAN="$TMP_DIR/human.json"
write_baton "$BATON_HUMAN" "human" "human_decision"
run_case "returns 11 on human decision" 11 \
  "$SCRIPT" --role vadi --file "$BATON_HUMAN" --interval 0 --max-wait 0

BATON_QUESTION="$TMP_DIR/question.json"
write_question_baton "$BATON_QUESTION"
question_output="$("$SCRIPT" --role vadi --file "$BATON_QUESTION" --interval 0 --max-wait 0 2>&1)"
question_exit=$?
if [[ "$question_exit" -ne 12 ]]; then
  echo "FAIL: returns 12 on human question expected exit 12, got $question_exit"
  echo "$question_output"
  failures=$((failures + 1))
elif [[ "$question_output" != *"resume_assignee=prativadi"* || "$question_output" != *"resume_status=spec_review"* || "$question_output" != *"Which scope should Dvandva choose?"* ]]; then
  echo "FAIL: human question output missing resume fields"
  echo "$question_output"
  failures=$((failures + 1))
else
  echo "PASS: returns 12 on human question with resume fields"
fi

BATON_WAIT="$TMP_DIR/wait.json"
write_baton "$BATON_WAIT" "prativadi" "phase_review"
run_case "returns 20 on timeout while assigned away" 20 \
  "$SCRIPT" --role vadi --file "$BATON_WAIT" --interval 0 --max-wait 0 --finite

RESOLVE_BOX="$TMP_DIR/resolve-box"
write_named_observed_baton "$RESOLVE_BOX/.dvandva/runs/alpha/baton.json" "alpha" "vadi" "implementing" "2026-06-29T10:00:00Z" "codex"
write_baton "$RESOLVE_BOX/.dvandva/baton.json" "human" "done"
run_case "no-selector wait delegates to named-run resolver before legacy baton" 0 \
  env -i PATH="$PATH" HOME="${HOME:-$TMP_DIR}" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$RESOLVE_BOX" "$SCRIPT"

BATON_CONTINUOUS="$TMP_DIR/continuous.json"
write_baton "$BATON_CONTINUOUS" "prativadi" "phase_review"
( sleep 2 && write_baton "$BATON_CONTINUOUS" "vadi" "implementing" ) &
continuous_pid=$!
run_case "default walkaway wait survives heartbeat until role returns" 0 \
  "$SCRIPT" --role vadi --file "$BATON_CONTINUOUS" --interval 1 --max-wait 1
wait "$continuous_pid" 2>/dev/null || true

run_case "rejects zero interval with positive max wait" 2 \
  "$SCRIPT" --role vadi --file "$BATON_WAIT" --interval 0 --max-wait 1

BATON_HEARTBEAT="$TMP_DIR/heartbeat-content.json"
write_observed_baton "$BATON_HEARTBEAT" "prativadi" "phase_review" "2026-06-27T14:09:08Z" "codex"
heartbeat_output="$(timeout 3 "$SCRIPT" --role vadi --file "$BATON_HEARTBEAT" --persist --interval 1 --max-wait 1 2>&1)"
heartbeat_exit=$?
if [[ "$heartbeat_exit" -ne 124 ]]; then
  echo "FAIL: --persist heartbeat content expected timeout exit 124, got $heartbeat_exit"
  echo "$heartbeat_output"
  failures=$((failures + 1))
elif [[ "$heartbeat_output" != *"last_seen_engine=codex"* || "$heartbeat_output" != *"updated_at=2026-06-27T14:09:08Z"* ]]; then
  echo "FAIL: --persist heartbeat content missing last-seen metadata"
  echo "$heartbeat_output"
  failures=$((failures + 1))
else
  echo "PASS: --persist heartbeat includes last-seen metadata"
fi

HEARTBEAT_RESOLVE_BOX="$TMP_DIR/heartbeat-resolve-box"
write_named_observed_baton "$HEARTBEAT_RESOLVE_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T14:09:08Z" "codex"
heartbeat_resolve_output="$(env -i PATH="$PATH" HOME="${HOME:-$TMP_DIR}" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$HEARTBEAT_RESOLVE_BOX" "$SCRIPT" 2>&1)"
heartbeat_resolve_exit=$?
if [[ "$heartbeat_resolve_exit" -ne 124 ]]; then
  echo "FAIL: resolver heartbeat selector metadata expected timeout exit 124, got $heartbeat_resolve_exit"
  echo "$heartbeat_resolve_output"
  failures=$((failures + 1))
elif [[ "$heartbeat_resolve_output" != *"run_id=alpha"* || "$heartbeat_resolve_output" != *"file=.dvandva/runs/alpha/baton.json"* || "$heartbeat_resolve_output" != *"selected_by=resolve"* || "$heartbeat_resolve_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: resolver heartbeat missing selector metadata"
  echo "$heartbeat_resolve_output"
  failures=$((failures + 1))
else
  echo "PASS: resolver heartbeat includes selector metadata"
fi

SPLIT_BRAIN_BOX="$TMP_DIR/split-brain-box"
write_named_observed_baton "$SPLIT_BRAIN_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T15:00:00Z" "codex"
write_named_observed_baton "$SPLIT_BRAIN_BOX/.dvandva/runs/beta/baton.json" "beta" "vadi" "implementing" "2026-06-29T15:01:00Z" "claude"
split_brain_output="$(env DVANDVA_RUN_ID="alpha" bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$SPLIT_BRAIN_BOX" "$SCRIPT" 2>&1)"
split_brain_exit=$?
if [[ "$split_brain_exit" -ne 29 ]]; then
  echo "FAIL: split-brain guard expected exit 29, got $split_brain_exit"
  echo "$split_brain_output"
  failures=$((failures + 1))
elif [[ "$split_brain_output" != *"split_brain"* || "$split_brain_output" != *"selected_run_id=alpha"* || "$split_brain_output" != *"sibling_run_id=beta"* ]]; then
  echo "FAIL: split-brain guard output missing run identifiers"
  echo "$split_brain_output"
  failures=$((failures + 1))
else
  echo "PASS: split-brain guard exits 29 with selected and sibling run ids"
fi

split_brain_suppressed_output="$(env DVANDVA_RUN_ID="alpha" DVANDVA_CONCURRENT=1 timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$SPLIT_BRAIN_BOX" "$SCRIPT" 2>&1)"
split_brain_suppressed_exit=$?
if [[ "$split_brain_suppressed_exit" -ne 124 ]]; then
  echo "FAIL: DVANDVA_CONCURRENT=1 suppression expected timeout exit 124, got $split_brain_suppressed_exit"
  echo "$split_brain_suppressed_output"
  failures=$((failures + 1))
elif [[ "$split_brain_suppressed_output" != *"run_id=alpha"* || "$split_brain_suppressed_output" != *"selected_by=run_id"* || "$split_brain_suppressed_output" != *"sibling_active_runs=1"* ]]; then
  echo "FAIL: DVANDVA_CONCURRENT=1 suppression heartbeat missing selector metadata"
  echo "$split_brain_suppressed_output"
  failures=$((failures + 1))
else
  echo "PASS: DVANDVA_CONCURRENT=1 suppresses split-brain exit"
fi

# Bug A: an active legacy .dvandva/baton.json assigned to my role is a split-brain
# sibling too. Selected named run alpha waits on the peer while the legacy baton
# is mine -> the scan must include legacy and fire exit 29 (not loop to timeout).
LEGACY_SIBLING_BOX="$TMP_DIR/legacy-sibling-box"
write_named_observed_baton "$LEGACY_SIBLING_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T16:00:00Z" "codex"
write_baton "$LEGACY_SIBLING_BOX/.dvandva/baton.json" "vadi" "implementing"
legacy_sibling_output="$(env DVANDVA_RUN_ID="alpha" timeout 5 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$LEGACY_SIBLING_BOX" "$SCRIPT" 2>&1)"
legacy_sibling_exit=$?
if [[ "$legacy_sibling_exit" -ne 29 ]]; then
  echo "FAIL: legacy sibling split-brain expected exit 29, got $legacy_sibling_exit"
  echo "$legacy_sibling_output"
  failures=$((failures + 1))
elif [[ "$legacy_sibling_output" != *"split_brain"* || "$legacy_sibling_output" != *"selected_run_id=alpha"* || "$legacy_sibling_output" != *"sibling_run_id=legacy"* ]]; then
  echo "FAIL: legacy sibling split-brain output missing run identifiers"
  echo "$legacy_sibling_output"
  failures=$((failures + 1))
else
  echo "PASS: active legacy baton counts as a split-brain sibling (exit 29)"
fi

# Bug B (cr-c6-selfskip): the self-skip must be path-based, not run-id-based.
# Selected path alpha carries a stale .run_id=beta; the genuine sibling lives at
# directory beta and is assigned to my role. A run-id self-skip would skip beta
# (the bug) and loop; path identity (-ef) skips only the selected file itself, so
# the genuine sibling beta is detected -> exit 29 with a path-truthful run id.
SELFSKIP_BOX="$TMP_DIR/selfskip-box"
write_named_observed_baton "$SELFSKIP_BOX/.dvandva/runs/alpha/baton.json" "beta" "prativadi" "phase_review" "2026-06-29T17:00:00Z" "codex"
write_named_observed_baton "$SELFSKIP_BOX/.dvandva/runs/beta/baton.json" "beta" "vadi" "implementing" "2026-06-29T17:01:00Z" "claude"
selfskip_output="$(env DVANDVA_RUN_ID="alpha" timeout 5 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$SELFSKIP_BOX" "$SCRIPT" 2>&1)"
selfskip_exit=$?
if [[ "$selfskip_exit" -ne 29 ]]; then
  echo "FAIL: path-vs-run_id self-skip expected exit 29, got $selfskip_exit"
  echo "$selfskip_output"
  failures=$((failures + 1))
elif [[ "$selfskip_output" != *"split_brain"* || "$selfskip_output" != *"selected_run_id=alpha"* || "$selfskip_output" != *"sibling_run_id=beta"* ]]; then
  echo "FAIL: path-vs-run_id self-skip output wrong run identifiers (must be path-truthful)"
  echo "$selfskip_output"
  failures=$((failures + 1))
else
  echo "PASS: self-skip is path-based; stale .run_id field does not hide sibling beta (exit 29)"
fi

# MED fix: for WAIT split-brain, human_decision and human_question are terminal /
# intervention states, not active runs competing for my role. A sibling parked in
# human_decision/human_question that still carries a STALE assignee or active_roles
# naming my role must NOT be counted as active and must NOT fire exit 29 (resolver
# DISCOVERY taxonomy treating human_* as resumable is separate and unchanged). The
# selected named run alpha waits on the peer while sibling beta is parked terminal.

# Case 1: older sibling in human_decision with a stale assignee=my role -> heartbeat, not 29.
TERMINAL_DECISION_BOX="$TMP_DIR/terminal-decision-box"
write_named_observed_baton "$TERMINAL_DECISION_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T18:01:00Z" "codex"
write_named_observed_baton "$TERMINAL_DECISION_BOX/.dvandva/runs/beta/baton.json" "beta" "vadi" "human_decision" "2026-06-29T18:00:00Z" "claude"
terminal_decision_output="$(env DVANDVA_RUN_ID="alpha" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$TERMINAL_DECISION_BOX" "$SCRIPT" 2>&1)"
terminal_decision_exit=$?
if [[ "$terminal_decision_exit" -ne 124 ]]; then
  echo "FAIL: human_decision sibling must not fire split-brain; expected timeout exit 124, got $terminal_decision_exit"
  echo "$terminal_decision_output"
  failures=$((failures + 1))
elif [[ "$terminal_decision_output" == *"split_brain"* || "$terminal_decision_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: human_decision sibling wrongly counted active or fired split_brain"
  echo "$terminal_decision_output"
  failures=$((failures + 1))
else
  echo "PASS: older human_decision sibling with stale my-role assignee is ignored (no split-brain, not counted)"
fi

# Case 2: older sibling in human_question with a stale assignee=my role -> heartbeat, not 29.
TERMINAL_QUESTION_BOX="$TMP_DIR/terminal-question-box"
write_named_observed_baton "$TERMINAL_QUESTION_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T18:11:00Z" "codex"
write_named_observed_baton "$TERMINAL_QUESTION_BOX/.dvandva/runs/beta/baton.json" "beta" "vadi" "human_question" "2026-06-29T18:10:00Z" "claude"
terminal_question_output="$(env DVANDVA_RUN_ID="alpha" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$TERMINAL_QUESTION_BOX" "$SCRIPT" 2>&1)"
terminal_question_exit=$?
if [[ "$terminal_question_exit" -ne 124 ]]; then
  echo "FAIL: human_question sibling must not fire split-brain; expected timeout exit 124, got $terminal_question_exit"
  echo "$terminal_question_output"
  failures=$((failures + 1))
elif [[ "$terminal_question_output" == *"split_brain"* || "$terminal_question_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: human_question sibling wrongly counted active or fired split_brain"
  echo "$terminal_question_output"
  failures=$((failures + 1))
else
  echo "PASS: older human_question sibling with stale my-role assignee is ignored (no split-brain, not counted)"
fi

# Stop-together resilience: a newer sibling human_decision is a paired pause for
# a selected run waiting on the peer. This must not wait until the 540s
# max-wait heartbeat; the timeout below catches heartbeat-only implementations.
NEWER_DECISION_BOX="$TMP_DIR/newer-decision-box"
write_named_observed_baton "$NEWER_DECISION_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T20:00:00Z" "codex"
write_named_observed_baton "$NEWER_DECISION_BOX/.dvandva/runs/beta/baton.json" "beta" "human" "human_decision" "2026-06-29T20:01:00Z" "claude"
newer_decision_output="$(env DVANDVA_RUN_ID="alpha" timeout 5 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 540' _ "$NEWER_DECISION_BOX" "$SCRIPT" 2>&1)"
newer_decision_exit=$?
if [[ "$newer_decision_exit" -ne 11 ]]; then
  echo "FAIL: newer sibling human_decision should stop paired vadi wait with exit 11, got $newer_decision_exit"
  echo "$newer_decision_output"
  failures=$((failures + 1))
elif [[ "$newer_decision_output" != *"sibling_run_id=beta"* || "$newer_decision_output" != *"selected_run_id=alpha"* ]]; then
  echo "FAIL: newer sibling human_decision output missing selected/sibling metadata"
  echo "$newer_decision_output"
  failures=$((failures + 1))
else
  echo "PASS: newer sibling human_decision stops paired vadi wait"
fi

# Same paired-stop contract for the prativadi helper, including actionable
# human_question metadata from the sibling baton.
NEWER_QUESTION_BOX="$TMP_DIR/newer-question-box"
write_named_observed_baton "$NEWER_QUESTION_BOX/.dvandva/runs/alpha/baton.json" "alpha" "vadi" "phase_review" "2026-06-29T20:10:00Z" "codex"
write_named_question_baton "$NEWER_QUESTION_BOX/.dvandva/runs/beta/baton.json" "beta" "2026-06-29T20:11:00Z" "claude"
newer_question_output="$(env DVANDVA_RUN_ID="alpha" timeout 5 bash -c 'cd "$1" && "$2" --role prativadi --persist --interval 1 --max-wait 540' _ "$NEWER_QUESTION_BOX" "$PRATIVADI_SCRIPT" 2>&1)"
newer_question_exit=$?
if [[ "$newer_question_exit" -ne 12 ]]; then
  echo "FAIL: newer sibling human_question should stop paired prativadi wait with exit 12, got $newer_question_exit"
  echo "$newer_question_output"
  failures=$((failures + 1))
elif [[ "$newer_question_output" != *"sibling_run_id=beta"* || "$newer_question_output" != *"resume_assignee=prativadi"* || "$newer_question_output" != *"resume_status=spec_review"* || "$newer_question_output" != *"Which scope should Dvandva choose?"* ]]; then
  echo "FAIL: newer sibling human_question output missing sibling question/resume metadata"
  echo "$newer_question_output"
  failures=$((failures + 1))
else
  echo "PASS: newer sibling human_question stops paired prativadi wait with metadata"
fi

newer_decision_suppressed_output="$(env DVANDVA_RUN_ID="alpha" DVANDVA_CONCURRENT=1 timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 540' _ "$NEWER_DECISION_BOX" "$SCRIPT" 2>&1)"
newer_decision_suppressed_exit=$?
if [[ "$newer_decision_suppressed_exit" -ne 124 ]]; then
  echo "FAIL: DVANDVA_CONCURRENT=1 should suppress newer sibling human_decision stop, got $newer_decision_suppressed_exit"
  echo "$newer_decision_suppressed_output"
  failures=$((failures + 1))
else
  echo "PASS: DVANDVA_CONCURRENT=1 suppresses newer sibling human_decision stop"
fi

newer_question_suppressed_output="$(env DVANDVA_RUN_ID="alpha" DVANDVA_CONCURRENT=1 timeout 3 bash -c 'cd "$1" && "$2" --role prativadi --persist --interval 1 --max-wait 540' _ "$NEWER_QUESTION_BOX" "$PRATIVADI_SCRIPT" 2>&1)"
newer_question_suppressed_exit=$?
if [[ "$newer_question_suppressed_exit" -ne 124 ]]; then
  echo "FAIL: DVANDVA_CONCURRENT=1 should suppress newer sibling human_question stop, got $newer_question_suppressed_exit"
  echo "$newer_question_suppressed_output"
  failures=$((failures + 1))
else
  echo "PASS: DVANDVA_CONCURRENT=1 suppresses newer sibling human_question stop"
fi

# Case 2b: a terminal sibling whose active_roles (not assignee) lists my role must
# also be skipped -- the contains_role branch of the fire condition is gated too.
TERMINAL_ACTIVE_ROLES_BOX="$TMP_DIR/terminal-active-roles-box"
write_named_observed_baton "$TERMINAL_ACTIVE_ROLES_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T18:20:00Z" "codex"
mkdir -p "$TERMINAL_ACTIVE_ROLES_BOX/.dvandva/runs/beta"
cat > "$TERMINAL_ACTIVE_ROLES_BOX/.dvandva/runs/beta/baton.json" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "beta",
  "assignee": "human",
  "active_roles": ["vadi", "prativadi"],
  "status": "human_decision",
  "phase": 2,
  "checkpoint": 8
}
JSON
terminal_active_roles_output="$(env DVANDVA_RUN_ID="alpha" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$TERMINAL_ACTIVE_ROLES_BOX" "$SCRIPT" 2>&1)"
terminal_active_roles_exit=$?
if [[ "$terminal_active_roles_exit" -ne 124 ]]; then
  echo "FAIL: terminal sibling with my role in active_roles must not fire split-brain; expected timeout exit 124, got $terminal_active_roles_exit"
  echo "$terminal_active_roles_output"
  failures=$((failures + 1))
elif [[ "$terminal_active_roles_output" == *"split_brain"* || "$terminal_active_roles_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: terminal sibling with my role in active_roles wrongly counted active or fired split_brain"
  echo "$terminal_active_roles_output"
  failures=$((failures + 1))
else
  echo "PASS: terminal sibling listing my role in active_roles is skipped (no split-brain, not counted)"
fi

# Case 3: legacy .dvandva/baton.json parked in human_decision with a stale my-role
# assignee is terminal too -> the PFX3 legacy path must skip it, not fire exit 29.
LEGACY_TERMINAL_BOX="$TMP_DIR/legacy-terminal-box"
write_named_observed_baton "$LEGACY_TERMINAL_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T19:00:00Z" "codex"
write_baton "$LEGACY_TERMINAL_BOX/.dvandva/baton.json" "vadi" "human_decision"
legacy_terminal_output="$(env DVANDVA_RUN_ID="alpha" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$LEGACY_TERMINAL_BOX" "$SCRIPT" 2>&1)"
legacy_terminal_exit=$?
if [[ "$legacy_terminal_exit" -ne 124 ]]; then
  echo "FAIL: legacy human_decision sibling must not fire split-brain; expected timeout exit 124, got $legacy_terminal_exit"
  echo "$legacy_terminal_output"
  failures=$((failures + 1))
elif [[ "$legacy_terminal_output" == *"split_brain"* || "$legacy_terminal_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: legacy human_decision sibling wrongly counted active or fired split_brain"
  echo "$legacy_terminal_output"
  failures=$((failures + 1))
else
  echo "PASS: legacy human_decision sibling with stale my-role assignee is terminal (no split-brain, not counted)"
fi

# Case 3b: legacy .dvandva/baton.json parked in human_question with a stale
# my-role assignee is also terminal -> the PFX3 legacy path must skip it, not
# fire exit 29. Symmetry coverage for Case 3 (human_decision). human_question
# entered the terminal set in the same PFX3b fix; this test closes the gap that
# existed only for the legacy-baton path.
LEGACY_TERMINAL_QUESTION_BOX="$TMP_DIR/legacy-terminal-question-box"
write_named_observed_baton "$LEGACY_TERMINAL_QUESTION_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T19:10:00Z" "codex"
write_baton "$LEGACY_TERMINAL_QUESTION_BOX/.dvandva/baton.json" "vadi" "human_question"
legacy_terminal_question_output="$(env DVANDVA_RUN_ID="alpha" timeout 3 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$LEGACY_TERMINAL_QUESTION_BOX" "$SCRIPT" 2>&1)"
legacy_terminal_question_exit=$?
if [[ "$legacy_terminal_question_exit" -ne 124 ]]; then
  echo "FAIL: legacy human_question sibling must not fire split-brain; expected timeout exit 124, got $legacy_terminal_question_exit"
  echo "$legacy_terminal_question_output"
  failures=$((failures + 1))
elif [[ "$legacy_terminal_question_output" == *"split_brain"* || "$legacy_terminal_question_output" != *"sibling_active_runs=0"* ]]; then
  echo "FAIL: legacy human_question sibling wrongly counted active or fired split_brain"
  echo "$legacy_terminal_question_output"
  failures=$((failures + 1))
else
  echo "PASS: legacy human_question sibling with stale my-role assignee is terminal (no split-brain, not counted)"
fi

# Case 4 (regression boundary): a genuinely active, non-terminal sibling assigned to
# my role must STILL fire exit 29 -- the terminal set is exactly done/human_decision/
# human_question, so a non-terminal status like phase_review (not just implementing)
# remains a live competing run. Guards against the fix over-skipping.
REGRESSION_NONTERMINAL_BOX="$TMP_DIR/regression-nonterminal-box"
write_named_observed_baton "$REGRESSION_NONTERMINAL_BOX/.dvandva/runs/alpha/baton.json" "alpha" "prativadi" "phase_review" "2026-06-29T19:30:00Z" "codex"
write_named_observed_baton "$REGRESSION_NONTERMINAL_BOX/.dvandva/runs/beta/baton.json" "beta" "vadi" "phase_review" "2026-06-29T19:31:00Z" "claude"
regression_nonterminal_output="$(env DVANDVA_RUN_ID="alpha" timeout 5 bash -c 'cd "$1" && "$2" --role vadi --persist --interval 1 --max-wait 1' _ "$REGRESSION_NONTERMINAL_BOX" "$SCRIPT" 2>&1)"
regression_nonterminal_exit=$?
if [[ "$regression_nonterminal_exit" -ne 29 ]]; then
  echo "FAIL: non-terminal phase_review sibling assigned to my role must still fire exit 29, got $regression_nonterminal_exit"
  echo "$regression_nonterminal_output"
  failures=$((failures + 1))
elif [[ "$regression_nonterminal_output" != *"split_brain"* || "$regression_nonterminal_output" != *"selected_run_id=alpha"* || "$regression_nonterminal_output" != *"sibling_run_id=beta"* ]]; then
  echo "FAIL: non-terminal sibling split-brain output missing run identifiers"
  echo "$regression_nonterminal_output"
  failures=$((failures + 1))
else
  echo "PASS: non-terminal sibling assigned to my role still fires split-brain (exit 29)"
fi

persist_max_output="$("$SCRIPT" --role vadi --file "$BATON_WAIT" --persist --persist-max 1 --interval 1 --max-wait 1 2>&1)"
persist_max_exit=$?
if [[ "$persist_max_exit" -ne 23 ]]; then
  echo "FAIL: --persist-max caps total wall-clock wait expected exit 23, got $persist_max_exit"
  echo "$persist_max_output"
  failures=$((failures + 1))
elif [[ "$persist_max_output" != *"DVANDVA_WAIT persist_max"* || "$persist_max_output" != *"persist_max=1s"* ]]; then
  echo "FAIL: --persist-max output missing persist_max markers"
  echo "$persist_max_output"
  failures=$((failures + 1))
else
  echo "PASS: --persist-max caps total wall-clock wait"
fi

# Run-scoped default path resolution: DVANDVA_BATON_FILE wins.
BATON_ENV_FILE_DIR="$TMP_DIR/env-file"
BATON_ENV_FILE="$BATON_ENV_FILE_DIR/custom-baton.json"
mkdir -p "$TMP_DIR/no-default-baton-here"
write_baton "$BATON_ENV_FILE" "vadi" "implementing"
run_case "DVANDVA_BATON_FILE sets default baton path" 0 \
  env DVANDVA_BATON_FILE="$BATON_ENV_FILE" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$TMP_DIR/no-default-baton-here" "$SCRIPT"

# Run-scoped default path resolution: DVANDVA_RUN_ID maps to .dvandva/runs/<id>/baton.json.
RUN_BOX="$TMP_DIR/run-box"
write_baton "$RUN_BOX/.dvandva/runs/alpha/baton.json" "vadi" "implementing"
run_case "DVANDVA_RUN_ID sets run-scoped default baton path" 0 \
  env DVANDVA_RUN_ID="alpha" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$RUN_BOX" "$SCRIPT"

run_case "DVANDVA_RUN_ID rejects parent traversal" 2 \
  env DVANDVA_RUN_ID="../escape" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$RUN_BOX" "$SCRIPT"

run_case "DVANDVA_RUN_ID rejects nested path" 2 \
  env DVANDVA_RUN_ID="alpha/beta" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$RUN_BOX" "$SCRIPT"

RUN_ISOLATION_BOX="$TMP_DIR/run-isolation-box"
write_baton "$RUN_ISOLATION_BOX/.dvandva/runs/alpha/baton.json" "vadi" "implementing"
write_baton "$RUN_ISOLATION_BOX/.dvandva/runs/beta/baton.json" "prativadi" "phase_review"
run_case "DVANDVA_RUN_ID alpha does not read beta for prativadi" 20 \
  env DVANDVA_RUN_ID="alpha" bash -c 'cd "$1" && "$2" --role prativadi --interval 0 --max-wait 0 --finite' _ "$RUN_ISOLATION_BOX" "$PRATIVADI_SCRIPT"
run_case "DVANDVA_RUN_ID beta resolves independent prativadi baton" 0 \
  env DVANDVA_RUN_ID="beta" bash -c 'cd "$1" && "$2" --role prativadi --interval 0 --max-wait 0' _ "$RUN_ISOLATION_BOX" "$PRATIVADI_SCRIPT"

# Run-scoped default path resolution: DVANDVA_RUN_DIR maps directly to <dir>/baton.json.
RUN_DIR_BOX="$TMP_DIR/run-dir-box/custom-run"
write_baton "$RUN_DIR_BOX/baton.json" "vadi" "implementing"
run_case "DVANDVA_RUN_DIR sets run directory default baton path" 0 \
  env DVANDVA_RUN_DIR="$RUN_DIR_BOX" bash -c 'cd "$1" && "$2" --role vadi --interval 0 --max-wait 0' _ "$TMP_DIR/no-default-baton-here" "$SCRIPT"

# --persist: helper does not return 20 on a missing run-scoped baton heartbeat;
# it keeps waiting inside the shell process until the baton appears.
PERSIST_BOX="$TMP_DIR/persist-box"
mkdir -p "$PERSIST_BOX/.dvandva/runs/persist"
( sleep 1 && write_baton "$PERSIST_BOX/.dvandva/runs/persist/baton.json" "prativadi" "phase_review" ) &
persist_pid=$!
run_case "--persist waits across missing-baton heartbeat until ready" 0 \
  env DVANDVA_RUN_ID="persist" bash -c 'cd "$1" && "$2" --role prativadi --allow-missing --persist --interval 1 --max-wait 1' _ "$PERSIST_BOX" "$PRATIVADI_SCRIPT"
wait "$persist_pid" 2>/dev/null || true

# --allow-missing: file appears mid-wait
BATON_LATE_DIR="$TMP_DIR/late"
mkdir -p "$BATON_LATE_DIR"
BATON_LATE="$BATON_LATE_DIR/baton.json"
( sleep 1 && write_baton "$BATON_LATE" "prativadi" "phase_review" ) &
late_pid=$!
run_case "--allow-missing returns 0 when file appears" 0 \
  "$PRATIVADI_SCRIPT" --role prativadi --file "$BATON_LATE" --allow-missing --interval 1 --max-wait 5
wait "$late_pid" 2>/dev/null || true

# --allow-missing: file never appears, times out
BATON_NEVER_DIR="$TMP_DIR/never"
mkdir -p "$BATON_NEVER_DIR"
BATON_NEVER="$BATON_NEVER_DIR/baton.json"
run_case "--allow-missing returns 20 on file-missing timeout" 20 \
  "$PRATIVADI_SCRIPT" --role prativadi --file "$BATON_NEVER" --allow-missing --interval 1 --max-wait 2 --finite

# Supervised-escape path: no --allow-missing → existing exit 21 preserved.
# This is what the prativadi skill triggers when DVANDVA_NO_WAIT=1 makes
# it skip the helper-with-flag and surface the missing-baton message.
run_case "no flag returns 21 on missing baton (supervised-escape path)" 21 \
  "$PRATIVADI_SCRIPT" --role prativadi --file "$BATON_NEVER" --interval 0 --max-wait 0

# Torn-read tolerance: persistently invalid JSON still exits 22 (after one retry).
BATON_BAD="$TMP_DIR/bad.json"
printf '{"schema": "dvandva.baton.v1", "assignee": ' > "$BATON_BAD"
run_case "persistently invalid baton exits 22 after retry" 22 \
  "$SCRIPT" --role vadi --file "$BATON_BAD" --interval 0 --max-wait 0

# A torn read that heals before the 1s retry exits 0.
BATON_HEAL="$TMP_DIR/heal.json"
printf '{"schema": "dvandva.baton.v1", "assignee": ' > "$BATON_HEAL"
# Healer fires at 0.3s; the helper's retry happens at ~1s — 0.7s margin.
( sleep 0.3 && write_baton "$BATON_HEAL" "vadi" "implementing" ) &
heal_pid=$!
run_case "torn read healed by retry exits 0" 0 \
  "$SCRIPT" --role vadi --file "$BATON_HEAL" --interval 0 --max-wait 0
wait "$heal_pid" 2>/dev/null || true

# Usage advertises the 540 default (one invocation fits Claude Code's 600s Bash-tool cap).
help_output="$("$SCRIPT" --help 2>&1)"
help_exit=$?
if [[ "$help_exit" -ne 0 ]]; then
  echo "FAIL: --help exited $help_exit, expected 0"
  failures=$((failures + 1))
elif ! grep -q -- '--max-wait 540' <<< "$help_output"; then
  echo "FAIL: usage does not advertise 540 default"
  echo "$help_output"
  failures=$((failures + 1))
else
  echo "PASS: usage advertises 540 default and --help exits 0"
fi

for helper in "$SCRIPT" "$PRATIVADI_SCRIPT"; do
  if [[ -x "$helper" ]]; then
    echo "PASS: executable helper exists at ${helper#$ROOT_DIR/}"
  else
    echo "FAIL: helper missing or not executable at ${helper#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
done

if cmp -s "$SCRIPT" "$PRATIVADI_SCRIPT"; then
  echo "PASS: plugin wait helpers are byte-identical"
else
  echo "FAIL: plugin wait helpers drifted"
  failures=$((failures + 1))
fi

if [[ -d "$ROOT_DIR/skills" ]]; then
  echo "FAIL: root skills/ directory should be removed after plugin migration"
  failures=$((failures + 1))
else
  echo "PASS: root skills/ directory removed"
fi

if [[ -f "$ROOT_DIR/scripts/dvandva-wait.sh" ]]; then
  echo "FAIL: root scripts/dvandva-wait.sh should not remain a runtime helper"
  failures=$((failures + 1))
else
  echo "PASS: no root runtime wait helper remains"
fi

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
