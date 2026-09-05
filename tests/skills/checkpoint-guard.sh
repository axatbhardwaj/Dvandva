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

# A terminal disposition is evidence, not a shortcut around identity, revision,
# timestamp, and basis fields.
review_dir="$test_root/review-artifacts"
mkdir -p "$review_dir"
terminal_fixture() { python3 - "$review_dir" "$test_root/review-snapshot.json" "$1" <<'PY'
import hashlib, json, pathlib, sys
root, snapshot_path, mode = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
record = {"url":"https://github.com/axatbhardwaj/Dvandva/pull/31","disposition":"merged","evidence_valid":True}
if mode == "complete":
    record.update({"author":"author","acting_reviewer":"reviewer","head":"1"*40,"base":"2"*40,"observed_at":"2026-09-06T12:00:00Z","review_basis":"Exact head/base and merged disposition re-queried"})
contents=json.dumps(record,separators=(",",":")); digest=hashlib.sha256(contents.encode()).hexdigest()
(root/f"{digest}.json").write_text(json.dumps({"digest":digest,"contents":contents}))
snapshot={"revision":7,"objective":{"refs":[{"kind":"workflow","value":"review"},{"kind":"review_member","value":record["url"]}]},"task":{"reference":None},"checkpoint":{"kind":"analysis","deliverables":[{"id":"pr-31","artifacts":[{"kind":"analysis_digest","value":digest}]}]}}
snapshot_path.write_text(json.dumps(snapshot))
PY
}
terminal_fixture minimal
set +e
error="$(python3 "$vadi_review" validate "$action" 7 "$review_dir" <"$test_root/review-snapshot.json")"; status=$?
set -e
test "$status" -ne 0
grep -Fq 'review_not_ready' <<<"$error"
find "$review_dir" -maxdepth 1 -type f -exec unlink {} \;
terminal_fixture complete
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
