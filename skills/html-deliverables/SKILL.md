---
name: html-deliverables
description: Use when creating or revising a human-facing Dvandva HTML report, explainer, audit, review, or status page. Do not use for arbitrary websites or web applications.
---

# Dvandva HTML deliverables

Write for an interested human who has not been following the run. The page
succeeds when that reader can answer these questions in about 30 seconds:

1. What is the answer or outcome?
2. Why does it matter?
3. What happens next, and who owns it?

Use an **answer-first** reading order. Put the conclusion, current status, and
next action in the first viewport. Follow with the reasoning and evidence.
Place hashes, source manifests, exhaustive matrices, commands, and other audit
material in labelled `<details class="technical">` blocks unless the reader
needs them to understand the conclusion. Keep the canonical scope, complete
manifest, findings and decisions, and current plan/TODO in the page; progressive
disclosure changes their prominence, not their completeness.

Use plain, specific language. Define unavoidable Dvandva terms on first use,
name roles only when ownership matters, keep paragraphs focused on one idea,
and make headings state conclusions rather than merely naming topics. Remove
repeated status, ornamental prose, and any detail that does not help the reader
understand, decide, verify, or act. A diagram must make one relationship easier
to grasp; explain its takeaway in prose and do not use it as decoration.

Start from `template.html` in this directory and keep it a complete standalone
HTML document. Preserve its `:root` token values, dark color scheme, typography,
figure overflow, answer-first markers, technical-detail disclosure, and
reduced-motion rule. Map the subject's opposing actors or states to `--vadi` and
`--prat`; reserve `--seal`, `--stop`, and `--human` for their stated meanings.

Fill the metadata with a schema shaped as
`dvandva.artifact.<artifact_type>.v1`, a matching `artifact_type`, title, ISO
date, and exact source/checkpoint basis. Replace every placeholder. Each section
opens with an eyebrow and a thesis-style `h2`. Draw structural ideas as inline,
labelled SVG figures with captions; use prose to interpret them. End with a
`.foot` stamp naming what the page reflects and its as-of checkpoint/version.

Run `python3 <this-skill-directory>/scripts/validate.py <artifact.html>`, then
render and inspect the complete page at desktop and mobile widths. Check text,
contrast, clipped content, horizontal page overflow, figure-local scrolling,
and reduced-motion behavior. Read the rendered first viewport as the intended
human: the answer, importance, status, and next action must be clear without
opening technical details. Static validation does not replace this comprehension
check or the rendered inspection.

## Active v4 runs

For `workflow=discovery`, follow the role-local Discovery drivers: Claude
Fable 5.1/high authors as vadi and Codex Astra/high reviews as prativadi,
including exact source and desktop/mobile rendering. Use those existing
sessions; the Sol/medium and Opus stations below apply to other workflows.
In the staging/publication steps below, "Opus review" means the selected
prativadi's review for discovery. Parent-only facade mutations, validation,
immutable bytes and Codex publication still apply.

When `stage_explainer` is legal, including the pre-work `run_started`
obligation, use native `gpt-5.6-sol` at `medium` reasoning to author the page. If the parent uses a different
reasoning level, delegate the bounded HTML task to native Sol/medium; a prompt
persona is not a reasoning-level switch.
Astra and Fable may advise on planning when useful. Opus reviews both the exact
staged source and its desktop/mobile rendering. These are model stations inside
the existing vadi and prativadi sessions, never a third Baton role.

The role parent alone calls the Dvandva facade. After validation and rendering,
it stages the complete HTML as exact bytes. Apply requested changes, restage,
and repeat Opus review until approved. When Codex participates, Codex publishes
the exact approved digest through Sites and records the resulting receipt via
the facade. Preserve approved bytes and retry from fresh state if publication
is unavailable.

Use this active validator instead of the retired v3 `dvandva lint artifacts`.
Never strip the document skeleton for a Claude Artifact wrapper.
