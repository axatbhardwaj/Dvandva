#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
asset="dvandva-kernel-linux-x86_64"
grep -Fq 'fetch-depth: 0' "$repo_root/.github/workflows/skills-release.yml"

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

make_adversarial_release() {
  local directory="$1" kind="$2"
  mkdir -p "$directory"
  {
    printf '#!/usr/bin/env bash\nkind=%q\n' "$kind"
    cat <<'ADVERSARIAL_KERNEL'
set -euo pipefail
valid_probe='{"package":"dvandva-v4","version":"0.2.0","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
if test "${1:-}" = "--version"; then
  case "$kind" in
    version-nul) printf 'dvandva-v4 0.2.0\0\n' ;;
    version-invalid-utf8) printf 'dvandva-v4 0.2.0\377\n' ;;
    version-oversize) printf 'dvandva-v4 0.2.0'; head -c 70000 /dev/zero | tr '\0' x ;;
    version-extra-newline) printf 'dvandva-v4 0.2.0\n\n' ;;
    version-nonzero) printf 'dvandva-v4 0.2.0\n'; exit 7 ;;
    *) printf 'dvandva-v4 0.2.0\n' ;;
  esac
  exit 0
fi
if test "${1:-}" = "probe"; then
  case "$kind" in
    probe-nul) printf '%s\0\n' "$valid_probe" ;;
    probe-invalid-utf8) printf '%s\377\n' "$valid_probe" ;;
    probe-oversize) printf '%s' "$valid_probe"; head -c 70000 /dev/zero | tr '\0' ' ' ;;
    probe-extra-newline) printf '%s\n\n' "$valid_probe" ;;
    probe-nonzero) printf '%s\n' "$valid_probe"; exit 7 ;;
    *) printf '%s\n' "$valid_probe" ;;
  esac
  exit 0
fi
exit 99
ADVERSARIAL_KERNEL
  } >"$directory/$asset"
  chmod 755 "$directory/$asset"
  (cd "$directory" && sha256sum "$asset" >SHA256SUMS)
}

old_release="$test_root/release-0.1.1"
new_release="$test_root/release-0.2.0"
make_release "$old_release" "$old_binary"
make_release "$new_release" "$new_binary"

export XDG_DATA_HOME="$test_root/data"
export XDG_STATE_HOME="$test_root/state"
old_installer="$test_root/old/skills/setup-dvandva/scripts/setup-dvandva.sh"
installer="${INSTALLER_UNDER_TEST:-$repo_root/skills/setup-dvandva/scripts/setup-dvandva.sh}"

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

adversarial_handshake_case() (
  local kind="$1" expected_error="$2" case_root="$test_root/handshake-$1"
  export XDG_DATA_HOME="$case_root/data"
  export XDG_STATE_HOME="$case_root/state"
  local release="$case_root/release"
  local temporary_root="$case_root/tmp"
  mkdir -p "$temporary_root"
  make_adversarial_release "$release" "$kind"

  expect_failure "$expected_error" env TMPDIR="$temporary_root" \
    DVANDVA_RELEASE_DIR="$release" bash "$installer" install --version 0.2.0
  test ! -e "$XDG_DATA_HOME/dvandva"
  test -z "$(find "$temporary_root" -mindepth 1 -print)"

  # A rejected first attempt must not make the invocation-created root look
  # foreign to a valid retry.
  TMPDIR="$temporary_root" DVANDVA_RELEASE_DIR="$new_release" \
    bash "$installer" install --version 0.2.0 >/dev/null
  test "$(readlink "$XDG_DATA_HOME/dvandva/bin/current")" = '0.2.0'
)

for handshake_kind in \
  version-nul version-invalid-utf8 version-oversize version-extra-newline \
  version-nonzero; do
  adversarial_handshake_case "$handshake_kind" version_mismatch
done
for handshake_kind in \
  probe-nul probe-invalid-utf8 probe-oversize probe-extra-newline probe-nonzero; do
  adversarial_handshake_case "$handshake_kind" probe_mismatch
done

purge_path_safety_case() (
  local kind="$1" case_root="$test_root/purge-$1"
  export XDG_DATA_HOME="$case_root/data"
  export XDG_STATE_HOME="$case_root/state"
  DVANDVA_RELEASE_DIR="$new_release" bash "$installer" \
    install --version 0.2.0 >/dev/null
  local case_data="$XDG_DATA_HOME/dvandva"
  local external="$case_root/external"
  mkdir -p "$external"
  printf 'foreign state\n' >"$external/foreign"

  case "$kind" in
    state-root-symlink)
      mkdir -p "$XDG_STATE_HOME"
      ln -s "$external" "$XDG_STATE_HOME/dvandva"
      ;;
    state-root-file)
      mkdir -p "$XDG_STATE_HOME"
      printf 'foreign root\n' >"$XDG_STATE_HOME/dvandva"
      ;;
    runs-symlink)
      mkdir -p "$XDG_STATE_HOME/dvandva"
      ln -s "$external" "$XDG_STATE_HOME/dvandva/runs"
      ;;
    runs-file)
      mkdir -p "$XDG_STATE_HOME/dvandva"
      printf 'foreign runs\n' >"$XDG_STATE_HOME/dvandva/runs"
      ;;
  esac

  expect_failure 'refusing unsafe' bash "$installer" uninstall --version 0.2.0 \
    --purge-runs --yes-purge-runs
  # Purge validation happens before uninstall: the owned installation and all
  # foreign state remain byte-for-byte present.
  test -f "$case_data/installation.json"
  test "$(readlink "$case_data/bin/current")" = '0.2.0'
  test "$(cat "$external/foreign")" = 'foreign state'
  case "$kind" in
    state-root-symlink) test -L "$XDG_STATE_HOME/dvandva" ;;
    state-root-file) test "$(cat "$XDG_STATE_HOME/dvandva")" = 'foreign root' ;;
    runs-symlink) test -L "$XDG_STATE_HOME/dvandva/runs" ;;
    runs-file) test "$(cat "$XDG_STATE_HOME/dvandva/runs")" = 'foreign runs' ;;
  esac
)

purge_path_safety_case state-root-symlink
purge_path_safety_case state-root-file
purge_path_safety_case runs-symlink
purge_path_safety_case runs-file

test -n "$(git -C "$repo_root" rev-parse --verify refs/tags/skills-v0.1.1)"

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

# Correct strings hidden in a nested decoy cannot satisfy the top-level contract.
decoy_release="$test_root/decoy-probe"
mkdir -p "$decoy_release"
cat >"$decoy_release/$asset" <<'DECOY_PROBE'
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
DECOY_PROBE
chmod 755 "$decoy_release/$asset"
(cd "$decoy_release" && sha256sum "$asset" >SHA256SUMS)
expect_failure 'probe_mismatch' env DVANDVA_RELEASE_DIR="$decoy_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
test ! -e "$XDG_DATA_HOME/dvandva/bin/0.2.0"

# An exit-zero candidate that claims publish=true is not a private kernel.
wrong_publish_release="$test_root/wrong-publish"
mkdir -p "$wrong_publish_release"
cat >"$wrong_publish_release/$asset" <<'WRONG_PUBLISH'
#!/usr/bin/env bash
if test "${1:-}" = "--version"; then printf 'dvandva-v4 0.2.0\n'; exit 0; fi
if test "${1:-}" = "probe"; then
  printf '%s\n' '{"package": "dvandva-v4", "version": "0.2.0", "publish": true, "write_schema": "dvandva.run.v2", "read_schemas": ["dvandva.run.v2", "dvandva.run.v1"], "role_api": 2, "capabilities": {"upgrade_from_v1": true}, "compatible": true}'
  exit 0
fi
exit 99
WRONG_PUBLISH
chmod 755 "$wrong_publish_release/$asset"
(cd "$wrong_publish_release" && sha256sum "$asset" >SHA256SUMS)
expect_failure 'probe_mismatch' env DVANDVA_RELEASE_DIR="$wrong_publish_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
test ! -e "$XDG_DATA_HOME/dvandva/bin/0.2.0"

# Validation also runs for a pre-existing version directory.
mkdir -p "$XDG_DATA_HOME/dvandva/bin/0.2.0"
cp "$wrong_probe_release/$asset" "$XDG_DATA_HOME/dvandva/bin/0.2.0/dvandva-kernel"
printf 'dvandva-skill-v1\n' >"$XDG_DATA_HOME/dvandva/bin/0.2.0/.owner"
expect_failure 'probe_mismatch' env DVANDVA_RELEASE_DIR="$wrong_probe_release" \
  bash "$installer" update --version 0.2.0
test "$(readlink "$current")" = '0.1.1'
rm -rf -- "$XDG_DATA_HOME/dvandva/bin/0.2.0"

path_safety_case() (
  local kind="$1" case_root="$test_root/path-$1"
  export XDG_DATA_HOME="$case_root/data"
  export XDG_STATE_HOME="$case_root/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  local case_bin="$XDG_DATA_HOME/dvandva/bin"
  local candidate="$case_bin/0.2.0"
  case "$kind" in
    unowned)
      mkdir "$candidate"
      cp "$new_binary" "$candidate/dvandva-kernel"
      ;;
    directory-symlink)
      mkdir "$case_bin/candidate-target"
      cp "$new_binary" "$case_bin/candidate-target/dvandva-kernel"
      printf 'dvandva-skill-v1\n' >"$case_bin/candidate-target/.owner"
      ln -s candidate-target "$candidate"
      ;;
    binary-symlink)
      mkdir "$candidate"
      printf 'dvandva-skill-v1\n' >"$candidate/.owner"
      ln -s "$new_binary" "$candidate/dvandva-kernel"
      ;;
    owner-symlink)
      mkdir "$candidate"
      cp "$new_binary" "$candidate/dvandva-kernel"
      printf 'dvandva-skill-v1\n' >"$case_bin/owner-target"
      ln -s ../owner-target "$candidate/.owner"
      ;;
    owner-wrong-bytes)
      mkdir "$candidate"
      cp "$new_binary" "$candidate/dvandva-kernel"
      printf 'dvandva-skill-v1\n\n' >"$candidate/.owner"
      ;;
  esac
  expect_failure 'unsafe existing version' env DVANDVA_RELEASE_DIR="$new_release" \
    bash "$installer" update --version 0.2.0
  test "$(readlink "$case_bin/current")" = '0.1.1'
)

path_safety_case unowned
path_safety_case directory-symlink
path_safety_case binary-symlink
path_safety_case owner-symlink
path_safety_case owner-wrong-bytes

empty_unowned_data_root_case() (
  export XDG_DATA_HOME="$test_root/empty-unowned/data"
  export XDG_STATE_HOME="$test_root/empty-unowned/state"
  mkdir -p "$XDG_DATA_HOME/dvandva"
  local before
  before="$(stat -c '%d:%i:%a:%s:%Y' "$XDG_DATA_HOME/dvandva")"
  expect_failure 'refusing unowned data' env DVANDVA_RELEASE_DIR="$new_release" \
    bash "$installer" install --version 0.2.0
  test "$before" = "$(stat -c '%d:%i:%a:%s:%Y' "$XDG_DATA_HOME/dvandva")"
  test -z "$(find "$XDG_DATA_HOME/dvandva" -mindepth 1 -print)"
)

# Managed parent directories may not redirect writes through symlinks.
(
  export XDG_DATA_HOME="$test_root/data-root-symlink/data"
  export XDG_STATE_HOME="$test_root/data-root-symlink/state"
  mkdir -p "$XDG_DATA_HOME" "$test_root/data-root-symlink/target"
  ln -s "$test_root/data-root-symlink/target" "$XDG_DATA_HOME/dvandva"
  expect_failure 'unsafe data root' env DVANDVA_RELEASE_DIR="$new_release" \
    bash "$installer" install --version 0.2.0
  test -z "$(find "$test_root/data-root-symlink/target" -mindepth 1 -print)"
)
(
  export XDG_DATA_HOME="$test_root/bin-root-symlink/data"
  export XDG_STATE_HOME="$test_root/bin-root-symlink/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  mv "$XDG_DATA_HOME/dvandva/bin" "$XDG_DATA_HOME/dvandva/bin-real"
  ln -s bin-real "$XDG_DATA_HOME/dvandva/bin"
  expect_failure 'unsafe bin root' env DVANDVA_RELEASE_DIR="$new_release" \
    bash "$installer" update --version 0.2.0
  test "$(readlink "$XDG_DATA_HOME/dvandva/bin/current")" = '0.1.1'
)

# A manifest-preparation failure cannot promote a candidate or split current/manifest.
(
  export XDG_DATA_HOME="$test_root/manifest-failure/data"
  export XDG_STATE_HOME="$test_root/manifest-failure/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  case_data="$XDG_DATA_HOME/dvandva"
  old_manifest_digest="$(sha256sum "$case_data/installation.json" | cut -d' ' -f1)"
  chmod 500 "$case_data"
  if env DVANDVA_RELEASE_DIR="$new_release" bash "$installer" \
    update --version 0.2.0 >"$test_root/manifest-failure.out" 2>&1; then
    printf 'manifest write failure unexpectedly installed the update\n' >&2
    exit 1
  fi
  chmod 700 "$case_data"
  test "$(readlink "$case_data/bin/current")" = '0.1.1'
  test "$old_manifest_digest" = \
    "$(sha256sum "$case_data/installation.json" | cut -d' ' -f1)"
  test ! -e "$case_data/bin/0.2.0"
  test -z "$(find "$case_data" -name '*.tmp' -print)"
)

installed="$(env DVANDVA_RELEASE_DIR="$new_release" bash "$installer" update --version 0.2.0)"
test "$(readlink "$current")" = '0.2.0'
test "$($current/dvandva-kernel --version)" = 'dvandva-v4 0.2.0'
grep -Fq 'write_schema=dvandva.run.v2' <<<"$installed"
grep -Fq 'role_api=2' <<<"$installed"
grep -Fq 'read_schemas=dvandva.run.v2,dvandva.run.v1' <<<"$installed"
grep -Fq 'upgrade_from_v1=true' <<<"$installed"
grep -Fq 'publish=false' <<<"$installed"
test "$before_runs" = "$(find "$runs" -printf '%P %y %m %s %T@\n' | sort)"

# Ownership is an exact top-level manifest field, not a nested matching string.
cp "$XDG_DATA_HOME/dvandva/installation.json" "$test_root/manifest.good"
cat >"$XDG_DATA_HOME/dvandva/installation.json" <<'MANIFEST_DECOY'
{
  "owner": "foreign-owner",
  "version": "0.2.0",
  "write_schema": "dvandva.run.v2",
  "read_schemas": "dvandva.run.v2,dvandva.run.v1",
  "role_api": 2,
  "upgrade_from_v1": true,
  "publish": false,
  "asset": "dvandva-kernel-linux-x86_64",
  "sha256": "unused",
  "decoy": {"owner": "dvandva-skill-v1"}
}
MANIFEST_DECOY
expect_failure 'installation_manifest_missing' bash "$installer" doctor --version 0.2.0
printf '%s\n' '{"owner":"dvandva-skill-v1"}' \
  >"$XDG_DATA_HOME/dvandva/installation.json"
expect_failure 'installation_manifest_missing' bash "$installer" doctor --version 0.2.0
cp "$test_root/manifest.good" "$XDG_DATA_HOME/dvandva/installation.json"

healthy="$(bash "$installer" doctor --version 0.2.0)"
grep -Fq 'healthy version=0.2.0' <<<"$healthy"
grep -Fq 'write_schema=dvandva.run.v2' <<<"$healthy"
grep -Fq 'role_api=2' <<<"$healthy"
grep -Fq 'read_schemas=dvandva.run.v2,dvandva.run.v1' <<<"$healthy"
grep -Fq 'upgrade_from_v1=true' <<<"$healthy"

# Failure after manifest replacement restores the old pair and quarantines fresh promotion.
rollback_bin="$test_root/rollback-bin"
mkdir -p "$rollback_bin"
cat >"$rollback_bin/mv" <<'FAIL_CURRENT_MV'
#!/usr/bin/env bash
set -euo pipefail
last="${!#}"
if [[ "$last" == */bin/current ]] && test ! -e "${MV_FAIL_MARKER:?}"; then
  touch "$MV_FAIL_MARKER"
  printf 'injected current commit failure\n' >&2
  exit 1
fi
exec /usr/bin/mv "$@"
FAIL_CURRENT_MV
chmod 755 "$rollback_bin/mv"

rollback_case() (
  local kind="$1" case_root="$test_root/rollback-$1"
  export XDG_DATA_HOME="$case_root/data"
  export XDG_STATE_HOME="$case_root/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  local case_data="$XDG_DATA_HOME/dvandva" before_manifest="$case_root/manifest.before"
  cp "$case_data/installation.json" "$before_manifest"
  if test "$kind" = preexisting; then
    mkdir "$case_data/bin/0.2.0"
    cp "$new_binary" "$case_data/bin/0.2.0/dvandva-kernel"
    printf 'dvandva-skill-v1\n' >"$case_data/bin/0.2.0/.owner"
  fi
  if env \
    PATH="$rollback_bin:$PATH" MV_FAIL_MARKER="$case_root/mv.failed" \
    DVANDVA_RELEASE_DIR="$new_release" bash "$installer" update --version 0.2.0 \
    >"$case_root/update.out" 2>&1; then
    printf 'current commit failure unexpectedly installed the update\n' >&2
    exit 1
  fi
  grep -Fq 'injected current commit failure' "$case_root/update.out"
  test "$(readlink "$case_data/bin/current")" = '0.1.1'
  cmp "$before_manifest" "$case_data/installation.json"
  if test "$kind" = fresh; then
    test ! -e "$case_data/bin/0.2.0"
    grep -Fq 'rollback_uncertain evidence=' "$case_root/update.out"
    mapfile -t transaction_evidence < <(
      find "$case_data" -maxdepth 1 -type d -name '.install-txn.*' -print
    )
    test "${#transaction_evidence[@]}" -eq 1
    test -x "${transaction_evidence[0]}/promoted-install/dvandva-kernel"
    test -f "${transaction_evidence[0]}/old-manifest"
  else
    test -x "$case_data/bin/0.2.0/dvandva-kernel"
    ! grep -Fq 'rollback_uncertain evidence=' "$case_root/update.out"
    test -z "$(find "$case_data" -maxdepth 1 -type d -name '.install-txn.*' -print)"
  fi
  test -z "$(find "$case_data/bin" -maxdepth 1 -name '.0.2.0.*.tmp' -print)"
)

rollback_case fresh
rollback_case preexisting

# A pathname replacement after identity lookup is never recursively deleted.
rollback_swap_bin="$test_root/rollback-swap-bin"
mkdir -p "$rollback_swap_bin"
cp "$rollback_bin/mv" "$rollback_swap_bin/mv"
cat >"$rollback_swap_bin/stat" <<'SWAP_AFTER_STAT'
#!/usr/bin/env bash
set -euo pipefail
last="${!#}"
output="$(/usr/bin/stat "$@")"
if test "$last" = "${SWAP_TARGET:?}" && test ! -e "${SWAP_MARKER:?}"; then
  /usr/bin/touch "$SWAP_MARKER"
  /usr/bin/mv -T -- "$SWAP_TARGET" "${SWAPPED_PROMOTED:?}"
  /usr/bin/mkdir -- "$SWAP_TARGET"
  printf 'foreign replacement\n' >"$SWAP_TARGET/foreign-marker"
fi
printf '%s\n' "$output"
SWAP_AFTER_STAT
chmod 755 "$rollback_swap_bin/stat"

rollback_path_swap_case() (
  export XDG_DATA_HOME="$test_root/rollback-swap/data"
  export XDG_STATE_HOME="$test_root/rollback-swap/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  case_data="$XDG_DATA_HOME/dvandva"
  cp "$case_data/installation.json" "$test_root/rollback-swap.manifest"
  if env PATH="$rollback_swap_bin:$PATH" \
    MV_FAIL_MARKER="$test_root/rollback-swap.mv-failed" \
    SWAP_MARKER="$test_root/rollback-swap.stat-swapped" \
    SWAP_TARGET="$case_data/bin/0.2.0" \
    SWAPPED_PROMOTED="$case_data/bin/invocation-promoted" \
    DVANDVA_RELEASE_DIR="$new_release" bash "$installer" \
    update --version 0.2.0 >"$test_root/rollback-swap.out" 2>&1; then
    printf 'path-swapped rollback unexpectedly installed the update\n' >&2
    exit 1
  fi
  test "$(readlink "$case_data/bin/current")" = '0.1.1'
  cmp "$test_root/rollback-swap.manifest" "$case_data/installation.json"
  test -x "$case_data/bin/invocation-promoted/dvandva-kernel"
  mapfile -t foreign_markers < <(find "$case_data" -name foreign-marker -print)
  test "${#foreign_markers[@]}" -eq 1
  grep -Fq 'rollback_uncertain evidence=' "$test_root/rollback-swap.out"
  mapfile -t transaction_evidence < <(
    find "$case_data" -maxdepth 1 -type d -name '.install-txn.*' -print
  )
  test "${#transaction_evidence[@]}" -eq 1
  test "${foreign_markers[0]}" = "${transaction_evidence[0]}/promoted-install/foreign-marker"
  test -f "${transaction_evidence[0]}/old-manifest"
)

rollback_quarantine_swap_bin="$test_root/rollback-quarantine-swap-bin"
mkdir -p "$rollback_quarantine_swap_bin"
cp "$rollback_bin/mv" "$rollback_quarantine_swap_bin/mv"
cat >"$rollback_quarantine_swap_bin/rm" <<'SWAP_QUARANTINE_BEFORE_RM'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  case "$argument" in
    */.install-txn.*/promoted-install)
      /usr/bin/touch "${QUARANTINE_SWAP_MARKER:?}"
      /usr/bin/mv -T -- "$argument" "${SWAPPED_QUARANTINE:?}"
      /usr/bin/mkdir -- "$argument"
      printf 'foreign replacement\n' >"$argument/foreign-marker"
      ;;
  esac
done
exec /usr/bin/rm "$@"
SWAP_QUARANTINE_BEFORE_RM
chmod 755 "$rollback_quarantine_swap_bin/rm"

rollback_quarantine_swap_case() (
  export XDG_DATA_HOME="$test_root/rollback-quarantine-swap/data"
  export XDG_STATE_HOME="$test_root/rollback-quarantine-swap/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  case_data="$XDG_DATA_HOME/dvandva"
  cp "$case_data/installation.json" "$test_root/rollback-quarantine-swap.manifest"
  if env PATH="$rollback_quarantine_swap_bin:$PATH" \
    MV_FAIL_MARKER="$test_root/rollback-quarantine-swap.mv-failed" \
    QUARANTINE_SWAP_MARKER="$test_root/rollback-quarantine-swap.rm-swapped" \
    SWAPPED_QUARANTINE="$case_data/bin/invocation-quarantined" \
    DVANDVA_RELEASE_DIR="$new_release" bash "$installer" \
    update --version 0.2.0 >"$test_root/rollback-quarantine-swap.out" 2>&1; then
    printf 'quarantine-swapped rollback unexpectedly installed the update\n' >&2
    exit 1
  fi
  test "$(readlink "$case_data/bin/current")" = '0.1.1'
  cmp "$test_root/rollback-quarantine-swap.manifest" "$case_data/installation.json"
  test ! -e "$test_root/rollback-quarantine-swap.rm-swapped"
  grep -Fq 'rollback_uncertain evidence=' "$test_root/rollback-quarantine-swap.out"
  mapfile -t transaction_evidence < <(
    find "$case_data" -maxdepth 1 -type d -name '.install-txn.*' -print
  )
  test "${#transaction_evidence[@]}" -eq 1
  test -x "${transaction_evidence[0]}/promoted-install/dvandva-kernel"
  test -f "${transaction_evidence[0]}/old-manifest"
)

rollback_quarantine_swap_case
empty_unowned_data_root_case
rollback_path_swap_case

# A signal delivered immediately after promotion still removes the fresh version.
promotion_term_bin="$test_root/promotion-term-bin"
mkdir -p "$promotion_term_bin"
cat >"$promotion_term_bin/mv" <<'TERM_AFTER_PROMOTION'
#!/usr/bin/env bash
set -euo pipefail
last="${!#}"
/usr/bin/mv "$@"
if test "$last" = "${PROMOTION_TARGET:?}"; then
  kill -TERM "$PPID"
fi
TERM_AFTER_PROMOTION
chmod 755 "$promotion_term_bin/mv"
(
  export XDG_DATA_HOME="$test_root/promotion-term/data"
  export XDG_STATE_HOME="$test_root/promotion-term/state"
  DVANDVA_RELEASE_DIR="$old_release" bash "$old_installer" \
    install --version 0.1.1 >/dev/null
  case_data="$XDG_DATA_HOME/dvandva"
  cp "$case_data/installation.json" "$test_root/promotion-term.manifest"
  if env PATH="$promotion_term_bin:$PATH" \
    PROMOTION_TARGET="$case_data/bin/0.2.0" \
    DVANDVA_RELEASE_DIR="$new_release" bash "$installer" \
    update --version 0.2.0 >"$test_root/promotion-term.out" 2>&1; then
    printf 'TERM after promotion unexpectedly installed the update\n' >&2
    exit 1
  fi
  test "$(readlink "$case_data/bin/current")" = '0.1.1'
  cmp "$test_root/promotion-term.manifest" "$case_data/installation.json"
  test ! -e "$case_data/bin/0.2.0"
  grep -Fq 'rollback_uncertain evidence=' "$test_root/promotion-term.out"
  mapfile -t transaction_evidence < <(
    find "$case_data" -maxdepth 1 -type d -name '.install-txn.*' -print
  )
  test "${#transaction_evidence[@]}" -eq 1
  test -x "${transaction_evidence[0]}/promoted-install/dvandva-kernel"
  test -f "${transaction_evidence[0]}/old-manifest"
  test -z "$(find "$case_data/bin" -maxdepth 1 -name '.0.2.0.*.tmp' -print)"
)

# Concurrent installers serialize: both succeed and no staging directory nests.
concurrent_release="$test_root/concurrent-release"
mkdir -p "$concurrent_release"
cat >"$concurrent_release/$asset" <<'SLOW_KERNEL'
#!/usr/bin/env bash
if test "${1:-}" = "--version"; then printf 'dvandva-v4 0.2.0\n'; exit 0; fi
if test "${1:-}" = "probe"; then
  sleep 1
  printf '%s\n' '{"package":"dvandva-v4","version":"0.2.0","publish":false,"write_schema":"dvandva.run.v2","read_schemas":["dvandva.run.v2","dvandva.run.v1"],"role_api":2,"capabilities":{"upgrade_from_v1":true},"compatible":true}'
  exit 0
fi
exit 99
SLOW_KERNEL
chmod 755 "$concurrent_release/$asset"
(cd "$concurrent_release" && sha256sum "$asset" >SHA256SUMS)
concurrent_data="$test_root/concurrent/data"
concurrent_state="$test_root/concurrent/state"
mkdir -p "$concurrent_data"
exec 8>>"$concurrent_data/.dvandva-install.lock"
flock -x 8
env XDG_DATA_HOME="$concurrent_data" XDG_STATE_HOME="$concurrent_state" \
  DVANDVA_RELEASE_DIR="$concurrent_release" bash "$installer" install --version 0.2.0 \
  >"$test_root/concurrent-a.out" 2>&1 &
first_pid=$!
sleep 1
kill -0 "$first_pid"
test ! -e "$concurrent_data/dvandva"
flock -u 8
exec 8>&-
wait "$first_pid"
env XDG_DATA_HOME="$concurrent_data" XDG_STATE_HOME="$concurrent_state" \
  DVANDVA_RELEASE_DIR="$concurrent_release" bash "$installer" install --version 0.2.0 \
  >"$test_root/concurrent-b.out" 2>&1
test "$(readlink "$concurrent_data/dvandva/bin/current")" = '0.2.0'
test "$(find "$concurrent_data/dvandva/bin/0.2.0" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' ')" = \
  '.owner dvandva-kernel '
test -z "$(find "$concurrent_data/dvandva" -name '*.tmp' -print)"

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
test ! -e "$XDG_DATA_HOME/dvandva"

# An owned uninstall leaves an absent root, so a normal reinstall remains valid.
DVANDVA_RELEASE_DIR="$new_release" bash "$installer" install --version 0.2.0 >/dev/null
test "$(readlink "$current")" = '0.2.0'
bash "$installer" uninstall --version 0.2.0 >/dev/null
test ! -e "$XDG_DATA_HOME/dvandva"
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
