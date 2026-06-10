#!/usr/bin/env bash
# Tests for the bundled Dvandva write helpers (validated atomic baton install).
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh"
SCHEMA_SEED="$ROOT_DIR/plugins/dvandva/references/baton-schema.json"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

# Build a full 36-key baton from the bundled schema seed with overrides.
# Usage: make_baton <file> <status> <assignee> <checkpoint> [extra-jq-filter ...]
make_baton() {
  local file="$1" status="$2" assignee="$3" checkpoint="$4"
  shift 4
  local prog='.status = $s | .assignee = $a | .checkpoint = $c | .master_plan_locked = false | .question = null | .resume_assignee = null | .resume_status = null'
  local extra
  for extra in "$@"; do
    prog="$prog | $extra"
  done
  mkdir -p "$(dirname "$file")"
  jq --arg s "$status" --arg a "$assignee" --argjson c "$checkpoint" "$prog" "$SCHEMA_SEED" > "$file"
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
    return 1
  fi
  echo "PASS: $name"
  return 0
}

# Fresh sandbox per scenario keeps history/ and baton state isolated.
new_box() {
  local box="$TMP_DIR/box-$1"
  mkdir -p "$box"
  echo "$box"
}

# --- scaffold ---

BOX="$(new_box scaffold-ok)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0
run_case "scaffold installs and snapshots" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if [[ -f "$BOX/baton.json" && -f "$BOX/history/0-spec_drafting-vadi.json" && -f "$BOX/baton.next.json" ]]; then
  echo "PASS: scaffold wrote baton, snapshot, and left candidate in place"
else
  echo "FAIL: scaffold missing baton, snapshot, or candidate"
  failures=$((failures + 1))
fi

BOX="$(new_box scaffold-bad)"
make_baton "$BOX/baton.next.json" "implementing" "vadi" 0
run_case "scaffold with wrong initial status exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if [[ ! -f "$BOX/baton.json" ]]; then
  echo "PASS: rejected scaffold left no baton behind"
else
  echo "FAIL: rejected scaffold created a baton"
  failures=$((failures + 1))
fi

# --- candidate-level validation ---

BOX="$(new_box cand-missing)"
run_case "missing candidate exits 21" 21 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-invalid)"
printf '{"schema": ' > "$BOX/baton.next.json"
run_case "invalid candidate JSON exits 22" 22 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-schema)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 '.schema = "dvandva.baton.v2"'
run_case "wrong schema string exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-key)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 'del(.branch)'
run_case "missing required key exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-status)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 '.status = "doing_stuff"'
run_case "unknown status exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-assignee)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 '.assignee = ""'
run_case "empty assignee exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-ckpt-string)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 '.checkpoint = "5"'
run_case "string checkpoint exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-ckpt-octal)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 7
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 8 '.checkpoint = "08"'
run_case "octal-string checkpoint exits 23" 23 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box current-ckpt-string)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 7 '.checkpoint = "7"'
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 8
run_case "string checkpoint in current baton exits 25" 25 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

# --- transitions: every documented v1 edge is legal ---

EDGES="spec_drafting:spec_review spec_review:spec_revision spec_review:implementing spec_revision:spec_review implementing:phase_review phase_review:phase_fixing phase_review:review_of_review phase_review:implementing phase_review:done phase_fixing:phase_review review_of_review:implementing review_of_review:done review_of_review:counter_review counter_review:implementing counter_review:done counter_review:review_of_review"
i=0
for edge in $EDGES; do
  i=$((i + 1))
  cur="${edge%%:*}"
  new="${edge##*:}"
  BOX="$(new_box "edge-$i")"
  make_baton "$BOX/baton.json" "$cur" "vadi" 4
  make_baton "$BOX/baton.next.json" "$new" "prativadi" 5
  run_case "edge $edge is legal" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

# --- transitions: illegal edges ---

BOX="$(new_box illegal-impl-done)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "done" "human" 5
run_case "implementing->done exits 24 (no self-declared done)" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box illegal-same)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "implementing" "vadi" 5
run_case "same-status rewrite exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box illegal-skip)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton "$BOX/baton.next.json" "implementing" "vadi" 5
run_case "spec_drafting->implementing exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box illegal-checkpoint)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 7
run_case "checkpoint jump exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if cmp -s <(jq -S . "$BOX/baton.json") <(jq -S . <(make_baton /dev/stdout "spec_drafting" "vadi" 4)); then
  echo "PASS: rejected write left baton bytes unchanged"
else
  echo "FAIL: rejected write modified the baton"
  failures=$((failures + 1))
fi

# --- universal escalation and human resume ---

for src in spec_drafting implementing phase_review counter_review; do
  BOX="$(new_box "esc-$src")"
  make_baton "$BOX/baton.json" "$src" "vadi" 4
  make_baton "$BOX/baton.next.json" "human_decision" "human" 5
  run_case "$src->human_decision is legal" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

BOX="$(new_box resume-decision)"
make_baton "$BOX/baton.json" "human_decision" "human" 4
make_baton "$BOX/baton.next.json" "implementing" "vadi" 5
run_case "human_decision->implementing (human-authorized) is legal" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

# --- human_question rules ---

BOX="$(new_box hq-entry-ok)"
make_baton "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton "$BOX/baton.next.json" "human_question" "human" 5 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
run_case "spec human_question entry with fields is legal" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-entry-locked)"
make_baton "$BOX/baton.json" "spec_review" "prativadi" 4 '.master_plan_locked = true'
make_baton "$BOX/baton.next.json" "human_question" "human" 5 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
run_case "human_question after plan lock exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-entry-nulls)"
make_baton "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton "$BOX/baton.next.json" "human_question" "human" 5
run_case "human_question entry with null fields exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-entry-impl)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "human_question" "human" 5 \
  '.question = "Which scope?"' '.resume_assignee = "vadi"' '.resume_status = "spec_review"'
run_case "human_question from non-spec state exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-resume-ok)"
make_baton "$BOX/baton.json" "human_question" "human" 4 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 5
run_case "human_question resume matching resume fields is legal" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-resume-bad)"
make_baton "$BOX/baton.json" "human_question" "human" 4 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
make_baton "$BOX/baton.next.json" "implementing" "vadi" 5
run_case "human_question resume to wrong state exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box hq-resume-uncleared)"
make_baton "$BOX/baton.json" "human_question" "human" 4 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 5 \
  '.question = "Which scope?"' '.resume_assignee = "prativadi"' '.resume_status = "spec_review"'
run_case "human_question resume without clearing fields exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

# --- broken current baton is never clobbered ---

BOX="$(new_box broken-current)"
printf '{"schema": "dvandva.baton.v1", "assignee": ' > "$BOX/baton.json"
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 5
run_case "unparseable current baton exits 25" 25 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if [[ "$(cat "$BOX/baton.json")" == '{"schema": "dvandva.baton.v1", "assignee": ' ]]; then
  echo "PASS: broken current baton bytes preserved"
else
  echo "FAIL: broken current baton was modified"
  failures=$((failures + 1))
fi

# --- snapshot failure after install exits 30, baton IS installed ---

LONELY_DIR="$TMP_DIR/lonely-bin"
mkdir -p "$LONELY_DIR"
cp "$SCRIPT" "$LONELY_DIR/dvandva-write.sh"
chmod +x "$LONELY_DIR/dvandva-write.sh"
BOX="$(new_box snapshot-fail)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0
run_case "missing sibling snapshot helper exits 30" 30 \
  "$LONELY_DIR/dvandva-write.sh" "$BOX/baton.json" "$BOX/baton.next.json"
if jq -e '.status == "spec_drafting"' "$BOX/baton.json" >/dev/null 2>&1; then
  echo "PASS: baton installed despite snapshot failure"
else
  echo "FAIL: baton not installed on snapshot failure"
  failures=$((failures + 1))
fi

# --- usage and hygiene ---

run_case "usage error without args exits 2" 2 "$SCRIPT"

for helper in "$SCRIPT" "$PRATIVADI_SCRIPT"; do
  if [[ -x "$helper" ]]; then
    echo "PASS: executable helper exists at ${helper#$ROOT_DIR/}"
  else
    echo "FAIL: helper missing or not executable at ${helper#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
done

if cmp -s "$SCRIPT" "$PRATIVADI_SCRIPT"; then
  echo "PASS: plugin write helpers are byte-identical"
else
  echo "FAIL: plugin write helpers drifted"
  failures=$((failures + 1))
fi

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
