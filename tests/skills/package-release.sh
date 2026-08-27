#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
packager="$repo_root/scripts/package-skills-release.sh"

output="$test_root/output"
CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.0 "$output"

test -x "$output/dvandva-kernel-linux-x86_64"
test -f "$output/SHA256SUMS"
test "$(find "$output" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' ')" = \
  'SHA256SUMS dvandva-kernel-linux-x86_64 '
(cd "$output" && sha256sum -c SHA256SUMS)
test "$($output/dvandva-kernel-linux-x86_64 --version)" = 'dvandva-v4 0.2.0'

wrong="$test_root/wrong"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.1 "$wrong" \
  >"$test_root/wrong.out" 2>&1; then
  printf 'mismatched tag unexpectedly packaged\n' >&2
  exit 1
fi
grep -Fq 'version_mismatch' "$test_root/wrong.out"
test ! -e "$wrong"

nonempty="$test_root/nonempty"
mkdir -p "$nonempty"
touch "$nonempty/foreign"
if CARGO_TARGET_DIR="$test_root/target" bash "$packager" skills-v0.2.0 "$nonempty" \
  >"$test_root/nonempty.out" 2>&1; then
  printf 'non-empty destination unexpectedly accepted\n' >&2
  exit 1
fi
grep -Fq 'output_not_empty' "$test_root/nonempty.out"
test -f "$nonempty/foreign"

workflow="$repo_root/.github/workflows/skills-release.yml"
grep -Fq '"skills-v*"' "$workflow"
grep -Fq 'contents: write' "$workflow"
grep -Fq 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' "$workflow"
grep -Fq 'cargo test --manifest-path rust/dvandva/Cargo.toml --locked' "$workflow"
grep -Fq 'bash tests/skills/two-role-canary.sh' "$workflow"

workflow_doc="$repo_root/docs/workflows/skill-only-run.md"
grep -Fq -- '--agent claude-code codex' "$workflow_doc"
grep -Fq -- '--skill setup-dvandva vadi prativadi' "$workflow_doc"
grep -Fq 'never puts the helper on' "$workflow_doc"

printf 'skills release packaging: ok\n'
