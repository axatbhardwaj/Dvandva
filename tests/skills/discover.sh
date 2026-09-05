#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
export XDG_DATA_HOME="$test_root/data" XDG_STATE_HOME="$test_root/state"
cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
mkdir -p "$XDG_DATA_HOME/dvandva/bin/0.3.9"
cp "$repo_root/v4/target/debug/dvandva-v4" "$XDG_DATA_HOME/dvandva/bin/0.3.9/dvandva-kernel"
vadi="$repo_root/skills/vadi/scripts/dvandva-role.sh"
prati="$repo_root/skills/prativadi/scripts/dvandva-role.sh"
workspace="$test_root/repo"
git init --quiet "$workspace"
git -C "$workspace" remote add origin https://github.com/example/project.git
field() { python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }
scan() { bash "$prati" discover reviewer codex claude "$workspace" "$@"; }
create() {
  bash "$vadi" start "$1" claude codex "$workspace" "$2" \
    --new-run --task-reference "$3" --objective-ref "workflow=$4" \
    --required-deliverable delivery='Complete work'
}
# An empty lookup neither creates registry directories nor requires objective wording.
result="$(scan --workflow review)"
test "$(field outcome <<<"$result")" = none
test ! -e "$XDG_STATE_HOME"
first="$(create worker-a 'Review this change carefully' PR-10 review)"
first_id="$(field run_id <<<"$first")"
create worker-b 'Maintain our own change' PR-10 babysitting >/dev/null
create worker-c 'Another external change' PR-20 review >/dev/null
foreign="$(bash "$vadi" start worker-foreign other-harness codex "$workspace" \
  'Review with another worker' --new-run --task-reference PR-30 \
  --objective-ref workflow=review --required-deliverable delivery='Complete work')"
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
cmp "$vadi" "$prati"
cmp "$repo_root/skills/vadi/scripts/discover.py" "$repo_root/skills/prativadi/scripts/discover.py"
printf 'automatic run discovery: ok\n'
