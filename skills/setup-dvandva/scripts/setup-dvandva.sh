#!/usr/bin/env bash
set -euo pipefail

owner="dvandva-skill-v1"
schema="dvandva.run.v1"
default_version="0.1.1"
operation="${1:-}"
shift || true
version="${DVANDVA_VERSION:-$default_version}"
purge_runs=false
confirm_purge=false
temporary_download=""

cleanup() {
  if test -n "$temporary_download"; then
    rm -rf -- "$temporary_download"
  fi
}
trap cleanup EXIT

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

manifest_owned() {
  test -f "$manifest" && grep -Fq '"owner": "dvandva-skill-v1"' "$manifest"
}

require_owned_or_empty() {
  if test -e "$data_root" && ! manifest_owned; then
    if find "$data_root" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null | grep -q .; then
      printf 'setup-dvandva: refusing unowned data at %s\n' "$data_root" >&2
      exit 1
    fi
  fi
}

asset_for_host() {
  test "$(uname -s)" = "Linux" || {
    printf 'setup-dvandva: only Linux is supported\n' >&2
    exit 1
  }
  case "$(uname -m)" in
    x86_64) printf 'dvandva-kernel-linux-x86_64\n' ;;
    *)
      printf 'setup-dvandva: unsupported architecture: %s\n' "$(uname -m)" >&2
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

manifest_value() {
  local key="$1"
  sed -n "s/^[[:space:]]*\"$key\": \"\([^\"]*\)\".*/\1/p" "$manifest" | head -n 1
}

write_manifest() {
  local digest="$1"
  local temporary="$data_root/.installation.$$.tmp"
  umask 077
  {
    printf '{\n'
    printf '  "owner": "%s",\n' "$owner"
    printf '  "version": "%s",\n' "$version"
    printf '  "schema": "%s",\n' "$schema"
    printf '  "asset": "%s",\n' "$(asset_for_host)"
    printf '  "sha256": "%s"\n' "$digest"
    printf '}\n'
  } >"$temporary"
  chmod 600 "$temporary"
  mv -fT -- "$temporary" "$manifest"
}

install_release() {
  require_owned_or_empty
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

  mkdir -p "$bin_root"
  if test -e "$version_dir"; then
    test -x "$binary" && test "$(sha256sum "$binary" | cut -d' ' -f1)" = "$digest" || {
      printf 'setup-dvandva: refusing to replace version %s in place\n' "$version" >&2
      exit 1
    }
  else
    local staged="$bin_root/.$version.$$.tmp"
    local probe_output
    mkdir "$staged"
    install -m 755 "$download_dir/$asset" "$staged/dvandva-kernel"
    printf '%s\n' "$owner" >"$staged/.owner"
    chmod 600 "$staged/.owner"
    probe_output="$("$staged/dvandva-kernel" probe --expected-schema "$schema")"
    grep -Fq '"compatible": true' <<<"$probe_output"
    mv -- "$staged" "$version_dir"
  fi

  local current_tmp="$bin_root/.current.$$.tmp"
  ln -s "$version" "$current_tmp"
  mv -fT -- "$current_tmp" "$bin_root/current"
  mkdir -p "$state_root/runs" "$state_root/credentials"
  chmod 700 "$state_root" "$state_root/runs" "$state_root/credentials"
  write_manifest "$digest"
  printf 'setup-dvandva: installed version=%s schema=%s sha256=%s binary=%s\n' \
    "$version" "$schema" "$digest" "$binary"
}

doctor() {
  manifest_owned || {
    printf 'setup-dvandva: unhealthy reason=installation_manifest_missing\n' >&2
    exit 1
  }
  local installed_version installed_digest installed_schema probe_output
  installed_version="$(manifest_value version)"
  installed_digest="$(manifest_value sha256)"
  installed_schema="$(manifest_value schema)"
  test "$installed_version" = "$version" || {
    printf 'setup-dvandva: unhealthy reason=version_mismatch expected=%s installed=%s\n' "$version" "$installed_version" >&2
    exit 1
  }
  test "$installed_schema" = "$schema" || {
    printf 'setup-dvandva: unhealthy reason=schema_mismatch\n' >&2
    exit 1
  }
  test "$(readlink "$bin_root/current" 2>/dev/null)" = "$version" || {
    printf 'setup-dvandva: unhealthy reason=current_link_mismatch\n' >&2
    exit 1
  }
  test -x "$binary" || {
    printf 'setup-dvandva: unhealthy reason=binary_missing\n' >&2
    exit 1
  }
  test "$(sha256sum "$binary" | cut -d' ' -f1)" = "$installed_digest" || {
    printf 'setup-dvandva: unhealthy reason=checksum_mismatch\n' >&2
    exit 1
  }
  probe_output="$("$binary" probe --expected-schema "$schema")" || {
    printf 'setup-dvandva: unhealthy reason=incompatible_probe\n' >&2
    exit 1
  }
  grep -Fq '"compatible": true' <<<"$probe_output" || {
    printf 'setup-dvandva: unhealthy reason=incompatible_probe\n' >&2
    exit 1
  }
  printf 'setup-dvandva: healthy version=%s schema=%s binary=%s\n' "$version" "$schema" "$binary"
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
  local current_target
  current_target="$(readlink "$bin_root/current" 2>/dev/null || true)"
  if test -n "$current_target" \
    && test -f "$bin_root/$current_target/.owner" \
    && grep -Fxq "$owner" "$bin_root/$current_target/.owner"; then
    rm -f -- "$bin_root/current"
  fi
  local version_path
  for version_path in "$bin_root"/*; do
    if test -f "$version_path/.owner" && grep -Fxq "$owner" "$version_path/.owner"; then
      rm -rf -- "$version_path"
    fi
  done
  rmdir -- "$bin_root" 2>/dev/null || true
  rm -f -- "$manifest"
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
  install|update) install_release ;;
  doctor) doctor ;;
  uninstall) uninstall_owned ;;
  *)
    printf 'usage: setup-dvandva.sh {install|update|doctor|uninstall} [--version X.Y.Z] [--purge-runs --yes-purge-runs]\n' >&2
    exit 2
    ;;
esac
