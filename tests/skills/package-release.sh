#!/usr/bin/env bash
set -euo pipefail
# Byte-order collation: filename comparisons below must not depend on the
# invoking user's locale.
export LC_ALL=C

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
packager="$repo_root/scripts/package-skills-release.sh"
ref_verifier="$repo_root/scripts/verify-skills-release-ref.sh"
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

ref_fake_bin="$test_root/ref-fake-bin"
mkdir -p "$ref_fake_bin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'case "${1:-}" in' \
  '  show-ref)' \
  '    test "$#" -eq 4' \
  '    test "$2" = "--verify"' \
  '    test "$3" = "--hash"' \
  '    test "$4" = "refs/tags/skills-v0.3.9"' \
  '    test "${FAKE_LOCAL_OBJECT+x}" = x || exit 1' \
  '    printf "%b" "$FAKE_LOCAL_OBJECT"' \
  '    ;;' \
  '  rev-parse)' \
  '    test "$#" -eq 3' \
  '    test "$2" = "--verify"' \
  '    test "$3" = "refs/tags/skills-v0.3.9^{commit}"' \
  '    test "${FAKE_LOCAL_PEELED+x}" = x || exit 1' \
  '    printf "%b" "$FAKE_LOCAL_PEELED"' \
  '    ;;' \
  '  ls-remote)' \
  '    test "$#" -eq 5' \
  '    test "$2" = "--tags"' \
  '    test "$3" = "origin"' \
  '    test "$4" = "refs/tags/skills-v0.3.9"' \
  '    test "$5" = "refs/tags/skills-v0.3.9^{}"' \
  '    test "${FAKE_REMOTE_REFS+x}" = x || exit 1' \
  '    printf "%b" "$FAKE_REMOTE_REFS"' \
  '    ;;' \
  '  *) exit 2 ;;' \
  'esac' \
  >"$ref_fake_bin/git"
chmod 755 "$ref_fake_bin/git"

release_commit=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
release_tag_object=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
moved_object=cccccccccccccccccccccccccccccccccccccccc

expect_ref_accepted() {
  local label="$1"
  local local_object="$2"
  local local_peeled="$3"
  local remote_refs="$4"
  if ! PATH="$ref_fake_bin:$PATH" \
    FAKE_LOCAL_OBJECT="$local_object" FAKE_LOCAL_PEELED="$local_peeled" \
    FAKE_REMOTE_REFS="$remote_refs" GITHUB_SHA="$release_commit" \
    bash "$ref_verifier" skills-v0.3.9 origin \
    >"$test_root/$label.out" 2>&1; then
    fail "$label release ref was unexpectedly rejected"
  fi
}

expect_ref_rejected() {
  local label="$1"
  local local_object="$2"
  local local_peeled="$3"
  local event_sha="$4"
  local remote_refs="$5"
  if PATH="$ref_fake_bin:$PATH" \
    FAKE_LOCAL_OBJECT="$local_object" FAKE_LOCAL_PEELED="$local_peeled" \
    FAKE_REMOTE_REFS="$remote_refs" GITHUB_SHA="$event_sha" \
    bash "$ref_verifier" skills-v0.3.9 origin \
    >"$test_root/$label.out" 2>&1; then
    fail "$label release ref was unexpectedly accepted"
  fi
  require_text 'release_ref_invalid' "$test_root/$label.out"
}

expect_ref_accepted lightweight-tag \
  "$release_commit\n" "$release_commit\n" \
  "$release_commit\trefs/tags/skills-v0.3.9\n"
expect_ref_accepted annotated-tag \
  "$release_tag_object\n" "$release_commit\n" \
  "$release_tag_object\trefs/tags/skills-v0.3.9\n$release_commit\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected moved-local-object \
  "$moved_object\n" "$release_commit\n" "$release_commit" \
  "$release_tag_object\trefs/tags/skills-v0.3.9\n$release_commit\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected moved-local-commit \
  "$release_tag_object\n" "$moved_object\n" "$release_commit" \
  "$release_tag_object\trefs/tags/skills-v0.3.9\n$release_commit\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected moved-event \
  "$release_tag_object\n" "$release_commit\n" "$moved_object" \
  "$release_tag_object\trefs/tags/skills-v0.3.9\n$release_commit\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected moved-remote-object \
  "$release_tag_object\n" "$release_commit\n" "$release_commit" \
  "$moved_object\trefs/tags/skills-v0.3.9\n$release_commit\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected moved-remote-commit \
  "$release_tag_object\n" "$release_commit\n" "$release_commit" \
  "$release_tag_object\trefs/tags/skills-v0.3.9\n$moved_object\trefs/tags/skills-v0.3.9^{}\n"
expect_ref_rejected missing-local-object \
  '' "$release_commit\n" "$release_commit" \
  "$release_commit\trefs/tags/skills-v0.3.9\n"
expect_ref_rejected missing-event \
  "$release_commit\n" "$release_commit\n" '' \
  "$release_commit\trefs/tags/skills-v0.3.9\n"
expect_ref_rejected missing-remote \
  "$release_commit\n" "$release_commit\n" "$release_commit" ''
expect_ref_rejected ambiguous-local \
  "$release_commit\n$moved_object\n" "$release_commit\n" "$release_commit" \
  "$release_commit\trefs/tags/skills-v0.3.9\n"
expect_ref_rejected ambiguous-remote \
  "$release_commit\n" "$release_commit\n" "$release_commit" \
  "$release_commit\trefs/tags/skills-v0.3.9\n$moved_object\trefs/tags/skills-v0.3.9\n"
expect_ref_rejected malformed-local \
  'not-an-object\n' "$release_commit\n" "$release_commit" \
  "$release_commit\trefs/tags/skills-v0.3.9\n"
expect_ref_rejected malformed-remote \
  "$release_commit\n" "$release_commit\n" "$release_commit" \
  "not-an-object\trefs/tags/skills-v0.3.9\n"

# A release ref must resolve to a commit, not merely to any Git object whose
# identity happens to match the event payload.
noncommit_repo="$test_root/noncommit-repo"
noncommit_remote="$test_root/noncommit-remote.git"
git init -q "$noncommit_repo"
git -C "$noncommit_repo" config user.name Dvandva
git -C "$noncommit_repo" config user.email dvandva@example.invalid
printf 'release fixture\n' >"$noncommit_repo/fixture"
git -C "$noncommit_repo" add fixture
git -C "$noncommit_repo" commit -qm fixture
tree_object="$(git -C "$noncommit_repo" rev-parse 'HEAD^{tree}')"
blob_object="$(git -C "$noncommit_repo" rev-parse 'HEAD:fixture')"
git -C "$noncommit_repo" update-ref refs/tags/skills-v0.3.9 "$tree_object"
git -C "$noncommit_repo" update-ref refs/tags/skills-v0.2.1 "$blob_object"
git init -q --bare "$noncommit_remote"
git -C "$noncommit_repo" remote add origin "$noncommit_remote"
git -C "$noncommit_repo" push -q origin \
  refs/tags/skills-v0.3.9 refs/tags/skills-v0.2.1
for noncommit_tag in skills-v0.3.9 skills-v0.2.1; do
  noncommit_object="$(git -C "$noncommit_repo" rev-parse "refs/tags/$noncommit_tag")"
  if (cd "$noncommit_repo" && GITHUB_SHA="$noncommit_object" \
    bash "$ref_verifier" "$noncommit_tag" origin) \
    >"$test_root/$noncommit_tag.out" 2>&1; then
    fail "$noncommit_tag release ref was unexpectedly accepted"
  fi
  require_text 'release_ref_invalid' "$test_root/$noncommit_tag.out"
done

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
valid_probe='{"package":"dvandva-v4","version":"0.3.9","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if test "${1:-}" = "--version"; then' \
  '  printf "dvandva-v4 0.3.9\\n"' \
  '  exit 0' \
  'fi' \
  'if test "${1:-}" = "probe"; then' \
  "  printf '%s\\n' '{\"package\":\"dvandva-v4\",\"version\":\"0.3.9\",\"publish\":false,\"write_schema\":\"dvandva.run.v1\",\"read_schemas\":[\"dvandva.run.v1\"],\"role_api\":1,\"capabilities\":{\"upgrade_from_v1\":false},\"compatible\":true}'" \
  '  exit 0' \
  'fi' \
  'exit 2' \
  >"$test_root/fake-kernel"
chmod 755 "$fake_bin/cargo" "$fake_bin/strip" "$test_root/fake-kernel"

incompatible="$test_root/incompatible"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/fake-kernel" \
  CARGO_TARGET_DIR="$fake_target" \
  bash "$packager" skills-v0.3.9 "$incompatible" \
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
  bash "$packager" skills-v0.3.9 "$wrong_binary" \
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
  '  if test -n "${VERSION_DIR_FILE:-}"; then dirname -- "$0" >"$VERSION_DIR_FILE"; fi' \
  '  case "${FAKE_VERSION_MODE:-valid}" in' \
  '    valid|one_lf) printf "dvandva-v4 0.3.9\\n" ;;' \
  '    exact) printf "dvandva-v4 0.3.9" ;;' \
  '    nul) printf "dvandva-v4 0.3.9\\0\\n" ;;' \
  '    invalid_utf8) printf "dvandva-v4 0.3.9\\377\\n" ;;' \
  '    oversized) printf "dvandva-v4 0.3.9"; head -c 300 /dev/zero | tr "\\0" x ;;' \
  '    extra_newline) printf "dvandva-v4 0.3.9\\n\\n" ;;' \
  '    crlf) printf "dvandva-v4 0.3.9\\r\\n" ;;' \
  '    nonzero) printf "dvandva-v4 0.3.9\\n"; exit 7 ;;' \
  '    *) exit 2 ;;' \
  '  esac' \
  '  exit 0' \
  'fi' \
  'test "${1:-}" = "probe" || exit 2' \
  'case "${FAKE_PROBE_MODE:-valid}" in' \
  "  valid) printf '%s\\n' '$valid_probe' ;;" \
  "  root_duplicate) printf '%s\\n' '{\"package\":\"wrong\",\"package\":\"dvandva-v4\",\"version\":\"0.3.9\",\"publish\":false,\"write_schema\":\"dvandva.run.v2\",\"read_schemas\":[\"dvandva.run.v2\",\"dvandva.run.v1\"],\"role_api\":2,\"capabilities\":{\"upgrade_from_v1\":true},\"compatible\":true}' ;;" \
  "  nested_duplicate) printf '%s\\n' '{\"package\":\"dvandva-v4\",\"version\":\"0.3.9\",\"publish\":false,\"write_schema\":\"dvandva.run.v2\",\"read_schemas\":[\"dvandva.run.v2\",\"dvandva.run.v1\"],\"role_api\":2,\"capabilities\":{\"upgrade_from_v1\":false,\"upgrade_from_v1\":true},\"compatible\":true}' ;;" \
  "  nul) printf '%s\\0' '$valid_probe' ;;" \
  "  oversized) printf '{'; head -c 17000 /dev/zero | tr '\\0' ' '; printf '%s' '${valid_probe:1}' ;;" \
  "  extra_newline) printf '%s\\n\\n' '$valid_probe' ;;" \
  "  trailing_space) printf '%s ' '$valid_probe' ;;" \
  "  invalid_utf8) printf '%s\\377' '$valid_probe' ;;" \
  "  malformed_json) printf '%s' '{' ;;" \
  '  *) exit 2 ;;' \
  'esac' \
  >"$test_root/adversarial-kernel"
chmod 755 "$test_root/adversarial-kernel"

expect_rejected_version() {
  local label="$1"
  local mode="$2"
  local diagnostic="$3"
  local rejected_output="$test_root/$label-output"
  local rejected_log="$test_root/$label.out"
  local version_dir_file="$test_root/$label.version-dir"

  if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
    FAKE_VERSION_MODE="$mode" VERSION_DIR_FILE="$version_dir_file" \
    CARGO_TARGET_DIR="$test_root/$label-target" \
    bash "$packager" skills-v0.3.9 "$rejected_output" \
    >"$rejected_log" 2>&1; then
    fail "$label version unexpectedly packaged"
  fi
  require_text "$diagnostic" "$rejected_log"
  reject_text 'package-skills-release: packaged' "$rejected_log"
  if test -e "$rejected_output" || test -L "$rejected_output"; then
    fail "$label version exposed a final output path"
  fi
  local version_staging
  version_staging="$(cat "$version_dir_file")"
  test "$(dirname -- "$version_staging")" = "$test_root" || \
    fail "$label version did not run in sibling staging"
  case "$(basename -- "$version_staging")" in
    ".$label-output.tmp."*) ;;
    *) fail "$label version staging did not use the hidden output prefix" ;;
  esac
  test ! -e "$version_staging" || fail "$label version staging was not cleaned"
}

expect_rejected_version nul-bearing-version nul binary_version_mismatch
expect_rejected_version invalid-utf8-version invalid_utf8 binary_version_mismatch
expect_rejected_version oversized-version oversized binary_version_too_large
expect_rejected_version extra-newline-version extra_newline binary_version_mismatch
expect_rejected_version crlf-version crlf binary_version_mismatch
expect_rejected_version nonzero-version nonzero binary_version_mismatch
require_text 'version_max_bytes=' "$packager"
require_text '.version' "$packager"

expect_accepted_version() {
  local label="$1"
  local mode="$2"
  local accepted_output="$test_root/$label-output"
  local version_dir_file="$test_root/$label.version-dir"

  if ! PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
    FAKE_VERSION_MODE="$mode" VERSION_DIR_FILE="$version_dir_file" \
    CARGO_TARGET_DIR="$test_root/$label-target" \
    bash "$packager" skills-v0.3.9 "$accepted_output" \
    >"$test_root/$label.out" 2>&1; then
    fail "$label version was unexpectedly rejected"
    return
  fi
  require_text 'package-skills-release: packaged' "$test_root/$label.out"
  test "$(find "$accepted_output" -mindepth 1 -maxdepth 1 -printf '%f\n' | \
    sort | tr '\n' ' ')" = 'SHA256SUMS dvandva-kernel-linux-x86_64 ' || \
    fail "$label version package did not contain exactly two release files"
  (cd "$accepted_output" && sha256sum -c SHA256SUMS >/dev/null) || \
    fail "$label version checksum did not verify"
  local version_staging
  version_staging="$(cat "$version_dir_file")"
  test "$(dirname -- "$version_staging")" = "$test_root" || \
    fail "$label version did not run in sibling staging"
  case "$(basename -- "$version_staging")" in
    ".$label-output.tmp."*) ;;
    *) fail "$label version staging did not use the hidden output prefix" ;;
  esac
  test ! -e "$version_staging" || fail "$label version staging was not promoted"
}

expect_accepted_version exact-version exact
expect_accepted_version one-lf-version one_lf

expect_rejected_probe() {
  local label="$1"
  local mode="$2"
  local diagnostic="$3"
  local rejected_output="$test_root/$label-output"
  local rejected_log="$test_root/$label.out"

  if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
    FAKE_PROBE_MODE="$mode" CARGO_TARGET_DIR="$test_root/$label-target" \
    bash "$packager" skills-v0.3.9 "$rejected_output" \
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
expect_rejected_probe extra-newline extra_newline probe_mismatch
expect_rejected_probe trailing-space trailing_space probe_mismatch
expect_rejected_probe invalid-utf8 invalid_utf8 probe_mismatch
expect_rejected_probe malformed-json malformed_json probe_mismatch

empty_output="$test_root/existing-empty"
mkdir "$empty_output"
if PATH="$fake_bin:$PATH" FAKE_KERNEL="$test_root/adversarial-kernel" \
  FAKE_PROBE_MODE=valid CARGO_TARGET_DIR="$test_root/existing-empty-target" \
  bash "$packager" skills-v0.3.9 "$empty_output" \
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
  bash "$packager" skills-v0.3.9 "$symlink_output" \
  >"$test_root/symlink.out" 2>&1; then
  fail 'pre-existing output symlink unexpectedly accepted'
fi
require_text 'output_exists' "$test_root/symlink.out"
test -L "$symlink_output" || fail 'pre-existing output symlink was replaced'
test -z "$(find "$symlink_target" -mindepth 1 -print -quit)" || \
  fail 'pre-existing output symlink target was modified'

output="$test_root/output"
CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.3.9 "$output"

test -x "$output/dvandva-kernel-linux-x86_64"
test -f "$output/SHA256SUMS"
test "$(find "$output" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' ')" = \
  'SHA256SUMS dvandva-kernel-linux-x86_64 '
(cd "$output" && sha256sum -c SHA256SUMS)
test "$($output/dvandva-kernel-linux-x86_64 --version)" = 'dvandva-v4 0.3.9'
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
    "version": "0.3.9",
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
  bash "$packager" skills-v0.3.9 "$checksum_output" \
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
test "$checksum_staging" != "$checksum_output" || \
  fail 'checksum ran in the final output path'
test ! -e "$checksum_staging" || fail 'checksum staging directory was not cleaned'

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
  bash "$packager" skills-v0.3.9 "$collision_output" \
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
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.3.9 "$nonempty" \
  >"$test_root/nonempty.out" 2>&1; then
  printf 'non-empty destination unexpectedly accepted\n' >&2
  exit 1
fi
require_text 'output_exists' "$test_root/nonempty.out"
test -f "$nonempty/foreign"

workflow="$repo_root/.github/workflows/skills-release.yml"
python3 - "$workflow" <<'PY'
import shlex
import sys
from copy import deepcopy

from ruamel.yaml import YAML

with open(sys.argv[1], encoding="utf-8") as stream:
    workflow = YAML(typ="safe").load(stream)


CHECKOUT = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
RELEASE_TOKENS = [
    "gh", "release", "create", "$GITHUB_REF_NAME",
    "artifacts/dvandva-kernel-linux-x86_64",
    "artifacts/SHA256SUMS",
    "--verify-tag",
    "--title", "Dvandva skills $release_version",
    "--notes-file", "docs/releases/$GITHUB_REF_NAME.md",
]
RELEASE_REF_COMMAND = (
    'bash scripts/verify-skills-release-ref.sh "$GITHUB_REF_NAME" origin'
)


def validate_release_boundaries(candidate):
    verify = candidate["jobs"]["verify"]
    assert set(verify) == {"runs-on", "steps"}
    verify_checkout = next(
        step for step in verify["steps"]
        if step["name"] == "Check out the tested revision"
    )
    assert verify_checkout == {
        "name": "Check out the tested revision",
        "uses": CHECKOUT,
        "with": {"fetch-depth": 0},
    }

    release_steps = candidate["jobs"]["release"]["steps"]
    release_checkout = next(
        step for step in release_steps if step["name"] == "Check out the release tag"
    )
    assert release_checkout == {
        "name": "Check out the release tag",
        "uses": CHECKOUT,
        "with": {"fetch-depth": 0},
    }
    ref_checks = [
        step for step in release_steps
        if step["name"] in {
            "Verify release ref before packaging",
            "Verify release ref before publication",
        }
    ]
    assert ref_checks == [
        {
            "name": "Verify release ref before packaging",
            "run": RELEASE_REF_COMMAND,
        },
        {
            "name": "Verify release ref before publication",
            "run": RELEASE_REF_COMMAND,
        },
    ]
    publish = next(step for step in release_steps if step["name"] == "Publish GitHub release")
    publish_run = publish["run"]
    release_command = publish_run[publish_run.index("gh release create"):]
    assert shlex.split(release_command.replace("\\\n", " ")) == RELEASE_TOKENS


def require_mutation_rejected(label, candidate):
    try:
        validate_release_boundaries(candidate)
    except (AssertionError, KeyError, StopIteration, ValueError):
        return
    raise AssertionError(f"workflow mutation accepted: {label}")


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
    "Verify HTML deliverables",
    "Verify role skills",
    "Verify automatic run discovery",
    "Verify release packaging",
    "Verify two-role canary",
    "Verify poll behaviour",
]
runs = "\n".join(step.get("run", "") for step in steps)
assert "cargo fmt --manifest-path v4/Cargo.toml -- --check" in runs
assert "cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings" in runs
assert "cargo test --manifest-path v4/Cargo.toml --all-targets --locked" in runs
assert "probe --expected-schema dvandva.run.v2 --expected-role-api 2" in runs
assert "cargo test --manifest-path rust/dvandva/Cargo.toml --locked" in runs
archive_run = next(step["run"] for step in steps if step["name"] == "Verify archived v3 remains intact")
assert archive_run.count("--skip") == 6
for obsolete_readme_test in ['readme_contract::documents_action_aware_waits', 'readme_contract::documents_dvandva_role_ownership_for_explainer_reviews', 'readme_contract::documents_multipart_termination_review', 'readme_contract::documents_preflight_role_invocation', 'f5_human_surfacing_contract::readme_documents_the_f5_rule', 'never_silent_stop_contract::readme_documents_the_watchdog_subcommand']:
    assert "--skip " + obsolete_readme_test in archive_run

assert "bash tests/skills/setup-dvandva.sh" in runs
assert "bash tests/skills/discover.sh" in runs
assert "bash tests/skills/html-deliverables.sh" in runs
assert "bash tests/skills/poll-errors.sh" in runs
assert "bash tests/skills/role-skills.sh" in runs
assert "bash tests/skills/package-release.sh" in runs
assert "bash tests/skills/two-role-canary.sh" in runs
assert "bash tests/skills/poll.sh" in runs
release_steps = workflow["jobs"]["release"]["steps"]
assert [step["name"] for step in release_steps] == [
    "Check out the release tag",
    "Select stable Rust",
    "Verify release ref before packaging",
    "Package the private kernel",
    "Verify release ref before publication",
    "Publish GitHub release",
]
package = next(step for step in release_steps if step["name"] == "Package the private kernel")
assert package["run"] == 'bash scripts/package-skills-release.sh "$GITHUB_REF_NAME" artifacts'
publish = next(step for step in release_steps if step["name"] == "Publish GitHub release")
assert publish["env"] == {"GH_TOKEN": "${{ github.token }}"}
publish_run = publish["run"]
assert 'release_version="${GITHUB_REF_NAME#skills-v}"' in publish_run
assert 'gh release create "$GITHUB_REF_NAME"' in publish_run
validate_release_boundaries(workflow)

mutated = deepcopy(workflow)
next(step for step in mutated["jobs"]["verify"]["steps"]
     if step["name"] == "Check out the tested revision")["with"]["ref"] = "main"
require_mutation_rejected("verify ref main", mutated)

mutated = deepcopy(workflow)
next(step for step in mutated["jobs"]["release"]["steps"]
     if step["name"] == "Check out the release tag")["with"]["ref"] = "main"
require_mutation_rejected("release ref main", mutated)

mutated = deepcopy(workflow)
mutated["jobs"]["verify"]["permissions"] = {"contents": "write"}
require_mutation_rejected("verify contents write", mutated)

mutated = deepcopy(workflow)
mutated_publish = next(
    step for step in mutated["jobs"]["release"]["steps"]
    if step["name"] == "Publish GitHub release"
)
mutated_publish["run"] = mutated_publish["run"].replace(
    "  --verify-tag", "  artifacts/forbidden-third-asset \\\n  --verify-tag", 1
)
require_mutation_rejected("third release asset", mutated)
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
require_text '--skill setup-dvandva vadi prativadi html-deliverables' "$workflow_doc"
require_text 'skills-v0.3.9' "$workflow_doc"
require_text 'Act as prativadi and join Dvandva run <run-id>.' "$workflow_doc"
require_text 'scope_mismatch' "$workflow_doc"
require_text 'request_checkpoint_supersession' "$workflow_doc"
require_text 'withdraw_approval' "$workflow_doc"
require_text 'ChatGPT Site' "$workflow_doc"
require_text 'Linux x86_64 only' "$workflow_doc"
require_text 'Claude' "$workflow_doc"
require_text 'Human Decision' "$workflow_doc"

readme_active="$repo_root/README.md"
require_text 'skills-v0.3.9' "$readme_active"
require_text 'Linux x86_64 only' "$readme_active"
require_text 'dvandva.run.v2' "$readme_active"
require_text 'role API 2' "$readme_active"
require_text 'Act as prativadi and join Dvandva run <run-id>.' "$readme_active"
require_text 'ChatGPT Sites' "$readme_active"
require_text 'Claude' "$readme_active"
require_text 'user-owned' "$readme_active"

claude_active="$test_root/claude-active.md"
sed '/^## Historical model discipline/,$d' "$repo_root/CLAUDE.md" >"$claude_active"
require_text 'next_actions' "$claude_active"
require_text 'ChatGPT Site' "$claude_active"
require_text 'Claude' "$claude_active"
require_text 'goals' "$claude_active"
require_text 'digest-bound HTML' "$claude_active"
require_text 'plan/TODO list' "$claude_active"
require_text 'mechanically' "$claude_active"
require_text 'applicable receipts' "$claude_active"
require_text 'If no participant is Codex' "$claude_active"
require_text 'report_progress' "$claude_active"

require_text 'dvandva.run.v2' "$repo_root/v4/README.md"
require_text 'Linux x86_64 only' "$repo_root/v4/README.md"
require_text 'role API 2' "$repo_root/v4/README.md"
require_text 'dedicated upgrade' "$repo_root/v4/README.md"
require_text '--repository-id' "$repo_root/v4/README.md"
require_text '--required-deliverable' "$repo_root/v4/README.md"

protocol="$repo_root/docs/protocol/minimal-run-baton.md"
require_text 'scope_revision' "$protocol"
require_text 'manifest_digest' "$protocol"
require_text 'request_checkpoint_supersession' "$protocol"
require_text 'withdraw_approval' "$protocol"
require_text 'ChatGPT Site' "$protocol"
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

context="$repo_root/CONTEXT.md"
for term in 'Canonical Scope' 'Scope Revision' 'Checkpoint Manifest' \
  'Manifest Digest' 'Checkpoint Binding' \
  'Checkpoint Supersession' 'Approval Withdrawal' 'Protocol Upgrade' \
  'Publication Gate' 'Explainer Site' 'Harness Goal'; do
  require_text "**$term**" "$context"
done
python3 - "$context" <<'PY' || fail 'CONTEXT glossary definitions are not exact'
import re
import sys
from pathlib import Path


source = Path(sys.argv[1]).read_text(encoding="utf-8")
definitions = {
    name: " ".join(body.split())
    for name, body in re.findall(
        r"^\*\*([^*]+)\*\*:\n(.*?)(?=\n\*\*|\Z)",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
}
assert definitions["Scope Revision"] == (
    "The identity of one declared Canonical Scope version. A human-approved "
    "amendment makes every earlier scope-bound checkpoint, Handoff, and review stale."
)
assert definitions["Manifest Digest"] == (
    "The immutable content identity of the checkpoint kind, checkpoint identity, "
    "complete Checkpoint Manifest, and Scope Revision."
)
assert definitions["Checkpoint Binding"] == (
    "The checkpoint identity, Manifest Digest, and Scope Revision that together name "
    "the exact review object. Changing any coordinate makes the earlier binding stale."
)
assert definitions["Protocol Upgrade"] == (
    "The dedicated one-way adoption of active v2 from a legacy v1 Baton, retaining "
    "prior state and history as provenance in the same run. It is distinct from "
    "ordinary role actions, recovery, or setup."
)
assert definitions["Handoff"] == (
    "A run milestone whose current Scope Revision and optional Checkpoint Binding are "
    "published and reviewed together. Handoffs cover role transfer, run start, Protocol "
    "Upgrade, scope amendment, accepted Checkpoint Supersession, and Approval Withdrawal."
)
assert definitions["Publication Gate"] == (
    "The requirement that vadi stages the Explainer Artifact and prativadi reviews "
    "those exact bytes. When the pairing contains Codex, that participant also "
    "records the matching Explainer Site for the same Handoff before finalization; "
    "without Codex, Sites publication is skipped and local approval is sufficient."
)
assert definitions["Explainer Artifact"] == (
    "The explainer's bytes, staged by vadi into the run directory at "
    "`explainer/<source_digest>.html` and bound by sha256 to one Handoff. Both roles "
    "read it locally, so it is the artifact the Publication Gate binds."
)
assert definitions["Explainer Site"] == (
    "The owner-only ChatGPT Sites rendering of an approved Explainer Artifact. It is "
    "the user's stable status page. Whichever participant is Codex publishes it; "
    "prativadi reviews the local artifact rather than this deployment."
)
assert "An assignee change" not in definitions["Handoff"]
PY

adr="$repo_root/docs/adr/0003-run-v2-security-epoch.md"
require_text 'status: accepted' "$adr"
require_text 'dvandva.run.v2' "$adr"
require_text 'role API 2' "$adr"
require_text 'structural receipts are not provider-signed proof' "$adr"
require_text 'deliberate protocol decision' "$adr"
require_text 'Superseded in part by' "$adr"

# The publication clause moved; the rest of the epoch must still stand.
superseding="$repo_root/docs/adr/0004-run-artifact-explainer-channel.md"
require_text 'status: accepted' "$superseding"
require_text 'supersedes ADR 0003' "$superseding"
require_text 'explainer/<source_digest>.html' "$superseding"
require_text 'optional human-facing rendering' "$superseding"
require_text 'repair-policy' "$superseding"
require_text 'dvandva.run.v2' "$superseding"

mandatory_site="$repo_root/docs/adr/0005-require-private-sites-explainer-mirror.md"
require_text 'status: accepted' "$mandatory_site"
require_text 'ADR 0004' "$mandatory_site"
require_text 'owner-only ChatGPT Site' "$mandatory_site"
require_text 'If neither participant is Codex' "$mandatory_site"
require_text 'status page' "$mandatory_site"

if test "$failures" -ne 0; then
  printf 'skills release packaging: %s failure(s)\n' "$failures" >&2
  exit 1
fi

printf 'skills release packaging: ok\n'
