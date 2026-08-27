#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
release_dir="$test_root/release"
mkdir -p "$release_dir"
cp "$repo_root/v4/target/debug/dvandva-v4" \
  "$release_dir/dvandva-kernel-linux-x86_64"
(cd "$release_dir" && sha256sum dvandva-kernel-linux-x86_64 >SHA256SUMS)

export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
export DVANDVA_RELEASE_DIR="$release_dir"
bash "$repo_root/skills/setup-dvandva/scripts/setup-dvandva.sh" \
  install --version 0.1.0 >/dev/null

workspace="$test_root/workspace"
mkdir -p "$workspace"
git -C "$workspace" init --quiet
git -C "$workspace" remote add origin git@github.com:axatbhardwaj/Dvandva.git

vadi="$repo_root/skills/vadi/scripts/dvandva-role.sh"
prativadi="$repo_root/skills/prativadi/scripts/dvandva-role.sh"

export CODEX_SESSION_ID="codex-session"
test "$(bash "$vadi" session-id)" = "codex-session"
generated="$(env -u CODEX_SESSION_ID bash "$vadi" session-id --generate)"
[[ "$generated" =~ ^[0-9a-f-]{36}$ ]]
bash "$vadi" probe | grep -Fq '"compatible": true'

worker="$(bash "$vadi" start codex-session codex claude "$workspace" \
  'Implement DEF-123' DEF-123)"
grep -Fq '"disposition": "created"' <<<"$worker"
run_id="$(sed -n 's/.*"run_id": "\([^"]*\)".*/\1/p' <<<"$worker")"
run_dir="$XDG_STATE_HOME/dvandva/runs/$run_id"

reviewer="$(bash "$prativadi" start claude-session claude codex "$workspace" \
  'Implement DEF-123' DEF-123 --wait)"
grep -Fq '"disposition": "claimed"' <<<"$reviewer"
grep -Fq "\"run_id\": \"$run_id\"" <<<"$reviewer"

bash "$vadi" read codex-session "$run_dir" | grep -Fq '"status": "working"'
bash "$prativadi" read claude-session "$run_dir" | grep -Fq '"status": "working"'

action="$test_root/checkpoint.json"
printf '%s\n' \
  '{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","verification":["test"]}}' \
  >"$action"
bash "$vadi" apply codex-session "$run_dir" 2 "$action" |
  grep -Fq '"status": "reviewing"'

cmp "$vadi" "$prativadi"

grep -Fq 'act as vadi' "$repo_root/skills/vadi/SKILL.md"
grep -Fq 'implement as vadi' "$repo_root/skills/vadi/SKILL.md"
grep -Fq 'act as prativadi' "$repo_root/skills/prativadi/SKILL.md"
grep -Fq 'join the current run as prativadi' "$repo_root/skills/prativadi/SKILL.md"
grep -Fq 'Never invoke the peer harness' "$repo_root/skills/vadi/SKILL.md"
grep -Fq 'Never invoke or wake the peer harness' "$repo_root/skills/prativadi/SKILL.md"
grep -Fq 'Matt Pocock skill unless the human explicitly invokes' \
  "$repo_root/skills/vadi/SKILL.md"
grep -Fq 'Matt Pocock skill unless the human explicitly invokes' \
  "$repo_root/skills/prativadi/SKILL.md"

skill_list="$(npx --yes skills add "$repo_root" --list)"
grep -Fq 'Found 3 skills' <<<"$skill_list"
for skill in setup-dvandva vadi prativadi; do
  grep -Fq "$skill" <<<"$skill_list"
done

skills_home="$test_root/skills-home"
mkdir -p "$skills_home"
HOME="$skills_home" npx --yes skills add "$repo_root" --copy --global \
  --agent claude-code codex --skill setup-dvandva vadi prativadi -y >/dev/null
for skill in setup-dvandva vadi prativadi; do
  test -f "$skills_home/.claude/skills/$skill/SKILL.md"
  test -f "$skills_home/.agents/skills/$skill/SKILL.md"
done

printf 'role skill wrappers: ok\n'
