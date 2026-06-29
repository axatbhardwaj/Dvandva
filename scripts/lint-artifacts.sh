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
  artifact_rel="${file#$ARTIFACT_DIR/}"

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

  artifact_type=""
  if [[ -n "$meta" ]]; then
    artifact_type="$(echo "$meta" | jq -r '.artifact_type // ""' 2>/dev/null)"
  fi

  if [[ "$artifact_type" == "run_explainer" ]]; then
    run_explainer_file_run_id=""
    if [[ "$artifact_rel" =~ ^run-reports/[0-9]{4}-[0-9]{2}-[0-9]{2}-([A-Za-z0-9._-]+)-explainer\.html$ ]]; then
      run_explainer_file_run_id="${BASH_REMATCH[1]}"
      pass "$rel run explainer path is canonical"
    else
      fail "$rel run explainer path must be run-reports/YYYY-MM-DD-<run_id>-explainer.html"
    fi

    if echo "$meta" | jq -e --arg run_id "$run_explainer_file_run_id" '
      .schema == "dvandva.artifact.run_explainer.v1" and
      .artifact_type == "run_explainer" and
      .run_id == $run_id and
      .baton_ref == (".dvandva/runs/" + $run_id + "/baton.json") and
      has("final_commit") and
      ((.sections | type) == "array") and
      ((["decisions", "development", "architecture", "verification", "diagrams"] - .sections) | length == 0)
    ' >/dev/null 2>&1; then
      pass "$rel run explainer metadata is complete"
    else
      fail "$rel run explainer metadata missing required fields or sections"
    fi

    for section in decisions development architecture verification diagrams; do
      if grep -Eiq "id=[\"']$section[\"']" "$file"; then
        pass "$rel includes #$section section"
      else
        fail "$rel missing #$section section"
      fi
    done

    if grep -Eiq '<svg([[:space:]>])' "$file"; then
      pass "$rel includes inline SVG diagram"
    else
      fail "$rel missing inline SVG diagram"
    fi
  fi

  if [[ "$artifact_type" == "pr_review" ]]; then
    if echo "$meta" | jq -e '
      .schema == "dvandva.artifact.pr_review.v1" and
      .artifact_type == "pr_review"
    ' >/dev/null 2>&1; then
      pass "$rel pr_review metadata schema and artifact_type match"
    else
      fail "$rel pr_review metadata schema must be dvandva.artifact.pr_review.v1 and artifact_type must be pr_review"
    fi

    for section in verdict severity findings ground-truth; do
      if grep -Eiq "id=[\"']${section}[\"']" "$file"; then
        pass "$rel includes #${section} section"
      else
        fail "$rel missing #${section} section"
      fi
    done
  fi

  if [[ "$artifact_type" == "bug_rca" ]]; then
    if echo "$meta" | jq -e '
      .schema == "dvandva.artifact.bug_rca.v1" and
      .artifact_type == "bug_rca"
    ' >/dev/null 2>&1; then
      pass "$rel bug_rca metadata schema and artifact_type match"
    else
      fail "$rel bug_rca metadata schema must be dvandva.artifact.bug_rca.v1 and artifact_type must be bug_rca"
    fi

    for section in symptom hypotheses root-cause fix-direction; do
      if grep -Eiq "id=[\"']${section}[\"']" "$file"; then
        pass "$rel includes #${section} section"
      else
        fail "$rel missing #${section} section"
      fi
    done
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

  if grep -Eiq '<script[^>]+src[[:space:]]*=[[:space:]]*["'\'']?\.\./' "$file" \
    || grep -Eiq '<link[^>]+href[[:space:]]*=[[:space:]]*["'\'']?\.\./' "$file" \
    || grep -Eiq '<(img|iframe|source|video|audio)[^>]+src[[:space:]]*=[[:space:]]*["'\'']?\.\./' "$file" \
    || grep -Eiq 'url\([[:space:]]*["'\'']?\.\./' "$file" \
    || grep -Eiq '@import[[:space:]]+(url\([[:space:]]*)?["'\'']?\.\./' "$file"; then
    fail "$rel contains path-traversal ref (../)"
  else
    pass "$rel has no path-traversal refs"
  fi
done

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
