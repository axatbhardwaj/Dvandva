#!/usr/bin/env bash
# Regression tests for generated-artifact lint policy.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTER="$ROOT_DIR/scripts/lint-artifacts.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

run_case() {
  local name="$1"
  local expected_exit="$2"
  shift 2

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name expected exit $expected_exit, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
  return 0
}

write_artifact() {
  local file="$1"
  local body="$2"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<HTML
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Artifact test</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
${body}
<script type="application/json" id="dvandva-artifact-meta">
{"schema":"dvandva.artifact.test.v1","artifact_type":"test"}
</script>
</body>
</html>
HTML
}

write_run_explainer() {
  local file="$1"
  local body="$2"
  local run_id="${3:-run-a}"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<HTML
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Run explainer</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
${body}
<script type="application/json" id="dvandva-artifact-meta">
{"schema":"dvandva.artifact.run_explainer.v1","artifact_type":"run_explainer","run_id":"$run_id","baton_ref":".dvandva/runs/$run_id/baton.json","final_commit":null,"sections":["decisions","development","architecture","verification","diagrams"]}
</script>
</body>
</html>
HTML
}

write_pr_review() {
  local file="$1"
  local body="$2"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<HTML
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>PR Review</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
${body}
<script type="application/json" id="dvandva-artifact-meta">
{"schema":"dvandva.artifact.pr_review.v1","artifact_type":"pr_review"}
</script>
</body>
</html>
HTML
}

write_bug_rca() {
  local file="$1"
  local body="$2"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<HTML
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Bug RCA</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
${body}
<script type="application/json" id="dvandva-artifact-meta">
{"schema":"dvandva.artifact.bug_rca.v1","artifact_type":"bug_rca"}
</script>
</body>
</html>
HTML
}

GOOD_PROSE="$TMP_DIR/good-prose"
write_artifact "$GOOD_PROSE/report.html" '<p>Source inventory: https://github.com/axatbhardwaj/Dvandva</p>'
run_case "prose source URL is allowed" 0 bash "$LINTER" "$GOOD_PROSE"

GOOD_LINK="$TMP_DIR/good-link"
write_artifact "$GOOD_LINK/report.html" '<p><a href="https://github.com/axatbhardwaj/Dvandva">source</a></p>'
run_case "navigation href URL is allowed" 0 bash "$LINTER" "$GOOD_LINK"

BAD_SCRIPT="$TMP_DIR/bad-script"
write_artifact "$BAD_SCRIPT/report.html" '<script src="https://cdn.example.com/app.js"></script>'
run_case "external script resource is rejected" 1 bash "$LINTER" "$BAD_SCRIPT"

BAD_IMG="$TMP_DIR/bad-img"
write_artifact "$BAD_IMG/report.html" '<img src="http://example.com/image.png" alt="external">'
run_case "external image resource is rejected" 1 bash "$LINTER" "$BAD_IMG"

BAD_IFRAME_SPACED="$TMP_DIR/bad-iframe-spaced"
write_artifact "$BAD_IFRAME_SPACED/report.html" '<iframe src = "https://example.com/embed"></iframe>'
run_case "external iframe resource with spaced attribute is rejected" 1 bash "$LINTER" "$BAD_IFRAME_SPACED"

BAD_CSS="$TMP_DIR/bad-css"
write_artifact "$BAD_CSS/report.html" '<style>.hero{background-image:url("https://example.com/bg.png")}@import url("http://example.com/theme.css");</style>'
run_case "external CSS resource is rejected" 1 bash "$LINTER" "$BAD_CSS"

BAD_MD="$TMP_DIR/bad-md"
mkdir -p "$BAD_MD/plans"
printf '# generated markdown artifact\n' > "$BAD_MD/plans/generated.md"
run_case "generated markdown artifact is rejected" 1 bash "$LINTER" "$BAD_MD"

BAD_FULL_BATON_STATE="$TMP_DIR/bad-full-baton-state"
write_artifact "$BAD_FULL_BATON_STATE/report.html" '
  <pre>BATON_STATE: {"work_split":[{"id":"too-much"}],"subagent_tracks":[],"verification_matrix":[]}</pre>'
run_case "routine full BATON_STATE dynamic arrays are rejected" 1 bash "$LINTER" "$BAD_FULL_BATON_STATE"

BAD_PARTIAL_FULL_BATON_STATE="$TMP_DIR/bad-partial-full-baton-state"
write_artifact "$BAD_PARTIAL_FULL_BATON_STATE/report.html" '
  <pre>BATON_STATE: {"work_split":[{"id":"too-much"}],"subagent_tracks":[]}</pre>'
run_case "routine full BATON_STATE with any dynamic array is rejected" 1 bash "$LINTER" "$BAD_PARTIAL_FULL_BATON_STATE"

GOOD_COMPACT_BATON_STATE="$TMP_DIR/good-compact-baton-state"
write_artifact "$GOOD_COMPACT_BATON_STATE/report.html" '
  <pre>BATON_STATE_COMPACT: {"counts":{"work_split":12,"subagent_tracks":8,"verification_matrix":5},"refs":{"plan_ref":"./superpowers/plans/x.html"}}</pre>'
run_case "compact BATON_STATE counts and refs are accepted" 0 bash "$LINTER" "$GOOD_COMPACT_BATON_STATE"

MISSING_ARTIFACT_DIR="$TMP_DIR/missing-artifacts"
run_case "missing artifact directory remains a no-op" 0 bash "$LINTER" "$MISSING_ARTIFACT_DIR"

DIRECT_BAD_META="$TMP_DIR/direct-bad-meta.html"
cat > "$DIRECT_BAD_META" <<'HTML'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Direct file artifact test</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
  <p>This direct file intentionally lacks Dvandva artifact metadata.</p>
</body>
</html>
HTML
run_case "direct HTML file target is linted" 1 bash "$LINTER" "$DIRECT_BAD_META"

MULTI_GOOD="$TMP_DIR/multi-good"
write_artifact "$MULTI_GOOD/report.html" '<p>Good first target.</p>'
MULTI_BAD="$TMP_DIR/multi-bad.html"
cat > "$MULTI_BAD" <<'HTML'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Second target artifact test</title>
  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>
</head>
<body>
  <p>This second target intentionally lacks Dvandva artifact metadata.</p>
</body>
</html>
HTML
run_case "multiple artifact targets are all linted" 1 bash "$LINTER" "$MULTI_GOOD" "$MULTI_BAD"

GOOD_RUN="$TMP_DIR/good-run"
write_run_explainer "$GOOD_RUN/run-reports/2026-06-28-run-a-explainer.html" '
  <main>
    <section id="decisions"><h2>Decisions</h2></section>
    <section id="development"><h2>Development</h2></section>
    <section id="architecture"><h2>Architecture</h2></section>
    <section id="verification"><h2>Verification</h2></section>
    <section id="diagrams"><h2>Diagrams</h2><svg viewBox="0 0 10 10"><path d="M1 1h8v8H1z"/></svg></section>
  </main>'
run_case "run explainer artifact is accepted" 0 bash "$LINTER" "$GOOD_RUN"

GOOD_RUN_DATE_PREFIXED="$TMP_DIR/good-run-date-prefixed"
write_run_explainer "$GOOD_RUN_DATE_PREFIXED/run-reports/2026-06-29-baton-accuracy-hook-coexist-explainer.html" '
  <main>
    <section id="decisions"><h2>Decisions</h2></section>
    <section id="development"><h2>Development</h2></section>
    <section id="architecture"><h2>Architecture</h2></section>
    <section id="verification"><h2>Verification</h2></section>
    <section id="diagrams"><h2>Diagrams</h2><svg viewBox="0 0 10 10"><path d="M1 1h8v8H1z"/></svg></section>
  </main>' \
  "2026-06-29-baton-accuracy-hook-coexist"
run_case "date-prefixed run explainer artifact is accepted without double date" 0 bash "$LINTER" "$GOOD_RUN_DATE_PREFIXED"

BAD_RUN_DATE_DOUBLED="$TMP_DIR/bad-run-date-doubled"
write_run_explainer "$BAD_RUN_DATE_DOUBLED/run-reports/2026-06-30-2026-06-29-baton-accuracy-hook-coexist-explainer.html" '
  <main>
    <section id="decisions"><h2>Decisions</h2></section>
    <section id="development"><h2>Development</h2></section>
    <section id="architecture"><h2>Architecture</h2></section>
    <section id="verification"><h2>Verification</h2></section>
    <section id="diagrams"><h2>Diagrams</h2><svg viewBox="0 0 10 10"><path d="M1 1h8v8H1z"/></svg></section>
  </main>' \
  "2026-06-29-baton-accuracy-hook-coexist"
run_case "date-prefixed run explainer artifact rejects double date" 1 bash "$LINTER" "$BAD_RUN_DATE_DOUBLED"

BAD_RUN_PATH="$TMP_DIR/bad-run-path"
write_run_explainer "$BAD_RUN_PATH/report.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
run_case "run explainer outside run-reports is rejected" 1 bash "$LINTER" "$BAD_RUN_PATH"

BAD_RUN_SECTION="$TMP_DIR/bad-run-section"
write_run_explainer "$BAD_RUN_SECTION/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagram"><svg viewBox="0 0 10 10"></svg></section>'
run_case "run explainer missing required section is rejected" 1 bash "$LINTER" "$BAD_RUN_SECTION"

BAD_RUN_VERIFICATION="$TMP_DIR/bad-run-verification"
write_run_explainer "$BAD_RUN_VERIFICATION/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
run_case "run explainer missing verification section is rejected" 1 bash "$LINTER" "$BAD_RUN_VERIFICATION"

BAD_RUN_SVG="$TMP_DIR/bad-run-svg"
write_run_explainer "$BAD_RUN_SVG/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"></section>'
run_case "run explainer missing inline SVG is rejected" 1 bash "$LINTER" "$BAD_RUN_SVG"

BAD_RUN_META="$TMP_DIR/bad-run-meta"
write_run_explainer "$BAD_RUN_META/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
sed -i 's/"sections":\["decisions","development","architecture","verification","diagrams"\]/"sections":["decisions","development","architecture"]/' "$BAD_RUN_META/run-reports/2026-06-28-run-a-explainer.html"
run_case "run explainer metadata missing sections is rejected" 1 bash "$LINTER" "$BAD_RUN_META"

BAD_RUN_META_ID="$TMP_DIR/bad-run-meta-id"
write_run_explainer "$BAD_RUN_META_ID/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
sed -i 's/"run_id":"run-a"/"run_id":"other-run"/' "$BAD_RUN_META_ID/run-reports/2026-06-28-run-a-explainer.html"
run_case "run explainer metadata run_id must match filename" 1 bash "$LINTER" "$BAD_RUN_META_ID"

BAD_RUN_TYPE="$TMP_DIR/bad-run-type"
write_run_explainer "$BAD_RUN_TYPE/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
sed -i 's/"artifact_type":"run_explainer"/"artifact_type":"test"/' "$BAD_RUN_TYPE/run-reports/2026-06-28-run-a-explainer.html"
run_case "run explainer reserved schema with wrong artifact_type is rejected" 1 bash "$LINTER" "$BAD_RUN_TYPE"

BAD_RUN_MISSING_TYPE="$TMP_DIR/bad-run-missing-type"
write_run_explainer "$BAD_RUN_MISSING_TYPE/run-reports/2026-06-28-run-a-explainer.html" '
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>'
sed -i 's/,"artifact_type":"run_explainer"//' "$BAD_RUN_MISSING_TYPE/run-reports/2026-06-28-run-a-explainer.html"
run_case "run explainer reserved schema missing artifact_type is rejected" 1 bash "$LINTER" "$BAD_RUN_MISSING_TYPE"

# --- pr_review cases ---

GOOD_PR="$TMP_DIR/good-pr"
write_pr_review "$GOOD_PR/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2><table><tr><th>Severity</th><th>Count</th></tr><tr><td>None</td><td>0</td></tr></table></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
run_case "valid pr_review artifact is accepted" 0 bash "$LINTER" "$GOOD_PR"

BAD_PR_SECTION="$TMP_DIR/bad-pr-section"
write_pr_review "$BAD_PR_SECTION/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
run_case "pr_review missing findings section is rejected" 1 bash "$LINTER" "$BAD_PR_SECTION"

BAD_PR_EXT="$TMP_DIR/bad-pr-ext"
write_pr_review "$BAD_PR_EXT/report.html" '
  <link href="https://cdn.example.com/style.css" rel="stylesheet">
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
run_case "pr_review external https resource is rejected" 1 bash "$LINTER" "$BAD_PR_EXT"

BAD_PR_TRAVERSAL="$TMP_DIR/bad-pr-traversal"
write_pr_review "$BAD_PR_TRAVERSAL/report.html" '
  <link href="../styles.css" rel="stylesheet">
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
run_case "pr_review path-traversal ref is rejected" 1 bash "$LINTER" "$BAD_PR_TRAVERSAL"

BAD_PR_SCHEMA="$TMP_DIR/bad-pr-schema"
write_pr_review "$BAD_PR_SCHEMA/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
sed -i 's/"schema":"dvandva.artifact.pr_review.v1"/"schema":"dvandva.artifact.bogus.v1"/' "$BAD_PR_SCHEMA/report.html"
run_case "pr_review with wrong schema is rejected" 1 bash "$LINTER" "$BAD_PR_SCHEMA"

BAD_PR_TYPE="$TMP_DIR/bad-pr-type"
write_pr_review "$BAD_PR_TYPE/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2><table><tr><td>None</td></tr></table></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
sed -i 's/"artifact_type":"pr_review"/"artifact_type":"test"/' "$BAD_PR_TYPE/report.html"
run_case "pr_review reserved schema with wrong artifact_type is rejected" 1 bash "$LINTER" "$BAD_PR_TYPE"

BAD_PR_MISSING_TYPE="$TMP_DIR/bad-pr-missing-type"
write_pr_review "$BAD_PR_MISSING_TYPE/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2><table><tr><td>None</td></tr></table></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
sed -i 's/,"artifact_type":"pr_review"//' "$BAD_PR_MISSING_TYPE/report.html"
run_case "pr_review reserved schema missing artifact_type is rejected" 1 bash "$LINTER" "$BAD_PR_MISSING_TYPE"

BAD_PR_STRUCTURE="$TMP_DIR/bad-pr-structure"
write_pr_review "$BAD_PR_STRUCTURE/report.html" '
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>'
run_case "pr_review missing severity table is rejected" 1 bash "$LINTER" "$BAD_PR_STRUCTURE"

# --- bug_rca cases ---

GOOD_RCA="$TMP_DIR/good-rca"
write_bug_rca "$GOOD_RCA/report.html" '
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2><svg viewBox="0 0 10 10"><path d="M1 5h8"/></svg></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>'
run_case "valid bug_rca artifact is accepted" 0 bash "$LINTER" "$GOOD_RCA"

BAD_RCA_SECTION="$TMP_DIR/bad-rca-section"
write_bug_rca "$BAD_RCA_SECTION/report.html" '
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>'
run_case "bug_rca missing root-cause section is rejected" 1 bash "$LINTER" "$BAD_RCA_SECTION"

BAD_RCA_TYPE="$TMP_DIR/bad-rca-type"
write_bug_rca "$BAD_RCA_TYPE/report.html" '
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2><svg viewBox="0 0 10 10"></svg></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>'
sed -i 's/"artifact_type":"bug_rca"/"artifact_type":"test"/' "$BAD_RCA_TYPE/report.html"
run_case "bug_rca reserved schema with wrong artifact_type is rejected" 1 bash "$LINTER" "$BAD_RCA_TYPE"

BAD_RCA_MISSING_TYPE="$TMP_DIR/bad-rca-missing-type"
write_bug_rca "$BAD_RCA_MISSING_TYPE/report.html" '
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2><svg viewBox="0 0 10 10"></svg></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>'
sed -i 's/,"artifact_type":"bug_rca"//' "$BAD_RCA_MISSING_TYPE/report.html"
run_case "bug_rca reserved schema missing artifact_type is rejected" 1 bash "$LINTER" "$BAD_RCA_MISSING_TYPE"

BAD_RCA_VISUAL="$TMP_DIR/bad-rca-visual"
write_bug_rca "$BAD_RCA_VISUAL/report.html" '
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>'
run_case "bug_rca missing causal-chain SVG is rejected" 1 bash "$LINTER" "$BAD_RCA_VISUAL"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
