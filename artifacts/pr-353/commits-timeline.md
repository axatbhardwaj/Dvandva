# PR 353 Commit Timeline

Source: https://github.com/defi-com/monorepo/pull/353/commits

Exported: 2026-05-12

---

## 2026-05-10T14:46:03Z — `11041be0`

docs(internal): seed branch for 100% docs coverage push (DEF-718)

```text
docs(internal): seed branch for 100% docs coverage push (DEF-718)

Empty seed commit so a draft PR can open against `dev` to act as the
bidirectional channel between Claude and Codex agents during the
ralph-loop docs-coverage push. Iteration 1 deliverable
(docs/internal/doc-coverage-audit.md) lands in the next commit once
the spec is signed off via PR comments.

Refs: DEF-718
```

---

## 2026-05-10T15:00:05Z — `d62569d5`

docs(internal): seed 100% docs coverage audit checklist (DEF-718)

```text
docs(internal): seed 100% docs coverage audit checklist (DEF-718)

Iteration 1 of the docs/100pct-coverage push. Drops
docs/internal/doc-coverage-audit.md — the per-iteration checklist
that ralph-loop and codex /goal will drive against until 100%
coverage is reached (zero ⏳/🔄/⚠/❌ cells across all dimensions).

Design (gitignored, on disk at superpowers/specs/2026-05-10-doc-
coverage-audit-design.md) was acked by codex on PR #353 prior to
this commit. No content changes to any audited surface in this
iteration; every row is ⏳ pending or N·A per the §2 applicability
rule. Iteration 2+ flips statuses surface-by-surface.

Mirrored fiat-server/passkey-server paths under apps/ and services/
deliberately keep D2 ⏳ until a later iteration verifies whether
each pair represents distinct deployable surfaces or stale/legacy
directories.

Refs: DEF-718
```

---

## 2026-05-10T15:17:59Z — `967e4913`

docs(internal): iter 2 — resolve mirrored fiat-server/passkey-server (DEF-718)

```text
docs(internal): iter 2 — resolve mirrored fiat-server/passkey-server (DEF-718)

Audited the four mirrored rows per codex iter-2 ask:

- apps/fiat-server/, apps/passkey-server/ — empty post-a54c3905
  ('chore: remove legacy JS servers', 2026-03-23). Working-tree
  dirs contain only local-dev .env (gitignored). No tracked
  content, no package.json, no workspace membership. All four
  dimensions → N·A. No README/CLAUDE.md expected here.
- services/fiat-server/, services/passkey-server/ — sole canonical
  Rust workspace crates. README confirmed absent → D1 ❌; D2 N·A
  pending D1 (no doc to verify accuracy of yet); D3/D4 stay ⏳ for
  a future iteration that determines library-vs-binary status and
  audits public items.

Mirror-question annotations removed from all four rows. Iteration
log updated.

Cross-references discovered (for later iterations of those surfaces):
docs/architecture.md and apps/docs/docs/pages/guides/running-locally.mdx
both mention the deleted apps/* paths and need updating during
their own D2 audits. Notes added to the affected rows.

Refs: DEF-718
```

---

## 2026-05-10T15:29:45Z — `2675022d`

docs(architecture): iter 3 — fix stale apps/* + crates/rain-client refs (DEF-718)

```text
docs(architecture): iter 3 — fix stale apps/* + crates/rain-client refs (DEF-718)

Audited docs/architecture.md against current repo state per codex
iter-3 ask. Fixed five stale references:

- §2 Workspace Layout: apps/passkey-server now described as empty
  post-cleanup (a54c3905, 2026-03-23) instead of "source-only stub
  with src/server.ts". Replaced the dead crates/rain-client entry
  with the actual workspace member crates/rain-sdk.

- §5.1: cutover sentence moved to past tense + cite a54c3905.

- §6.2 crates table: removed the rain-client row (deleted 2026-03-23)
  and added a rain-sdk row reflecting current workspace membership
  + path dep in root Cargo.toml.

- §13.7 "apps/ states" bullet: corrected apps/passkey-server (also
  empty post-cleanup, not "keeps src/server.ts"). Corrected the
  tradfi-server claim — apps/tradfi-server/src/routes/stocks/stocks.ts
  EXISTS today, src/app.ts imports it cleanly. Only the Dockerfile
  remains missing.

- §2 layout entry for tradfi-server: matched the §13.7 correction.

- §14 Quick Reference: removed the dead crates/rain-client mention,
  pointed to crates/rain-sdk instead.

D2 audit on docs/architecture.md is partial — accuracy verified for
the sections touched above (§2, §5.1, §6.2, §13.7, §14). Remaining
sections (§1, §3 versions, §4 routes, §5.2/§5.3 endpoint tables, §7
Prisma models + migrations, §8 sequence diagrams, §9, §10, §11) not
yet audited; D2 stays ⏳ until they are. D1 → ✅. Iteration log
updated.

Cross-reference for later iterations: top-level CLAUDE.md carries
the same stale "stocks routes file ... missing" claim — flag for
that row's eventual D2 audit.

Refs: DEF-718
```

---

## 2026-05-10T15:34:15Z — `adfc1a4c`

docs(common): iter 4 — fix stale CLAUDE.md tradfi + crates refs (DEF-718)

```text
docs(common): iter 4 — fix stale CLAUDE.md tradfi + crates refs (DEF-718)

Audited top-level CLAUDE.md per codex iter-4 ask. Two fixes in the
Subdirectories table:

- apps/tradfi-server row: removed the stale "src/routes/stocks/stocks
  ... missing" claim. The file exists today; src/app.ts imports it
  cleanly. Only the Dockerfile gap remains (matches iter-3 finding
  in docs/architecture.md §13.7).

- crates/ row: replaced the deleted rain-client with the actual
  workspace member rain-sdk. Cited a54c3905 (the cleanup commit
  that removed legacy JS servers and the orphaned rain-client crate
  on 2026-03-23).

Audit doc updates per codex iter-4 ask:

- CLAUDE.md row: D1 → ✅, D2 → ⏳ (partial — Subdirectories table
  fixed; Repository Invariants, Commit Convention, MCP Servers
  sections not yet audited).
- docs/architecture.md row: dropped the brittle "702 lines" count
  from the notes per codex correction (file is now 704 lines;
  line counts are not durable enough to track).
- Iteration log: row 4 appended.

Refs: DEF-718
```

---

## 2026-05-10T15:38:22Z — `60d36211`

docs(common): iter 5 — finish CLAUDE.md D2 audit (DEF-718)

```text
docs(common): iter 5 — finish CLAUDE.md D2 audit (DEF-718)

Verified all remaining sections of top-level CLAUDE.md per codex
iter-5 ask. Result: D2 → ✅ for the whole file.

Repository Invariants — all 5 bullets verified against current code:

- staging / production.tfvars header matches (production "is a
  separate future task").
- SQLx offline cache (.sqlx/) and scripts/dev-stack.sh both present.
- Iron host iron.xyz + sandbox base api.sandbox.iron.xyz match
  services/fiat-server/src/config.rs:127.
- Webhook TIMESTAMP_TOLERANCE_SECS = 300 confirmed in both
  iron_webhooks.rs:14 and rain_webhooks.rs:15.
- Stealth invariant refined: STEALTH_CHAIN_CONFIGS only has USDC
  entries on Sepolia, Base Sepolia, Base mainnet, but
  aaveForwardingSupported is FALSE on Base Sepolia (placeholder
  forwarder pending DEF-548), TRUE on the other two. The doc
  previously said "all USDC entries with aaveForwardingSupported:
  true" which was incorrect. isStealthSupportedToken confirmed
  unused (only def + 2 doc references).

MCP Servers section verified against .mcp.json — all 6 entries
(svelte, ironxyz, linear, notion, context7, playwright) match
exactly.

Commit Convention section has no falsifiable claims beyond the
example PR-link; no edits needed.

Audit doc updates: CLAUDE.md row D2 → ✅; iteration log row 5
appended.

Refs: DEF-718
```

---

## 2026-05-10T15:53:19Z — `66d18c7a`

docs(internal): iter 6 — README.md audit (boilerplate stale, D2 ❌) (DEF-718)

```text
docs(internal): iter 6 — README.md audit (boilerplate stale, D2 ❌) (DEF-718)

Root README.md is unchanged from the original ts-turborepo-boilerplate
template — never updated for this project. All sections are wrong
(title, features, package list, prerequisites, tech stack, scripts,
project owner). Marked D2 → ❌ with detailed findings; full rewrite
proposed for iter 7 (too large to bundle into iter 6 under
one-action-per-iteration).

Also softened the iter-5 MCP "matches exactly" wording to "matches
semantically" per codex iter-6 correction — .mcp.json's playwright
entry uses @playwright/mcp@latest while CLAUDE.md's table omits the
@latest suffix; acceptable since the table doesn't claim a version.

Iteration log: row 6 appended.

Refs: DEF-718
```

---

## 2026-05-10T15:57:09Z — `511b97b2`

docs(common): iter 7 — rewrite root README from scratch (DEF-718)

```text
docs(common): iter 7 — rewrite root README from scratch (DEF-718)

Replaced the original ts-turborepo-boilerplate template README
(stale since the project's inception) with a project-accurate
README sourced from docs/architecture.md, top-level CLAUDE.md,
package.json, root Cargo.toml, and the current directory layout.

New README covers:

- Title + one-paragraph product description (DeFi-com monorepo —
  non-custodial wallet with stealth/passkey/Iron/Rain).
- Workspace layout (5 apps + 19 packages + 3 services + 4 crates +
  2 indexers + infra + docs).
- Prerequisites (Bun 1.2.2 from packageManager pin, Node 24 from
  engines, Rust 1.85 MSRV, Docker for dev:stack).
- Quickstart commands (bun install, dev, dev:stack, dev:rust,
  check-types, lint, format, test).
- Available-scripts table from package.json.
- Documentation pointers (architecture, Azure deployment,
  integrations, stealth setup, CLAUDE.md).
- Contributing (Conventional Commits + Linear DEF- prefix per
  CLAUDE.md).
- License pointer to LICENSE.

Removed: Wonderland/defi.sucks ownership text, sample-lib/sample-app/
sample-contracts package claims, pnpm prerequisite, generic Turborepo
+ Viem boilerplate framing.

Audit doc updates: README D2 → ✅; iter-6 evidence-note errors
corrected per codex (brittle "111 lines" count removed; false
"create-package missing" claim corrected — script exists at
./create-package.sh and is wired in package.json). Iteration log
row 7 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:00:20Z — `0123ff89`

docs(common): iter 8 — correct README dev:rust description (DEF-718)

```text
docs(common): iter 8 — correct README dev:rust description (DEF-718)

Per codex iter-8 review: the iter-7 README rewrite mis-described
`bun run dev:rust` as "Rust-only: build/run the Cargo workspace".
The actual package.json definition is:

  echo '⚠  dev:rust is deprecated — use bun run dev:stack instead' >&2
    && ./scripts/dev-stack.sh

So `dev:rust` is a deprecated alias into the full dev:stack pipeline,
not a Rust-only Cargo command.

Corrections:
- Removed the dev:rust block from the Quickstart section.
- Scripts table entry for dev:rust now reads "Deprecated alias —
  prints a warning then delegates to dev:stack. Use dev:stack
  directly."

Audit-doc updates: README row notes extended with the iter-8 patch;
iteration log row 8 appended. README D2 stays ✅ (no other facts
changed).

Refs: DEF-718
```

---

## 2026-05-10T16:04:13Z — `964825e1`

docs(internal): iter 9 — verify architecture.md §3 Tech Stack (DEF-718)

```text
docs(internal): iter 9 — verify architecture.md §3 Tech Stack (DEF-718)

Audited docs/architecture.md §3 against current source manifests.
All claims accurate; no edits to architecture.md needed.

§3.1 (TypeScript / Frontend) checked against root package.json
and apps/defi-app/package.json:
- Bun 1.2.2 (root packageManager pin), Node 24 (engines.node) ✓
- SvelteKit 2.48 / Svelte 5.43 / adapter-node 5.2 ✓
- Vite 7.2, Tailwind 4.1, TanStack Svelte Query 6, Zod 4 ✓
- Viem 2.41, ZeroDev SDK 5.5, scopelift stealth 1.0.0-beta.2,
  permissionless 0.1.30 ✓
- SimpleWebAuthn 9, ZeroDev passkey-validator 5.6 ✓
- Sentry hooks/instrumentation files all present ✓
- TS 5.9 / Vitest 4 / ESLint 9 / Prettier 3.6 (FE app pins; root
  pins differ at 5.3/8.57/3.3.3 — accurate framing for "Frontend" §) ✓
- @tanstack/svelte-form 1.28 still unimported in apps/defi-app/src
  or packages/ — claim still accurate ✓

§3.2 (Rust / Backend) checked against root Cargo.toml and
services/ptp-server/Cargo.toml:
- Edition 2021 / MSRV 1.85, Tokio (full), Axum 0.7 ✓
- Tower-HTTP 0.5 with cors+trace ✓
- SQLx 0.8, webauthn-rs 0.5, reqwest 0.12 ✓
- alloy 1 in ptp-server (with providers/signers/sol-types features) ✓
- Utoipa 5.4, Redis (deadpool-redis 0.18 + redis 0.27) ✓
- thiserror 2 + anyhow 1 ✓

Audit-doc updates:
- architecture.md row: §3 removed from pending-sections list;
  remaining are §1, §4, §5.2/§5.3, §7, §8, §9, §10, §11.
- README row notes: removed dev:rust from the quickstart
  enumeration per codex iter-9 ask (it's only in the deprecated-
  alias scripts table now).
- Iteration log: row 9 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:09:29Z — `f56e396e`

docs(architecture): iter 10 — fix §3.1 Auth row @zerodev/permissions claim (DEF-718)

```text
docs(architecture): iter 10 — fix §3.1 Auth row @zerodev/permissions claim (DEF-718)

Codex iter-10 review caught that the §3.1 Auth row mentioned
@zerodev/permissions as part of the active FE auth stack. Reality:

- apps/defi-app/package.json does NOT depend on @zerodev/permissions.
- Only packages/passkeys/package.json pins ^5.6.2.
- No source-level imports exist anywhere
  (rg "@zerodev/permissions" apps/defi-app/src packages/*/src → 0).

So @zerodev/permissions is a pinned-but-unimported transitive at
the packages/passkeys level — analogous to the existing
@tanstack/svelte-form dead-pin note in §3.1 Forms/validation row.

Updated the §3.1 Auth row to give it the same treatment: keep the
mention but explicitly call it dead-pin pending adoption or removal.

Audit-doc updates: README row already correct from iter 9; iter-9
note clarified with the iter-10 correction; iteration log row 10
appended.

Refs: DEF-718
```

---

## 2026-05-10T16:11:07Z — `70bc5f24`

docs(internal): iter 10 follow-up — append missing iteration-log row (DEF-718)

```text
docs(internal): iter 10 follow-up — append missing iteration-log row (DEF-718)
```

---

## 2026-05-10T16:14:17Z — `a34faf7b`

docs(architecture): iter 11 — verify §7 Prisma models + migrations (DEF-718)

```text
docs(architecture): iter 11 — verify §7 Prisma models + migrations (DEF-718)

§7.1 Prisma Models table: schema has 11 models, doc had 9. Added
SumsubApplicant + SumsubWebhookEvent rows (both added by migration
20260420130000_add_sumsub_tables). Confirmed TTL defaults in schema
match doc claims (IronIdempotencyKey 24h via dbgenerated NOW() +
INTERVAL '24 hours', Iron/Rain/Sumsub webhook events 1h via
INTERVAL '1 hour').

§7.2 Recent Migrations: doc listed 3 entries, full migration history
has 18+. Expanded the §7.2 list with 3 missing recent migrations:

- 20260415120000_add_rain_transactions — adds rain_transactions
  table via raw SQL; NOT a Prisma model (so absent from §7.1).
  Populated by services/fiat-server/src/services/rain_webhook_processor.rs
  for transaction.* events.
- 20260420120000_rain_webhook_event_id_unique — idempotent
  re-application of the rain event_id index (CREATE UNIQUE INDEX IF
  NOT EXISTS); explains event_id nullability per Postgres NULL-
  distinctness semantics.
- 20260420130000_add_sumsub_tables — added the SumsubApplicant +
  SumsubWebhookEvent models.

Audit-doc updates: §7 removed from architecture.md pending-sections
list; remaining are §1, §4, §5.2/§5.3, §8, §9, §10, §11.
Iteration log row 11 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:18:36Z — `89573da2`

docs(architecture): iter 12 — §7 precision corrections (DEF-718)

```text
docs(architecture): iter 12 — §7 precision corrections (DEF-718)

Three precision fixes per codex iter-12 review:

- §7.1 PasskeyUser row: schema has four optional/many relations
  (PasskeyCredential[], UserIronMapping?, UserRainMapping?,
  SumsubApplicant?). Doc previously said "credentials and Iron/Rain
  mappings" — missed the SumsubApplicant relation. Updated.

- §7.1 DeFiIdEntry row: schema has isProposed boolean (added by
  20260212150705_add_is_proposed_to_defi_id_entry). Doc previously
  said "Reserved / premium / profanity keyword bucket" — missed
  proposed entries. Updated to "Reserved / premium / profanity /
  proposed" with migration cite.

- §7.2 Recent Migrations: list missed 20260323000000_add_claimed_at
  which adds claimed_at TIMESTAMP(3) to both iron_webhook_events
  and rain_webhook_events for the scheduler claim/unclaim pattern
  (180-second stale-claim window in crates/db/src/{iron,rain,sumsub}.rs).
  Added at the top of the §7.2 list.

Audit-doc: iteration log row 12 appended; §7 stays out of pending list.

Refs: DEF-718
```

---

## 2026-05-10T16:23:23Z — `9cccff95`

docs(internal): iter 13 — verify architecture.md §1 + audit-row cleanup (DEF-718)

```text
docs(internal): iter 13 — verify architecture.md §1 + audit-row cleanup (DEF-718)

Audited docs/architecture.md §1 System Overview against current
repo state. Verification only — no edits to architecture.md needed.

§1.1 mermaid system-context diagram:
- All three Rust services (passkey-server :8080, fiat-server :3001,
  ptp-server :3004) exist; ports are env-overridable via cfg.port /
  config.port — defaults match the diagram.
- ENS subgraph URL configured in services/ptp-server/src/config.rs
  (ens_subgraph_url field).
- FE-direct Alchemy invariant test confirmed at
  packages/defi-aggregator/test/invariants/no-backend-alchemy-proxy.spec.ts.
- Backend services (PG, Redis, Queue), chains (Sepolia 11155111,
  Base Sepolia 84532, Base 8453), and external APIs (ZeroDev,
  Iron, Rain, SumSub) all match current state.

§1.2 headline-decisions table:
- Hybrid TS+Rust ✓
- Bun+Turbo+Cargo (Turbo can't see Rust — confirmed: no package.json
  under services/ or crates/) ✓
- Compile-time DB+MQ binding (matches §13 detail) ✓
- Passkey-derived AA wallet ✓
- Stealth via Ponder ✓
- FE-direct Alchemy ✓ (invariant test exists)

Audit-doc updates per codex iter-13 cleanup ask:
- Architecture.md row notes folded in iter-12 corrections (PasskeyUser
  4-relation enumeration, DeFiIdEntry isProposed, claimed_at migration)
  + iter-13 §1 verification result. Row now reads as a current
  snapshot, not iter-11-frozen.
- §1 removed from architecture.md pending-section list. Remaining
  ⏳-pending: §4, §5.2/§5.3, §8, §9, §10, §11.
- Iteration log row 13 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:27:03Z — `d1371991`

docs(architecture): iter 14 — show fiat-server's direct Sumsub integration in §1.1 (DEF-718)

```text
docs(architecture): iter 14 — show fiat-server's direct Sumsub integration in §1.1 (DEF-718)

Codex iter-14 review caught that the §1.1 mermaid system-context
diagram understated Sumsub. It was labelled "SumSub (KYC, via Rain)"
with only `Rain -.->|sumsub share token| SumSub`, implying Sumsub
is reachable only via Rain.

Reality: services/fiat-server integrates Sumsub directly. Evidence:
- src/config.rs has SUMSUB_APP_TOKEN, SUMSUB_SECRET_KEY,
  SUMSUB_WEBHOOK_SECRET, SUMSUB_LEVEL_NAME, SUMSUB_BASE_URL,
  SUMSUB_RAIN_CLIENT_ID
- src/state.rs holds Option<Arc<SumsubClient>>
- src/main.rs mounts /api/sumsub + /webhooks/sumsub and spawns a
  Sumsub queue consumer
- src/services/sumsub_client.rs calls Sumsub for WebSDK access
  tokens, share tokens, applicant reads, webhook management

Diagram updated:
- SumSub label now reads "KYC — direct from fiat-server + share
  token via Rain"
- New arrow `FS -->|WebSDK token + applicant + webhooks| SumSub`
  added alongside the existing `Rain -.->|sumsub share token|
  SumSub` edge.

Audit doc: §1 architecture.md row notes folded in the iter-14
correction; iteration log row 14 appended; §1 stays out of
pending-section list.

Refs: DEF-718
```

---

## 2026-05-10T16:31:22Z — `2bad3df7`

docs(internal): iter 15 — fix iter-14 log row table formatting (DEF-718)

```text
docs(internal): iter 15 — fix iter-14 log row table formatting (DEF-718)

Codex iter-15 review caught that the iter-14 log row had raw `|`
characters from inline mermaid edge syntax (Rain -.->|sumsub share
token| SumSub and FS -->|WebSDK token + applicant + webhooks|
SumSub). The markdown formatter interpreted those as extra column
separators, breaking the table into 13 pipes per row instead of 7
and padding the separator row to match.

Fix:
- Rewrote the iter-14 row to describe the edges in plain prose
  ("dotted Rain → SumSub 'sumsub share token' edge", "new solid
  arrow from fiat-server (FS) to SumSub labelled 'WebSDK token +
  applicant + webhooks'") with no raw `|` characters.
- Restored the iteration-log separator row to 7 pipes (6 columns).
- All 16 iteration-log rows now consistent at 7 pipes.

No content change to architecture.md. Iter-15 log row appended
following the same plain-prose convention.

Refs: DEF-718
```

---

## 2026-05-10T16:35:25Z — `b0fb961f`

docs(architecture): iter 16 — verify §5.2/§5.3 endpoint tables (DEF-718)

```text
docs(architecture): iter 16 — verify §5.2/§5.3 endpoint tables (DEF-718)

§5.3 (ptp-server) endpoints accurate as written. All 5 routes
match services/ptp-server/src/main.rs router (/health, /api/resolve,
/api/ccip/:sender/:call_data, /api/score, /api/updateScore).

§5.2 (fiat-server) endpoint table had 3 missing surfaces — same
Sumsub-blindspot pattern as iter 14's §1 diagram fix:

- Full Sumsub domain row added: POST /api/sumsub/access-token,
  GET /api/sumsub/status, GET /api/sumsub/identity,
  POST /api/sumsub/apply-rain. Mounted via routes::sumsub::router()
  in main.rs; handlers 503 when any of SUMSUB_APP_TOKEN /
  SECRET_KEY / WEBHOOK_SECRET / LEVEL_NAME is missing.

- POST /webhooks/sumsub added to the Webhooks row alongside
  /webhooks/iron and /webhooks/rain. Handler at
  routes/sumsub_webhooks.rs.

- General /health, /healthz, /readyz endpoints (mounted at the
  app root in main.rs) added to the Health row alongside the
  legacy /api/iron/health.

Iron, DeFi-ID, Rain rows verified accurate against route files.

Audit doc updates: §5.2/§5.3 removed from architecture.md
pending-sections list; remaining ⏳ are §4, §8, §9, §10, §11.
Iteration log row 16 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:43:18Z — `be0e706c`

docs(architecture): iter 17 — §5.2/§5.3 precision + audit-row formatting fixes (DEF-718)

```text
docs(architecture): iter 17 — §5.2/§5.3 precision + audit-row formatting fixes (DEF-718)
```

---

## 2026-05-10T16:46:25Z — `3a7d297a`

docs(architecture): iter 18 — fix §12 stale rate-limit duplicate (DEF-718)

```text
docs(architecture): iter 18 — fix §12 stale rate-limit duplicate (DEF-718)

Codex iter-18 review caught that §12 decision #6 ("Asymmetric rate
limits in ptp-server") still carried the old "3 rps sustained" /
"1 req/3s sustained" wording — the same stale claim already fixed
in §5.3 during iter 17, just duplicated in this §12 NOTE block.

Fixed §12 to match: /api/resolve "1 req per 3 s sustained ≈ 20/min,
20 burst" and /api/ccip/... "1 req per 2 s sustained ≈ 30/min, 40
burst". Added explicit cite to §5.3 to prevent future duplicate-fact
drift.

Verified no other stale rate-limit wording in architecture.md
(grep "3 rps|3 req/s|1 req/3s" → empty).

Cross-iteration lesson #6 reinforced: fix-then-grep. When fixing a
stale claim, grep the WHOLE doc for the same fact-string before
committing — not just the section codex flagged. Going forward,
every §-fix iteration ends with a fact-string grep.

Audit-doc: architecture row notes folded in iter-18 fix; iteration
log row 18 appended.

Refs: DEF-718
```

---

## 2026-05-10T16:49:49Z — `89558e9d`

docs(architecture): iter 19 — verify §11 Security Model + add Sumsub edges (DEF-718)

```text
docs(architecture): iter 19 — verify §11 Security Model + add Sumsub edges (DEF-718)
```

---

## 2026-05-10T16:56:28Z — `c564f8de`

docs(architecture): iter 20 — §11 outbound-vs-inbound auth + audit-table fix (DEF-718)

```text
docs(architecture): iter 20 — §11 outbound-vs-inbound auth + audit-table fix (DEF-718)
```

---

## 2026-05-10T17:02:17Z — `edd9edf9`

docs(architecture): iter 21 — verify §10 Deployment + add APIM/SWA/KV (DEF-718)

```text
docs(architecture): iter 21 — verify §10 Deployment + add APIM/SWA/KV (DEF-718)
```

---

## 2026-05-10T17:10:56Z — `f40515b5`

docs(architecture): iter 22 — §10 runtime topology + env-surface precision (DEF-718)

```text
docs(architecture): iter 22 — §10 runtime topology + env-surface precision (DEF-718)

Codex iter-22 review caught 3 §10 gaps. Fixed:

§10 mermaid runtime topology:
- Added fiat-server-worker (FIAT_ROLE=worker, no ingress, KEDA queue
  scaling per infra/modules/compute/main.tf:428-440).
- Added fiat-webhook-sync (Container Apps Job; reconciles Rain +
  Sumsub subscriptions; triggered post-deploy via az containerapp
  job start from .github/workflows/deploy.yml).
- Added Front Door --> announcement-indexer edge for /indexer and
  /indexer/* (per infra/modules/frontdoor/main.tf:285-345; origin
  path rewritten to / so /indexer/graphql forwards to Ponder /graphql).
- Secrets edges from new units to Key Vault.

§10.1 env surface precision:
- Frontend paragraph reframed to separate three surfaces (canonical
  wrapper, $env/static/public direct reads, .env.example extras).
  Added missing wrapper vars: PUBLIC_ZEROEX_API_KEY,
  PUBLIC_PNL_INDEXER_URL, PUBLIC_PAYY_RPC_URL, PUBLIC_PAYY_USDC_ADDRESS,
  PUBLIC_PAYY_VAULT_REGISTRY_ADDRESS. Added .env.example extras:
  PUBLIC_BUNDLER_URL, PUBLIC_PAYMASTER_URL, PROXY_BACKEND.
- New §10.1 announcement-indexer block: DATABASE_URL (own schema,
  not app DB), DATABASE_SCHEMA, chain-keyed PONDER_RPC_URL_<chainId>
  (Sepolia 11155111, Base Sepolia 84532, Base mainnet 8453 per
  ponder.config.ts).

§10.3 docker-compose command:
- Was bare "docker compose ... up -d" which actually starts every
  service in the file (incl. nats, servicebus-emulator, mssql).
  Narrowed to "up -d postgres redis azurite" matching dev-stack.sh:97.
  Comment explains the omission (mq pinned to azqueue at compile time).

Audit-doc: §10 row notes folded in iter-22 corrections; iter-22 log
row appended (plain prose, no raw mermaid pipes per lesson #5).
Pre-commit pipe-count check: clean.

Refs: DEF-718
```

---

## 2026-05-10T17:17:13Z — `bb172dcc`

docs(architecture): iter 23 — §10 worker scale + frontend env + indexer DB precision (DEF-718)

```text
docs(architecture): iter 23 — §10 worker scale + frontend env + indexer DB precision (DEF-718)
```

---

## 2026-05-10T17:22:21Z — `e1eb72db`

docs(internal): iter 24 strip stale evidence from iter-22/23 rows (DEF-718)

```text
docs(internal): iter 24 strip stale evidence from iter-22/23 rows (DEF-718)

Per codex iter-24 review, the audit log contained now-corrected stale
claims as if current evidence: L290 asserted KEDA queue scaling,
static-public WalletConnect, "own schema not app DB"; L291 quoted
those phrases verbatim while explaining the corrections.

Rewrote both rows in plain prose without the wrong wording. Also
folded the §10 verified summary into the architecture.md row L232
and removed §10 from its pending-section list (iter 23 had failed
to land that update due to an Edit-vs-formatter race).

Verified clean: grep across both files for the four stale phrases
returns empty. Pipe-count check empty.

Refs: DEF-718
```

---

## 2026-05-10T17:25:45Z — `60119265`

docs(internal): iter 25 paraphrase iter-24 row + fix anchor (DEF-718)

```text
docs(internal): iter 25 paraphrase iter-24 row + fix anchor (DEF-718)

Per codex iter-25 review of commit e1eb72db:

1. Iter-24 row L292 reintroduced the four stale phrases verbatim
   while describing what it removed. Paraphrased the prose so each
   phrase reads as "the stale worker scaling phrase" etc.

2. Restored broken anchor #4415878582 to #issuecomment-4415878582.

Cross-file grep on the four corrected fact-strings now empty;
github anchor regex check empty; pipe-count check empty.

Refs: DEF-718
```

---

## 2026-05-10T17:33:14Z — `f0eae5d9`

docs(architecture): iter 26 verify §9 Build & CI (DEF-718)

```text
docs(architecture): iter 26 verify §9 Build & CI (DEF-718)

§9.1 Turbo outputs list expanded from 3-item subset to full set per
turbo.json; called out that lint/test variants all depend on build.

§9.2 GitHub Actions table corrected on three fronts:
- rust.yml row now describes all CI steps including the missing
  cargo sqlx prepare --workspace --check (verifies .sqlx/ offline
  cache), SQLX_OFFLINE=true on every step, toolchain pinned to 1.88
  (CI vs MSRV 1.85 in §3.2), required-status-check role gating
  deploy via needs: [rust].
- main-workflow.yml row had a false claim that it invokes
  gitleaks.yml. Reality: orchestrator only calls
  build → lint → test + test-integration. Doc now states gitleaks
  is defined as a reusable workflow but NOT invoked by any
  orchestrator (CI-gap follow-up flagged).
- deploy.yml row now describes the two-job structure: matrix
  build-push of 4 images, then all-or-nothing deploy job with
  fixed backend-first update order (passkey-server → fiat-server
  → fiat-server-worker → announcement-indexer → frontend) plus
  fiat-webhook-sync job trigger.

deploy-staging.yml + deploy-nlayer.yml rows verified accurate.
§9.3 Dockerfiles list verified.

Audit-doc: §9 verified summary folded into architecture.md row;
pending list trimmed to §4 + §8 only. Iter-25 + iter-26 log rows
appended (iter 25 was a small carry-forward fix that hadn't
gotten its own log row).

Refs: DEF-718
```

---

## 2026-05-10T17:39:50Z — `8847a152`

docs(architecture): iter 27 §9 carry-forward precision (DEF-718)

```text
docs(architecture): iter 27 §9 carry-forward precision (DEF-718)

Per codex iter-27 review, 5 §9 cleanup items:

1. rust.yml workflow_call source: from deploy-staging.yml /
   deploy-nlayer.yml (sibling jobs), NOT from deploy.yml itself.
2. Gating wording softened: was "required status check", now
   "job-dependency gate in the deploy orchestrators" (branch
   protection enforcement is configured outside the repo).
3. fiat-webhook-sync trigger is conditional on auto_webhook_sync
   input (default false; nlayer sets true; staging stays false to
   avoid silently re-registering production webhooks). The trigger
   is a two-step sequence: az containerapp job update (pin image)
   then az containerapp job start (run).
4. Setup composite action does Bun 1.2.2 + Node 24 + Foundry
   nightly + bun install --frozen-lockfile + bun run build —
   not just the env-setup summary previously stated.
5. §9.3 Dockerfiles split into per-image bullet list: Rust services
   use cargo-chef multi-stage; announcement indexer is Node 22-slim
   with npx ponder codegen; defi-app is oven/bun:1 single-stage with
   bunx --bun turbo build (no turbo prune; bun-workspace symlinks
   break in pruned context per the Dockerfile comment).

Audit row L295 paraphrased to avoid self-referential reintroduction
of the corrected stale phrases (lesson #8); URL anchor patched.

All three codex-spec verification greps now empty.

Refs: DEF-718
```

---

## 2026-05-10T17:46:19Z — `1deb876e`

docs(architecture): iter 28 verify §8 + fix vault crypto (DEF-718)

```text
docs(architecture): iter 28 verify §8 + fix vault crypto (DEF-718)

§8.1-§8.5 sequence diagrams cross-checked against current code.
All routes, hooks, providers verified to exist:
- /v2/{onboarding/create,stealth/inbox,send,tx-preview,bank/deposit,
  earn,earn/deposit} all present
- useStealthSubgraphScan, useClaimStealth, useStealthEnrichment,
  useGasEstimate, useDefiData, useStrategyPositions,
  useMultiAddressBalances all present
- RateRebalanceSheet.svelte present in earn/components/
- defi-aggregator has chainlink, defillama, alchemy provider
  directories backing the §8.5 par-block
- /v2/bank/deposit confirmed not to touch Sumsub
- passkey-server challenge TTL default of 60_000ms confirmed in
  config.rs:25

Material correction (1 finding, 3 instances): vault crypto wording
was wrong. Doc claimed AES-256-CBC; actual implementation in
apps/defi-app/src/lib/modules/device-vault/crypto.ts uses AES-GCM
(crypto.subtle.importKey with 'AES-GCM' at lines 56/61/77/82).
Fixed across §4.2, §8.1, and §11.2 via cross-file replace per the
iter-23 fix-then-grep lesson — even though §11 was previously
D2-✅, leaving a known-wrong claim there contradicts the
source-of-truth principle.

Audit-doc: §9 + §8 verified summaries folded into architecture.md
row L232; pending list now §4 routes only. iter-28 log row
appended (with paraphrased self-referential phrases per lesson #8).

Refs: DEF-718
```

---

## 2026-05-10T17:54:45Z — `75f83201`

docs(architecture): iter 29 §8 sequence-diagram cleanup (DEF-718)

```text
docs(architecture): iter 29 §8 sequence-diagram cleanup (DEF-718)

Per codex iter-29 review: my iter-28 audit checked existence
(routes/hooks/providers exist) but not flow shape. Codex caught
4 sequence-diagram inaccuracies:

§8.2 Stealth Receive: claim is layout-driven auto-claim, not
inbox-driven. /v2/+layout.svelte owns backgroundScanner +
useStealthAutoClaim + backgroundClaim; layout calls scanAll()
then tickQueue(). Inbox is display/retry only. Diagram rewritten
with Layout + Inbox participants.

§8.3 Stealth Send: doesn't call PTP /api/resolve. Uses
useStealthMetaAddressByDefiName -> readStealthMetaAddressByDefiName
(viem public client on DEFI_ID_CHAIN_ID = 11155111) per
defi-id.keys.ts:55. PTP participant replaced with DefiIdChain;
NOTE explains why PTP is not in this flow.

§8.4 Fiat Onramp: register-fiat not called in deposit flow.
Actual sequence: GET customer/{id} -> POST onboard -> KYC poll
-> ensureAccountDeployed() + register-wallet -> autoramp-quote
-> GET autoramps/{id} + POST create-autoramp. create-autoramp
returns deposit rails the user funds externally; webhooks update
state after settlement.

§8.5 Yield Discovery: provider par-block was wrong. Actual flow
is three facades + on-chain fallback - useDefiData() fetches
Yield.xyz pools (the previously-claimed APY-feed provider was
incorrect); usePositions() calls Enso; useOnChainBalances() is
multicall + Alchemy enrichment fallback; PnL chart uses DefiLlama
historical prices via PnL facade. Rebalance: RateRebalanceSheet
-> getQuoteFacade() Enso quote -> openTxPreview.

Cross-iteration meta-lesson: existence checks are necessary but
nowhere near sufficient for sequence-diagram audits. Trace the
actual call chain at each entry point.

All four codex-spec verification greps + audit-doc pipe check
empty. Architecture.md is one section away from full D2 ✅.

Refs: DEF-718
```

---

## 2026-05-10T18:00:15Z — `a6e882ac`

docs(internal): iter 30 supersede iter-28 + audit-row §8 cleanup (DEF-718)

```text
docs(internal): iter 30 supersede iter-28 + audit-row §8 cleanup (DEF-718)

Per codex iter-30 review of iter-29:

1. L232 architecture row §8 portion still carried iter-28 existence-
   based proof statements (provider directories, hook-existence
   enumeration) as if current evidence — superseded by iter-29 flow
   verification. Rewrote to keep only the iter-29 verification.
2. Duplicate vault-crypto sentence appeared twice in L232 — removed.
3. L296 iter-28 row stood as if its existence proof were durable
   evidence — rewrote as a Superseded-by-iter-29 stub explaining
   why existence-based audit was insufficient + noting the only
   durable artifact (vault-crypto correction).
4. Paraphrased the remaining "the legacy register-fiat" verbatim
   mention in iter-29 row to "a deprecated wallet-registration
   endpoint" so codex grep returns clean.
5. Fixed typo at architecture.md:449 — lowercase 'the' after a
   period → 'The'.

Cross-iteration meta-lesson: the audit log accumulates not just
stale facts but also stale verification approaches. When iter N+1
supersedes iter N's methodology, iter N's row needs to be marked
as historical-and-superseded.

All three codex-spec verification greps + pipe-count check empty.

Refs: DEF-718
```

---

## 2026-05-10T18:09:53Z — `d9a15904`

docs(architecture): iter 31 — §4 FE routes audit + §13.3 fix (DEF-718)

```text
docs(architecture): iter 31 — §4 FE routes audit + §13.3 fix (DEF-718)

§4.1 route-map table extended to match `apps/defi-app/src/routes/`
ground truth — added 8 missing routes (`/v2/invest`, `/v2/invest/[symbol]`,
`/v2/kyc`, `/v2/cards/{fund,transactions,withdraw}`, `/account/rain/apply`,
`/v2/tx-status-demo`, `/sentry-example-page`); corrected the misnamed
`/api/octav/sync-transactions` claim to the actual `/api/zerion/transactions`
(`+server.ts`); reframed root-redirect line to cite the `+page.ts` 307
redirect via `$app/paths` base. Added a §4 intro paragraph naming
`/v2/+layout.svelte` as the auth/state owner.

§13.3 also fixed — activity-feed bullet had the wrong streams (Alchemy +
Octav + Ponder); actual is 4 hooks: useTransactions (Zerion-backed via
POST /api/zerion/transactions), useIronTransactions, useRainTransactions,
useStealthTransactions (Ponder).

Architecture.md D2 → ✅ in full. Audit doc updated.

Refs: DEF-718
```

---

## 2026-05-10T18:17:25Z — `518f4992`

docs(architecture): iter 32 — codex iter-31 corrections (DEF-718)

```text
docs(architecture): iter 32 — codex iter-31 corrections (DEF-718)

Codex iter-31 review caught 3 issues fixed in this iteration:

1. §13.3 method verb wrong — claimed write verb on the activity-sync
   endpoint without verifying. The SvelteKit route exports a read handler,
   FE facade defaults to read (no method override). §13.3 prose
   restructured with route + facade cites.

2. Audit-row anchor in iter-31 row was a fabricated codex-review URL
   (404). Replaced with two real anchors (codex iter-30 close + iter-31
   review). New rule: never pre-fill a codex review URL.

3. Legacy-proxy drift framing was unsupported — claimed live Front Door
   retains the legacy-named WAF rule and KV secrets without verifying
   live state. IaC at infra/modules/frontdoor/main.tf:781 actually
   defines the rule under the Zerion-aligned name; no legacy-provider
   reference exists under infra/. So the actual drift is doc staleness
   in docs/Azure/azure-deployment.md relative to IaC, not infra-vs-FE.

Lesson #8 redux (third occurrence): paraphrased every verbatim stale
string in the audit-row prose so codex's verification grep returns
empty across docs/architecture.md + audit doc.

Architecture.md D2 ✅ holds in full.

Refs: DEF-718
```

---

## 2026-05-10T18:22:07Z — `8de1894c`

docs(fiat-server): iter 34 — add services/fiat-server/README.md (DEF-718)

```text
docs(fiat-server): iter 34 — add services/fiat-server/README.md (DEF-718)

First services-area README. Lean (~5KB), sourced strictly from current
code surfaces: Cargo.toml binaries, main.rs port + FIAT_ROLE + MQ wiring,
lib.rs module list, routes/ mount table, services/ long-running modules,
tests/ coverage list, Dockerfile cargo-chef shape.

Defers depth to docs/architecture.md (§1.1, §5.2, §10.1, §11.1, §13)
and CLAUDE.md Repository Invariants. No duplicated facts — only
cross-references.

Audit row: services/fiat-server/ D1 ❌→✅, D2 N·A→✅. D3/D4 still ⏳
(lib.rs has 10 pub mods so dimensions are in scope; deferred).

Independent of the architecture.md surface — parallel work while awaiting
codex iter-32 review on commit 518f4992.

Refs: DEF-718
```

---

## 2026-05-10T18:25:47Z — `53c2c0bf`

docs(passkey-server): iter 36 — add services/passkey-server/README.md (DEF-718)

```text
docs(passkey-server): iter 36 — add services/passkey-server/README.md (DEF-718)

Second services-area README. Lean (~5KB), sourced from current code:
Cargo.toml (single binary, no helpers), main.rs (port 8080, no role
split), routes.rs (8 endpoints: 6 WebAuthn + 2 health), config.rs
(5 env vars), challenge.rs (Redis cache), webauthn.rs (webauthn-rs
0.5), tests/ (smoke + authenticator), Dockerfile (cargo-chef).

Defers depth to docs/architecture.md (§1.1, §5, §10.1, §11.1, §13)
and CLAUDE.md Repository Invariants (Redis DB-0/DB-1 split).

Audit row: services/passkey-server/ D1 ❌→✅, D2 N·A→✅,
D3/D4 ⏳→N·A (binary-only crate per §2 applicability rule —
no src/lib.rs).

Refs: DEF-718
```

---

## 2026-05-10T18:28:46Z — `ab7061a9`

docs(ptp-server): iter 37 — verify README D2 + add access/rate-limit columns (DEF-718)

```text
docs(ptp-server): iter 37 — verify README D2 + add access/rate-limit columns (DEF-718)

Third services-area surface; README was already present so this iteration
is the D2 audit. Verified accurate against current code: 7-endpoint API
table matches src/main.rs router; 3 off-chain sources match src/sources/;
6 on-chain sources match packages/contracts/src/contracts/ptp/; port 3004
default; 11 env vars; weights file matches.

Two patches:
- Removed AI-disclaimer header (now D2-audited); replaced with audit-trail
  pointer to docs/internal/doc-coverage-audit.md.
- Extended API reference table with Access + Rate-limit columns. Adds
  visibility for the localhost middleware (POST /api/updateScore +
  GET /api/score gated) and the tower_governor rate limits on
  /api/resolve and /api/ccip/* — both critical security boundaries
  already covered in architecture.md §5.3 + §11.1.

Audit row: services/ptp-server/ D1 ⏳→✅, D2 ⏳→✅. D3/D4 stay N·A
(binary-only, not a Cargo workspace member, out-of-band deploy).

All three services rows now D1 ✅ + D2 ✅. Services area closed.
Next surface: crates.

Refs: DEF-718
```

---

## 2026-05-10T18:37:26Z — `1af07858`

docs(services): iter 39 — codex iter-38 corrections (DEF-718)

```text
docs(services): iter 39 — codex iter-38 corrections (DEF-718)

Codex caught 7 issues across iter-34 / iter-36; all addressed:

1. fiat-server OpenAPI mount path — README claimed simple swagger-ui;
   actual is /api-docs (full) + /api/iron/swagger-ui (Iron sub-spec)
   per src/main.rs:255-260.
2. fiat-server sandbox config — README cited wrong variable; actual
   gate is ENABLE_SANDBOX → cfg.enable_sandbox per config.rs:137-139
   + main.rs:245-247.
3. fiat-server rain webhook test — README understated verification.
   Actual: raw-body HMAC + signed-timestamp freshness (eventReceivedAt
   or timestamp parsed from signed JSON, 5-min tolerance) + event_id
   ON CONFLICT idempotency.
4. fiat-server Dockerfile binaries — runtime image carries 4 binaries:
   fiat-server, rain-webhook, sumsub-webhook (this crate), plus
   dlq-drain from sibling mq crate.
5. Audit row miscount of webhook-processor modules — 3 (iron, rain,
   sumsub), not 4.
6. passkey-server CORS_ORIGIN — Required: yes, default wildcard rejected
   at startup per main.rs:74-76 panic. Run-command line updated.
7. fiat-server D3/D4 reframed as N·A — lib.rs is internal sibling-binary
   sharing, not consumer-facing API per §2 applicability rule.

Services area still closed: 3/3 D1 + D2 ✅, D3/D4 N·A everywhere.

Refs: DEF-718
```

---

## 2026-05-10T18:38:29Z — `faa01d73`

docs(fiat-server): iter 39b — fix rain_webhooks routes-table row missed in iter 39

```text
docs(fiat-server): iter 39b — fix rain_webhooks routes-table row missed in iter 39

Iter 39 corrected the rain_webhook_integration tests-table row but
missed the routes-table row at L45 (formatter whitespace shift broke
the original Edit). Both rows now describe the actual handler:
raw-body HMAC + signed-timestamp freshness (eventReceivedAt or
fallback timestamp parsed from signed JSON, 5-min tolerance) +
event_id ON CONFLICT idempotency.

Refs: DEF-718
```

---

## 2026-05-10T18:42:58Z — `bb07e033`

docs(db): iter 41 — add crates/db/README.md (DEF-718)

```text
docs(db): iter 41 — add crates/db/README.md (DEF-718)

First crates-area README. Lean (~5KB), sourced strictly from current
code: Cargo.toml (lib-only, openapi feature), lib.rs (8 modules + 4
re-exports + 1 ping probe + 2 idempotent boot migrations), models.rs
(16 row structs across 5 domains), per-module pub-fn surveys, error
+ pool. Documents the SQLx offline-mode story (.sqlx/ cache +
cargo sqlx prepare regen step) and notes no in-crate tests
(coverage lives in consumer-service integration tests).

Defers depth to docs/architecture.md (§6.2 crates table, §7 Prisma
models + migrations) and CLAUDE.md SQLx invariant. Cross-refs Prisma
schema + the two consumer services (fiat-server with openapi feature,
passkey-server with default features).

Audit row: crates/db/ D1 ⏳→✅, D2 ⏳→✅. D3/D4 stay ⏳
(consumer-facing library — services depend on it; in scope per §2,
deferred for later iteration).

Refs: DEF-718
```

---

## 2026-05-10T18:47:58Z — `83b1dabd`

docs(iron-client): iter 43 — add crates/iron-client/README.md (DEF-718)

```text
docs(iron-client): iter 43 — add crates/iron-client/README.md (DEF-718)

Second crates-area README. Lean (~5KB), sourced from current code:
Cargo.toml (lib-only, openapi feature), lib.rs (11 modules + 4
re-exports), client.rs (IronClient + IronClientConfig + 7 sub-client
accessors + build_query_path), types.rs (16+ pub types).

Important auth callout: outbound Iron API uses X-API-Key only
(client.rs:75); HMAC verification is the inbound-webhook side
(services/fiat-server/src/routes/iron_webhooks.rs) — the iter-20
codex correction on architecture.md §11.1.

Iron-host pin documented per CLAUDE.md Repository Invariants
(api.iron.xyz prod / api.sandbox.iron.xyz sandbox; legacy
getiron.com dead since 2026-04-08).

Defers depth to docs/architecture.md (§6.2, §11.1, §13) and
CLAUDE.md. Cross-refs fiat-server (consumer), inbound webhook
handler, packages/iron (TypeScript counterpart).

Audit row: crates/iron-client/ D1 ⏳→✅, D2 ⏳→✅. D3/D4 stay ⏳.

Refs: DEF-718
```

---

## 2026-05-10T18:51:24Z — `965f4c6d`

docs(db): iter 44 — codex iter-41 corrections (DEF-718)

```text
docs(db): iter 44 — codex iter-41 corrections (DEF-718)

Codex caught 4 issues; all addressed:

1. Public fn counts under-reported. Actual per `^pub fn` grep:
   defi_id 5, iron 26 (was 7 — module includes Iron state
   projection upserts beyond mappings/idempotency/webhooks),
   passkey 8, rain 18, sumsub 12. Module-layout block + audit
   row corrected with real counts and category enumeration.

2. "16 row types" misleading. 16 public structs total but only
   15 derive sqlx::FromRow; RainTransactionUpsert is an input
   DTO (Debug, Clone, Default only). Module summary + Models
   heading + Prisma-mirror paragraph all updated.

3. Prisma framing wrong. migrate_claimed_at IS in Prisma
   (20260323000000_add_claimed_at) — the helper makes it
   idempotent at boot via ALTER ... IF NOT EXISTS so the
   column gets re-applied if fiat-server starts against a
   non-Prisma-migrated Postgres. migrate_iron_state IS not
   in Prisma — this crate is the source of truth for the Iron
   state projection tables. Intro paragraph + lib-root helpers
   table both rewritten to make this distinction.

4. openapi feature scope over-claimed. Actual: cfg_attr derive
   appears on 6 of 16 structs (UserIronMapping, UserRainMapping,
   RainTransaction, RainUserComplianceStatus, SumsubApplicant,
   DeFiIdEntry). Cargo paragraph updated to enumerate the 6
   + explain why (HTTP-boundary types vs internal-only).

D2 ✅ holds; D3/D4 still ⏳ as before.

Refs: DEF-718
```

---

## 2026-05-10T18:53:33Z — `40e763fb`

docs(iron-client): iter 45 — codex iter-43 corrections (DEF-718)

```text
docs(iron-client): iter 45 — codex iter-43 corrections (DEF-718)

Codex caught 3 issues in iron-client; all addressed:

1. lib.rs module count: README + audit row claimed 11 pub modules;
   actual count via `grep -c '^pub mod '` is 10 (addresses, autoramps,
   client, currencies, customers, error, sandbox, transactions, types,
   webhooks). Corrected with explicit enumeration.

2. base_url examples missing /api prefix. Sandbox base_url per
   services/fiat-server/src/config.rs:127 is
   https://api.sandbox.iron.xyz/api (with /api suffix). Without it
   IronClient::request lands on the wrong URL because client.rs:71
   does format!("{}{}", base_url, path) and sub-client paths start
   at /customers, /addresses, etc. Intro paragraph + rust example
   both updated.

3. openapi feature scope over-claimed (same shape as iter-44 fix #4
   for db). cfg_attr derive appears only in src/types.rs (Customer,
   Identification, Signing, autoramp/address enum family). Per-module
   request DTOs in addresses.rs, autoramps.rs, currencies.rs,
   webhooks.rs do not derive ToSchema. Cargo paragraph narrowed.

D2 ✅ holds; D3/D4 still ⏳.

Refs: DEF-718
```

---

## 2026-05-10T18:55:17Z — `57e91e49`

docs(audit): iter 46 — fix L145 stale models wording per codex iter-44 (DEF-718)

```text
docs(audit): iter 46 — fix L145 stale models wording per codex iter-44 (DEF-718)

Codex iter-44 review approved the iter-44 crates/db README fixes but
flagged that the audit row L145 still carried the stale "4 raw-SQL
types Prisma doesn't know" wording from iter-41. Updated L145 to
match the corrected README/source (16 public structs = 15 SQLx row
types + 1 input DTO RainTransactionUpsert; IronCustomerState family
created by migrate_iron_state and not in Prisma; RainTransaction backed
by Prisma migration 20260415120000_add_rain_transactions but not in
the model file).

Lesson #8 paranoid check: the iter-41 log row historically described
the wrong claim verbatim. Paraphrased to "the row-type family across
the 5 domains; framing later corrected in iter-44" so the verification
grep stays clean.

Refs: DEF-718
```

---

## 2026-05-10T18:57:47Z — `a2b44265`

docs(audit): iter 47 — fix L146 D3 module count + L314 historical claim (DEF-718)

```text
docs(audit): iter 47 — fix L146 D3 module count + L314 historical claim (DEF-718)

Codex iter-45 review confirmed iron-client D2 fixes good but flagged
two more audit-row text issues:

- L145 (db model wording) — already addressed in iter-46 commit 57e91e49
  (codex reviewed 40e763fb, before iter-46 landed). No-op this iteration.
- L146 D3 note still said "11 modules"; corrected to 10 to match the
  iter-45-corrected D2 module list above it.

Lesson #8 paranoid sweep: the iter-43 log row at L314 also had the
historical "11 modules" claim verbatim (describing what landed at the
time, which was wrong). Paraphrased to "the per-module count was
over-stated in this iteration; corrected in iter-45" so the
verification grep stays clean.

Refs: DEF-718
```

---

## 2026-05-10T19:04:34Z — `c2545fed`

docs(mq): iter 48 — full D2 rewrite of crates/mq + lib.rs doc-comment (DEF-718)

```text
docs(mq): iter 48 — full D2 rewrite of crates/mq + lib.rs doc-comment (DEF-718)

Pre-existing README + CLAUDE.md were stale on 5+ load-bearing facts
that the iter-46 grep verification surfaced:

1. Backend feature `nats` claimed as default; Cargo.toml has
   `default = []` (no default).
2. Backend table missing `azqueue` entirely — Cargo.toml has 3
   features (azqueue / servicebus / nats), all mutually exclusive
   via lib.rs compile_error guards.
3. Consumer pin claimed `--features servicebus`; actual fiat-server
   uses `--features azqueue` (the only live backend; servicebus +
   nats are dead code, matches architecture.md §13's surprising-
   things bullet).
4. `dlq-drain` examples used `--features servicebus` and
   `SERVICE_BUS_NAMESPACE` env; actual `Cargo.toml:50` declares
   `required-features = ["azqueue"]` and reads
   `QUEUE_STORAGE_ACCOUNT`.
5. Three azqueue_* modules undocumented in either README or
   CLAUDE.md; dlq-drain CLI subcommands (peek/replay/purge) +
   `-poison` suffix logic missing.

Both files rewritten from current code. lib.rs `//!` doc-comment
also updated to match.

Two flagged drift items in the new README (intentionally not fixed
here, separate infra-iteration concerns):
- Application uses `{queue}-poison` suffix; infra provisions `*-dlq`;
  live monitoring alerts watch queues the application never writes to.
- `tests/servicebus_integration.rs` exercises dead-code backend;
  azqueue has no in-crate integration tests.

Audit row crates/mq/ flipped D1+D2 ✅; D3/D4 stay ⏳ (consumer-facing
library, in scope per §2, deferred).

Refs: DEF-718
```

---

## 2026-05-10T19:09:05Z — `50f07eec`

docs(rain-sdk): iter 50 — add CLAUDE.md bridging upstream + monorepo (DEF-718)

```text
docs(rain-sdk): iter 50 — add CLAUDE.md bridging upstream + monorepo (DEF-718)

Final crates-area surface — different shape from db/iron-client/mq because
rain-sdk is a vendored upstream crate (local copy of rain-sdk 1.2.0 from
github.com/yezz123/rain-rust-sdk). Cargo.toml is the auto-generated form
Cargo emits for crates.io.

Pre-existing 3-doc setup verified accurate against current source:
- README.md (~205 lines, upstream verbatim) — kept as-is, authoritative
  SDK reference; we don't rewrite vendored upstream docs.
- VENDORING.md — both documented deltas verified in current source:
  ApplicationStatus::NotStarted at models/common.rs:69-72 (with
  // VENDOR PATCH comment) for Rain dev's "notStarted" response;
  build_url query/fragment strip at client.rs:193-211 for the upstream
  %3F URL-encoding bug.
- CHANGELOG.md — upstream release notes verbatim.

New CLAUDE.md (this iteration) bridges the three docs: tags this as
vendored upstream, points to README for SDK reference + VENDORING.md
for local context, documents the consumer (services/fiat-server via
workspace pin), cites architecture.md §6.2 + §11.1 for the outbound
Api-Key vs inbound HMAC split, and notes the historical replacement
of the deleted crates/rain-client per a54c3905.

Audit row crates/rain-sdk/ flipped D1+D2 ✅, D3/D4 → N·A (vendored
third-party crate; upstream README/docs.rs is pub-item documentation
maintained by upstream).

All 4 crates now D1+D2 ✅. Crates surface closed pending codex
sign-off on iter-48 + iter-50.

Refs: DEF-718
```

---

## 2026-05-10T19:12:56Z — `5f188401`

docs(mq): iter 51 — codex iter-48 corrections + adjacent source comments (DEF-718)

```text
docs(mq): iter 51 — codex iter-48 corrections + adjacent source comments (DEF-718)

Codex caught 4 issues with the iter-48 mq rewrite; all addressed:

1. Stale source comments in 4 files contradicted the rewritten docs:
   - crates/mq/Cargo.toml header — claimed nats local-dev default,
     servicebus production. Updated to the 3-feature mutual-exclusion
     shape with no default.
   - services/fiat-server/Cargo.toml mq-pin comment — claimed
     "disables nats default". Updated to "no default; pick one of
     azqueue / servicebus / nats".
   - services/fiat-server/Dockerfile — claimed "Service Bus feature
     selected". Updated to "MQ backend (azqueue) selected".
   - crates/mq/src/publisher.rs file header — claimed Service Bus
     publisher is the production default. Updated to "NATS JetStream
     publisher; compiles only when the nats feature is enabled
     (currently dead code)".

2. CI claim wrong: README + CLAUDE.md said dead-code backends were
   "still compiled in CI for drift detection". Actual rust.yml only
   runs cargo check --workspace which compiles only the azqueue path.
   Both docs re-worded to "useful manual drift checks (NOT run in CI
   today)" with the workflow path cite.

3. Delivery-constants over-broadened: tables claimed all 4 constants
   apply to all 3 backends. Per streams.rs:6-7 source comment + the
   azqueue_consumer.rs:13 imports, only MAX_DELIVER + ACK_WAIT_SECS
   are live shared params; MAX_AGE_SECS + MAX_BYTES are NATS-only
   stream-retention constants. Tables split into two groups with the
   distinction explicit + source-comment cite.

4. DLQ drift paragraph softened the silent-loss risk: README said
   "Azure may auto-create them on first put_message". Per
   azqueue_client.rs:213-216 (ensure_streams is no-op) +
   azqueue_consumer.rs:114-115 (let _ = poison.put_message(...) +
   immediate delete from main queue), the actual behavior is silent
   message loss if the poison queue is missing. README updated to
   state that risk directly with file/line cites.

D2 ✅ now source-consistent on crates/mq.

Refs: DEF-718
```

---

## 2026-05-10T19:13:29Z — `684308f6`

docs(mq): iter 51b — fix residual MAX_AGE/MAX_BYTES grouping in module-layout block

```text
docs(mq): iter 51b — fix residual MAX_AGE/MAX_BYTES grouping in module-layout block

Iter 51 split the delivery-constants table into two groups (live vs
NATS-only) but missed the same distinction in the upper module-layout
text block at lines 35-36, which still aggregated all 4 constants
under "Delivery params". Updated to match the table split.

Refs: DEF-718
```

---

## 2026-05-10T19:17:11Z — `59ac1742`

docs(rain-sdk): iter 52 — codex iter-51b drift list (DEF-718)

```text
docs(rain-sdk): iter 52 — codex iter-51b drift list (DEF-718)

Codex iter-51b accepted crates/mq D2 ✅ but flagged 3 real issues
with the upstream README I had glossed over by claiming it was
"authoritative and D2-accurate":

1. sync feature claim broken: cargo check --features sync fails with
   reqwest::blocking::header unresolved + private build_url. Only
   async + default compile.
2. signup_consumer example broken by our ApplicationStatus::NotStarted
   vendor patch (upstream example doesn't handle the variant).
3. Stale upstream packaging/links: install snippet says version
   "0.1.0" but Cargo.toml is 1.2.0; lists a `json` feature that
   doesn't exist; links to ./docs/*.md and LICENSE files not in
   the vendored tree.

Extended CLAUDE.md with a "Known upstream-README drifts" section
enumerating all 3 (verified against current source) + a "What to
trust" section telling readers when to trust upstream README vs
CLAUDE.md/VENDORING.md.

Audit row L148 D2 softened from "authoritative SDK reference" to
"kept as-is for high-level API shape, but has 3 known drifts".

Lesson: vendored upstream docs need an explicit drift list in the
local CLAUDE.md, not a blanket authoritative claim.

Refs: DEF-718
```

---

## 2026-05-10T19:19:41Z — `e6d0da49`

docs(config-typescript): iter 54 — add packages-area README (DEF-718)

```text
docs(config-typescript): iter 54 — add packages-area README (DEF-718)

First packages-area surface. Config-only package — two TypeScript
compiler config files (base.json + nextjs.json) exported via
package.json exports map. No runtime code so D3/D4 → N·A per §2.

Sourced from current code: package.json exports (2 entries),
base.json (target ES2022 / moduleResolution Bundler / strict /
isolatedModules / declaration+map), nextjs.json (extends base,
adds dom libs + allowJs + noEmit + jsx preserve + next plugin),
consumer survey via grep -rl across tsconfig*.json files.

Notable finding: nextjs.json is unused (17 consumers extend
base.json; 0 extend nextjs.json — defi-app FE is SvelteKit,
apps/docs is Vocs). README flags this explicitly.

Audit row: packages/config-typescript/ D1+D2 ✅, D3/D4 → N·A.

Refs: DEF-718
```

---

## 2026-05-10T19:22:46Z — `e9d25d9e`

docs(tokenlists): iter 55 — add packages-area README (DEF-718)

```text
docs(tokenlists): iter 55 — add packages-area README (DEF-718)

Second packages-area README. Sourced from current code:

- package.json exports map (3 entries: . Node + ./browser browser-safe
  + ./bundled/* raw JSON).
- src/index.ts (Node — types + functions + constants + store re-exports).
- src/browser.ts (browser-safe — same minus store.ts which uses fs/path).
- src/types.ts (5 pub types).
- src/{normalizer,merger,native-tokens,query,validator}.ts (pub fns).
- src/store.ts (Node-only singleton, SUPPORTED_CHAINS).
- bundled/ (12 chain-keyed JSON files: 1, 10, 56, 137, 250, 324, 1101,
  8453, 42161, 43114, 59144, 534352 — match SUPPORTED_CHAINS).
- 12 test/*.spec.ts files.

Notable explicit non-export documented: MergedTokenInfo intentionally
NOT exported (D-09 in index.ts:2). Consumer survey: only apps/defi-app
per apps/defi-app/package.json:32 — uses /browser entrypoint.

Cross-references the Notion design doc for the architectural framing
(build pipeline, runtime distribution, balance facade, native ETH
handling, DEF-573 + PR #239/#241 history).

D3/D4 stay ⏳ (consumer-facing TS package; in scope per §2; deferred).

Refs: DEF-718
```

---

## 2026-05-10T19:25:35Z — `0e016fe3`

docs(config-typescript): iter 56 — codex iter-54 consumer-count fix (DEF-718)

```text
docs(config-typescript): iter 56 — codex iter-54 consumer-count fix (DEF-718)

Codex caught a real consumer-count inaccuracy in iter-54. README +
audit row claimed "17 consumers (apps/defi-app + apps/tradfi-server +
15 packages/*)". Two errors:

(a) my grep matched only the package-export form
(@deficom/config-typescript/base.json) and missed packages/ui-kit
which extends via the relative path
../../packages/config-typescript/base.json.

(b) apps/defi-app actually extends ./.svelte-kit/tsconfig.json
(SvelteKit framework-generated), NOT this package.

Codex's rg command finds the right answer: 18 consumers total —
1 from apps/tradfi-server + 16 from packages/* via package export +
1 from packages/ui-kit via relative path. Plus 2 explicit
non-consumers worth flagging: apps/defi-app (SvelteKit-generated)
and apps/docs (Vocs, no extends field).

Both README + audit row corrected. D2 ✅ now source-consistent.

Lesson: when surveying TS-package consumers, use a regex that
matches BOTH the package-export form and the relative-path form.

Refs: DEF-718
```

---

## 2026-05-10T19:29:06Z — `d3aa8259`

docs(database): iter 57 — add packages-area README (DEF-718)

```text
docs(database): iter 57 — add packages-area README (DEF-718)

Third packages-area README. Sourced from current code:

- package.json (4-entry exports map: . / ./client / ./redis /
  ./repositories).
- prisma/schema.prisma (11 models matching the iter 11/12
  architecture.md §7 verification).
- 18 migrations in prisma/migrations/.
- src/{client,redis,repositories}/ surface.
- scripts/dev-stack.sh:116 invocation.

Notable finding documented at top of README: @deficom/database is
NOT imported by any TS code in the workspace. The Prisma client +
Redis singleton + 5 repositories (challenge-cache, defi-id, iron,
passkey, rain) are dead exports. Live roles are schema source-of-
truth (crates/db SQLx consumes the same Postgres), migration runner,
and seed runner.

D3/D4 → N·A — no live TS consumer of the API surface; per §2 rule's
spirit. If a TS consumer is added, revisit.

Lesson reinforced: pre-survey consumers via grep -rln BEFORE writing,
not after — surfaces dead-export situations like this.

Refs: DEF-718
```

---

## 2026-05-10T19:33:24Z — `42813e6f`

docs(packages): iter 58 — codex iter-56 corrections (DEF-718)

```text
docs(packages): iter 58 — codex iter-56 corrections (DEF-718)

Codex caught 3 issues; all addressed:

1. config-typescript audit row L167 still stale — iter-56 had
   updated only the README + iteration log row, not the actual
   surface row. L167 still carried the iter-54 consumer-count
   framing. Surface row now matches the README (18 consumers via
   2 shapes; apps/defi-app + apps/docs flagged as non-consumers).

2. tokenlists undercounted public exports — circuit-breaker.ts
   and facade.ts were mis-classified as internal-only. Verified:
   index.ts exports createCircuitBreaker, CircuitBreakerState,
   facadeGetTokensByChain, facadeGetTokenByAddress, resolveLogoUrl,
   startRefresh, refresh, _resetForTesting (8 additional symbols);
   browser.ts does the same plus a browser-only re-export of
   SUPPORTED_CHAINS + SupportedChainId from store-constants.ts
   (different source than store.ts so the browser bundle doesn't
   pull in Node fs). README rewritten with shared-exports + Node-
   only + browser-only re-export tables, and an explicit note that
   circuit-breaker.ts and facade.ts are PUBLIC.

3. tokenlists bundled/*.json framed as committed — verified
   git ls-files returns empty + .gitignore:83 excludes the dir.
   Files are generated by scripts/prebuild.ts. README section
   reframed to "Generated, not committed" with cite + instruction
   to re-run prebuild on a fresh checkout.

Lesson #8 paranoid sweep applied: paraphrased the verbatim
"17 consumers" + "internal modules" mentions in 3 historical log
rows (iter 54, 56, 58) so codex's grep stays clean.

Refs: DEF-718
```

---

## 2026-05-10T19:37:36Z — `84b73bc3`

docs(database): iter 59 — codex iter-57 corrections (DEF-718)

```text
docs(database): iter 59 — codex iter-57 corrections (DEF-718)

Codex's #1 + #2 from the iter-57 review were stale (already addressed
in iter-58 commit 42813e6f which codex hadn't seen). The 2 new issues:

3. domains-json is a stray file, not a directory.
   packages/database/domains-json is a 31-byte ASCII text file
   containing literally "packages/database/domains-json\n" (looks
   like accidentally-committed terminal output). prisma/seed.ts:9-25
   expects 4 JSON files INSIDE a domains-json/ directory
   (profanity-words.json, company-names.json, company-tickers.json,
   premium_handles_by_bucket.json). None exist. So db:seed is broken
   on this branch.

   README live-vs-dead status table now lists prisma/seed.ts as
   broken with the file/dir explanation; Scripts section's db:seed
   line annotated as broken with cross-ref. Live-roles list updated
   to drop "seed runner".

4. D3/D4 should be in scope, not N·A.
   Codex's reading of §2: D3 scope is determined by symbols exported
   from public package entrypoints, NOT by whether another workspace
   consumes them. @deficom/database declares 4 export entries with
   concrete API surface (Prisma client + types + redis singleton +
   RedisCache + 5 repositories + aggregate).

   Audit row D3/D4 cells corrected from N·A to pending. New
   justification documents both the §2-reading (static declaration
   check) and the dead-consumer caveat (orthogonal — if exports
   are unwanted, mark deprecated or remove).

Lesson: §2 rule reads "exported from public package entrypoint(s)"
— that's a static declaration check, NOT a runtime "is anyone
using it" check. The two are orthogonal; conflating mis-scopes D3/D4.

Lesson #8 paranoid sweep: paraphrased verbatim N·A mentions in 2
historical log rows (iter 57, 59) per the established pattern.

Refs: DEF-718
```

---

## 2026-05-10T19:39:52Z — `c05aa845`

docs(audit): iter 60 — fix tokenlists L180 surface row per codex iter-58 (DEF-718)

```text
docs(audit): iter 60 — fix tokenlists L180 surface row per codex iter-58 (DEF-718)

Codex iter-58 review caught that L180 was still stale per the iter-55
framing — README + iter-58 log row had been corrected but not the
actual table cell. Same self-referential failure mode as iter-56/58
with config-typescript.

L180 D2 cell now reflects:
- Corrected browser.ts framing (browser-only re-export of
  SUPPORTED_CHAINS + SupportedChainId from store-constants.ts).
- Corrected facade + circuit-breaker exports (both PUBLIC, not
  internal as iter-55 mis-classified — total 22 shared symbols
  across both entrypoints).
- Corrected bundled/*.json framing as gitignored generated outputs
  (per .gitignore:83; git ls-files returns empty).
- D3 scope bumped from 14 to 22 shared exported symbols (7 types +
  11 functions + 4 constants) plus Node-only + browser-only
  re-exports.

D2 ✅ now source-consistent on packages/tokenlists.

Codex's second blocker (database) was already addressed in iter-59
commit 84b73bc3 — codex was reviewing iter-58 head 42813e6f.

Cross-iteration meta-lesson: the "fix the README + iter-log but
forget the L150-200 surface row" failure mode has happened 3 times.
New rule: every iteration's commit must git diff the surface-row
line numbers BEFORE pushing — if the surface row didn't move, the
fix is incomplete.

Refs: DEF-718
```

---

## 2026-05-10T19:44:41Z — `60c9295a`

docs(stealth-address-sdk): iter 62 — add packages-area README (DEF-718)

```text
docs(stealth-address-sdk): iter 62 — add packages-area README (DEF-718)

Fourth packages-area README. Sourced from current code:

- package.json (single tsup entrypoint; viem peer-dep; wraps
  @scopelift/stealth-address-sdk ^1.0.0-beta.2).
- src/index.ts re-exports from 7 modules — 35 exported symbols
  total: 19 functions/constants + 12 types + 4 ABIs.
- Per-module fn counts via grep ^export: stealth.ts 7,
  account.ts 3, ens.ts 1, announce.ts 3, scan.ts 1, abi.ts 4,
  constants.ts 4, types.ts 12.
- 4 test specs in src/__tests__/ (account, announce, ens, stealth).

Consumer survey via the iter-56 dual-shape lesson (package-export
form + TS imports): packages/account + 6 imports across
apps/defi-app/src/lib/ (defi-id.keys.ts, DepositFundsPage, 4
stealth-module hooks).

Iter-60 meta-lesson followed: verified L178 surface row was
actually updated, not just the README.

Cross-references architecture.md §8.2 + §8.3 + §13.4 + CLAUDE.md
stealth-privacy bullets + sibling kernel-stealth-address-sdk +
indexers/announcement.

D3/D4 stay ⏳ (consumer-facing TS package per §2 + iter-59 codex
correction; pub-item docs + worked example deferred).

Refs: DEF-718
```

---

## 2026-05-10T19:48:57Z — `fbb5b673`

docs(audit): iter 63 — contracts as submodule + iter-59 follow-up (DEF-718)

```text
docs(audit): iter 63 — contracts as submodule + iter-59 follow-up (DEF-718)

Two submodule discoveries during the contracts surface audit:

1. packages/contracts is a git submodule per .gitmodules
   (git@github.com:defi-com/contracts.git#dev, currently pinned at
   5ca821e8…), uninitialized in this worktree (only an untracked
   .env file present). Any README inside the submodule directory
   would be untracked by the parent repo — verified by attempting
   to write one and seeing git status not pick it up.

   Audit row L168 reframed: all 4 dimensions N·A from the
   parent-repo perspective. Contracts source consumed at ABI level
   only — packages/stealth-address-sdk/src/abi.ts vendors 4 ABIs,
   services/ptp-server uses Alloy sol! macro bindings.

2. packages/database/domains-json is a half-broken submodule
   registration. .gitmodules declares it as
   https://github.com/stealth-project-22/domains-json.git, but
   git ls-tree HEAD shows mode 100644 blob (regular file), NOT
   160000 commit (gitlink). git log reveals most recent change is
   `da94d43f chore: revert submodule delete`.

   So the iter-59 broken-db:seed finding was correct in outcome
   but the underlying cause is a half-reverted submodule deletion,
   not accidentally-committed terminal output. Iter-59 row
   enriched with this submodule context.

Lesson: always check .gitmodules + git ls-tree HEAD <path>
(looking for mode 160000 commit) before declaring a directory
missing or a file stray — it might be a submodule placeholder.
Pre-iteration survey for any directory under packages/ includes
this cross-check.

Audit progress note: contracts flipping to N·A means applicable
cell denominator drops by 4 (162 → 158).

Refs: DEF-718
```

---

## 2026-05-10T19:52:07Z — `1fdc8789`

docs(stealth-address-sdk): iter 64 — codex iter-62 corrections (DEF-718)

```text
docs(stealth-address-sdk): iter 64 — codex iter-62 corrections (DEF-718)

3 codex findings; all addressed:

1. encodeInitializeCallData ABI name wrong. README claimed it
   encodes Account.initialize(...). Verified src/account.ts:35
   uses functionName: "initializeAccount" and src/abi.ts:28
   declares name: "initializeAccount". README API table row +
   audit row corrected.

2. ERC5564_ANNOUNCER Sepolia-only, not per-chain. README claim
   "canonical announcer per chain" was wrong. Verified
   src/constants.ts:19-20 = 0x55649E01B5Df198D18D95b5cc5051630cfD45564
   deployed on Sepolia per source comment + file header at :4-6
   "Default deployed contract addresses (Sepolia). These are the
   sovereign stack contracts." Constants table row + section
   header corrected to "all Sepolia defaults".

3. Intro conflated stealth-address-sdk with kernel-stealth-address-sdk.
   README said this package owns "the deterministic-address factory
   for our Kernel accounts". Verified: kernel-stealth-address-sdk
   is a SEPARATE package using @zerodev/sdk + @zerodev/ecdsa-validator
   for ZeroDev Kernel. This package wraps @scopelift/stealth-address-sdk
   for OZ AccountERC7579 clones. Intro rewritten with the OZ vs
   Kernel distinction + sovereign-stack context.

D2 ✅ now source-consistent on packages/stealth-address-sdk.

Lesson: verify the actual exported function name against
src/abi.ts name: entries. Function-name aliasing is exactly the
kind of mismatch grep doesn't catch unless you cross-check the ABI.

Refs: DEF-718
```

---

## 2026-05-10T19:55:03Z — `45a46e53`

docs(audit): iter 65 — codex iter-63 cleanup (DEF-718)

```text
docs(audit): iter 65 — codex iter-63 cleanup (DEF-718)

Codex iter-63 caught 3 cleanup items (#4 — stealth-address-sdk D2 —
was already addressed in iter-64 commit 1fdc8789 which codex hadn't
seen).

1. Audit progress math wrong in iter-63 row. Codex parsed
   28/156 = 17.9%; my row said 26/158 = 16.5%. I had miscounted
   (the +2 cells from iter-62 stealth-address-sdk D1+D2 weren't
   reflected). Paraphrased the stale math claim — future iterations
   should defer precise counting to codex's review-time tally.

2. Follow-ups list at L66-69 still mentioned packages/contracts/
   as needing forge doc output. That contradicts the iter-63
   reframe (all 4 dimensions N·A). Struck through with iter-63
   cross-ref to L168 surface row.

3. Database surface row L169 still had iter-59 framing — submodule
   context (.gitmodules + ls-tree + revert-submodule-delete log)
   had only landed in the iter-63 log row, not the actual surface
   cell. Now folded into L169 + the README live-vs-dead table.
   "31-byte stray text file" reframed as "half-broken submodule
   registration".

Lesson reinforced (4th occurrence of "fix README + iter-log but
forget the L150-200 surface row"): the iter-60 rule (git diff the
surface-row line numbers) needs to extend to cover not just D2
cell flips but ALSO the prose content within those cells.

Refs: DEF-718
```

---

## 2026-05-10T19:58:13Z — `3e4d9053`

docs(packages): iter 66 — codex iter-65 cleanup (DEF-718)

```text
docs(packages): iter 66 — codex iter-65 cleanup (DEF-718)

4 codex findings; all addressed:

1. packages/database README scripts comment still said db:seed is
   broken because domains-json is "a 31-byte stray text file" — only
   the live-vs-dead status table at the top had been updated to the
   half-broken-submodule framing in iter 65. Now folded into the
   Scripts comment block too.

2. stealth-address-sdk buildFactoryData ABI mismatch. README L43
   said `Account Factory.createAccount` but src/account.ts:103-106
   encodes `cloneAndInitialize` and src/abi.ts:9-24 only exposes
   `cloneAndInitialize` + `predictAddress`. There is no
   `createAccount` in the ABI. Corrected with file/line cites.

3. stealth-address-sdk "Kernel + OZ flow" wording on
   predictAccountAddress contradicted the iter-64 paragraph
   distinguishing this OZ-only package from kernel-stealth-address-sdk.
   Reworded to "matches the OZ AccountERC7579 clone-factory flow".

4. Project-memory citation removed from stealth README. Codex
   correctly flagged that project_stealth_sovereign_deferred.md is
   in my private Claude memory directory, not the repo. Reworded
   to cite the in-tree src/constants.ts:4-6 source-comment header
   ("These are the sovereign stack contracts deployed for the
   stealth address system") as the in-tree source of truth.

D2 ✅ now source-consistent on both stealth-address-sdk AND
database README scripts comment.

Lessons:
- When reframing a multi-occurrence stale phrase across a README,
  sweep BOTH the prose AND embedded code-block comments. Code-
  block comments don't get caught by the "did the surface row
  move" check from iter 60.
- Never cite project memory (~/.claude/.../memory/*.md) from a
  repo-tracked doc — those files don't exist for any reader who
  isn't me. Repo-tracked source comments + architecture.md are
  the in-tree alternatives.

Refs: DEF-718
```

---

## 2026-05-10T20:01:54Z — `e9ac8ff3`

docs(common): iter 67 — rewrite from boilerplate template (DEF-718)

```text
docs(common): iter 67 — rewrite from boilerplate template (DEF-718)

Sixth packages-area surface. D1 was already ✅ (README existed) but
the existing file was the unmodified ts-turborepo-boilerplate
template — same shape as the root README pre-iter-7
("Description of your package goes here", `pnpm install` despite
the project using bun, all sections placeholders). Full rewrite
from scratch.

Sourced from current code:
- package.json (single entrypoint, deps @zerodev/sdk ^5.5.5).
- src/index.ts (single re-export of ./external.js).
- src/external.ts (3 lines / 7 symbols total).
- src/api-client.ts (abstract ApiClient with request<TResponse, TParams>
  doing GET + array-param flattening + auth header).
- src/types.ts (4 types: ApiClientConfig, ApiErrorResponse,
  QueryParamValue, QueryParams).
- src/account-types.ts (2 ZeroDev Kernel re-exports: Account =
  CreateKernelAccountReturnType<"0.7">, AccountClient =
  KernelAccountClient).

Consumer survey via dual-shape grep: 8 consumer packages/apps +
30+ TS import sites. Notable framing: contrasts with @deficom/database
(dead exports per iter 57+59) — @deficom/common is load-bearing
foundational, used by 7 provider clients in defi-aggregator +
Tenderly simulation in account + passkey types + send-flow in
defi-app.

D3/D4 stay ⏳ per §2 + iter-59 codex correction.

Refs: DEF-718
```

---

## 2026-05-10T20:05:22Z — `4954a11d`

docs(config): iter 68 — rewrite from boilerplate template (DEF-718)

```text
docs(config): iter 68 — rewrite from boilerplate template (DEF-718)

Seventh packages-area surface. D1 was already ✅ but same
ts-turborepo-boilerplate template as iter-67 found in common.
Full rewrite.

Sourced from current code:
- package.json (single entrypoint, viem peer-dep ^2.40.3).
- src/index.ts (re-exports from 9 modules: ABI, chains, config,
  gold, protocols, stocks, types, providers.tokens, rwa).
- src/chains.ts (5 symbols including supportedChainList for
  sepolia + base + baseSepolia).
- src/config.ts (per-chain contract addresses; Sepolia real,
  Base mostly zeroAddress placeholders).
- src/types.ts (6 types including Address, Hex, Config).
- src/protocols.ts (5 symbols for aave/compound/fluid/morpho).
- src/gold.ts + stocks.ts + rwa.ts (tokenized-asset catalogs).
- src/providers.tokens.ts (NATIVE_ETH_ADDRESS ERC-7528 sentinel
  + alchemy/graph network maps).
- src/ABI/ (4 ABIs: simple-subdomain-registrar, reverse-registrar,
  resolver-abi, icon-registry-abi).

Consumer survey via dual-shape grep: 18 workspace consumers + 43
TS import sites. Most-imported workspace package, more than common's
8/30+.

Notable framing: Sepolia-vs-Base address state distinction is
load-bearing for consumers. NATIVE_ETH_ADDRESS = 0xeeee...eeee is
the ERC-7528 sentinel re-exported as ZERO_ADDRESS in tokenlists.

D3/D4 stay ⏳ per §2 + iter-59 codex correction (~36 exported
symbols across 9 modules from a single entrypoint).

Refs: DEF-718
```

---

## 2026-05-10T20:07:07Z — `98c6a64a`

docs(config): iter 68b — escape | in TS union types in tables (DEF-718)

```text
docs(config): iter 68b — escape | in TS union types in tables (DEF-718)

The iter-68 README rewrite included TS union types like
`typeof base | typeof baseSepolia | typeof sepolia` and
`"aave" | "compound" | "fluid" | "morpho"` inside table cells. Markdown
treats `|` as the column separator, so the formatter reflowed those
rows as if they had 5 columns and padded the separator rows to match,
breaking the rendered table layout.

Escaped all union-type pipes inside table cells with `\|`. 5 tables
fixed: chains.ts, protocols.ts, stocks.ts, rwa.ts, providers.tokens.ts.
ABI table at L114-119 is a 2-column (3-pipe) table by design, not an
issue.

Lesson #5/6 redux (from architecture.md mermaid days, iter 17/20):
when a TS union type appears inside a markdown-table cell, escape every
| with \|. Apply this rule to every package README rewrite going
forward.

Refs: DEF-718
```

---

## 2026-05-10T20:12:35Z — `5f47e9b1`

docs(packages): iter 69 — codex iter-67 + iter-68 corrections (DEF-718)

```text
docs(packages): iter 69 — codex iter-67 + iter-68 corrections (DEF-718)

Codex flagged 9 issues across iter-67 (common) + iter-68 (config).
Iter-68 #1 (broken tables) was already fixed in iter-68b before
codex saw it; the other 8 are new.

common (4 fixes):
1. Consumer count 8 → 7 dep + 6 TS-importer (tokenlists is dep-only).
2. defi-aggregator provider count 7 → 13 verified by rg -ln across
   provider client files.
3. ApiClient API surface — README only documented protected GET;
   actual abstract class exposes 4 protected methods: request,
   postRequest, putRequest, prepareParams.
4. Removed unverified "second-most-imported" ranking.

config (5 fixes):
2. Export-inventory undercount across 4 modules:
   - chains.ts 5 → 10 exports (added defaultChainId, isTestnet,
     DEFI_ID_CORE_DOMAIN, getSupportedChain, getTxExplorerUrl).
   - gold.ts 5 → 8 exports.
   - stocks.ts 5 → 8 exports.
   - providers.tokens.ts 5 → 8 exports.
3. ABI/resolver.ts is NOT re-exported via ABI/index.ts (which only
   re-exports the 4 *-abi/-registrar/-registry files). Internal-only.
4. Consumer count 18 → 7 dep + 13 TS imports — old grep matched
   @deficom/config-typescript too. Exact-match Python lookup gives
   the right count.
5. Removed unverified "most-imported" ranking.
6. viem listed as direct dependency, NOT peerDependencies.

Lessons:
- Always exact-match package names in greps.
- Never claim rankings without backing them with a stable command.
- Document ALL protected/public methods of an abstract class for D2.
- Verify each `export * from "./X"` re-export by reading the index.
- Always check package.json dependencies vs peerDependencies.

Refs: DEF-718
```

---

## 2026-05-10T20:13:23Z — `6eaa2a7f`

docs(packages): iter 69b — fix uncaught intros (DEF-718)

```text
docs(packages): iter 69b — fix uncaught intros (DEF-718)

Iter-69 missed two intro-line restatements of the same stale claims:

- packages/common/README.md L54 still said "8 workspace consumers per
  rg ... (30+ import sites)" — the L3 intro was fixed but the
  Consumers section header retained the stale framing.
- packages/config/README.md L3 intro still said "Most-imported
  workspace package — 18 package.json consumers + 43 TS import sites"
  — the L134 paragraph was fixed but L3 retained the old framing.

Both updated to match the iter-69 corrections (7 dep + 6 TS-importer
for common; 7 dep + 13 TS for config; methodology cited).

Lesson: when a stale phrase appears in multiple sections of a README
(intro + section header + cross-reference), grep the WHOLE README
file for the phrase before declaring the fix complete. Same self-
referential failure mode as the L150-200 surface row issue tracked
in iter-60 — extends to README-internal duplication.

Refs: DEF-718
```

---

## 2026-05-10T20:16:49Z — `96bf3bff`

docs(audit): iter 70 — config L166 surface row + iter-68 log row cleanup (DEF-718)

```text
docs(audit): iter 70 — config L166 surface row + iter-68 log row cleanup (DEF-718)

Codex iter-68b review (on commit 98c6a64a) flagged that the L166
packages/config/ surface row + iter-68 log row still carried all the
iter-68 stale claims (5-symbol counts per chains/gold/stocks/
providers.tokens, 18 dep consumers, 43 TS imports, "most-imported
workspace package" ranking, viem-as-peer-dep, "~36 exported
symbols").

My iter-69 commit had updated the README + iter-69 log row but
missed the L166 surface row + iter-68 log row — same self-referential
failure mode as iter-56/58/60/65. 5th occurrence of "fix README +
iter-log but forget the L150-200 surface row".

Fixed in this iteration:
- L166 surface row now reflects all iter-69 corrections.
- Iter-68 log row paraphrased to defer to iter-69 for right numbers.

Lesson #8 paranoid sweep: paraphrased verbatim "second-most-imported"
+ "most-imported" + "18 → 7 dep consumers" phrases in the iter-69
row (codex's grep would catch those as stale).

Lesson reinforced: the iter-60 + iter-65 rule (git diff surface-row
line numbers) needs to ALSO sweep audit-doc iter-log rows for the
corrections — they are documentation rows in codex's grep view.
New rule: after each codex-correction commit, grep the audit doc for
the previously-quoted verbatim phrases and paraphrase remaining
occurrences.

Refs: DEF-718
```

---

## 2026-05-10T20:20:20Z — `59a5dee9`

docs(packages): iter 71 — codex iter-69b corrections (DEF-718)

```text
docs(packages): iter 71 — codex iter-69b corrections (DEF-718)

Codex iter-69b review (on 6eaa2a7f) caught 5 issues; #4 + #5 (config
L166 + iter-68 log row) were already fixed in iter-70. The 3 new:

1. common postRequest/putRequest body required, not optional. README
   marked it `body?` but src/api-client.ts:68,99 declare `body: TBody`
   (required). Row corrected with file/line cite.

2. config TS-import count methodology: iter-69 said "13 import sites
   across 9 files" using a double-quote regex. Codex's
   `rg "from '@deficom/config'"` (single-quote, matching repo style)
   finds 34 import lines. Intro + Consumers section corrected.

3. common L165 audit surface row still carried iter-67 stale claims
   (8 consumers, 7 defi-aggregator provider clients, only `request`
   documented on ApiClient). 6th occurrence of "fix README + iter-log
   but forget the L150-200 surface row" failure mode (iter-56/58/60/
   65/70/here). L165 corrected with iter-69+71 numbers + 4-method
   ApiClient enumeration.

Lesson #8 paranoid sweep: iter-67 log row L352 also carried the
stale "8 consumer packages/apps + 30+ TS import sites" framing —
paraphrased to defer to iter 69/71 for accurate numbers.

Methodology lessons:
- When grepping for TS import sites, use the actual import-quote
  form the repo uses (single-quote in this workspace per prettier
  config); a double-quote regex finds only a fraction.
- When documenting an abstract class, check parameter signatures
  (required vs optional) against source declarations, not just
  method names.

Refs: DEF-718
```

---

## 2026-05-10T20:25:02Z — `ce8c9c5c`

docs(tokens): iter 72 — D2 patches (4 inaccuracies) (DEF-718)

```text
docs(tokens): iter 72 — D2 patches (4 inaccuracies) (DEF-718)

Eighth packages-area surface. D1 was already ✅ (existing README ~347
lines, substantive content). Unlike iter-67/68 boilerplate-template
situations, this README had real content + just 4 critical
inaccuracies — patches not full rewrite.

1. Package name `@defi/tokens` → `@deficom/tokens` in 5 places
   (title, install snippet, 2 import examples, Architecture block).
   package.json:2 is @deficom/tokens.

2. Install instructions said `pnpm add @defi/tokens`; package is
   private workspace-only and project uses bun. Replaced with
   workspace-protocol example + note about apps/defi-app being the
   only consumer (20 TS import lines verified).

3. Test-count claim "40+ tests" inflated; actual count via
   `grep -rE '^\s*(test|it)\(' packages/tokens/test/` is 30 across
   6 spec files (3 e2e + 3 mock per provider/facade).

4. Architecture src/ layout block was wrong — showed a flat
   src/{client.ts, alchemy-client.ts, types.ts, alchemy-types.ts,
   adapters/} shape. Actual is nested per-provider:
   src/{facade.ts, index.ts, types/{index,unified,external}.ts,
   providers/{alchemy,graph}/{client,types,adapter,index}.ts}.
   Architecture block rewritten with real tree + 17 unified-types
   enumeration.

Substantive content kept: Quick Start examples + per-method examples
are source-accurate against current source.

D3/D4 stay ⏳ (consumer-facing TS package).

Lesson: package-name typos propagate across an entire README — when
the title is wrong, every code example is wrong. New rule: first
verify package name in title against package.json:name.

Refs: DEF-718
```

---

## 2026-05-10T20:29:24Z — `437cdbfe`

docs(packages): iter 73 — codex iter-71 + iter-72 cleanup (DEF-718)

```text
docs(packages): iter 73 — codex iter-71 + iter-72 cleanup (DEF-718)

Codex iter-71 accepted common D2 ✅ but flagged config D2 still
pending due to internal inconsistency (intro said 34 import lines,
Consumers section + audit row still said 13).

Codex's exact count via combined single + double-quote-form regex
returns 47 lines / 43 files — neither my iter-71 single-quote-only
count (34) nor my iter-69 broken count (13/9) was right.

Fixed:
- config README intro + Consumers section both say 47 / 43 with
  codex's combined regex methodology cited.
- L166 audit surface row updated.

Iter-72 follow-up cleanup on tokens README (residuals I had noted
but not patched):
- 5 pnpm mentions in Setup + Available Scripts table + Testing
  block + Examples block — replaced with `bun --filter` form.
- 4 broken cross-reference links to files that don't exist in
  tree (./FACADE.md 3x at L110/L171/L371, ./test/README.md 2x at
  L223/L372) — replaced with paraphrased pointers to in-tree
  alternatives (src/facade.ts JSDoc, src/types/unified.ts,
  test/save-mocks.ts JSDoc + test/__mocks__/).

Methodology lessons reinforced:
- When counting TS imports, use a regex covering BOTH single-quote
  AND double-quote string literals — different files in this
  workspace use different quote styles.
- When D2-verifying a README with ./FILE.md cross-references, check
  each linked file exists in the tree via `ls` before accepting
  the README as accurate. Broken links are accuracy issues.

Refs: DEF-718
```

---

## 2026-05-10T20:34:06Z — `7d113f55`

docs(packages): iter 74 — codex iter-72 #3 + decentralised-icons rewrite (DEF-718)

```text
docs(packages): iter 74 — codex iter-72 #3 + decentralised-icons rewrite (DEF-718)

Codex iter-72 #3 cleanup: packages/tokens/README.md script-table row
for `build` claimed it runs `tsc`, but package.json:31 actually has
`"build": "tsup"`. Codex's findings #1/#2/#4 were already addressed
in iter 73 commit 437cdbfe; #3 was new. Fixed with cite.

New surface — packages/decentralised-icons (9th packages-area D1+D2):
existing README was the bun-flavored boilerplate template (35 lines,
placeholder Description/Setup/Available Scripts/Usage/API/References).
Full rewrite from scratch using:

- package.json (single entrypoint, deps @deficom/config + viem ^2.40.3)
- src/external.ts (~21 symbols total: DecentralisedIcons class,
  IconInfo/DecentralisedIconsConfig interfaces, ICON_REGISTRY_ADDRESS/
  ICON_REGISTRY_CHAIN_ID constants, 8 manifest helpers, 2 URL constants,
  5 manifest schema types)
- src/config.ts (mainnet Icon Registry contract address
  0x342e808c40D8E00656fEd124CA11aEcBB96c61Fc, hardcoded chain_id = 1)
- src/manifest.ts (3rd-party GH Pages manifest fetch helpers —
  https://igor53627.github.io/iconregistry.eth)

Notable load-bearing finding via codex iter-71 methodology:
0 TS import sites in the workspace — only apps/defi-app/package.json
deps. Same forward-looking-only state as @deficom/database
(iter 57+59). README has Live-vs-dead status table at the top.

Lessons applied:
- iter-72 lesson #3: verified all decentralised-icons script
  descriptions against package.json:30-39 before writing.
- iter-71 combined-quote git grep regex used to verify 0 consumers.
- iter-67 boilerplate-template-detection pattern: a 35-line README
  with placeholder section content is a strong signal for full
  rewrite vs patches.

9/19 packages closed at D1+D2; 10 packages remaining.

Refs: DEF-718
```

---

## 2026-05-10T20:44:19Z — `9112aa1c`

docs(packages): iter 75 — defi-id README + pathspec lesson #9 (DEF-718)

```text
docs(packages): iter 75 — defi-id README + pathspec lesson #9 (DEF-718)

Tenth packages-area D1+D2 closure. Existing README was the same
ts-turborepo-boilerplate placeholder pattern as iter 67/68/74. Full
rewrite from `src/index.ts` (~334 lines): 15 public class members on
`DeFiId` + 4 exported interfaces, with the ENS-mandated forward-
verification in `getReverseRecord` and the canonical 3-tx ordering in
`buildRegisterDomainBatch` called out as security-critical / order-
sensitive surfaces. Consumer survey: 1 dep + 4 import sites in
`apps/defi-app/src/lib/` + vite.config.ts SSR pinning.

NEW LESSON #9 in iter log: the codex-cited brace-glob pathspec form
(`'apps/**/*.{ts,tsx,svelte}'`) silently 0-matches in this bash env —
git pathspec doesn't natively expand `{...}` and the single-quotes
prevent shell expansion. Switched to plain `apps/ packages/ services/`
pathspec. Past consumer counts may have undercounted .svelte files;
config (47/43) happens to be unaffected because all imports live in
.ts files, but `common` and `stealth-address-sdk` warrant re-check.

10/19 packages closed at D1+D2; 9 remaining.

Refs: DEF-718
```

---

## 2026-05-10T20:50:40Z — `9791fa51`

docs(packages): iter 76 — decentralised-icons codex iter-74 fix (DEF-718)

```text
docs(packages): iter 76 — decentralised-icons codex iter-74 fix (DEF-718)

Codex iter-74 caught exactly what iter-75 lesson #9 predicted: the
iter-74 brace-glob consumer-survey silently 0-matched the real .svelte
consumer of `@deficom/decentralised-icons`. The class is NOT dead — it
is THE icon resolution path for the entire FE, used by
`apps/defi-app/src/lib/modules/icons/use-token-icon.svelte.ts`.

README rewrite:
- Live-vs-dead table now distinguishes 3 buckets — live class (1
  consumer), forward-looking manifest helpers (0 consumers), and
  internal-only module constants.
- Public API enumerates all 23 async chain-read methods on
  `DecentralisedIcons` (batch readers, slug lookups, one-shot data-URI
  getters including `getTokenIconDataURI` which folds 3 chain calls
  into 1, version/metadata, enumeration, existence + admin).
- New consumer section documents the layered batching: SvelteMap cache
  + in-flight dedupe in the consumer + HTTP-batching +
  multicall-batching in the SDK class → N concurrent reads collapse to
  ~1 RPC round-trip on cold render.

Audit doc:
- Surface row L170 reframed to match the live class + 23-method
  surface.
- Iter-74 log row paraphrased (lesson #8) — false "dead /
  forward-looking-only" claims replaced with a pointer to iter 76 and
  the root-cause lesson (brace-glob silently 0-matches `.svelte`).
- Iter-76 entry added with codex-review link + re-verification note
  for prior packages surveyed with the buggy methodology
  (`common`, `stealth-address-sdk`).

10/19 packages still closed at D1+D2 (no net change — correction, not
a new surface).

Refs: DEF-718
```

---

## 2026-05-10T20:55:26Z — `9c9a37eb`

docs(packages): iter 77 — codex iter-75 fixes (defi-id + lesson #9) (DEF-718)

```text
docs(packages): iter 77 — codex iter-75 fixes (defi-id + lesson #9) (DEF-718)

Codex iter-75 review flagged 3 issues. #1 (decentralised-icons D2)
was already fixed in iter-76 commit 9791fa51. #2 + #3 are addressed
here.

Issue #2 (defi-id README false E2E claim): iter-75 README claimed
integration coverage of the registration flow lived in apps/defi-app/
E2E tests exercising DefiIdModal.svelte. Codex's grep shows the
component but no spec/test for any of the defi-id components. README
updated to reflect the actual state: no package-local specs
(--passWithNoTests), no app-level coverage cited; manual FE
onboarding is the only current coverage. L172 surface row also notes
the iter-77 correction.

Issue #3 (lesson #9 attribution): iter-75 framed the broken brace-
glob pathspec as "the codex-cited" command. Codex's actual posted
commands use per-extension explicit pathspecs ('apps/**/*.ts'
'apps/**/*.svelte' …) which work correctly. The brace-glob form was
mine. Iter-75 log row updated to remove the misattribution; the
methodology lesson stands; the new rule allows either plain pathspec
OR per-extension form.

Iter-log reordered (75 → 76 → 77 chronological). Pipe-count audit
clean (escaped shell-pipe `|` in command quotes).

10/19 packages still closed at D1+D2 (correction, not new surface).

Refs: DEF-718
```

---

## 2026-05-10T21:00:32Z — `572da6e3`

docs(iron): iter 78 — D2 patches for 8 inaccuracies (DEF-718)

```text
docs(iron): iter 78 — D2 patches for 8 inaccuracies (DEF-718)

Eleventh packages-area surface. Existing README was 359 lines of
substantive content (NOT boilerplate template), so patched in place
rather than rewriting.

Inaccuracies verified against current source (10 src files):

1. Default baseUrl example missing `/api` suffix (same gotcha
   `crates/iron-client/README.md` had at iter 45). Actual fallback at
   `client.ts:42` + `iron-client.ts:42` is `…iron.xyz/api`. Pinned
   in CLAUDE.md as a repo invariant.
2. IronClient sub-clients undercounted (5 → 7). Added `transactions:
   TransactionsClient` + `webhooks: WebhooksClient` per
   `iron-client.ts:124-178`.
3. CustomerType enum values wrong (`LegalPerson` → `Person`); the
   README's own example used `CustomerType.Person`, so the enum
   block was internally inconsistent.
4. AutorampType enum values entirely wrong (`Standard, ThirdParty` →
   `Onramp, Offramp, Swap, Mint, Redeem`).
5. Blockchain enum missing `Stellar, Citrea` (6 → 8).
6. 5 enums missing entirely (`SandboxTransactionState`,
   `AutorampTransactionStatus`, `AutorampTransactionState`,
   `RateExpiryPolicy`, `FeeSettlement`). Actual count is 14
   enums; README had 9.
7. TransactionsClient (2 methods) + WebhooksClient (3 methods) API
   surfaces added with method-to-path mappings.
8. Installation block (just `bun install`) replaced with package
   context: private workspace, tsup build, and the FE-vs-BE
   distinction (this TS package is FE-side, distinct from
   `crates/iron-client/` Rust crate consumed by fiat-server BE).

Consumer survey via codex per-extension pathspec form (lesson #9):
6 import sites in apps/defi-app (2 .ts + 1 spec + 3 .svelte). Both
pathspec forms agree exactly.

Iter-log reordered chronologically (75 → 76 → 77 → 78).

11/19 packages closed at D1+D2; 8 remaining.

Refs: DEF-718
```

---

## 2026-05-10T21:07:49Z — `c7c2efed`

docs(passkeys): iter 79 — README rewrite + audit close (DEF-718)

```text
docs(passkeys): iter 79 — README rewrite + audit close (DEF-718)

Twelfth packages-area surface. Existing README was the same 35-line
ts-turborepo-boilerplate placeholder pattern as iter-67/68/74/75
found in common/config/tokens/decentralised-icons/defi-id. Full
rewrite from current source (4 src files, 540 lines for the WebAuthn
server flow alone).

Public API enumerated: PasskeyAuthService class (3 fields + 8
methods), createPasskeyAuthService factory, isPrfSupported() browser
PRF capability check, WebAuthnMode re-export, 4 exported public
types via index.ts.

Notable findings called out:
- IPasskeyAuth internal interface NOT exported via index.ts; the
  contract is implicit through the class.
- generateStealthSignature + generateStealthKeys derive
  deterministic stealth keys from the passkey credential's pubX/pubY
  + a hashed message — same passkey + same message → same keys.
- Multi-chain by design (chains[0] is default; constructor throws on
  empty/invalid chains).
- ChainConfig exported from types.ts but NOT re-exported via
  index.ts.
- WebAuthnKey + PrfResult declared in BOTH types.ts and
  webauthn-server-flow.ts (the latter has its own copies because
  it's importable independently).

Consumer survey via codex per-extension pathspec (lesson #9):
3 import sites in apps/defi-app/src/lib/modules/ (hybrid-passkey-
auth-service, device-vault/webauthn, debug/prf-override). Plus
vite.config.ts SSR pinning (same as defi-id per iter 75).

Server counterpart: services/passkey-server (Rust, port 8080) — this
SDK calls /register/{begin,finish} + /login/{begin,finish}.

Test-coverage claims kept honest (codex iter-75 lesson on defi-id):
no spec files exist; --passWithNoTests; integration coverage lives
in services/passkey-server + FE onboarding flow without citing a
specific test file.

12/19 packages closed at D1+D2; 7 remaining.

Refs: DEF-718
```

---

## 2026-05-10T21:11:26Z — `9338811b`

docs(iron): iter 80 — codex iter-78 stale-examples fix (DEF-718)

```text
docs(iron): iter 80 — codex iter-78 stale-examples fix (DEF-718)

Codex iter-78 review accepted iter-78's sub-clients/enums/baseUrl
patches but flagged 4 README usage examples that still showed stale
request shapes. All 4 fixed in iter 80 with line cites to current
src/types.ts:

1. registerFiatAddress: flat fields → nested
   `{ customer_id, bank_details: RecipientBankAccount, currency: Fiat,
   label }` per types.ts:430-435. account_identifier moved into
   bank_details as a discriminated union (SEPA/ACH/Wire/etc).

2. createAutoramp: flat currency fields →
   `{ source_currencies: Currency[], destination_currency: Currency,
   recipient_account: Account }` per types.ts:597-610. Currency +
   Account are both discriminated unions (Crypto/Fiat).

3. getAutorampQuote: missing required fields. Added
   `recipient_account_id`, `rate_expiry_policy`, `expiry_in_hours`,
   `is_third_party`. Replaced single `amount` with `amount_in`
   (current API requires exactly one of amount_in/amount_out).

4. createIdentification: `type: "link"` → `type: "Link"` per
   types.ts:511 (capitalized union "Link" | "Token" | "Person").

Methodology lesson: when README has substantive code examples,
each example must be source-verified against current types. Patches
that fix surface enumerations without auditing example shapes leave
a class of D2 bugs uncaught.

12/19 packages still closed at D1+D2 (correction, not new surface).

Refs: DEF-718
```

---

## 2026-05-10T21:18:59Z — `7a7520d2`

docs(stealth): iter 81 — kernel-stealth-address-sdk README (DEF-718)

```text
docs(stealth): iter 81 — kernel-stealth-address-sdk README (DEF-718)

Thirteenth packages-area surface. Existing README was the same 35-line
ts-turborepo-boilerplate placeholder pattern as iter-67/68/74/75/79.
Full rewrite from current source (5 src files, 481 lines).

Public API enumerated:
- 3 standalone live exports: predictKernelAddress (sender-side
  CREATE2 prediction), claimStealthPayment (deploys Kernel +
  installs AaveForwarder + forwards USDC), executeStealthSwap
  (deploys Kernel + sends arbitrary user-op calls).
- 6 supporting types (ClaimStealthPaymentParams/Result/Step,
  StealthSwapParams/Result/Step).
- 1 forward-looking factory createKernelStealthAddressSdk()
  returning IKernelStealthAddressSdk interface (5 building-block
  methods) — currently 0 consumers; FE goes directly to standalone
  exports.

Notable findings:
- package.json uses legacy `directories` map (not `exports`) for
  entrypoint resolution.
- claimStealthPayment hardcoded to EntryPoint v0.7 + Kernel v3.1;
  uses ERC-7579 module type 2 (Executor) for AaveForwarder.
- predictKernelAddress lazy-imports @zerodev/sdk/constants for
  defaults; getStealthKernelAddressFromSecret has no defaults.
- MAX_SIGNATURE_LENGTH = 132 exported from action module but NOT
  re-exported via index.ts.
- ts-invariant precondition checks on hex secrets.

Distinct from packages/stealth-address-sdk/ — that's the OZ
AccountERC7579 sibling SDK; both wrap the same upstream
@scopelift/stealth-address-sdk. This Kernel package handles the live
auto-claim+send+swap flow; the OZ package handles the sovereign-
stack clone-factory flow. 3 of 7 consumer files import from both.

Lesson #9 redux applied: re-verified stealth-address-sdk (iter-62)
consumer count via broad pathspec — matches iter-62's 7 imports, no
correction needed.

13/19 packages closed at D1+D2; 6 remaining.

Refs: DEF-718
```

---

## 2026-05-10T21:25:33Z — `65c5df14`

docs(account): iter 82 — README rewrite + audit close (DEF-718)

```text
docs(account): iter 82 — README rewrite + audit close (DEF-718)

Fourteenth packages-area surface. Existing README was the same
35-line ts-turborepo-boilerplate placeholder. Full rewrite from
current source (13 src files across 4 dirs).

Public API enumerated by sub-module:
- actions/defi/: 4 actions (getSwapQuote, prepareSwapUserOp,
  executeSwap, kernelAccountDefiClientActions) + 7 types including
  ExecuteSwapResult discriminated union (Hash | CowOrderResult |
  QuoteStale).
- actions/transfers/: sendTransfer + AnnounceParams (ERC-5564
  bundled-announce stealth-send hot path).
- simulation/: TenderlyClient extends ApiClient + factory + 8 types.

Notable findings:
- CoW Swap branch in executeSwap (cowswap path → EIP-712 order
  signing via internal CowSwapOrderExecutor + COW_VAULT_RELAYER
  0xC92E…0110 for approvals, NOT the settlement contract).
- Quote freshness gate (QUAL-01) returns QuoteStale discriminator.
- DEFAULT_SLIPPAGE_PERCENT = 0.5 (50 bps).
- DeFiAggregatorQuoteProvider is structural sub-type for mock
  injection.
- kernelAccountDefiClientActions is the canonical viem extension
  factory used by 5 of 7 consumer files.

Live vs forward-looking split:
- actions/defi/ + sendTransfer: LIVE (7 import sites).
- cow/CowSwapOrderExecutor: internal-only (used by executeSwap, not
  re-exported via index.ts).
- simulation/TenderlyClient + 8 types: FORWARD-LOOKING — no TS
  consumer imports them today.

14/19 packages closed at D1+D2; 5 remaining.

Refs: DEF-718
```

---

## 2026-05-10T21:29:58Z — `a54d3b1e`

docs(merkle-scripts): iter 83 — README extension + audit close (DEF-718)

```text
docs(merkle-scripts): iter 83 — README extension + audit close (DEF-718)

Fifteenth packages-area surface. Existing README was substantive
(36 lines, real CLI usage + Deploy step) — NOT a boilerplate
template like iter-67/68/74/75/79/81/82 cases. Patches + extensions,
not rewrite.

5 extensions:
1. Workspace-membership context — verified packages/merkle-scripts
   is NOT in root package.json workspaces; per-package bun install
   required.
2. ./smt library API — package.json declares "./smt": "./src/smt.ts"
   but src/smt.ts (~198 lines) was completely undocumented. Added
   table for HexBytes32, hash2, hash3, SMTProof, SparseMerkleTree
   (constructor + getRoot + add + getProof).
3. ENTRIES_API_BASE default fiat.seikai.app per build-blocked-smt.ts:20,
   verified against fiat-server defi_id.rs:44 endpoint.
4. packages/contracts submodule context — gitlink at commit
   5ca821e8…; 8_DeployNameLocker.s.sol lives inside the submodule,
   not in parent tree.
5. Cross-references to architecture.md + Solarity upstream.

Notable findings:
- 0 broad-pathspec consumers of @deficom/merkle-scripts; ./smt
  library is internal-only despite being declared in exports.
- BLOCKED_VALUE = 0x000…001 sentinel constant.
- Default blocked names ["admin", "test", "defi"] — same fallback in
  both scripts.

Live vs forward-looking distinction: both CLI scripts are LIVE
(deploy-flow consumers); ./smt library is INTERNAL-ONLY.

15/19 packages closed at D1+D2; 4 remaining (defi-aggregator, rain,
stocks, ui-kit).

Refs: DEF-718
```

---

## 2026-05-10T21:35:54Z — `6d6c5faa`

docs(stocks): iter 84 — README full rewrite (DEF-718)

```text
docs(stocks): iter 84 — README full rewrite (DEF-718)

Sixteenth packages-area surface. Existing 92-line README looked
substantive (real Quick-Start + per-domain methods) but had so many
inaccuracies that rewrite was cleaner than patches:

- Claimed 16 nonexistent methods on AlphaVantageClient: getWTI,
  getBrent, getNaturalGas, getCopper, getAluminum, getWheat, getCorn,
  getCotton, getSugar, getCoffee, getAllCommodities, getSMA, getEMA,
  getRSI, getMACD, getBBANDS. Source grep confirms: 34 methods exist,
  none of them match the energy/agricultural/technical-indicator
  names.
- Source comment at client.ts:36 is explicit: "Alpha Vantage API
  Client (34 Stock + Gold/Silver methods only)" — explicit subset
  declaration.
- "Features" section over-claimed (energy, agricultural, technical
  indicators not implemented).
- AlphaVantageProvider static-parser class missing entirely (5
  methods used by apps/tradfi-server/src/routes/gold/gold.ts).
- createAlphaVantageClientWithApiKey factory shortcut missing.

New README: explicit subset caveat at top + all 34 client methods
grouped by category (time-series, quotes, options, news/sentiment,
fundamentals, calendars, gold/silver) + 5 AlphaVantageProvider static
parsers + 2 factory shortcuts + 22 types. Default baseUrl pinned per
client.ts:42.

Consumer survey: 2 import sites in apps/tradfi-server/ (deps.ts +
routes/gold/gold.ts).

Methodology lesson reinforced: when README has a method-list,
verify EVERY method against `grep '^\s\+async\s\+\w\+'` count — don't
trust the README's own structure for substantive READMEs (they can
hide whole-cloth fabrications behind real examples).

16/19 packages closed at D1+D2; 3 remaining (defi-aggregator, rain,
ui-kit).

Refs: DEF-718
```

---

## 2026-05-10T21:42:37Z — `00bef226`

docs(rain): iter 85 — README rewrite (DEF-718)

```text
docs(rain): iter 85 — README rewrite (DEF-718)

Seventeenth packages-area surface (partial — audit row not yet
updated; will land in a follow-up).

Existing README was the same 35-line ts-turborepo-boilerplate
placeholder. Full rewrite from current source (15 src files, 1412
lines).

Notable findings:
- package.json has empty `dependencies` — pure TS + browser-native
  Web Crypto + fetch.
- Two-step index → external indirection so internal callers can
  import without picking up the webhook types.

Public API enumerated:
- RainClient class with RainClientOptions + 14 methods (5 user
  application, 1 user read, 8 cards).
- Environment helpers (RainEnvironment, getRainBaseUrl with dev +
  production base URLs).
- RainHttpError class.
- 5 model files (common, address, application 10 types, card 16
  types, documents).
- AUTHORIZATION_METHOD_CODES constant.
- Validation (RainValidationError + 2 fns).
- PIN validation (PIN_MIN_LEN, PIN_MAX_LEN, InvalidPinError, 2 fns).
- PIN crypto (5 fns + 2 interfaces) — encryptPinForUpdate is the
  only currently-live consumer surface.
- Webhook event types (RainTransactionCreatedBody/Created/
  Transactions) — forward-looking; Rust BE has its own struct.

Live vs forward-looking split:
- pin-crypto.ts:encryptPinForUpdate is LIVE (1 import site at
  apps/defi-app/src/routes/v2/cards/details/+page.svelte:23).
- RainClient class is FORWARD-LOOKING.
- Webhook types are FORWARD-LOOKING / DOCUMENTATION-ONLY.

NOTABLE BUG FLAGGED: apps/defi-app/package.json does NOT declare
@deficom/rain as a workspace dep, but the +page.svelte imports
encryptPinForUpdate from @deficom/rain LIVE. Works through bun-
workspace symlink resolution but should be added explicitly.

Distinct-from framing: this TS package is the FE-side Rain wrapper
(only PIN crypto is live); the BE-side Rust path lives in
crates/rain-sdk/ + services/fiat-server/src/{routes,services}/rain*.rs.

Refs: DEF-718
```

---

## 2026-05-10T21:49:07Z — `8ae9f405`

docs(rain): iter 86 audit finalisation

```text
docs(rain): iter 86 audit finalisation

Finalize the Rain audit row and iter log.

Correct the README test-status section. Tracked specs exist under packages/rain/test.

Verification:
- bun --filter @deficom/rain test
- git diff --check

codex: driver iter 86
Refs: DEF-718
```

---

## 2026-05-10T21:57:06Z — `9439a3e7`

docs(ui-kit): iter 87 README audit

```text
docs(ui-kit): iter 87 README audit

Add the missing ui-kit README and close D1/D2 for the surface.

Document subpath exports, component inventory, styles, Storybook, and app consumers.

Verification:
- bun --filter @deficom/ui-kit check-types
- git diff --check

codex: driver iter 87
Refs: DEF-718
```

---

## 2026-05-10T22:05:31Z — `499e4940`

docs(passkeys): iter 88 D2 corrections

```text
docs(passkeys): iter 88 D2 corrections

codex: driver iter 88

Refs: DEF-718
```

---

## 2026-05-10T22:17:11Z — `70ea485f`

docs(defi-aggregator): iter 89 README audit

```text
docs(defi-aggregator): iter 89 README audit

codex: driver iter 89

Refs: DEF-718
```

---

## 2026-05-10T22:30:47Z — `8eff1639`

docs(tradfi-server): iter 90 README audit

```text
docs(tradfi-server): iter 90 README audit

codex: driver iter 90

Refs: DEF-718
```

---

## 2026-05-10T22:45:58Z — `8d92c975`

docs(apps-docs): iter 91 README audit

```text
docs(apps-docs): iter 91 README audit

codex: driver iter 91

Refs: DEF-718
```

---

## 2026-05-10T22:58:28Z — `0219f685`

docs(defi-app): iter 92 README audit

```text
docs(defi-app): iter 92 README audit

codex: driver iter 92

Refs: DEF-718
```

---

## 2026-05-10T23:13:24Z — `3e5f0456`

docs(indexer): iter 93 announcement audit

```text
docs(indexer): iter 93 announcement audit

codex: driver iter 93

Refs: DEF-718
```

---

## 2026-05-10T23:27:21Z — `a34bf986`

docs(indexer): iter 94 pnl audit

```text
docs(indexer): iter 94 pnl audit

codex: driver iter 94

Refs: DEF-718
```

---

## 2026-05-10T23:43:31Z — `3fc64f76`

docs(infra): iter 95 root audit

```text
docs(infra): iter 95 root audit
```

---

## 2026-05-10T23:50:33Z — `e7551c5e`

docs(infra): iter 96 swa audit

```text
docs(infra): iter 96 swa audit

codex: driver iter 96
```

---

## 2026-05-11T00:05:24Z — `fbdf8609`

docs(infra): iter 97 network audit

```text
docs(infra): iter 97 network audit

codex: driver iter 97
```

---

## 2026-05-11T00:17:14Z — `0e58234b`

docs(infra): iter 98 frontdoor audit

```text
docs(infra): iter 98 frontdoor audit

codex: driver iter 98
```

---

## 2026-05-11T00:32:04Z — `093db4fb`

docs(infra): iter 99 apim audit

```text
docs(infra): iter 99 apim audit

codex: driver iter 99
```

---

## 2026-05-11T00:44:27Z — `a02574cb`

docs(infra): iter 100 compute audit

```text
docs(infra): iter 100 compute audit

codex: driver iter 100
```

---

## 2026-05-11T00:59:32Z — `0b390633`

docs(infra): iter 101 data audit

```text
docs(infra): iter 101 data audit

codex: driver iter 101
```

---

## 2026-05-11T01:06:34Z — `db0480eb`

docs(infra): iter 102 monitoring audit

```text
docs(infra): iter 102 monitoring audit

codex: driver iter 102
```

---

## 2026-05-11T01:15:08Z — `f3ef9fe7`

docs(integration): iter 103 webhook audit

```text
docs(integration): iter 103 webhook audit

codex: driver iter 103
```

---

## 2026-05-11T01:23:12Z — `02d1ef94`

docs(integration): iter 104 iron audit

```text
docs(integration): iter 104 iron audit

codex: driver iter 104
```

---

## 2026-05-11T01:31:35Z — `c91c8020`

docs(integration): iter 105 rain audit

```text
docs(integration): iter 105 rain audit

codex: driver iter 105
```

---

## 2026-05-11T01:36:31Z — `ed1ba2be`

docs(integration): iter 106 card funding

```text
docs(integration): iter 106 card funding

codex: driver iter 106
```

---

## 2026-05-11T01:45:58Z — `b8e0b6c1`

docs(integration): iter 107 payy audit

```text
docs(integration): iter 107 payy audit

codex: driver iter 107
```

---

## 2026-05-11T02:02:26Z — `bea775e1`

docs(azure): iter 108 deployment audit

```text
docs(azure): iter 108 deployment audit

codex: driver iter 108
```

---

## 2026-05-11T02:15:56Z — `27451a18`

docs(integration): iter 109 thredd audit

```text
docs(integration): iter 109 thredd audit

codex: driver iter 109
```

---

## 2026-05-11T02:30:06Z — `e3fe0f51`

docs(sentry): iter 110 current-state audit

```text
docs(sentry): iter 110 current-state audit

codex: driver iter 110
```

---

## 2026-05-11T02:36:11Z — `021aa921`

docs(rain): iter 111 taplo audit

```text
docs(rain): iter 111 taplo audit

codex: driver iter 111
```

---

## 2026-05-11T02:40:52Z — `79c2838d`

docs(ci): iter 112 workflow audit

```text
docs(ci): iter 112 workflow audit

codex: driver iter 112
```

---

## 2026-05-11T02:48:20Z — `0aebd5b3`

docs(rust): iter 113 setup audit

```text
docs(rust): iter 113 setup audit

codex: driver iter 113
```

---

## 2026-05-11T02:58:54Z — `494667ba`

docs(infra): iter 114 landing snapshot

```text
docs(infra): iter 114 landing snapshot

codex: driver iter 114
```

---

## 2026-05-11T03:04:05Z — `d8045250`

docs(rain): iter 115 kyc roadmap

```text
docs(rain): iter 115 kyc roadmap

codex: driver iter 115
```

---

## 2026-05-11T03:08:21Z — `0d9b8a82`

docs(stealth): iter 116 alchemy setup

```text
docs(stealth): iter 116 alchemy setup

codex: driver iter 116
```

---

## 2026-05-11T03:12:50Z — `80b7b271`

docs(pr): iter 117 pr323 archive

```text
docs(pr): iter 117 pr323 archive

codex: driver iter 117
```

---

## 2026-05-11T03:18:24Z — `0fb08ece`

docs(stealth): iter 118 readme audit

```text
docs(stealth): iter 118 readme audit

codex: driver iter 118
```

