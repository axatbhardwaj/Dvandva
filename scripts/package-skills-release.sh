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

mkdir -p "$output"
install -m 755 "$source_binary" "$output/$asset"
if command -v strip >/dev/null 2>&1; then
  strip "$output/$asset"
fi

test "$($output/$asset --version)" = "dvandva-v4 $version"
(cd "$output" && sha256sum "$asset" >SHA256SUMS)
printf 'package-skills-release: packaged tag=%s asset=%s output=%s\n' \
  "$tag" "$asset" "$output"
