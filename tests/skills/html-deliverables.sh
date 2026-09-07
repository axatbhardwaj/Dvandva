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
    "<!-- REQUIRED: direct conclusion in plain language -->": "One checkpoint has passed independent review",
    "<!-- REQUIRED: why this conclusion matters to the reader -->": "The approved bytes and review evidence describe one immutable delivery.",
    "<!-- REQUIRED: current status in a few words -->": "Ready for the next gate",
    "<!-- REQUIRED: what is true right now -->": "The reviewer approved the exact staged checkpoint.",
    "<!-- REQUIRED: next action and its owner -->": "The vadi records the final evidence.",
    "<!-- REQUIRED: the one sentence answer -->": "The exact delivery is approved and ready to advance.",
    "<!-- REQUIRED: What changed? / What did we learn? -->": "What changed?",
    "<!-- REQUIRED: the outcome, without setup or protocol jargon -->": "The complete delivery passed its independent review.",
    "<!-- REQUIRED: Why does it matter? -->": "Why it matters",
    "<!-- REQUIRED: the consequence for the human reader -->": "The result now has evidence from both roles.",
    "<!-- REQUIRED: What happens next? -->": "What happens next?",
    "<!-- REQUIRED: the next action, decision, or owner -->": "The vadi records the final evidence and advances the run.",
    "<!-- REQUIRED: what this work does and does not cover -->": "The review covers one complete delivery",
    "<!-- REQUIRED: canonical objective and deliverables in reader language -->": "The scope contains the requested implementation and its verification.",
    "<!-- REQUIRED: the conclusion supported by the evidence -->": "Review follows the immutable checkpoint",
    "<!-- REQUIRED: interpret the evidence before showing its structure -->": "The author stages exact bytes before the reviewer records a verdict.",
    "<!-- REQUIRED: concise description of the diagram -->": "Author to reviewer handoff",
    "<!-- REQUIRED: step or state label -->": "staged bytes",
    "<!-- REQUIRED: plain-language annotation -->": "exact checkpoint",
    "<!-- REQUIRED: the one insight the drawing cannot say -->": "Approval binds the staged digest.",
    "<!-- REQUIRED: specific label for the audit detail -->": "Exact manifest and verification",
    "<!-- REQUIRED: complete manifest, exact sources, hashes, commands, or matrix -->": "Deliverable implementation maps to checkpoint abc123; validation passed.",
    "<!-- REQUIRED: the most important finding or decision -->": "The checkpoint is approved without findings",
    "<!-- REQUIRED: findings, decisions, and their practical effect -->": "The reviewer found no blocking issue, so the delivery may advance.",
    "<!-- REQUIRED: current plan state -->": "Only the final evidence step remains",
    "<!-- REQUIRED: completed, active, and remaining work with clear owners -->": "Review is complete. The vadi now records the final evidence.",
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
  local unexpected="${3:-}"
  local file="$test_root/$label.html"
  cp "$valid" "$file"
  python3 - "$file" "$label" <<'PY'
import re
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
elif case == "missing-svg-label":
    text = text.replace('aria-label="Author to reviewer handoff"', 'aria-label=""')
elif case == "missing-foot":
    text = text.replace('class="foot"', 'class="not-foot"')
elif case == "unreplaced-content":
    text = text.replace("The complete delivery passed its independent review.", "<!-- REQUIRED: outcome -->")
elif case == "summary-not-first":
    text = text.replace('id="summary"', 'id="background"', 1)
elif case == "missing-reader-summary":
    text = text.replace(" data-reader-summary", "", 1)
elif case == "duplicate-reader-summary":
    text = text.replace('<section id="scope">',
                        '<section id="scope" data-reader-summary>', 1)
elif case == "missing-status":
    text = text.replace('class="status"', 'class="not-status"', 1)
elif case == "missing-next":
    text = text.replace('class="next"', 'class="not-next"', 1)
elif case == "missing-meaning":
    text = text.replace('data-summary="meaning"', 'data-summary="context"', 1)
elif case == "missing-outcome":
    text = text.replace('data-summary="outcome"', 'data-summary="context"', 1)
elif case == "missing-summary-next":
    text = text.replace('data-summary="next"', 'data-summary="context"', 1)
elif case == "missing-technical":
    text = text.replace('class="technical"', 'class="not-technical"', 1)
elif case == "empty-technical":
    text = re.sub(r'<details class="technical">.*?</details>',
                  '<details class="technical"></details>', text,
                  count=1, flags=re.S)
elif case == "missing-scope":
    text = re.sub(r'<section id="scope">.*?</section>', '', text,
                  count=1, flags=re.S)
elif case in {"missing-evidence", "missing-decisions", "missing-plan"}:
    section_id = case.removeprefix("missing-")
    text = re.sub(rf'<section id="{section_id}">.*?</section>', '', text,
                  count=1, flags=re.S)
elif case in {"empty-scope", "empty-evidence", "empty-decisions", "empty-plan"}:
    section_id = case.removeprefix("empty-")
    text = re.sub(rf'<section id="{section_id}">.*?</section>',
                  f'<section id="{section_id}"></section>', text,
                  count=1, flags=re.S)
elif case == "empty-heading":
    text = text.replace('<h1>One checkpoint has passed independent review</h1>',
                        '<h1></h1>', 1)
elif case == "empty-thesis":
    text = text.replace(
        '<p class="thesis">The approved bytes and review evidence describe one immutable delivery.</p>',
        '<p class="thesis"></p>', 1)
elif case == "empty-next-after-voids":
    text = re.sub(r'<p class="next">.*?</p>',
                  '<p class="next"><br><br><br></p>', text,
                  count=1, flags=re.S)
elif case == "status-after-summary":
    status = re.search(r'  <aside class="status".*?</aside>\n', text, re.S).group(0)
    text = text.replace(status, '', 1)
    first_section_end = text.index('</section>') + len('</section>')
    text = text[:first_section_end] + '\n' + status + text[first_section_end:]
elif case == "unclosed-next-child":
    text = re.sub(r'<p class="next">.*?</p>',
                  '<p class="next"><span></p>', text,
                  count=1, flags=re.S)
elif case == "unclosed-heading-child":
    text = re.sub(r'<h1>.*?</h1>', '<h1><span></h1>', text,
                  count=1, flags=re.S)
elif case == "unclosed-thesis-child":
    text = re.sub(r'<p class="thesis">.*?</p>',
                  '<p class="thesis"><span></p>', text,
                  count=1, flags=re.S)
elif case == "unclosed-technical-child":
    text = re.sub(r'<details class="technical">.*?</details>',
                  '<details class="technical"><span></details>', text,
                  count=1, flags=re.S)
elif case == "omitted-next-close":
    text = re.sub(r'<p class="next">.*?</p>',
                  '<p class="next"><span>', text,
                  count=1, flags=re.S)
elif case == "boilerplate-next":
    text = re.sub(r'<p class="next">.*?</p>',
                  '<p class="next"><b>Next:</b> </p>', text,
                  count=1, flags=re.S)
elif case == "summary-items-outside-summary":
    grid = re.search(r'  <div class="summary-grid">.*?</div>\n', text, re.S).group(0)
    text = text.replace(grid, '', 1)
    plan_end = text.rindex('</section>')
    text = text[:plan_end] + grid + text[plan_end:]
elif case == "heading-after-sections":
    heading = re.search(r'  <h1>.*?</h1>\n', text, re.S).group(0)
    text = text.replace(heading, '', 1)
    main_end = text.index('</main>')
    text = text[:main_end] + heading + text[main_end:]
elif case == "thesis-after-sections":
    thesis = re.search(r'  <p class="thesis">.*?</p>\n', text, re.S).group(0)
    text = text.replace(thesis, '', 1)
    main_end = text.index('</main>')
    text = text[:main_end] + thesis + text[main_end:]
elif case == "status-only-has-next":
    text = text.replace('<strong>Ready for the next gate</strong>', '<strong></strong>', 1)
    text = text.replace(
        '<p>The reviewer approved the exact staged checkpoint.</p>', '<p></p>', 1)
elif case == "technical-only-has-label":
    text = re.sub(r'<details class="technical">.*?</details>',
                  '<details class="technical"><summary>Full source manifest</summary></details>',
                  text, count=1, flags=re.S)
elif case == "duplicate-outcome":
    item = re.search(
        r'    <article class="summary-item" data-summary="outcome">.*?</article>\n',
        text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case in {"label-only-outcome", "label-only-meaning", "label-only-summary-next"}:
    kind = case.removeprefix("label-only-")
    if kind == "summary-next":
        kind = "next"
    pattern = rf'(<article class="summary-item" data-summary="{kind}">\s*<h3>.*?</h3>)\s*<p>.*?</p>'
    text = re.sub(pattern, r'\1\n      <p></p>', text, count=1, flags=re.S)
elif case in {"duplicate-meaning", "duplicate-summary-next"}:
    kind = case.removeprefix("duplicate-")
    if kind == "summary-next":
        kind = "next"
    item = re.search(
        rf'    <article class="summary-item" data-summary="{kind}">.*?</article>\n',
        text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case == "duplicate-next-action":
    item = re.search(r'    <p class="next">.*?</p>\n', text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case == "duplicate-heading":
    item = re.search(r'  <h1>.*?</h1>\n', text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case == "duplicate-thesis":
    item = re.search(r'  <p class="thesis">.*?</p>\n', text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case == "duplicate-status":
    item = re.search(r'  <aside class="status".*?</aside>\n', text, re.S).group(0)
    text = text.replace(item, item + item, 1)
elif case in {"duplicate-scope", "duplicate-evidence", "duplicate-decisions", "duplicate-plan"}:
    section_id = case.removeprefix("duplicate-")
    item = re.search(rf'<section id="{section_id}">.*?</section>\n', text, re.S).group(0)
    text = text.replace(item, item + item, 1)
path.write_text(text)
PY
  if python3 "$validator" "$file" >"$test_root/$label.out" 2>&1; then
    printf 'expected %s to fail validation\n' "$label" >&2
    exit 1
  fi
  grep -Fq "$expected" "$test_root/$label.out"
  if [[ -n "$unexpected" ]] && grep -Fq "$unexpected" "$test_root/$label.out"; then
    printf 'expected %s not to report %s\n' "$label" "$unexpected" >&2
    exit 1
  fi
}

expect_failure bad-meta 'metadata date must use YYYY-MM-DD'
expect_failure compact-date 'metadata date must use YYYY-MM-DD'
expect_failure unreplaced-meta 'metadata basis contains an unreplaced placeholder'
expect_failure bad-schema 'metadata schema must match artifact_type'
expect_failure missing-token 'missing house token --prat:#a78bfa'
expect_failure missing-caption 'every figure needs a non-empty figcaption'
expect_failure missing-svg-label 'every figure needs a plain-language SVG aria-label'
expect_failure missing-foot 'missing non-empty .foot stamp'
expect_failure unreplaced-content 'unreplaced REQUIRED content placeholder'
expect_failure summary-not-first 'the first section must be #summary'
expect_failure missing-reader-summary 'expected #summary to be the one data-reader-summary section'
expect_failure duplicate-reader-summary 'expected #summary to be the one data-reader-summary section'
expect_failure missing-status 'expected one non-empty current-status block'
expect_failure missing-next 'expected one non-empty next-action statement'
expect_failure missing-meaning 'expected one non-empty meaning summary item'
expect_failure missing-outcome 'expected one non-empty outcome summary item'
expect_failure missing-summary-next 'expected one non-empty next summary item'
expect_failure missing-technical 'expected at least one details.technical disclosure'
expect_failure empty-technical 'expected every details.technical disclosure to be non-empty'
expect_failure missing-scope 'expected one #scope section'
expect_failure missing-evidence 'expected one #evidence section'
expect_failure missing-decisions 'expected one #decisions section'
expect_failure missing-plan 'expected one #plan section'
expect_failure empty-scope 'expected #scope section to contain non-empty content'
expect_failure empty-evidence 'expected #evidence section to contain non-empty content'
expect_failure empty-decisions 'expected #decisions section to contain non-empty content'
expect_failure empty-plan 'expected #plan section to contain non-empty content'
expect_failure empty-heading 'expected one non-empty h1 conclusion'
expect_failure empty-thesis 'expected one non-empty thesis statement'
expect_failure empty-next-after-voids 'expected one non-empty next-action statement'
expect_failure status-after-summary 'current-status block must appear before the first section'
expect_failure unclosed-next-child 'expected one non-empty next-action statement'
expect_failure unclosed-heading-child 'expected one non-empty h1 conclusion'
expect_failure unclosed-thesis-child 'expected one non-empty thesis statement'
expect_failure unclosed-technical-child 'expected every details.technical disclosure to be non-empty'
expect_failure omitted-next-close 'expected one non-empty next-action statement'
expect_failure boilerplate-next 'expected one non-empty next-action statement'
expect_failure summary-items-outside-summary 'summary items must be inside #summary'
expect_failure heading-after-sections 'h1 conclusion must appear before the first section'
expect_failure thesis-after-sections 'thesis statement must appear before the first section'
expect_failure status-only-has-next 'current-status block needs its own text beyond the next-action line'
expect_failure technical-only-has-label 'expected every details.technical disclosure to be non-empty'
expect_failure label-only-outcome 'expected one non-empty outcome summary answer'
expect_failure label-only-meaning 'expected one non-empty meaning summary answer'
expect_failure label-only-summary-next 'expected one non-empty next summary answer'
expect_failure duplicate-outcome 'duplicate outcome summary items' 'summary items must be inside #summary'
expect_failure duplicate-meaning 'duplicate meaning summary items' 'summary items must be inside #summary'
expect_failure duplicate-summary-next 'duplicate next summary items' 'summary items must be inside #summary'
expect_failure duplicate-next-action 'duplicate next-action statements'
expect_failure duplicate-heading 'duplicate h1 conclusions'
expect_failure duplicate-thesis 'duplicate thesis statements'
expect_failure duplicate-status 'duplicate current-status blocks'
expect_failure duplicate-scope 'expected one #scope section'
expect_failure duplicate-evidence 'expected one #evidence section'
expect_failure duplicate-decisions 'expected one #decisions section'
expect_failure duplicate-plan 'expected one #plan section'

void_elements="$test_root/void-elements.html"
cp "$valid" "$void_elements"
python3 - "$void_elements" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text()
text = text.replace('Ready for the next gate', 'Ready<br>for the next gate', 1)
text = text.replace('The complete delivery passed its independent review.',
                    'The complete delivery<br>passed its independent review.', 1)
path.write_text(text)
PY
python3 "$validator" "$void_elements" | grep -Fq 'html-deliverable: valid'

printf 'html-deliverables tests: ok\n'
