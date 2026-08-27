#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
asset="dvandva-kernel-linux-x86_64"

mkdir -p "$test_root/old"
git -C "$repo_root" archive skills-v0.1.1 v4 skills/setup-dvandva | \
  tar -x -C "$test_root/old"
CARGO_TARGET_DIR="$test_root/old-target" cargo build --quiet --locked \
  --manifest-path "$test_root/old/v4/Cargo.toml"
CARGO_TARGET_DIR="$test_root/new-target" cargo build --quiet --locked \
  --manifest-path "$repo_root/v4/Cargo.toml"
old_binary="$test_root/old-target/debug/dvandva-v4"
new_binary="$test_root/new-target/debug/dvandva-v4"
test "$($old_binary --version)" = 'dvandva-v4 0.1.1'
test "$($new_binary --version)" = 'dvandva-v4 0.2.0'

make_release() {
  local directory="$1" source="$2"
  mkdir -p "$directory"
  cp "$source" "$directory/$asset"
  (cd "$directory" && sha256sum "$asset" >SHA256SUMS)
}

old_release="$test_root/release-0.1.1"
new_release="$test_root/release-0.2.0"
make_release "$old_release" "$old_binary"
make_release "$new_release" "$new_binary"

export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
old_installer="$test_root/old/skills/setup-dvandva/scripts/setup-dvandva.sh"
installer="$repo_root/skills/setup-dvandva/scripts/setup-dvandva.sh"

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

DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" install --version 0.1.1 >/dev/null
current="$XDG_DATA_HOME/dvandva/bin/current"
test "$(readlink "$current")" = '0.1.1'
test "$($current/dvandva-kernel --version)" = 'dvandva-v4 0.1.1'

runs="$XDG_STATE_HOME/dvandva/runs"
printf 'preserve me\n' >"$runs/keep-me"
before_runs="$(find "$runs" -printf '%P %y %m %s %T@\n' | sort)"

# A checksummed but wrong-version binary cannot replace the known-good current link.
wrong_version_release="$test_root/wrong-version"
make_release "$wrong_version_release" "$old_binary"
expect_failure 'version_mismatch' env DVANDVA_RELEASE_DIR="$wrong_version_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
test "$before_runs" = "$(find "$runs" -printf '%P %y %m %s %T@\n' | sort)"
test -z "$(find "$XDG_DATA_HOME/dvandva/bin" -maxdepth 1 -name '.0.2.0.*.tmp' -print)"

# A controlled 0.2.0 probe stub with the wrong schema/API is also rejected.
wrong_probe_release="$test_root/wrong-probe"
mkdir -p "$wrong_probe_release"
cat >"$wrong_probe_release/$asset" <<'WRONG_PROBE'
#!/usr/bin/env bash
if test "${1:-}" = "--version"; then printf 'dvandva-v4 0.2.0\n'; exit 0; fi
if test "${1:-}" = "probe"; then
  printf '{"package":"dvandva-v4","version":"0.2.0","write_schema":"dvandva.run.v1","read_schemas":["dvandva.run.v1"],"role_api":1,"capabilities":{"upgrade_from_v1":false},"compatible":false}\n'
  exit 1
fi
exit 99
WRONG_PROBE
chmod 755 "$wrong_probe_release/$asset"
(cd "$wrong_probe_release" && sha256sum "$asset" >SHA256SUMS)
expect_failure 'probe_mismatch' env DVANDVA_RELEASE_DIR="$wrong_probe_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
test "$before_runs" = "$(find "$runs" -printf '%P %y %m %s %T@\n' | sort)"

# Validation also runs for a pre-existing version directory.
mkdir -p "$XDG_DATA_HOME/dvandva/bin/0.2.0"
cp "$wrong_probe_release/$asset" "$XDG_DATA_HOME/dvandva/bin/0.2.0/dvandva-kernel"
printf 'dvandva-skill-v1\n' >"$XDG_DATA_HOME/dvandva/bin/0.2.0/.owner"
expect_failure 'probe_mismatch' env DVANDVA_RELEASE_DIR="$wrong_probe_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
rm -rf -- "$XDG_DATA_HOME/dvandva/bin/0.2.0"

installed="$(env DVANDVA_RELEASE_DIR="$new_release" bash "$installer" update --version 0.2.0)"
test "$(readlink "$current")" = '0.2.0'
test "$($current/dvandva-kernel --version)" = 'dvandva-v4 0.2.0'
grep -Fq 'write_schema=dvandva.run.v2' <<<"$installed"
grep -Fq 'role_api=2' <<<"$installed"
grep -Fq 'read_schemas=dvandva.run.v2,dvandva.run.v1' <<<"$installed"
grep -Fq 'upgrade_from_v1=true' <<<"$installed"
grep -Fq 'publish=false' <<<"$installed"
test "$before_runs" = "$(find "$runs" -printf '%P %y %m %s %T@\n' | sort)"

healthy="$(bash "$installer" doctor --version 0.2.0)"
grep -Fq 'healthy version=0.2.0' <<<"$healthy"
grep -Fq 'write_schema=dvandva.run.v2' <<<"$healthy"
grep -Fq 'role_api=2' <<<"$healthy"
grep -Fq 'read_schemas=dvandva.run.v2,dvandva.run.v1' <<<"$healthy"
grep -Fq 'upgrade_from_v1=true' <<<"$healthy"

# Checksum failure is fail-closed too.
cp "$new_release/SHA256SUMS" "$test_root/sums.good"
printf '%064d  %s\n' 0 "$asset" >"$new_release/SHA256SUMS"
expect_failure 'checksum_mismatch' env DVANDVA_RELEASE_DIR="$new_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.2.0'
mv "$test_root/sums.good" "$new_release/SHA256SUMS"

# Uninstall preserves runs unless the explicit destructive pair is supplied.
bash "$installer" uninstall --version 0.2.0 | grep -Fq 'preserved_runs=true'
test -f "$runs/keep-me"

export XDG_DATA_HOME="$test_root/unowned/data"
export XDG_STATE_HOME="$test_root/unowned/state"
mkdir -p "$XDG_DATA_HOME/dvandva"
touch "$XDG_DATA_HOME/dvandva/foreign"
expect_failure 'refusing unowned data' env DVANDVA_RELEASE_DIR="$new_release" \
  bash "$installer" install --version 0.2.0
expect_failure 'refusing uninstall without owned manifest' \
  bash "$installer" uninstall --version 0.2.0

printf 'setup-dvandva installer tests: ok\n'
