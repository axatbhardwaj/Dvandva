#!/usr/bin/env bash
set -euo pipefail

owner="dvandva-skill-v1"
schema="dvandva.run.v2"
role_api="2"
read_schemas="dvandva.run.v2,dvandva.run.v1"
upgrade_from_v1="true"
default_version="0.3.5"
version_max_bytes=256
probe_max_bytes=16384
operation="${1:-}"
shift || true
version="${DVANDVA_VERSION:-$default_version}"
purge_runs=false
confirm_purge=false
temporary_download=""
staged_install=""
transaction_dir=""
transaction_active=false
promoted_install=""
promoted_install_identity=""
old_manifest_present=false
old_current_present=false
old_current_target=""
preserve_transaction=false
candidate_capture=""
created_data_root=false
created_bin_root=false

cleanup() {
  local status=$?
  trap - EXIT
  if $transaction_active; then
    rollback_transaction || status=1
  fi
  if test -n "$temporary_download"; then
    rm -rf -- "$temporary_download" || true
  fi
  if test -n "$staged_install"; then
    rm -rf -- "$staged_install" || true
  fi
  if test -n "$transaction_dir" && ! $preserve_transaction; then
    rm -rf -- "$transaction_dir" || true
  fi
  if test -n "$candidate_capture"; then
    rm -rf -- "$candidate_capture" || true
  fi
  if $created_bin_root; then
    rmdir -- "$bin_root" 2>/dev/null || true
  fi
  if $created_data_root; then
    rmdir -- "$data_root" 2>/dev/null || true
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

while (($#)); do
  case "$1" in
    --version)
      version="${2:-}"
      shift 2
      ;;
    --purge-runs)
      purge_runs=true
      shift
      ;;
    --yes-purge-runs)
      confirm_purge=true
      shift
      ;;
    *)
      printf 'setup-dvandva: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

case "$version" in
  ''|*[!0-9.]*|.*|*.)
    printf 'setup-dvandva: invalid version\n' >&2
    exit 2
    ;;
esac

data_home="${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}"
state_home="${XDG_STATE_HOME:-${HOME:?HOME is required}/.local/state}"
data_root="$data_home/dvandva"
state_root="$state_home/dvandva"
manifest="$data_root/installation.json"
bin_root="$data_root/bin"
version_dir="$bin_root/$version"
binary="$version_dir/dvandva-kernel"

require_runtime() {
  command -v python3 >/dev/null 2>&1 || {
    printf 'setup-dvandva: python3 is required to validate release metadata\n' >&2
    exit 1
  }
  command -v flock >/dev/null 2>&1 || {
    printf 'setup-dvandva: flock is required to serialize installation changes\n' >&2
    exit 1
  }
}

acquire_install_lock() {
  mkdir -p "$data_home"
  local lock="$data_home/.dvandva-install.lock"
  if test -e "$lock" || test -L "$lock"; then
    test -f "$lock" && ! test -L "$lock" || {
      printf 'setup-dvandva: refusing unsafe install lock at %s\n' "$lock" >&2
      exit 1
    }
  fi
  umask 077
  exec 9>>"$lock"
  flock -x -w 30 9 || {
    printf 'setup-dvandva: timed out waiting for install lock\n' >&2
    exit 1
  }
}

validate_managed_paths() {
  if test -e "$data_root" || test -L "$data_root"; then
    test -d "$data_root" && ! test -L "$data_root" || {
      printf 'setup-dvandva: refusing unsafe data root at %s\n' "$data_root" >&2
      exit 1
    }
  fi
  if test -e "$bin_root" || test -L "$bin_root"; then
    test -d "$bin_root" && ! test -L "$bin_root" || {
      printf 'setup-dvandva: refusing unsafe bin root at %s\n' "$bin_root" >&2
      exit 1
    }
  fi
}

manifest_owned() {
  test -f "$manifest" && ! test -L "$manifest" &&
    python3 -c '
import json, os, re, stat, sys
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result
try:
    fd = os.open(sys.argv[1], os.O_RDONLY | os.O_NOFOLLOW)
    if not stat.S_ISREG(os.fstat(fd).st_mode):
        raise ValueError("not regular")
    with os.fdopen(fd, encoding="utf-8") as handle:
        value = json.load(handle, object_pairs_hook=unique)
    keys = set(value) if type(value) is dict else set()
    common = (
        type(value) is dict
        and value.get("owner") == sys.argv[2]
        and type(value.get("version")) is str
        and re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value["version"])
        and value.get("asset") == "dvandva-kernel-linux-x86_64"
        and type(value.get("sha256")) is str
        and re.fullmatch(r"[0-9a-f]{64}", value["sha256"])
    )
    legacy = keys == {"owner", "version", "schema", "asset", "sha256"} and value.get("schema") == "dvandva.run.v1"
    current = (
        keys == {"owner", "version", "write_schema", "read_schemas", "role_api", "upgrade_from_v1", "publish", "asset", "sha256"}
        and value.get("write_schema") == "dvandva.run.v2"
        and value.get("read_schemas") == ["dvandva.run.v2", "dvandva.run.v1"]
        and type(value.get("role_api")) is int and value["role_api"] == 2
        and value.get("upgrade_from_v1") is True
        and value.get("publish") is False
    )
    valid = common and (legacy or current)
except (OSError, UnicodeError, ValueError, TypeError, json.JSONDecodeError):
    valid = False
raise SystemExit(0 if valid else 1)
' "$manifest" "$owner" 2>/dev/null
}

require_owned_or_absent() {
  if { test -e "$data_root" || test -L "$data_root"; } && ! manifest_owned; then
    printf 'setup-dvandva: refusing unowned data at %s\n' "$data_root" >&2
    exit 1
  fi
}

asset_for_host() {
  test "$(uname -s)" = "Linux" || {
    printf 'setup-dvandva: only Linux x86_64 is supported for now (host: %s)\n' "$(uname -s)" >&2
    exit 1
  }
  case "$(uname -m)" in
    x86_64) printf 'dvandva-kernel-linux-x86_64\n' ;;
    *)
      printf 'setup-dvandva: unsupported architecture: %s (only x86_64 for now)\n' "$(uname -m)" >&2
      exit 1
      ;;
  esac
}

fetch_release_file() {
  local name="$1"
  local destination="$2"
  if test -n "${DVANDVA_RELEASE_DIR:-}"; then
    cp -- "$DVANDVA_RELEASE_DIR/$name" "$destination"
    return
  fi
  local base="${DVANDVA_RELEASE_BASE_URL:-https://github.com/axatbhardwaj/Dvandva/releases/download}"
  curl --fail --location --silent --show-error \
    "$base/skills-v$version/$name" --output "$destination"
}

write_manifest_to() {
  local destination="$1" digest="$2" asset="$3"
  umask 077
  {
    printf '{\n'
    printf '  "owner": "%s",\n' "$owner"
    printf '  "version": "%s",\n' "$version"
    printf '  "write_schema": "%s",\n' "$schema"
    printf '  "read_schemas": ["dvandva.run.v2", "dvandva.run.v1"],\n'
    printf '  "role_api": %s,\n' "$role_api"
    printf '  "upgrade_from_v1": %s,\n' "$upgrade_from_v1"
    printf '  "publish": false,\n'
    printf '  "asset": "%s",\n' "$asset"
    printf '  "sha256": "%s"\n' "$digest"
    printf '}\n'
  } >"$destination" || {
    printf 'setup-dvandva: manifest_prepare_failed\n' >&2
    exit 1
  }
  chmod 600 "$destination" || {
    printf 'setup-dvandva: manifest_prepare_failed\n' >&2
    exit 1
  }
}

manifest_digest() {
  local path="$1" expected_version="$2" expected_asset="$3"
  python3 -c '
import json, os, re, stat, sys
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result
try:
    fd = os.open(sys.argv[1], os.O_RDONLY | os.O_NOFOLLOW)
    if not stat.S_ISREG(os.fstat(fd).st_mode):
        raise ValueError("not regular")
    with os.fdopen(fd, encoding="utf-8") as handle:
        value = json.load(handle, object_pairs_hook=unique)
    expected_keys = {"owner", "version", "write_schema", "read_schemas", "role_api", "upgrade_from_v1", "publish", "asset", "sha256"}
    digest = value.get("sha256") if type(value) is dict else None
    valid = (
        type(value) is dict and set(value) == expected_keys
        and value.get("owner") == sys.argv[2]
        and value.get("version") == sys.argv[3]
        and value.get("write_schema") == "dvandva.run.v2"
        and value.get("read_schemas") == ["dvandva.run.v2", "dvandva.run.v1"]
        and type(value.get("role_api")) is int and value["role_api"] == 2
        and value.get("upgrade_from_v1") is True
        and value.get("publish") is False
        and value.get("asset") == sys.argv[4]
        and type(digest) is str and re.fullmatch(r"[0-9a-f]{64}", digest)
    )
except (OSError, UnicodeError, ValueError, TypeError, json.JSONDecodeError):
    valid = False
if not valid:
    raise SystemExit(1)
print(digest)
' "$path" "$owner" "$expected_version" "$expected_asset" 2>/dev/null
}

validate_candidate() {
  local candidate="$1"
  local version_file probe_file candidate_status capture_status
  local -a pipeline_status
  command -v python3 >/dev/null 2>&1 || {
    printf 'setup-dvandva: python3 is required to validate release metadata\n' >&2
    exit 1
  }
  candidate_capture="$(mktemp -d)"
  version_file="$candidate_capture/version.raw"
  probe_file="$candidate_capture/probe.raw"

  set +e
  "$candidate" --version 2>/dev/null | \
    head -c "$((version_max_bytes + 1))" >"$version_file"
  pipeline_status=("${PIPESTATUS[@]}")
  set -e
  candidate_status=${pipeline_status[0]}
  capture_status=${pipeline_status[1]}
  if test "$candidate_status" -ne 0 || test "$capture_status" -ne 0; then
    printf 'setup-dvandva: version_mismatch expected=%s reported=unavailable\n' "$version" >&2
    exit 1
  fi
  python3 -c '
import pathlib, sys
raw = pathlib.Path(sys.argv[1]).read_bytes()
try:
    raw.decode("utf-8")
except UnicodeDecodeError:
    raise SystemExit(1)
expected = ("dvandva-v4 " + sys.argv[2]).encode()
valid = len(raw) <= int(sys.argv[3]) and b"\0" not in raw and raw in (expected, expected + b"\n")
raise SystemExit(0 if valid else 1)
' "$version_file" "$version" "$version_max_bytes" || {
    printf 'setup-dvandva: version_mismatch expected=%s reported=invalid\n' \
      "$version" >&2
    exit 1
  }

  set +e
  "$candidate" probe --expected-schema "$schema" \
    --expected-role-api "$role_api" 2>/dev/null | \
    head -c "$((probe_max_bytes + 1))" >"$probe_file"
  pipeline_status=("${PIPESTATUS[@]}")
  set -e
  candidate_status=${pipeline_status[0]}
  capture_status=${pipeline_status[1]}
  if test "$candidate_status" -ne 0 || test "$capture_status" -ne 0; then
    printf 'setup-dvandva: probe_mismatch expected_schema=%s expected_role_api=%s\n' \
      "$schema" "$role_api" >&2
    exit 1
  fi
  python3 -c '
import json, pathlib, sys
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result
try:
    raw = pathlib.Path(sys.argv[1]).read_bytes()
    if len(raw) > int(sys.argv[3]) or b"\0" in raw:
        raise ValueError("invalid raw probe")
    text = raw.decode("utf-8")
    if text.endswith("\n"):
        text = text[:-1]
    if text.endswith("\n"):
        raise ValueError("extra newline")
    decoder = json.JSONDecoder(object_pairs_hook=unique)
    probe, end = decoder.raw_decode(text)
    if end != len(text):
        raise ValueError("trailing bytes")
except (json.JSONDecodeError, UnicodeDecodeError, ValueError, TypeError):
    raise SystemExit(1)
capabilities = probe.get("capabilities") if type(probe) is dict else None
valid = (
    type(probe) is dict
    and set(probe) == {"package", "version", "publish", "write_schema", "read_schemas", "role_api", "capabilities", "compatible"}
    and probe.get("package") == "dvandva-v4"
    and probe.get("version") == sys.argv[2]
    and probe.get("publish") is False
    and probe.get("write_schema") == "dvandva.run.v2"
    and probe.get("read_schemas") == ["dvandva.run.v2", "dvandva.run.v1"]
    and type(probe.get("role_api")) is int and probe["role_api"] == 2
    and type(capabilities) is dict and set(capabilities) == {"upgrade_from_v1"}
    and capabilities.get("upgrade_from_v1") is True
    and probe.get("compatible") is True
)
raise SystemExit(0 if valid else 1)
' "$probe_file" "$version" "$probe_max_bytes" || {
      printf 'setup-dvandva: probe_mismatch expected_schema=%s expected_role_api=%s\n' \
        "$schema" "$role_api" >&2
      exit 1
    }
  rm -rf -- "$candidate_capture"
  candidate_capture=""
}

owner_marker_matches() {
  local path="$1"
  python3 -c '
import os, stat, sys
try:
    fd = os.open(sys.argv[1], os.O_RDONLY | os.O_NOFOLLOW)
    valid = stat.S_ISREG(os.fstat(fd).st_mode) and os.read(fd, 4096) == (sys.argv[2] + "\n").encode()
    os.close(fd)
except OSError:
    valid = False
raise SystemExit(0 if valid else 1)
' "$path" "$owner" 2>/dev/null
}

validate_existing_version() {
  local digest="$1" owner_file="$version_dir/.owner"
  test -d "$version_dir" && ! test -L "$version_dir" &&
    owner_marker_matches "$owner_file" &&
    test -f "$binary" && ! test -L "$binary" && test -x "$binary" &&
    test "$(sha256sum "$binary" | cut -d' ' -f1)" = "$digest" || {
      printf 'setup-dvandva: unsafe existing version %s\n' "$version" >&2
      exit 1
    }
  validate_candidate "$binary"
}

owned_version_path() {
  local path="$1" owner_file="$1/.owner"
  test -d "$path" && ! test -L "$path" &&
    owner_marker_matches "$owner_file"
}

quarantine_promoted_path() {
  local quarantine="$transaction_dir/promoted-install"
  ! test -e "$quarantine" && ! test -L "$quarantine" || return 1
  python3 -c '
import ctypes, os, stat, sys

source, quarantine, expected = sys.argv[1:]
try:
    expected_device, expected_inode = (int(part) for part in expected.split(":", 1))
    source_parent, source_name = os.path.split(source)
    quarantine_parent, quarantine_name = os.path.split(quarantine)
    open_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    source_parent_fd = os.open(source_parent, open_flags)
    quarantine_parent_fd = os.open(quarantine_parent, open_flags)
except (OSError, TypeError, ValueError):
    raise SystemExit(1)

quarantine_fd = None
try:
    libc = ctypes.CDLL(None, use_errno=True)
    renameat2 = libc.renameat2
    renameat2.argtypes = (
        ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint
    )
    renameat2.restype = ctypes.c_int
    result = renameat2(
        source_parent_fd,
        os.fsencode(source_name),
        quarantine_parent_fd,
        os.fsencode(quarantine_name),
        1,  # RENAME_NOREPLACE
    )
    if result != 0:
        raise OSError(ctypes.get_errno(), "renameat2")
    quarantine_fd = os.open(
        quarantine_name, open_flags, dir_fd=quarantine_parent_fd
    )
    status = os.fstat(quarantine_fd)
    valid = (
        stat.S_ISDIR(status.st_mode)
        and status.st_dev == expected_device
        and status.st_ino == expected_inode
    )
except (AttributeError, OSError):
    valid = False
finally:
    if quarantine_fd is not None:
        os.close(quarantine_fd)
    os.close(quarantine_parent_fd)
    os.close(source_parent_fd)
raise SystemExit(0 if valid else 1)
' "$promoted_install" "$quarantine" "$promoted_install_identity" 2>/dev/null
}

preserve_promoted_install() {
  if ! test -e "$promoted_install" && ! test -L "$promoted_install"; then
    return 0
  fi
  test -d "$promoted_install" && ! test -L "$promoted_install" || return 1
  test -n "$promoted_install_identity" || return 1
  test "$(stat -c '%d:%i' -- "$promoted_install" 2>/dev/null)" = \
    "$promoted_install_identity" || return 1
  quarantine_promoted_path || return 1
  # Linux cannot recursively unlink this directory by its verified descriptor.
  # Keep the quarantined inode and transaction evidence instead of reopening it.
  return 1
}

rollback_uncertain() {
  preserve_transaction=true
  printf 'setup-dvandva: rollback_uncertain evidence=%s\n' "$transaction_dir" >&2
  return 1
}

rollback_transaction() {
  local current_restored=false manifest_restored=false
  if $old_current_present; then
    if ln -s "$old_current_target" "$transaction_dir/restore-current" &&
      mv -fT -- "$transaction_dir/restore-current" "$bin_root/current" &&
      test "$(readlink "$bin_root/current" 2>/dev/null)" = "$old_current_target"; then
      current_restored=true
    fi
  else
    if rm -f -- "$bin_root/current" && ! test -e "$bin_root/current" && ! test -L "$bin_root/current"; then
      current_restored=true
    fi
  fi
  if $old_manifest_present; then
    if cp -p -- "$transaction_dir/old-manifest" "$transaction_dir/restore-manifest" &&
      mv -fT -- "$transaction_dir/restore-manifest" "$manifest" &&
      cmp -s "$transaction_dir/old-manifest" "$manifest"; then
      manifest_restored=true
    fi
  else
    if rm -f -- "$manifest" && ! test -e "$manifest" && ! test -L "$manifest"; then
      manifest_restored=true
    fi
  fi
  if $current_restored && $manifest_restored; then
    if test -n "$promoted_install"; then
      preserve_transaction=true
      if ! preserve_promoted_install; then
        rollback_uncertain
        return 1
      fi
      preserve_transaction=false
    fi
    promoted_install=""
    promoted_install_identity=""
    return 0
  fi
  rollback_uncertain
}

prepare_transaction() {
  local digest="$1" asset="$2"
  transaction_dir="$(mktemp -d "$data_root/.install-txn.XXXXXX")" || {
    printf 'setup-dvandva: manifest_prepare_failed\n' >&2
    exit 1
  }
  if test -e "$manifest" || test -L "$manifest"; then
    manifest_owned || {
      printf 'setup-dvandva: refusing invalid installation manifest\n' >&2
      exit 1
    }
    cp -p -- "$manifest" "$transaction_dir/old-manifest"
    old_manifest_present=true
  fi
  if test -e "$bin_root/current" || test -L "$bin_root/current"; then
    test -L "$bin_root/current" || {
      printf 'setup-dvandva: refusing unsafe current path\n' >&2
      exit 1
    }
    old_current_target="$(readlink "$bin_root/current")"
    test -n "$old_current_target" || {
      printf 'setup-dvandva: refusing unsafe current path\n' >&2
      exit 1
    }
    old_current_present=true
  fi
  write_manifest_to "$transaction_dir/new-manifest" "$digest" "$asset"
  test "$(manifest_digest "$transaction_dir/new-manifest" "$version" "$asset")" = "$digest" || {
    printf 'setup-dvandva: manifest_prepare_failed\n' >&2
    exit 1
  }
  ln -s "$version" "$transaction_dir/new-current"
  transaction_active=true
}

install_release() {
  require_owned_or_absent
  local asset
  asset="$(asset_for_host)"
  local download_dir
  download_dir="$(mktemp -d)"
  temporary_download="$download_dir"
  fetch_release_file "$asset" "$download_dir/$asset"
  fetch_release_file "SHA256SUMS" "$download_dir/SHA256SUMS"

  local digest
  digest="$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset { print $1; exit }' "$download_dir/SHA256SUMS")"
  test "${#digest}" = 64 || {
    printf 'setup-dvandva: checksum manifest does not name %s\n' "$asset" >&2
    exit 1
  }
  if ! printf '%s  %s\n' "$digest" "$download_dir/$asset" | sha256sum -c - >/dev/null; then
    printf 'setup-dvandva: checksum_mismatch asset=%s\n' "$asset" >&2
    exit 1
  fi

  if ! test -e "$data_root" && ! test -L "$data_root"; then
    created_data_root=true
  fi
  if ! test -e "$bin_root" && ! test -L "$bin_root"; then
    created_bin_root=true
  fi
  mkdir -p "$bin_root"
  if test -e "$version_dir" || test -L "$version_dir"; then
    validate_existing_version "$digest"
  else
    local staged="$bin_root/.$version.$$.tmp"
    staged_install="$staged"
    mkdir "$staged"
    install -m 755 "$download_dir/$asset" "$staged/dvandva-kernel"
    printf '%s\n' "$owner" >"$staged/.owner"
    chmod 600 "$staged/.owner"
    validate_candidate "$staged/dvandva-kernel"
  fi

  prepare_transaction "$digest" "$asset"
  if test -n "$staged_install"; then
    promoted_install_identity="$(stat -c '%d:%i' -- "$staged_install")"
    promoted_install="$version_dir"
    mv -T -- "$staged_install" "$version_dir"
    staged_install=""
  fi
  mv -fT -- "$transaction_dir/new-manifest" "$manifest"
  mv -fT -- "$transaction_dir/new-current" "$bin_root/current"
  transaction_active=false
  promoted_install=""
  promoted_install_identity=""
  created_data_root=false
  created_bin_root=false
  printf 'setup-dvandva: installed version=%s write_schema=%s role_api=%s read_schemas=%s upgrade_from_v1=%s publish=false sha256=%s binary=%s\n' \
    "$version" "$schema" "$role_api" "$read_schemas" "$upgrade_from_v1" "$digest" "$binary" || true
}

doctor() {
  manifest_owned || {
    printf 'setup-dvandva: unhealthy reason=installation_manifest_missing\n' >&2
    exit 1
  }
  local asset installed_digest
  asset="$(asset_for_host)"
  installed_digest="$(manifest_digest "$manifest" "$version" "$asset")" || {
    printf 'setup-dvandva: unhealthy reason=installation_manifest_invalid\n' >&2
    exit 1
  }
  test "$(readlink "$bin_root/current" 2>/dev/null)" = "$version" || {
    printf 'setup-dvandva: unhealthy reason=current_link_mismatch\n' >&2
    exit 1
  }
  validate_existing_version "$installed_digest"
  printf 'setup-dvandva: healthy version=%s write_schema=%s role_api=%s read_schemas=%s upgrade_from_v1=%s publish=false binary=%s\n' \
    "$version" "$schema" "$role_api" "$read_schemas" "$upgrade_from_v1" "$binary"
}

uninstall_owned() {
  manifest_owned || {
    printf 'setup-dvandva: refusing uninstall without owned manifest\n' >&2
    exit 1
  }
  if $purge_runs && ! $confirm_purge; then
    printf 'setup-dvandva: --purge-runs requires --yes-purge-runs\n' >&2
    exit 2
  fi
  if $purge_runs; then
    if test -e "$state_root" || test -L "$state_root"; then
      test -d "$state_root" && ! test -L "$state_root" || {
        printf 'setup-dvandva: refusing unsafe state root at %s\n' "$state_root" >&2
        exit 1
      }
    fi
    if test -e "$state_root/runs" || test -L "$state_root/runs"; then
      test -d "$state_root/runs" && ! test -L "$state_root/runs" || {
        printf 'setup-dvandva: refusing unsafe runs path at %s\n' "$state_root/runs" >&2
        exit 1
      }
    fi
  fi
  local current_target
  current_target="$(readlink "$bin_root/current" 2>/dev/null || true)"
  if test -n "$current_target" \
    && owned_version_path "$bin_root/$current_target"; then
    rm -f -- "$bin_root/current"
  fi
  local version_path
  for version_path in "$bin_root"/*; do
    if owned_version_path "$version_path"; then
      rm -rf -- "$version_path"
    fi
  done
  rmdir -- "$bin_root" 2>/dev/null || true
  rm -f -- "$manifest"
  rmdir -- "$data_root" 2>/dev/null || true
  if $purge_runs; then
    rm -rf -- "$state_root/runs"
  fi
  local preserved="true"
  if $purge_runs; then
    preserved="false"
  fi
  printf 'setup-dvandva: uninstalled preserved_runs=%s\n' "$preserved"
}

case "$operation" in
  install|update|doctor|uninstall) ;;
  *)
    printf 'usage: setup-dvandva.sh {install|update|doctor|uninstall} [--version X.Y.Z] [--purge-runs --yes-purge-runs]\n' >&2
    exit 2
    ;;
esac

require_runtime
acquire_install_lock
validate_managed_paths
case "$operation" in
  install|update) install_release ;;
  doctor) doctor ;;
  uninstall) uninstall_owned ;;
esac
