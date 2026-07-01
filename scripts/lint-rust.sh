#!/usr/bin/env bash
# Rust lint + test gate for the dvandva Cargo workspace.
#
# Mirrors the definition-of-done gate: formatting, clippy (warnings denied),
# and the test suite, all against rust/Cargo.toml. Intended for CI and local
# pre-commit use.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/../rust/Cargo.toml"

echo "== cargo fmt --check =="
cargo fmt --manifest-path "$MANIFEST" --all --check

echo "== cargo clippy -D warnings =="
cargo clippy --manifest-path "$MANIFEST" --all-targets -- -D warnings

echo "== cargo test =="
cargo test --manifest-path "$MANIFEST"
