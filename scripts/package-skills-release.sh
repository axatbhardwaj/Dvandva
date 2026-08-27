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

if test -e "$output" || test -L "$output"; then
  printf 'package-skills-release: output_exists path=%s\n' "$output" >&2
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
output_parent="$(dirname -- "$output")"
output_name="$(basename -- "$output")"
version_max_bytes=256
probe_max_bytes=16384

mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd -P)"
output="$output_parent/$output_name"
if test -e "$output" || test -L "$output"; then
  printf 'package-skills-release: output_exists path=%s\n' "$output" >&2
  exit 1
fi

staging="$(mktemp -d "$output_parent/.${output_name}.tmp.XXXXXX")"

cleanup() {
  if test -n "${staging:-}"; then
    rm -rf -- "$staging"
  fi
}
trap cleanup EXIT

install -m 755 "$source_binary" "$staging/$asset"
if command -v strip >/dev/null 2>&1; then
  strip "$staging/$asset"
fi

version_file="$staging/.version"
set +e
"$staging/$asset" --version 2>/dev/null | \
  head -c "$((version_max_bytes + 1))" >"$version_file"
version_statuses=("${PIPESTATUS[@]}")
set -e

version_size="$(wc -c <"$version_file")"
if test "$version_size" -gt "$version_max_bytes"; then
  printf 'package-skills-release: binary_version_too_large max_bytes=%s\n' \
    "$version_max_bytes" >&2
  exit 1
fi
if test "${version_statuses[0]}" -ne 0 || test "${version_statuses[1]}" -ne 0; then
  printf 'package-skills-release: binary_version_mismatch expected=%s\n' \
    "dvandva-v4 $version" >&2
  exit 1
fi

python3 - "$version" "$version_file" "$version_max_bytes" <<'PY' || {
import sys
from pathlib import Path


raw = Path(sys.argv[2]).read_bytes()
if len(raw) > int(sys.argv[3]) or b"\0" in raw:
    raise SystemExit(1)
try:
    reported = raw.decode("utf-8", errors="strict")
except UnicodeDecodeError:
    raise SystemExit(1)
expected = f"dvandva-v4 {sys.argv[1]}"
raise SystemExit(0 if reported in (expected, expected + "\n") else 1)
PY
  printf 'package-skills-release: binary_version_mismatch expected=%s\n' \
    "dvandva-v4 $version" >&2
  exit 1
}

rm -f -- "$version_file"
probe_file="$staging/.probe.json"
set +e
"$staging/$asset" probe \
  --expected-schema dvandva.run.v2 --expected-role-api 2 2>/dev/null | \
  head -c "$((probe_max_bytes + 1))" >"$probe_file"
probe_statuses=("${PIPESTATUS[@]}")
set -e

probe_size="$(wc -c <"$probe_file")"
if test "$probe_size" -gt "$probe_max_bytes"; then
  printf 'package-skills-release: probe_too_large max_bytes=%s\n' \
    "$probe_max_bytes" >&2
  exit 1
fi
if test "${probe_statuses[0]}" -ne 0 || test "${probe_statuses[1]}" -ne 0; then
  printf 'package-skills-release: probe_mismatch expected_schema=dvandva.run.v2 expected_role_api=2\n' >&2
  exit 1
fi

python3 - "$version" "$probe_file" "$probe_max_bytes" <<'PY' || {
import json
import sys
from pathlib import Path


def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate key: {key}")
        value[key] = item
    return value


raw = Path(sys.argv[2]).read_bytes()
if len(raw) > int(sys.argv[3]) or b"\0" in raw:
    raise SystemExit(1)
text = raw.decode("utf-8", errors="strict")
if text.endswith("\n"):
    text = text[:-1]
decoder = json.JSONDecoder(object_pairs_hook=unique)
probe, end = decoder.raw_decode(text)
if end != len(text):
    raise ValueError("trailing probe bytes")
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

rm -f -- "$probe_file"
(cd "$staging" && sha256sum "$asset" >SHA256SUMS)
chmod 755 "$staging"

if ! mv -nT -- "$staging" "$output"; then
  if test -e "$output" || test -L "$output"; then
    printf 'package-skills-release: output_exists path=%s\n' "$output" >&2
  else
    printf 'package-skills-release: promotion_failed path=%s\n' "$output" >&2
  fi
  exit 1
fi
if test -e "$staging" || test -L "$staging"; then
  printf 'package-skills-release: output_exists path=%s\n' "$output" >&2
  exit 1
fi
staging=""
printf 'package-skills-release: packaged tag=%s asset=%s output=%s\n' \
  "$tag" "$asset" "$output" || true
