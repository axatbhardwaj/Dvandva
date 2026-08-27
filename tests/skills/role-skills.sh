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

# Handshake validation preserves raw bytes, size, and producer status.
mv "$binary" "$binary.real"
cat >"$binary" <<'ADVERSARIAL_KERNEL'
#!/usr/bin/env bash
valid_probe='{"package":"dvandva-v4","version":"0.2.0","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
if test "${1:-}" = "--version"; then
  case "${DVANDVA_FAKE_MODE:-valid}" in
    valid|probe_*) printf 'dvandva-v4 0.2.0\n' ;;
    version_nul) printf 'dvandva-v4 0.2.0\0\n' ;;
    version_invalid_utf8) printf 'dvandva-v4 0.2.0\377\n' ;;
    version_oversized) printf 'dvandva-v4 0.2.0'; head -c 20000 /dev/zero | tr '\0' '\n' ;;
    version_extra_newline) printf 'dvandva-v4 0.2.0\n\n' ;;
    version_nonzero) printf 'dvandva-v4 0.2.0\n'; exit 7 ;;
  esac
  exit 0
fi
if test "${1:-}" = "probe"; then
  case "${DVANDVA_FAKE_MODE:-valid}" in
    probe_nul) printf '%s\0\n' "$valid_probe" ;;
    probe_invalid_utf8) printf '%s\377\n' "$valid_probe" ;;
    probe_oversized) printf '%s' "$valid_probe"; head -c 20000 /dev/zero | tr '\0' ' ' ;;
    probe_extra_newline) printf '%s\n\n' "$valid_probe" ;;
    probe_nonzero) printf '%s\n' "$valid_probe"; exit 9 ;;
    *) printf '%s\n' "$valid_probe" ;;
  esac
  exit 0
fi
exit 99
ADVERSARIAL_KERNEL
chmod 755 "$binary"
for facade in "$vadi" "$prativadi"; do
  for mode in \
    version_nul version_invalid_utf8 version_oversized \
    version_extra_newline version_nonzero \
    probe_nul probe_invalid_utf8 probe_oversized probe_extra_newline probe_nonzero
  do
    expect_failure 'incompatible kernel' env DVANDVA_FAKE_MODE="$mode" \
      bash "$facade" probe
  done
done
mv "$binary.real" "$binary"

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

# Reviewer-first discovery reaches the kernel without inventing a manifest and
# cannot create a run.
reviewer_first="$(DVANDVA_WAIT_TIMEOUT_MS=1 bash "$prativadi" start \
  reviewer-first claude codex "$workspace" 'Review DEF-123' --wait)"
grep -Fq '"outcome": "none"' <<<"$reviewer_first"
test ! -e "$XDG_STATE_HOME/dvandva/runs"

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
reclaimed="$(bash "$vadi" start codex-session codex claude "$workspace" \
  --run-id "$run_id")"
grep -Fq '"outcome": "started"' <<<"$reclaimed"
grep -Fq '"disposition": "reclaimed"' <<<"$reclaimed"
grep -Fq '"revision": 3' <<<"$reclaimed"
bash "$vadi" read codex-session "$run_dir" | grep -Fq '"revision": 3'
bash "$vadi" heartbeat codex-session "$run_dir" 3 | grep -Fq '"revision":4'
bash "$vadi" wait codex-session "$run_dir" 4 50 | grep -Fq '"revision": 4'

action="$test_root/human.json"
(umask 077; printf '%s\n' '{"type":"request_human_decision","question":"Confirm scope","evidence":["scope changed"],"options":["yes","no"]}' >"$action")
test "$(stat -c '%a' "$action")" = 600
bash "$vadi" apply codex-session "$run_dir" 4 "$action" | grep -Fq '"status": "human_decision"'
rm -f -- "$action"
test ! -e "$action"

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

for role_skill in "$vadi_skill" "$prativadi_skill"; do
  grep -Fq 'Dvandva never creates, replaces, pauses, completes, or clears any harness goal.' \
    "$role_skill"
  grep -Fq 'Goals the user sets in a launch prompt remain outside the protocol.' \
    "$role_skill"
  grep -Fq 'new human scope, ambiguity' "$role_skill"
  grep -Fq 'unavailable mandated publication/review capability' "$role_skill"
  ! grep -Fq 'only for new human scope or ambiguity' "$role_skill"
done

for role_source in \
  "$(cat "$vadi_skill" "$vadi_contract")" \
  "$(cat "$prativadi_skill" "$prativadi_contract")"
do
  for required in \
    'fresh facade snapshot' \
    'next_actions' \
    'advisory_actions' \
    'legal_actions' \
    'scope_mismatch' \
    'complete deliverable manifest' \
    'request_checkpoint_supersession' \
    'accept_checkpoint_supersession' \
    'withdraw_approval' \
    'Codex harness publishes' \
    'Claude harness reviews' \
    'user-created harness goals remain unchanged' \
    'human starts the peer session' \
    'foreground local wait' \
    'upgrade_required' \
    'upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION' \
    'claim SESSION RUN_DIR EXPECTED_REVISION' \
    'reclaim SESSION RUN_DIR EXPECTED_REVISION' \
    'exact `start --run-id` automatically reclaims' \
    'ACTION_FILE' \
    '"type":"resume_human_decision"' \
    '"scope_amendment"'
  do
    grep -Fq "$required" <<<"$role_source"
  done
done

for required in \
  'first user-visible protocol output' \
  'canonical objective and scope' \
  'status and assignee' \
  'next_actions' \
  'peer_prompt' \
  'fresh facade snapshot' \
  'advisory_actions' \
  'legal_actions' \
  'new human scope, ambiguity' \
  'unavailable mandated publication/review capability' \
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
grep -Fq 'mode 0600' <<<"$role_contract"
grep -Fq 'private temporary file' <<<"$role_contract"
grep -Fq 'deletes it after' <<<"$role_contract"
! grep -Fq 'ACTION_JSON' <<<"$role_contract"
grep -Fq 'Publication never substitutes for supersession or withdrawal.' <<<"$role_contract"
grep -Fq 'foreground local wait' <<<"$role_contract"
grep -Fq 'Prativadi never creates a run.' "$prativadi_contract"
! sed -n '/^```text$/,/^```$/p' "$prativadi_contract" | grep -Fq -- '--new-run'

for required in \
  '"type":"submit_checkpoint"' \
  '"type":"request_checkpoint_supersession"' \
  '"type":"withdraw_approval"' \
  '"type":"finalize"'
do
  grep -Fq "$required" "$vadi_contract"
  ! grep -Fq "$required" "$prativadi_contract"
done

for required in \
  '"type":"record_review"' \
  '"type":"accept_checkpoint_supersession"'
do
  grep -Fq "$required" "$prativadi_contract"
  ! grep -Fq "$required" "$vadi_contract"
done

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
  'source and planned release target' \
  'installation is available only after' \
  'tag and release asset exist' \
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
