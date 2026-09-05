#!/usr/bin/env bash
set -euo pipefail
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
  install --version 0.3.8 >/dev/null

for role in vadi prativadi; do
  for reference in initiation discovery; do
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

# Stage the bytes behind an analysis deliverable and echo their digest, so the
# manifest cites something the reviewer can materialize.
stage_analysis() {
  local facade="$1" session="$2" run_dir="$3" label="$4" source staged
  source="$test_root/analysis-$label.md"
  printf '# %s\nanalysis deliverable\n' "$label" >"$source"
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
  # Git checkpoints bind full-length object names, so the per-casting suffix
  # has to stay inside the hex alphabet.
  local nibble
  case "$label" in normal) nibble=1 ;; reverse) nibble=2 ;; *) nibble=f ;; esac
  checkpoint_a="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa$nibble"
  checkpoint_b="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb$nibble"
  reviewing_a="$(apply_action "$worker" "$worker_session" "$run_dir" 5 checkpoint-a-$label \
    "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$checkpoint_a\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$checkpoint_a\"}]}],\"verification\":[\"cargo test\"]}}")"
  digest_a="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' <<<"$reviewing_a")"
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 6 "$site_id" deployment-2
  apply_action "$reviewer" "$reviewer_session" "$run_dir" 9 changes-$label \
    "{\"type\":\"record_review\",\"verdict\":\"changes_requested\",\"checkpoint_identity\":\"$checkpoint_a\",\"manifest_digest\":\"$digest_a\",\"scope_revision\":0,\"findings\":[\"Add contention coverage\"]}" >/dev/null
  approve_explainer "$worker" "$worker_session" "$reviewer" "$reviewer_session" \
    "$sites_publisher" "$sites_session" "$run_dir" 10 "$site_id" deployment-3
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

test ! -e "$test_root/peer-launched"
printf 'two-role skill canary: ok\n'
