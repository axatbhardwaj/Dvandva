# Published artifact channels for an independent-harness Dvandva

Date: 2026-08-26

## Question

Can a Dvandva run hosted inside T3 Code publish durable, mobile-reachable web
artifacts without a Claude Agent SDK session launching Codex, or a Codex
session launching Claude?

## Conclusion

Yes, through ChatGPT Sites owned by the already-running Codex lane. Both lanes
keep the authoritative run plan and progress current through the baton, while
the resident Codex lane alone uses T3 Code's Sites connector to save and
privately deploy the generated Run Explainer. Claude may also author reviewed
artifact source and hand it over for publication. Neither path is a
cross-harness invocation.

Claude Artifacts should not be the automated publication channel. Anthropic
documents Artifact publishing as an interactive Claude product action, while
the Claude Agent SDK documents filesystem output, commands, hooks, subagents,
skills, MCP, and custom tools but no claude.ai Artifact creation or publication
interface.

## Verified product surfaces

### Claude

- Claude can publish consumer Artifacts through the Claude UI. Free, Pro, and
  Max accounts can publish publicly; Team and Enterprise accounts share within
  their organization. The documented flow requires opening the Artifact and
  clicking **Publish** or **Share**.
- The Claude Agent SDK exposes the Claude Code agent loop and built-in tools for
  reading, writing, and editing files, running commands, and searching the web.
  It can also load skills, hooks, subagents, plugins, and MCP tools.
- No official Agent SDK documentation found in this investigation exposes a
  claude.ai Artifact object, a create-Artifact call, or a publish-Artifact call.
  This is an absence-of-documented-surface finding, not a claim that Anthropic
  could never add one.

Sources:

- [Publish and share artifacts](https://support.claude.com/en/articles/9547008-publish-and-share-artifacts)
- [Claude Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview)
- [Use Claude Code features in the SDK](https://code.claude.com/docs/en/agent-sdk/claude-code-features)

### ChatGPT Sites

- Sites is a public-beta ChatGPT hosting product for websites, web apps, and
  games. It can start from a compatible local source project and returns a
  hosted production URL.
- A Site is persistent independently of the chat that created it. Its project
  linkage is stored in `.openai/hosting.json`.
- Publication has two stages: save a commit-associated version, then deploy a
  selected version. Saving does not make the version live.
- A new Site starts owner/admin restricted. Depending on workspace policy, it
  can remain owner-only, be shared with selected users or a workspace, or be
  made public. Public deployment is not required to get a hosted URL.
- The public documentation says Sites management is performed through ChatGPT
  web or desktop rather than a standalone Codex CLI management view.

Source:

- [ChatGPT Sites documentation](https://learn.chatgpt.com/docs/sites)

### T3 Code capability observed on 2026-08-26

The Codex harness in this T3 Code session exposes callable Sites connector
operations for:

- creating and inspecting a Site;
- obtaining a short-lived, repository-scoped source credential;
- saving a version tied to an exact Git commit and validated build archive;
- deploying an owner-only version privately;
- deploying a shared/public version after explicit approval;
- polling deployment status and retrieving the resulting URL;
- updating access controls and runtime environment values.

A read-only `list_sites` capability probe succeeded. It returned at least one
owner-visible Site; no project names, IDs, URLs, source credentials, or access
records were printed or retained in this note. No Site was created, saved,
deployed, updated, or deleted.

The relevant connector operations were:

```text
mcp__codex_apps__sites_create_site
mcp__codex_apps__sites_save_site_version
mcp__codex_apps__sites_deploy_private_site_version
mcp__codex_apps__sites_deploy_site_version
mcp__codex_apps__sites_get_deployment_status
mcp__codex_apps__sites_get_site
mcp__codex_apps__sites_update_site_access
```

This establishes current T3 Code feasibility. It does not imply the same
publication surface exists in standalone Codex CLI sessions.

## Recommended architecture

Use one persistent **Run Site** for every Dvandva run, not one repository-wide
hub and not one Site per report. Its primary surface is a live **Run
Explainer**: the agreed plan rendered as a to-do list together with current
ownership, progress, evidence, decisions, and blockers.

```text
Claude or Codex authors frozen report source
  -> peer reviews the exact content hash
  -> baton offers a publication request to the Codex lane
  -> Codex claims the request without launching another harness
  -> Codex creates or updates that run's Site
  -> Codex validates and saves a commit-pinned Sites version
  -> Codex deploys owner-only
  -> baton records the content hash, Sites version, route, URL, and access mode
```

Progress follows a lighter path because it is a deterministic view of
authoritative coordination state rather than a newly authored report:

```text
either lane records progress while it owns the baton
  -> the transition updates Run Plan item status and evidence
  -> the Codex lane renders a frozen Run Explainer snapshot
  -> Codex updates the existing Run Site owner-only
```

The lanes never edit or deploy the Site concurrently. A status change does not
change the agreed plan; adding, removing, or materially redefining work creates
a new plan revision and passes through peer review.

Every handoff has two Codex-owned publication points. The first publishes the
completed owner's checkpoint and pending transfer before ownership changes; the
second publishes the new active owner after the handoff commits. The Claude
lane signals publication through shared run state and never invokes Codex or
the Sites connector itself.

Suggested stable route shape inside each Run Site:

```text
/explainer
/research
/plan
/review
```

The local, reviewed artifact remains the canonical record. The Site is its
viewer-facing publication. A publisher may wrap or package the artifact but
must not alter its reviewed content; any content change creates a new hash and
requires review.

The existing single-file Dvandva HTML house format can remain the authoring
format, but Sites expects a compatible hosted project and validated build
output. Each run should therefore have a small generated Site project that
renders or serves its reviewed artifacts and current progress. The Site is
created when the run starts, initially shows intake or research status, and is
updated through saved versions as the run progresses.

## Proposed baton record

```json
{
  "id": "publish-<artifact-hash>",
  "artifact_ref": "<local path>",
  "artifact_hash": "sha256:<digest>",
  "site_project_id": "<opaque project id created once for the run>",
  "route": "/<artifact-kind>",
  "access": "owner_only",
  "status": "offered | claimed | saved | deployed | failed | cancelled",
  "site_version_id": null,
  "deployment_url": null
}
```

Do not store source credentials, bearer tokens, or environment secrets in the
baton. The exact opaque Site project ID belongs in run state because every
later artifact deployment must reuse it; all transient credentials stay behind
the Codex publication adapter.

Deployment receipts belong to a separate, Codex-owned Publication Ledger. The
baton declares the desired explainer snapshot and content identities; the
ledger records which desired revision Codex actually saved and deployed. This
lets the workflow owner and publisher advance independently without both
writing the same record.

## Safety and failure semantics

- Default to owner-only deployment. Shared or public access is a separate human
  decision because it changes who can see potentially private repository data.
- After peer approval, owner-only artifact save and deployment are automatic.
  Progress-only snapshots may publish directly from accepted baton state
  because they do not change artifact content or plan scope. Promoting access
  to selected users, a workspace, or the public always requires an explicit
  human decision.
- Treat every deployment URL as production. Use **save version** as the review
  boundary and **deploy version** only after the artifact and access mode are
  settled.
- A publication failure must not erase or invalidate the canonical local
  artifact. The baton should retain a retryable failure with the frozen content
  hash.
- Publishing must be idempotent by artifact hash and route. A crash after a
  successful deployment must be recoverable by inspecting the current Site and
  version before attempting another publish.
- Stable artifact-kind routes update through Sites versions. The baton records
  the approved content hash and resulting Sites version ID; it does not create
  a hash-specific public route.
- Publish meaningful progress transitions: work-item start or completion,
  handoff, blocker, decision, or revised plan. Coalesce rapid adjacent updates
  rather than deploying token-level activity. Never coalesce away the required
  before-handoff and after-handoff snapshots.
- Only explicitly classified Publishable Artifacts may appear on the Site.
  Private summaries are eligible after review, but secrets, credentials, raw
  private exports, and unreviewed proprietary source excerpts are prohibited.
- Never delete a Run Site automatically. Completed and abandoned runs retain
  their owner-only Site. If the workspace cannot create another Site because
  of a product or plan limit, route the run to `human_decision`.
- A successful final owner-only Run Explainer deployment is required before
  `done`. An unavailable Sites service is a retryable `human_decision`, not
  permission to silently downgrade to a local-only result.
- Preserve the historical HTML metadata and add the published route, version,
  and content hash outside the rendered content or in a new metadata revision.

## Minimal proof before adopting the design

After explicit approval, use a synthetic, non-sensitive report to verify:

1. both T3 Code role sessions can see the same frozen source;
2. Claude can author the file without any Codex process invocation;
3. a baton publication request wakes the already-running Codex lane;
4. run creation produces an owner-only explainer showing intake status;
5. Codex can build, save, and owner-only deploy through Sites;
6. both lanes' accepted progress transitions appear from the shared state;
7. the URL opens from the user's Android/T3 workflow;
8. updating the same route creates a new Sites version without changing the
   Site identity;
9. retry after an interrupted claim does not create duplicate Sites or publish
   an unreviewed source hash.

The proof should not use a private repository artifact, public access, a custom
domain, or destructive teardown.

## Decisions adopted

- Every intended T3-hosted Dvandva run has one owner-only Run Site and a live
  Run Explainer; Sites publication is not an optional capability in this
  profile.
- Create the Run Site when the run starts so intake and research progress are
  visible before the plan is approved.
- Render the peer-agreed Run Plan as a to-do list. Both lanes keep its progress
  current through baton transitions under the existing write lease; the Codex
  lane remains the sole Sites publisher.
- Publish meaningful accepted transitions while coalescing rapid adjacent
  updates; do not publish token-level activity.
- Make Codex solely responsible for maintaining the Run Site and publishing
  both the completed checkpoint immediately before each handoff and the new
  owner immediately after it.
- Keep actual Site versions, URLs, and deployment status in a Codex-owned
  Publication Ledger rather than allowing the publisher to mutate the current
  workflow owner's baton fields.
- Keep each Site discoverable through its baton and final handoff rather than a
  global index.
- Publish only explicitly classified artifacts, excluding secrets,
  credentials, raw private exports, and unreviewed proprietary source excerpts.
- Automatically save and deploy peer-approved revisions owner-only; require a
  human decision before any broader access.
- Never automatically delete a Run Site. Treat a creation limit as a
  `human_decision`.
- Require a successful final owner-only Run Explainer deployment before
  `done`; treat an outage as retryable rather than silently degrading the run.
- Reuse stable artifact-kind routes and record the exact content hash and Sites
  version ID for each deployed revision.
