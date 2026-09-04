#!/usr/bin/env bash
set -euo pipefail
# Byte-order collation: filename comparisons below must not depend on the
# invoking user's locale.
export LC_ALL=C

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
# The facade resolves the pinned version directory directly; `current` is only
# the shared default selector and must not be able to break a pinned session.
binary="$XDG_DATA_HOME/dvandva/bin/0.3.5/dvandva-kernel"
mkdir -p "$(dirname "$binary")"
cp "$repo_root/v4/target/debug/dvandva-v4" "$binary"
ln -s 0.3.5 "$XDG_DATA_HOME/dvandva/bin/current"

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
grep -Fq '"version": "0.3.5"' <<<"$probe"
grep -Fq '"write_schema": "dvandva.run.v2"' <<<"$probe"
grep -Fq '"read_schemas": [' <<<"$probe"
grep -Fq '"role_api": 2' <<<"$probe"
grep -Fq '"upgrade_from_v1": true' <<<"$probe"
grep -Fq '"publish": false' <<<"$probe"

# Handshake validation preserves raw bytes, size, and producer status.
mv "$binary" "$binary.real"
cat >"$binary" <<'ADVERSARIAL_KERNEL'
#!/usr/bin/env bash
valid_probe='{"package":"dvandva-v4","version":"0.3.5","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
if test "${1:-}" = "--version"; then
  case "${DVANDVA_FAKE_MODE:-valid}" in
    valid|probe_*) printf 'dvandva-v4 0.3.5\n' ;;
    version_nul) printf 'dvandva-v4 0.3.5\0\n' ;;
    version_invalid_utf8) printf 'dvandva-v4 0.3.5\377\n' ;;
    version_oversized) printf 'dvandva-v4 0.3.5'; head -c 300 /dev/zero | tr '\0' x ;;
    version_extra_newline) printf 'dvandva-v4 0.3.5\n\n' ;;
    version_nonzero) printf 'dvandva-v4 0.3.5\n'; exit 7 ;;
  esac
  exit 0
fi
if test "${1:-}" = "probe"; then
  case "${DVANDVA_FAKE_MODE:-valid}" in
    probe_nul) printf '%s\0\n' "$valid_probe" ;;
    probe_invalid_utf8) printf '%s\377\n' "$valid_probe" ;;
    probe_oversized) printf '{'; head -c 17000 /dev/zero | tr '\0' ' '; printf '%s' "${valid_probe:1}" ;;
    probe_extra_newline) printf '%s\n\n' "$valid_probe" ;;
    probe_trailing_space) printf '%s ' "$valid_probe" ;;
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
    probe_nul probe_invalid_utf8 probe_oversized probe_extra_newline \
    probe_trailing_space probe_nonzero
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
if test "${1:-}" = "--version"; then printf 'dvandva-v4 0.3.5\n'; exit 0; fi
if test "${1:-}" = "probe"; then
  printf '%s\n' '{' \
    '  "package": 7, "version": false, "publish": true, "write_schema": [],' \
    '  "read_schemas": "wrong", "role_api": "2",' \
    '  "capabilities": {"upgrade_from_v1": "true"}, "compatible": "true",' \
    '  "decoy": {' \
    '    "package": "dvandva-v4", "version": "0.3.5", "publish": false,' \
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

# A concurrent session flipping the shared bin/current selector must not break
# this pinned session, and the pinned path must never consult the selector.
rm "$XDG_DATA_HOME/dvandva/bin/current"
ln -s 0.0.0 "$XDG_DATA_HOME/dvandva/bin/current"
bash "$vadi" read codex-session "$run_dir" | grep -Fq '"revision": 3'
rm "$XDG_DATA_HOME/dvandva/bin/current"
ln -s 0.3.5 "$XDG_DATA_HOME/dvandva/bin/current"

# Observe is claim-independent and read-only: a session with no credential can
# watch the run, sees the explicit read-only marker, and never moves the head.
expect_failure 'error' bash "$vadi" read watcher-session "$run_dir"
observed="$(bash "$vadi" observe watcher-session "$run_dir")"
grep -Fq '"outcome": "observed"' <<<"$observed"
grep -Fq '"read_only": true' <<<"$observed"
grep -Fq '"revision": 3' <<<"$observed"
bash "$vadi" read codex-session "$run_dir" | grep -Fq '"revision": 3'

bash "$vadi" heartbeat codex-session "$run_dir" 3 | grep -Fq '"revision":4'
bash "$vadi" wait codex-session "$run_dir" 4 50 | grep -Fq '"revision": 4'

action="$test_root/human.json"
(umask 077; printf '%s\n' '{"type":"request_human_decision","kind":"scope","question":"Which sections are in scope","evidence":["scope changed"],"options":["all","only the kernel"]}' >"$action")
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

# A Codex-looking alias is intentionally not the canonical Codex identity. The
# supported facade carries that pair through both local explainer handoffs to
# terminal state without a Sites receipt.
export DVANDVA_LEASE_SECONDS=300
no_codex_worker="$(bash "$vadi" start alias-worker codex-cli claude "$workspace" \
  'Complete without Sites' NO-CODEX --new-run \
  --required-deliverable implementation='Complete without Sites')"
no_codex_run_id="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' \
  <<<"$no_codex_worker")"
no_codex_dir="$XDG_STATE_HOME/dvandva/runs/$no_codex_run_id"
no_codex_reviewer="$(bash "$prativadi" start alias-reviewer claude codex-cli "$workspace" \
  --run-id "$no_codex_run_id")"
grep -Fq '"harness": "codex-cli"' <<<"$no_codex_worker"
grep -Fq '"harness": "Claude"' <<<"$no_codex_reviewer"

no_codex_source="$test_root/no-codex.html"
no_codex_action="$test_root/no-codex-action.json"
(umask 077; printf '%s\n' '<!doctype html><title>Non-Codex status</title>' >"$no_codex_source")
(umask 077; printf '%s\n' \
  "{\"type\":\"stage_explainer\",\"obligation\":{\"handoff_revision\":0,\"kind\":\"run_started\",\"scope_revision\":0},\"after_seq\":0,\"source_path\":\"$no_codex_source\"}" \
  >"$no_codex_action")
no_codex_staged="$(bash "$vadi" apply alias-worker "$no_codex_dir" 2 "$no_codex_action")"
no_codex_digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["publication_binding"]["artifact"]["source_digest"])' \
  <<<"$no_codex_staged")"
printf '%s\n' \
  "{\"type\":\"record_explainer_review\",\"obligation\":{\"handoff_revision\":0,\"kind\":\"run_started\",\"scope_revision\":0},\"after_seq\":1,\"source_digest\":\"$no_codex_digest\",\"verdict\":\"approved\",\"findings\":[]}" \
  >"$no_codex_action"
bash "$prativadi" apply alias-reviewer "$no_codex_dir" 3 "$no_codex_action" >/dev/null

no_codex_checkpoint=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
printf '%s\n' \
  "{\"type\":\"submit_checkpoint\",\"checkpoint\":{\"kind\":\"git\",\"identity\":\"$no_codex_checkpoint\",\"deliverables\":[{\"id\":\"implementation\",\"artifacts\":[{\"kind\":\"commit\",\"value\":\"$no_codex_checkpoint\"}]}],\"verification\":[\"tests passed\"]}}" \
  >"$no_codex_action"
no_codex_submitted="$(bash "$vadi" apply alias-worker "$no_codex_dir" 4 "$no_codex_action")"
no_codex_manifest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["checkpoint"]["manifest_digest"])' \
  <<<"$no_codex_submitted")"
printf '%s\n' \
  "{\"type\":\"record_review\",\"verdict\":\"approved\",\"checkpoint_identity\":\"$no_codex_checkpoint\",\"manifest_digest\":\"$no_codex_manifest\",\"scope_revision\":0,\"findings\":[]}" \
  >"$no_codex_action"
bash "$prativadi" apply alias-reviewer "$no_codex_dir" 5 "$no_codex_action" >/dev/null

# Approval preserved the delivery obligation: stage its explainer once and finalize.
printf '%s\n' '<!doctype html><title>Non-Codex complete</title>' >"$no_codex_source"
printf '%s\n' \
  "{\"type\":\"stage_explainer\",\"obligation\":{\"handoff_revision\":5,\"kind\":\"worker_to_reviewer\",\"scope_revision\":0,\"checkpoint\":{\"checkpoint_identity\":\"$no_codex_checkpoint\",\"manifest_digest\":\"$no_codex_manifest\",\"scope_revision\":0}},\"after_seq\":0,\"source_path\":\"$no_codex_source\"}" \
  >"$no_codex_action"
no_codex_final_stage="$(bash "$vadi" apply alias-worker "$no_codex_dir" 6 "$no_codex_action")"
no_codex_final_digest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["publication_binding"]["artifact"]["source_digest"])' \
  <<<"$no_codex_final_stage")"
printf '%s\n' \
  "{\"type\":\"record_explainer_review\",\"obligation\":{\"handoff_revision\":5,\"kind\":\"worker_to_reviewer\",\"scope_revision\":0,\"checkpoint\":{\"checkpoint_identity\":\"$no_codex_checkpoint\",\"manifest_digest\":\"$no_codex_manifest\",\"scope_revision\":0}},\"after_seq\":1,\"source_digest\":\"$no_codex_final_digest\",\"verdict\":\"approved\",\"findings\":[]}" \
  >"$no_codex_action"
no_codex_approved="$(bash "$prativadi" apply alias-reviewer "$no_codex_dir" 7 "$no_codex_action")"
grep -Fq '"deployment": null' <<<"$no_codex_approved"
no_codex_finalizer="$(bash "$vadi" read alias-worker "$no_codex_dir")"
grep -Fq '"finalize"' <<<"$no_codex_finalizer"
printf '%s\n' '{"type":"finalize"}' >"$no_codex_action"
no_codex_done="$(bash "$vadi" apply alias-worker "$no_codex_dir" 8 "$no_codex_action")"
grep -Fq '"status": "done"' <<<"$no_codex_done"
grep -Fq '"outcome": "done"' <<<"$no_codex_done"

# A terminal run stays observable without a claim, so a watcher can tell a
# finished run from its own lapsed claim without a mutating start --run-id.
terminal_observed="$(bash "$prativadi" observe watcher-session "$no_codex_dir")"
grep -Fq '"outcome": "observed"' <<<"$terminal_observed"
grep -Fq '"read_only": true' <<<"$terminal_observed"
grep -Fq '"status": "done"' <<<"$terminal_observed"
unlink "$no_codex_action"
unlink "$no_codex_source"

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
  grep -Fq 'the human'"'"'s alone' "$role_skill"
  grep -Fq 'Protocol-internal problems resolve autonomously' "$role_skill"
  grep -Fq 'the human may be absent' "$role_skill"
  grep -Fq 'is a human interrupt: it force-ends the' "$role_skill"
  grep -Fq 'A bare continue or an empty resume is never a stop and never' "$role_skill"
  ! grep -Fq 'only for new human scope or ambiguity' "$role_skill"
done

for role_source in \
  "$(cat "$vadi_skill" "$vadi_contract")" \
  "$(cat "$prativadi_skill" "$prativadi_contract")"
do
  for required in \
    'fresh facade snapshot' \
    'The first `poll` is illegal until' \
    'is a human interrupt: it force-ends the' \
    'A bare continue or an empty resume is never a stop and never' \
    'next_actions' \
    'advisory_actions' \
    'legal_actions' \
    'scope_mismatch' \
    'complete deliverable manifest' \
    'request_checkpoint_supersession' \
    'accept_checkpoint_supersession' \
    'withdraw_approval' \
    'Vadi stages' \
    'prativadi reviews' \
    'If neither participant is Codex' \
    'user-created harness goals remain unchanged' \
    'human starts the peer session' \
    'foreground local wait' \
    'Ending the turn is not a wait' \
  'Ending the turn is not a wait' \
  'poll  SESSION RUN_DIR AFTER_REVISION' \
    'upgrade_required' \
    'upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION' \
    'repair-policy SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION' \
    'explainer SESSION RUN_DIR' \
    'analysis SESSION RUN_DIR DIGEST' \
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
  'never for protocol approval' \
  'never block on human approval' \
  'no approval kind' \
  'wait_outcome: idle_timeout' \
  'never leaves `request_human_decision` as the only way forward' \
  'never an ordinary wake or action' \
  'scope_mismatch' \
  'complete deliverable manifest' \
  'canonical deliverable IDs exactly once' \
  'manifest_digest' \
  'scope_revision' \
  'request_checkpoint_supersession' \
  'accept_checkpoint_supersession' \
  'withdraw_approval' \
  'Vadi stages' \
  'prativadi reviews' \
  'regardless of which harness fills either role' \
  'For `run_started`' \
  'the gate binds a digest, not a URL' \
  'canonical scope, complete manifest, findings and decisions, and a current plan/TODO' \
  'stage_explainer' \
  'explainer/<source_digest>.html' \
  'stable Site ID' \
  'new Site version' \
  'required work for whichever participant is Codex' \
  'If neither participant is Codex' \
  'status page' \
  'sites:sites-building' \
  'sites:sites-hosting' \
  'When Codex participates, finalization requires both' \
  'Never record a verdict on bytes you did not read' \
  'Claude Artifact' \
  'generic publisher' \
  'silent fallback' \
  'publication_unreadable' \
  'repair-policy' \
  'report_progress' \
  'slow from dead' \
  'user-created harness goals remain unchanged' \
  'human starts the peer session' \
  'user-invoked workflow skills' \
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
  '0.3.5' \
  'skills-v0.3.5' \
  'release target' \
  'fails closed if either is missing' \
  'Linux x86_64 only' \
  'only Linux x86_64 is supported for now' \
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
