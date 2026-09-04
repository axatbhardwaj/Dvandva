#!/usr/bin/env bash
# Behavioural coverage for `dvandva-role.sh poll`: idle re-entry, an actionable
# wake, a terminal return, lease renewal across the wait, and the MAX_MS budget.
set -euo pipefail
# Byte-order collation: filename comparisons below must not depend on the
# invoking user's locale.
export LC_ALL=C

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
binary="$XDG_DATA_HOME/dvandva/bin/0.3.8/dvandva-kernel"
mkdir -p "$(dirname "$binary")"
cp "$repo_root/v4/target/debug/dvandva-v4" "$binary"

workspace="$test_root/workspace"
mkdir -p "$workspace"
git -C "$workspace" init --quiet
git -C "$workspace" remote add origin git@github.com:axatbhardwaj/Dvandva.git

vadi="$repo_root/skills/vadi/scripts/dvandva-role.sh"
prativadi="$repo_root/skills/prativadi/scripts/dvandva-role.sh"
runs="$XDG_STATE_HOME/dvandva/runs"

field() { python3 -c 'import json,sys; v=json.load(sys.stdin); print(eval("v"+sys.argv[1]))' "$1"; }
now_ms() { date +%s%3N; }

apply() {
  local facade="$1" session="$2" run_dir="$3" revision="$4" payload="$5" action
  action="$(mktemp "$test_root/action.XXXXXX")"
  printf '%s\n' "$payload" >"$action"
  bash "$facade" apply "$session" "$run_dir" "$revision" "$action"
  local status=$?
  rm -f -- "$action"
  return "$status"
}

# Apply against whatever the head is right now. A peer polling in the
# background renews its lease, which moves the revision, so a fixed revision
# can lose the race; a role reads a fresh snapshot and retries.
apply_fresh() {
  local facade="$1" session="$2" run_dir="$3" payload="$4" attempt revision
  for attempt in 1 2 3 4 5 6; do
    revision="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' "$run_dir/baton.json")"
    if apply "$facade" "$session" "$run_dir" "$revision" "$payload"; then
      return 0
    fi
    sleep 0.2
  done
  return 1
}

# A short worker lease makes renewal observable.
started="$(DVANDVA_LEASE_SECONDS=4 bash "$vadi" start worker-session claude codex "$workspace" \
  "Poll behaviour" TASK-POLL --required-deliverable kernel="Poll behaviour")"
run_id="$(field '["run_id"]' <<<"$started")"
run_dir="$runs/$run_id"
joined="$(bash "$prativadi" start reviewer-session codex claude "$workspace" --run-id "$run_id")"
test "$(field '["revision"]' <<<"$joined")" = 2

# The worker hands off a checkpoint, so it has nothing to do but wait.
digest="$(printf 'a%.0s' $(seq 64))"
handed="$(apply "$vadi" worker-session "$run_dir" 2 \
  "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$(python3 -c 'import hashlib; print(hashlib.sha256(("a"*64).encode()).hexdigest())')\",\"deliverables\":[{\"id\":\"kernel\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$digest\"}]}],\"verification\":[\"poll test\"]}}" 2>/dev/null || true)"
# The analysis digest was never staged; fall back to a git checkpoint so the
# handoff is about the wait, not about staging.
if ! grep -q '"status": "reviewing"' <<<"$handed"; then
  commit="$(printf 'b%.0s' $(seq 40))"
  handed="$(apply "$vadi" worker-session "$run_dir" 2 \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$commit\",\"deliverables\":[{\"id\":\"kernel\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$commit\"}]}],\"verification\":[\"poll test\"]}}")"
fi
test "$(field '["status"]' <<<"$handed")" = reviewing
revision="$(field '["revision"]' <<<"$handed")"

# Each handoff opens an explainer obligation that vadi alone owes, so the
# worker is still actionable right after the checkpoint. Discharge it here so
# the wait below is about polling rather than about pending staging work.
printf '%s\n' '<!doctype html><title>poll</title>' >"$test_root/poll.html"
printf '%s' "$handed" >"$test_root/handed.json"
python3 - "$test_root/handed.json" "$test_root/poll.html" "$test_root/stage.json" <<'STAGE'
import json, sys
binding = json.load(open(sys.argv[1]))["publication_binding"]
json.dump({
    "type": "stage_explainer",
    "obligation": binding["obligation"],
    "after_seq": binding["receipt_seq"],
    "source_path": sys.argv[2],
}, open(sys.argv[3], "w"))
STAGE
staged="$(apply "$vadi" worker-session "$run_dir" "$revision" "$(cat "$test_root/stage.json")")"
test "$(field '["actionable"]' <<<"$staged")" = False
revision="$(field '["revision"]' <<<"$staged")"

# 1. Idle re-entry and the MAX_MS budget: with a 300ms kernel chunk and a
#    1500ms budget, poll re-enters several times and returns idle at the budget.
before="$(now_ms)"
trace="$test_root/poll.trace"
idle="$(DVANDVA_POLL_TRACE="$trace" DVANDVA_POLL_CHUNK_MS=300 DVANDVA_LEASE_SECONDS=4 bash "$vadi" poll worker-session "$run_dir" "$revision" 1500)"
elapsed=$(( $(now_ms) - before ))
test "$(field '["wait_outcome"]' <<<"$idle")" = idle_timeout
test "$(field '["actionable"]' <<<"$idle")" = False
test "$elapsed" -ge 1500 && test "$elapsed" -lt 6000 || { printf 'poll budget not honoured: %sms\n' "$elapsed" >&2; exit 1; }
# Re-entry is observed, not inferred: several kernel waits within one poll.
reentries="$(wc -l <"$trace")"
test "$reentries" -ge 3 || { printf 'poll re-entered only %s times\n' "$reentries" >&2; exit 1; }
grep -q 'timeout_ms=300' "$trace"

# The budget contract: MAX_MS must be a positive integer.
for bad in 0 -5 abc; do
  if bash "$vadi" poll worker-session "$run_dir" "$revision" "$bad" >/dev/null 2>&1; then
    printf 'poll accepted MAX_MS=%q\n' "$bad" >&2; exit 1
  fi
done

# Kernel failures propagate: a run that does not exist is an error, not a wait.
if out="$(bash "$vadi" poll worker-session "$runs/no-such-run" 0 500 2>&1)"; then
  printf 'poll on a missing run succeeded\n' >&2; exit 1
fi
grep -q '"error"' <<<"$out"

# 2. Lease renewal across the wait: the 4s worker lease survives a 6s poll.
lease_before="$(field '["participants"]["worker"]["claim"]["lease_expires_at"]' <<<"$idle")"
renewed="$(DVANDVA_POLL_CHUNK_MS=500 DVANDVA_LEASE_SECONDS=4 bash "$vadi" poll worker-session "$run_dir" "$(field '["revision"]' <<<"$idle")" 6000)"
lease_after="$(field '["participants"]["worker"]["claim"]["lease_expires_at"]' <<<"$renewed")"
test "$lease_after" '>' "$lease_before" || { printf 'lease was not renewed across the poll\n' >&2; exit 1; }
bash "$vadi" read worker-session "$run_dir" >/dev/null

# 3. Actionable wake: the peer acts while the worker is polling.
revision="$(field '["revision"]' <<<"$renewed")"
DVANDVA_POLL_CHUNK_MS=500 DVANDVA_LEASE_SECONDS=4 bash "$vadi" poll worker-session "$run_dir" "$revision" 20000 >"$test_root/woke.json" &
poller=$!
sleep 1
binding="$(bash "$prativadi" read reviewer-session "$run_dir")"
apply_fresh "$prativadi" reviewer-session "$run_dir" \
  "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$(field '["checkpoint"]["identity"]' <<<"$binding")\",\"manifest_digest\":\"$(field '["checkpoint"]["manifest_digest"]' <<<"$binding")\",\"scope_revision\":0,\"findings\":[\"wake the worker\"]}" >/dev/null
wait "$poller"
test "$(field '["wait_outcome"]' <"$test_root/woke.json")" = actionable
test "$(field '["status"]' <"$test_root/woke.json")" = revising
test "$(field '["actionable"]' <"$test_root/woke.json")" = True

# 4. Terminal return: the worker hands off again, so it is the one waiting,
#    and the run is abandoned while it polls.
revision="$(field '["revision"]' <"$test_root/woke.json")"
commit_b="$(printf 'c%.0s' $(seq 40))"
handed="$(apply "$vadi" worker-session "$run_dir" "$revision" \
  "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$commit_b\",\"deliverables\":[{\"id\":\"kernel\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$commit_b\"}]}],\"verification\":[\"poll test\"]}}")"
test "$(field '["status"]' <<<"$handed")" = reviewing
revision="$(field '["revision"]' <<<"$handed")"

# Discharge this handoff's explainer obligation too, so the worker is waiting
# on the peer rather than on its own staging work.
printf '%s' "$handed" >"$test_root/handed-b.json"
python3 - "$test_root/handed-b.json" "$test_root/poll.html" "$test_root/stage-b.json" <<'STAGE'
import json, sys
binding = json.load(open(sys.argv[1]))["publication_binding"]
json.dump({
    "type": "stage_explainer",
    "obligation": binding["obligation"],
    "after_seq": binding["receipt_seq"],
    "source_path": sys.argv[2],
}, open(sys.argv[3], "w"))
STAGE
staged="$(apply "$vadi" worker-session "$run_dir" "$revision" "$(cat "$test_root/stage-b.json")")"
test "$(field '["actionable"]' <<<"$staged")" = False
revision="$(field '["revision"]' <<<"$staged")"
DVANDVA_POLL_CHUNK_MS=500 DVANDVA_LEASE_SECONDS=4 bash "$vadi" poll worker-session "$run_dir" "$revision" 20000 >"$test_root/terminal.json" &
poller=$!
sleep 1
apply_fresh "$prativadi" reviewer-session "$run_dir" '{"type":"abandon","reason":"poll test complete"}' >/dev/null
wait "$poller"
test "$(field '["wait_outcome"]' <"$test_root/terminal.json")" = terminal
test "$(field '["status"]' <"$test_root/terminal.json")" = abandoned

printf 'poll behaviour: ok\n'
