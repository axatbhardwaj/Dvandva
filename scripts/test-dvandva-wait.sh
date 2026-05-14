#!/usr/bin/env bash
# Tests for scripts/dvandva-wait.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/dvandva-wait.sh"
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
  "$SCRIPT" --role vadi --file "$BATON_WAIT" --interval 0 --max-wait 0

run_case "rejects zero interval with positive max wait" 2 \
  "$SCRIPT" --role vadi --file "$BATON_WAIT" --interval 0 --max-wait 1

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
