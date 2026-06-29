#!/usr/bin/env bash
# Tests for the bundled Dvandva selector-first active-run resolver.
#
# dvandva-resolve.sh is the single source of run selection used before any
# read / write / wait / scaffold. It emits exactly one stdout line:
#   RESOLVED <path>  (exit 0)  an existing baton is selected
#   CREATE <path>    (exit 0)  no resumable run -> deterministic new named path
#   ASK <json[]>     (exit 12) >1 resumable run + no explicit selector -> stop
# Selector precedence (explicit wins): DVANDVA_BATON_FILE | DVANDVA_RUN_DIR |
# DVANDVA_RUN_ID. Taxonomy: only status==done is run-terminal; human_decision
# and human_question are resumable. The runtime ships byte-identical at both
# per-role script dirs; this suite fails if either copy is missing or drifts.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-resolve.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-resolve.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

# Seed a v2 named-run baton at <file> with run_id/status/assignee/updated_at.
seed_baton() {
  local file="$1" run_id="$2" status="$3" assignee="$4" updated_at="$5"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "$run_id",
  "status": "$status",
  "assignee": "$assignee",
  "phase": 1,
  "checkpoint": 3,
  "updated_at": "$updated_at"
}
JSON
}

# Exact-line assertion: exit code + full stdout match.
assert_line() {
  local name="$1" expected_exit="$2" expected_stdout="$3"
  shift 3
  local out actual_exit
  out="$("$@" 2>/dev/null)"
  actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name — expected exit $expected_exit, got $actual_exit"
    echo "  stdout: $out"
    failures=$((failures + 1))
    return
  fi
  if [[ "$out" != "$expected_stdout" ]]; then
    echo "FAIL: $name — stdout mismatch"
    echo "  expected: $expected_stdout"
    echo "  got:      $out"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: $name"
}

# Substring assertion: exit code + stdout contains every given needle.
assert_contains() {
  local name="$1" expected_exit="$2"
  shift 2
  # Remaining args: needles... -- cmd...
  local needles=()
  while [[ $# -gt 0 && "$1" != "--" ]]; do
    needles+=("$1")
    shift
  done
  shift  # drop the --
  local out actual_exit
  out="$("$@" 2>/dev/null)"
  actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name — expected exit $expected_exit, got $actual_exit"
    echo "  stdout: $out"
    failures=$((failures + 1))
    return
  fi
  local needle
  for needle in "${needles[@]}"; do
    if [[ "$out" != *"$needle"* ]]; then
      echo "FAIL: $name — stdout missing: $needle"
      echo "  got: $out"
      failures=$((failures + 1))
      return
    fi
  done
  echo "PASS: $name"
}

# (a) no baton at all -> CREATE deterministic slug, exit 0.
BOX_A="$TMP_DIR/a-empty"
mkdir -p "$BOX_A"
assert_line "no baton -> CREATE" 0 "CREATE .dvandva/runs/run/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_A"

# (b) one resumable active run -> RESOLVED that path.
BOX_B="$TMP_DIR/b-one-active"
seed_baton "$BOX_B/.dvandva/runs/alpha/baton.json" "alpha" "spec_review" "prativadi" "2026-06-29T10:00:00Z"
assert_line "one active run -> RESOLVED" 0 "RESOLVED .dvandva/runs/alpha/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_B"

# (c) one human_decision -> RESOLVED (resume), NOT CREATE.
BOX_C="$TMP_DIR/c-human-decision"
seed_baton "$BOX_C/.dvandva/runs/decide/baton.json" "decide" "human_decision" "human" "2026-06-29T10:00:00Z"
assert_line "human_decision is resumable -> RESOLVED" 0 "RESOLVED .dvandva/runs/decide/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_C"

# (d) one human_question -> RESOLVED (resume).
BOX_D="$TMP_DIR/d-human-question"
seed_baton "$BOX_D/.dvandva/runs/askrun/baton.json" "askrun" "human_question" "human" "2026-06-29T10:00:00Z"
assert_line "human_question is resumable -> RESOLVED" 0 "RESOLVED .dvandva/runs/askrun/baton.json" \
  "$SCRIPT" --role prativadi --cwd "$BOX_D"

# (e) only a done baton -> CREATE (done is the only run-terminal status).
BOX_E="$TMP_DIR/e-only-done"
seed_baton "$BOX_E/.dvandva/runs/finished/baton.json" "finished" "done" "human" "2026-06-29T10:00:00Z"
assert_line "only done archive -> CREATE" 0 "CREATE .dvandva/runs/run/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_E"

# (e2) done archive named 'run' forces a deterministic non-colliding slug.
BOX_E2="$TMP_DIR/e2-done-named-run"
seed_baton "$BOX_E2/.dvandva/runs/run/baton.json" "run" "done" "human" "2026-06-29T10:00:00Z"
assert_line "done archive named run -> CREATE run-2" 0 "CREATE .dvandva/runs/run-2/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_E2"

# (f) two non-terminal runs + no selector -> ASK, exit 12, both candidates present.
BOX_F="$TMP_DIR/f-two-active"
seed_baton "$BOX_F/.dvandva/runs/alpha/baton.json" "alpha" "spec_review" "prativadi" "2026-06-29T10:00:00Z"
seed_baton "$BOX_F/.dvandva/runs/beta/baton.json" "beta" "implementing" "vadi" "2026-06-29T11:00:00Z"
assert_contains "two active + no selector -> ASK(12)" 12 \
  "ASK " '"run_id":"alpha"' '"run_id":"beta"' \
  -- "$SCRIPT" --role vadi --cwd "$BOX_F"

# (f2) ASK ordering is deterministic: updated_at desc, then run_id asc.
# beta (11:00) must precede alpha (10:00) in the candidate JSON.
ask_out="$("$SCRIPT" --role vadi --cwd "$BOX_F" 2>/dev/null)"
ask_json="${ask_out#ASK }"
order="$(printf '%s' "$ask_json" | jq -r '[.[].run_id] | join(",")' 2>/dev/null)"
if [[ "$order" == "beta,alpha" ]]; then
  echo "PASS: ASK orders by updated_at desc (beta,alpha)"
else
  echo "FAIL: ASK ordering wrong — expected beta,alpha got '$order'"
  failures=$((failures + 1))
fi

# (f3) updated_at tie -> run_id ascending tiebreak.
BOX_F3="$TMP_DIR/f3-tie"
seed_baton "$BOX_F3/.dvandva/runs/zeta/baton.json" "zeta" "spec_review" "prativadi" "2026-06-29T10:00:00Z"
seed_baton "$BOX_F3/.dvandva/runs/gamma/baton.json" "gamma" "implementing" "vadi" "2026-06-29T10:00:00Z"
tie_out="$("$SCRIPT" --role vadi --cwd "$BOX_F3" 2>/dev/null)"
tie_order="$(printf '%s' "${tie_out#ASK }" | jq -r '[.[].run_id] | join(",")' 2>/dev/null)"
if [[ "$tie_order" == "gamma,zeta" ]]; then
  echo "PASS: ASK breaks updated_at ties by run_id asc (gamma,zeta)"
else
  echo "FAIL: ASK tiebreak wrong — expected gamma,zeta got '$tie_order'"
  failures=$((failures + 1))
fi

# (g) DVANDVA_BATON_FILE wins -> RESOLVED that exact path.
BOX_G="$TMP_DIR/g-baton-file"
EXPLICIT_FILE="$BOX_G/custom/explicit-baton.json"
seed_baton "$EXPLICIT_FILE" "custom" "spec_review" "prativadi" "2026-06-29T10:00:00Z"
# Also seed two active runs to prove the explicit selector skips discovery/ASK.
seed_baton "$BOX_G/.dvandva/runs/alpha/baton.json" "alpha" "spec_review" "prativadi" "2026-06-29T10:00:00Z"
seed_baton "$BOX_G/.dvandva/runs/beta/baton.json" "beta" "implementing" "vadi" "2026-06-29T11:00:00Z"
assert_line "DVANDVA_BATON_FILE wins -> RESOLVED exact path" 0 "RESOLVED $EXPLICIT_FILE" \
  env DVANDVA_BATON_FILE="$EXPLICIT_FILE" "$SCRIPT" --role vadi --cwd "$BOX_G"

# (h) DVANDVA_RUN_DIR wins -> RESOLVED that dir's baton.json.
BOX_H="$TMP_DIR/h-run-dir"
RUN_DIR_PATH="$BOX_H/.dvandva/runs/gamma"
seed_baton "$RUN_DIR_PATH/baton.json" "gamma" "implementing" "vadi" "2026-06-29T10:00:00Z"
assert_line "DVANDVA_RUN_DIR wins -> RESOLVED dir baton" 0 "RESOLVED $RUN_DIR_PATH/baton.json" \
  env DVANDVA_RUN_DIR="$RUN_DIR_PATH" "$SCRIPT" --role vadi --cwd "$BOX_H"

# (h2) DVANDVA_RUN_DIR with trailing slash is normalized.
assert_line "DVANDVA_RUN_DIR trailing slash normalized" 0 "RESOLVED $RUN_DIR_PATH/baton.json" \
  env DVANDVA_RUN_DIR="$RUN_DIR_PATH/" "$SCRIPT" --role vadi --cwd "$BOX_H"

# (i) safe DVANDVA_RUN_ID=alpha -> RESOLVED .dvandva/runs/alpha/baton.json.
BOX_I="$TMP_DIR/i-run-id"
mkdir -p "$BOX_I"
assert_line "safe DVANDVA_RUN_ID -> RESOLVED named path" 0 "RESOLVED .dvandva/runs/alpha/baton.json" \
  env DVANDVA_RUN_ID="alpha" "$SCRIPT" --role vadi --cwd "$BOX_I"

# (j) unsafe DVANDVA_RUN_ID values -> exit 2 before any fs op.
BOX_J="$TMP_DIR/j-unsafe"
mkdir -p "$BOX_J"
for bad in "../x" "a/b" ".." "a..b" 'a\b' "" "   "; do
  assert_line "unsafe DVANDVA_RUN_ID '$bad' -> exit 2" 2 "" \
    env DVANDVA_RUN_ID="$bad" "$SCRIPT" --role vadi --cwd "$BOX_J"
done

# (j2) unsafe DVANDVA_RUN_ID exits before touching the filesystem: even with a
# non-existent --cwd, it must still reject on the run id (exit 2), not on cwd.
assert_line "unsafe DVANDVA_RUN_ID rejected before fs op" 2 "" \
  env DVANDVA_RUN_ID="../escape" "$SCRIPT" --role vadi --cwd "$TMP_DIR/does-not-exist"

# Usage errors: missing role / unknown role / missing --cwd value.
assert_line "missing --role -> usage exit 2" 2 "" \
  "$SCRIPT" --cwd "$BOX_A"
assert_line "unknown role -> usage exit 2" 2 "" \
  "$SCRIPT" --role bystander --cwd "$BOX_A"

# (k) vadi and prativadi produce identical stdout for identical inputs.
v_out="$("$SCRIPT" --role vadi --cwd "$BOX_F" 2>/dev/null)"
p_out="$("$PRATIVADI_SCRIPT" --role prativadi --cwd "$BOX_F" 2>/dev/null)"
if [[ "$v_out" == "$p_out" ]]; then
  echo "PASS: vadi and prativadi yield identical resolution stdout"
else
  echo "FAIL: vadi/prativadi resolution stdout diverged"
  echo "  vadi:      $v_out"
  echo "  prativadi: $p_out"
  failures=$((failures + 1))
fi

# (k2) the two per-role runtime copies are byte-identical and executable.
for helper in "$SCRIPT" "$PRATIVADI_SCRIPT"; do
  if [[ -x "$helper" ]]; then
    echo "PASS: executable resolver exists at ${helper#$ROOT_DIR/}"
  else
    echo "FAIL: resolver missing or not executable at ${helper#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
done

if cmp -s "$SCRIPT" "$PRATIVADI_SCRIPT"; then
  echo "PASS: plugin resolve helpers are byte-identical"
else
  echo "FAIL: plugin resolve helpers drifted"
  failures=$((failures + 1))
fi

# No root runtime copy of the resolver may exist (per-role only).
if [[ -f "$ROOT_DIR/scripts/dvandva-resolve.sh" ]]; then
  echo "FAIL: root scripts/dvandva-resolve.sh should not exist (per-role runtime only)"
  failures=$((failures + 1))
else
  echo "PASS: no root runtime resolve helper remains"
fi

# Legacy .dvandva/baton.json participates in discovery as a single resumable run.
BOX_L="$TMP_DIR/l-legacy"
seed_baton "$BOX_L/.dvandva/baton.json" "legacy-run" "implementing" "vadi" "2026-06-29T10:00:00Z"
assert_line "legacy baton resolves as the single resumable run" 0 "RESOLVED .dvandva/baton.json" \
  "$SCRIPT" --role vadi --cwd "$BOX_L"

if [[ "$failures" -gt 0 ]]; then
  echo "TOTAL FAILURES: $failures"
  exit 1
fi

echo "ALL RESOLVE TESTS PASSED"
exit 0
