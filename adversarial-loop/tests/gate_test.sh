#!/usr/bin/env bash
# Regression coverage for the adversarial-loop Stop gate.
# Layer A calls the sourceable predicate directly. Layer B invokes the hook in
# actual temporary Git repositories, as Claude Code does.

set -u -o pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
LOOP_DIR=$(cd -- "$SCRIPT_DIR/.." && pwd -P)
PREDICATE="$LOOP_DIR/lib/predicate.sh"
GATE="$LOOP_DIR/hooks/gate.sh"
GATE_CLI="$LOOP_DIR/hooks/gate-cli.sh"

if [[ ! -f "$PREDICATE" || ! -f "$GATE" ]]; then
  printf 'FAIL: expected predicate and hook under %s\n' "$LOOP_DIR" >&2
  exit 1
fi

# shellcheck source=../lib/predicate.sh
source "$PREDICATE"

TEST_TMP=$(mktemp -d "${TMPDIR:-/tmp}/adversarial-loop-gate-test.XXXXXX")
trap 'rm -rf "$TEST_TMP"' EXIT

passed=0
failed=0

record() {
  local outcome=$1
  local label=$2
  if [[ "$outcome" == pass ]]; then
    passed=$((passed + 1))
    printf 'PASS  %s\n' "$label"
  else
    failed=$((failed + 1))
    printf 'FAIL  %s\n' "$label" >&2
  fi
}

new_repo() {
  local name=$1
  local repo="$TEST_TMP/$name"
  mkdir -p "$repo"
  git -C "$repo" init --quiet
  printf 'artifact version one\n' > "$repo/artifact.txt"
  printf '%s\n' "$repo"
}

sha_of() {
  sha256sum -- "$1" | awk '{print $1}'
}

write_file() {
  local path=$1
  local content=$2
  mkdir -p "$(dirname -- "$path")"
  printf '%s\n' "$content" > "$path"
}

write_goal() {
  local repo=$1
  local owner=$2
  local status=$3
  local steps_json=$4
  local goal_id=${5:-goal-1}
  local mode=${6:-}
  write_file "$repo/.adversarial-loop/goal.json" "$(jq -cn \
    --arg goal_id "$goal_id" \
    --arg owner "$owner" \
    --arg status "$status" \
    --arg mode "$mode" \
    --argjson steps "$steps_json" \
    '({goal_id:$goal_id,owner_session_id:$owner,status:$status,acceptance:"gate test",steps:$steps}) +
      (if $mode == "" then {} else {mode:$mode} end)')"
}

step_json() {
  local id=$1
  local step_status=$2
  local author=$3
  local revision=$4
  local artifact_path=$5
  local digest=$6
  local author_agent_id=${7:-}
  jq -cn \
    --arg id "$id" \
    --arg status "$step_status" \
    --arg author "$author" \
    --arg path "$artifact_path" \
    --arg digest "$digest" \
    --arg author_agent_id "$author_agent_id" \
    --argjson revision "$revision" \
    '({id:$id,kind:"execute",author_family:$author,revision:$revision,status:$status,artifact_path:$path,artifact_digest:$digest}) +
      (if $author_agent_id == "" then {} else {author_agent_id:$author_agent_id} end)'
}

write_evidence() {
  local repo=$1
  local step_id=$2
  local filename=$3
  local created_at=$4
  local revision=$5
  local digest=$6
  local reviewer=$7
  local verdict=$8
  local goal_id=${9:-goal-1}
  local reviewer_agent_id=${10:-}
  write_file "$repo/.adversarial-loop/evidence/$goal_id/$step_id/$filename" "$(jq -cn \
    --arg goal_id "$goal_id" \
    --arg step_id "$step_id" \
    --arg digest "$digest" \
    --arg reviewer "$reviewer" \
    --arg verdict "$verdict" \
    --arg created_at "$created_at" \
    --arg reviewer_agent_id "$reviewer_agent_id" \
    --argjson revision "$revision" \
    '({goal_id:$goal_id,step_id:$step_id,step_revision:$revision,artifact_digest:$digest,reviewer_family:$reviewer,reviewer_model:"test-model",verdict:$verdict,findings:[],transcript_ref:"test://transcript",created_at:$created_at}) +
      (if $reviewer_agent_id == "" then {} else {reviewer_agent_id:$reviewer_agent_id} end)')"
}

predicate_result() {
  adversarial_loop_gate_predicate "$1" "$2"
}

assert_predicate() {
  local label=$1
  local expected=$2
  local repo=$3
  local session=$4
  local expected_reason=${5:-}
  local result decision reason
  result=$(predicate_result "$repo" "$session")
  decision=${result%%$'\t'*}
  reason=${result#*$'\t'}

  if [[ "$decision" != "$expected" ]]; then
    record fail "$label (expected $expected, got $decision: $reason)"
  elif [[ -n "$expected_reason" && "$reason" != *"$expected_reason"* ]]; then
    record fail "$label (reason did not contain '$expected_reason': $reason)"
  else
    record pass "$label"
  fi
}

hook_input() {
  local session=$1
  local cwd=$2
  local stop_hook_active=${3:-false}
  jq -cn --arg session "$session" --arg cwd "$cwd" --argjson active "$stop_hook_active" \
    '{session_id:$session,cwd:$cwd,stop_hook_active:$active}'
}

assert_hook() {
  local label=$1
  local expected=$2
  local repo=$3
  local session=$4
  local cwd=${5:-$repo}
  local stop_hook_active=${6:-false}
  local expected_reason=${7:-}
  local output exit_code

  output=$(cd "$cwd" && hook_input "$session" "$cwd" "$stop_hook_active" | "$GATE")
  exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    record fail "$label (hook exited $exit_code)"
    return
  fi

  if [[ "$expected" == allow ]]; then
    if [[ -z "$output" ]]; then
      record pass "$label"
    else
      record fail "$label (expected no stdout, got: $output)"
    fi
    return
  fi

  if [[ -z "$expected_reason" ]]; then
    record fail "$label (block assertion is missing an expected reason substring)"
    return
  fi

  if jq -e --arg expected_reason "$expected_reason" '
    .decision == "block" and
    (.reason | type == "string" and contains($expected_reason))
  ' \
    >/dev/null 2>&1 <<<"$output"; then
    record pass "$label"
  else
    record fail "$label (expected one Stop block JSON object containing reason '$expected_reason', got: $output)"
  fi
}

make_valid_active_goal() {
  local repo=$1
  local author=${2:-claude}
  local step_status=${3:-complete}
  local mode=${4:-}
  local author_agent_id=${5:-}
  local digest
  digest=$(sha_of "$repo/artifact.txt")
  write_goal "$repo" session-a active "[$(step_json step-1 "$step_status" "$author" 1 artifact.txt "$digest" "$author_agent_id")]" goal-1 "$mode"
  printf '%s\n' "$digest"
}

printf '%s\n' 'Layer A: sourceable predicate'

repo=$(new_repo absent-goal)
assert_predicate 'absent goal allows' allow "$repo" session-a

repo=$(new_repo malformed-goal)
write_file "$repo/.adversarial-loop/goal.json" '{not valid json'
assert_predicate 'malformed goal blocks' block "$repo" session-a 'malformed'

repo=$(new_repo done-with-evidence)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" session-a done "[$(step_json step-1 complete claude 1 artifact.txt "$digest")]"
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
assert_predicate 'status=done with full passing evidence allows' allow "$repo" session-a

repo=$(new_repo done-missing-evidence)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" session-a done "[$(step_json step-1 complete claude 1 artifact.txt "$digest")]"
assert_predicate 'status=done with missing evidence blocks' block "$repo" session-a 'missing evidence'

repo=$(new_repo invalid-status)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" session-a paused "[$(step_json step-1 complete claude 1 artifact.txt "$digest")]"
assert_predicate 'invalid status blocks fail-closed' block "$repo" session-a 'invalid status'

repo=$(new_repo wrong-session)
make_valid_active_goal "$repo" >/dev/null
assert_predicate 'wrong session allows' allow "$repo" session-b

repo=$(new_repo wrong-session-missing-acceptance)
digest=$(sha_of "$repo/artifact.txt")
write_file "$repo/.adversarial-loop/goal.json" "$(jq -cn \
  --arg digest "$digest" \
  '{goal_id:"goal-1",owner_session_id:"session-a",status:"active",steps:[{id:"step-1",kind:"execute",author_family:"claude",revision:1,status:"complete",artifact_path:"artifact.txt",artifact_digest:$digest}]}')"
assert_predicate 'other-session goal missing acceptance allows' allow "$repo" session-b

repo=$(new_repo empty-owner)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" '' active "[$(step_json step-1 pending claude 1 artifact.txt "$digest")]"
assert_predicate 'active goal with empty owner blocks' block "$repo" session-a 'owner_session_id'

repo=$(new_repo invalid-goal-id)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" session-a active "[$(step_json step-1 complete claude 1 artifact.txt "$digest")]" '../outside'
assert_predicate 'goal id traversal blocks before evidence lookup' block "$repo" session-a 'invalid goal id'

repo=$(new_repo empty-steps)
write_goal "$repo" session-a active '[]'
assert_predicate 'empty steps block' block "$repo" session-a 'steps empty'

repo=$(new_repo duplicate-ids)
digest=$(sha_of "$repo/artifact.txt")
step=$(step_json step-1 complete claude 1 artifact.txt "$digest")
write_goal "$repo" session-a active "[$step,$step]"
assert_predicate 'duplicate step ids block' block "$repo" session-a 'duplicate'

repo=$(new_repo invalid-id)
digest=$(sha_of "$repo/artifact.txt")
write_goal "$repo" session-a active "[$(step_json 'Bad!step' complete claude 1 artifact.txt "$digest")]"
assert_predicate 'bad id regex blocks' block "$repo" session-a 'invalid id'

repo=$(new_repo pending-step)
digest=$(make_valid_active_goal "$repo" claude pending)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
assert_predicate 'step not complete blocks' block "$repo" session-a 'not complete'

repo=$(new_repo pending-empty-digest)
write_goal "$repo" session-a active "[$(step_json step-1 pending claude 1 artifact.txt '')]"
assert_predicate 'pending step accepts empty digest before evidence check' block "$repo" session-a 'missing evidence'

repo=$(new_repo complete-invalid-digest)
write_goal "$repo" session-a active "[$(step_json step-1 complete claude 1 artifact.txt not-a-sha256)]"
assert_predicate 'complete step with non-64hex digest blocks' block "$repo" session-a 'missing or invalid required fields'

repo=$(new_repo missing-evidence)
make_valid_active_goal "$repo" >/dev/null
assert_predicate 'missing evidence blocks' block "$repo" session-a 'missing evidence'

repo=$(new_repo verdict-fail)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt fail
assert_predicate 'failing latest verdict blocks' block "$repo" session-a 'verdict'

repo=$(new_repo self-family)
digest=$(make_valid_active_goal "$repo" claude)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" claude pass
assert_predicate 'self-family review blocks' block "$repo" session-a 'reviewer family'

repo=$(new_repo stale-revision)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 2 "$digest" gpt pass
assert_predicate 'stale revision blocks' block "$repo" session-a 'revision mismatch'

repo=$(new_repo artifact-changed)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
printf 'artifact version two\n' > "$repo/artifact.txt"
assert_predicate 'changed completed artifact blocks' block "$repo" session-a 'artifact changed'

repo=$(new_repo evidence-digest-mismatch)
digest=$(make_valid_active_goal "$repo")
wrong_digest=$(printf '0%.0s' {1..64})
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$wrong_digest" gpt pass
assert_predicate 'evidence artifact-digest mismatch blocks' block "$repo" session-a 'artifact digest mismatch'

repo=$(new_repo later-pass)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt fail
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T10:01:00Z 1 "$digest" gpt pass
assert_predicate 'latest pass after earlier fail allows' allow "$repo" session-a
if [[ -f "$repo/.adversarial-loop/evidence/goal-1/step-1/attempt-1.json" && -f "$repo/.adversarial-loop/evidence/goal-1/step-1/attempt-2.json" ]]; then
  record pass 'later-pass fixture retains both attempts'
else
  record fail 'later-pass fixture retains both attempts'
fi

repo=$(new_repo later-fail)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T09:00:00Z 1 "$digest" gpt pass
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T10:00:00Z 1 "$digest" gpt fail
assert_predicate 'latest fail after earlier pass blocks' block "$repo" session-a 'verdict is fail'

repo=$(new_repo malformed-created-at-precision)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T9:00:00Z 1 "$digest" gpt pass
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T10:00:00Z 1 "$digest" gpt fail
assert_predicate 'non-fixed-width created_at blocks as malformed' block "$repo" session-a 'malformed evidence'

repo=$(new_repo malformed-created-at-offset)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00+05:00 1 "$digest" gpt pass
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T06:00:00Z 1 "$digest" gpt fail
assert_predicate 'offset created_at blocks as malformed' block "$repo" session-a 'malformed evidence'

repo=$(new_repo numeric-attempt-order)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
write_evidence "$repo" step-1 attempt-10.json 2026-07-13T10:00:00Z 1 "$digest" gpt fail
assert_predicate 'same-time attempt-10 sorts after attempt-2' block "$repo" session-a 'verdict is fail'

repo=$(new_repo fractional-created-at-order)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
write_evidence "$repo" step-1 attempt-2.json 2026-07-13T10:00:00.1Z 1 "$digest" gpt fail
assert_predicate 'fractional-second later fail sorts after whole second' block "$repo" session-a 'verdict is fail'

repo=$(new_repo cross-family-pass)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
assert_predicate 'all-pass cross-family allows' allow "$repo" session-a

repo=$(new_repo cross-context-pass)
digest=$(make_valid_active_goal "$repo" claude complete cross-context author-agent-a)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" claude pass goal-1 reviewer-agent-b
assert_predicate 'cross-context distinct reviewer agent allows' allow "$repo" session-a

repo=$(new_repo cross-context-same-agent)
digest=$(make_valid_active_goal "$repo" claude complete cross-context agent-a)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" claude pass goal-1 agent-a
assert_predicate 'cross-context same reviewer agent blocks' block "$repo" session-a 'reviewer agent id matches author agent id'

repo=$(new_repo cross-context-missing-reviewer-agent)
digest=$(make_valid_active_goal "$repo" claude complete cross-context author-agent-a)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" claude pass
assert_predicate 'cross-context missing reviewer agent id blocks' block "$repo" session-a 'reviewer_agent_id'

repo=$(new_repo cross-context-missing-author-agent)
digest=$(make_valid_active_goal "$repo" claude complete cross-context)
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" claude pass goal-1 reviewer-agent-b
assert_predicate 'cross-context missing author agent id blocks' block "$repo" session-a 'author_agent_id'

repo=$(new_repo unknown-step)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
write_file "$repo/.adversarial-loop/evidence/goal-1/unknown-step/ignored.json" '{invalid evidence for an unknown step}'
assert_predicate 'unknown-step evidence is ignored' allow "$repo" session-a

repo=$(new_repo concurrent-goal)
make_valid_active_goal "$repo" >/dev/null
assert_predicate 'concurrent second goal not owned by this session allows' allow "$repo" session-b

repo=$(new_repo truncated-step-enumeration)
make_valid_active_goal "$repo" >/dev/null
real_jq=$(command -v jq)
fake_bin="$TEST_TMP/truncated-step-jq-bin"
mkdir -p "$fake_bin"
write_file "$fake_bin/jq" '#!/usr/bin/env bash
for arg in "$@"; do
  if [[ "$arg" == ".steps | keys[]" ]]; then
    exit 0
  fi
done
exec "$ADVERSARIAL_LOOP_REAL_JQ" "$@"'
chmod +x "$fake_bin/jq"
result=$(ADVERSARIAL_LOOP_REAL_JQ="$real_jq" PATH="$fake_bin:$PATH" predicate_result "$repo" session-a)
decision=${result%%$'\t'*}
reason=${result#*$'\t'}
if [[ "$decision" == block && "$reason" == *'validated step count mismatch'* ]]; then
  record pass 'truncated step enumeration blocks fail-closed'
else
  record fail "truncated step enumeration blocks fail-closed (got $decision: $reason)"
fi

printf '%s\n' 'Layer B: Stop hook end-to-end'

repo=$(new_repo hook-absent)
assert_hook 'hook allows when goal is absent' allow "$repo" session-a

repo=$(new_repo hook-malformed)
write_file "$repo/.adversarial-loop/goal.json" '{not valid json'
assert_hook 'hook blocks malformed goal' block "$repo" session-a "$repo" false 'malformed'

repo=$(new_repo hook-jq-missing)
make_valid_active_goal "$repo" >/dev/null
fake_bin="$TEST_TMP/no-jq-bin"
mkdir -p "$fake_bin"
for command in cat dirname git grep pwd; do
  ln -s "$(command -v "$command")" "$fake_bin/$command"
done
output=$(cd "$repo" && hook_input session-a "$repo" false | PATH="$fake_bin" /usr/bin/bash "$GATE")
exit_code=$?
if [[ $exit_code -eq 0 ]] && jq -e '.decision == "block" and (.reason | contains("jq"))' >/dev/null 2>&1 <<<"$output"; then
  record pass 'hook blocks active goal when jq is missing'
else
  record fail "hook blocks active goal when jq is missing (exit $exit_code, stdout: $output)"
fi

repo=$(new_repo hook-sha256sum-missing)
make_valid_active_goal "$repo" >/dev/null
fake_bin="$TEST_TMP/no-sha256sum-bin"
mkdir -p "$fake_bin"
for command in cat dirname git jq; do
  ln -s "$(command -v "$command")" "$fake_bin/$command"
done
output=$(cd "$repo" && hook_input session-a "$repo" false | PATH="$fake_bin" /usr/bin/bash "$GATE")
exit_code=$?
if [[ $exit_code -eq 0 ]] && jq -e '.decision == "block" and (.reason | contains("sha256sum"))' >/dev/null 2>&1 <<<"$output"; then
  record pass 'hook blocks active goal when sha256sum is missing'
else
  record fail "hook blocks active goal when sha256sum is missing (exit $exit_code, stdout: $output)"
fi

repo=$(new_repo hook-nested-cwd)
digest=$(make_valid_active_goal "$repo")
write_evidence "$repo" step-1 attempt-1.json 2026-07-13T10:00:00Z 1 "$digest" gpt pass
mkdir -p "$repo/a/deep/nested"
assert_hook 'hook resolves Git root from nested cwd' allow "$repo" session-a "$repo/a/deep/nested"

repo=$(new_repo hook-refire)
make_valid_active_goal "$repo" >/dev/null
assert_hook 'hook re-blocks when stop_hook_active is true' block "$repo" session-a "$repo" true 'missing evidence'

repo=$(new_repo hook-control-character)
make_valid_active_goal "$repo" >/dev/null
control_name=$'bad\x1fname.json'
write_file "$repo/.adversarial-loop/evidence/goal-1/step-1/$control_name" '{not valid json'
assert_hook 'hook emits valid block JSON when reason contains a control character' block "$repo" session-a "$repo" false 'malformed evidence'

printf '%s\n' 'Layer C: CLI adapter end-to-end'

repo=$(new_repo cli-missing-evidence)
make_valid_active_goal "$repo" >/dev/null
cli_stderr="$TEST_TMP/gate-cli.stderr"
if [[ ! -x "$GATE_CLI" ]]; then
  record fail 'CLI adapter blocks missing evidence (adapter is missing or not executable)'
else
  cli_stdout=$(cd "$repo" && "$GATE_CLI" --session session-a 2>"$cli_stderr")
  exit_code=$?
  if [[ $exit_code -ne 0 && -z "$cli_stdout" ]] && grep -q 'missing evidence' "$cli_stderr"; then
    record pass 'CLI adapter blocks missing evidence'
  else
    record fail "CLI adapter blocks missing evidence (exit $exit_code, stdout: $cli_stdout, stderr: $(<"$cli_stderr"))"
  fi
fi

total=$((passed + failed))
if [[ $failed -eq 0 ]]; then
  printf 'PASS: %d/%d tests passing\n' "$passed" "$total"
  exit 0
fi

printf 'FAIL: %d/%d tests passing; %d failed\n' "$passed" "$total" "$failed" >&2
exit 1
