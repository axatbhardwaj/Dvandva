#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"

release_dir="$test_root/release"
mkdir -p "$release_dir"
asset="dvandva-kernel-linux-x86_64"
cp "$repo_root/v4/target/debug/dvandva-v4" "$release_dir/$asset"
(cd "$release_dir" && sha256sum "$asset" > SHA256SUMS)

export DVANDVA_RELEASE_DIR="$release_dir"
installer="$repo_root/skills/setup-dvandva/scripts/setup-dvandva.sh"

reset_xdg() {
  local name="$1"
  export XDG_DATA_HOME="$test_root/$name/data"
  export XDG_STATE_HOME="$test_root/$name/state"
}

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

reset_xdg clean
bash "$installer" install --version 0.1.0

installed="$XDG_DATA_HOME/dvandva/bin/0.1.0/dvandva-kernel"
test -x "$installed"
test "$(readlink "$XDG_DATA_HOME/dvandva/bin/current")" = "0.1.0"
test -d "$XDG_STATE_HOME/dvandva/runs"
test -d "$XDG_STATE_HOME/dvandva/credentials"
test "$(stat -c '%a' "$XDG_STATE_HOME/dvandva/credentials")" = "700"
test -f "$XDG_DATA_HOME/dvandva/installation.json"

probe="$($installed probe --expected-schema dvandva.run.v1)"
grep -q '"compatible": true' <<<"$probe"

bash "$installer" doctor --version 0.1.0 | grep -q 'healthy'

original_current="$(readlink "$XDG_DATA_HOME/dvandva/bin/current")"
cp "$release_dir/SHA256SUMS" "$release_dir/SHA256SUMS.good"
printf '%064d  %s\n' 0 "$asset" >"$release_dir/SHA256SUMS"
expect_failure 'checksum_mismatch' bash "$installer" update --version 0.1.1
test "$(readlink "$XDG_DATA_HOME/dvandva/bin/current")" = "$original_current"
mv "$release_dir/SHA256SUMS.good" "$release_dir/SHA256SUMS"

bash "$installer" update --version 0.1.1
test -x "$XDG_DATA_HOME/dvandva/bin/0.1.0/dvandva-kernel"
test -x "$XDG_DATA_HOME/dvandva/bin/0.1.1/dvandva-kernel"
test "$(readlink "$XDG_DATA_HOME/dvandva/bin/current")" = "0.1.1"
bash "$installer" doctor --version 0.1.1 | grep -q 'healthy'
expect_failure 'version_mismatch' bash "$installer" doctor --version 0.1.0

cp "$XDG_DATA_HOME/dvandva/installation.json" "$test_root/manifest.good"
printf 'not-json\n' >"$XDG_DATA_HOME/dvandva/installation.json"
expect_failure 'installation_manifest_missing' bash "$installer" doctor --version 0.1.1
cp "$test_root/manifest.good" "$XDG_DATA_HOME/dvandva/installation.json"
printf 'corrupt\n' >>"$XDG_DATA_HOME/dvandva/bin/0.1.1/dvandva-kernel"
expect_failure 'checksum_mismatch' bash "$installer" doctor --version 0.1.1

reset_xdg missing
expect_failure 'installation_manifest_missing' bash "$installer" doctor --version 0.1.0

reset_xdg preserve
bash "$installer" install --version 0.1.0 >/dev/null
touch "$XDG_STATE_HOME/dvandva/runs/keep-me"
bash "$installer" uninstall --version 0.1.0 | grep -q 'preserved_runs=true'
test -f "$XDG_STATE_HOME/dvandva/runs/keep-me"
test ! -e "$XDG_DATA_HOME/dvandva/bin"

reset_xdg purge
bash "$installer" install --version 0.1.0 >/dev/null
touch "$XDG_STATE_HOME/dvandva/runs/remove-me"
expect_failure 'requires --yes-purge-runs' bash "$installer" uninstall --version 0.1.0 --purge-runs
test -f "$XDG_STATE_HOME/dvandva/runs/remove-me"
bash "$installer" uninstall --version 0.1.0 --purge-runs --yes-purge-runs >/dev/null
test ! -e "$XDG_STATE_HOME/dvandva/runs"

reset_xdg unowned
mkdir -p "$XDG_DATA_HOME/dvandva"
touch "$XDG_DATA_HOME/dvandva/foreign"
expect_failure 'refusing unowned data' bash "$installer" install --version 0.1.0
test -f "$XDG_DATA_HOME/dvandva/foreign"
expect_failure 'refusing uninstall without owned manifest' bash "$installer" uninstall --version 0.1.0

printf 'setup-dvandva installer tests: ok\n'
