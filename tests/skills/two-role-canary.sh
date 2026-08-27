#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
release_dir="$test_root/release"
mkdir -p "$release_dir"
cp "$repo_root/v4/target/debug/dvandva-v4" \
  "$release_dir/dvandva-kernel-linux-x86_64"
(cd "$release_dir" && sha256sum dvandva-kernel-linux-x86_64 >SHA256SUMS)

export HOME="$test_root/home"
export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
export DVANDVA_RELEASE_DIR="$release_dir"
export DVANDVA_WAIT_TIMEOUT_MS=2000
mkdir -p "$HOME"

npx --yes skills add "$repo_root" --copy --global \
  --agent claude-code codex --skill setup-dvandva vadi prativadi -y >/dev/null
bash "$HOME/.agents/skills/setup-dvandva/scripts/setup-dvandva.sh" \
  install --version 0.1.0 >/dev/null

vadi="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
prativadi="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
test "$vadi" != "$prativadi"
workspace="$test_root/workspace"
mkdir -p "$workspace"
git -C "$workspace" init --quiet
git -C "$workspace" remote add origin git@github.com:axatbhardwaj/Dvandva.git

fakebin="$test_root/fakebin"
mkdir -p "$fakebin"
for peer in claude codex; do
  printf '#!/usr/bin/env bash\ntouch "%s"\nexit 99\n' "$test_root/peer-launched" \
    >"$fakebin/$peer"
  chmod +x "$fakebin/$peer"
done
export PATH="$fakebin:$PATH"

reviewer_a_out="$test_root/reviewer-a.out"
reviewer_b_out="$test_root/reviewer-b.out"
bash "$prativadi" start reviewer-a claude codex "$workspace" \
  'Implement DEF-123' DEF-123 --wait >"$reviewer_a_out" 2>"$test_root/reviewer-a.err" &
reviewer_a_pid=$!
bash "$prativadi" start reviewer-b claude codex "$workspace" \
  'Implement DEF-123' DEF-123 --wait >"$reviewer_b_out" 2>"$test_root/reviewer-b.err" &
reviewer_b_pid=$!

worker="$(bash "$vadi" start worker-a codex claude "$workspace" \
  'Implement DEF-123' DEF-123)"
wait "$reviewer_a_pid"
wait "$reviewer_b_pid"

started_count="$(grep -l '"outcome": "started"' "$reviewer_a_out" "$reviewer_b_out" | wc -l)"
test "$started_count" = "1"
none_count="$(grep -l '"outcome": "none"' "$reviewer_a_out" "$reviewer_b_out" | wc -l)"
test "$none_count" = "1"
if grep -Fq '"outcome": "started"' "$reviewer_a_out"; then
  reviewer_session="reviewer-a"
  reviewer_out="$reviewer_a_out"
else
  reviewer_session="reviewer-b"
  reviewer_out="$reviewer_b_out"
fi

run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$worker")"
run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
grep -Fq "\"run_id\": \"$run_id\"" "$reviewer_out"

apply_action() {
  local facade="$1"
  local session="$2"
  local directory="$3"
  local revision="$4"
  local name="$5"
  local payload="$6"
  local action_dir="$test_root/actions"
  mkdir -p "$action_dir"
  chmod 700 "$action_dir"
  local action="$action_dir/$name.json"
  printf '%s\n' "$payload" >"$action"
  chmod 600 "$action"
  local status=0
  bash "$facade" apply "$session" "$directory" "$revision" "$action" || status=$?
  rm -f -- "$action"
  return "$status"
}

apply_action "$vadi" worker-a "$run_dir" 2 publication-required \
  '{"type":"record_publication","required":true,"desired_revision":2,"published_revision":null,"refs":[]}' \
  >/dev/null
apply_action "$vadi" worker-a "$run_dir" 3 checkpoint-a \
  '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","verification":["cargo test"]}}' \
  >/dev/null
apply_action "$prativadi" "$reviewer_session" "$run_dir" 4 changes \
  '{"type":"record_review","verdict":"changes_requested","checkpoint_identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","findings":["Add the missing contention test"]}' \
  >/dev/null
apply_action "$vadi" worker-a "$run_dir" 5 checkpoint-b \
  '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","verification":["cargo test","contention test"]}}' \
  >/dev/null
apply_action "$prativadi" "$reviewer_session" "$run_dir" 6 approve \
  '{"type":"record_review","verdict":"approved","checkpoint_identity":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","findings":[]}' \
  >/dev/null

if apply_action "$vadi" worker-a "$run_dir" 7 premature-finalize \
  '{"type":"finalize"}' >"$test_root/premature.out" 2>"$test_root/premature.err"; then
  printf 'finalization unexpectedly bypassed publication\n' >&2
  exit 1
fi
grep -Fq 'publication_stale' "$test_root/premature.err"

apply_action "$vadi" worker-a "$run_dir" 7 publication-synced \
  '{"type":"record_publication","required":true,"desired_revision":7,"published_revision":7,"refs":[{"kind":"explainer","value":"https://example.test/def-123"}]}' \
  >/dev/null
terminal="$(apply_action "$vadi" worker-a "$run_dir" 8 finalize '{"type":"finalize"}')"
grep -Fq '"status": "done"' <<<"$terminal"
bash "$vadi" wait worker-a "$run_dir" 8 500 | grep -Fq '"status": "done"'
bash "$prativadi" wait "$reviewer_session" "$run_dir" 8 500 |
  grep -Fq '"status": "done"'

for credential in \
  "$XDG_STATE_HOME/dvandva/credentials/worker-a/$run_id/worker.json" \
  "$XDG_STATE_HOME/dvandva/credentials/$reviewer_session/$run_id/reviewer.json"; do
  test "$(stat -c '%a' "$credential")" = "600"
  token="$(sed -n 's/.*"token": "\([^"]*\)".*/\1/p' "$credential")"
  test -n "$token"
  ! grep -R -Fq -- "$token" "$run_dir" "$test_root"/*.out "$test_root"/*.err
done

reverse_worker="$(bash "$vadi" start worker-b claude codex "$workspace" \
  'Implement DEF-456' DEF-456)"
reverse_run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$reverse_worker")"
reverse_run_dir="$XDG_STATE_HOME/dvandva/runs/$reverse_run_id"
reverse_reviewer="$(bash "$prativadi" start reviewer-c codex claude "$workspace" \
  'Implement DEF-456' DEF-456)"
grep -Fq "\"run_id\": \"$reverse_run_id\"" <<<"$reverse_reviewer"

apply_action "$vadi" worker-b "$reverse_run_dir" 2 reverse-publication-required \
  '{"type":"record_publication","required":true,"desired_revision":2,"published_revision":null,"refs":[]}' \
  >/dev/null
apply_action "$vadi" worker-b "$reverse_run_dir" 3 reverse-checkpoint \
  '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"cccccccccccccccccccccccccccccccccccccccc","verification":["test"]}}' \
  >/dev/null
apply_action "$prativadi" reviewer-c "$reverse_run_dir" 4 reverse-approve \
  '{"type":"record_review","verdict":"approved","checkpoint_identity":"cccccccccccccccccccccccccccccccccccccccc","findings":[]}' \
  >/dev/null
apply_action "$vadi" worker-b "$reverse_run_dir" 5 reverse-publication \
  '{"type":"record_publication","required":true,"desired_revision":5,"published_revision":5,"refs":[{"kind":"explainer","value":"https://example.test/def-456"}]}' \
  >/dev/null
apply_action "$vadi" worker-b "$reverse_run_dir" 6 reverse-finalize \
  '{"type":"finalize"}' | grep -Fq '"status": "done"'

test ! -e "$test_root/peer-launched"
printf 'two-role skill canary: ok\n'
