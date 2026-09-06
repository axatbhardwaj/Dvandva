#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
export XDG_DATA_HOME="$test_root/data" XDG_STATE_HOME="$test_root/state"
cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
mkdir -p "$XDG_DATA_HOME/dvandva/bin/0.4.1"
cp "$repo_root/v4/target/debug/dvandva-v4" "$XDG_DATA_HOME/dvandva/bin/0.4.1/dvandva-kernel"
vadi="$repo_root/skills/vadi/scripts/dvandva-role.sh"
prati="$repo_root/skills/prativadi/scripts/dvandva-role.sh"
workspace="$test_root/repo"
git init --quiet "$workspace"
git -C "$workspace" remote add origin https://github.com/example/project.git
field() { python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }
scan() { bash "$prati" discover reviewer codex claude "$workspace" "$@"; }
create() {
  if test "$4" = review; then
    local number="${3#PR-}"
    bash "$vadi" start "$1" claude codex "$workspace" "$2" \
      --new-run --task-reference "$3" --objective-ref workflow=review \
      --objective-ref "review_member=https://github.com/example/project/pull/$number" \
      --required-deliverable "pr-$number=Review PR $number"
  else
    bash "$vadi" start "$1" claude codex "$workspace" "$2" \
      --new-run --task-reference "$3" --objective-ref "workflow=$4" \
      --required-deliverable delivery='Complete work'
  fi
}
revision() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' "$1/baton.json"
}
apply_json() {
  local facade="$1" session="$2" run_dir="$3" name="$4" payload="$5"
  local action="$test_root/$name.json"
  printf '%s\n' "$payload" >"$action"
  chmod 600 "$action"
  bash "$facade" apply "$session" "$run_dir" "$(revision "$run_dir")" "$action"
  unlink "$action"
}
# An empty lookup neither creates registry directories nor requires objective wording.
result="$(scan --workflow review)"
test "$(field outcome <<<"$result")" = none
test ! -e "$XDG_STATE_HOME"
result="$(scan --workflow freeflow)"
test "$(field outcome <<<"$result")" = none
test ! -e "$XDG_STATE_HOME"
first="$(create worker-a 'Review this change carefully' PR-10 review)"
first_id="$(field run_id <<<"$first")"
create worker-b 'Maintain our own change' PR-10 babysitting >/dev/null
create worker-c 'Another external change' PR-20 review >/dev/null
foreign="$(bash "$vadi" start worker-foreign other-harness codex "$workspace" \
  'Review with another worker' --new-run --task-reference PR-30 \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/30 \
  --required-deliverable pr-30='Review PR 30')"
foreign_id="$(field run_id <<<"$foreign")"
# The intended peer must match before claiming, even for an otherwise unique task.
test "$(field outcome <<<"$(scan --workflow review --task-reference PR-30)")" = none
wrong_pair="$(bash "$prati" start reviewer codex claude "$workspace" --run-id "$foreign_id")"
test "$(field outcome <<<"$wrong_pair")" = scope_mismatch
# Enumeration is observational: compare every durable byte and file mode.
fingerprint() {
  python3 - "$XDG_STATE_HOME" <<'PY'
import hashlib, pathlib, sys
root = pathlib.Path(sys.argv[1])
for p in sorted(root.rglob('*')):
    print(p.relative_to(root), p.stat().st_mode, hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else '')
PY
}
before="$(fingerprint)"
result="$(scan --workflow review --task-reference PR-10)"
test "$(field outcome <<<"$result")" = match
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["read_only"] is True; assert len(s["candidates"]) == 1; assert s["candidates"][0]["run_id"] == sys.argv[1]' "$first_id" <<<"$result"
test "$(fingerprint)" = "$before"
test "$(field outcome <<<"$(scan --workflow review)")" = ambiguous
# Workflow filters must distinguish legacy one-shot review from persistent Review.
create worker-d 'Legacy external review' PR-10 pr_review >/dev/null
test "$(field outcome <<<"$(scan --workflow review --task-reference PR-10)")" = match
# Batch Review uses a null/real parent scalar task and exact canonical member
# refs. A member lookup must select the batch without pretending that member is
# the run's scalar task assertion.
batch="$(bash "$vadi" start worker-batch claude codex "$workspace" \
  'Review the selected pull requests' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/41 \
  --objective-ref review_member=https://github.com/example/project/pull/42 \
  --required-deliverable pr-41='Review https://github.com/example/project/pull/41' \
  --required-deliverable pr-42='Review https://github.com/example/project/pull/42')"
batch_id="$(field run_id <<<"$batch")"
result="$(scan --workflow review --task-reference https://GITHUB.com/EXAMPLE/PROJECT/pull/42)"
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["outcome"] == "match"; assert s["candidates"][0]["run_id"] == sys.argv[1]; assert s["match_basis"] == "review_member"' "$batch_id" <<<"$result"
# An exact member shared by two eligible batches is ambiguous; lookup must not
# guess the newest candidate.
bash "$vadi" start worker-batch-two claude codex "$workspace" \
  'Review another selected batch' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/42 \
  --required-deliverable pr-42='Review https://github.com/example/project/pull/42' >/dev/null
test "$(field outcome <<<"$(scan --workflow review --task-reference https://github.com/example/project/pull/42)")" = ambiguous
# Exact supplied identity resolves that ambiguity and intentionally omits the
# member as a scalar task assertion.
joined_batch="$(bash "$prati" start batch-reviewer codex claude "$workspace" --run-id "$batch_id")"
test "$(field run_id <<<"$joined_batch")" = "$batch_id"
# Enumeration is not an exact-join assertion. If the human changes canonical
# scope between those operations, exact join returns the new scope and the role
# must recheck it instead of acting on the enumerated member list.
drift_batch="$(bash "$vadi" start worker-drift claude codex "$workspace" \
  'Review a selection that changes before join' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/81 \
  --required-deliverable pr-81='Review https://github.com/example/project/pull/81')"
drift_id="$(field run_id <<<"$drift_batch")"
drift_dir="$XDG_STATE_HOME/dvandva/runs/$drift_id"
result="$(scan --workflow review --task-reference https://github.com/example/project/pull/81)"
test "$(field outcome <<<"$result")" = match
apply_json "$vadi" worker-drift "$drift_dir" drift-request \
  '{"type":"request_human_decision","kind":"scope","question":"Which reviewed PR remains in scope?","evidence":["The human changed the selected PR"],"options":["Review PR 82","Keep PR 81"]}' >/dev/null
apply_json "$vadi" worker-drift "$drift_dir" drift-resume \
  '{"type":"resume_human_decision","answer":"Review PR 82","scope_amendment":{"objective":"Review the amended selection","objective_refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":"https://github.com/example/project/pull/82"}],"task_reference":null,"scope_deliverables":[{"id":"pr-82","description":"Review https://github.com/example/project/pull/82"}]}}' >/dev/null
joined_drift="$(bash "$prati" start drift-reviewer codex claude "$workspace" --run-id "$drift_id")"
python3 -c '
import json, sys
s = json.load(sys.stdin)
refs = {(ref["kind"], ref["value"]) for ref in s["objective"]["refs"]}
assert s["scope_revision"] == 1
assert ("review_member", "https://github.com/example/project/pull/82") in refs
assert all(value != "https://github.com/example/project/pull/81" for _, value in refs)
assert s["scope_deliverables"] == [{"id":"pr-82","description":"Review https://github.com/example/project/pull/82"}]
' <<<"$joined_drift"
test "$(field outcome <<<"$(scan --workflow review --task-reference https://github.com/example/project/pull/81)")" = none
# Duplicate member refs are malformed scope, not two votes for the same PR.
# New starts reject them at the facade; retain discovery hardening for an older
# or human-amended run whose durable scope contains duplicates.
duplicate="$(bash "$vadi" start worker-duplicate claude codex "$workspace" \
  'Batch amended into malformed duplicate scope' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/55 \
  --required-deliverable pr-55='Review https://github.com/example/project/pull/55')"
duplicate_id="$(field run_id <<<"$duplicate")"
duplicate_dir="$XDG_STATE_HOME/dvandva/runs/$duplicate_id"
apply_json "$vadi" worker-duplicate "$duplicate_dir" duplicate-request \
  '{"type":"request_human_decision","kind":"scope","question":"Which members remain in scope?","evidence":["The selected batch changed"],"options":["Apply amended batch","Keep current batch"]}' >/dev/null
apply_json "$vadi" worker-duplicate "$duplicate_dir" duplicate-resume \
  '{"type":"resume_human_decision","answer":"Apply amended batch","scope_amendment":{"objective":"Malformed duplicate batch","objective_refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":"https://github.com/example/project/pull/55"},{"kind":"review_member","value":"https://GITHUB.com/EXAMPLE/PROJECT/pull/55"}],"task_reference":null,"scope_deliverables":[{"id":"pr-55","description":"Review https://github.com/example/project/pull/55"}]}}' >/dev/null
set +e
duplicate_result="$(scan --workflow review --task-reference https://github.com/example/project/pull/55)"
duplicate_status=$?
set -e
test "$duplicate_status" -eq 0
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["outcome"] == "none"; assert s["invalid_candidates"] == [{"run_id":sys.argv[1],"error":"candidate has duplicate review_member references"}]' "$duplicate_id" <<<"$duplicate_result"
# One malformed candidate cannot poison a healthy exact member match.
healthy_55="$(bash "$vadi" start worker-healthy-55 claude codex "$workspace" \
  'Healthy batch beside malformed scope' --new-run --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/55 \
  --required-deliverable pr-55='Review https://github.com/example/project/pull/55')"
healthy_55_id="$(field run_id <<<"$healthy_55")"
result="$(scan --workflow review --task-reference https://github.com/example/project/pull/55)"
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["outcome"] == "match"; assert s["candidates"][0]["run_id"] == sys.argv[1]; assert len(s["invalid_candidates"]) == 1' "$healthy_55_id" <<<"$result"
# Freeflow is independently filterable and never aliases Implementation.
freeflow="$(create worker-freeflow 'Investigate runtime behavior' REPORT-1 freeflow)"
freeflow_id="$(field run_id <<<"$freeflow")"
result="$(scan --workflow freeflow --task-reference REPORT-1)"
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["outcome"] == "match"; assert s["candidates"][0]["run_id"] == sys.argv[1]' "$freeflow_id" <<<"$result"
test "$(field outcome <<<"$(scan --workflow implementation --task-reference REPORT-1)")" = none
# A unique discovered candidate joins through the unchanged exact-run protocol.
joined="$(bash "$prati" start reviewer codex claude "$workspace" --run-id "$first_id")"
test "$(field run_id <<<"$joined")" = "$first_id"
# Another reviewer must not silently take the busy candidate.
test "$(field outcome <<<"$(bash "$prati" discover other-reviewer codex claude "$workspace" --workflow review --task-reference PR-10)")" != match
# Missing explicit IDs remain exact failures; discovery cannot replace the run.
missing="$(bash "$prati" start reviewer codex claude "$workspace" --run-id missing-run)"
test "$(field outcome <<<"$missing")" = run_missing
# Same repository across worktrees is discoverable; unrelated repositories are not.
git -C "$workspace" -c user.name=Canary -c user.email=canary@example.invalid commit --quiet --allow-empty -m fixture
linked="$test_root/linked"
git -C "$workspace" worktree add --quiet --detach "$linked" HEAD
test "$(field outcome <<<"$(bash "$prati" discover reviewer codex claude "$linked" --workflow review --task-reference PR-10)")" = match
other="$test_root/other"
git init --quiet "$other"
git -C "$other" remote add origin https://github.com/example/unrelated.git
test "$(field outcome <<<"$(bash "$prati" discover reviewer codex claude "$other" --workflow review)")" = none
# A wait with unrelated candidates must rest and find a run created later.
DVANDVA_DISCOVER_TIMEOUT_MS=5000 DVANDVA_DISCOVER_INTERVAL_MS=50 \
  scan --workflow discovery --task-reference SPEC-1 --wait >"$test_root/wait.json" &
wait_pid=$!
sleep 0.2
create worker-e 'Explore the feature' SPEC-1 discovery >/dev/null
wait "$wait_pid"
test "$(field outcome <"$test_root/wait.json")" = match
# No matching candidate returns bounded none, never an invented replacement.
result="$(DVANDVA_DISCOVER_TIMEOUT_MS=50 scan --workflow discovery --task-reference MISSING --wait)"
test "$(field outcome <<<"$result")" = none
# A legacy or human-amended candidate cannot smuggle a member from another
# canonical repository into this repository-scoped lookup.
cross_repo="$(bash "$vadi" start worker-cross-repo claude codex "$workspace" \
  'Batch amended into cross-repository scope' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/70 \
  --required-deliverable pr-70='Review https://github.com/example/project/pull/70')"
cross_id="$(field run_id <<<"$cross_repo")"
cross_dir="$XDG_STATE_HOME/dvandva/runs/$cross_id"
apply_json "$vadi" worker-cross-repo "$cross_dir" cross-request \
  '{"type":"request_human_decision","kind":"scope","question":"Which repository remains in scope?","evidence":["The selected repository changed"],"options":["Apply amended repository","Keep current repository"]}' >/dev/null
apply_json "$vadi" worker-cross-repo "$cross_dir" cross-resume \
  '{"type":"resume_human_decision","answer":"Apply amended repository","scope_amendment":{"objective":"Malformed cross-repository batch","objective_refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":"https://github.com/example/other/pull/70"}],"task_reference":null,"scope_deliverables":[{"id":"pr-70","description":"Review https://github.com/example/other/pull/70"}]}}' >/dev/null
set +e
cross_result="$(scan --workflow review --task-reference https://github.com/example/other/pull/70)"
cross_status=$?
set -e
test "$cross_status" -eq 0
python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["outcome"] == "none"; assert s["invalid_candidates"][0]["error"] == "candidate has a non-canonical or cross-repository review member"' <<<"$cross_result"
# Multiple workflow refs are a run-contract violation, not a malformed member
# that discovery may silently skip. This intentionally poisons later scans, so
# keep the fixture last.
ambiguous_workflow="$(bash "$vadi" start worker-ambiguous-workflow claude codex "$workspace" \
  'Review batch amended into ambiguous workflow scope' --new-run \
  --objective-ref workflow=review \
  --objective-ref review_member=https://github.com/example/project/pull/56 \
  --required-deliverable pr-56='Review https://github.com/example/project/pull/56')"
ambiguous_workflow_id="$(field run_id <<<"$ambiguous_workflow")"
ambiguous_workflow_dir="$XDG_STATE_HOME/dvandva/runs/$ambiguous_workflow_id"
apply_json "$vadi" worker-ambiguous-workflow "$ambiguous_workflow_dir" ambiguous-workflow-request \
  '{"type":"request_human_decision","kind":"scope","question":"Which workflow remains in scope?","evidence":["The workflow selection changed"],"options":["Apply ambiguous workflow","Keep Review"]}' >/dev/null
apply_json "$vadi" worker-ambiguous-workflow "$ambiguous_workflow_dir" ambiguous-workflow-resume \
  '{"type":"resume_human_decision","answer":"Apply ambiguous workflow","scope_amendment":{"objective":"Malformed ambiguous workflow","objective_refs":[{"kind":"workflow","value":"review"},{"kind":"Workflow","value":"freeflow"},{"kind":"review_member","value":"https://github.com/example/project/pull/56"}],"task_reference":null,"scope_deliverables":[{"id":"pr-56","description":"Review https://github.com/example/project/pull/56"}]}}' >/dev/null
set +e
ambiguous_workflow_result="$(scan --workflow review 2>&1)"
ambiguous_workflow_status=$?
set -e
test "$ambiguous_workflow_status" -ne 0
grep -Fq 'candidate has ambiguous workflow references' <<<"$ambiguous_workflow_result"
cmp "$vadi" "$prati"
cmp "$repo_root/skills/vadi/scripts/discover.py" "$repo_root/skills/prativadi/scripts/discover.py"
printf 'automatic run discovery: ok\n'
