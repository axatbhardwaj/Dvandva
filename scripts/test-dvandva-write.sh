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
    implementing|parallel_implementing|test_creation|cross_review|cross_fixing|deep_review|deslop|phase_review|phase_fixing|review_of_review|counter_review|done)
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
    parallel_implementing|cross_review|cross_fixing)
      echo "team"
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
	    "owner": "dvandva-deep-reviewer",
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
	    "owner": "dvandva-deep-reviewer",
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
	    "owner": "dvandva-baton-auditor",
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

v2_parallel_chunks_filter() {
  cat <<'JQ'
.active_roles = ["vadi", "prativadi"]
| .work_split += [
  {
    "id": "implementation-chunk-a",
    "phase": "1",
    "chunk_type": "implementation",
    "owner": "vadi",
    "owner_role": "vadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Vadi-owned implementation chunk A.",
    "paths": ["src/a.ts"],
    "cross_review_by": "prativadi",
    "can_parallelize": true,
    "parallel_rationale": "Independent file.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "implementation-chunk-b",
    "phase": "1",
    "chunk_type": "implementation",
    "owner": "vadi",
    "owner_role": "vadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Vadi-owned implementation chunk B.",
    "paths": ["src/b.ts"],
    "cross_review_by": "prativadi",
    "can_parallelize": true,
    "parallel_rationale": "Independent file.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "implementation-chunk-c",
    "phase": "1",
    "chunk_type": "implementation",
    "owner": "prativadi",
    "owner_role": "prativadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Prativadi-owned implementation chunk C.",
    "paths": ["src/c.ts"],
    "cross_review_by": "vadi",
    "can_parallelize": true,
    "parallel_rationale": "Independent file.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "implementation-chunk-d",
    "phase": "1",
    "chunk_type": "implementation",
    "owner": "prativadi",
    "owner_role": "prativadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Prativadi-owned implementation chunk D.",
    "paths": ["src/d.ts"],
    "cross_review_by": "vadi",
    "can_parallelize": true,
    "parallel_rationale": "Independent file.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "implementation-chunk-e",
    "phase": "1",
    "chunk_type": "implementation",
    "owner": "vadi",
    "owner_role": "vadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Vadi-owned integration chunk E.",
    "paths": ["src/e.ts"],
    "cross_review_by": "prativadi",
    "can_parallelize": true,
    "parallel_rationale": "Independent file.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  }
]
JQ
}

v2_dynamic_agent_instances_filter() {
  cat <<'JQ'
.agent_instances = [
  {
    "id": "r3-generated-dynamic-review",
    "parent_role": "vadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": "research",
    "purpose": "Run-scoped generated agent for dynamic-agent gate coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "verify-only",
    "status": "closed",
    "work_item_ids": ["implementation-chunk-1"],
    "read_paths": ["plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"],
    "write_paths": [],
    "depends_on": [],
    "conflict_group": "r3-dynamic-review",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-generated-dynamic-review"],
    "evidence_refs": ["subagent:r3-generated-dynamic-review", "closed:r3-generated-dynamic-review"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  }
]
JQ
}

v2_dynamic_parallel_track_filter() {
  cat <<'JQ'
.subagent_tracks[0].parallelized = true
| .subagent_tracks[0].owner = "r3-generated-dynamic-review"
| .subagent_tracks[0].outputs = ["Generated dynamic review completed."]
| .subagent_tracks[0].evidence_refs = ["subagent:r3-generated-dynamic-review", "closed:r3-generated-dynamic-review"]
JQ
}

v2_implementation_tracks_filter() {
  cat <<'JQ'
.subagent_tracks += [
  {
    "id": "impl-a",
    "phase": 1,
    "status": "completed",
    "track": "implementation-chunk",
    "owner": "dvandva-implementer",
    "owner_role": "vadi",
    "parallelized": true,
    "rationale": "Vadi implementation chunk completed in parallel.",
    "inputs": ["implementation-chunk-a"],
    "outputs": ["Chunk A implemented."],
    "evidence_refs": ["subagent:impl-a"],
    "result": "passed"
  },
  {
    "id": "impl-b",
    "phase": 1,
    "status": "completed",
    "track": "implementation-chunk",
    "owner": "dvandva-implementer",
    "owner_role": "vadi",
    "parallelized": true,
    "rationale": "Vadi implementation chunk completed in parallel.",
    "inputs": ["implementation-chunk-b"],
    "outputs": ["Chunk B implemented."],
    "evidence_refs": ["subagent:impl-b"],
    "result": "passed"
  },
  {
    "id": "impl-c",
    "phase": 1,
    "status": "completed",
    "track": "implementation-chunk",
    "owner": "dvandva-implementer",
    "owner_role": "prativadi",
    "parallelized": true,
    "rationale": "Prativadi implementation chunk completed in parallel.",
    "inputs": ["implementation-chunk-c"],
    "outputs": ["Chunk C implemented."],
    "evidence_refs": ["subagent:impl-c"],
    "result": "passed"
  },
  {
    "id": "impl-d",
    "phase": 1,
    "status": "completed",
    "track": "implementation-chunk",
    "owner": "dvandva-implementer",
    "owner_role": "prativadi",
    "parallelized": true,
    "rationale": "Prativadi implementation chunk completed in parallel.",
    "inputs": ["implementation-chunk-d"],
    "outputs": ["Chunk D implemented."],
    "evidence_refs": ["subagent:impl-d"],
    "result": "passed"
  },
  {
    "id": "impl-e",
    "phase": 1,
    "status": "completed",
    "track": "implementation-chunk",
    "owner": "dvandva-implementer",
    "owner_role": "vadi",
    "parallelized": true,
    "rationale": "Vadi integration chunk completed in parallel.",
    "inputs": ["implementation-chunk-e"],
    "outputs": ["Chunk E implemented."],
    "evidence_refs": ["subagent:impl-e"],
    "result": "passed"
  }
]
JQ
}

v2_test_creation_track_filter() {
  cat <<'JQ'
.subagent_tracks += [
  {
    "id": "test-creation-evidence",
    "phase": "test_creation",
    "status": "completed",
    "track": "test-creation",
    "owner": "dvandva-test-creator",
    "owner_role": "vadi",
    "parallelized": false,
    "rationale": "Vadi test_creation recorded coverage evidence before cross-review.",
    "inputs": ["implementation evidence"],
    "outputs": ["Motivating tests and coverage evidence recorded."],
    "evidence_refs": ["bash scripts/test-dvandva-write.sh PASS"],
    "result": "passed"
  }
]
JQ
}

v2_cross_review_tracks_filter() {
  cat <<'JQ'
.subagent_tracks += [
  {
    "id": "cross-vadi",
    "phase": "cross_review",
    "status": "completed",
    "track": "cross-review",
    "owner": "dvandva-cross-reviewer",
    "owner_role": "vadi",
    "parallelized": true,
    "rationale": "Vadi cross-reviewed prativadi-owned chunks.",
    "inputs": ["implementation-chunk-c", "implementation-chunk-d"],
    "outputs": ["Peer chunks accepted."],
    "evidence_refs": ["subagent:cross-vadi"],
    "result": "approved"
  },
  {
    "id": "cross-prativadi",
    "phase": "cross_review",
    "status": "completed",
    "track": "cross-review",
    "owner": "dvandva-cross-reviewer",
    "owner_role": "prativadi",
    "parallelized": true,
    "rationale": "Prativadi cross-reviewed vadi-owned chunks.",
    "inputs": ["implementation-chunk-a", "implementation-chunk-b", "implementation-chunk-e"],
    "outputs": ["Peer chunks accepted."],
    "evidence_refs": ["subagent:cross-prativadi"],
    "result": "approved"
  }
]
JQ
}

v2_cross_review_finding_filter() {
  cat <<'JQ'
.subagent_tracks += [
  {
    "id": "cross-prativadi-finding",
    "phase": "cross_review",
    "status": "completed",
    "track": "cross-review",
    "owner": "dvandva-cross-reviewer",
    "owner_role": "prativadi",
    "parallelized": true,
    "rationale": "Prativadi cross-reviewed vadi-owned chunks and found fix-required evidence.",
    "inputs": ["implementation-chunk-a"],
    "outputs": ["changes-requested: vadi-owned chunk needs a fix."],
    "evidence_refs": ["subagent:cross-prativadi-finding"],
    "result": "changes-requested"
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

BOX="$(new_box v2-missing-active-roles)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.active_roles)'
run_case_contains "v2 missing active_roles exits 23" 23 "DVANDVA_WRITE missing_key key=active_roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-missing-agent-instances)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.agent_instances)'
run_case_contains "v2 missing agent_instances exits 23" 23 "DVANDVA_WRITE missing_key key=agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-non-array-agent-instances)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.agent_instances = {}'
run_case_contains "v2 non-array agent_instances exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-bad-active-roles)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.active_roles = ["vadi", "vadi"]'
run_case_contains "v2 duplicate active_roles exits 23" 23 "DVANDVA_WRITE bad_active_roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-empty-subagent-tracks)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.subagent_tracks = []'
run_case_contains "v2 empty subagent_tracks exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-malformed-subagent-tracks)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 'del(.subagent_tracks[0].owner)'
run_case_contains "v2 malformed subagent_tracks exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-null-subagent-track-phase)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.subagent_tracks[0].phase = null'
run_case_contains "v2 null subagent track phase exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-fake-parallel-subagent-track)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  '.subagent_tracks[0].parallelized = true' \
  '.subagent_tracks[0].owner = "vadi"' \
  '.subagent_tracks[0].outputs = []' \
  '.subagent_tracks[0].evidence_refs = []'
run_case_contains "v2 fake parallel subagent track exits 23" 23 "DVANDVA_WRITE bad_subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-standalone-parallel-subagent-owner)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  '.subagent_tracks[0].parallelized = true' \
  '.subagent_tracks[0].owner = "adversarial-analyst"' \
  '.subagent_tracks[0].outputs = ["Independent review completed."]' \
  '.subagent_tracks[0].evidence_refs = ["subagent:adversarial-analyst"]'
run_case "v2 standalone parallel subagent owner is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-bundled-adversarial-parallel-subagent-owner)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  '.subagent_tracks[0].parallelized = true' \
  '.subagent_tracks[0].owner = "dvandva-adversarial-analyst"' \
  '.subagent_tracks[0].outputs = ["Bundled adversarial review completed."]' \
  '.subagent_tracks[0].evidence_refs = ["subagent:dvandva-adversarial-analyst"]'
run_case "v2 bundled adversarial analyst parallel owner is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-dynamic-owner-missing-agent-instance)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_parallel_track_filter)" \
  '.agent_instances = []'
run_case_contains "v2 dynamic owner requires agent_instance" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-dynamic-owner-missing-closure)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  "$(v2_dynamic_parallel_track_filter)" \
  '.agent_instances[0].evidence_refs = ["subagent:r3-generated-dynamic-review"]' \
  '.agent_instances[0].closed_at = null'
run_case_contains "v2 dynamic owner requires closure evidence" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-dynamic-owner-accepted)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  "$(v2_dynamic_parallel_track_filter)"
run_case "v2 dynamic owner with closed agent_instance is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-parent-role)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].parent_role = "team"'
run_case_contains "v2 agent_instance bad parent_role exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-blank-spawned-by)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].spawned_by = "   "'
run_case_contains "v2 agent_instance blank spawned_by exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-spawn-checkpoint)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].spawned_at_checkpoint = "0"'
run_case_contains "v2 agent_instance bad spawned checkpoint exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-empty-phase)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].phase = ""'
run_case_contains "v2 agent_instance empty phase exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-blank-purpose)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].purpose = "   "'
run_case_contains "v2 agent_instance blank purpose exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-wrong-kind)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].agent_kind = "static"'
run_case_contains "v2 agent_instance wrong kind exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-status)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].status = "done"'
run_case_contains "v2 agent_instance bad status exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-unsafe-read-path)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].read_paths = ["/absolute"]'
run_case_contains "v2 agent_instance unsafe read path exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-depends-on)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].depends_on = "r3-other"'
run_case_contains "v2 agent_instance bad depends_on exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-output-refs)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].output_refs = "subagent_track:r3-generated-dynamic-review"'
run_case_contains "v2 agent_instance bad output_refs exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-bad-base-checkpoint)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].base_checkpoint = "0"'
run_case_contains "v2 agent_instance bad base_checkpoint exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-closed-missing-result)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].result = ""'
run_case_contains "v2 closed agent_instance missing result exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-closed-empty-work-items)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].work_item_ids = []'
run_case_contains "v2 closed agent_instance empty work_item_ids exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-dynamic-owner-missing-output-refs)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  "$(v2_dynamic_parallel_track_filter)" \
  '.agent_instances[0].output_refs = []'
run_case_contains "v2 dynamic owner requires output_refs" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-duplicate-agent-instance-id)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances += [.agent_instances[0]]'
run_case_contains "v2 duplicate agent_instance ids exit 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-unsafe-agent-instance-id)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].id = "../escape"'
run_case_contains "v2 unsafe agent_instance id exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-bad-agent-instance-permission)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].permission_class = "full-write"'
run_case_contains "v2 bad agent_instance permission exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-bad-agent-instance-model)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].model_class = "haiku"'
run_case_contains "v2 bad agent_instance model exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-write-path-collision)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case_contains "v2 agent_instance write path collision exits 23" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-unsafe-write-path)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["../escape"]'
run_case_contains "v2 agent_instance unsafe write path exits 23" 23 "DVANDVA_WRITE bad_agent_instances" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-write-path-prefix-collision)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["src/a"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .write_paths = ["src/a/b"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case_contains "v2 agent_instance write path prefix collision exits 23" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-sibling-prefix-paths)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["src/a"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .write_paths = ["src/ab"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case "v2 agent_instance sibling prefix paths are accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-serialized-conflict)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .depends_on = ["r3-generated-dynamic-review"] | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case "v2 serialized agent_instance conflict is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

for owner in \
  dvandva-security-auditor \
  dvandva-integration-checker \
  dvandva-debugger \
  dvandva-doc-verifier \
  dvandva-pattern-mapper; do
  BOX="$(new_box "v2-${owner}-parallel-subagent-owner")"
  make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
    '.subagent_tracks[0].parallelized = true' \
    ".subagent_tracks[0].owner = \"$owner\"" \
    ".subagent_tracks[0].outputs = [\"New bundled owner accepted: $owner\"]" \
    ".subagent_tracks[0].evidence_refs = [\"subagent:$owner\"]"
  run_case "v2 new bundled owner $owner is accepted" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

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

V2_EDGES="research_drafting:research_review research_review:research_revision research_revision:research_review research_review:spec_drafting spec_drafting:spec_review spec_review:spec_revision spec_review:parallel_implementing spec_revision:spec_review parallel_implementing:test_creation test_creation:cross_review cross_review:cross_fixing cross_fixing:test_creation cross_review:deep_review deep_review:phase_fixing deep_review:deslop phase_fixing:test_creation deslop:phase_fixing deslop:parallel_implementing deslop:done"
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
  if [[ "$new" == "parallel_implementing" ]]; then
    extras+=("$(v2_parallel_chunks_filter)")
  fi
  if [[ "$new" == "cross_review" || "$new" == "cross_fixing" ]]; then
    extras+=('.active_roles = ["vadi", "prativadi"]')
  fi
  if [[ "$edge" == "test_creation:cross_review" ]]; then
    extras+=("$(v2_test_creation_track_filter)")
  fi
  if [[ "$edge" == "cross_review:cross_fixing" ]]; then
    extras+=("$(v2_cross_review_finding_filter)")
  fi
  if [[ "$edge" == "parallel_implementing:test_creation" ]]; then
    extras+=("$(v2_parallel_chunks_filter)")
    extras+=('.active_roles = []')
    extras+=("$(v2_implementation_tracks_filter)")
  fi
  if [[ "$edge" == "cross_review:deep_review" ]]; then
    extras+=("$(v2_cross_review_tracks_filter)")
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

BOX="$(new_box v2-wrong-owner-parallel-implementing)"
make_baton_v2 "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "vadi" 5 "$(v2_parallel_chunks_filter)"
run_case_contains "v2 parallel_implementing requires team assignee" 23 "bad_assignee_owner" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-parallel-missing-prativadi-role)"
make_baton_v2 "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi"]'
run_case_contains "v2 parallel_implementing requires both active roles" 23 "bad_active_roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-parallel-missing-work-split)"
make_baton_v2 "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 '.active_roles = ["vadi", "prativadi"]'
run_case_contains "v2 parallel_implementing requires two-team chunks" 23 "bad_parallel_work_split" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-parallel-empty-path-chunks)"
make_baton_v2 "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.work_split |= map(if (.chunk_type // .type // "") == "implementation" then .paths = [] else . end)'
run_case_contains "v2 parallel_implementing rejects empty-path chunks" 23 "bad_parallel_work_split" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-impl-review)"
make_baton_v2 "$BOX/baton.json" "implementing" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "phase_review" "prativadi" 5
run_case_contains "v2 implementing->phase_review rejects legacy direct review" 24 "no legal edge implementing->phase_review" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-spec-review-implementing)"
make_baton_v2 "$BOX/baton.json" "spec_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "implementing" "vadi" 5
run_case_contains "v2 spec_review->implementing rejects sequential implementation" 24 "no legal edge spec_review->implementing" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-test-creation-deep-review)"
make_baton_v2 "$BOX/baton.json" "test_creation" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "deep_review" "prativadi" 5
run_case_contains "v2 test_creation->deep_review requires cross_review first" 24 "no legal edge test_creation->deep_review" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-test-creation-cross-review-missing-test-evidence)"
make_baton_v2 "$BOX/baton.json" "test_creation" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 '.active_roles = ["vadi", "prativadi"]'
run_case_contains "v2 test_creation->cross_review rejects missing test evidence" 24 "completed test-creation subagent_track" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-parallel-test-creation-missing-impl-evidence)"
make_baton_v2 "$BOX/baton.json" "parallel_implementing" "team" 4 \
  "$(v2_parallel_chunks_filter)"
make_baton_v2 "$BOX/baton.next.json" "test_creation" "vadi" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = []'
run_case_contains "v2 parallel_implementing->test_creation rejects missing implementation evidence" 24 "completed implementation-chunk subagent_tracks for both roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-parallel-test-creation-single-role-evidence)"
make_baton_v2 "$BOX/baton.json" "parallel_implementing" "team" 4 \
  "$(v2_parallel_chunks_filter)"
make_baton_v2 "$BOX/baton.next.json" "test_creation" "vadi" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = []' \
  "$(v2_implementation_tracks_filter)" \
  '.subagent_tracks |= map(if .track == "implementation-chunk" then .owner_role = "vadi" else . end)'
run_case_contains "v2 parallel_implementing->test_creation requires both implementation roles" 24 "completed implementation-chunk subagent_tracks for both roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-cross-review-deep-review-missing-evidence)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "deep_review" "prativadi" 5
run_case_contains "v2 cross_review->deep_review rejects missing cross-review evidence" 24 "completed cross-review subagent_tracks for both roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-cross-review-cross-fixing-missing-evidence)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 '.active_roles = ["vadi", "prativadi"]'
run_case_contains "v2 cross_review->cross_fixing rejects missing cross-review evidence" 24 "completed cross-review subagent_tracks" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-cross-review-deep-review-single-role-evidence)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "deep_review" "prativadi" 5 \
  "$(v2_cross_review_tracks_filter)" \
  '.subagent_tracks |= map(if .track == "cross-review" then .owner_role = "vadi" else . end)'
run_case_contains "v2 cross_review->deep_review requires both cross-review roles" 24 "completed cross-review subagent_tracks for both roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-cross-review-deep-review-numeric-phase-evidence)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "deep_review" "prativadi" 5 \
  "$(v2_cross_review_tracks_filter)" \
  '.subagent_tracks |= map(if .track == "cross-review" then .phase = 1 else . end)'
run_case_contains "v2 cross_review tracks must use status-name phase" 24 "completed cross-review subagent_tracks for both roles" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-phase-review-done)"
make_baton_v2 "$BOX/baton.json" "phase_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"'
run_case_contains "v2 phase_review->done rejects legacy terminal review" 24 "no legal edge phase_review->done" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-review-of-review-done)"
make_baton_v2 "$BOX/baton.json" "review_of_review" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"'
run_case_contains "v2 review_of_review->done rejects legacy terminal review" 24 "no legal edge review_of_review->done" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-counter-review-done)"
make_baton_v2 "$BOX/baton.json" "counter_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"'
run_case_contains "v2 counter_review->done rejects legacy terminal review" 24 "no legal edge counter_review->done" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-team-cross-fixing-sync)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: prativadi protocol slice complete; vadi owns agent-roster slice."' \
  '.next_action = "Vadi: complete agent-roster slice; prativadi is polling for next checkpoint."'
run_case "v2 team cross_fixing accepts same-status sync checkpoint" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-team-cross-review-sync-phase-mutation)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  '.phase = 2' \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: attempted phase mutation."' \
  '.next_action = "Team: this must be rejected because sync checkpoints cannot advance phases."'
run_case_contains "v2 team same-status sync cannot change phase" 24 "same-status team sync cannot change phase" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-team-cross-review-sync-whitespace-summary)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "   \t  "' \
  '.next_action = "Team: valid next action."'
run_case_contains "v2 team sync rejects whitespace summary" 24 "same-status team sync requires team assignee, both active_roles, summary, and next_action" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-team-cross-review-sync-whitespace-next-action)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: valid summary."' \
  '.next_action = "   \t  "'
run_case_contains "v2 team sync rejects whitespace next_action" 24 "same-status team sync requires team assignee, both active_roles, summary, and next_action" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-non-team-same-status-still-rejected)"
make_baton_v2 "$BOX/baton.json" "test_creation" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "test_creation" "vadi" 5
run_case_contains "v2 non-team same-status rewrite still rejects" 24 "same-status rewrite" \
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
