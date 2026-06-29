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
    human_question|human_decision)
      echo "human"
      ;;
    done)
      echo "team"
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
| .subagent_tracks[0].owner_role = "vadi"
| .subagent_tracks[0].outputs = ["Generated dynamic review completed."]
| .subagent_tracks[0].evidence_refs = ["subagent:r3-generated-dynamic-review", "closed:r3-generated-dynamic-review"]
JQ
}

v2_many_agent_instances_filter() {
  cat <<'JQ'
.agent_instances = [
  {
    "id": "r3-gen-0",
    "parent_role": "vadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Collapsed generated instance included to exercise large dynamic registries.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "collapsed",
    "work_item_ids": [],
    "read_paths": ["src/gen-0"],
    "write_paths": [],
    "depends_on": [],
    "conflict_group": "many-0",
    "base_checkpoint": 0,
    "output_refs": [],
    "evidence_refs": [],
    "result": "collapsed"
  },
  {
    "id": "r3-gen-1",
    "parent_role": "vadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Closed generated instance 1 for large dynamic registry coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "closed",
    "work_item_ids": ["chunk-1"],
    "read_paths": ["src/gen-1"],
    "write_paths": ["src/gen-1"],
    "depends_on": [],
    "conflict_group": "many-1",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-gen-1"],
    "evidence_refs": ["subagent:r3-gen-1", "closed:r3-gen-1"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  },
  {
    "id": "r3-gen-2",
    "parent_role": "prativadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Closed generated instance 2 for large dynamic registry coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "closed",
    "work_item_ids": ["chunk-2"],
    "read_paths": ["src/gen-2"],
    "write_paths": ["src/gen-2"],
    "depends_on": [],
    "conflict_group": "many-2",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-gen-2"],
    "evidence_refs": ["subagent:r3-gen-2", "closed:r3-gen-2"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  },
  {
    "id": "r3-gen-3",
    "parent_role": "vadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Closed generated instance 3 for large dynamic registry coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "closed",
    "work_item_ids": ["chunk-3"],
    "read_paths": ["src/gen-3"],
    "write_paths": ["src/gen-3"],
    "depends_on": [],
    "conflict_group": "many-3",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-gen-3"],
    "evidence_refs": ["subagent:r3-gen-3", "closed:r3-gen-3"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  },
  {
    "id": "r3-gen-4",
    "parent_role": "prativadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Closed generated instance 4 for large dynamic registry coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "closed",
    "work_item_ids": ["chunk-4"],
    "read_paths": ["src/gen-4"],
    "write_paths": ["src/gen-4"],
    "depends_on": [],
    "conflict_group": "many-4",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-gen-4"],
    "evidence_refs": ["subagent:r3-gen-4", "closed:r3-gen-4"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  },
  {
    "id": "r3-gen-5",
    "parent_role": "vadi",
    "spawned_by": "dvandva-implementer",
    "spawned_at_checkpoint": 0,
    "phase": 1,
    "purpose": "Closed generated instance 5 for large dynamic registry coverage.",
    "agent_kind": "generated",
    "seed_agent": "dvandva-implementer",
    "model_class": "sonnet-class|gpt-5.4",
    "permission_class": "edit-scoped",
    "status": "closed",
    "work_item_ids": ["chunk-5"],
    "read_paths": ["src/gen-5"],
    "write_paths": ["src/gen-5"],
    "depends_on": [],
    "conflict_group": "many-5",
    "base_checkpoint": 0,
    "output_refs": ["subagent_track:r3-gen-5"],
    "evidence_refs": ["subagent:r3-gen-5", "closed:r3-gen-5"],
    "closed_at": "2026-06-28T00:00:00Z",
    "result": "passed"
  }
]
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

v2_cross_fixing_chunks_filter() {
  cat <<'JQ'
.work_split = [
  {
    "id": "cross-fixing-a",
    "phase": "1",
    "chunk_type": "cross_fixing",
    "owner": "vadi",
    "owner_role": "vadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Vadi-owned cross-fixing chunk A.",
    "paths": ["src/fix/a.ts"],
    "can_parallelize": true,
    "parallel_rationale": "Independent fix slice.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "cross-fixing-b",
    "phase": "1",
    "chunk_type": "cross_fixing",
    "owner": "prativadi",
    "owner_role": "prativadi",
    "suggested_agent": "dvandva-implementer",
    "scope": "Prativadi-owned cross-fixing chunk B.",
    "paths": ["src/fix/b.ts"],
    "can_parallelize": true,
    "parallel_rationale": "Independent fix slice.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  }
]
JQ
}

v2_cross_review_chunks_filter() {
  cat <<'JQ'
.work_split = [
  {
    "id": "cross-review-a",
    "phase": "1",
    "chunk_type": "cross_review",
    "owner": "vadi",
    "owner_role": "vadi",
    "suggested_agent": "dvandva-cross-reviewer",
    "scope": "Vadi cross-reviews prativadi-owned code.",
    "paths": ["src/shared-review.ts"],
    "can_parallelize": true,
    "parallel_rationale": "Cross-review is read-only by default.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
  },
  {
    "id": "cross-review-b",
    "phase": "1",
    "chunk_type": "cross_review",
    "owner": "prativadi",
    "owner_role": "prativadi",
    "suggested_agent": "dvandva-cross-reviewer",
    "scope": "Prativadi cross-reviews vadi-owned code.",
    "paths": ["src/shared-review.ts"],
    "can_parallelize": true,
    "parallel_rationale": "Cross-review is read-only by default.",
    "depends_on": [],
    "status": "planned",
    "artifact_refs": []
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
make_baton_v2 "$ALPHA_DIR/baton.next.json" "research_drafting" "vadi" 0 '.branch = "alpha-branch" | .run_id = "alpha"'
make_baton_v2 "$BETA_DIR/baton.next.json" "research_drafting" "vadi" 0 '.branch = "beta-branch" | .run_id = "beta"'
run_case "run alpha v2 scaffold writes only alpha baton" 0 \
  "$SCRIPT" "$ALPHA_DIR/baton.json" "$ALPHA_DIR/baton.next.json"
run_case "run beta v2 scaffold writes only beta baton" 0 \
  "$SCRIPT" "$BETA_DIR/baton.json" "$BETA_DIR/baton.next.json"
if [[ -f "$ALPHA_DIR/baton.json" && -f "$ALPHA_DIR/history/0-research_drafting-vadi.json" \
  && -f "$BETA_DIR/baton.json" && -f "$BETA_DIR/history/0-research_drafting-vadi.json" \
  && ! -e "$RUNS_BOX/.dvandva/history" ]]; then
  echo "PASS: two named runs keep batons and histories isolated"
else
  echo "FAIL: two named runs collided or wrote shared history"
  failures=$((failures + 1))
fi

BOX="$(new_box legacy-v1-dot-dvandva)"
mkdir -p "$BOX/.dvandva"
make_baton "$BOX/.dvandva/baton.next.json" "spec_drafting" "vadi" 0
run_case "legacy .dvandva/baton.json v1 scaffold remains allowed" 0 \
  "$SCRIPT" "$BOX/.dvandva/baton.json" "$BOX/.dvandva/baton.next.json"

BOX="$(new_box named-run-v1-schema)"
mkdir -p "$BOX/.dvandva/runs/alpha"
make_baton "$BOX/.dvandva/runs/alpha/baton.next.json" "spec_drafting" "vadi" 0
run_case_contains "named run v1 scaffold exits 23" 23 "DVANDVA_WRITE bad_run_id_dir" \
  "$SCRIPT" "$BOX/.dvandva/runs/alpha/baton.json" "$BOX/.dvandva/runs/alpha/baton.next.json"

BOX="$(new_box named-run-v2-run-id-mismatch)"
mkdir -p "$BOX/.dvandva/runs/alpha"
make_baton_v2 "$BOX/.dvandva/runs/alpha/baton.next.json" "research_drafting" "vadi" 0 '.run_id = "beta"'
run_case_contains "named run v2 run_id mismatch exits 23" 23 "DVANDVA_WRITE bad_run_id_dir" \
  "$SCRIPT" "$BOX/.dvandva/runs/alpha/baton.json" "$BOX/.dvandva/runs/alpha/baton.next.json"

BOX="$(new_box named-run-v2-run-id-null)"
mkdir -p "$BOX/.dvandva/runs/alpha"
make_baton_v2 "$BOX/.dvandva/runs/alpha/baton.next.json" "research_drafting" "vadi" 0 '.run_id = null'
run_case_contains "named run v2 null run_id exits 23" 23 "DVANDVA_WRITE bad_run_id_dir" \
  "$SCRIPT" "$BOX/.dvandva/runs/alpha/baton.json" "$BOX/.dvandva/runs/alpha/baton.next.json"

BOX="$(new_box named-run-v2-run-id-missing)"
mkdir -p "$BOX/.dvandva/runs/alpha"
make_baton_v2 "$BOX/.dvandva/runs/alpha/baton.next.json" "research_drafting" "vadi" 0 'del(.run_id)'
run_case_contains "named run v2 missing run_id exits 23" 23 "DVANDVA_WRITE bad_run_id_dir" \
  "$SCRIPT" "$BOX/.dvandva/runs/alpha/baton.json" "$BOX/.dvandva/runs/alpha/baton.next.json"

BOX="$(new_box named-run-v2-run-id-empty)"
mkdir -p "$BOX/.dvandva/runs/alpha"
make_baton_v2 "$BOX/.dvandva/runs/alpha/baton.next.json" "research_drafting" "vadi" 0 '.run_id = ""'
run_case_contains "named run v2 empty run_id exits 23" 23 "DVANDVA_WRITE bad_run_id_dir" \
  "$SCRIPT" "$BOX/.dvandva/runs/alpha/baton.json" "$BOX/.dvandva/runs/alpha/baton.next.json"

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

BOX="$(new_box v2-unsafe-work-split-path)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 '.work_split[0].paths = ["../escape"]'
run_case_contains "v2 unsafe work_split path exits 23" 23 "DVANDVA_WRITE bad_work_split" \
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

BOX="$(new_box v2-nonparallel-dynamic-owner-missing-agent-instance)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_parallel_track_filter)" \
  '.subagent_tracks[0].parallelized = false' \
  '.agent_instances = []'
run_case_contains "v2 nonparallel dynamic owner requires agent_instance" 23 "DVANDVA_WRITE bad_agent_instances" \
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

BOX="$(new_box v2-nonparallel-dynamic-owner-accepted)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  "$(v2_dynamic_parallel_track_filter)" \
  '.subagent_tracks[0].parallelized = false'
run_case "v2 nonparallel dynamic owner with closed agent_instance is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-dynamic-owner-parent-role-mismatch)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  "$(v2_dynamic_parallel_track_filter)" \
  '.subagent_tracks[0].owner_role = "prativadi"'
run_case_contains "v2 dynamic owner_role must match parent_role" 23 "DVANDVA_WRITE bad_agent_instances" \
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

for reserved_id in \
  dvandva-implementer \
  adversarial-analyst \
  vadi; do
  BOX="$(new_box "v2-reserved-agent-instance-id-$reserved_id")"
  make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
    "$(v2_dynamic_agent_instances_filter)" \
    ".agent_instances[0].id = \"$reserved_id\""
  run_case_contains "v2 generated agent_instance rejects reserved id $reserved_id" 23 "DVANDVA_WRITE bad_agent_instances" \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

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
  '.agent_instances[0].status = "running"' \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .status = "running" | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
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
  '.agent_instances[0].status = "running"' \
  '.agent_instances[0].write_paths = ["src/a"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .status = "running" | .write_paths = ["src/a/b"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case_contains "v2 agent_instance write path prefix collision exits 23" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-sibling-prefix-paths)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].status = "running"' \
  '.agent_instances[0].write_paths = ["src/a"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .status = "running" | .write_paths = ["src/ab"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case "v2 agent_instance sibling prefix paths are accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-six-agent-instances-accepted)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_many_agent_instances_filter)"
run_case "v2 six generated agent_instances with collapsed mix are accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-six-agent-instances-late-collision)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_many_agent_instances_filter)" \
  '.agent_instances[4].status = "running"' \
  '.agent_instances[5].status = "running"' \
  '.agent_instances[4].write_paths = ["src/late"]' \
  '.agent_instances[5].write_paths = ["src/late/sub"]'
run_case_contains "v2 six generated agent_instances catch late path collision" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-closed-agent-instances-same-base-collision)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case_contains "v2 closed agent_instances sharing base checkpoint still collide" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-running-agent-instances-prior-base-collision)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].status = "running"' \
  '.agent_instances[0].base_checkpoint = 5' \
  '.agent_instances[0].spawned_at_checkpoint = 5' \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances[0].evidence_refs = ["subagent:r3-generated-dynamic-review"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .status = "running" | .base_checkpoint = 12 | .spawned_at_checkpoint = 12 | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case_contains "v2 running historical agent_instances sharing write paths still collide" 23 "DVANDVA_WRITE bad_agent_instances_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-closed-agent-instances-prior-base-reuse-paths)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].base_checkpoint = 5' \
  '.agent_instances[0].spawned_at_checkpoint = 5' \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .base_checkpoint = 12 | .spawned_at_checkpoint = 12 | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b", "closed:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case "v2 closed historical agent_instances may reuse write paths" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-agent-instance-serialized-conflict)"
make_baton_v2 "$BOX/baton.next.json" "research_drafting" "vadi" 0 \
  "$(v2_dynamic_agent_instances_filter)" \
  '.agent_instances[0].status = "running"' \
  '.agent_instances[0].write_paths = ["scripts/test-dvandva-write.sh"]' \
  '.agent_instances += [(.agent_instances[0] | .id = "r3-generated-dynamic-review-b" | .status = "running" | .depends_on = ["r3-generated-dynamic-review"] | .write_paths = ["scripts/test-dvandva-write.sh"] | .evidence_refs = ["subagent:r3-generated-dynamic-review-b"] | .output_refs = ["subagent_track:r3-generated-dynamic-review-b"])]'
run_case "v2 serialized agent_instance conflict is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-bare-path-collision)"
make_baton_v2 "$BOX/baton.json" "parallel_implementing" "team" 4 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: candidate introduces a parallel implementation collision."' \
  '.next_action = "Team: reject overlapping write intent before continuing."' \
  '.work_split |= map(
    if .id == "implementation-chunk-a" then .paths = ["src/shared"]
    elif .id == "implementation-chunk-b" then .paths = ["src/shared"]
    else .
    end
  )'
run_case_contains "v2 parallel work_split bare path collision exits 23" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-default-implementation-collision)"
make_baton_v2 "$BOX/baton.json" "parallel_implementing" "team" 4 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: candidate uses default implementation chunks with colliding paths."' \
  '.next_action = "Team: reject missing chunk_type chunks as implementation write intent."' \
  '.work_split |= map(
    if .id == "implementation-chunk-a" then del(.chunk_type) | .paths = ["src/default-impl.ts"]
    elif .id == "implementation-chunk-b" then del(.chunk_type) | .paths = ["src/default-impl.ts"]
    else .
    end
  )'
run_case_contains "v2 default implementation chunks collide on bare paths" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-prefix-collision)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: candidate introduces an ancestor-descendant write collision."' \
  '.next_action = "Team: reject prefix-overlapping fix chunks."' \
  '.work_split[0].paths = ["src/tree"]' \
  '.work_split[1].paths = ["src/tree/child"]'
run_case_contains "v2 work_split prefix collision exits 23" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-empty-write-paths-cannot-mask-paths)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: empty write_paths must not mask write-capable paths."' \
  '.next_action = "Team: reject colliding paths even when one chunk declares empty write_paths."' \
  '.work_split[0].paths = ["src/masked.ts"]' \
  '.work_split[0].write_paths = []' \
  '.work_split[1].paths = ["src/masked.ts"]'
run_case_contains "v2 work_split empty write_paths cannot mask paths collision" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-sibling-prefix-accepted)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: sibling prefixes should remain disjoint."' \
  '.next_action = "Team: continue with non-overlapping sibling write paths."' \
  '.work_split[0].paths = ["src/a"]' \
  '.work_split[1].paths = ["src/ab"]'
run_case "v2 work_split sibling prefix paths are accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-serialized-conflict)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: serialized write conflict is intentional."' \
  '.next_action = "Team: allow the dependent fix chunk to reuse the path after its dependency."' \
  '.work_split[0].paths = ["src/shared-fix.ts"]' \
  '.work_split[1].paths = ["src/shared-fix.ts"]' \
  '.work_split[0].conflict_group = "fix-shared"' \
  '.work_split[1].conflict_group = "fix-shared"' \
  '.work_split[1].depends_on = ["cross-fixing-a"]'
run_case "v2 serialized work_split conflict is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-conflict-group-without-depends-on)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: conflict_group alone must not serialize writers."' \
  '.next_action = "Team: reject overlapping write chunks without an explicit dependency edge."' \
  '.work_split[0].paths = ["src/shared-fix.ts"]' \
  '.work_split[1].paths = ["src/shared-fix.ts"]' \
  '.work_split[0].conflict_group = "fix-shared"' \
  '.work_split[1].conflict_group = "fix-shared"'
run_case_contains "v2 work_split conflict_group without depends_on rejects" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-depends-on-without-conflict-group)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: depends_on alone must not serialize writers."' \
  '.next_action = "Team: reject overlapping write chunks without a shared conflict group."' \
  '.work_split[0].paths = ["src/shared-fix.ts"]' \
  '.work_split[1].paths = ["src/shared-fix.ts"]' \
  '.work_split[1].depends_on = ["cross-fixing-a"]'
run_case_contains "v2 work_split depends_on without conflict_group rejects" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-cross-review-read-overlap)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: cross-review overlaps are read-only by default."' \
  '.next_action = "Team: continue with read-only cross-review coverage."' \
  '.work_split[0].paths = ["src/shared-review.ts"]' \
  '.work_split[1].paths = ["src/shared-review.ts"]'
run_case "v2 cross_review overlapping read paths are accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-cross-review-explicit-write-collision)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: explicit write_paths should make cross-review collisions fail."' \
  '.next_action = "Team: reject cross-review write collisions unless serialized."' \
  '.work_split[0].write_paths = ["src/shared-review.ts"]' \
  '.work_split[1].write_paths = ["src/shared-review.ts"]'
run_case_contains "v2 cross_review explicit write_paths collision rejects" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-team-sync-work-split-collision)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.work_split = [.work_split[0]]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: candidate adds a new colliding live fix chunk."' \
  '.next_action = "Team: reject the sync because it introduces overlapping write ownership."' \
  '.work_split[0].paths = ["src/live.ts"]' \
  '.work_split[1].paths = ["src/live.ts"]'
run_case_contains "v2 team sync rejects newly introduced live work_split collision" 23 "DVANDVA_WRITE bad_work_split_write_paths" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-terminal-reuse)"
make_baton_v2 "$BOX/baton.json" "cross_fixing" "team" 4 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_fixing" "team" 5 \
  "$(v2_cross_fixing_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: terminal chunks should not block later path reuse."' \
  '.next_action = "Team: continue because the live fix chunk is reusing a completed path."' \
  '.work_split[0].paths = ["src/reuse.ts"]' \
  '.work_split[0].status = "completed"' \
  '.work_split[1].paths = ["src/reuse.ts"]' \
  '.work_split[1].status = "planned"'
run_case "v2 terminal-aware work_split path reuse is accepted" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-work-split-empty-explicit-write-paths)"
make_baton_v2 "$BOX/baton.json" "parallel_implementing" "team" 4 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "parallel_implementing" "team" 5 \
  "$(v2_parallel_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Team sync: implementation paths still carry write intent with empty write_paths."' \
  '.next_action = "Team: continue because write_paths does not narrow paths for write-capable chunks."' \
  '.work_split |= map(if .id == "implementation-chunk-a" then .write_paths = [] else . end)'
run_case "v2 implementation chunk with explicit empty write_paths keeps paths write intent" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

# --- concurrent writers: lost-update / TOCTOU stale-write race guard ---------
# Two engines share one worktree and may invoke this helper at the same time.
# Without a lock both read on-disk checkpoint=N, both build a checkpoint=N+1
# candidate, both pass the (N+1 > N) guard, and both mv their candidate into
# place -- the later mv silently clobbers the earlier accepted write (a lost
# update). The fix serializes read-state -> validate -> mv -> snapshot so that
# exactly one writer installs N+1 and the loser, re-reading the now-advanced
# state, fails closed with exit 27 stale_checkpoint. Two concurrent valid
# same-status team-sync candidates at N+1 must therefore yield exactly one
# exit 0 and one exit 27 -- never two exit 0.
BOX="$(new_box v2-concurrent-write-race)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/cand-a.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Concurrent writer A team sync."' \
  '.next_action = "Team: continue after writer A wins the race."'
make_baton_v2 "$BOX/cand-b.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Concurrent writer B team sync."' \
  '.next_action = "Team: continue after writer B wins the race."'

"$SCRIPT" "$BOX/baton.json" "$BOX/cand-a.json" >/dev/null 2>&1 &
race_pid_a=$!
"$SCRIPT" "$BOX/baton.json" "$BOX/cand-b.json" >/dev/null 2>&1 &
race_pid_b=$!
wait "$race_pid_a"; race_rc_a=$?
wait "$race_pid_b"; race_rc_b=$?

race_zeros=0
race_staled=0
for race_rc in "$race_rc_a" "$race_rc_b"; do
  case "$race_rc" in
    0) race_zeros=$((race_zeros + 1)) ;;
    27) race_staled=$((race_staled + 1)) ;;
  esac
done

if [[ "$race_zeros" -eq 1 && "$race_staled" -eq 1 ]] \
  && jq -e '.checkpoint == 5 and .status == "cross_review"' "$BOX/baton.json" >/dev/null 2>&1; then
  echo "PASS: concurrent writers serialize (one 0, one 27, surviving baton at checkpoint 5)"
else
  echo "FAIL: concurrent writers raced (rc_a=$race_rc_a rc_b=$race_rc_b zeros=$race_zeros stale=$race_staled checkpoint=$(jq -r '.checkpoint // "?"' "$BOX/baton.json" 2>/dev/null))"
  failures=$((failures + 1))
fi

# --- PFX1 lock hardening: mandatory acquisition + fencing token --------------
# GAP 1 (fail-closed): a NON-DIRECTORY squatting the lock path
# ($BATON_DIR/.baton.lock.d) means mkdir can never acquire the lock. The old
# code fell through and ran the read->validate->mv critical section UNLOCKED,
# installing rc=0 and re-opening the write race. Lock acquisition is mandatory:
# refuse with exit 28 and leave the baton untouched.
BOX="$(new_box v2-lock-path-non-directory)"
RUN_DIR="$BOX/.dvandva/runs/alpha"
mkdir -p "$RUN_DIR"
make_baton_v2 "$RUN_DIR/baton.next.json" "research_drafting" "vadi" 0 \
  '.run_id = "alpha" | .branch = "alpha-branch"'
printf 'corrupt-non-directory\n' > "$RUN_DIR/.baton.lock.d"
run_case_contains "non-directory at lock path fails closed exit 28" 28 "DVANDVA_WRITE lock_unavailable" \
  "$SCRIPT" "$RUN_DIR/baton.json" "$RUN_DIR/baton.next.json"
if [[ ! -f "$RUN_DIR/baton.json" && -f "$RUN_DIR/.baton.lock.d" && ! -d "$RUN_DIR/.baton.lock.d" ]]; then
  echo "PASS: non-directory lock path installed no baton (critical section never ran unlocked)"
else
  echo "FAIL: non-directory lock path installed a baton unlocked or mutated the squatter"
  failures=$((failures + 1))
fi

# GAP 2 (single writer never self-fences): a normal uncontended write still owns
# its token at the pre-install check and must succeed rc=0.
BOX="$(new_box v2-fencing-single-writer-no-self-fence)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Single writer keeps its own fencing token."' \
  '.next_action = "Team: continue; the sole holder must not self-fence."'
run_case "single uncontended writer passes its own fencing check" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

# GAP 2 (stale-lock recovery): an old started_at with a foreign token and no live
# holder is a crashed writer's leftover. A fresh writer must steal it (replacing
# the token) and succeed rc=0. Fencing must not break legitimate recovery.
BOX="$(new_box v2-stale-lock-recovery)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/baton.next.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Stale-lock recovery: new writer steals an abandoned lock."' \
  '.next_action = "Team: continue after recovering the abandoned lock."'
mkdir -p "$BOX/.baton.lock.d"
printf '%s' 0 > "$BOX/.baton.lock.d/started_at"
printf '%s' "ghost-holder-token" > "$BOX/.baton.lock.d/owner"
run_case "abandoned stale lock is recovered and write succeeds" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
if jq -e '.checkpoint == 5 and .status == "cross_review"' "$BOX/baton.json" >/dev/null 2>&1; then
  echo "PASS: stale-lock recovery installed checkpoint 5"
else
  echo "FAIL: stale-lock recovery did not install checkpoint 5"
  failures=$((failures + 1))
fi

# GAP 2 (fencing / live-lock): writer A acquires the lock, reads checkpoint=4,
# judges its 5 candidate legal, then PARKS at the install barrier still holding
# the lock and still believing checkpoint=4. Writer B steals A's still-LIVE lock,
# installs 5, and exits 0. When A is released it must detect its fencing token is
# gone and abort fail-closed (exit 29) rather than clobber B's write. Net: exactly
# one checkpoint+1 install survives even though a live writer's lock was stolen.
# (DVANDVA_WRITE_BARRIER is a test-only seam that only touches/stats sentinel files;
# it is unset in production.)
#
# NOTE: the prior approach used DVANDVA_LOCK_TIMEOUT=0 to force an instant steal.
# That value is now correctly rejected as invalid (zero ≡ "steal everything immediately").
# Instead we: (a) let writer A acquire the lock normally (started_at = now), (b) wait
# until A arrives at the barrier, (c) backdate A's lock started_at to epoch 1 so the
# computed age is astronomically large, then (d) run writer B with DVANDVA_LOCK_TIMEOUT=1.
# Writer B sees age >> 1 and steals immediately on the first loop iteration — same
# deterministic outcome, no reliance on the now-prohibited zero value.
BOX="$(new_box v2-fencing-stolen-lock)"
make_baton_v2 "$BOX/baton.json" "cross_review" "team" 4 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]'
make_baton_v2 "$BOX/cand-a.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Fencing: slow writer A whose lock is stolen."' \
  '.next_action = "Team: A must abort after losing the lock."'
make_baton_v2 "$BOX/cand-b.json" "cross_review" "team" 5 \
  "$(v2_cross_review_chunks_filter)" \
  '.active_roles = ["vadi", "prativadi"]' \
  '.summary = "Fencing: peer writer B steals the lock and installs."' \
  '.next_action = "Team: B wins the stolen-lock race."'
fence_barrier="$BOX/barrierA"
rm -f "$fence_barrier.arrived" "$fence_barrier.release"
DVANDVA_WRITE_BARRIER="$fence_barrier" "$SCRIPT" "$BOX/baton.json" "$BOX/cand-a.json" >/dev/null 2>&1 &
fence_pid_a=$!
fence_waited=0
while [[ ! -e "$fence_barrier.arrived" && "$fence_waited" -lt 200 ]]; do
  sleep 0.05
  fence_waited=$((fence_waited + 1))
done
# Writer A is now parked at the barrier while holding the lock. Backdate the lock's
# started_at to epoch 1 so writer B computes a huge age (>> LOCK_TIMEOUT=1) and
# steals immediately on the first iteration without needing DVANDVA_LOCK_TIMEOUT=0.
printf '%s' "1" > "$BOX/.baton.lock.d/started_at"
DVANDVA_LOCK_TIMEOUT=1 "$SCRIPT" "$BOX/baton.json" "$BOX/cand-b.json" >/dev/null 2>&1
fence_rc_b=$?
: > "$fence_barrier.release"
wait "$fence_pid_a"; fence_rc_a=$?
fence_ckpt="$(jq -r '.checkpoint // "?"' "$BOX/baton.json" 2>/dev/null)"
fence_summary="$(jq -r '.summary // "?"' "$BOX/baton.json" 2>/dev/null)"
fence_zeros=0
[[ "$fence_rc_a" -eq 0 ]] && fence_zeros=$((fence_zeros + 1))
[[ "$fence_rc_b" -eq 0 ]] && fence_zeros=$((fence_zeros + 1))
if [[ -e "$fence_barrier.arrived" && "$fence_zeros" -eq 1 && "$fence_rc_a" -eq 29 \
  && "$fence_rc_b" -eq 0 && "$fence_ckpt" == "5" && "$fence_summary" == *"peer writer B"* ]]; then
  echo "PASS: fenced slow writer aborts (rc_a=29), peer install survives (exactly one checkpoint 5)"
else
  echo "FAIL: fencing failed (arrived=$([[ -e "$fence_barrier.arrived" ]] && echo y || echo n) rc_a=$fence_rc_a rc_b=$fence_rc_b zeros=$fence_zeros ckpt=$fence_ckpt summary=$fence_summary)"
  failures=$((failures + 1))
fi

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
  extras=()
  if [[ "$new" == "done" ]]; then
    extras+=('.vadi_final_approval = true')
    extras+=('.prativadi_final_approval = true')
  fi
  make_baton "$BOX/baton.json" "$cur" "vadi" 4
  make_baton "$BOX/baton.next.json" "$new" "prativadi" 5 "${extras[@]}"
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
    extras+=('.vadi_final_approval = true')
    extras+=('.prativadi_final_approval = true')
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
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
run_case_contains "v2 phase_review->done rejects legacy terminal review" 24 "no legal edge phase_review->done" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-review-of-review-done)"
make_baton_v2 "$BOX/baton.json" "review_of_review" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
run_case_contains "v2 review_of_review->done rejects legacy terminal review" 24 "no legal edge review_of_review->done" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-reject-legacy-counter-review-done)"
make_baton_v2 "$BOX/baton.json" "counter_review" "prativadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
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
make_baton_v2 "$BOX/baton.next.json" "done" "human" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
run_case "v2 done accepts valid run_explainer_ref path" 0 \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

for done_owner in human team vadi prativadi; do
  BOX="$(new_box "v2-done-accepts-$done_owner")"
  make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
  make_baton_v2 "$BOX/baton.next.json" "done" "$done_owner" 5 \
    '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
    '.vadi_final_approval = true' \
    '.prativadi_final_approval = true'
  run_case "v2 done accepts coordinator assignee $done_owner" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

BOX="$(new_box v2-done-rejects-missing-final-approval)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "team" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = false'
run_case_contains "v2 done requires both final approvals" 23 "DVANDVA_WRITE bad_done_state" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v2-done-rejects-generated-assignee)"
make_baton_v2 "$BOX/baton.json" "deslop" "vadi" 4
make_baton_v2 "$BOX/baton.next.json" "done" "r3-generated-dynamic-review" 5 \
  '.run_explainer_ref = "./superpowers/run-reports/2026-06-28-run-a-explainer.html"' \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
run_case_contains "v2 done rejects generated assignee" 23 "DVANDVA_WRITE bad_done_state" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

for done_owner in human team vadi prativadi; do
  BOX="$(new_box "v1-done-accepts-$done_owner")"
  make_baton "$BOX/baton.json" "phase_review" "prativadi" 4
  make_baton "$BOX/baton.next.json" "done" "$done_owner" 5 \
    '.vadi_final_approval = true' \
    '.prativadi_final_approval = true'
  run_case "v1 done accepts coordinator assignee $done_owner" 0 \
    "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"
done

BOX="$(new_box v1-done-rejects-missing-final-approval)"
make_baton "$BOX/baton.json" "phase_review" "prativadi" 4
make_baton "$BOX/baton.next.json" "done" "team" 5 \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = false'
run_case_contains "v1 done requires both final approvals" 23 "DVANDVA_WRITE bad_done_state" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box v1-done-rejects-generated-assignee)"
make_baton "$BOX/baton.json" "phase_review" "prativadi" 4
make_baton "$BOX/baton.next.json" "done" "generated-owner" 5 \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
run_case_contains "v1 done rejects generated assignee" 23 "DVANDVA_WRITE bad_done_state" \
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
make_baton "$BOX/baton.next.json" "done" "human" 5 \
  '.vadi_final_approval = true' \
  '.prativadi_final_approval = true'
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

BOX="$(new_box stale-checkpoint-same)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 4
run_case_contains "same checkpoint exits 27 stale_checkpoint" 27 "stale_checkpoint" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box stale-checkpoint-lower)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 3
run_case_contains "lower checkpoint exits 27 stale_checkpoint" 27 "stale_checkpoint" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

BOX="$(new_box future-checkpoint-plus-two)"
make_baton "$BOX/baton.json" "spec_drafting" "vadi" 4
make_baton "$BOX/baton.next.json" "spec_review" "prativadi" 6
run_case_contains "checkpoint plus two remains illegal_transition" 24 "DVANDVA_WRITE illegal_transition" \
  "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json"

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

# --- PFX-TIMEOUT: DVANDVA_LOCK_TIMEOUT input validation ----------------------
# The acquire_lock loop uses LOCK_TIMEOUT in shell arithmetic ([[ "$age" -ge
# "$LOCK_TIMEOUT" ]]). Without early validation:
#   (a) A non-numeric value like "abc" is expanded as a variable name in bash
#       arithmetic under set -u → unbound-variable crash (rc=1, unstructured
#       error). The baton is NOT installed but the error is invisible/confusing.
#   (b) A negative value like "-5" makes age(>=0) always satisfy the comparison
#       → immediate steal of ANY held (even live) lock → write succeeds, defeating
#       the locking protocol entirely.
# Fix: validate LOCK_TIMEOUT as ^[0-9]+$ before the lock loop; emit
# "DVANDVA_WRITE bad_lock_timeout" and exit 2 for any invalid value.

# Case (a): non-numeric DVANDVA_LOCK_TIMEOUT with a live contended lock.
# Current: bash crashes with "abc: unbound variable" (rc=1), not a clean error.
# Fixed:   exit 2 + "bad_lock_timeout" before the lock loop.
BOX="$(new_box lock-timeout-non-numeric)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-abc" > "$BOX/.baton.lock.d/owner"
lock_abc_output="$(DVANDVA_LOCK_TIMEOUT=abc "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_abc_exit=$?
if [[ "$lock_abc_exit" -eq 2 && "$lock_abc_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: non-numeric DVANDVA_LOCK_TIMEOUT=abc fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: non-numeric DVANDVA_LOCK_TIMEOUT=abc expected exit 2 + bad_lock_timeout, got exit=$lock_abc_exit output='$lock_abc_output'"
  failures=$((failures + 1))
fi

# Case (b): negative DVANDVA_LOCK_TIMEOUT with a live contended lock.
# Current: age(0) >= -5 is true → immediate steal of the live lock → exit 0
#          (bypass: write succeeds even though someone else holds the lock).
# Fixed:   exit 2 + "bad_lock_timeout" before the lock loop; live lock intact.
BOX="$(new_box lock-timeout-negative)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-neg5" > "$BOX/.baton.lock.d/owner"
lock_neg5_output="$(DVANDVA_LOCK_TIMEOUT=-5 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_neg5_exit=$?
if [[ "$lock_neg5_exit" -eq 2 && "$lock_neg5_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: negative DVANDVA_LOCK_TIMEOUT=-5 fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: negative DVANDVA_LOCK_TIMEOUT=-5 expected exit 2 + bad_lock_timeout, got exit=$lock_neg5_exit output='$lock_neg5_output'"
  failures=$((failures + 1))
fi
ckpt_after_neg5="$(jq -r '.checkpoint // "missing"' "$BOX/baton.json" 2>/dev/null)"
if [[ "$ckpt_after_neg5" == "4" ]]; then
  echo "PASS: negative DVANDVA_LOCK_TIMEOUT did not steal live lock (baton still at checkpoint 4)"
else
  echo "FAIL: negative DVANDVA_LOCK_TIMEOUT stole live lock; baton checkpoint=$ckpt_after_neg5 (expected 4)"
  failures=$((failures + 1))
fi

# Case (c): DVANDVA_LOCK_TIMEOUT=08 - leading-zero octal-invalid value.
# Under bash arithmetic, 08 is an invalid octal literal. In [[ age -ge 08 ]] bash
# prints "value too great for base" and the comparison returns false, so the steal
# path is NEVER taken and the script spin-sleeps until killed by an external timeout.
# Fixed: ^[1-9][0-9]*$ rejects 08 → exit 2 + "bad_lock_timeout" immediately.
BOX="$(new_box lock-timeout-leading-zero-08)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-08" > "$BOX/.baton.lock.d/owner"
lock_08_output="$(DVANDVA_LOCK_TIMEOUT=08 timeout 3 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_08_exit=$?
if [[ "$lock_08_exit" -eq 2 && "$lock_08_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=08 fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=08 expected exit 2 + bad_lock_timeout, got exit=$lock_08_exit output='$lock_08_output'"
  failures=$((failures + 1))
fi

# Case (d): DVANDVA_LOCK_TIMEOUT=09 - leading-zero octal-invalid value (same class as 08).
BOX="$(new_box lock-timeout-leading-zero-09)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-09" > "$BOX/.baton.lock.d/owner"
lock_09_output="$(DVANDVA_LOCK_TIMEOUT=09 timeout 3 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_09_exit=$?
if [[ "$lock_09_exit" -eq 2 && "$lock_09_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=09 fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=09 expected exit 2 + bad_lock_timeout, got exit=$lock_09_exit output='$lock_09_output'"
  failures=$((failures + 1))
fi

# Case (e): DVANDVA_LOCK_TIMEOUT=0 - zero timeout means age(0) >= 0 is always true,
# so any held lock (even a fresh live one) is stolen immediately → baton installs → rc=0.
# This reopens the exact lock-bypass the negative-value fix was supposed to close.
# Fixed: ^[1-9][0-9]*$ rejects 0 → exit 2 + "bad_lock_timeout".
BOX="$(new_box lock-timeout-zero)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-zero" > "$BOX/.baton.lock.d/owner"
lock_zero_output="$(DVANDVA_LOCK_TIMEOUT=0 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_zero_exit=$?
if [[ "$lock_zero_exit" -eq 2 && "$lock_zero_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=0 fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=0 expected exit 2 + bad_lock_timeout, got exit=$lock_zero_exit output='$lock_zero_output'"
  failures=$((failures + 1))
fi
ckpt_after_zero="$(jq -r '.checkpoint // "missing"' "$BOX/baton.json" 2>/dev/null)"
if [[ "$ckpt_after_zero" == "4" ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=0 did not steal live lock (baton still at checkpoint 4)"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=0 stole live lock; baton checkpoint=$ckpt_after_zero (expected 4)"
  failures=$((failures + 1))
fi

# Case (f): DVANDVA_LOCK_TIMEOUT=00 - double-zero leading form; 00 is valid octal (= 0)
# so [[ age -ge 00 ]] ≡ [[ age -ge 0 ]] → instant steal, same bypass as case (e).
# Fixed: ^[1-9][0-9]*$ rejects 00 → exit 2 + "bad_lock_timeout".
BOX="$(new_box lock-timeout-double-zero)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
mkdir -p "$BOX/.baton.lock.d"
printf '%s' "$(date +%s)" > "$BOX/.baton.lock.d/started_at"
printf '%s' "foreign-token-00" > "$BOX/.baton.lock.d/owner"
lock_00_output="$(DVANDVA_LOCK_TIMEOUT=00 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_00_exit=$?
if [[ "$lock_00_exit" -eq 2 && "$lock_00_output" == *"bad_lock_timeout"* ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=00 fails closed exit 2 with bad_lock_timeout"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=00 expected exit 2 + bad_lock_timeout, got exit=$lock_00_exit output='$lock_00_output'"
  failures=$((failures + 1))
fi
ckpt_after_00="$(jq -r '.checkpoint // "missing"' "$BOX/baton.json" 2>/dev/null)"
if [[ "$ckpt_after_00" == "4" ]]; then
  echo "PASS: DVANDVA_LOCK_TIMEOUT=00 did not steal live lock (baton still at checkpoint 4)"
else
  echo "FAIL: DVANDVA_LOCK_TIMEOUT=00 stole live lock; baton checkpoint=$ckpt_after_00 (expected 4)"
  failures=$((failures + 1))
fi

# Case (g): valid DVANDVA_LOCK_TIMEOUT=5 (canonical positive decimal) must still be
# accepted; uncontended write must succeed so we don't break the normal code path.
BOX="$(new_box lock-timeout-valid-5)"
make_baton "$BOX/baton.json" "implementing" "vadi" 4
make_baton "$BOX/baton.next.json" "phase_review" "prativadi" 5
lock_valid5_output="$(DVANDVA_LOCK_TIMEOUT=5 "$SCRIPT" "$BOX/baton.json" "$BOX/baton.next.json" 2>&1)"
lock_valid5_exit=$?
if [[ "$lock_valid5_exit" -eq 0 ]]; then
  echo "PASS: valid DVANDVA_LOCK_TIMEOUT=5 succeeds (canonical positive decimal accepted)"
else
  echo "FAIL: valid DVANDVA_LOCK_TIMEOUT=5 rejected; got exit=$lock_valid5_exit output='$lock_valid5_output'"
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
