#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
packager="$repo_root/scripts/package-skills-release.sh"
failures=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  failures=$((failures + 1))
}

require_text() {
  local needle="$1"
  local file="$2"
  grep -Fq -- "$needle" "$file" || fail "$file missing: $needle"
}

reject_text() {
  local needle="$1"
  local file="$2"
  if grep -Fq -- "$needle" "$file"; then
    fail "$file unexpectedly contains: $needle"
  fi
}

fake_bin="$test_root/fake-bin"
fake_target="$test_root/fake-target"
mkdir -p "$fake_bin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'mkdir -p "$CARGO_TARGET_DIR/release"' \
  'cp "$FAKE_KERNEL" "$CARGO_TARGET_DIR/release/dvandva-v4"' \
  'chmod 755 "$CARGO_TARGET_DIR/release/dvandva-v4"' \
  >"$fake_bin/cargo"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$fake_bin/strip"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if test "${1:-}" = "--version"; then' \
  '  printf "dvandva-v4 0.2.0\\n"' \
  '  exit 0' \
  'fi' \
  'if test "${1:-}" = "probe"; then' \
  "  printf '%s\\n' '{\"package\":\"dvandva-v4\",\"version\":\"0.2.0\",\"publish\":false,\"write_schema\":\"dvandva.run.v1\",\"read_schemas\":[\"dvandva.run.v1\"],\"role_api\":1,\"capabilities\":{\"upgrade_from_v1\":false},\"compatible\":true}'" \
  '  exit 0' \
  'fi' \
  'exit 2' \
  >"$test_root/fake-kernel"
chmod 755 "$fake_bin/cargo" "$fake_bin/strip" "$test_root/fake-kernel"

incompatible="$test_root/incompatible"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/fake-kernel" \
  CARGO_TARGET_DIR="$fake_target" \
  bash "$packager" skills-v0.2.0 "$incompatible" \
  >"$test_root/incompatible.out" 2>&1; then
  fail 'correct-version v1/API1 kernel unexpectedly packaged'
fi
require_text 'probe_mismatch' "$test_root/incompatible.out"
test ! -e "$incompatible/SHA256SUMS" || fail 'incompatible kernel was checksummed'

output="$test_root/output"
CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.0 "$output"

test -x "$output/dvandva-kernel-linux-x86_64"
test -f "$output/SHA256SUMS"
test "$(find "$output" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' ')" = \
  'SHA256SUMS dvandva-kernel-linux-x86_64 '
(cd "$output" && sha256sum -c SHA256SUMS)
test "$($output/dvandva-kernel-linux-x86_64 --version)" = 'dvandva-v4 0.2.0'
"$output/dvandva-kernel-linux-x86_64" probe \
  --expected-schema dvandva.run.v2 --expected-role-api 2 \
  >"$test_root/probe.json"
python3 - "$test_root/probe.json" <<'PY'
import json
import sys


def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate key: {key}")
        value[key] = item
    return value


with open(sys.argv[1], encoding="utf-8") as stream:
    probe = json.load(stream, object_pairs_hook=unique)

assert set(probe) == {
    "package", "version", "publish", "write_schema", "read_schemas",
    "role_api", "capabilities", "compatible",
}
assert probe == {
    "package": "dvandva-v4",
    "version": "0.2.0",
    "publish": False,
    "write_schema": "dvandva.run.v2",
    "read_schemas": ["dvandva.run.v2", "dvandva.run.v1"],
    "role_api": 2,
    "capabilities": {"upgrade_from_v1": True},
    "compatible": True,
}
PY

wrong="$test_root/wrong"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.1 "$wrong" \
  >"$test_root/wrong.out" 2>&1; then
  printf 'mismatched tag unexpectedly packaged\n' >&2
  exit 1
fi
grep -Fq 'version_mismatch' "$test_root/wrong.out"
test ! -e "$wrong"

nonempty="$test_root/nonempty"
mkdir -p "$nonempty"
touch "$nonempty/foreign"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.0 "$nonempty" \
  >"$test_root/nonempty.out" 2>&1; then
  printf 'non-empty destination unexpectedly accepted\n' >&2
  exit 1
fi
grep -Fq 'output_not_empty' "$test_root/nonempty.out"
test -f "$nonempty/foreign"

workflow="$repo_root/.github/workflows/skills-release.yml"
python3 - "$workflow" <<'PY'
import sys
from ruamel.yaml import YAML

with open(sys.argv[1], encoding="utf-8") as stream:
    workflow = YAML(typ="safe").load(stream)

assert isinstance(workflow, dict)
assert workflow["on"]["push"]["tags"] == ["skills-v*"]
assert workflow["permissions"] == {"contents": "read"}
assert workflow["jobs"]["release"]["if"] == "startsWith(github.ref, 'refs/tags/skills-v')"
assert workflow["jobs"]["release"]["permissions"] == {"contents": "write"}
steps = workflow["jobs"]["verify"]["steps"]
runs = "\n".join(step.get("run", "") for step in steps)
assert "cargo test --manifest-path rust/dvandva/Cargo.toml --locked" in runs
assert "bash tests/skills/two-role-canary.sh" in runs
PY
require_text 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' "$workflow"
require_text 'probe --expected-schema dvandva.run.v2 --expected-role-api 2' "$workflow"
reject_text 'cargo publish' "$workflow"

test "$(sed -n '/^publish = /p' "$repo_root/v4/Cargo.toml")" = 'publish = false' || \
  fail 'v4 crate is not explicitly private'
reject_text 'cargo publish' "$repo_root/scripts/package-skills-release.sh"

workflow_doc="$repo_root/docs/workflows/skill-only-run.md"
require_text '--agent claude-code codex' "$workflow_doc"
require_text '--skill setup-dvandva vadi prativadi' "$workflow_doc"
require_text 'skills-v0.2.0' "$workflow_doc"
require_text 'Act as prativadi and join Dvandva run <run-id>.' "$workflow_doc"
require_text 'scope_mismatch' "$workflow_doc"
require_text 'request_checkpoint_supersession' "$workflow_doc"
require_text 'withdraw_approval' "$workflow_doc"
require_text 'Codex Sites' "$workflow_doc"
require_text 'Claude' "$workflow_doc"
require_text 'Human Decision' "$workflow_doc"

readme_active="$test_root/readme-active.md"
sed '/^## Retired v3 archive/,$d' "$repo_root/README.md" >"$readme_active"
require_text 'skills-v0.2.0' "$readme_active"
require_text 'dvandva.run.v2' "$readme_active"
require_text 'role API 2' "$readme_active"
require_text 'Act as prativadi and join Dvandva run <run-id>.' "$readme_active"
require_text 'Codex Sites' "$readme_active"
require_text 'Claude' "$readme_active"
require_text 'user-owned' "$readme_active"

claude_active="$test_root/claude-active.md"
sed '/^## Historical model discipline/,$d' "$repo_root/CLAUDE.md" >"$claude_active"
require_text 'next_actions' "$claude_active"
require_text 'Codex Sites' "$claude_active"
require_text 'Claude' "$claude_active"
require_text 'goals' "$claude_active"

require_text 'dvandva.run.v2' "$repo_root/v4/README.md"
require_text 'role API 2' "$repo_root/v4/README.md"
require_text 'dedicated upgrade' "$repo_root/v4/README.md"
require_text '--repository-id' "$repo_root/v4/README.md"
require_text '--deliverable' "$repo_root/v4/README.md"

protocol="$repo_root/docs/protocol/minimal-run-baton.md"
require_text 'scope_revision' "$protocol"
require_text 'manifest_digest' "$protocol"
require_text 'request_checkpoint_supersession' "$protocol"
require_text 'withdraw_approval' "$protocol"
require_text 'Codex Sites' "$protocol"
require_text 'Claude' "$protocol"
require_text 'goals' "$protocol"
reject_text 'projection revision' "$protocol"

require_text 'Historical v3 archive guidance' \
  "$repo_root/docs/workflows/two-mode-agent-workflow.md"
require_text 'docs/workflows/skill-only-run.md' \
  "$repo_root/docs/workflows/two-mode-agent-workflow.md"

context="$repo_root/CONTEXT.md"
for term in 'Canonical Scope' 'Checkpoint Manifest' 'Checkpoint Binding' \
  'Checkpoint Supersession' 'Publication Gate' 'Harness Goal'; do
  require_text "**$term**" "$context"
done

adr="$repo_root/docs/adr/0003-run-v2-security-epoch.md"
require_text 'status: accepted' "$adr"
require_text 'dvandva.run.v2' "$adr"
require_text 'role API 2' "$adr"
require_text 'provider-signed' "$adr"
require_text 'future protocol epoch' "$adr"

if test "$failures" -ne 0; then
  printf 'skills release packaging: %s failure(s)\n' "$failures" >&2
  exit 1
fi

printf 'skills release packaging: ok\n'
