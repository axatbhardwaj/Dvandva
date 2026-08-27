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
valid_probe='{"package":"dvandva-v4","version":"0.2.0","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
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
reject_text 'package-skills-release: packaged' "$test_root/incompatible.out"
test ! -e "$incompatible/SHA256SUMS" || fail 'incompatible kernel was checksummed'
test ! -e "$incompatible/dvandva-kernel-linux-x86_64" || \
  fail 'incompatible kernel was promoted to the final asset path'

printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if test "${1:-}" = "--version"; then' \
  '  printf "dvandva-v4 9.9.9\\n"' \
  '  exit 0' \
  'fi' \
  'exit 2' \
  >"$test_root/wrong-version-kernel"
chmod 755 "$test_root/wrong-version-kernel"

wrong_binary="$test_root/wrong-binary"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/wrong-version-kernel" \
  CARGO_TARGET_DIR="$test_root/wrong-binary-target" \
  bash "$packager" skills-v0.2.0 "$wrong_binary" \
  >"$test_root/wrong-binary.out" 2>&1; then
  fail 'wrong-version kernel unexpectedly packaged'
fi
require_text 'binary_version_mismatch' "$test_root/wrong-binary.out"
reject_text 'package-skills-release: packaged' "$test_root/wrong-binary.out"
test ! -e "$wrong_binary/SHA256SUMS" || fail 'wrong-version kernel was checksummed'
test ! -e "$wrong_binary/dvandva-kernel-linux-x86_64" || \
  fail 'wrong-version kernel was promoted to the final asset path'

printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if test "${1:-}" = "--version"; then' \
  '  printf "dvandva-v4 0.2.0\\n"' \
  '  exit 0' \
  'fi' \
  'test "${1:-}" = "probe" || exit 2' \
  'case "${FAKE_PROBE_MODE:-valid}" in' \
  "  valid) printf '%s\\n' '$valid_probe' ;;" \
  "  root_duplicate) printf '%s\\n' '{\"package\":\"wrong\",\"package\":\"dvandva-v4\",\"version\":\"0.2.0\",\"publish\":false,\"write_schema\":\"dvandva.run.v2\",\"read_schemas\":[\"dvandva.run.v2\",\"dvandva.run.v1\"],\"role_api\":2,\"capabilities\":{\"upgrade_from_v1\":true},\"compatible\":true}' ;;" \
  "  nested_duplicate) printf '%s\\n' '{\"package\":\"dvandva-v4\",\"version\":\"0.2.0\",\"publish\":false,\"write_schema\":\"dvandva.run.v2\",\"read_schemas\":[\"dvandva.run.v2\",\"dvandva.run.v1\"],\"role_api\":2,\"capabilities\":{\"upgrade_from_v1\":false,\"upgrade_from_v1\":true},\"compatible\":true}' ;;" \
  "  nul) printf '%s\\0' '$valid_probe' ;;" \
  "  oversized) printf '%s' '$valid_probe'; head -c 20000 /dev/zero | tr '\\0' ' ' ;;" \
  "  invalid_utf8) printf '%s\\377' '$valid_probe' ;;" \
  "  malformed_json) printf '%s' '{' ;;" \
  '  *) exit 2 ;;' \
  'esac' \
  >"$test_root/adversarial-kernel"
chmod 755 "$test_root/adversarial-kernel"

expect_rejected_probe() {
  local label="$1"
  local mode="$2"
  local diagnostic="$3"
  local rejected_output="$test_root/$label-output"
  local rejected_log="$test_root/$label.out"

  if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
    FAKE_PROBE_MODE="$mode" CARGO_TARGET_DIR="$test_root/$label-target" \
    bash "$packager" skills-v0.2.0 "$rejected_output" \
    >"$rejected_log" 2>&1; then
    fail "$label probe unexpectedly packaged"
  fi
  require_text "$diagnostic" "$rejected_log"
  reject_text 'package-skills-release: packaged' "$rejected_log"
  if test -e "$rejected_output" || test -L "$rejected_output"; then
    fail "$label probe exposed a final output path"
  fi
}

expect_rejected_probe root-duplicate root_duplicate probe_mismatch
expect_rejected_probe nested-duplicate nested_duplicate probe_mismatch
expect_rejected_probe nul-bearing nul probe_mismatch
expect_rejected_probe oversized oversized probe_too_large
expect_rejected_probe invalid-utf8 invalid_utf8 probe_mismatch
expect_rejected_probe malformed-json malformed_json probe_mismatch

empty_output="$test_root/existing-empty"
mkdir "$empty_output"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
  FAKE_PROBE_MODE=valid CARGO_TARGET_DIR="$test_root/existing-empty-target" \
  bash "$packager" skills-v0.2.0 "$empty_output" \
  >"$test_root/existing-empty.out" 2>&1; then
  fail 'pre-existing empty output directory unexpectedly accepted'
fi
require_text 'output_exists' "$test_root/existing-empty.out"
test -d "$empty_output" || fail 'pre-existing empty output directory was removed'
test -z "$(find "$empty_output" -mindepth 1 -print -quit)" || \
  fail 'pre-existing empty output directory was modified'

symlink_target="$test_root/symlink-target"
symlink_output="$test_root/symlink-output"
mkdir "$symlink_target"
ln -s "$symlink_target" "$symlink_output"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
  FAKE_PROBE_MODE=valid CARGO_TARGET_DIR="$test_root/symlink-target-build" \
  bash "$packager" skills-v0.2.0 "$symlink_output" \
  >"$test_root/symlink.out" 2>&1; then
  fail 'pre-existing output symlink unexpectedly accepted'
fi
require_text 'output_exists' "$test_root/symlink.out"
test -L "$symlink_output" || fail 'pre-existing output symlink was replaced'
test -z "$(find "$symlink_target" -mindepth 1 -print -quit)" || \
  fail 'pre-existing output symlink target was modified'

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

checksum_bin="$test_root/checksum-bin"
mkdir "$checksum_bin"
ln -s "$fake_bin/cargo" "$checksum_bin/cargo"
ln -s "$fake_bin/strip" "$checksum_bin/strip"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "%s\\n" "$PWD" >"$CHECKSUM_PWD_FILE"' \
  'exit 1' \
  >"$checksum_bin/sha256sum"
chmod 755 "$checksum_bin/sha256sum"
checksum_output="$test_root/checksum-output"
if PATH="$checksum_bin:$PATH" FAKE_KERNEL="$output/dvandva-kernel-linux-x86_64" \
  CHECKSUM_PWD_FILE="$test_root/checksum.pwd" \
  CARGO_TARGET_DIR="$test_root/checksum-target" \
  bash "$packager" skills-v0.2.0 "$checksum_output" \
  >"$test_root/checksum.out" 2>&1; then
  fail 'checksum failure unexpectedly packaged'
fi
reject_text 'package-skills-release: packaged' "$test_root/checksum.out"
if test -e "$checksum_output" || test -L "$checksum_output"; then
  fail 'checksum failure exposed a partial output path'
fi
checksum_staging="$(cat "$test_root/checksum.pwd")"
test "$(dirname -- "$checksum_staging")" = "$test_root" || \
  fail 'checksum did not run in a sibling staging directory'
case "$(basename -- "$checksum_staging")" in
  .checksum-output.tmp.*) ;;
  *) fail 'checksum staging directory did not use the hidden output prefix' ;;
esac

collision_bin="$test_root/collision-bin"
mkdir "$collision_bin"
ln -s "$fake_bin/cargo" "$collision_bin/cargo"
ln -s "$fake_bin/strip" "$collision_bin/strip"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'destination="${@: -1}"' \
  'printf "foreign\\n" >"$destination"' \
  'exec /usr/bin/mv "$@"' \
  >"$collision_bin/mv"
chmod 755 "$collision_bin/mv"
collision_output="$test_root/collision-output"
if PATH="$collision_bin:$PATH" FAKE_KERNEL="$output/dvandva-kernel-linux-x86_64" \
  CARGO_TARGET_DIR="$test_root/collision-target" \
  bash "$packager" skills-v0.2.0 "$collision_output" \
  >"$test_root/collision.out" 2>&1; then
  fail 'promotion collision unexpectedly packaged'
fi
require_text 'output_exists' "$test_root/collision.out"
reject_text 'package-skills-release: packaged' "$test_root/collision.out"
test -f "$collision_output" || fail 'promotion collision replaced the foreign path'
if test -f "$collision_output"; then
  require_text 'foreign' "$collision_output"
fi

wrong="$test_root/wrong"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.1 "$wrong" \
  >"$test_root/wrong.out" 2>&1; then
  printf 'mismatched tag unexpectedly packaged\n' >&2
  exit 1
fi
grep -Fq 'version_mismatch' "$test_root/wrong.out"
test ! -e "$wrong"

nonempty="$test_root/nonempty"
mkdir "$nonempty"
touch "$nonempty/foreign"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.0 "$nonempty" \
  >"$test_root/nonempty.out" 2>&1; then
  printf 'non-empty destination unexpectedly accepted\n' >&2
  exit 1
fi
require_text 'output_exists' "$test_root/nonempty.out"
test -f "$nonempty/foreign"

workflow="$repo_root/.github/workflows/skills-release.yml"
python3 - "$workflow" <<'PY'
import sys
from ruamel.yaml import YAML

with open(sys.argv[1], encoding="utf-8") as stream:
    workflow = YAML(typ="safe").load(stream)

assert isinstance(workflow, dict)
assert set(workflow) == {"name", "on", "permissions", "jobs"}
assert workflow["name"] == "Skills v4"
assert set(workflow["on"]) == {"push", "pull_request"}
assert workflow["on"]["push"] == {
    "branches": ["**"],
    "tags": ["skills-v*"],
}
assert workflow["on"]["pull_request"] is None
assert workflow["on"]["push"]["tags"] == ["skills-v*"]
assert workflow["permissions"] == {"contents": "read"}
assert set(workflow["jobs"]) == {"verify", "release"}
assert workflow["jobs"]["release"]["if"] == "startsWith(github.ref, 'refs/tags/skills-v')"
assert workflow["jobs"]["release"]["needs"] == "verify"
assert workflow["jobs"]["release"]["permissions"] == {"contents": "write"}
steps = workflow["jobs"]["verify"]["steps"]
assert [step["name"] for step in steps] == [
    "Check out the tested revision",
    "Select stable Rust",
    "Install release-test tooling",
    "Verify v4 kernel",
    "Verify archived v3 remains intact",
    "Verify setup skill",
    "Verify role skills",
    "Verify release packaging",
    "Verify two-role canary",
]
runs = "\n".join(step.get("run", "") for step in steps)
assert "cargo fmt --manifest-path v4/Cargo.toml -- --check" in runs
assert "cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings" in runs
assert "cargo test --manifest-path v4/Cargo.toml --all-targets --locked" in runs
assert "probe --expected-schema dvandva.run.v2 --expected-role-api 2" in runs
assert "cargo test --manifest-path rust/dvandva/Cargo.toml --locked" in runs
assert "bash tests/skills/setup-dvandva.sh" in runs
assert "bash tests/skills/role-skills.sh" in runs
assert "bash tests/skills/package-release.sh" in runs
assert "bash tests/skills/two-role-canary.sh" in runs
checkout = next(step for step in steps if step["name"] == "Check out the tested revision")
assert checkout["uses"] == "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
assert checkout["with"]["fetch-depth"] == 0
release_steps = workflow["jobs"]["release"]["steps"]
assert [step["name"] for step in release_steps] == [
    "Check out the release tag",
    "Select stable Rust",
    "Package the private kernel",
    "Publish GitHub release",
]
release_checkout = next(step for step in release_steps if step["name"] == "Check out the release tag")
assert release_checkout["uses"] == "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
assert release_checkout["with"]["fetch-depth"] == 0
package = next(step for step in release_steps if step["name"] == "Package the private kernel")
assert package["run"] == 'bash scripts/package-skills-release.sh "$GITHUB_REF_NAME" artifacts'
publish = next(step for step in release_steps if step["name"] == "Publish GitHub release")
assert publish["env"] == {"GH_TOKEN": "${{ github.token }}"}
publish_run = publish["run"]
assert 'release_version="${GITHUB_REF_NAME#skills-v}"' in publish_run
assert 'gh release create "$GITHUB_REF_NAME"' in publish_run
assert "artifacts/dvandva-kernel-linux-x86_64" in publish_run
assert "artifacts/SHA256SUMS" in publish_run
assert "artifacts/*" not in publish_run
assert "--verify-tag" in publish_run
assert 'Dvandva skills $release_version' in publish_run
assert 'Kernel $release_version writes dvandva.run.v2' in publish_run
PY
require_text 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' "$workflow"
require_text 'probe --expected-schema dvandva.run.v2 --expected-role-api 2' "$workflow"
reject_text 'cargo publish' "$workflow"

python3 - "$repo_root/v4/Cargo.toml" <<'PY' || fail 'v4 crate is not explicitly private'
import sys
import tomllib

with open(sys.argv[1], "rb") as stream:
    manifest = tomllib.load(stream)
assert manifest["package"]["publish"] is False
PY
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
require_text 'explainer, including its plan/TODO' "$claude_active"

require_text 'dvandva.run.v2' "$repo_root/v4/README.md"
require_text 'role API 2' "$repo_root/v4/README.md"
require_text 'dedicated upgrade' "$repo_root/v4/README.md"
require_text '--repository-id' "$repo_root/v4/README.md"
require_text '--required-deliverable' "$repo_root/v4/README.md"

protocol="$repo_root/docs/protocol/minimal-run-baton.md"
require_text 'scope_revision' "$protocol"
require_text 'manifest_digest' "$protocol"
require_text 'request_checkpoint_supersession' "$protocol"
require_text 'withdraw_approval' "$protocol"
require_text 'Codex Sites' "$protocol"
require_text 'Claude' "$protocol"
require_text 'goals' "$protocol"
reject_text 'projection revision' "$protocol"
reject_text 'publication is optional' "$protocol"
reject_text 'prativadi implements fixes' "$protocol"
reject_text 'Claude Artifact fallback' "$protocol"
reject_text 'active schema is dvandva.run.v1' "$protocol"

require_text 'Historical v3 archive guidance' \
  "$repo_root/docs/workflows/two-mode-agent-workflow.md"
require_text 'docs/workflows/skill-only-run.md' \
  "$repo_root/docs/workflows/two-mode-agent-workflow.md"
test "$(sed -n '/^# Two-Mode Agent Workflow/,$p' \
  "$repo_root/docs/workflows/two-mode-agent-workflow.md" | sha256sum | cut -d' ' -f1)" = \
  '12d51f85fc0ec5e945e99122f465ee5dcad205604993eeb7cb1340f65acdf6b8' || \
  fail 'historical two-mode workflow body changed'

test "$(sed -n '/^## Retired v3 archive/,$p' "$repo_root/README.md" | \
  sha256sum | cut -d' ' -f1)" = \
  '83182b2773ae4c52a71c2568cb856770b4269d1cdeb47bd92fb400fa4807629a' || \
  fail 'README archive changed'

context="$repo_root/CONTEXT.md"
for term in 'Canonical Scope' 'Checkpoint Manifest' 'Checkpoint Binding' \
  'Checkpoint Supersession' 'Approval Withdrawal' 'Protocol Upgrade' \
  'Publication Gate' 'Harness Goal'; do
  require_text "**$term**" "$context"
done

adr="$repo_root/docs/adr/0003-run-v2-security-epoch.md"
require_text 'status: accepted' "$adr"
require_text 'dvandva.run.v2' "$adr"
require_text 'role API 2' "$adr"
require_text 'structural receipts are not provider-signed proof' "$adr"
require_text 'future protocol epoch' "$adr"

if test "$failures" -ne 0; then
  printf 'skills release packaging: %s failure(s)\n' "$failures" >&2
  exit 1
fi

printf 'skills release packaging: ok\n'
