#!/usr/bin/env bash
set -euo pipefail
shopt -s inherit_errexit
# Byte-order collation: filename comparisons below must not depend on the
# invoking user's locale.
export LC_ALL=C

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
  --agent claude-code codex --skill setup-dvandva vadi prativadi html-deliverables -y >/dev/null
bash "$HOME/.agents/skills/setup-dvandva/scripts/setup-dvandva.sh" \
  install --version 0.3.9 >/dev/null

for role in vadi prativadi; do
  for host_skills in "$HOME/.agents/skills" "$HOME/.claude/skills"; do
    cmp "$repo_root/skills/$role/scripts/discover.py" "$host_skills/$role/scripts/discover.py"
    cmp "$repo_root/skills/$role/scripts/checkpoint_guard.py" \
      "$host_skills/$role/scripts/checkpoint_guard.py"
    cmp "$repo_root/skills/$role/scripts/review_guard.py" \
      "$host_skills/$role/scripts/review_guard.py"
    cmp "$repo_root/skills/$role/scripts/role_guard.py" \
      "$host_skills/$role/scripts/role_guard.py"
    cmp "$repo_root/skills/$role/scripts/skill_metadata.py" \
      "$host_skills/$role/scripts/skill_metadata.py"
  done
  for reference in initiation discovery freeflow review; do
    cmp "$repo_root/skills/$role/references/$reference.md" \
      "$HOME/.agents/skills/$role/references/$reference.md"
    cmp "$repo_root/skills/$role/references/$reference.md" \
      "$HOME/.claude/skills/$role/references/$reference.md"
  done
  cmp "$repo_root/skills/$role/scripts/dvandva-role.sh" \
    "$HOME/.agents/skills/$role/scripts/dvandva-role.sh"
  cmp "$repo_root/skills/$role/scripts/dvandva-role.sh" \
    "$HOME/.claude/skills/$role/scripts/dvandva-role.sh"
done

# The restored companion must travel as a complete, identical skill in both hosts.
for host_skills in "$HOME/.agents/skills" "$HOME/.claude/skills"; do
  diff -r "$repo_root/skills/html-deliverables" "$host_skills/html-deliverables"
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

apply_action_error() {
  local facade="$1" session="$2" run_dir="$3" revision="$4" name="$5" payload="$6"
  local action_dir="$test_root/actions" action status
  mkdir -p "$action_dir"
  chmod 700 "$action_dir"
  action="$action_dir/$name.json"
  printf '%s\n' "$payload" >"$action"
  chmod 600 "$action"
  set +e
  bash "$facade" apply "$session" "$run_dir" "$revision" "$action" 2>&1
  status=$?
  set -e
  rm -f -- "$action"
  return "$status"
}

# An analysis identity is derived from the digests the manifest cites.
analysis_identity() {
  python3 -c 'import hashlib,sys; print(hashlib.sha256("\n".join(sorted(set(sys.argv[1:]))).encode()).hexdigest())' "$@"
}

receipt_seq() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["publication_binding"].get("receipt_seq", 0))' "$1/baton.json"
}

rev() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' "$1/baton.json"
}

fixture_commit() {
  local label="$1"
  git -C "$workspace" -c user.name=Canary -c user.email=canary@example.invalid \
    commit --quiet --allow-empty -m "$label"
  git -C "$workspace" rev-parse HEAD
}

# Stage the bytes behind an analysis deliverable and echo their digest, so the
# manifest cites something the reviewer can materialize.
stage_analysis() {
  local facade="$1" session="$2" run_dir="$3" label="$4" source="${5:-}" staged
  if test -z "$source"; then
    source="$test_root/analysis-$label.md"
    printf '# %s\nanalysis deliverable\n' "$label" >"$source"
  else
    test -f "$source"
  fi
  local digest
  # staged_analysis is a sorted set, so the digest is computed here rather than
  # read back positionally.
  digest="$(sha256sum "$source" | cut -d' ' -f1)"
  staged="$(apply_action "$facade" "$session" "$run_dir" "$(rev "$run_dir")" \
    "stage-analysis-$label" \
    "{\"type\":\"stage_analysis\",\"source_path\":\"$source\"}")"
  python3 -c 'import json,sys; assert sys.argv[1] in json.load(sys.stdin)["staged_analysis"]' \
    "$digest" <<<"$staged"
  printf '%s\n' "$digest"
}

# Build the staged-digest identity and complete deliverable manifest once. Each
# remaining argument is ID=SOURCE_PATH; callers submit the returned JSON through
# the public role facade rather than duplicating checkpoint assembly logic.
stage_analysis_manifest() {
  local facade="$1" session="$2" run_dir="$3" label="$4"
  shift 4
  local item id source staged
  local -a ids=() digests=()
  for item in "$@"; do
    id="${item%%=*}"
    source="${item#*=}"
    test -n "$id" && test "$source" != "$item" && test -f "$source"
    staged="$(stage_analysis "$facade" "$session" "$run_dir" "$label-$id" "$source")"
    ids+=("$id")
    digests+=("$staged")
  done
  python3 - "${#ids[@]}" "${ids[@]}" "${digests[@]}" <<'PY'
import hashlib, json, sys

count = int(sys.argv[1])
ids = sys.argv[2:2 + count]
digests = sys.argv[2 + count:]
assert count and len(digests) == count and len(set(ids)) == count
identity = hashlib.sha256("\n".join(sorted(set(digests))).encode()).hexdigest()
deliverables = [
    {"id": item_id, "artifacts": [{"kind": "analysis_digest", "value": digest}]}
    for item_id, digest in zip(ids, digests)
]
print(json.dumps({
    "identity": identity,
    "digests": digests,
    "deliverables": deliverables,
}, separators=(",", ":")))
PY
}

obligation_json() {
  python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["publication_binding"]["obligation"],separators=(",",":")))' "$1/baton.json"
}

# Vadi stages digest-bound bytes, prativadi approves those exact local bytes,
# then whichever participant is Codex records the matching owner-only Sites
# deployment receipt.
approve_explainer() {
  local author="$1" author_session="$2" reviewer="$3" reviewer_session="$4"
  local sites_publisher="$5" sites_session="$6" run_dir="$7" revision="$8"
  local site_id="$9" site_version="${10}"
  local obligation source staged source_digest relayed reviewed published
  obligation="$(obligation_json "$run_dir")"
  source="$test_root/explainer-$site_id-$site_version.html"
  printf '<h1>%s %s</h1>\n' "$site_id" "$site_version" >"$source"
  staged="$(apply_action "$author" "$author_session" "$run_dir" "$revision" \
    "stage-$site_version" \
    "{\"type\":\"stage_explainer\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_path\":\"$source\"}")"
  python3 -c 'import json,sys; baton=json.load(sys.stdin); artifact=baton["publication_binding"]["artifact"]; assert artifact["publisher_harness"] == baton["participants"]["worker"]["harness"]; assert artifact["channel"] == "run_artifact"; assert artifact["access"] == "run_private"' <<<"$staged"
  source_digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["publication_binding"]["artifact"]["source_digest"])' <<<"$staged")"

  # The reviewing harness must be able to read the bytes it is about to approve.
  relayed="$(bash "$reviewer" explainer "$reviewer_session" "$run_dir")"
  python3 -c 'import json,sys; staged=json.load(sys.stdin); assert staged["source_digest"] == sys.argv[1]; assert staged["contents"].strip() == sys.argv[2]' <<<"$relayed" \
    "$source_digest" "$(cat "$source")"

  reviewed="$(apply_action "$reviewer" "$reviewer_session" "$run_dir" "$((revision + 1))" \
    "review-$site_version" \
    "{\"type\":\"record_explainer_review\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_digest\":\"$source_digest\",\"verdict\":\"approved\",\"findings\":[]}")"
  python3 -c 'import json,sys; baton=json.load(sys.stdin); binding=baton["publication_binding"]; review=binding["review"]; assert review["reviewer_harness"] == baton["participants"]["reviewer"]["harness"]; assert review["source_digest"] == binding["artifact"]["source_digest"]' <<<"$reviewed"
  published="$(apply_action "$sites_publisher" "$sites_session" "$run_dir" "$((revision + 2))" \
    "publish-$site_version" \
    "{\"type\":\"record_explainer_publication\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_digest\":\"$source_digest\",\"site_id\":\"$site_id\",\"site_version\":\"$site_version\",\"url\":\"https://sites.openai.test/$site_id/$site_version\",\"channel\":\"codex_sites\",\"access\":\"owner_only\"}")"
  python3 -c 'import json,sys; binding=json.load(sys.stdin)["publication_binding"]; deployment=binding["deployment"]; assert deployment["publisher_harness"] == "Codex"; assert deployment["source_digest"] == binding["artifact"]["source_digest"]; assert deployment["channel"] == "codex_sites"; assert deployment["access"] == "owner_only"' <<<"$published"
  test -n "$source_digest"
}

run_casting() {
  local label="$1" worker="$2" reviewer="$3" worker_harness="$4" reviewer_harness="$5"
  local worker_session="$label-worker" reviewer_session="$label-reviewer"
  local objective="Implement $label" task="TASK-$label" site_id="site-$label"
  local peer_for_worker peer_for_reviewer started joined run_id run_dir checkpoint_a checkpoint_b
  local digest_a digest_b reviewing_a reviewing_b terminal worker_wait reviewer_wait
  case "$worker_harness" in codex) peer_for_worker=claude ;; claude) peer_for_worker=codex ;; esac
  case "$reviewer_harness" in codex) peer_for_reviewer=claude ;; claude) peer_for_reviewer=codex ;; esac

  started="$(bash "$worker" start "$worker_session" "$worker_harness" "$peer_for_worker" \
    "$workspace" "$objective" "$task" --required-deliverable implementation="$objective")"
  run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  joined="$(bash "$reviewer" start "$reviewer_session" "$reviewer_harness" \
    "$peer_for_reviewer" "$workspace" --run-id "$run_id")"
  grep -Fq "\"run_id\": \"$run_id\"" <<<"$joined"

  local sites_publisher sites_session
  if test "$worker_harness" = codex; then
    sites_publisher="$worker"; sites_session="$worker_session"
  else
    sites_publisher="$reviewer"; sites_session="$reviewer_session"
  fi

  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 2 "$site_id" deployment-1
  checkpoint_a="$(fixture_commit "$label checkpoint A")"
  reviewing_a="$(apply_action "$worker" "$worker_session" "$run_dir" 5 checkpoint-a-$label \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$checkpoint_a\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$checkpoint_a\"}]}],\"verification\":[\"cargo test\"]}}")"
  digest_a="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing_a")"
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 6 "$site_id" deployment-2
  apply_action "$reviewer" "$reviewer_session" "$run_dir" 9 changes-$label \
    "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$checkpoint_a\",\"manifest_digest\":\"$digest_a\",\"scope_revision\":0,\"findings\":[\"Add contention coverage\"]}" >/dev/null
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 10 "$site_id" deployment-3
  checkpoint_b="$(fixture_commit "$label checkpoint B")"
  reviewing_b="$(apply_action "$worker" "$worker_session" "$run_dir" 13 checkpoint-b-$label \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$checkpoint_b\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$checkpoint_b\"}]}],\"verification\":[\"cargo test\",\"contention test\"]}}")"
  digest_b="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing_b")"
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 14 "$site_id" deployment-4
  apply_action "$reviewer" "$reviewer_session" "$run_dir" 17 approve-$label \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$checkpoint_b\",\"manifest_digest\":\"$digest_b\",\"scope_revision\":0,\"findings\":[]}" >/dev/null
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 18 "$site_id" deployment-5
  terminal="$(apply_action "$worker" "$worker_session" "$run_dir" 21 finalize-$label \
    '{"type":"finalize"}')"
  grep -Fq '"status": "done"' <<<"$terminal"
  grep -Fq "\"identity\": \"$checkpoint_b\"" <<<"$terminal"

  # Every staged explainer is content-addressed, and the head's recorded digest
  # names bytes that are actually on disk and actually hash to it.
  test "$(ls "$run_dir/explainer" | wc -l)" -ge 1
  python3 - "$run_dir" <<'CHECK'
import hashlib, json, sys
from pathlib import Path

run_dir = Path(sys.argv[1])
baton = json.loads((run_dir / "baton.json").read_text())
artifact = baton["publication_binding"]["artifact"]
bytes_on_disk = (run_dir / artifact["path"]).read_bytes()
assert hashlib.sha256(bytes_on_disk).hexdigest() == artifact["source_digest"]
assert len(bytes_on_disk) == artifact["byte_length"]
for staged in sorted((run_dir / "explainer").iterdir()):
    actual = hashlib.sha256(staged.read_bytes()).hexdigest()
    if staged.stem != actual:
        raise SystemExit(f"staged {staged.name} hashes to {actual}")
CHECK
  worker_wait="$(bash "$worker" wait "$worker_session" "$run_dir" 21 500)"
  reviewer_wait="$(bash "$reviewer" wait "$reviewer_session" "$run_dir" 21 500)"
  python3 -c '
import json, sys
worker, reviewer = (json.loads(value) for value in sys.stdin.read().split("\n---\n"))
expected = {"identity": sys.argv[1], "manifest_digest": sys.argv[2], "scope_revision": 0}
assert worker["status"] == reviewer["status"] == "done"
assert {key: worker["checkpoint"][key] for key in expected} == expected
assert {key: reviewer["checkpoint"][key] for key in expected} == expected
assert worker["checkpoint"] == reviewer["checkpoint"]
' "$checkpoint_b" "$digest_b" <<<"$worker_wait
---
$reviewer_wait"
}

run_supersession_incident() {
  local worker="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
  local worker_session="incident-worker" reviewer_session="incident-reviewer"
  local objective="Review architecture and module reuse" task="TASK-incident"
  local site_id="site-incident" started mismatch joined run_id run_dir revision
  local checkpoint_a checkpoint_b checkpoint_b_extra identity_a identity_b
  local reviewing_a reviewing_b digest_a digest_b request accepted terminal
  local failure_output failure_status worker_wait reviewer_wait

  started="$(bash "$worker" start "$worker_session" codex claude "$workspace" \
    "$objective" "$task" \
    --required-deliverable review-package="Complete review package")"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' \
    <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  python3 -c '
import json, sys
started = json.load(sys.stdin)
run_id = sys.argv[1]
assert started["revision"] == 1
assert started["participants"]["worker"]["harness"] == "Codex"
assert started["participants"]["reviewer"]["harness"] == "Claude"
assert started["peer_prompt"] == f"Act as prativadi and join Dvandva run {run_id}."
' "$run_id" <<<"$started"

  mismatch="$(bash "$reviewer" start "$reviewer_session" claude codex \
    "$workspace" "Conflicting review scope" "TASK-other" \
    --required-deliverable review-package="Different package" --run-id "$run_id")"
  python3 -c '
import json, sys
mismatch = json.load(sys.stdin)
assert mismatch["outcome"] == "scope_mismatch"
assert mismatch["candidates"][0]["run_id"] == sys.argv[1]
' "$run_id" <<<"$mismatch"
  revision="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' \
    "$run_dir/baton.json")"
  test "$revision" = 1

  joined="$(bash "$reviewer" start "$reviewer_session" claude codex \
    "$workspace" --run-id "$run_id")"
  python3 -c 'import json,sys; joined=json.load(sys.stdin); assert joined["run_id"] == sys.argv[1] and joined["revision"] == 2' \
    "$run_id" <<<"$joined"
  approve_explainer "$worker" "$worker_session" "$reviewer" \
    "$reviewer_session" "$worker" "$worker_session" "$run_dir" 2 "$site_id" incident-1

  checkpoint_a="$(stage_analysis "$worker" "$worker_session" "$run_dir" review)"
  identity_a="$(analysis_identity "$checkpoint_a")"
  reviewing_a="$(apply_action "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" incident-a \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity_a\",\"deliverables\":[{\"id\":\"review-package\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$checkpoint_a\"}]}],\"verification\":[\"review.md checked\"]}}")"
  digest_a="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' \
    <<<"$reviewing_a")"
  approve_explainer "$worker" "$worker_session" "$reviewer" \
    "$reviewer_session" "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" "$site_id" incident-2

  request="$(apply_action "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" incident-request \
    '{"type":"request_checkpoint_supersession","reason":"Required reuse analysis is absent"}')"
  python3 -c 'import json,sys; baton=json.load(sys.stdin); assert baton["pending_checkpoint_supersession"]["reason"] == "Required reuse analysis is absent"' \
    <<<"$request"

  set +e
  failure_output="$(apply_action_error "$reviewer" "$reviewer_session" "$run_dir" "$(( $(rev "$run_dir") - 1 ))" \
    incident-stale-approval \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity_a\",\"manifest_digest\":\"$digest_a\",\"scope_revision\":0,\"findings\":[]}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"revision_conflict"' <<<"$failure_output"

  set +e
  failure_output="$(apply_action_error "$reviewer" "$reviewer_session" "$run_dir" "$(rev "$run_dir")" \
    incident-blocked-approval \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity_a\",\"manifest_digest\":\"$digest_a\",\"scope_revision\":0,\"findings\":[]}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"supersession_pending"' <<<"$failure_output"

  accepted="$(apply_action "$reviewer" "$reviewer_session" "$run_dir" "$(rev "$run_dir")" \
    incident-accept '{"type":"accept_checkpoint_supersession"}')"
  python3 -c 'import json,sys; baton=json.load(sys.stdin); assert baton["status"] == "revising" and baton["assignee"] == "worker"' \
    <<<"$accepted"
  approve_explainer "$worker" "$worker_session" "$reviewer" \
    "$reviewer_session" "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" "$site_id" incident-3

  checkpoint_b="$(stage_analysis "$worker" "$worker_session" "$run_dir" reuse)"
  checkpoint_b_extra="$(stage_analysis "$worker" "$worker_session" "$run_dir" reuse-extra)"
  identity_b="$(analysis_identity "$checkpoint_b" "$checkpoint_b_extra")"
  reviewing_b="$(apply_action "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" incident-b \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity_b\",\"deliverables\":[{\"id\":\"review-package\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$checkpoint_b\"},{\"kind\":\"analysis_digest\",\"value\":\"$checkpoint_b_extra\"}]}],\"verification\":[\"review.md checked\",\"reuse-analysis.md checked\"]}}")"
  digest_b="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' \
    <<<"$reviewing_b")"
  approve_explainer "$worker" "$worker_session" "$reviewer" \
    "$reviewer_session" "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" "$site_id" incident-4
  apply_action "$reviewer" "$reviewer_session" "$run_dir" "$(rev "$run_dir")" incident-approve-b \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity_b\",\"manifest_digest\":\"$digest_b\",\"scope_revision\":0,\"findings\":[]}" >/dev/null
  # Approval preserves the delivery obligation and its incident-4 receipts:
  # finalize follows directly in the same handshake.
  terminal="$(apply_action "$worker" "$worker_session" "$run_dir" "$(rev "$run_dir")" \
    incident-finalize '{"type":"finalize"}')"

  python3 - "$run_dir" "$site_id" "$identity_b" "$digest_b" <<'PY'
import hashlib, json, pathlib, sys
run_dir, _site_id, checkpoint, digest = sys.argv[1:]
receipts = []
seen = set()
for path in sorted(pathlib.Path(run_dir, "history").glob("*.json")):
    baton = json.loads(path.read_text())
    binding = baton.get("publication_binding") or {}
    obligation = binding.get("obligation") or {}
    review = binding.get("review")
    artifact = binding.get("artifact")
    handoff = obligation.get("handoff_revision")
    if review is not None and artifact is not None and handoff not in seen:
        seen.add(handoff)
        receipts.append((obligation, artifact, review))
assert [entry[0]["kind"] for entry in receipts] == [
    "run_started", "worker_to_reviewer", "checkpoint_superseded",
    "worker_to_reviewer",
]
# Each work-carrying handoff staged its own bytes — approval preserved the last
# delivery's receipts instead of opening a fresh obligation — each review bound
# exactly those bytes, and every digest still names readable content on disk.
assert len({entry[1]["source_digest"] for entry in receipts}) == 4
assert all(entry[1]["channel"] == "run_artifact" for entry in receipts)
assert all(entry[1]["access"] == "run_private" for entry in receipts)
baton = json.loads(pathlib.Path(run_dir, "baton.json").read_text())
assert all(entry[1]["publisher_harness"] == baton["participants"]["worker"]["harness"] for entry in receipts)
assert all(entry[2]["reviewer_harness"] == baton["participants"]["reviewer"]["harness"] for entry in receipts)
assert all(entry[2]["source_digest"] == entry[1]["source_digest"] for entry in receipts)
for _, artifact, _ in receipts:
    staged = pathlib.Path(run_dir, artifact["path"]).read_bytes()
    assert hashlib.sha256(staged).hexdigest() == artifact["source_digest"]
expected = {"identity": checkpoint, "manifest_digest": digest, "scope_revision": 0}
assert {key: baton["checkpoint"][key] for key in expected} == expected
assert baton["review"]["checkpoint_identity"] == checkpoint
assert baton["review"]["manifest_digest"] == digest
assert baton["review"]["scope_revision"] == 0
assert baton["terminal"] == {"outcome": "done", "reason": None}
assert baton["status"] == "done"
PY

  worker_wait="$(bash "$worker" wait "$worker_session" "$run_dir" 17 500)"
  reviewer_wait="$(bash "$reviewer" wait "$reviewer_session" "$run_dir" 17 500)"
  python3 -c '
import json, sys
terminal, worker, reviewer = (json.loads(value) for value in sys.stdin.read().split("\n---\n"))
checkpoint_bytes = [
    json.dumps(snapshot["checkpoint"], ensure_ascii=False, separators=(",", ":")).encode()
    for snapshot in (terminal, worker, reviewer)
]
assert checkpoint_bytes[0] == checkpoint_bytes[1] == checkpoint_bytes[2]
assert worker["terminal"] == reviewer["terminal"] == {"outcome": "done", "reason": None}
' <<<"$terminal
---
$worker_wait
---
$reviewer_wait"
}

# Normal semantic casting: Codex vadi publishes, Claude prativadi reviews.
run_casting normal \
  "$HOME/.agents/skills/vadi/scripts/dvandva-role.sh" \
  "$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh" codex claude

# Reverse semantic casting: Claude vadi works/reviews the Site; Codex prativadi publishes.
run_casting reverse \
  "$HOME/.claude/skills/vadi/scripts/dvandva-role.sh" \
  "$HOME/.agents/skills/prativadi/scripts/dvandva-role.sh" claude codex

# Original incident: exact scope mismatch, checkpoint supersession, and exact-B completion.
run_supersession_incident

# Discovery startup evidence must use changes_requested, then changed bytes and
# empty approval findings. Exercise the real facade, not just source wording.
run_discovery_startup() {
  local worker="$HOME/.claude/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.agents/skills/prativadi/scripts/dvandva-role.sh"
  local started run_id run_dir source obligation digest snapshot
  started="$(bash "$worker" start discovery-worker claude codex "$workspace" \
    'Discover a feature' --new-run --objective-ref workflow=discovery \
    --objective-ref discovery_stage=spec --required-deliverable spec='Reviewed spec')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start discovery-reviewer codex claude "$workspace" --run-id "$run_id" >/dev/null
  obligation="$(obligation_json "$run_dir")"
  source="$test_root/discovery-source.html"
  printf '<h1>Source manifest</h1><p>repo@fixed-revision</p>\n' >"$source"
  apply_action "$worker" discovery-worker "$run_dir" "$(rev "$run_dir")" discovery-stage \
    "{\"type\":\"stage_explainer\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_path\":\"$source\"}" >/dev/null
  digest="$(sha256sum "$source" | cut -d' ' -f1)"
  apply_action "$reviewer" discovery-reviewer "$run_dir" "$(rev "$run_dir")" discovery-research \
    "{\"type\":\"record_explainer_review\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_digest\":\"$digest\",\"verdict\":\"changes_requested\",\"findings\":[\"Incorporate independent evidence: existing interface supports the feature; ask about retention.\"]}" >/dev/null
  snapshot="$(bash "$worker" read discovery-worker "$run_dir")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert "work" not in s["advisory_actions"]' <<<"$snapshot"
  printf '<p>Independent evidence: existing interface supports the feature; retention is a human question. Vadi agrees after checking.</p>\n' >>"$source"
  apply_action "$worker" discovery-worker "$run_dir" "$(rev "$run_dir")" discovery-restage \
    "{\"type\":\"stage_explainer\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_path\":\"$source\"}" >/dev/null
  digest="$(sha256sum "$source" | cut -d' ' -f1)"
  apply_action "$reviewer" discovery-reviewer "$run_dir" "$(rev "$run_dir")" discovery-approve \
    "{\"type\":\"record_explainer_review\",\"obligation\":$obligation,\"after_seq\":$(receipt_seq "$run_dir"),\"source_digest\":\"$digest\",\"verdict\":\"approved\",\"findings\":[]}" >/dev/null
  snapshot="$(bash "$worker" read discovery-worker "$run_dir")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert "work" in s["advisory_actions"]' <<<"$snapshot"
  # Intentional invocation wait is durable progress, not terminal completion.
  apply_action "$worker" discovery-worker "$run_dir" "$(rev "$run_dir")" discovery-skill-wait \
    '{"type":"report_progress","phase":"waiting","detail":"waiting_for_skill: /to-spec; research reconciled"}' >/dev/null
  snapshot="$(bash "$worker" start discovery-worker claude codex "$workspace" --run-id "$run_id")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["run_id"] == sys.argv[1]' "$run_id" <<<"$snapshot"
  snapshot="$(bash "$worker" read discovery-worker "$run_dir")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "working"; assert s["participants"]["worker"]["progress"]["detail"].startswith("waiting_for_skill: /to-spec")' <<<"$snapshot"
}
run_discovery_startup

# Every Freeflow scope declares code or analysis delivery at initiation. The
# public role facade rejects missing/invalid markers and mismatched checkpoint
# kinds while retaining report-only analysis delivery.
run_checkpoint_kind_guard() {
  local worker="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
  local started run_id run_dir source artifact identity failure failure_status reviewing manifest_digest snapshot
  local current_revision stale_revision
  local fake_commit="cccccccccccccccccccccccccccccccccccccccc" commit mismatch_commit
  local report_commit="dddddddddddddddddddddddddddddddddddddddd"

  started="$(bash "$worker" start mixed-worker codex claude "$workspace" \
    'Test and fix the reported defect' --new-run \
    --objective-ref workflow=freeflow --objective-ref delivery_kind=code \
    --required-deliverable mixed='Tests, code repair, and report')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start mixed-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  approve_explainer "$worker" mixed-worker "$reviewer" mixed-reviewer \
    "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-delivery mixed-start
  source="$test_root/mixed-analysis.md"
  commit="$(fixture_commit 'mixed checkpoint')"
  printf '# Mixed delivery\nCode changes committed at %s\n' "$commit" >"$source"
  artifact="$(sha256sum "$source" | cut -d' ' -f1)"
  apply_action "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-stage \
    "{\"type\":\"stage_analysis\",\"source_path\":\"$source\"}" >/dev/null
  identity="$(analysis_identity "$artifact")"
  current_revision="$(rev "$run_dir")"
  stale_revision="$((current_revision - 1))"
  set +e
  failure="$(apply_action_error "$worker" mixed-worker "$run_dir" "$stale_revision" \
    mixed-stale-analysis \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"stale submission\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"revision_conflict"' <<<"$failure"
  set +e
  failure="$(apply_action_error "$worker" mixed-worker "$run_dir" "$current_revision" \
    mixed-analysis \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"commit named in report\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
  set +e
  failure="$(apply_action_error "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-fake-git \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$fake_commit\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$fake_commit\"}]}],\"verification\":[\"fake commit must fail\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
  mismatch_commit="$(fixture_commit 'mismatched artifact checkpoint')"
  set +e
  failure="$(apply_action_error "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-mismatch-git \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$commit\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$mismatch_commit\"}]}],\"verification\":[\"mismatched commit artifact must fail\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
  set +e
  failure="$(apply_action_error "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-wrong-type-git \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$commit\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"tree\",\"value\":\"$commit\"}]}],\"verification\":[\"commit cannot masquerade as tree\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
  reviewing="$(apply_action "$worker" mixed-worker "$run_dir" "$(rev "$run_dir")" mixed-git \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$commit\",\"deliverables\":[{\"id\":\"mixed\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$commit\"}]}],\"verification\":[\"Standards and Spec review attached\"]}}")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "reviewing" and s["checkpoint"]["kind"] == "git"' <<<"$reviewing"

  started="$(bash "$worker" start report-worker codex claude "$workspace" \
    'Produce an evidence-backed report' --new-run --objective-ref workflow=freeflow \
    --objective-ref delivery_kind=analysis \
    --required-deliverable report='Source-backed report')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start report-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  approve_explainer "$worker" report-worker "$reviewer" report-reviewer \
    "$worker" report-worker "$run_dir" "$(rev "$run_dir")" report-delivery report-start
  artifact="$(stage_analysis "$worker" report-worker "$run_dir" report-only)"
  identity="$(analysis_identity "$artifact")"
  set +e
  failure="$(apply_action_error "$worker" report-worker "$run_dir" "$(rev "$run_dir")" report-git \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$report_commit\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$report_commit\"}]}],\"verification\":[\"report cannot become code delivery\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
  reviewing="$(apply_action "$worker" report-worker "$run_dir" "$(rev "$run_dir")" report-analysis \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"sources checked\"]}}")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "reviewing" and s["checkpoint"]["kind"] == "analysis"' <<<"$reviewing"
  manifest_digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing")"
  apply_action "$reviewer" report-reviewer "$run_dir" "$(rev "$run_dir")" report-revision \
    "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$identity\",\"manifest_digest\":\"$manifest_digest\",\"scope_revision\":0,\"findings\":[\"Clarify the evidence limit\"]}" >/dev/null
  snapshot="$(bash "$worker" start report-worker codex claude "$workspace" --run-id "$run_id")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "revising" and s.get("human_decision") is None' <<<"$snapshot"

  started="$(bash "$worker" start missing-kind-worker codex claude "$workspace" \
    'Report with a missing delivery marker' --new-run --objective-ref workflow=freeflow \
    --required-deliverable report='Source-backed report')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start missing-kind-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  artifact="$(stage_analysis "$worker" missing-kind-worker "$run_dir" missing-kind)"
  identity="$(analysis_identity "$artifact")"
  set +e
  failure="$(apply_action_error "$worker" missing-kind-worker "$run_dir" "$(rev "$run_dir")" \
    missing-kind-checkpoint \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"sources checked\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"

  started="$(bash "$worker" start invalid-kind-worker codex claude "$workspace" \
    'Report with an invalid delivery marker' --new-run --objective-ref workflow=freeflow \
    --objective-ref delivery_kind=report \
    --required-deliverable report='Source-backed report')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start invalid-kind-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  artifact="$(stage_analysis "$worker" invalid-kind-worker "$run_dir" invalid-kind)"
  identity="$(analysis_identity "$artifact")"
  set +e
  failure="$(apply_action_error "$worker" invalid-kind-worker "$run_dir" "$(rev "$run_dir")" \
    invalid-kind-checkpoint \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"sources checked\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"
}
run_checkpoint_kind_guard

# Freeflow autonomy uses ordinary facade progress/resume actions for routine
# choices and recovery. Only real authority and an explicitly required
# user-only skill remain human entry points.
run_freeflow_autonomy() {
  local worker="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
  local started run_id run_dir snapshot trace failure failure_status
  local artifact identity reviewing manifest_digest
  started="$(bash "$worker" start autonomy-worker codex claude "$workspace" \
    'Produce and publish the autonomy report beyond the owner-only Site' --new-run --autonomous \
    --objective-ref workflow=freeflow --objective-ref delivery_kind=analysis \
    --objective-ref model_pair=codex-sol-high+claude-opus \
    --required-deliverable report='Autonomy evidence and broader-publication receipt')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start autonomy-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  apply_action "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-routine \
    '{"type":"report_progress","phase":"working","detail":"routine choices recorded: test seam, format, and tool"}' >/dev/null
  apply_action "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-tool-failure \
    '{"type":"report_progress","phase":"working","detail":"recoverable tool failure; using verified fallback"}' >/dev/null
  snapshot="$(bash "$worker" start autonomy-worker codex claude "$workspace" --run-id "$run_id")"
  python3 -c '
import json, sys
s=json.load(sys.stdin)
refs={(r["kind"],r["value"]) for r in s["objective"]["refs"]}
assert s["run_id"] == sys.argv[1] and s["status"] == "working"
assert ("model_pair","codex-sol-high+claude-opus") in refs
assert s.get("human_decision") is None
' "$run_id" <<<"$snapshot"

  trace="$test_root/autonomy-poll.trace"
  DVANDVA_POLL_TRACE="$trace" DVANDVA_POLL_CHUNK_MS=50 \
    bash "$reviewer" poll autonomy-reviewer "$run_dir" "$(rev "$run_dir")" 80 >/dev/null
  test "$(wc -l <"$trace")" -ge 1
  test "$(wc -l <"$trace")" -le 3

  artifact="$(stage_analysis "$worker" autonomy-worker "$run_dir" autonomy-round)"
  identity="$(analysis_identity "$artifact")"
  reviewing="$(apply_action "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-checkpoint \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"routine choices and recovery verified\"]}}")"
  manifest_digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing")"
  apply_action "$reviewer" autonomy-reviewer "$run_dir" "$(rev "$run_dir")" autonomy-peer-revision \
    "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$identity\",\"manifest_digest\":\"$manifest_digest\",\"scope_revision\":0,\"findings\":[\"Add recovery limitation\"]}" >/dev/null
  snapshot="$(bash "$worker" start autonomy-worker codex claude "$workspace" --run-id "$run_id")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); refs={(r["kind"],r["value"]) for r in s["objective"]["refs"]}; assert s["status"] == "revising" and s.get("human_decision") is None; assert ("model_pair","codex-sol-high+claude-opus") in refs' <<<"$snapshot"

  apply_action "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-authority \
    '{"type":"request_human_decision","kind":"authority","question":"May this run publish beyond the owner-only Site?","evidence":["No broader publication authority exists"],"options":["Authorize broader publication","Keep owner-only"]}' >/dev/null
  snapshot="$(bash "$worker" read autonomy-worker "$run_dir")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "human_decision" and s["human_decision"]["kind"] == "authority"' <<<"$snapshot"
  apply_action "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-answer \
    '{"type":"resume_human_decision","answer":"Authorize broader publication"}' >/dev/null
  snapshot="$(bash "$worker" start autonomy-worker codex claude "$workspace" --run-id "$run_id")"
  python3 -c 'import json,sys; s=json.load(sys.stdin); refs={(r["kind"],r["value"]) for r in s["objective"]["refs"]}; assert ("authority","Authorize broader publication") in refs; assert s["human_decision"]["answer"] == "Authorize broader publication"' <<<"$snapshot"
  set +e
  failure="$(apply_action_error "$worker" autonomy-worker "$run_dir" "$(rev "$run_dir")" autonomy-repeat \
    '{"type":"request_human_decision","kind":"authority","question":"May this run publish beyond the owner-only Site?","evidence":["No broader publication authority exists"],"options":["Authorize broader publication","Keep owner-only"]}')"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"repeated_decision"' <<<"$failure"
}
run_freeflow_autonomy

run_required_user_only_gate() {
  local worker="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
  local started run_id run_dir snapshot artifact identity failure failure_status skill_root
  skill_root="$test_root/required-user-only"
  mkdir -p "$skill_root/agents"
  cat >"$skill_root/SKILL.md" <<'EOF'
---
name: required-user-only
description: Disposable mandatory user-only canary skill.
disable-model-invocation: true
---
# Required user-only fixture
EOF
  cat >"$skill_root/agents/openai.yaml" <<'EOF'
interface:
  display_name: Required user-only fixture
policy:
  allow_implicit_invocation: false
EOF
  rg -q '^disable-model-invocation:[[:space:]]*true$' "$skill_root/SKILL.md"
  rg -q 'allow_implicit_invocation:[[:space:]]*false' "$skill_root/agents/openai.yaml"
  started="$(bash "$worker" start user-skill-worker codex claude "$workspace" \
    'Run a mandatory user-only method' --new-run --autonomous \
    --objective-ref workflow=freeflow --objective-ref delivery_kind=analysis \
    --objective-ref "required_user_skill=$skill_root" \
    --required-deliverable report='User-skill result')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start user-skill-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  apply_action "$worker" user-skill-worker "$run_dir" "$(rev "$run_dir")" user-skill-wait \
    '{"type":"report_progress","phase":"waiting","detail":"waiting_for_skill: /required-user-only; explicit invocation required before delivery"}' >/dev/null
  snapshot="$(bash "$worker" read user-skill-worker "$run_dir")"
  python3 -c '
import json,sys
s=json.load(sys.stdin)
assert s["status"] == "working" and s["checkpoint"] is None
assert s.get("human_decision") is None
assert "finalize" not in s["advisory_actions"]
assert s["participants"]["worker"]["progress"]["detail"].startswith("waiting_for_skill:")
' <<<"$snapshot"

  artifact="$(stage_analysis "$worker" user-skill-worker "$run_dir" user-skill-blocked)"
  identity="$(analysis_identity "$artifact")"
  set +e
  failure="$(apply_action_error "$worker" user-skill-worker "$run_dir" "$(rev "$run_dir")" user-skill-checkpoint \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"user-only skill metadata discovered\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq 'required user-only skill has not been explicitly invoked' <<<"$failure"

  started="$(bash "$worker" start user-skill-invoked-worker codex claude "$workspace" \
    'Run an explicitly invoked mandatory user-only method' --new-run --autonomous \
    --objective-ref workflow=freeflow --objective-ref delivery_kind=analysis \
    --objective-ref "required_user_skill=$skill_root" \
    --objective-ref "invoked_user_skill=$skill_root" \
    --required-deliverable report='User-skill result')"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start user-skill-invoked-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  artifact="$(stage_analysis "$worker" user-skill-invoked-worker "$run_dir" user-skill-invoked)"
  identity="$(analysis_identity "$artifact")"
  apply_action "$worker" user-skill-invoked-worker "$run_dir" "$(rev "$run_dir")" user-skill-invoked-checkpoint \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":[{\"id\":\"report\",\"artifacts\":[{\"kind\":\"analysis_digest\",\"value\":\"$artifact\"}]}],\"verification\":[\"explicit invocation represented by exact metadata root\"]}}" >/dev/null
}
run_required_user_only_gate

# A five-member Review stays one run and one atomic checkpoint. The kernel's
# existing complete-manifest interface rejects partial/duplicate coverage; a
# receipt/readiness update uses approval withdrawal and a complete replacement.
run_batch_review() {
  local worker="$HOME/.agents/skills/vadi/scripts/dvandva-role.sh"
  local reviewer="$HOME/.claude/skills/prativadi/scripts/dvandva-role.sh"
  local started run_id run_dir failure failure_status reviewing digest identity terminal missing_identity
  local round_identity round_digest
  local fixture_root="$test_root/github-review-fixture" round_bundle requested_bundle receipt_bundle
  local manifest missing_manifest duplicate_manifest requested_manifest receipt_manifest
  local -a members=(101 102 103 104 105) refs=() deliverables=()
  local -a initial_specs=() requested_specs=() final_specs=()
  local member
  python3 "$repo_root/tests/skills/fixtures/github_review_lifecycle.py" "$fixture_root"
  python3 - "$fixture_root/summary.json" <<'PY'
import json, sys
summary = json.load(open(sys.argv[1]))
events = {event["event"] for event in summary["events"]}
required = {
    "interrupted_before_write", "interrupted_after_write", "checks_changed",
    "evidence_invalidated", "evidence_rechecked", "author_repair",
    "disposition_changed", "review_submitted",
}
assert required <= events
assert summary["zero_duplicate_confirmed_retries"] is True
assert summary["write_count"] == summary["receipt_count"]
assert summary["initial_ready"] is False
assert summary["requested_changes_ready"] is False
assert summary["final_ready"] is True
PY
  for member in "${members[@]}"; do
    refs+=(--objective-ref "review_member=https://github.com/axatbhardwaj/Dvandva/pull/$member")
    deliverables+=(--required-deliverable "pr-$member=Review https://github.com/axatbhardwaj/Dvandva/pull/$member")
    initial_specs+=("pr-$member=$fixture_root/initial/pr-$member.json")
    requested_specs+=("pr-$member=$fixture_root/requested-changes/pr-$member.json")
    final_specs+=("pr-$member=$fixture_root/final/pr-$member.json")
  done
  started="$(bash "$worker" start batch-worker codex claude "$workspace" \
    'Review the frozen five-PR fixture' --new-run --objective-ref workflow=review \
    "${refs[@]}" "${deliverables[@]}")"
  run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$started")"
  run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"
  bash "$reviewer" start batch-reviewer claude codex "$workspace" --run-id "$run_id" >/dev/null
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-start

  round_bundle="$(stage_analysis_manifest "$worker" batch-worker "$run_dir" \
    initial "${initial_specs[@]}")"
  identity="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["identity"])' <<<"$round_bundle")"
  manifest="$(python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["deliverables"],separators=(",",":")))' <<<"$round_bundle")"
  missing_manifest="$(python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["deliverables"][:4],separators=(",",":")))' <<<"$round_bundle")"
  missing_identity="$(python3 -c 'import hashlib,json,sys; values=json.load(sys.stdin)["digests"][:4]; print(hashlib.sha256("\n".join(sorted(set(values))).encode()).hexdigest())' <<<"$round_bundle")"
  duplicate_manifest="$(python3 -c 'import json,sys; values=json.load(sys.stdin)["deliverables"]; print(json.dumps(values + values[:1],separators=(",",":")))' <<<"$round_bundle")"

  set +e
  failure="$(apply_action_error "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-missing \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$missing_identity\",\"deliverables\":${missing_manifest},\"verification\":[\"five deterministic PR fixtures inspected\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"

  set +e
  failure="$(apply_action_error "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-duplicate \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":${duplicate_manifest},\"verification\":[\"five deterministic PR fixtures inspected\"]}}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"invalid_checkpoint"' <<<"$failure"

  reviewing="$(apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-round \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":${manifest},\"verification\":[\"five deterministic PR fixtures inspected: mixed APPROVE and REQUEST_CHANGES; pending CI recorded\"]}}")"
  digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing")"
  round_identity="$identity"
  round_digest="$digest"
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-round
  apply_action "$reviewer" batch-reviewer "$run_dir" "$(rev "$run_dir")" batch-approve \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity\",\"manifest_digest\":\"$digest\",\"scope_revision\":0,\"findings\":[]}" >/dev/null

  set +e
  failure="$(apply_action_error "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-initial-finalize '{"type":"finalize"}')"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"review_not_ready"' <<<"$failure"

  apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-withdraw \
    '{"type":"withdraw_approval","reason":"Record confirmed REQUEST_CHANGES receipt"}' >/dev/null
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-requested-open
  requested_bundle="$(stage_analysis_manifest "$worker" batch-worker "$run_dir" \
    requested "${requested_specs[@]}")"
  identity="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["identity"])' <<<"$requested_bundle")"
  requested_manifest="$(python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["deliverables"],separators=(",",":")))' <<<"$requested_bundle")"
  reviewing="$(apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-requested \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":${requested_manifest},\"verification\":[\"REQUEST_CHANGES receipt persisted; run remains incomplete\"]}}")"
  digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing")"
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-requested
  apply_action "$reviewer" batch-reviewer "$run_dir" "$(rev "$run_dir")" batch-requested-approve \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity\",\"manifest_digest\":\"$digest\",\"scope_revision\":0,\"findings\":[]}" >/dev/null
  set +e
  failure="$(apply_action_error "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-requested-finalize '{"type":"finalize"}')"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"review_not_ready"' <<<"$failure"

  apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-receipt-withdraw \
    '{"type":"withdraw_approval","reason":"Record exact APPROVE receipts and pending-to-green readiness"}' >/dev/null
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-receipt-open
  receipt_bundle="$(stage_analysis_manifest "$worker" batch-worker "$run_dir" \
    final "${final_specs[@]}")"
  identity="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["identity"])' <<<"$receipt_bundle")"
  receipt_manifest="$(python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["deliverables"],separators=(",",":")))' <<<"$receipt_bundle")"
  reviewing="$(apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-receipts \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"analysis\",\"identity\":\"$identity\",\"deliverables\":${receipt_manifest},\"verification\":[\"actor PR head state and body digest receipts verified; pending checks now green; unchanged reviews submitted zero duplicate writes\"]}}")"
  digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing")"
  approve_explainer "$worker" batch-worker "$reviewer" batch-reviewer \
    "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-review batch-receipts
  set +e
  failure="$(apply_action_error "$reviewer" batch-reviewer "$run_dir" "$(rev "$run_dir")" \
    batch-stale-approval \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$round_identity\",\"manifest_digest\":\"$round_digest\",\"scope_revision\":0,\"findings\":[]}")"
  failure_status=$?
  set -e
  test "$failure_status" -ne 0
  grep -Fq '"error":"stale_review"' <<<"$failure"
  apply_action "$reviewer" batch-reviewer "$run_dir" "$(rev "$run_dir")" batch-receipt-approve \
    "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$identity\",\"manifest_digest\":\"$digest\",\"scope_revision\":0,\"findings\":[]}" >/dev/null
  terminal="$(apply_action "$worker" batch-worker "$run_dir" "$(rev "$run_dir")" batch-finalize '{"type":"finalize"}')"
  python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["status"] == "done"; assert len(s["checkpoint"]["deliverables"]) == 5' <<<"$terminal"
}
run_batch_review

test ! -e "$test_root/peer-launched"
printf 'two-role skill canary: ok\n'
