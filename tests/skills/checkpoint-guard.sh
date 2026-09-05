#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
workspace="$test_root/repo"
git init --quiet "$workspace"
git -C "$workspace" -c user.name=Canary -c user.email=canary@example.invalid \
  commit --quiet --allow-empty -m fixture
commit="$(git -C "$workspace" rev-parse HEAD)"
vadi="$repo_root/skills/vadi/scripts/checkpoint_guard.py"
prativadi="$repo_root/skills/prativadi/scripts/checkpoint_guard.py"
cmp "$vadi" "$prativadi"
cmp "$repo_root/skills/vadi/scripts/role_guard.py" "$repo_root/skills/prativadi/scripts/role_guard.py"
cmp "$repo_root/skills/vadi/scripts/skill_metadata.py" "$repo_root/skills/prativadi/scripts/skill_metadata.py"
vadi_review="$repo_root/skills/vadi/scripts/review_guard.py"
prativadi_review="$repo_root/skills/prativadi/scripts/review_guard.py"
cmp "$vadi_review" "$prativadi_review"

snapshot() {
  local workflow="$1" marker="$2" task="${3:-}"
  python3 - "$workspace" "$workflow" "$marker" "$task" <<'PY'
import json, sys
refs = [{"kind":"workflow","value":sys.argv[2]}]
if sys.argv[3]: refs.append({"kind":"delivery_kind","value":sys.argv[3]})
task = {"reference": sys.argv[4] or None}
print(json.dumps({"revision":7,"objective":{"refs":refs},"task":task,"workspace":{"worktree":sys.argv[1]}}))
PY
}
action="$test_root/action.json"
printf '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"%s","deliverables":[{"id":"work","artifacts":[{"kind":"commit","value":"%s"}]}]}}\n' "$commit" "$commit" >"$action"
snapshot freeflow code | python3 "$vadi" "$action" 7
snapshot freeflow code | python3 "$prativadi" "$action" 7

# A commit exists, but its exact type is not tree.
printf '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"%s","deliverables":[{"id":"work","artifacts":[{"kind":"tree","value":"%s"}]}]}}\n' "$commit" "$commit" >"$action"
set +e
vadi_error="$(snapshot freeflow code | python3 "$vadi" "$action" 7)"; vadi_status=$?
prati_error="$(snapshot freeflow code | python3 "$prativadi" "$action" 7)"; prati_status=$?
set -e
test "$vadi_status" -ne 0 && test "$prati_status" -ne 0
test "$vadi_error" = "$prati_error"
grep -Fq '"error":"invalid_checkpoint"' <<<"$vadi_error"

# Legacy workflows retain kernel-only Git acceptance behavior.
snapshot implementation '' | python3 "$vadi" "$action" 7
# A stale snapshot/expected pair is left to kernel compare-and-swap.
snapshot freeflow code | python3 "$vadi" "$action" 6

printf '{"type":"submit_checkpoint","checkpoint":{"kind":"analysis","identity":"%064d","deliverables":[]}}\n' 0 >"$action"
set +e
missing_error="$(snapshot freeflow '' | python3 "$vadi" "$action" 7)"; missing_status=$?
set -e
test "$missing_status" -ne 0
grep -Fq 'Freeflow delivery_kind must be exactly one of code or analysis' <<<"$missing_error"

# Every member-less legacy Review run retains its pre-existing kernel-only
# finalization behavior, including null and tracker-style task references.
printf '{"type":"finalize"}\n' >"$action"
for legacy_task in '' 'PR-10' 'issue-31' 'https://github.com/axatbhardwaj/Dvandva/pull/31'; do
  legacy_output="$(snapshot review '' "$legacy_task" | python3 "$vadi_review" list "$action" 7)"
  test -z "$legacy_output"
done

# Exact legacy joins and human amendments bypass new-run validation, so the
# final boundary independently rejects a cross-repository frozen batch.
python3 - "$test_root/review-snapshot.json" <<'PY'
import json, pathlib, sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "revision": 7,
    "objective": {"refs": [
        {"kind": "workflow", "value": "review"},
        {"kind": "review_member", "value": "https://github.com/axatbhardwaj/Dvandva/pull/31"},
        {"kind": "review_member", "value": "https://github.com/example/other/pull/32"},
    ]},
    "task": {"reference": None},
    "workspace": {"repository_id": "github.com/axatbhardwaj/dvandva"},
    "checkpoint": {"kind": "analysis", "deliverables": []},
}))
PY
set +e
error="$(python3 "$vadi_review" validate "$action" 7 "$test_root" <"$test_root/review-snapshot.json")"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'members do not belong to one canonical repository' <<<"$error"

# A verified terminal disposition does not pretend that current-head review
# evidence still exists. Identity and disposition remain mandatory.
review_dir="$test_root/review-artifacts"
mkdir -p "$review_dir"
terminal_fixture() { python3 - "$review_dir" "$test_root/review-snapshot.json" "$1" <<'PY'
import hashlib, json, pathlib, sys
root, snapshot_path, mode = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
record = {"url":"https://github.com/axatbhardwaj/Dvandva/pull/31","disposition":"merged","evidence_valid":True,"observed_at":"2026-09-06T12:00:00Z"}
if mode == "missing-observed-at":
    record.pop("observed_at")
if mode == "complete":
    record.update({"author":"author","acting_reviewer":"reviewer","head":"1"*40,"base":"2"*40,"dependencies":[],"review_basis":"Merged disposition re-queried; no current approval asserted","findings":[],"proposed_verdict":None,"adjudicated_verdict":None,"exact_body":None,"body_digest":None,"receipts":[],"checks":None,"blocking_feedback":[],"next_action":"No action; PR is merged"})
contents=json.dumps(record,separators=(",",":")); digest=hashlib.sha256(contents.encode()).hexdigest()
(root/f"{digest}.json").write_text(json.dumps({"digest":digest,"contents":contents}))
snapshot={"revision":7,"objective":{"refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":record["url"]}]},"task":{"reference":None},"workspace":{"repository_id":"github.com/axatbhardwaj/dvandva"},"checkpoint":{"kind":"analysis","deliverables":[{"id":"pr-31","artifacts":[{"kind":"analysis_digest","value":digest}]}]}}
snapshot_path.write_text(json.dumps(snapshot))
PY
}
terminal_fixture minimal
set +e
error="$(python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json")"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'record is incomplete' <<<"$error"
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
terminal_fixture missing-observed-at
set +e
error="$(python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json")"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'timestamp is invalid' <<<"$error"
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
terminal_fixture complete
python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json"

# Repository identity is bound to the credential-checked run workspace, not
# merely shared among all frozen members.
python3 - "$test_root/review-snapshot.json" <<'PY'
import json, pathlib, sys
path=pathlib.Path(sys.argv[1]); snapshot=json.loads(path.read_text())
snapshot["workspace"]["repository_id"]="github.com/example/other"
path.write_text(json.dumps(snapshot))
PY
set +e
error="$(python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json")"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'members do not match the canonical workspace repository' <<<"$error"

# Open readiness requires complete current basis and an exact approval receipt.
open_fixture() { python3 - "$review_dir" "$test_root/review-snapshot.json" "$1" <<'PY'
import hashlib, json, pathlib, sys
root, snapshot_path, mode = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
body="Approved on exact current evidence"; head="3"*40
record={"url":"https://github.com/axatbhardwaj/Dvandva/pull/31","disposition":"open","evidence_valid":True,"author":"author","acting_reviewer":"reviewer","head":head,"base":"4"*40,"dependencies":[],"observed_at":"2026-09-06T12:00:00Z","review_basis":"Exact head, base, dependency, checks, feedback, and receipt query","findings":[],"proposed_verdict":"APPROVE","checks":"green","blocking_feedback":[],"adjudicated_verdict":"APPROVE","exact_body":body,"body_digest":hashlib.sha256(body.encode()).hexdigest(),"next_action":"Finalize after exact approval receipt"}
record["receipts"]=[{"pr":31,"actor":"reviewer","head":head,"state":"APPROVE","body_digest":record["body_digest"]}]
if mode.startswith("missing-"): record.pop(mode.removeprefix("missing-"))
if mode == "invalid-base": record["base"]="main"
if mode == "invalid-observed_at": record["observed_at"]="yesterday"
if mode == "invalid-dependencies": record["dependencies"]=[31]
if mode == "padded-self-review": record["author"]=" reviewer "
if mode == "padded-receipt": record["receipts"][0]["actor"]=" reviewer "
if mode == "padded-actor": record["acting_reviewer"]=" reviewer "
if mode == "numeric-receipt":
    record["acting_reviewer"]="123"
    record["receipts"][0]["actor"]=123
contents=json.dumps(record,separators=(",",":")); digest=hashlib.sha256(contents.encode()).hexdigest()
(root/f"{digest}.json").write_text(json.dumps({"digest":digest,"contents":contents}))
snapshot={"revision":7,"objective":{"refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":record["url"]}]},"task":{"reference":None},"workspace":{"repository_id":"github.com/axatbhardwaj/dvandva"},"checkpoint":{"kind":"analysis","deliverables":[{"id":"pr-31","artifacts":[{"kind":"analysis_digest","value":digest}]}]}}
snapshot_path.write_text(json.dumps(snapshot))
PY
}
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
for mode in missing-base missing-dependencies missing-observed_at missing-review_basis missing-findings missing-proposed_verdict missing-next_action invalid-base invalid-observed_at invalid-dependencies padded-self-review numeric-receipt; do
  open_fixture "$mode"
  set +e
  error="$(python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json")"; status=$?
  set -e
  test "$status" -ne 0
  grep -Fq 'review_not_ready' <<<"$error"
  find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
done
open_fixture complete
python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json"
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
open_fixture padded-receipt
python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json"
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
open_fixture padded-actor
python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json"

# Metadata keys in examples/body text and oversized files must not mark a skill
# user-only. A real leading frontmatter/policy document does.
valid_skill="$test_root/valid-skill"; false_skill="$test_root/false-skill"; huge_skill="$test_root/huge-skill"; malformed_skill="$test_root/malformed-skill"; unclosed_skill="$test_root/unclosed-skill"
mkdir -p "$valid_skill/agents" "$false_skill/agents" "$huge_skill/agents" "$malformed_skill/agents" "$unclosed_skill/agents"
printf '%s\n' '---' 'name: fixture' "description: This user's skill can't run implicitly." 'disable-model-invocation: true # user-only' '---' '# Fixture' >"$valid_skill/SKILL.md"
printf '%s\n' 'interface:' '  display_name: "Fixture"' '  short_description: "Explicit # invocation"' 'policy:' '  allow_implicit_invocation: false' >"$valid_skill/agents/openai.yaml"
printf '%s\n' '---' 'name: example' '---' '# Example' '```yaml' 'disable-model-invocation: true' '```' >"$false_skill/SKILL.md"
printf '%s\n' 'examples:' '  allow_implicit_invocation: false' >"$false_skill/agents/openai.yaml"
python3 - "$huge_skill" <<'PY'
from pathlib import Path
import sys
root=Path(sys.argv[1]); (root/'SKILL.md').write_bytes(b'---\nname: huge\n---\n'+b'x'*70000); (root/'agents/openai.yaml').write_text('policy:\n  allow_implicit_invocation: false\n')
PY
printf '%s\n' '---' 'name: [' 'disable-model-invocation: true' '---' >"$malformed_skill/SKILL.md"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' >"$malformed_skill/agents/openai.yaml"
printf '%s\n' '---' 'name: fixture' 'disable-model-invocation: true' >"$unclosed_skill/SKILL.md"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' >"$unclosed_skill/agents/openai.yaml"
printf '{"type":"submit_checkpoint","checkpoint":{"kind":"analysis","identity":"%064d","deliverables":[]}}\n' 0 >"$action"
skill_snapshot() { python3 - "$workspace" "$1" "$2" <<'PY'
import json,sys
refs=[{"kind":"workflow","value":"freeflow"},{"kind":"delivery_kind","value":"analysis"},{"kind":"required_user_skill","value":sys.argv[2]},{"kind":"invoked_user_skill","value":sys.argv[2]}]
print(json.dumps({"revision":7,"objective":{"refs":refs},"workspace":{"worktree":sys.argv[1]}}))
PY
}
skill_snapshot "$valid_skill" yes | python3 "$vadi" "$action" 7
skill_snapshot "$repo_root/skills/setup-dvandva" yes | python3 "$vadi" "$action" 7
for root in "$false_skill" "$huge_skill" "$malformed_skill" "$unclosed_skill"; do
  set +e
  error="$(skill_snapshot "$root" yes | python3 "$vadi" "$action" 7)"; status=$?
  set -e
  test "$status" -ne 0
  grep -Fq 'metadata is missing, unreadable, or model-invocable' <<<"$error"
done
printf '%s\n' '---' 'name: invalid: yaml' 'disable-model-invocation: true' '---' >"$malformed_skill/SKILL.md"
set +e
error="$(skill_snapshot "$malformed_skill" yes | python3 "$vadi" "$action" 7)"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'metadata is missing, unreadable, or model-invocable' <<<"$error"
printf '%s\n' '---' 'name: invalid:' 'disable-model-invocation: true' '---' >"$malformed_skill/SKILL.md"
set +e
error="$(skill_snapshot "$malformed_skill" yes | python3 "$vadi" "$action" 7)"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'metadata is missing, unreadable, or model-invocable' <<<"$error"
printf '%s\n' '---' 'name: %invalid' 'disable-model-invocation: true' '---' >"$malformed_skill/SKILL.md"
set +e
error="$(skill_snapshot "$malformed_skill" yes | python3 "$vadi" "$action" 7)"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'metadata is missing, unreadable, or model-invocable' <<<"$error"
printf '%s\n' '---' 'name: fixture' 'disable-model-invocation: "true"' '---' >"$malformed_skill/SKILL.md"
printf '%s\n' 'policy:' '  allow_implicit_invocation: "false"' >"$malformed_skill/agents/openai.yaml"
set +e
error="$(skill_snapshot "$malformed_skill" yes | python3 "$vadi" "$action" 7)"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'metadata is missing, unreadable, or model-invocable' <<<"$error"
printf 'checkpoint guard: ok\n'
