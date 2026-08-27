#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

mkdir -p "$test_root/old"
git -C "$repo_root" archive skills-v0.1.1 v4 skills/vadi | tar -x -C "$test_root/old"
CARGO_TARGET_DIR="$test_root/old-target" cargo build --quiet --locked \
  --manifest-path "$test_root/old/v4/Cargo.toml"
cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
old_binary="$test_root/old-target/debug/dvandva-v4"
test "$($old_binary --version)" = 'dvandva-v4 0.1.1'
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
grep -Fq '"publish": false' <<<"$probe"

# Correct strings hidden in a nested decoy cannot satisfy the top-level contract.
mv "$binary" "$binary.real"
cat >"$binary" <<'DECOY_KERNEL'
#!/usr/bin/env bash
if test "${1:-}" = "--version"; then printf 'dvandva-v4 0.2.0\n'; exit 0; fi
if test "${1:-}" = "probe"; then
  printf '%s\n' '{' \
    '  "package": 7, "version": false, "publish": true, "write_schema": [],' \
    '  "read_schemas": "wrong", "role_api": "2",' \
    '  "capabilities": {"upgrade_from_v1": "true"}, "compatible": "true",' \
    '  "decoy": {' \
    '    "package": "dvandva-v4", "version": "0.2.0", "publish": false,' \
    '    "write_schema": "dvandva.run.v2",' \
    '    "read_schemas": ["dvandva.run.v2", "dvandva.run.v1"],' \
    '    "role_api": 2, "capabilities": {"upgrade_from_v1": true},' \
    '    "compatible": true' \
    '  }' '}'
  exit 0
fi
exit 99
DECOY_KERNEL
chmod 755 "$binary"
expect_failure 'incompatible kernel' bash "$vadi" probe
mv "$binary.real" "$binary"

# A truthful v1/API-1 kernel must be rejected before the facade reaches a role command.
mv "$binary" "$binary.new"
cp "$old_binary" "$binary"
expect_failure 'incompatible kernel' bash "$vadi" start old codex claude "$workspace" \
  'Must not mutate' DEF-OLD --required-deliverable implementation=old
test ! -e "$XDG_STATE_HOME/dvandva/runs"
mv "$binary.new" "$binary"

# The released 0.1.1 facade also fails its v1 handshake against the new kernel.
old_facade="$test_root/old/skills/vadi/scripts/dvandva-role.sh"
expect_failure 'incompatible kernel' bash "$old_facade" start old codex claude \
  "$workspace" 'Must not mutate' DEF-OLD --new-run
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
printf '%s\n' '{"type":"request_human_decision","question":"Confirm scope","evidence":["scope changed"],"options":["yes","no"]}' >"$action"
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

vadi_skill="$repo_root/skills/vadi/SKILL.md"
vadi_contract="$repo_root/skills/vadi/references/run-contract.md"
vadi_prompt="$repo_root/skills/vadi/agents/openai.yaml"
prativadi_skill="$repo_root/skills/prativadi/SKILL.md"
prativadi_contract="$repo_root/skills/prativadi/references/run-contract.md"
prativadi_prompt="$repo_root/skills/prativadi/agents/openai.yaml"
setup_skill="$repo_root/skills/setup-dvandva/SKILL.md"
setup_contract="$repo_root/skills/setup-dvandva/references/installation.md"

role_contract="$(cat "$vadi_skill" "$vadi_contract" "$prativadi_skill" "$prativadi_contract")"
setup_docs="$(cat "$setup_skill" "$setup_contract")"

for required in \
  'first user-visible protocol output' \
  'canonical objective and scope' \
  'status and assignee' \
  'next_actions' \
  'peer_prompt' \
  'fresh facade snapshot' \
  'advisory_actions' \
  'legal_actions' \
  'new human scope or ambiguity' \
  'never an ordinary wake or action' \
  'scope_mismatch' \
  'complete deliverable manifest' \
  'canonical deliverable IDs exactly once' \
  'manifest_digest' \
  'scope_revision' \
  'request_checkpoint_supersession' \
  'accept_checkpoint_supersession' \
  'withdraw_approval' \
  'Codex harness publishes' \
  'Claude harness reviews' \
  'regardless of semantic casting' \
  'canonical scope, complete manifest, findings and decisions, and a current plan/TODO' \
  'stable Site ID' \
  'new Site version' \
  'owner-only' \
  'Claude Artifact' \
  'generic publisher' \
  'public access' \
  'silent fallback' \
  'user-created harness goals remain unchanged' \
  'human starts the peer session' \
  'explicitly invokes them in this session' \
  'What changed' \
  'What was verified' \
  'What is blocked' \
  'Who owns the next action' \
  'Exact command or prompt'
do
  grep -Fq "$required" <<<"$role_contract"
done

grep -Fq 'Exact joins pass only `--run-id`' <<<"$role_contract"
grep -Fq 'Publication never substitutes for supersession or withdrawal.' <<<"$role_contract"
grep -Fq 'foreground local wait' <<<"$role_contract"
grep -Fq 'Prativadi never creates a run.' "$prativadi_contract"

for forbidden in create_goal update_goal get_goal pause_goal complete_goal clear_goal; do
  ! grep -Fq "$forbidden" <<<"$role_contract"
done

for required in \
  '"type":"submit_checkpoint"' \
  '"deliverables"' \
  '"verification"' \
  '"type":"record_review"' \
  '"checkpoint_identity"' \
  '"manifest_digest"' \
  '"scope_revision"' \
  '"type":"request_human_decision"' \
  '"question"' \
  '"evidence"' \
  '"options"' \
  '"type":"record_explainer_publication"' \
  '"channel":"codex_sites"' \
  '"access":"owner_only"' \
  '"type":"record_explainer_review"'
do
  grep -Fq "$required" <<<"$role_contract"
done

for required in \
  '0.2.0' \
  'skills-v0.2.0' \
  'dvandva.run.v2' \
  'facade API 2' \
  'v1 read support is only for explicit migration' \
  'setup never migrates runs'
do
  grep -Fq "$required" <<<"$setup_docs"
done

test "$(wc -l <"$vadi_prompt")" -le 7
test "$(wc -l <"$prativadi_prompt")" -le 7
! grep -Eq 'next_actions|legal_actions|submit_checkpoint|record_review' \
  "$vadi_prompt" "$prativadi_prompt"

printf 'role skill wrappers: ok\n'
