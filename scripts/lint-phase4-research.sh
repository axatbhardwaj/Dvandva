#!/usr/bin/env bash
# Phase 4 contract lint for Dvandva research and subagent workflow text.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=0

require_text() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  if [[ ! -f "$file" ]]; then
    echo "FAIL: $label"
    echo "  missing file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
    return
  fi

  if grep -Fq -- "$pattern" "$file"; then
    echo "PASS: $label"
  else
    echo "FAIL: $label"
    echo "  missing pattern: $pattern"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
}

reject_text() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  if [[ ! -f "$file" ]]; then
    echo "PASS: $label"
    return
  fi

  if grep -Fq -- "$pattern" "$file"; then
    echo "FAIL: $label"
    echo "  rejected pattern: $pattern"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  else
    echo "PASS: $label"
  fi
}

require_agent_model() {
  local file="$1"
  local expected="$2"
  local label="$3"

  if [[ ! -f "$file" ]]; then
    echo "FAIL: $label"
    echo "  missing file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
    return
  fi

  local count
  count="$(grep -Ec '^model:' "$file" || true)"
  if [[ "$count" -ne 1 ]]; then
    echo "FAIL: $label"
    echo "  expected exactly one model: field, found $count"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
    return
  fi

  if grep -Fxq -- "model: $expected" "$file"; then
    echo "PASS: $label"
  else
    echo "FAIL: $label"
    echo "  expected model: $expected"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
}

require_output_contract_text() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  if [[ ! -f "$file" ]]; then
    echo "FAIL: $label"
    echo "  missing file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
    return
  fi

  if awk -v pattern="$pattern" '
    /^## Output Contract/ { in_contract = 1 }
    /^## Evidence Rules/ { in_contract = 0 }
    in_contract && index($0, pattern) { found = 1 }
    END { exit(found ? 0 : 1) }
  ' "$file"; then
    echo "PASS: $label"
  else
    echo "FAIL: $label"
    echo "  missing output-contract pattern: $pattern"
    echo "  file: ${file#$ROOT_DIR/}"
    failures=$((failures + 1))
  fi
}

research_skill="$ROOT_DIR/plugins/dvandva/skills/research/SKILL.md"
require_text "$research_skill" "name: research" "research skill has plugin-local name"
require_text "$research_skill" "description: Use when" "research skill has trigger-only description"
require_text "$research_skill" "original_ask" "research skill preserves original ask"
require_text "$research_skill" "research_ref" "research skill produces research_ref"
require_text "$research_skill" "run_explainer_reviews" "research skill preserves final explainer review records"
require_text "$research_skill" "./superpowers/research/YYYY-MM-DD-<topic>.html" "research skill writes generated HTML research artifact"
require_text "$research_skill" "work_split" "research skill records work split"
require_text "$research_skill" "verification_matrix" "research skill records verification matrix"
require_text "$research_skill" "100% test coverage" "research skill requires full coverage planning"
require_text "$research_skill" "test creation is separate from review" "research skill separates testing and review"
require_text "$research_skill" "deep_review" "research skill includes deep review loop"
require_text "$research_skill" "deslop" "research skill includes de-slop pass"
require_text "$research_skill" "parallel subagents" "research skill requires parallel subagents"
require_text "$research_skill" "research_review" "research skill documents prativadi review"
require_text "$research_skill" "research_revision" "research skill documents revision loop"
require_text "$research_skill" "generated user-facing HTML artifact" "research skill follows HTML artifact policy"
require_text "$research_skill" "dark self-contained HTML" "research skill requires dark HTML"
require_text "$research_skill" "machine-readable metadata" "research skill requires machine-readable metadata"
require_text "$research_skill" "If no subagent tool is available, do the same exploration directly and record the fallback in work_split." "research skill records subagent fallback"
require_text "$research_skill" "Do not rely solely on the vadi's research_ref" "research skill requires independent prativadi review"
reject_text "$research_skill" "./superpowers/research/YYYY-MM-DD-<topic>.md" "research skill rejects generated markdown research artifacts"

for file in \
  "$ROOT_DIR/README.md" \
  "$ROOT_DIR/product.md" \
  "$ROOT_DIR/docs/protocol/local-baton-channel.md" \
  "$ROOT_DIR/plugins/dvandva/references/local-baton-channel.md" \
  "$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md" \
  "$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md" \
  "$ROOT_DIR/plugins/dvandva/commands/vadi.md" \
  "$ROOT_DIR/plugins/dvandva/commands/prativadi.md"; do
  name="${file#$ROOT_DIR/}"
  require_text "$file" "Superpowers is a hard runtime dependency" "$name requires Superpowers at runtime"
  require_text "$file" "Dvandva owns baton state" "$name separates Dvandva coordination from Superpowers work discipline"
done

for role in vadi prativadi; do
  skill="$ROOT_DIR/plugins/dvandva/skills/$role/SKILL.md"
  require_text "$skill" "Invoke \`dvandva:research\`" "$role invokes shared research skill"
  require_text "$skill" "research_drafting" "$role handles research_drafting"
  require_text "$skill" "research_review" "$role handles research_review"
  require_text "$skill" "research_revision" "$role handles research_revision"
  require_text "$skill" "work_split" "$role surfaces work split"
  require_text "$skill" "verification_matrix" "$role surfaces verification matrix"
  require_text "$skill" "100% test coverage" "$role requires full coverage planning"
  require_text "$skill" "test_creation" "$role separates test creation"
  require_text "$skill" "deep_review" "$role includes deep review"
  require_text "$skill" "deslop" "$role includes de-slop pass"
  require_text "$skill" "parallel subagents" "$role requires parallel subagents"
  require_text "$skill" "canonical Dvandva subagent roster" "$role uses canonical subagent roster"
  require_text "$skill" "all phases are subagent-driven" "$role makes all phases subagent-driven"
  require_text "$skill" "independent research review" "$role supports independent research review"
  require_text "$skill" 'BATON_BROKEN_FILE="$BATON_DIR/baton.broken.json"' "$role defines broken-baton path"
  require_text "$skill" 'Write `$BATON_BROKEN_FILE` preserving' "$role uses broken-baton path"
  require_text "$skill" "write-helper validation exit 23" "$role disambiguates write exit 23"
  require_text "$skill" "wait-helper persist cap exit 23" "$role disambiguates wait exit 23"
  require_text "$skill" 'dvandva.baton.v1` or `dvandva.baton.v2' "$role accepts supported v1/v2 baton schemas"
  require_text "$skill" "Regular checkpoint commits" "$role documents regular checkpoint commits"
  require_text "$skill" "conditional parallelism" "$role documents conditional parallelism"
	  require_text "$skill" "parallelize only genuinely disjoint tracks" "$role avoids fake subagent parallelism"
	  require_text "$skill" "record what was not parallelized and why" "$role records non-parallelized work"
	  require_text "$skill" "two-team parallel implementation" "$role requires two-team implementation"
	  require_text "$skill" "cross-review" "$role requires cross-review"
	  require_text "$skill" "implementation-phase parallelism is mandatory" "$role requires mandatory implementation parallelism"
	  require_text "$skill" "Phase convention: implementation-chunk" "$role documents subagent track phase convention"
	  require_text "$skill" "same-status sync checkpoints" "$role documents team sync checkpoints"
	  require_text "$skill" "subagent_tracks" "$role records subagent tracks in baton evidence"
	  reject_text "$skill" "until the v2 write-helper phase lands" "$role does not reference dangling v2 phase"
	  reject_text "$skill" "equals \`dvandva.baton.v1\` in this helper version" "$role does not reject live v2 schema"
  reject_text "$skill" "Phase 6 includes v2 write-helper enforcement; until then" "$role does not describe v2 enforcement as future-only"
  reject_text "$skill" "once Phase 6 includes v2 write-helper enforcement" "$role does not describe v2 enforcement as future-only"
  reject_text "$skill" "21/22/23: fix the candidate file and re-run" "$role does not group exit 23 ambiguously"
  reject_text "$skill" "proceed even without superpowers" "$role does not allow Superpowers-free active work"
done

vadi_skill="$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md"
require_text "$vadi_skill" "BATON_BROKEN_FILE" "vadi defines broken-baton path symmetrically"
require_text "$vadi_skill" "Existing baton discovery" "vadi documents existing-baton discovery"
require_text "$vadi_skill" ".dvandva/runs/*/baton.json" "vadi scans named run batons"
require_text "$vadi_skill" "auto-create a new named run" "vadi auto-creates new run when only terminal batons exist"
require_text "$vadi_skill" "ask the user whether to continue" "vadi asks before reusing active batons"
reject_text "$vadi_skill" 'Write `$BATON_DIR/baton.broken.json`' "vadi avoids hardcoded broken-baton path"

for command in "$ROOT_DIR/plugins/dvandva/commands/vadi.md" "$ROOT_DIR/plugins/dvandva/commands/prativadi.md"; do
  name="${command#$ROOT_DIR/}"
  require_text "$command" "research_ref" "$name goal includes research_ref"
  require_text "$command" "work_split" "$name goal includes work_split"
  require_text "$command" "verification_matrix" "$name goal includes verification_matrix"
  require_text "$command" "test_creation" "$name goal separates test creation"
  require_text "$command" "deep_review" "$name goal includes deep review"
  require_text "$command" "deslop" "$name goal includes de-slop pass"
  require_text "$command" "parallel subagents" "$name goal includes subagent parallelism"
  require_text "$command" "conditional parallelism" "$name goal includes conditional parallelism"
  require_text "$command" "subagent_tracks" "$name goal records subagent tracks"
  require_text "$command" "Invoke \`dvandva:research\`" "$name goal invokes research skill"
  require_text "$command" "regular local checkpoint commits" "$name goal includes regular checkpoint commits"
done

for file in \
  "$ROOT_DIR/product.md" \
  "$ROOT_DIR/docs/protocol/local-baton-channel.md" \
  "$ROOT_DIR/plugins/dvandva/references/local-baton-channel.md" \
  "$ROOT_DIR/plugins/dvandva/references/state-transition-table.md"; do
  name="${file#$ROOT_DIR/}"
  require_text "$file" "work_split" "$name documents work split"
  require_text "$file" "verification_matrix" "$name documents verification matrix"
  require_text "$file" "100% test coverage" "$name documents full coverage target"
  require_text "$file" "test_creation" "$name documents separate test creation"
  require_text "$file" "deep_review" "$name documents deep review loop"
  require_text "$file" "deslop" "$name documents de-slop pass"
  require_text "$file" "Regular checkpoint commits" "$name documents regular checkpoint commits"
	  require_text "$file" "conditional parallelism" "$name documents conditional parallelism"
	  require_text "$file" "two-team parallel implementation" "$name documents two-team implementation"
	  require_text "$file" "cross-review" "$name documents cross-review"
	  require_text "$file" "implementation-phase parallelism is mandatory" "$name documents mandatory implementation parallelism"
	  require_text "$file" "Phase convention: implementation-chunk" "$name documents subagent track phase convention"
	  require_text "$file" "same-status sync checkpoints" "$name documents team sync checkpoints"
	  require_text "$file" "subagent_tracks" "$name documents subagent track evidence"
	  require_text "$file" "run_explainer_ref" "$name documents final run explainer"
	  require_text "$file" "run_explainer_reviews" "$name documents final run explainer reviews"
	  require_text "$file" "v2 write-helper enforcement" "$name documents v2 enforcement"
  require_text "$file" "wait-helper persist cap exit 23" "$name disambiguates wait exit 23"
  require_text "$file" "write-helper validation exit 23" "$name disambiguates write exit 23"
done

readme="$ROOT_DIR/README.md"
require_text "$readme" "regular local checkpoint commits" "README documents regular checkpoint commits"
require_text "$readme" 'dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup' "README documents all installed Dvandva skills"
require_text "$readme" "all six Dvandva skills" "README validation describes all six Dvandva skills"
reject_text "$readme" "all five Dvandva skills" "README avoids stale five-skill validation wording"
reject_text "$readme" "both Dvandva skills" "README avoids stale two-skill install wording"
reject_text "$readme" "Agents may commit and push only after both" "README no longer says commits are final-only"
for command in \
  "bash scripts/lint-protocol-phase1.sh" \
  "bash scripts/lint-skill-phase3.sh" \
  "bash scripts/lint-phase4-research.sh" \
  "bash scripts/lint-artifacts.sh" \
  "bash scripts/test-lint-artifacts.sh" \
  "bash scripts/test-lint-skills.sh" \
  "bash scripts/test-dvandva-wait.sh" \
  "bash scripts/test-dvandva-write.sh" \
  "bash scripts/test-dvandva-snapshot.sh" \
  "bash scripts/test-install.sh" \
  "bash scripts/test-install-codex.sh" \
  "bash scripts/smoke-plugin-install.sh" \
  "claude plugin validate plugins/dvandva" \
  "claude plugin validate ."; do
  require_text "$readme" "$command" "README full validation includes $command"
done

schema="$ROOT_DIR/plugins/dvandva/references/baton-schema-v2.json"
require_text "$schema" '"work_split"' "v2 schema includes work_split"
require_text "$schema" '"verification_matrix"' "v2 schema includes verification_matrix"
require_text "$schema" '"run_explainer_ref"' "v2 schema includes final explainer ref"
require_text "$schema" '"run_explainer_reviews"' "v2 schema includes final explainer reviews"
require_text "$schema" '"active_roles"' "v2 schema includes active roles"
require_text "$schema" '"parallel_implementing"' "v2 schema includes parallel implementation status"
require_text "$schema" '"test_creation"' "v2 schema includes test creation status"
require_text "$schema" '"cross_review"' "v2 schema includes cross-review status"
require_text "$schema" '"cross_fixing"' "v2 schema includes cross-fixing status"
require_text "$schema" '"deep_review"' "v2 schema includes deep review status"
require_text "$schema" '"deslop"' "v2 schema includes de-slop status"
reject_text "$schema" '"id": "deep_review-security"' "v2 seed does not make security auditor mandatory"
reject_text "$schema" '"id": "deep_review-integration"' "v2 seed does not make integration checker mandatory"
reject_text "$schema" '"id": "deep_review-doc-verification"' "v2 seed does not make doc verifier mandatory"
reject_text "$schema" '"id": "phase_fixing-debug"' "v2 seed does not make debugger mandatory"
reject_text "$schema" '"id": "research-pattern-mapping"' "v2 seed does not make pattern mapper mandatory"

agent_dir="$ROOT_DIR/plugins/dvandva/agents"
for agent in researcher architect implementer test-creator cross-reviewer adversarial-analyst deep-reviewer deslopper sandbox-verifier baton-auditor security-auditor integration-checker debugger doc-verifier pattern-mapper; do
  file="$agent_dir/$agent.md"
  require_text "$file" "name: dvandva-$agent" "agent $agent has Dvandva name"
  require_text "$file" "description: Use" "agent $agent has trigger-focused description"
  reject_text "$file" "model: haiku" "agent $agent rejects haiku model class"
  require_text "$file" "phase:" "agent $agent declares phase ownership"
  require_text "$file" "tools:" "agent $agent declares explicit tool scope"
  require_text "$file" "## Mission" "agent $agent declares a mission"
  require_text "$file" "## Use When" "agent $agent declares triggers"
  require_text "$file" "## Required Inputs" "agent $agent declares required inputs"
  require_text "$file" "## Operating Loop" "agent $agent declares operating loop"
  require_text "$file" "## Output Contract" "agent $agent declares output contract"
  require_text "$file" "## Evidence Rules" "agent $agent declares evidence rules"
  require_text "$file" "## Guardrails" "agent $agent declares guardrails"
  require_text "$file" "## Common Failures" "agent $agent declares common failures"
  require_text "$file" "work_split" "agent $agent reports work_split"
  require_text "$file" "verification_matrix" "agent $agent reports verification_matrix"
  require_text "$file" "subagent_tracks" "agent $agent reports subagent track evidence"
  reject_text "$file" "not an orchestrator" "agent $agent avoids old no-orchestrator framing"
done

for agent in security-auditor integration-checker debugger doc-verifier pattern-mapper; do
  file="$agent_dir/$agent.md"
  require_output_contract_text "$file" "id:" "new agent $agent outputs schema-valid track id"
  require_output_contract_text "$file" "phase:" "new agent $agent outputs schema-valid track phase"
  require_output_contract_text "$file" "status: completed|blocked" "new agent $agent outputs schema-valid track status"
  require_output_contract_text "$file" "track:" "new agent $agent outputs schema-valid track name"
  require_output_contract_text "$file" "owner: dvandva-$agent" "new agent $agent outputs schema-valid track owner"
  require_output_contract_text "$file" "parallelized:" "new agent $agent outputs schema-valid parallelized flag"
  require_output_contract_text "$file" "rationale:" "new agent $agent outputs schema-valid rationale"
  require_output_contract_text "$file" "inputs:" "new agent $agent outputs schema-valid inputs"
  require_output_contract_text "$file" "outputs:" "new agent $agent outputs schema-valid outputs"
  require_output_contract_text "$file" "evidence_refs:" "new agent $agent outputs schema-valid evidence refs"
  require_output_contract_text "$file" "result: approved|findings|blocked" "new agent $agent outputs schema-valid result"
done

for agent in adversarial-analyst architect baton-auditor deep-reviewer doc-verifier integration-checker security-auditor; do
  require_agent_model "$agent_dir/$agent.md" "opus" "agent $agent uses opus-class model for planning/reviewing/architecture"
done

for agent in cross-reviewer debugger deslopper implementer pattern-mapper researcher sandbox-verifier test-creator; do
  require_agent_model "$agent_dir/$agent.md" "sonnet" "agent $agent uses sonnet-class model for development/implementation/documentation"
done

for agent in researcher architect implementer test-creator deslopper pattern-mapper; do
  require_text "$agent_dir/$agent.md" "## Downstream Consumer" "agent $agent names downstream consumer"
done

for agent in cross-reviewer adversarial-analyst deep-reviewer sandbox-verifier baton-auditor security-auditor integration-checker doc-verifier; do
  require_text "$agent_dir/$agent.md" "## Adversarial Stance" "agent $agent declares adversarial stance"
  require_text "$agent_dir/$agent.md" "If you cannot verify a claim" "agent $agent uses correct proof standard"
  reject_text "$agent_dir/$agent.md" "If you cannot disprove a claim" "agent $agent avoids inverted proof standard"
done

require_text "$agent_dir/researcher.md" "tools: Read, Glob, Grep, WebFetch" "researcher stays read-only plus WebFetch"
require_text "$agent_dir/architect.md" "tools: Read, Glob, Grep" "architect stays read-only"
require_text "$agent_dir/architect.md" "must_not_do:" "architect work split carries must-not-do boundary"
require_text "$agent_dir/implementer.md" "phase: parallel_implementing" "implementer maps to parallel implementation"
require_text "$agent_dir/cross-reviewer.md" "phase: cross_review" "cross reviewer maps to cross_review"
require_text "$agent_dir/adversarial-analyst.md" "phase: deep_review" "adversarial analyst maps to deep_review"
require_text "$agent_dir/deep-reviewer.md" "tools: Read, Glob, Grep, Bash" "deep reviewer can verify without editing"
require_text "$agent_dir/adversarial-analyst.md" "tools: Read, Glob, Grep, Bash" "adversarial analyst can inspect and run probes without editing"
require_text "$agent_dir/baton-auditor.md" "tools: Read, Glob, Grep, Bash" "baton auditor can inspect without editing"
require_text "$agent_dir/sandbox-verifier.md" "tools: Read, Glob, Grep, Bash" "sandbox verifier can run probes without editing"
require_text "$agent_dir/implementer.md" "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write" "implementer declares edit tools"
require_text "$agent_dir/test-creator.md" "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write" "test creator declares edit tools"
require_text "$agent_dir/deslopper.md" "tools: Read, Glob, Grep, Bash, Edit, MultiEdit, Write" "deslopper declares edit tools"
require_text "$agent_dir/cross-reviewer.md" "tools: Read, Glob, Grep, Bash" "cross reviewer can verify without editing"
require_text "$agent_dir/architect.md" "two-team parallel implementation" "architect plans two-team implementation"
require_text "$agent_dir/architect.md" "implementation-phase parallelism is mandatory" "architect enforces mandatory implementation parallelism"
require_text "$agent_dir/architect.md" "cross-review" "architect plans cross-review"
require_text "$agent_dir/adversarial-analyst.md" "Attack Hypothesis" "adversarial analyst emits attack hypotheses"
require_text "$agent_dir/deep-reviewer.md" "at least three angle-specific reviewers" "deep reviewer requires multi-angle review"
require_text "$agent_dir/baton-auditor.md" "active_roles" "baton auditor checks active_roles"

require_text "$agent_dir/security-auditor.md" "tools: Read, Glob, Grep, Bash" "security auditor can verify without editing"
require_text "$agent_dir/security-auditor.md" "phase: deep_review" "security auditor maps to deep_review"
require_text "$agent_dir/security-auditor.md" "threat_category" "security auditor classifies by threat category"
require_text "$agent_dir/integration-checker.md" "tools: Read, Glob, Grep, Bash" "integration checker can verify without editing"
require_text "$agent_dir/integration-checker.md" "phase: deep_review" "integration checker maps to deep_review"
require_text "$agent_dir/integration-checker.md" "chunk_boundaries_reviewed" "integration checker reviews chunk boundaries"
require_text "$agent_dir/debugger.md" "tools: Read, Glob, Grep, Bash" "debugger can inspect without editing"
require_text "$agent_dir/debugger.md" "phase: phase_fixing" "debugger maps to phase_fixing"
require_text "$agent_dir/debugger.md" "root_cause_confirmed" "debugger confirms root cause"

require_text "$ROOT_DIR/product.md" "GSD-style fresh-context subagents" "product cites GSD-style subagent pattern"
require_text "$ROOT_DIR/product.md" "OMO-style team roles" "product cites OMO-style team role pattern"
require_text "$ROOT_DIR/product.md" "canonical Dvandva subagent roster" "product declares canonical Dvandva agent roster"
require_text "$ROOT_DIR/product.md" "dvandva-adversarial-analyst" "product includes adversarial analyst"
for agent in security-auditor integration-checker debugger doc-verifier pattern-mapper; do
  require_text "$ROOT_DIR/product.md" "dvandva-$agent" "product includes $agent"
  require_text "$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md" "dvandva-$agent" "vadi skill includes $agent"
  require_text "$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md" "dvandva-$agent" "prativadi skill includes $agent"
  require_text "$research_skill" "dvandva-$agent" "research skill includes $agent"
done
for file in \
  "$ROOT_DIR/README.md" \
  "$ROOT_DIR/product.md" \
  "$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md" \
  "$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md" \
  "$research_skill"; do
  name="${file#$ROOT_DIR/}"
  require_text "$file" "Dvandva model classes are vendor-neutral" "$name documents vendor-neutral model classes"
  require_text "$file" "Claude Code maps \`opus\` to Opus-class and \`sonnet\` to Sonnet-class models" "$name documents Claude model-class mapping"
  require_text "$file" "Codex maps \`opus\` to \`gpt-5.5\` and \`sonnet\` to \`gpt-5.4\`" "$name documents Codex model-class mapping"
  require_text "$file" "\`xhigh\` reasoning effort" "$name documents Codex xhigh reasoning effort"
  require_text "$file" "Do not use \`haiku\` for Dvandva subagents" "$name forbids haiku-class Dvandva subagents"
done
for file in \
  "$ROOT_DIR/plugins/dvandva/commands/vadi.md" \
  "$ROOT_DIR/plugins/dvandva/commands/prativadi.md"; do
  name="${file#$ROOT_DIR/}"
  require_text "$file" "Claude Code maps \`opus\` to Opus-class and \`sonnet\` to Sonnet-class models" "$name documents Claude model-class mapping"
  require_text "$file" "Codex maps \`opus\` to \`gpt-5.5\` and \`sonnet\` to \`gpt-5.4\`" "$name documents Codex model-class mapping"
  require_text "$file" "\`xhigh\` reasoning effort" "$name documents Codex xhigh reasoning effort"
  require_text "$file" "never use \`haiku\`" "$name forbids haiku-class Dvandva subagents"
done
for file in \
  "$ROOT_DIR/plugins/dvandva/references/local-baton-channel.md" \
  "$ROOT_DIR/docs/protocol/local-baton-channel.md"; do
  name="${file#$ROOT_DIR/}"
  require_text "$file" "Codex runs both classes at \`xhigh\` reasoning effort" "$name documents Codex xhigh reasoning effort"
  require_text "$file" "routine cross-review" "$name documents routine cross-review as sonnet-class work"
done
require_text "$ROOT_DIR/plugins/dvandva/references/state-transition-table.md" "gpt-5.5 (xhigh)" "state-transition table documents opus-class xhigh"
require_text "$ROOT_DIR/plugins/dvandva/references/state-transition-table.md" "gpt-5.4 (xhigh)" "state-transition table documents sonnet-class xhigh"
require_text "$ROOT_DIR/plugins/dvandva/references/state-transition-table.md" "routine cross-review" "state-transition table documents routine cross-review as sonnet-class work"
require_text "$ROOT_DIR/product.md" "adversarial-analyst.md" "product layout includes adversarial analyst agent file"
require_text "$ROOT_DIR/product.md" "at least three angle-specific reviewers" "product requires multi-angle deep review"
require_text "$ROOT_DIR/product.md" "one-date explainer under \`./superpowers/run-reports/\`" "product documents final explainer path"
require_text "$ROOT_DIR/product.md" "never add a second date prefix" "product documents date-prefixed run_id explainer convention"
require_text "$ROOT_DIR/product.md" 'direct Codex plugin install, dual installer install, and `install-codex.sh` helper install' "product documents all smoke install runtime probes"
require_text "$ROOT_DIR/product.md" 'dvandva:vadi`, `dvandva:prativadi`, `dvandva:research`, `dvandva:testing`, `dvandva:understanding`, and `dvandva:worktree-setup' "product documents all smoke-verified Dvandva skills"
require_text "$ROOT_DIR/scripts/smoke-plugin-install.sh" "dvandva:research" "smoke script requires research skill runtime surface"
reject_text "$ROOT_DIR/product.md" 'then write baton with `status: deep_review, assignee: prativadi' "product avoids stale direct test_creation-to-deep_review mode wording"
reject_text "$ROOT_DIR/product.md" '| `test_creation` | `deep_review, review_target: implementation`' "product avoids stale direct test_creation-to-deep_review transition row"
require_text "$research_skill" "canonical Dvandva subagent roster" "research skill declares canonical Dvandva agent roster"
require_text "$research_skill" "dvandva-adversarial-analyst" "research skill includes adversarial analyst"
require_text "$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md" "Add \`dvandva-adversarial-analyst\` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses" "prativadi deep review invokes adversarial analyst"

for absorbed in testing understanding worktree-setup; do
  file="$ROOT_DIR/plugins/dvandva/skills/$absorbed/SKILL.md"
  require_text "$file" "name: $absorbed" "absorbed skill $absorbed has plugin-local name"
  require_text "$file" "Dvandva" "absorbed skill $absorbed is rewritten for Dvandva"
  require_text "$file" "BATON_STATE" "absorbed skill $absorbed surfaces baton state"
done

require_text "$ROOT_DIR/plugins/dvandva/skills/testing/SKILL.md" "100% test coverage" "testing skill requires full coverage"
require_text "$ROOT_DIR/plugins/dvandva/skills/testing/SKILL.md" "test_creation" "testing skill maps to test_creation"
require_text "$ROOT_DIR/plugins/dvandva/skills/testing/SKILL.md" "verification_matrix" "testing skill updates verification matrix"
require_text "$ROOT_DIR/plugins/dvandva/skills/understanding/SKILL.md" "./superpowers/understanding/YYYY-MM-DD-<topic>.html" "understanding skill writes HTML checklist"
require_text "$ROOT_DIR/plugins/dvandva/skills/worktree-setup/SKILL.md" "BRANCH-NOTES.md" "worktree skill preserves branch notes"
require_text "$ROOT_DIR/plugins/dvandva/skills/worktree-setup/SKILL.md" "~/ACTIVE-WORK.md" "worktree skill updates active work"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
