//! `lint schema-parity` — cross-copy schema/enum parity (hardening S6-T1).
//!
//! Guards the many hand-maintained copies of the `dvandva.baton.v3` contract
//! against silent drift. Two axes are covered:
//!
//! * **Code side** (unit tests in this module): the engine's own status catalog
//!   ([`crate::write::V3_STATUS_CATALOG`]) is asserted against
//!   [`crate::baton::Status`]'s catalog, the historical v2 catalog is asserted
//!   against [`crate::preflight::V2_STATUS_TOKENS`], and the run-terminal set is
//!   asserted to be exactly `{done, abandoned}`. These are the cheapest checks
//!   and never touch the filesystem.
//! * **Doc/source side** ([`report`]): the lint parses the DOC copies of the
//!   contract and compares them against the compiled engine lists.
//!
//! ## What [`report`] asserts (and the exact doc-wave contract each expects)
//!
//! 1. **Status-enum parity.** Four doc copies are pinned to an engine catalog:
//!    * `plugins/dvandva/references/baton-schema-v3.json` — its `status_catalog`
//!      JSON array of strings (the live write-schema reference) must equal the
//!      29-token v3 engine catalog (the 26-token lifecycle base plus the three
//!      v3-only `workflow_declaring`/`workflow_review`/`workflow_revision`
//!      declaration states).
//!    * `plugins/dvandva/references/baton-schema-v2.json` — its historical
//!      read-path `status_catalog` JSON array of strings must equal the frozen
//!      26-token v2 catalog (v2 never had the workflow-declaration states).
//!    * `product.md` — a single line of the form
//!      `Status catalog (26): clarifying_questions_drafting, … abandoned`
//!      pinned to the frozen 26-token v2 catalog.
//!      Everything after the literal marker `Status catalog (26):` is tokenised
//!      (`[a-z][a-z0-9_]*`) and must equal the engine catalog exactly, so the
//!      marker line must carry ONLY the 26 tokens (any stray lowercase word is
//!      treated as drift).
//!    * `plugins/dvandva/references/state-transition-table.md` — the same
//!      `Status catalog (26):` marker line.
//! 2. **Required-keys parity.** The `vadi` + `prativadi` SKILL.md inline
//!    fenced `json` blocks' top-level keys must equal
//!    [`crate::write::v2_required_keys`] plus required `run_workflow`. The fence is extracted with the
//!    SAME scanner `lint skills` uses
//!    ([`crate::lint::skills::extract_fenced_json_block`], `pub(crate)`), so
//!    the two lints can never diverge on which lines belong to the inline
//!    contract block. A body carrying more than one ` ```json ` fence FAILS
//!    outright — the A2 precondition the hardening docs wave pins for
//!    single-fence SKILL.md files.
//! 3. **Channel-doc parity.** `docs/protocol/local-baton-channel.md` and
//!    `plugins/dvandva/references/local-baton-channel.md` must be byte-identical.
//! 4. **Historical markers.** `plugins/dvandva/references/baton-schema.json` and
//!    `templates/channel/baton.json` must each contain a line with the
//!    case-sensitive token `HISTORICAL: dvandva.baton.v1`; the v2 reference must
//!    contain `HISTORICAL: dvandva.baton.v2`.
//! 5. **Local-list drift guard (source-scan).** Every literal token in
//!    [`crate::commit_gate::REMINDER_HARD_PATH_TOKENS`] must appear in
//!    `rust/dvandva/src/write.rs` — a documented approximation of "the
//!    commit-gate reminder subset ⊆ `write.rs`'s canonical hard-path behavior".
//!
//! Assertions 1 (the doc copies), 3, and 4 FAIL on the live tree until the
//! hardening docs wave lands them; the crate's fixture tests cover pass+fail per
//! assertion, and a single `#[ignore]`d live-tree test flips green once the wave
//! is in.

use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::commit_gate::REMINDER_HARD_PATH_TOKENS;
use crate::lint::skills;
use crate::lint::{file_contains, read, resolve_root, Report};
use crate::write::{v2_required_keys, V2_STATUS_CATALOG, V3_STATUS_CATALOG};

const CATALOG_MARKER: &str = "Status catalog (26):";
const HISTORICAL_V1_MARKER: &str = "HISTORICAL: dvandva.baton.v1";
const HISTORICAL_V2_MARKER: &str = "HISTORICAL: dvandva.baton.v2";
const WRITE_SRC: &str = "rust/dvandva/src/write.rs";
const CHANNEL_A: &str = "docs/protocol/local-baton-channel.md";
const CHANNEL_B: &str = "plugins/dvandva/references/local-baton-channel.md";
const SCHEMA_V2: &str = "plugins/dvandva/references/baton-schema-v2.json";
const SCHEMA_V3: &str = "plugins/dvandva/references/baton-schema-v3.json";
const STATE_TABLE: &str = "plugins/dvandva/references/state-transition-table.md";

/// Build the schema-parity findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();
    status_enum_parity(root, &mut r);
    required_keys_parity(root, &mut r);
    channel_doc_parity(root, &mut r);
    historical_markers(root, &mut r);
    reminder_hard_path_subset(root, &mut r);
    disagreement_cap_default(root, &mut r);
    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}

// ---------------------------------------------------------------------------
// Assertion 1 — status-enum doc parity.
// ---------------------------------------------------------------------------

fn status_enum_parity(root: &Path, r: &mut Report) {
    // The LIVE v3 engine catalog (29 tokens) pins the v3 write-schema reference;
    // the retired v2 copies stay frozen at the historical 26-token lifecycle
    // catalog (v2 never had the three per-run-workflow declaration states).
    let want_v3 = canonical_catalog(V3_STATUS_CATALOG);
    let want_v2 = canonical_catalog(V2_STATUS_CATALOG);

    let schema_v3_ok = json_status_catalog(root, SCHEMA_V3).as_deref() == Some(&want_v3[..]);
    r.add(
        schema_v3_ok,
        "baton-schema-v3.json status_catalog equals the engine v3 status catalog",
    );

    let schema_v2_ok = json_status_catalog(root, SCHEMA_V2).as_deref() == Some(&want_v2[..]);
    r.add(
        schema_v2_ok,
        "historical baton-schema-v2.json status_catalog equals the engine v2 status catalog",
    );

    let product_ok = marked_catalog(root, "product.md").as_deref() == Some(&want_v2[..]);
    r.add(
        product_ok,
        "product.md status catalog line equals the engine v2 status catalog",
    );

    let stt_ok = marked_catalog(root, STATE_TABLE).as_deref() == Some(&want_v2[..]);
    r.add(
        stt_ok,
        "state-transition-table.md status catalog equals the engine v2 status catalog",
    );
}

/// A status catalog sorted for order-insensitive comparison.
fn canonical_catalog(catalog: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = catalog.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

/// `.status_catalog` from a baton-schema reference as a sorted token list. `None`
/// when the file is absent/unparseable, `status_catalog` is missing or not an
/// array, or any element is not a string. Duplicates are NOT collapsed, so a
/// repeated token fails the exact-equality comparison as drift.
fn json_status_catalog(root: &Path, rel: &str) -> Option<Vec<String>> {
    let text = read(root, rel)?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let arr = value.get("status_catalog")?.as_array()?;
    let mut tokens: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if tokens.len() != arr.len() {
        return None; // a non-string element in the array
    }
    tokens.sort();
    Some(tokens)
}

/// The status tokens on the `Status catalog (26):` marker line of a markdown
/// doc, sorted (duplicates preserved so they read as drift). `None` when no line
/// contains the marker.
fn marked_catalog(root: &Path, rel: &str) -> Option<Vec<String>> {
    let text = read(root, rel)?;
    let line = text.lines().find(|l| l.contains(CATALOG_MARKER))?;
    let idx = line.find(CATALOG_MARKER)? + CATALOG_MARKER.len();
    let tail = &line[idx..];
    let re = Regex::new(r"[a-z][a-z0-9_]*").ok()?;
    let mut tokens: Vec<String> = re.find_iter(tail).map(|m| m.as_str().to_string()).collect();
    tokens.sort();
    Some(tokens)
}

// ---------------------------------------------------------------------------
// Assertion 2 — required-keys parity.
// ---------------------------------------------------------------------------

fn required_keys_parity(root: &Path, r: &mut Report) {
    let mut want: Vec<String> = v3_inline_required_keys()
        .iter()
        .map(|s| s.to_string())
        .collect();
    want.sort();

    for (rel, label) in [
        ("plugins/dvandva/skills/vadi/SKILL.md", "vadi"),
        ("plugins/dvandva/skills/prativadi/SKILL.md", "prativadi"),
    ] {
        let text = read(root, rel);
        let fence_count = text.as_deref().map(skills::count_json_fences).unwrap_or(0);
        if fence_count > 1 {
            r.add(
                false,
                format!(
                    "{label} SKILL.md body carries {fence_count} fenced json blocks (single JSON fence required)"
                ),
            );
            continue;
        }
        let ok = text
            .as_deref()
            .and_then(skill_inline_keys)
            .map(|mut got| {
                got.sort();
                got == want
            })
            .unwrap_or(false);
        r.add(
            ok,
            format!("{label} SKILL.md inline baton keys equal write.rs v2_required_keys() plus run_workflow"),
        );
    }
}

fn v3_inline_required_keys() -> Vec<&'static str> {
    let mut keys = v2_required_keys();
    keys.push("run_workflow");
    keys
}

/// The top-level keys of the first fenced `json` object in an already-read
/// SKILL.md body. `None` when there is no such fence, or the fence is not a
/// JSON object. The fence itself is extracted via the shared
/// [`crate::lint::skills::extract_fenced_json_block`] scanner — the same one
/// `lint skills` uses — so the two lints can never diverge on which lines
/// belong to the inline contract block.
fn skill_inline_keys(text: &str) -> Option<Vec<String>> {
    let block = skills::extract_fenced_json_block(text);
    if block.trim().is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(&block).ok()?;
    let obj = value.as_object()?;
    Some(obj.keys().cloned().collect())
}

// ---------------------------------------------------------------------------
// Assertion 3 — channel-doc byte parity.
// ---------------------------------------------------------------------------

fn channel_doc_parity(root: &Path, r: &mut Report) {
    let a = fs::read(root.join(CHANNEL_A)).ok();
    let b = fs::read(root.join(CHANNEL_B)).ok();
    let ok = matches!((&a, &b), (Some(x), Some(y)) if x == y);
    r.add(
        ok,
        "docs/protocol and references local-baton-channel.md copies are byte-identical",
    );
}

// ---------------------------------------------------------------------------
// Assertion 4 — HISTORICAL markers.
// ---------------------------------------------------------------------------

fn historical_markers(root: &Path, r: &mut Report) {
    for rel in [
        "plugins/dvandva/references/baton-schema.json",
        "templates/channel/baton.json",
    ] {
        r.add(
            file_contains(root, rel, HISTORICAL_V1_MARKER),
            format!("{rel} carries the HISTORICAL: dvandva.baton.v1 marker"),
        );
    }
    r.add(
        file_contains(root, SCHEMA_V2, HISTORICAL_V2_MARKER),
        "baton-schema-v2.json carries the HISTORICAL: dvandva.baton.v2 marker",
    );
}

// ---------------------------------------------------------------------------
// Assertion 5 — commit-gate reminder subset ⊆ write.rs hard-path source.
// ---------------------------------------------------------------------------

fn reminder_hard_path_subset(root: &Path, r: &mut Report) {
    let missing: Vec<&str> = REMINDER_HARD_PATH_TOKENS
        .iter()
        .copied()
        .filter(|tok| !file_contains(root, WRITE_SRC, tok))
        .collect();
    r.add(
        missing.is_empty(),
        "commit_gate reminder hard-path tokens all appear in write.rs hard-path source",
    );
}

// ---------------------------------------------------------------------------
// Assertion 6 — the disagreement-loop cap default is pinned to 10.
// ---------------------------------------------------------------------------

/// The live v3 write-schema reference must carry the default disagreement cap of
/// 10 (raised from 3 in 830e1d1). Reverting the default back to 3 on this
/// surface — with no pin — would leave every parity check green; this fails
/// closed instead.
fn disagreement_cap_default(root: &Path, r: &mut Report) {
    r.add(
        file_contains(root, SCHEMA_V3, "\"disagreement_cap\": 10"),
        "baton-schema-v3.json pins the disagreement cap default to 10",
    );
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde_json::Value;

    use crate::baton::Status;
    use crate::commit_gate::{matches_reminder_hard_path, REMINDER_HARD_PATH_TOKENS};
    use crate::preflight::V2_STATUS_TOKENS;
    use crate::write::{status_enum_ok, v2_required_keys, V2_STATUS_CATALOG, V3_STATUS_CATALOG};

    #[test]
    fn engine_catalog_has_26_unique_tokens() {
        let mut sorted = V2_STATUS_CATALOG.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            26,
            "v2 engine catalog must be 26 unique tokens"
        );
        assert_eq!(
            sorted.len(),
            V2_STATUS_CATALOG.len(),
            "v2 engine catalog must have no duplicates"
        );
    }

    #[test]
    fn v3_engine_catalog_has_29_unique_tokens_superset_of_v2() {
        let mut sorted = V3_STATUS_CATALOG.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            29,
            "v3 engine catalog must be 29 unique tokens"
        );
        assert_eq!(
            sorted.len(),
            V3_STATUS_CATALOG.len(),
            "v3 engine catalog must have no duplicates"
        );
        // The v3 catalog is the v2 catalog plus exactly the three declaration
        // states.
        for tok in V2_STATUS_CATALOG {
            assert!(
                V3_STATUS_CATALOG.contains(tok),
                "v3 catalog must contain every v2 token, missing {tok}"
            );
        }
        for tok in ["workflow_declaring", "workflow_review", "workflow_revision"] {
            assert!(
                V3_STATUS_CATALOG.contains(&tok) && !V2_STATUS_CATALOG.contains(&tok),
                "{tok} must be v3-only"
            );
        }
    }

    #[test]
    fn baton_status_matches_engine_catalog() {
        // Each engine token maps bijectively onto a `Status` (FromStr + as_str
        // round-trip). With `baton`'s own 29-variant enforcement, the bijection
        // pins the two catalogs equal in both directions.
        for tok in V3_STATUS_CATALOG {
            let s =
                Status::from_str(tok).unwrap_or_else(|_| panic!("engine token must parse: {tok}"));
            assert_eq!(s.as_str(), *tok, "as_str must round-trip {tok}");
            let via_serde: Status = serde_json::from_str(&format!("\"{tok}\"")).unwrap();
            assert_eq!(via_serde.as_str(), *tok);
        }
    }

    #[test]
    fn preflight_tokens_match_engine_catalog() {
        let mut a = V2_STATUS_TOKENS.to_vec();
        let mut b = V2_STATUS_CATALOG.to_vec();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(
            a, b,
            "preflight V2_STATUS_TOKENS must equal the engine v2 status catalog"
        );
    }

    #[test]
    fn engine_terminal_set_is_done_and_abandoned() {
        for tok in V2_STATUS_CATALOG {
            let s = Status::from_str(tok).unwrap();
            let expected = *tok == "done" || *tok == "abandoned";
            assert_eq!(
                s.is_terminal(),
                expected,
                "terminal set disagreement for {tok}"
            );
        }
    }

    #[test]
    fn v3_inline_required_keys_are_unique() {
        let mut sorted = super::v3_inline_required_keys();
        let total = sorted.len();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            total,
            "v3 inline required keys must have no duplicates"
        );
        assert!(
            sorted.binary_search(&"run_workflow").is_ok(),
            "v3_required_keys must include run_workflow"
        );
    }

    #[test]
    fn status_enum_ok_accepts_every_catalog_token() {
        // Pins the hot-path acceptor (`write.rs`'s match arm) equal to the
        // canonical v3 catalog list, so the two can never silently diverge.
        for tok in V3_STATUS_CATALOG {
            assert!(
                status_enum_ok(tok),
                "status_enum_ok must accept catalog token {tok}"
            );
        }
    }

    #[test]
    fn reminder_hard_path_tokens_each_match_a_representative_path() {
        // Closes the const->function direction the source-scan in
        // `reminder_hard_path_subset` misses: that scan only asserts every
        // token is a SUBSTRING of write.rs's source, not that
        // `matches_reminder_hard_path` actually accepts a path built from it.
        // A future token added to REMINDER_HARD_PATH_TOKENS without a
        // fixture here panics instead of silently passing unverified.
        for tok in REMINDER_HARD_PATH_TOKENS {
            let path = match *tok {
                ".env" => ".env",
                "secret" => "secret/foo",
                "secrets" => "secrets/foo",
                "credential" => "credential/foo",
                "credentials" => "credentials/foo",
                "product.md" => "product.md",
                "plugins/dvandva/skills/" => "plugins/dvandva/skills/vadi/SKILL.md",
                "rust/dvandva/src/" => "rust/dvandva/src/write.rs",
                other => panic!("no representative path fixture for reminder token {other}"),
            };
            assert!(
                matches_reminder_hard_path(path),
                "matches_reminder_hard_path({path:?}) must be true for token {tok}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Chunk C (delta-reverification-2, Option B) — the seven additive carry
    // fields stay optional/nested and never regress v2/v3 backward
    // compatibility. Pure Rust, no filesystem dependency, matching this
    // module's existing catalog/key-parity test style.
    // -----------------------------------------------------------------------

    /// The exact seven Option-B carry fields (see baton-schema-v3.json's
    /// `_carry_fields_note` / `subagent_tracks_example`). There is
    /// deliberately no `carried_from_id` — Option B uses same-id carry.
    const CARRY_FIELDS: [&str; 7] = [
        "carried_from_checkpoint",
        "carry_reason",
        "covers_chunks",
        "global",
        "covered_input_digest",
        "digest_algo",
        "covered_paths",
    ];

    #[test]
    fn legacy_baton_without_carry_fields_validates() {
        // A pre-carry subagent_track, shaped exactly like the v2 reference
        // schema's real `startup-controller` example: none of the seven
        // carry fields present.
        let legacy_track = serde_json::json!({
            "id": "startup-controller",
            "phase": "research",
            "status": "planned",
            "track": "controller",
            "owner": "vadi",
            "parallelized": false,
            "rationale": "Initial run scaffold; record concrete subagent tracks as each phase begins.",
            "inputs": [],
            "outputs": [],
            "evidence_refs": [],
            "result": "pending"
        });
        // A pre-carry verification_matrix row, shaped like the v2
        // reference's real `verify-research-coverage` example.
        let legacy_row = serde_json::json!({
            "id": "verify-research-coverage",
            "phase": "research",
            "owner": "prativadi",
            "covers": ["original_ask", "research_ref", "work_split"],
            "command": null,
            "expected": "Independent research review confirms the artifact is source-backed and sufficient for spec drafting.",
            "result": "pending",
            "evidence_ref": null
        });
        for unit in [&legacy_track, &legacy_row] {
            let obj = unit.as_object().expect("fixture unit is a JSON object");
            for field in CARRY_FIELDS {
                assert!(
                    !obj.contains_key(field),
                    "legacy fixture must validate with none of the seven carry fields present, found {field}"
                );
            }
            // Round-trips byte-equal — the "legacy batons validate byte-equal"
            // half of the Option B backward-compat constraint.
            let serialized = serde_json::to_string(unit).unwrap();
            let reparsed: Value = serde_json::from_str(&serialized).unwrap();
            assert_eq!(&reparsed, unit, "legacy fixture must round-trip byte-equal");
        }

        // Guard against future drift: the seven fields stay optional and
        // NESTED in Baton::rest — never promoted to a top-level required key
        // (v2/v3 required-keys parity stays unchanged) ...
        let mut required: Vec<&str> = super::v3_inline_required_keys();
        required.extend(v2_required_keys());
        for field in CARRY_FIELDS {
            assert!(
                !required.contains(&field),
                "{field} must never be a top-level required key (v2/v3 required-keys parity is unchanged)"
            );
        }
        // ... and never promoted to a new lifecycle Status.
        for field in CARRY_FIELDS {
            assert!(
                !V2_STATUS_CATALOG.contains(&field) && !V3_STATUS_CATALOG.contains(&field),
                "{field} must never be a status_catalog member (no new status was added)"
            );
        }
    }
}
