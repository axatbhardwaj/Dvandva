#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT

validator="$repo_root/skills/html-deliverables/scripts/validate.py"
template="$repo_root/skills/html-deliverables/template.html"

test -x "$validator"
test -f "$template"
grep -Fq 'allow_implicit_invocation: true' \
  "$repo_root/skills/html-deliverables/agents/openai.yaml"

valid="$test_root/valid.html"
python3 - "$template" "$valid" <<'PY'
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text()
replacements = {
    "<!-- concise page title -->": "Dvandva delivery review",
    "<!-- full title -->": "Dvandva delivery review",
    "<!-- YYYY-MM-DD -->": "2026-09-05",
    "<!-- what ground truth this reflects: version, commit, source -->": "checkpoint abc123",
    "<!-- subject line, lowercase mono -->": "delivery review",
    "<!-- the thesis headline; map opposition with <span class=\"k-vadi\">/<span class=\"k-prat\"> -->": "One checkpoint, two roles",
    "<!-- 1–2 sentence stance, --dim -->": "The approved bytes and review evidence describe one immutable delivery.",
    "<!-- hard fact -->": "checkpoint abc123",
    "<!-- section eyebrow -->": "handoff map",
    "<!-- thesis, not topic -->": "Review follows the immutable checkpoint",
    "<!-- prose annotating the figure below -->": "The author stages exact bytes before the reviewer records a verdict.",
    "<!-- what it shows -->": "Author to reviewer handoff",
    "<!-- the one insight the drawing can't say -->": "Approval binds the staged digest.",
    "<!-- WHAT · as of VERSION/COMMIT · DATE -->": "Dvandva delivery · as of abc123 · 2026-09-05",
}
for old, new in replacements.items():
    text = text.replace(old, new)
Path(sys.argv[2]).write_text(text)
PY

python3 "$validator" "$valid" | grep -Fq 'html-deliverable: valid'

expect_failure() {
  local label="$1"
  local expected="$2"
  local file="$test_root/$label.html"
  cp "$valid" "$file"
  python3 - "$file" "$label" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text()
case = sys.argv[2]
if case == "bad-meta":
    text = text.replace('"date": "2026-09-05"', '"date": "05/09/2026"')
elif case == "compact-date":
    text = text.replace('"date": "2026-09-05"', '"date": "20260905"')
elif case == "unreplaced-meta":
    text = text.replace("checkpoint abc123", "<!-- unresolved basis -->", 1)
elif case == "bad-schema":
    text = text.replace("dvandva.artifact.run_explainer.v1", "dvandva.artifact.research.v1")
elif case == "missing-token":
    text = text.replace("--prat:#a78bfa;", "--prat:#ffffff;")
elif case == "missing-caption":
    text = text.replace("Approval binds the staged digest.", "")
elif case == "missing-foot":
    text = text.replace('class="foot"', 'class="not-foot"')
path.write_text(text)
PY
  if python3 "$validator" "$file" >"$test_root/$label.out" 2>&1; then
    printf 'expected %s to fail validation\n' "$label" >&2
    exit 1
  fi
  grep -Fq "$expected" "$test_root/$label.out"
}

expect_failure bad-meta 'metadata date must use YYYY-MM-DD'
expect_failure compact-date 'metadata date must use YYYY-MM-DD'
expect_failure unreplaced-meta 'metadata basis contains an unreplaced placeholder'
expect_failure bad-schema 'metadata schema must match artifact_type'
expect_failure missing-token 'missing house token --prat:#a78bfa'
expect_failure missing-caption 'every figure needs a non-empty figcaption'
expect_failure missing-foot 'missing non-empty .foot stamp'

printf 'html-deliverables tests: ok\n'
