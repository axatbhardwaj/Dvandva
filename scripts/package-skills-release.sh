#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
output="${2:-}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
manifest="$repo_root/v4/Cargo.toml"

if test -z "$tag" || test -z "$output"; then
  printf 'usage: package-skills-release.sh skills-vX.Y.Z OUTPUT_DIR\n' >&2
  exit 2
fi

version="$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$manifest" | head -n 1)"
if test "$tag" != "skills-v$version"; then
  printf 'package-skills-release: version_mismatch tag=%s source=%s\n' "$tag" "$version" >&2
  exit 1
fi

if test -e "$output" && find "$output" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
  printf 'package-skills-release: output_not_empty path=%s\n' "$output" >&2
  exit 1
fi

test "$(uname -s)" = "Linux" && test "$(uname -m)" = "x86_64" || {
  printf 'package-skills-release: unsupported_host expected=linux-x86_64\n' >&2
  exit 1
}

cargo build --locked --release --manifest-path "$manifest"
target_root="${CARGO_TARGET_DIR:-$repo_root/v4/target}"
source_binary="$target_root/release/dvandva-v4"
asset="dvandva-kernel-linux-x86_64"
staging="$(mktemp -d)"

cleanup() {
  rm -rf -- "$staging"
}
trap cleanup EXIT

install -m 755 "$source_binary" "$staging/$asset"
if command -v strip >/dev/null 2>&1; then
  strip "$staging/$asset"
fi

reported_version="$("$staging/$asset" --version 2>/dev/null || true)"
if test "$reported_version" != "dvandva-v4 $version"; then
  printf 'package-skills-release: binary_version_mismatch expected=%s reported=%s\n' \
    "dvandva-v4 $version" "$reported_version" >&2
  exit 1
fi

probe_output="$("$staging/$asset" probe \
  --expected-schema dvandva.run.v2 --expected-role-api 2 2>/dev/null)" || {
  printf 'package-skills-release: probe_mismatch expected_schema=dvandva.run.v2 expected_role_api=2\n' >&2
  exit 1
}

python3 - "$version" "$probe_output" <<'PY' || {
import json
import sys


def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate key: {key}")
        value[key] = item
    return value


probe = json.loads(sys.argv[2], object_pairs_hook=unique)
capabilities = probe.get("capabilities") if type(probe) is dict else None
valid = (
    type(probe) is dict
    and set(probe) == {
        "package", "version", "publish", "write_schema", "read_schemas",
        "role_api", "capabilities", "compatible",
    }
    and probe.get("package") == "dvandva-v4"
    and probe.get("version") == sys.argv[1]
    and probe.get("publish") is False
    and probe.get("write_schema") == "dvandva.run.v2"
    and probe.get("read_schemas") == ["dvandva.run.v2", "dvandva.run.v1"]
    and type(probe.get("role_api")) is int
    and probe["role_api"] == 2
    and type(capabilities) is dict
    and set(capabilities) == {"upgrade_from_v1"}
    and capabilities.get("upgrade_from_v1") is True
    and probe.get("compatible") is True
)
raise SystemExit(0 if valid else 1)
PY
  printf 'package-skills-release: probe_mismatch expected_schema=dvandva.run.v2 expected_role_api=2\n' >&2
  exit 1
}

mkdir -p "$output"
install -m 755 "$staging/$asset" "$output/$asset"
(cd "$output" && sha256sum "$asset" >SHA256SUMS)
printf 'package-skills-release: packaged tag=%s asset=%s output=%s\n' \
  "$tag" "$asset" "$output"
