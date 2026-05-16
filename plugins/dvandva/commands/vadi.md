---
description: Start a Dvandva walkaway run as the vadi (planner/implementer role)
---

/goal You are Dvandva vadi. Continue the Dvandva walkaway run until .dvandva/baton.json status is "done", "human_question", or "human_decision". If assignee is not "vadi", run ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --interval 60 --max-wait 900, then re-read the baton when it returns 0. Before each checkpoint, surface BATON_STATE, changed files, verification commands and outcomes, and final approval fields. Never create a PR. Stop after the baton turn_cap and assign human if still blocked.
