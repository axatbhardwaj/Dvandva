---
name: html-deliverables
description: Use when creating any HTML file for a human to read — a report, explainer, spec, plan, review, audit, research write-up, or status page. Applies to every HTML deliverable regardless of project or topic.
---

# HTML Deliverables — the house format

One visual identity for every HTML file produced for the human: dark ground, a
role-colored duel palette, mono for machine words, diagram-led sections. Copy
`template.html` (next to this file) and fill it — do not restyle from scratch.

## The contract

Every page IS, in order:

1. `<title>` + the `dvandva-artifact-meta` JSON script block (schema, type, title, date, basis).
2. The `:root` token block from the template, **verbatim** — including the
   literal `color-scheme: dark;` declaration.
3. Sections, each opened by an `.eyebrow` mono label + an `h2` thesis.
4. At least one `figure` per structural idea: hand-authored SVG + `figcaption`.
   Anything you would sketch on a whiteboard is drawn, not listed.
5. Prose in system sans at ≤68ch; every state name, command, field, and exit
   code in mono (`code` or `.st`).
6. A `.foot` stamp line: what the page reflects and as-of what version/commit.

## Tokens (never improvise replacements)

| Token | Hex | Role |
|---|---|---|
| `--ground` | `#0b0f14` | page (blue-biased near-black) |
| `--panel` / `--panel2` | `#121821` / `#182130` | cards, figures |
| `--line` | `#26303e` | borders |
| `--ink` / `--dim` / `--faint` | `#dce4ee` / `#8a97a8` / `#5c6774` | text tiers |
| `--vadi` | `#34d399` | actor A / primary accent |
| `--prat` | `#a78bfa` | actor B / opposing accent |
| `--team` | `#5ca9ff` | shared / both |
| `--human` | `#e0a63d` | the human, pauses |
| `--seal` | `#46c26a` | success, gates, done |
| `--stop` | `#ff6a5e` | failure, dead ends |

Two actors in tension is the identity: map the subject's own opposition onto
`--vadi`/`--prat` (writer/reviewer, client/server, before/after). Semantic
color (`--seal`/`--stop`/`--human`) never doubles as decoration. `--vadi`'s
emerald is lighter/cooler than `--seal`'s forest green — keep gates on the
`--seal`-stroked-rect idiom so the two greens never carry the same shape.

## Components (all defined in the template)

`.eyebrow` section label · `.chip` fact strip · `.cards` grid of `.card` ·
`.lane` (route/option card with colored dot + mono `.route` line) ·
`figure > svg + figcaption` · sticky `nav` only when the page exceeds ~3
screens · `.baton-rail` hero animation only on editorial pages (always behind
`prefers-reduced-motion`).

## Diagram rules

- Hand-authored SVG; geometric shapes only (rects, circles, lines, short
  paths). `svg text` is mono. Wide `viewBox`, `min-width` on the svg, and the
  `figure` scrolls (`overflow-x:auto`) — the page body never scrolls sideways.
- Node fill = owning actor's token. Dashed stroke = loop, optional, or
  fallback. `--seal`-stroked rect = gate/terminal.
- Multiple routes through shared stages → draw ONE comparative map: stages as
  columns, routes as horizontal lines stopping only at their stations
  (transit-map idiom). Don't draw N separate near-identical flowcharts.
- Label sparingly at 8.5–10px `--dim`/`--faint`; a legend when >2 encodings.

## Copy

Eyebrows are lowercase mono, ≤5 words. `h2`s state a thesis, not a topic
("Pick a profile: every path, one map", not "Profiles"). Captions add the one
insight the drawing can't say. Real content only — never lorem.

## Verify & publish

- In the Dvandva repo: `dvandva lint artifacts <file>` must pass. Elsewhere:
  keep the meta block + `color-scheme: dark;` anyway (they cost nothing).
- Publishing via the claude.ai Artifact tool: strip the outer
  doctype/html/head/body skeleton first (keep `<title>`, style, body content) —
  the tool wraps content itself.

## Common mistakes

| Mistake | Fix |
|---|---|
| Ad-hoc palette ("looks dark enough") | Tokens verbatim; identity comes from the duel pair |
| Structure described in bullets | Draw it; prose annotates the figure |
| One accent color everywhere | Ownership/oppposition mapped to `--vadi`/`--prat` |
| Figures without `figcaption` | Every figure carries its insight line |
| Page scrolls sideways on mobile | Wide content scrolls inside its own container |
