//! `lint schema-parity` — cross-copy schema/enum parity (hardening S6-T1).
//!
//! Guards the many hand-maintained copies of the `dvandva.baton.v2` contract
//! against silent drift. Two axes are covered:
//!
//! * **Code side** (unit tests in this module): the engine's own status catalog
//!   ([`crate::write::V2_STATUS_CATALOG`]) is asserted equal to
//!   [`crate::baton::Status`]'s catalog and to
//!   [`crate::preflight::V2_STATUS_TOKENS`], and the run-terminal set is asserted
//!   to be exactly `{done, abandoned}`. These are the cheapest checks and never
//!   touch the filesystem.
//! * **Doc/source side** ([`report`]): the lint parses the DOC copies of the
//!   contract and compares them against the compiled engine lists.
//!
//! ## What [`report`] asserts (and the exact doc-wave contract each expects)
//!
//! 1. **Status-enum parity.** Three doc copies must enumerate exactly the 26
//!    engine status tokens:
//!    * `plugins/dvandva/references/baton-schema-v2.json` — its `status_catalog`
//!      JSON array of strings.
//!    * `product.md` — a single line of the form
//!      `Status catalog (26): clarifying_questions_drafting, … abandoned`.
//!      Everything after the literal marker `Status catalog (26):` is tokenised
//!      (`[a-z][a-z0-9_]*`) and must equal the engine catalog exactly, so the
//!      marker line must carry ONLY the 26 tokens (any stray lowercase word is
//!      treated as drift).
//!    * `plugins/dvandva/references/state-transition-table.md` — the same
//!      `Status catalog (26):` marker line.
//! 2. **Required-keys parity.** The `vadi` + `prativadi` SKILL.md inline
//!    fenced `json` blocks' top-level keys must equal
//!    [`crate::write::v2_required_keys`]. The fence is extracted with the
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
//!    case-sensitive token `HISTORICAL: dvandva.baton.v1`.
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
use crate::write::{v2_required_keys, V2_STATUS_CATALOG};

const CATALOG_MARKER: &str = "Status catalog (26):";
const HISTORICAL_MARKER: &str = "HISTORICAL: dvandva.baton.v1";
const WRITE_SRC: &str = "rust/dvandva/src/write.rs";
const CHANNEL_A: &str = "docs/protocol/local-baton-channel.md";
const CHANNEL_B: &str = "plugins/dvandva/references/local-baton-channel.md";
const SCHEMA_V2: &str = "plugins/dvandva/references/baton-schema-v2.json";
const STATE_TABLE: &str = "plugins/dvandva/references/state-transition-table.md";

/// Build the schema-parity findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();
    status_enum_parity(root, &mut r);
    required_keys_parity(root, &mut r);
    channel_doc_parity(root, &mut r);
    historical_markers(root, &mut r);
    reminder_hard_path_subset(root, &mut r);
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
    let want = canonical_status_catalog();

    let schema_ok = json_status_catalog(root).as_deref() == Some(&want[..]);
    r.add(
        schema_ok,
        "baton-schema-v2.json status_catalog equals the engine v2 status catalog",
    );

    let product_ok = marked_catalog(root, "product.md").as_deref() == Some(&want[..]);
    r.add(
        product_ok,
        "product.md status catalog line equals the engine v2 status catalog",
    );

    let stt_ok = marked_catalog(root, STATE_TABLE).as_deref() == Some(&want[..]);
    r.add(
        stt_ok,
        "state-transition-table.md status catalog equals the engine v2 status catalog",
    );
}

/// The engine's status catalog, sorted for order-insensitive comparison.
fn canonical_status_catalog() -> Vec<String> {
    let mut v: Vec<String> = V2_STATUS_CATALOG.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

/// `.status_catalog` from `baton-schema-v2.json` as a sorted token list. `None`
/// when the file is absent/unparseable, `status_catalog` is missing or not an
/// array, or any element is not a string. Duplicates are NOT collapsed, so a
/// repeated token fails the exact-equality comparison as drift.
fn json_status_catalog(root: &Path) -> Option<Vec<String>> {
    let text = read(root, SCHEMA_V2)?;
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
    let mut want: Vec<String> = v2_required_keys().iter().map(|s| s.to_string()).collect();
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
            format!("{label} SKILL.md inline baton keys equal write.rs v2_required_keys()"),
        );
    }
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
            file_contains(root, rel, HISTORICAL_MARKER),
            format!("{rel} carries the HISTORICAL: dvandva.baton.v1 marker"),
        );
    }
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::baton::Status;
    use crate::commit_gate::{matches_reminder_hard_path, REMINDER_HARD_PATH_TOKENS};
    use crate::preflight::V2_STATUS_TOKENS;
    use crate::write::{status_enum_ok, v2_required_keys, V2_STATUS_CATALOG};

    #[test]
    fn engine_catalog_has_26_unique_tokens() {
        let mut sorted = V2_STATUS_CATALOG.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 26, "engine catalog must be 26 unique tokens");
        assert_eq!(
            sorted.len(),
            V2_STATUS_CATALOG.len(),
            "engine catalog must have no duplicates"
        );
    }

    #[test]
    fn baton_status_matches_engine_catalog() {
        // Each engine token maps bijectively onto a `Status` (FromStr + as_str
        // round-trip). With `baton`'s own 26-variant enforcement, the bijection
        // pins the two catalogs equal in both directions.
        for tok in V2_STATUS_CATALOG {
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
    fn v2_required_keys_are_unique() {
        let mut sorted = v2_required_keys();
        let total = sorted.len();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            total,
            "v2_required_keys must have no duplicates"
        );
    }

    #[test]
    fn status_enum_ok_accepts_every_catalog_token() {
        // Pins the hot-path acceptor (`write.rs`'s match arm) equal to the
        // canonical catalog list, so the two can never silently diverge.
        for tok in V2_STATUS_CATALOG {
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
}
