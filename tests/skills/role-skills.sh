#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
binary="$XDG_DATA_HOME/dvandva/bin/current/dvandva-kernel"
mkdir -p "$(dirname "$binary")"
cp "$repo_root/v4/target/debug/dvandva-v4" "$binary"

workspace="$test_root/workspace"
mkdir -p "$workspace"
git -C "$workspace" init --quiet
git -C "$workspace" remote add origin git@github.com:axatbhardwaj/Dvandva.git

vadi="$repo_root/skills/vadi/scripts/dvandva-role.sh"
prativadi="$repo_root/skills/prativadi/scripts/dvandva-role.sh"

expect_failure() {
  local pattern="$1"
  shift
  local output
  if output="$("$@" 2>&1)"; then
    printf 'expected command to fail: %s\n' "$*" >&2
    exit 1
  fi
  grep -Fq "$pattern" <<<"$output"
}

export CODEX_SESSION_ID="codex-session"
test "$(bash "$vadi" session-id)" = "codex-session"
generated="$(env -u CODEX_SESSION_ID bash "$vadi" session-id --generate)"
[[ "$generated" =~ ^[0-9a-f-]{36}$ ]]
probe="$(bash "$vadi" probe)"
grep -Fq '"version": "0.2.0"' <<<"$probe"
grep -Fq '"write_schema": "dvandva.run.v2"' <<<"$probe"
grep -Fq '"read_schemas": [' <<<"$probe"
grep -Fq '"role_api": 2' <<<"$probe"
grep -Fq '"upgrade_from_v1": true' <<<"$probe"

# A truthful v1/API-1 kernel must be rejected before the facade reaches a role command.
mv "$binary" "$binary.new"
cat >"$binary" <<'OLD_KERNEL'
#!/usr/bin/env bash
if test "${1:-}" = "--version"; then
  printf 'dvandva-v4 0.1.1\n'
  exit 0
fi
if test "${1:-}" = "probe"; then
  printf '{"package":"dvandva-v4","version":"0.1.1","write_schema":"dvandva.run.v1","read_schemas":["dvandva.run.v1"],"role_api":1,"capabilities":{"upgrade_from_v1":false},"compatible":false}\n'
  exit 1
fi
printf 'role-called\n' >>"${OLD_KERNEL_LOG:?}"
exit 99
OLD_KERNEL
chmod 755 "$binary"
export OLD_KERNEL_LOG="$test_root/old-kernel.log"
expect_failure 'incompatible kernel' bash "$vadi" start old codex claude "$workspace" \
  'Must not mutate' DEF-OLD --required-deliverable implementation=old
test ! -e "$OLD_KERNEL_LOG"
test ! -e "$XDG_STATE_HOME/dvandva/runs"
mv "$binary.new" "$binary"

# A truthful old facade asks for v1/API 1 and the v0.2 kernel rejects it before mutation.
old_facade="$test_root/dvandva-role-v0.1.1.sh"
cat >"$old_facade" <<'OLD_FACADE'
#!/usr/bin/env bash
set -euo pipefail
binary="${XDG_DATA_HOME:?}/dvandva/bin/current/dvandva-kernel"
"$binary" probe --expected-schema dvandva.run.v1 --expected-role-api 1 >/dev/null
"$binary" role start --api 1 "$@"
OLD_FACADE
chmod 755 "$old_facade"
expect_failure 'kernel compatibility mismatch' bash "$old_facade" \
  --workspace "$workspace" --runs-dir "$XDG_STATE_HOME/dvandva/runs" \
  --credentials-root "$XDG_STATE_HOME/dvandva/credentials" --role worker \
  --session-id old --current-harness codex --peer-harness claude \
  --objective old --required-deliverable implementation=old
test ! -e "$XDG_STATE_HOME/dvandva/runs"

export DVANDVA_LEASE_SECONDS=1
worker="$(bash "$vadi" start codex-session codex claude "$workspace" \
  'Implement DEF-123' DEF-123 --objective-ref ticket=https://tracker.test/DEF-123 \
  --required-deliverable implementation='Implement DEF-123')"
grep -Fq '"outcome": "started"' <<<"$worker"
run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$worker")"
run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"

# Exact joins pass only identity unless scope was explicitly supplied.
reviewer="$(bash "$prativadi" start claude-session claude codex "$workspace" \
  --run-id "$run_id")"
grep -Fq "\"run_id\": \"$run_id\"" <<<"$reviewer"
grep -Fq '"summary": "Implement DEF-123"' <<<"$reviewer"
test "$(find "$XDG_STATE_HOME/dvandva/runs" -mindepth 1 -maxdepth 1 -type d | wc -l)" = 1

bash "$vadi" read codex-session "$run_dir" | grep -Fq '"status": "working"'
sleep 2
bash "$vadi" reclaim codex-session "$run_dir" 2 | grep -Fq '"revision": 3'
bash "$vadi" heartbeat codex-session "$run_dir" 3 | grep -Fq '"revision":4'
bash "$vadi" wait codex-session "$run_dir" 4 50 | grep -Fq '"revision": 4'

action="$test_root/human.json"
printf '%s\n' '{"type":"request_human_decision","question":"Confirm scope","evidence":["scope changed"],"options":["yes","no"],"contact_role":"worker","resume_status":"working","resume_assignee":"worker"}' >"$action"
bash "$vadi" apply codex-session "$run_dir" 4 "$action" | grep -Fq '"status": "human_decision"'

# Explicit upgrade is followed by an explicit reclaim and a normal v2 read.
legacy_dir="$XDG_STATE_HOME/dvandva/runs/legacy-run"
mkdir -p "$legacy_dir/history"
cat >"$legacy_dir/baton.json" <<'LEGACY'
{
  "schema":"dvandva.run.v1","run_id":"legacy-run",
  "objective":{"summary":"Migrate safely","refs":[]},
  "workspace":{"repository_id":"github.com/axatbhardwaj/dvandva","origin":"git@github.com:axatbhardwaj/Dvandva.git","worktree":null},
  "task":{"reference":"DEF-LEGACY","summary":"Migrate safely"},
  "participants":{"worker":{"harness":"codex","claim":null},"reviewer":{"harness":"claude","claim":null}},
  "status":"working","assignee":"worker","revision":0,"checkpoint":null,"review":null,
  "publication":{"required":true,"desired_revision":0,"published_revision":null,"refs":[]},
  "human_decision":null,"predecessor_run_id":null,"terminal":null,"recovery":null
}
LEGACY
cp "$legacy_dir/baton.json" "$legacy_dir/history/00000000000000000000.json"
upgrade_required="$(bash "$vadi" start legacy-session codex claude "$workspace" --run-id legacy-run)"
grep -Fq '"outcome": "upgrade_required"' <<<"$upgrade_required"
bash "$vadi" upgrade legacy-session "$legacy_dir" codex claude 0 | grep -Fq '"schema": "dvandva.run.v2"'
bash "$vadi" claim legacy-session "$legacy_dir" 1 | grep -Fq '"revision": 2'
bash "$vadi" read legacy-session "$legacy_dir" | grep -Fq '"status": "revising"'

cmp "$vadi" "$prativadi"

grep -Fq 'act as vadi' "$repo_root/skills/vadi/SKILL.md"
grep -Fq 'act as prativadi' "$repo_root/skills/prativadi/SKILL.md"

printf 'role skill wrappers: ok\n'
