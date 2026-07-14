// Adversarial-loop workflow template — propose → stamp → attack → gate.
//
// Execute lanes author artifacts, one STAMP lane atomically binds all completed steps in
// goal.json, and only then do attack lanes read the stamped snapshot and write append-only
// evidence. Copy this file per goal, fill the STEPS list, and run it with the Workflow tool.
//
// Contract the review lanes MUST honor (the predicate checks it):
//   - cross-vendor: reviewer_family differs from the step's author_family
//   - cross-context: reviewer_agent_id differs from the step's author_agent_id
//   - evidence path: .adversarial-loop/evidence/<goal_id>/<step_id>/attempt-N.json
//   - created_at: `date -u +%Y-%m-%dT%H:%M:%SZ` (fixed-width UTC Zulu)
//   - artifact_digest: `sha256sum --` of the stamped artifact bytes

export const meta = {
  name: 'adversarial-loop',
  description: 'Propose → stamp → attack → gate: bind artifacts before independent review evidence',
  phases: [
    { title: 'Preflight', detail: 'Require one clean tree before concurrent execute-step mutation' },
    { title: 'Execute', detail: 'GPT/Codex authors each execute-step artifact' },
    { title: 'Stamp', detail: 'One writer atomically binds revisions and artifact digests in goal.json' },
    { title: 'Attack', detail: 'Claude/Opus attacks each stamped artifact and appends review evidence' },
  ],
}

// args = { goal_id, mode?, steps: [ { id, artifact_path, author_family, author_agent_id?, revision } ] }
const GOAL = args.goal_id
const MODE = args.mode ?? 'cross-vendor'
if (MODE !== 'cross-vendor') {
  throw new Error('adversarial-loop.template.js implements cross-vendor only; claude-only/codex-only need their own lane casting')
}
const STEPS = args.steps // execute steps; plan steps run before this workflow
const ID = /^[a-z0-9][a-z0-9_-]{0,63}$/

if (typeof GOAL !== 'string' || !ID.test(GOAL)) {
  throw new Error('workflow goal_id must match /^[a-z0-9][a-z0-9_-]{0,63}$/')
}
if (!Array.isArray(STEPS) || STEPS.length === 0) {
  throw new Error('workflow steps must be a non-empty array')
}
if (STEPS.some(s => !s || typeof s.id !== 'string' || !ID.test(s.id))) {
  throw new Error('each workflow step id must match /^[a-z0-9][a-z0-9_-]{0,63}$/')
}
const STEP_IDS = STEPS.map(s => s.id)
if (new Set(STEP_IDS).size !== STEP_IDS.length) {
  throw new Error('workflow step ids must be unique')
}
if (STEPS.some(s => !Number.isSafeInteger(s.revision) || s.revision < 1)) {
  throw new Error('each workflow step must have a positive integer revision')
}

const VERDICT = {
  type: 'object', required: ['step_id', 'verdict', 'evidence_path', 'findings'],
  properties: {
    step_id: { type: 'string' },
    verdict: { type: 'string', enum: ['pass', 'fail'] },
    evidence_path: { type: 'string', description: 'the append-only evidence file the reviewer wrote' },
    findings: { type: 'array', items: { type: 'string' } },
  },
}

const shellQuote = value => `'${String(value).replace(/'/g, `'"'"'`)}'`
const executeAgentId = s => s.author_agent_id || `execute:${GOAL}:${s.id}:r${s.revision}`
const reviewerAgentId = s => `attack:${GOAL}:${s.id}:r${s.revision}`

// GPT authors the artifact. STAMP, not this lane, is the sole goal.json writer.
const executeLane = s => `Thin Codex wrapper — EXECUTE lane. Your dispatched agent id is ${executeAgentId(s)}. Build step '${s.id}' at ${s.artifact_path} per the plan; do not edit .adversarial-loop/goal.json. Each step's write paths are declared by the plan; do not touch paths outside this step's scope.

Set REPO_ROOT to the repository root, MODEL=gpt-5.6-terra, EFFORT=xhigh, and SANDBOX=workspace-write. Before writing the self-contained brief at "$ATT/brief.md", allocate ATT as a new path such as "/tmp/codex-attempts/${GOAL}-${s.id}-r${s.revision}-a$N" and create it with 'mkdir -p "$ATT"'. Do this before every dispatch, retry, and resume; N must increment and no output path may ever be reused. The self-contained brief must enumerate these six parts verbatim: (1) goal+acceptance criteria; (2) exact paths; (3) decisions already made with the why; (4) boundaries incl. no-commit; (5) verification commands+expected results, and for new or changed behavior instructions to EITHER write and run a new failing test BEFORE the implementation OR, when an existing failing test already pins the behavior, run and report that existing failing test red BEFORE the implementation without authoring another, with BOTH the red (pre-fix) and green (post-fix) outputs included in the report — no red evidence, no acceptance; only demonstrably behavior-free work (pure renames with references updated, formatting, comments) is exempt, and prompts/config that steer agents are NOT exempt; (6) output contract writable under the sandbox. Capture the status baseline before dispatch; the sequential PREFLIGHT lane guarantees the clean starting tree required for parallel mutation.

  git -C "$REPO_ROOT" status --short > "$ATT/pre.status"
  if ! command -v codex > "$ATT/codex.path"; then
    printf 'FAIL: codex is not available on PATH\n' >&2
    exit 1
  fi
  if ! codex --version > "$ATT/codex.version" 2>&1; then
    printf 'FAIL: codex --version failed; see %s\n' "$ATT/codex.version" >&2
    exit 1
  fi
  timeout --kill-after=10 600 codex exec \\
    -C "$REPO_ROOT" \\
    -m "$MODEL" -s "$SANDBOX" \\
    -c "model_reasoning_effort=$EFFORT" \\
    --json -o "$ATT/last-message.md" \\
    "$(cat "$ATT/brief.md")" </dev/null \\
    > "$ATT/events.jsonl" 2> "$ATT/stderr.log"
  EXIT=$?
  printf 'EXIT:%s\\n' "$EXIT" > "$ATT/exit"

Keep stdout (the --json JSONL) and stderr in those separate files: never merge stderr into events.jsonl, because it corrupts the stream. Treat the attempt as complete only when EXIT is 0, a JSONL event in events.jsonl has parsed .type exactly equal to 'turn.completed' (for example, run jq -e 'select(.type == "turn.completed")' "$ATT/events.jsonl" >/dev/null), and last-message.md is non-empty; a nonzero exit, any parsed 'turn.failed' or error-typed event, a missing terminal event, or empty -o output is a failed attempt. Workflow lanes have no old-build fallback: a missing terminal event is a failure here by design, stricter than the delegating-to-codex skill's verified-old-build fallback. On a timeout, first confirm the process is dead. After a failed attempt, allow at most ONE exact-thread retry or resume, using a new ATT with new events.jsonl, stderr.log, and last-message.md (never resume --last); if that recovery fails, report failure. Rerun the plan's verification commands, verify the artifact on disk, and verify no changes appeared outside this step's declared write paths, ignoring sibling lanes' changes inside their declared write paths. The workflow guarantees a clean start and disjoint per-step write scopes. Return the artifact path, the red and green verification outputs (or the explicit behavior-free exemption claim), and a one-line summary. The normal execute budget is timeout --kill-after=10 600. Only when the plan explicitly marks this lane long-running, use timeout --kill-after=15 1500 in wrapper-owned background Bash and await its completion notification; backgrounding does not extend either timeout, and never use a sleep-poll loop.`

const stampIds = STEPS.map(s => shellQuote(s.id)).join(' ')
const stampRevisionCases = STEPS.map(s => `    ${shellQuote(s.id)}) REVISION=${s.revision} ;;`).join('\n')

// Exactly one lane updates goal.json. Its same-directory mv is the barrier before Attack starts.
const stampLane = () => `You are the sole STAMP writer. All execute lanes have finished. From the repository root, atomically stamp every listed step into .adversarial-loop/goal.json before returning. Before stamping, verify goal.json's goal_id equals the workflow goal '${GOAL}' and refuse to stamp on mismatch. Run this procedure and fail closed on any error:

  set -eu -o pipefail
  GOAL_FILE=.adversarial-loop/goal.json
  STATE_DIR=.adversarial-loop
  STEP_IDS=(${stampIds})
  WORK=$(mktemp "$STATE_DIR/.goal.stamp.XXXXXX")
  NEXT=''
  cleanup_stamp() { rm -f -- "$WORK"; if [ -n "$NEXT" ]; then rm -f -- "$NEXT"; fi; }
  trap cleanup_stamp EXIT HUP INT TERM
  cp -- "$GOAL_FILE" "$WORK"
  for STEP_ID in "\${STEP_IDS[@]}"; do
    case "$STEP_ID" in
${stampRevisionCases}
      *) printf 'unexpected stamp step: %s\\n' "$STEP_ID" >&2; exit 1 ;;
    esac
    jq -e --arg id "$STEP_ID" '[.steps[] | select(.id == $id)] | length == 1' "$WORK" >/dev/null
    ARTIFACT_PATH=$(jq -er --arg id "$STEP_ID" '.steps[] | select(.id == $id) | .artifact_path | select(type == "string" and length > 0)' "$WORK")
    case "$ARTIFACT_PATH" in
      /*) ARTIFACT_FILE=$ARTIFACT_PATH ;;
      *) ARTIFACT_FILE="$PWD/$ARTIFACT_PATH" ;;
    esac
    DIGEST_LINE=$(sha256sum -- "$ARTIFACT_FILE")
    DIGEST=\${DIGEST_LINE%%[[:space:]]*}
    case "$DIGEST" in
      *[!0-9a-f]*|'') printf 'invalid sha256 for step %s\\n' "$STEP_ID" >&2; exit 1 ;;
    esac
    [ "\${#DIGEST}" -eq 64 ]
    NEXT=$(mktemp "$STATE_DIR/.goal.stamp.next.XXXXXX")
    jq --arg id "$STEP_ID" --arg digest "$DIGEST" --argjson revision "$REVISION" '
      (.steps[] | select(.id == $id)) |=
        (.artifact_digest = $digest | .revision = $revision | .status = "complete")
    ' "$WORK" > "$NEXT"
    jq -e --arg id "$STEP_ID" --arg digest "$DIGEST" --argjson revision "$REVISION" '
      [.steps[] | select(.id == $id and .status == "complete" and .revision == $revision and .artifact_digest == $digest)] | length == 1
    ' "$NEXT" >/dev/null
    mv -- "$NEXT" "$WORK"
    NEXT=''
  done
  jq -e --argjson expected_count "\${#STEP_IDS[@]}" '[.steps[] | select(.status == "complete")] | length >= $expected_count' "$WORK" >/dev/null
  mv -- "$WORK" "$GOAL_FILE"
  WORK=''
  trap - EXIT HUP INT TERM

Return only after the atomic mv succeeds. The values written must be each artifact's real \`sha256sum --\` digest, the listed revision, and status "complete".`

// Claude attacks only the stamped bytes, then publishes one immutable evidence attempt.
const attackLane = s => {
  const reviewerId = reviewerAgentId(s)
  return `You are Opus, the adversarial ATTACK lane for step '${s.id}'. Your dispatched reviewer_agent_id is ${reviewerId}. The STAMP barrier has completed. Before reviewing, read .adversarial-loop/goal.json and require exactly one matching step whose revision is ${s.revision}, status is "complete", and artifact path is ${s.artifact_path}. Recompute the artifact with \`sha256sum --\` and refuse to review if it differs from the stamped artifact_digest. In cross-context mode, also require a non-empty author_agent_id different from ${reviewerId}.

Adversarially review the stamped artifact for correctness, security, edge cases, and spec deviations; run its tests if any. Set VERDICT to pass or fail, FINDINGS_JSON to an actual JSON array of finding strings, and TRANSCRIPT_REF to your notes path. Immediately before publication, rerun the stamped-snapshot checks and digest comparison so changed bytes cannot receive evidence.

Publish with this exact append-only procedure; do not use a heredoc, ls, wc, or an overwriting move:

  set -eu -o pipefail
  GOAL_FILE=.adversarial-loop/goal.json
  STEP_ID=${shellQuote(s.id)}
  EXPECTED_REVISION=${s.revision}
  REVIEWER_AGENT_ID=${shellQuote(reviewerId)}
  REVIEW_MODE=${shellQuote(MODE)}
  jq -e --arg id "$STEP_ID" '[.steps[] | select(.id == $id)] | length == 1' "$GOAL_FILE" >/dev/null
  STEP_JSON=$(jq -c --arg id "$STEP_ID" '.steps[] | select(.id == $id)' "$GOAL_FILE")
  STAMPED_STATUS=$(jq -r '.status' <<<"$STEP_JSON")
  STAMPED_REVISION=$(jq -r '.revision' <<<"$STEP_JSON")
  ARTIFACT_PATH=$(jq -er '.artifact_path | select(type == "string" and length > 0)' <<<"$STEP_JSON")
  STAMPED_DIGEST=$(jq -er '.artifact_digest | select(type == "string" and test("^[0-9a-f]{64}$"))' <<<"$STEP_JSON")
  AUTHOR_FAMILY=$(jq -er '.author_family | select(. == "claude" or . == "gpt")' <<<"$STEP_JSON")
  AUTHOR_AGENT_ID=$(jq -r 'if (.author_agent_id? | type) == "string" then .author_agent_id else "" end' <<<"$STEP_JSON")
  [ "$STAMPED_STATUS" = complete ] && [ "$STAMPED_REVISION" = "$EXPECTED_REVISION" ]
  if [ "$REVIEW_MODE" = cross-context ]; then
    [ -n "$AUTHOR_AGENT_ID" ] && [ "$REVIEWER_AGENT_ID" != "$AUTHOR_AGENT_ID" ]
  else
    [ "$AUTHOR_FAMILY" != claude ]
  fi
  case "$ARTIFACT_PATH" in
    /*) ARTIFACT_FILE=$ARTIFACT_PATH ;;
    *) ARTIFACT_FILE="$PWD/$ARTIFACT_PATH" ;;
  esac
  DIGEST_LINE=$(sha256sum -- "$ARTIFACT_FILE")
  CURRENT_DIGEST=\${DIGEST_LINE%%[[:space:]]*}
  [ "$CURRENT_DIGEST" = "$STAMPED_DIGEST" ]
  [ "$VERDICT" = pass ] || [ "$VERDICT" = fail ]
  jq -e 'type == "array" and all(.[]; type == "string")' <<<"$FINDINGS_JSON" >/dev/null
  TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  ATT=.adversarial-loop/evidence/${shellQuote(GOAL)}/${shellQuote(s.id)}
  mkdir -p -- "$ATT"
  LOCK="$ATT/.publish.lock"
  mkdir -- "$LOCK"
  TMP=''
  cleanup_evidence() { if [ -n "$TMP" ]; then rm -f -- "$TMP"; fi; rmdir -- "$LOCK" 2>/dev/null || true; }
  trap cleanup_evidence EXIT HUP INT TERM
  TMP=$(mktemp "$ATT/.evidence.XXXXXX")
  jq -n \
    --arg goal_id ${shellQuote(GOAL)} \
    --arg step_id "$STEP_ID" \
    --argjson step_revision "$EXPECTED_REVISION" \
    --arg artifact_digest "$STAMPED_DIGEST" \
    --arg reviewer_family claude \
    --arg reviewer_agent_id "$REVIEWER_AGENT_ID" \
    --arg reviewer_model opus \
    --arg verdict "$VERDICT" \
    --argjson findings "$FINDINGS_JSON" \
    --arg transcript_ref "$TRANSCRIPT_REF" \
    --arg created_at "$TS" \
    '{goal_id:$goal_id,step_id:$step_id,step_revision:$step_revision,artifact_digest:$artifact_digest,reviewer_family:$reviewer_family,reviewer_agent_id:$reviewer_agent_id,reviewer_model:$reviewer_model,verdict:$verdict,findings:$findings,transcript_ref:$transcript_ref,created_at:$created_at}' \
    > "$TMP"
  jq -e '
    type == "object" and
    (.goal_id | type == "string" and length > 0) and
    (.step_id | type == "string" and length > 0) and
    (.step_revision | type == "number" and floor == . and . >= 1) and
    (.artifact_digest | type == "string" and test("^[0-9a-f]{64}$")) and
    (.reviewer_family == "claude") and
    (.reviewer_agent_id | type == "string" and length > 0) and
    (.reviewer_model | type == "string") and
    (.verdict == "pass" or .verdict == "fail") and
    (.findings | type == "array") and
    (.transcript_ref | type == "string") and
    (.created_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
  ' "$TMP" >/dev/null
  N=1
  while :; do
    while [ -e "$ATT/attempt-$N.json" ] || [ -L "$ATT/attempt-$N.json" ]; do N=$((N + 1)); done
    TARGET="$ATT/attempt-$N.json"
    if ln -- "$TMP" "$TARGET"; then break; fi
    if [ -e "$TARGET" ] || [ -L "$TARGET" ]; then N=$((N + 1)); continue; fi
    printf 'failed to publish evidence at %s\\n' "$TARGET" >&2
    exit 1
  done
  rm -f -- "$TMP"
  TMP=''
  rmdir -- "$LOCK"
  trap - EXIT HUP INT TERM

Set verdict to pass only if the stamped artifact genuinely survives the attack. Return {step_id, verdict, evidence_path: TARGET, findings}.`
}

phase('Preflight')
const preflight = String(await agent(
  `PREFLIGHT clean-tree check. Set REPO_ROOT to the repository root and run git -C "$REPO_ROOT" status --short. If the output is non-empty, return the dirty paths and the single word DIRTY. If it is empty, return CLEAN.`,
  { label: 'preflight:clean-tree', phase: 'Preflight', model: 'sonnet', effort: 'low' },
)).trim()
if (preflight !== 'CLEAN') {
  throw new Error(`execute preflight requires a clean starting tree; dirty paths:\n${preflight}`)
}

phase('Execute')
const executions = await parallel(STEPS.map(s => () => agent(
  executeLane(s),
  { label: executeAgentId(s), phase: 'Execute', model: 'sonnet', effort: 'low' },
)))
if (executions.filter(Boolean).length !== STEPS.length) {
  throw new Error('execute barrier incomplete; refusing to stamp or attack')
}

phase('Stamp')
const stamped = await agent(stampLane(), { label: `stamp:${GOAL}`, phase: 'Stamp', model: 'sonnet', effort: 'low' })
if (!stamped) throw new Error('stamp barrier failed; refusing to dispatch attack lanes')

phase('Attack')
const results = await parallel(STEPS.map(s => () => agent(
  attackLane(s),
  { label: reviewerAgentId(s), phase: 'Attack', model: 'opus', effort: 'xhigh', schema: VERDICT },
)))

const verdicts = results.filter(Boolean)
const reviewedStepIds = new Set()
for (const verdict of verdicts) {
  if (!STEP_IDS.includes(verdict.step_id)) {
    throw new Error(`attack lane returned an unknown step_id: ${verdict.step_id}`)
  }
  if (reviewedStepIds.has(verdict.step_id)) {
    throw new Error(`attack lanes returned duplicate step_id: ${verdict.step_id}`)
  }
  reviewedStepIds.add(verdict.step_id)
}
const failed = verdicts.filter(v => v.verdict !== 'pass')
let missingAttacks = null
if (verdicts.length !== STEPS.length) {
  missingAttacks = STEP_IDS.filter(stepId => !reviewedStepIds.has(stepId))
  log(`attack barrier incomplete; missing attacks: ${missingAttacks.join(', ')}`)
} else {
  log(`${verdicts.length} stamped steps attacked; ${failed.length} failed → adjudicate, fix, bump revision, re-run those`)
}
return {
  goal_id: GOAL,
  verdicts,
  failed,
  ...(missingAttacks ? { missing_attacks: missingAttacks } : {}),
}
