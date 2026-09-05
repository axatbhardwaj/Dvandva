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

snapshot() {
  local workflow="$1" marker="$2"
  python3 - "$workspace" "$workflow" "$marker" <<'PY'
import json, sys
refs = [{"kind":"workflow","value":sys.argv[2]}]
if sys.argv[3]: refs.append({"kind":"delivery_kind","value":sys.argv[3]})
print(json.dumps({"revision":7,"objective":{"refs":refs},"workspace":{"worktree":sys.argv[1]}}))
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
printf 'checkpoint guard: ok\n'
