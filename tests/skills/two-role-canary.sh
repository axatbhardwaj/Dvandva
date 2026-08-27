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
  install --version 0.2.0 >/dev/null

for role in vadi prativadi; do
  cmp "$repo_root/skills/$role/scripts/dvandva-role.sh" \
    "$HOME/.agents/skills/$role/scripts/dvandva-role.sh"
  cmp "$repo_root/skills/$role/scripts/dvandva-role.sh" \
    "$HOME/.claude/skills/$role/scripts/dvandva-role.sh"
done

workspace="$test_root/workspace"
mkdir -p "$workspace"
git -C "$workspace" init --quiet
git -C "$workspace" remote add origin git@github.com:axatbhardwaj/Dvandva.git

fakebin="$test_root/fakebin"
mkdir -p "$fakebin"
for peer in claude codex; do
  printf '#!/usr/bin/env bash\ntouch "%s"\nexit 99\n' "$test_root/peer-launched" \
    >"$fakebin/$peer"
  chmod 755 "$fakebin/$peer"
done
export PATH="$fakebin:$PATH"

apply_action() {
  local facade="$1" session="$2" run_dir="$3" revision="$4" name="$5" payload="$6"
  local action_dir="$test_root/actions" action
  mkdir -p "$action_dir"
  chmod 700 "$action_dir"
  action="$action_dir/$name.json"
  printf '%s\n' "$payload" >"$action"
  chmod 600 "$action"
  bash "$facade" apply "$session" "$run_dir" "$revision" "$action"
  rm -f -- "$action"
}

obligation_json() {
  python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["publication_binding"]["obligation"],separators=(",",":")))' "$1/baton.json"
}

approve_explainer() {
  local publisher="$1" publisher_session="$2" reviewer="$3" reviewer_session="$4"
  local run_dir="$5" revision="$6" site_id="$7" site_version="$8"
  local obligation source_digest url published deployment reviewed
  obligation="$(obligation_json "$run_dir")"
  source_digest="$(printf '%064d' "$revision")"
  url="https://sites.openai.test/$site_id/$site_version"
  published="$(apply_action "$publisher" "$publisher_session" "$run_dir" "$revision" \
    "publish-$site_version" \
    "{\"type\":\"record_explainer_publication\",\"obligation\":$obligation,\"source_digest\":\"$source_digest\",\"site_id\":\"$site_id\",\"site_version\":\"$site_version\",\"url\":\"$url\",\"channel\":\"codex_sites\",\"access\":\"owner_only\"}")"
  python3 -c 'import json,sys; assert json.load(sys.stdin)["publication_binding"]["deployment"]["publisher_harness"] == "Codex"' <<<"$published"
  deployment="$(python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["publication_binding"]["deployment"],separators=(",",":")))' <<<"$published")"
  reviewed="$(apply_action "$reviewer" "$reviewer_session" "$run_dir" "$((revision + 1))" \
    "review-$site_version" \
    "{\"type\":\"record_explainer_review\",\"obligation\":$obligation,\"source_digest\":\"$source_digest\",\"site_id\":\"$site_id\",\"site_version\":\"$site_version\",\"url\":\"$url\",\"verdict\":\"approved\",\"findings\":[]}")"
  python3 -c 'import json,sys; binding=json.load(sys.stdin)["publication_binding"]; review=binding["review"]; deployment=binding["deployment"]; assert review["reviewer_harness"] == "Claude"; assert all(review[key] == deployment[key] for key in ["source_digest","site_id","site_version","url"])' <<<"$reviewed"
  test -n "$deployment"
}

run_casting() {
  local label="$1" worker="$2" reviewer="$3" worker_harness="$4" reviewer_harness="$5"
  local worker_session="$label-worker" reviewer_session="$label-reviewer"
  local objective="Implement $label" task="TASK-$label" site_id="site-$label"
  local peer_for_worker peer_for_reviewer started joined run_id run_dir checkpoint_a checkpoint_b digest_a digest_b
  case "$worker_harness" in codex) peer_for_worker=claude ;; claude) peer_for_worker=codex ;; esac
  case "$reviewer_harness" in codex) peer_for_reviewer=claude ;; claude) peer_for_reviewer=codex ;; esac

  started="$(bash "$worker" start "$worker_session" "$worker_harness" "$peer_for_worker" \
    "$workspace" "$objective" "$task" --required-deliverable implementation="$objective")"
  run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  joined="$(bash "$reviewer" start "$reviewer_session" "$reviewer_harness" \
    "$peer_for_reviewer" "$workspace" --run-id "$run_id")"
  grep -Fq "\"run_id\": \"$run_id\"" <<<"$joined"

  local publisher publisher_session explainer_reviewer explainer_reviewer_session
  if test "$worker_harness" = codex; then
    publisher="$worker"; publisher_session="$worker_session"
    explainer_reviewer="$reviewer"; explainer_reviewer_session="$reviewer_session"
  else
    publisher="$reviewer"; publisher_session="$reviewer_session"
    explainer_reviewer="$worker"; explainer_reviewer_session="$worker_session"
  fi

  approve_explainer "$publisher" "$publisher_session" "$explainer_reviewer" \
    "$explainer_reviewer_session" "$run_dir" 2 "$site_id" deployment-1
  checkpoint_a="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa${label: -1}"
  checkpoint_b="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb${label: -1}"
  reviewing_a="$(apply_action "$worker" "$worker_session" "$run_dir" 4 checkpoint-a-$label \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$checkpoint_a\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$checkpoint_a\"}]}],\"verification\":[\"cargo test\"]}}")"
  digest_a="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing_a")"
  approve_explainer "$publisher" "$publisher_session" "$explainer_reviewer" \
    "$explainer_reviewer_session" "$run_dir" 5 "$site_id" deployment-2
  apply_action "$reviewer" "$reviewer_session" "$run_dir" 7 changes-$label \
    "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$checkpoint_a\",\"manifest_digest\":\"$digest_a\",\"scope_revision\":0,\"findings\":[\"Add contention coverage\"]}" >/dev/null
  approve_explainer "$publisher" "$publisher_session" "$explainer_reviewer" \
    "$explainer_reviewer_session" "$run_dir" 8 "$site_id" deployment-3
  reviewing_b="$(apply_action "$worker" "$worker_session" "$run_dir" 10 checkpoint-b-$label \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$checkpoint_b\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$checkpoint_b\"}]}],\"verification\":[\"cargo test\",\"contention test\"]}}")"
  digest_b="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing_b")"
  approve_explainer "$publisher" "$publisher_session" "$explainer_reviewer" \
    "$explainer_reviewer_session" "$run_dir" 11 "$site_id" deployment-4
  apply_action "$reviewer" "$reviewer_session" "$run_dir" 13 approve-$label \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$checkpoint_b\",\"manifest_digest\":\"$digest_b\",\"scope_revision\":0,\"findings\":[]}" >/dev/null
  approve_explainer "$publisher" "$publisher_session" "$explainer_reviewer" \
    "$explainer_reviewer_session" "$run_dir" 14 "$site_id" deployment-5
  terminal="$(apply_action "$worker" "$worker_session" "$run_dir" 16 finalize-$label \
    '{"type":"finalize"}')"
  grep -Fq '"status": "done"' <<<"$terminal"
  grep -Fq "\"identity\": \"$checkpoint_b\"" <<<"$terminal"

  test "$(grep -Rho '"site_id": "[^"]*"' "$run_dir" | sort -u)" = \
    "\"site_id\": \"$site_id\""
  grep -Rq '"publisher_harness": "Codex"' "$run_dir/history"
  grep -Rq '"reviewer_harness": "Claude"' "$run_dir/history"
  bash "$worker" wait "$worker_session" "$run_dir" 16 500 | grep -Fq '"status": "done"'
  bash "$reviewer" wait "$reviewer_session" "$run_dir" 16 500 | grep -Fq '"status": "done"'
}

# Normal semantic casting: Codex vadi publishes, Claude prativadi reviews.
run_casting normal \
  "$HOME/.agents/skills/vadi/scripts/dvandva-role.sh" \
  "$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh" codex claude

# Reverse semantic casting: Claude vadi works/reviews the Site; Codex prativadi publishes.
run_casting reverse \
  "$HOME/.claude/skills/vadi/scripts/dvandva-role.sh" \
  "$HOME/.agents/skills/prativadi/scripts/dvandva-role.sh" claude codex

test ! -e "$test_root/peer-launched"
printf 'two-role skill canary: ok\n'
