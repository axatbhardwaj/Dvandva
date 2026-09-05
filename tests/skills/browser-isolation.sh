#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
output_dir="$(mktemp -d)"
trap 'find "$output_dir" -type f -exec unlink {} \;; find "$output_dir" -depth -type d -exec rmdir {} \;' EXIT
playwright_bin="$(npx --yes --package=playwright@1.63.0 -c 'command -v playwright')"
playwright_modules="$(dirname "$(dirname "$(readlink -f "$playwright_bin")")")"
chromium_bin="$(command -v chromium || command -v chromium-browser || true)"
if test -z "$chromium_bin"; then
  chromium_bin="$(NODE_PATH="$playwright_modules" node -e \
    'process.stdout.write(require("playwright").chromium.executablePath())')"
fi
test -x "$chromium_bin"
output="$(cd "$repo_root" && NODE_PATH="$playwright_modules" PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH="$chromium_bin" "$playwright_bin" test tests/skills/browser-isolation.spec.js --reporter=line --workers=1 --output="$output_dir" 2>&1)" || {
  printf '%s\n' "$output" >&2
  exit 1
}
grep -Eq '1 passed' <<<"$output"
printf 'browser role isolation: ok\n'
