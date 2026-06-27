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
{"schema":"dvandva.artifact.run_explainer.v1","artifact_type":"run_explainer","run_id":"run-a","baton_ref":".dvandva/runs/run-a/baton.json","final_commit":null,"sections":["decisions","development","architecture","verification","diagrams"]}
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

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
