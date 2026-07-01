#!/usr/bin/env bash
# Lint generated human-facing Dvandva artifacts.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=0
TARGETS=("$@")
if [[ "${#TARGETS[@]}" -eq 0 ]]; then
  TARGETS=("$ROOT_DIR/superpowers")
fi

fail() {
  echo "FAIL: $1"
  failures=$((failures + 1))
}

pass() {
  echo "PASS: $1"
}

run_explainer_stem_matches_run_id() {
  local stem="$1"
  local run_id="$2"
  if [[ ! "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ || "$run_id" == *".."* ]]; then
    return 1
  fi

  if [[ "$run_id" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}- ]]; then
    [[ "$stem" == "$run_id" ]]
  elif [[ "$stem" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}-(.+)$ ]]; then
    [[ "${BASH_REMATCH[1]}" == "$run_id" ]]
  else
    return 1
  fi
}

path_abs() {
  local path="$1"
  if [[ -d "$path" ]]; then
    (cd "$path" && pwd -P)
  elif [[ -e "$path" ]]; then
    local dir base
    dir="$(dirname "$path")"
    base="$(basename "$path")"
    printf '%s/%s\n' "$(cd "$dir" && pwd -P)" "$base"
  else
    return 1
  fi
}

scope_label() {
  local joined="${TARGETS[*]}"
  printf '%s\n' "$joined"
}

artifact_rel_for_file() {
  local file="$1"
  local base="$2"
  if [[ "$file" == "$ROOT_DIR/superpowers/"* ]]; then
    printf '%s\n' "${file#$ROOT_DIR/superpowers/}"
  elif [[ -n "$base" && "$file" == "$base/"* ]]; then
    printf '%s\n' "${file#$base/}"
  else
    basename "$file"
  fi
}

markdown_files=()
html_files=()
html_bases=()
existing_targets=0

for target in "${TARGETS[@]}"; do
  if [[ ! -e "$target" ]]; then
    pass "no generated artifact directory present"
    continue
  fi

  existing_targets=$((existing_targets + 1))
  target_abs="$(path_abs "$target")"
  if [[ -d "$target_abs" ]]; then
    while IFS= read -r file; do
      markdown_files+=("$file")
    done < <(find "$target_abs" -type f -name '*.md' | sort)
    while IFS= read -r file; do
      html_files+=("$file")
      html_bases+=("$target_abs")
    done < <(find "$target_abs" -type f -name '*.html' | sort)
  elif [[ "$target_abs" == *.md ]]; then
    markdown_files+=("$target_abs")
  elif [[ "$target_abs" == *.html ]]; then
    html_files+=("$target_abs")
    html_bases+=("$(dirname "$target_abs")")
  fi
done

if [[ "$existing_targets" -eq 0 ]]; then
  exit 0
fi

if [[ "${#markdown_files[@]}" -gt 0 ]]; then
  fail "generated Markdown artifacts are not allowed under $(scope_label)"
  printf '  %s\n' "${markdown_files[@]#$ROOT_DIR/}"
else
  pass "no generated Markdown artifacts under $(scope_label)"
fi

if [[ "${#html_files[@]}" -eq 0 ]]; then
  fail "no generated HTML artifacts found under $(scope_label)"
fi

for index in "${!html_files[@]}"; do
  file="${html_files[$index]}"
  artifact_base="${html_bases[$index]}"
  rel="${file#$ROOT_DIR/}"
  artifact_rel="$(artifact_rel_for_file "$file" "$artifact_base")"

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
  artifact_schema=""
  if [[ -n "$meta" ]]; then
    artifact_type="$(echo "$meta" | jq -r '.artifact_type // ""' 2>/dev/null)"
    artifact_schema="$(echo "$meta" | jq -r '.schema // ""' 2>/dev/null)"
  fi

  if [[ "$artifact_schema" == "dvandva.artifact.pr_review.v1" && "$artifact_type" != "pr_review" ]]; then
    fail "$rel pr_review schema requires artifact_type pr_review"
  fi

  if [[ "$artifact_schema" == "dvandva.artifact.bug_rca.v1" && "$artifact_type" != "bug_rca" ]]; then
    fail "$rel bug_rca schema requires artifact_type bug_rca"
  fi

  if [[ "$artifact_schema" == "dvandva.artifact.run_explainer.v1" && "$artifact_type" != "run_explainer" ]]; then
    fail "$rel run_explainer schema requires artifact_type run_explainer"
  fi

  if grep -Eiq 'BATON_STATE([^_A-Z0-9]|$)' "$file" \
    && grep -Eiq '"(work_split|subagent_tracks|verification_matrix)"[[:space:]]*:' "$file"; then
    fail "$rel contains routine full BATON_STATE dynamic-array dump"
  else
    pass "$rel avoids routine full BATON_STATE dynamic-array dumps"
  fi

  if [[ "$artifact_type" == "run_explainer" ]]; then
    run_explainer_file_stem=""
    run_explainer_meta_run_id=""
    run_explainer_candidate_stem=""
    if [[ -n "$meta" ]]; then
      run_explainer_meta_run_id="$(echo "$meta" | jq -r 'if (.run_id | type) == "string" then .run_id else "" end' 2>/dev/null)"
    fi
    if [[ "$artifact_rel" =~ ^run-reports/([A-Za-z0-9._-]+)-explainer\.html$ ]]; then
      run_explainer_candidate_stem="${BASH_REMATCH[1]}"
    fi
    if [[ -n "$run_explainer_candidate_stem" ]] && run_explainer_stem_matches_run_id "$run_explainer_candidate_stem" "$run_explainer_meta_run_id"; then
      run_explainer_file_stem="$run_explainer_candidate_stem"
      pass "$rel run explainer path is canonical"
    else
      fail "$rel run explainer path must be run-reports/YYYY-MM-DD-<run_id>-explainer.html, or <run_id>-explainer.html when run_id is already date-prefixed"
    fi

    if [[ -n "$run_explainer_file_stem" ]] && echo "$meta" | jq -e --arg run_id "$run_explainer_meta_run_id" '
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

    if grep -Eiq '<table([[:space:]>])' "$file"; then
      pass "$rel includes PR review severity table"
    else
      fail "$rel missing PR review severity table"
    fi
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

    if grep -Eiq '<svg([[:space:]>])' "$file"; then
      pass "$rel includes bug RCA causal-chain SVG"
    else
      fail "$rel missing bug RCA causal-chain SVG"
    fi
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
