#!/usr/bin/env bash
# Lint generated human-facing Dvandva artifacts.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="${1:-$ROOT_DIR/superpowers}"
failures=0

fail() {
  echo "FAIL: $1"
  failures=$((failures + 1))
}

pass() {
  echo "PASS: $1"
}

if [[ ! -d "$ARTIFACT_DIR" ]]; then
  pass "no generated artifact directory present"
  exit 0
fi

mapfile -t markdown_files < <(find "$ARTIFACT_DIR" -type f -name '*.md' | sort)
if [[ "${#markdown_files[@]}" -gt 0 ]]; then
  fail "generated Markdown artifacts are not allowed under $ARTIFACT_DIR"
  printf '  %s\n' "${markdown_files[@]#$ROOT_DIR/}"
else
  pass "no generated Markdown artifacts under $ARTIFACT_DIR"
fi

mapfile -t html_files < <(find "$ARTIFACT_DIR" -type f -name '*.html' | sort)
if [[ "${#html_files[@]}" -eq 0 ]]; then
  fail "no generated HTML artifacts found under $ARTIFACT_DIR"
fi

for file in "${html_files[@]}"; do
  rel="${file#$ROOT_DIR/}"

  if head -n 5 "$file" | grep -iq '<!doctype html'; then
    pass "$rel declares HTML doctype"
  else
    fail "$rel missing HTML doctype"
  fi

  if grep -Fq 'color-scheme: dark' "$file"; then
    pass "$rel declares dark color scheme"
  else
    fail "$rel missing dark color-scheme"
  fi

  if grep -Eq '<script[^>]+type="application/json"[^>]+id="dvandva-artifact-meta"|<script[^>]+id="dvandva-artifact-meta"[^>]+type="application/json"' "$file"; then
    pass "$rel includes Dvandva artifact metadata block"
  else
    fail "$rel missing Dvandva artifact metadata block"
  fi

  meta="$(awk '/<script[^>]*id="dvandva-artifact-meta"[^>]*>/{flag=1; next} flag && /<\/script>/{exit} flag {print}' "$file")"
  if [[ -n "$meta" ]] && echo "$meta" | jq -e '.schema | startswith("dvandva.artifact.")' >/dev/null 2>&1; then
    pass "$rel metadata JSON parses"
  else
    fail "$rel metadata JSON missing or invalid"
  fi

  if grep -Eiq '<script[^>]+src[[:space:]]*=[[:space:]]*["'\'']?https?://' "$file" \
    || grep -Eiq '<link[^>]+href[[:space:]]*=[[:space:]]*["'\'']?https?://' "$file" \
    || grep -Eiq '<(img|iframe|source|video|audio)[^>]+src[[:space:]]*=[[:space:]]*["'\'']?https?://' "$file" \
    || grep -Eiq 'url\([[:space:]]*["'\'']?https?://' "$file" \
    || grep -Eiq '@import[[:space:]]+(url\([[:space:]]*)?["'\'']?https?://' "$file"; then
    fail "$rel contains external resource reference"
  else
    pass "$rel has no external resource references"
  fi
done

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
