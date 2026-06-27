#!/usr/bin/env bash
# Tests for the bundled Dvandva write helpers (validated atomic baton install).
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh"
SCHEMA_SEED="$ROOT_DIR/plugins/dvandva/references/baton-schema.json"
V2_SCHEMA_SEED="$ROOT_DIR/plugins/dvandva/references/baton-schema-v2.json"
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

make_baton_v2() {
  local file="$1" status="$2" assignee="$3" checkpoint="$4"
  shift 4
  local phase_json='"research"'
  case "$status" in
    spec_drafting|spec_review|spec_revision)
      phase_json='"spec"'
      ;;
    implementing|test_creation|deep_review|deslop|phase_review|phase_fixing|review_of_review|counter_review|done)
      phase_json='1'
      ;;
  esac
  local prog='.updated_at = "2026-06-27T00:00:00Z"
    | .status = $s
    | .assignee = $a
    | .checkpoint = $c
    | .phase = $p
    | .run_id = "run-a"
    | .original_ask = "Original user ask for v2 enforcement"
    | .research_ref = "./superpowers/research/run-a.html"
    | .current_engine = "codex"
    | .branch = "test-branch"
    | .master_plan_locked = false
    | .question = null
    | .resume_assignee = null
    | .resume_status = null'
  local extra
  for extra in "$@"; do
    prog="$prog | $extra"
  done
  mkdir -p "$(dirname "$file")"
  jq --arg s "$status" --arg a "$assignee" --argjson c "$checkpoint" --argjson p "$phase_json" "$prog" "$V2_SCHEMA_SEED" > "$file"
}

v2_status_owner() {
  case "$1" in
    research_drafting|research_revision|spec_drafting|spec_revision|implementing|test_creation|deslop|phase_fixing|review_of_review)
      echo "vadi"
      ;;
    research_review|spec_review|deep_review|phase_review|counter_review)
      echo "prativadi"
      ;;
    human_question|human_decision|done)
      echo "human"
      ;;
    *)
      echo "vadi"
      ;;
  esac
}

v2_review_angles_filter() {
  cat <<'JQ'
.subagent_tracks += [
  {
    "id": "review-correctness",
    "phase": "deep_review",
    "status": "completed",
    "track": "correctness-regression",
    "owner": "dvandva-review-correctness",
    "parallelized": true,
    "rationale": "Independent correctness and regression review can run without editing shared files.",
    "inputs": ["candidate diff"],
    "outputs": ["No correctness or regression blockers found."],
    "evidence_refs": ["subagent:review-correctness"],
    "result": "passed"
  },
  {
    "id": "review-tests",
    "phase": "deep_review",
    "status": "completed",
    "track": "test-evidence",
    "owner": "dvandva-review-tests",
    "parallelized": true,
    "rationale": "Independent test evidence review can run beside correctness and protocol review.",
    "inputs": ["verification output"],
    "outputs": ["Coverage and motivating tests accepted."],
    "evidence_refs": ["subagent:review-tests"],
    "result": "passed"
  },
  {
    "id": "review-protocol",
    "phase": "deep_review",
    "status": "completed",
    "track": "protocol-handoff",
    "owner": "dvandva-review-protocol",
    "parallelized": true,
    "rationale": "Independent protocol handoff review checks baton and docs without editing code.",
    "inputs": ["baton candidate"],
    "outputs": ["Handoff state accepted."],
    "evidence_refs": ["subagent:review-protocol"],
    "result": "passed"
  }
]
JQ
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

run_case_contains() {
  local name="$1"
  local expected_exit="$2"
  local expected_text="$3"
  shift 3

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name expected exit $expected_exit, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return 1
  fi
  if [[ "$output" != *"$expected_text"* ]]; then
    echo "FAIL: $name missing expected output: $expected_text"
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

RUNS_BOX="$(new_box run-isolation)"
ALPHA_DIR="$RUNS_BOX/.dvandva/runs/alpha"
BETA_DIR="$RUNS_BOX/.dvandva/runs/beta"
make_baton "$ALPHA_DIR/baton.next.json" "spec_drafting" "vadi" 0 '.branch = "alpha-branch"'
make_baton "$BETA_DIR/baton.next.json" "spec_drafting" "vadi" 0 '.branch = "beta-branch"'
run_case "run alpha scaffold writes only alpha baton" 0 \
  "$SCRIPT" "$ALPHA_DIR/baton.json" "$ALPHA_DIR/baton.next.json"
run_case "run beta scaffold writes only beta baton" 0 \
  "$SCRIPT" "$BETA_DIR/baton.json" "$BETA_DIR/baton.next.json"
if [[ -f "$ALPHA_DIR/baton.json" && -f "$ALPHA_DIR/history/0-spec_drafting-vadi.json" \
  && -f "$BETA_DIR/baton.json" && -f "$BETA_DIR/history/0-spec_drafting-vadi.json" \
  && ! -e "$RUNS_BOX/.dvandva/history" ]]; then
  echo "PASS: two named runs keep batons and histories isolated"
else
  echo "FAIL: two named runs collided or wrote shared history"
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
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 '.schema = "dvandva.baton.v3"'
run_case_contains "wrong schema string exits 23" 23 "DVANDVA_WRITE schema_mismatch" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box cand-key)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0 'del(.branch)'
run_case_contains "missing required key exits 23" 23 "DVANDVA_WRITE missing_key" \
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

# --- v2 candidate-level validation and scaffold ---

BOX="$(new_box v2-scaffold-ok)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0
run_case "v2 scaffold research_drafting installs and snapshots" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if jq -e '.schema == "dvandva.baton.v2" and .run_id == "run-a"' "$BOX/baton.json" >/dev/null 2>&1 \
  && [[ -f "$BOX/history/0-research_drafting-vadi.json" ]]; then
  echo "PASS: v2 scaffold wrote run id and snapshot"
else
  echo "FAIL: v2 scaffold missing run id or snapshot"
  failures=$((failures + 1))
fi

BOX="$(new_box v2-empty-run-id)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.run_id = ""'
run_case_contains "v2 empty run_id exits 23" 23 "DVANDVA_WRITE bad_run_id" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-unsafe-run-id-parent)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.run_id = "../escape"'
run_case_contains "v2 unsafe parent run_id exits 23" 23 "DVANDVA_WRITE bad_run_id" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-unsafe-run-id-slash)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.run_id = "alpha/beta"'
run_case_contains "v2 unsafe slash run_id exits 23" 23 "DVANDVA_WRITE bad_run_id" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-empty-original-ask)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.original_ask = ""'
run_case_contains "v2 empty original_ask exits 23" 23 "DVANDVA_WRITE bad_original_ask" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-missing-work-split)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.work_split)'
run_case_contains "v2 missing work_split exits 23" 23 "DVANDVA_WRITE missing_key key=work_split" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-empty-work-split)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.work_split = []'
run_case_contains "v2 empty work_split exits 23" 23 "DVANDVA_WRITE bad_work_split" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-empty-verification-matrix)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.verification_matrix = []'
run_case_contains "v2 empty verification_matrix exits 23" 23 "DVANDVA_WRITE bad_verification_matrix" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-missing-run-explainer)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.run_explainer_ref)'
run_case_contains "v2 missing run_explainer_ref exits 23" 23 "DVANDVA_WRITE missing_key key=run_explainer_ref" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-empty-subagent-tracks)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.subagent_tracks = []'
run_case_contains "v2 empty subagent_tracks exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-malformed-subagent-tracks)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.subagent_tracks[0].owner)'
run_case_contains "v2 malformed subagent_tracks exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-fake-parallel-subagent-track)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  '.subagent_tracks[0].parallelized = true' \
  '.subagent_tracks[0].owner = "vadi"' \
  '.subagent_tracks[0].outputs = []' \
  '.subagent_tracks[0].evidence_refs = []'
run_case_contains "v2 fake parallel subagent track exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-phase-status-mismatch)"
make_baton_v2 "$BOX/baton.next.json" "implementing" "vadi" 0 '.phase = "research"'
run_case_contains "v2 implementation status rejects research phase" 23 "DVANDVA_WRITE bad_phase_status" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-missing-research-ref-after-draft)"
make_baton_v2 "$BOX/baton.next.json" "research_review" "prativadi" 0 '.research_ref = null'
run_case_contains "v2 missing research_ref after draft exits 23" 23 "DVANDVA_WRITE bad_research_ref" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-early-human-question-no-research-ref)"
make_baton_v2 "$BOX/baton.json" "research_drafting" "vadi" 0 '.research_ref = null'
make_baton_v2 "$BOX/baton.next.json" "human_question" "human" 1 \
  '.research_ref = null' '.question = "Which source should research use?"' '.resume_assignee = "vadi"' '.resume_status = "research_drafting"'
run_case "v2 research_drafting without research_ref can enter human_question" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-early-human-decision-no-research-ref)"
make_baton_v2 "$BOX/baton.json" "research_drafting" "vadi" 0 '.research_ref = null'
make_baton_v2 "$BOX/baton.next.json" "human_decision" "human" 1 '.research_ref = null'
run_case "v2 research_drafting without research_ref can escalate human_decision" 0 \
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

# --- transitions: documented v2 edges are legal ---

V2_EDGES="research_drafting:research_review research_review:research_revision research_revision:research_review research_review:spec_drafting implementing:test_creation test_creation:deep_review deep_review:phase_fixing deep_review:deslop phase_fixing:test_creation deslop:phase_fixing deslop:implementing deslop:done"
i=0
for edge in $V2_EDGES; do
  i=$((i + 1))
  cur="${edge%%:*}"
  new="${edge##*:}"
  BOX="$(new_box "v2-edge-$i")"
  extras=()
  if [[ "$edge" == "deep_review:deslop" ]]; then
    extras+=("$(v2_review_angles_filter)")
  fi
  if [[ "$edge" == "deslop:done" ]]; then
    extras+=('.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"')
  fi
  make_baton_v2 "$BOX/baton.json" "$cur" "$(v2_status_owner "$cur")" 4
  make_baton_v2 "$BOX/baton.next.json" "$new" "$(v2_status_owner "$new")" 5 "${extras[@]}"
  run_case "v2 edge $edge is legal" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

BOX="$(new_box v2-schema-downgrade-research)"
make_baton_v2 "$BOX/baton.json" "research_review" "prativadi" 4
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 5
run_case_contains "v2 current cannot downgrade to v1 candidate during research" 24 "schema_change" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-schema-downgrade-implementation)"
make_baton_v2 "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
run_case_contains "v2 current cannot downgrade to v1 candidate during implementation" 24 "schema_change" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v1-schema-upgrade-mid-run)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "spec_review" "prativadi" 5
run_case_contains "v1 current cannot silently upgrade to v2 candidate" 24 "schema_change" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-run-id-mutation)"
make_baton_v2 "$BOX/baton.json" "research_review" "prativadi" 4 '.run_id = "alpha"'
make_baton_v2 "$BOX/baton.next.json" "research_revision" "vadi" 5 '.run_id = "beta"'
run_case_contains "v2 current cannot change run_id mid-run" 24 "run_id_change" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-current-missing-run-id)"
make_baton_v2 "$BOX/baton.json" "research_review" "prativadi" 4 'del(.run_id)'
make_baton_v2 "$BOX/baton.next.json" "research_revision" "vadi" 5
run_case_contains "v2 current missing run_id exits 25" 25 "bad_run_id" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-wrong-owner-revision)"
make_baton_v2 "$BOX/baton.json" "research_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "research_revision" "prativadi" 5
run_case_contains "v2 research_revision requires vadi assignee" 23 "bad_assignee_owner" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-wrong-owner-deep-review)"
make_baton_v2 "$BOX/baton.json" "test_creation" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "deep_review" "vadi" 5
run_case_contains "v2 deep_review requires prativadi assignee" 23 "bad_assignee_owner" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-wrong-owner-deslop)"
make_baton_v2 "$BOX/baton.json" "deep_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "deslop" "prativadi" 5
run_case_contains "v2 deslop requires vadi assignee" 23 "bad_assignee_owner" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-done-missing-run-explainer)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = null'
run_case_contains "v2 done requires run_explainer_ref" 23 "bad_run_explainer_ref" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-done-invalid-run-explainer-path)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = "../run-a-explainer.html"'
run_case_contains "v2 done rejects invalid run_explainer_ref path" 23 "bad_run_explainer_ref" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-done-mismatched-run-explainer)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4 '.run_id = "alpha"'
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 \
  '.run_id = "alpha"' \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-beta-explainer.html"'
run_case_contains "v2 done rejects run_explainer_ref for different run_id" 23 "bad_run_explainer_ref" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-done-valid-run-explainer)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"'
run_case "v2 done accepts valid run_explainer_ref path" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-deep-review-missing-angles)"
make_baton_v2 "$BOX/baton.json" "deep_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "deslop" "vadi" 5
run_case_contains "v2 deep_review->deslop requires three review angles" 24 "three completed review-angle subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-deep-review-stale-angles)"
make_baton_v2 "$BOX/baton.json" "deep_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "deslop" "vadi" 5 \
  "$(v2_review_angles_filter)" \
  '.subagent_tracks |= map(if (.track == "correctness-regression" or .track == "test-evidence" or .track == "protocol-handoff") then .phase = "research" else . end)'
run_case_contains "v2 deep_review->deslop rejects stale review angles from another phase" 24 "three completed review-angle subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-deep-review-empty-evidence)"
make_baton_v2 "$BOX/baton.json" "deep_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "deslop" "vadi" 5 \
  "$(v2_review_angles_filter)" \
  '.subagent_tracks |= map(if (.track == "correctness-regression" or .track == "test-evidence" or .track == "protocol-handoff") then .parallelized = false | .owner = "prativadi" | .inputs = [] | .outputs = [] | .evidence_refs = [] else . end)'
run_case_contains "v2 deep_review->deslop rejects empty review evidence" 24 "three completed review-angle subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-deep-review-with-angles)"
make_baton_v2 "$BOX/baton.json" "deep_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "deslop" "vadi" 5 "$(v2_review_angles_filter)"
run_case "v2 deep_review->deslop accepts three review angles" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-illegal-skip)"
make_baton_v2 "$BOX/baton.json" "research_drafting" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "spec_drafting" "vadi" 5
run_case "v2 research_drafting->spec_drafting exits 24" 24 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-research-human-question)"
make_baton_v2 "$BOX/baton.json" "research_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "human_question" "human" 5 \
  '.question = "Which source should research use?"' '.resume_assignee = "prativadi"' '.resume_status = "research_review"'
run_case "v2 research state can enter human_question" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

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

# --- install failure (read-only baton dir) exits 26, baton unchanged ---

BOX="$(new_box install-fail)"
make_baton "$BOX/baton.next.json" "spec_drafting" "vadi" 0
chmod a-w "$BOX"
run_case "read-only baton dir exits 26" 26 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
chmod u+w "$BOX"
if [[ ! -f "$BOX/baton.json" ]]; then
  echo "PASS: failed install left no baton behind"
else
  echo "FAIL: failed install created a baton"
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
