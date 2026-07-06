//! Golden differential tests for the workflow preset graphs.
//!
//! These tests compare the new `workflow::preset` data against the current
//! `dvandva next` transition oracle. `next` deliberately adds universal pause
//! and same-status sync helper edges, so the comparison keeps only edges whose
//! target belongs to the preset graph under test.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{make_baton_v2, make_baton_v3, v2_status_owner};
use dvandva::workflow::preset;
use serde_json::{json, Value};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

fn run_next_list(baton: &Path) -> String {
    let output = Command::new(bin())
        .arg("next")
        .arg("--file")
        .arg(baton)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .output()
        .expect("spawn dvandva next");
    assert_eq!(
        output.status.code().unwrap_or(-1),
        0,
        "dvandva next LIST must exit 0\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[derive(Clone)]
struct ProbeVariant {
    mode: &'static str,
    profile: Option<&'static str>,
    phase_profiles: Option<Value>,
}

fn probe_variants(preset_name: &str) -> Vec<ProbeVariant> {
    match preset_name {
        "fast" => vec![development("fast", None)],
        "standard" => vec![
            development("standard", None),
            development("standard", Some(json!({"2": "full"}))),
        ],
        "full" => vec![
            development("full", None),
            development("full", Some(json!({"2": "standard"}))),
        ],
        "research" => vec![ProbeVariant {
            mode: "research",
            profile: None,
            phase_profiles: None,
        }],
        "review" => vec![ProbeVariant {
            mode: "review",
            profile: None,
            phase_profiles: None,
        }],
        _ => panic!("unknown preset {preset_name}"),
    }
}

fn development(profile: &'static str, phase_profiles: Option<Value>) -> ProbeVariant {
    ProbeVariant {
        mode: "development",
        profile: Some(profile),
        phase_profiles,
    }
}

fn configure_profile(baton: &mut Value, profile: Option<&str>) {
    match profile {
        Some("fast") => common::fast_profile(baton),
        Some("standard") => common::standard_profile(baton),
        Some("full") | None => {
            baton["profile"] = json!("full");
            baton["profile_floor"] = json!("full");
            baton["profile_decision"] = json!({
                "selected_profile": "full",
                "floor": "full",
                "reason": "workflow golden fixture",
                "decided_by": "test-suite",
                "decided_at": "2026-07-06T00:00:00Z",
                "risk_inputs": [],
                "hard_triggers": [],
                "allowlist_match": false,
                "allowlist_refs": [],
                "evidence_refs": ["test:workflow-golden"]
            });
            baton["profile_history"] = json!([]);
        }
        Some(other) => panic!("unknown profile {other}"),
    }
}

fn write_probe_baton(dir: &Path, from: &str, variant: &ProbeVariant) -> PathBuf {
    let safe_from = from.replace('/', "_");
    let path = dir.join(format!(
        "{safe_from}-{}-{}.json",
        variant.mode,
        variant.profile.unwrap_or("none")
    ));
    make_baton_v2(&path, from, v2_status_owner(from), 4, |b| {
        b["mode"] = json!(variant.mode);
        b["master_plan_locked"] = json!(true);
        b["total_phases"] = json!(3);
        b["disagreement_cap"] = json!(99);
        b["loop_counts"] = json!({});
        configure_profile(b, variant.profile);
        if let Some(phase_profiles) = &variant.phase_profiles {
            b["phase_profiles"] = phase_profiles.clone();
        } else {
            b["phase_profiles"] = Value::Null;
        }
    });
    path
}

fn expected_by_from(preset_name: &str) -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let graph = preset(preset_name).expect("known preset");
    let mut by_from = BTreeMap::new();
    for edge in graph.edges {
        by_from
            .entry(edge.from)
            .or_insert_with(BTreeSet::new)
            .insert(edge.to);
    }
    by_from
}

fn graph_states(preset_name: &str) -> BTreeSet<&'static str> {
    preset(preset_name)
        .expect("known preset")
        .states
        .into_iter()
        .map(|state| state.name)
        .collect()
}

fn listed_graph_targets(
    stdout: &str,
    from: &str,
    graph_states: &BTreeSet<&str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("DVANDVA_NEXT ") else {
            continue;
        };
        let Some(to) = rest.split_whitespace().next() else {
            continue;
        };
        if to == "note" || to == from {
            continue;
        }
        if graph_states.contains(to) {
            out.insert(to.to_string());
        }
    }
    out
}

/// `(to, (owner, has_loop))` for every `DVANDVA_NEXT <to> owner=<o>
/// phase=<p> [loop=<n>/<c>] ...` line in `stdout` whose `to` belongs to
/// `graph_states` — mirrors [`listed_graph_targets`]'s filtering, but keeps
/// the `owner=` and `loop=` tokens instead of discarding them.
fn listed_graph_details(
    stdout: &str,
    from: &str,
    graph_states: &BTreeSet<&str>,
) -> BTreeMap<String, (String, bool)> {
    let mut out = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("DVANDVA_NEXT ") else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        let Some(to) = tokens.next() else { continue };
        if to == "note" || to == from {
            continue;
        }
        if !graph_states.contains(to) {
            continue;
        }
        let mut owner = None;
        let mut has_loop = false;
        for tok in tokens {
            if let Some(o) = tok.strip_prefix("owner=") {
                owner = Some(o.to_string());
            } else if tok.starts_with("loop=") {
                has_loop = true;
            }
        }
        let owner = owner
            .unwrap_or_else(|| panic!("DVANDVA_NEXT line for {to} missing owner= token: {line}"));
        out.insert(to.to_string(), (owner, has_loop));
    }
    out
}

#[test]
fn preset_edges_match_legacy_next_oracle() {
    let tmp = tempfile::tempdir().unwrap();

    for preset_name in ["fast", "standard", "full", "research", "review"] {
        let expected = expected_by_from(preset_name);
        let states = graph_states(preset_name);
        let variants = probe_variants(preset_name);

        for (from, expected_targets) in expected {
            let mut actual_targets = BTreeSet::new();
            for variant in &variants {
                let baton = write_probe_baton(tmp.path(), from, variant);
                let stdout = run_next_list(&baton);
                actual_targets.extend(listed_graph_targets(&stdout, from, &states));
            }

            let expected_targets: BTreeSet<String> =
                expected_targets.into_iter().map(str::to_string).collect();
            assert_eq!(
                actual_targets, expected_targets,
                "{preset_name}: preset graph drift for source status {from}"
            );
        }
    }
}

// ===========================================================================
// Owner parity — presets.rs's `owner()` transcription of
// `write::v2_expected_assignee` (+ `write::owner_for`'s `done` special case).
// ===========================================================================

/// For every preset edge, the `owner=` token the legacy `next` oracle reports
/// for the TARGET status must equal that state's transcribed owner. `next`'s
/// LIST assignee is produced by `write::owner_for`, which wraps
/// `write::v2_expected_assignee` and special-cases `done` to `"team"` — so
/// this single oracle already exercises both halves of presets.rs's
/// transcription (see `done_owner_matches_owner_for_special_case` below for
/// an explicit pin of that special case).
///
/// GAP: `clarifying_questions_drafting` is never a transition TARGET in any
/// of the five graphs (every graph's own edges use it only as a SOURCE — it
/// is the universal scaffold seed), so this oracle can never observe its
/// owner. That state is covered separately by
/// `clarifying_questions_drafting_owner_matches_write_engine_want_token`.
#[test]
fn preset_owners_match_legacy_next_oracle() {
    let tmp = tempfile::tempdir().unwrap();

    for preset_name in ["fast", "standard", "full", "research", "review"] {
        let graph = preset(preset_name).expect("known preset");
        let states = graph_states(preset_name);
        let variants = probe_variants(preset_name);

        for (from, _) in expected_by_from(preset_name) {
            for variant in &variants {
                let baton = write_probe_baton(tmp.path(), from, variant);
                let stdout = run_next_list(&baton);
                for (to, (owner, _has_loop)) in listed_graph_details(&stdout, from, &states) {
                    let want = graph
                        .states
                        .iter()
                        .find(|s| s.name == to)
                        .unwrap_or_else(|| {
                            panic!("{preset_name}: target status {to} missing from graph.states")
                        })
                        .owner;
                    assert_eq!(
                        owner, want,
                        "{preset_name}: owner drift for target status {to} (observed via next oracle from source {from})"
                    );
                }
            }
        }
    }
}

/// Explicit pin of presets.rs's one judgment call (see its module doc
/// comment): `write::v2_expected_assignee("done")` alone returns `""` (done
/// is a same-status handshake, not a role-assigned status), but
/// `write::owner_for` special-cases `done` to `"team"` before that empty
/// string ever reaches a caller. presets.rs's `owner("done")` follows
/// `owner_for`, not the bare `v2_expected_assignee`. The `next` LIST oracle
/// runs through `owner_for` too, so probing `termination_review -> done`
/// (the only edge that reaches `done` in any preset) exercises that special
/// case directly instead of relying on it falling out of the generic loop
/// above.
#[test]
fn done_owner_matches_owner_for_special_case() {
    let tmp = tempfile::tempdir().unwrap();

    for preset_name in ["fast", "standard", "full", "research", "review"] {
        let states = graph_states(preset_name);
        let variants = probe_variants(preset_name);
        let mut saw_done = false;

        for variant in &variants {
            let baton = write_probe_baton(tmp.path(), "termination_review", variant);
            let stdout = run_next_list(&baton);
            if let Some((owner, _has_loop)) =
                listed_graph_details(&stdout, "termination_review", &states).get("done")
            {
                saw_done = true;
                assert_eq!(
                    owner, "team",
                    "{preset_name}: done must be owner=team via owner_for's special case"
                );
            }
        }
        assert!(
            saw_done,
            "{preset_name}: termination_review -> done must be observable via the next oracle"
        );
    }
}

/// GAP-CLOSER for `preset_owners_match_legacy_next_oracle`:
/// `clarifying_questions_drafting` is never a transition TARGET, so its
/// owner can't be observed via the `next` LIST oracle. `dvandva write`
/// rejects a mismatched assignee with a `bad_assignee_owner ...
/// want=<expected> got=<wrong>` line built straight from
/// `write::v2_expected_assignee` (`src/write.rs` around line 562) — probe
/// THAT oracle directly with a deliberately wrong assignee on a fresh
/// scaffold candidate. `validate_candidate_shape` (and therefore the
/// assignee-owner check) runs before the current baton is ever read, so an
/// absent `baton.json` (a genuine first-ever scaffold write) is enough setup.
#[test]
fn clarifying_questions_drafting_owner_matches_write_engine_want_token() {
    let tmp = tempfile::tempdir().unwrap();
    let baton = tmp.path().join("baton.json"); // never created: this IS the scaffold write
    let candidate = tmp.path().join("baton.next.json");
    make_baton_v3(
        &candidate,
        "clarifying_questions_drafting",
        "prativadi", // deliberately wrong: presets.rs transcribes "vadi"
        0,
        |b| {
            b["phase"] = json!("clarifying");
        },
    );

    let output = Command::new(bin())
        .arg("write")
        .arg(&baton)
        .arg(&candidate)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .output()
        .expect("spawn dvandva write");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(23),
        "wrong-assignee scaffold must be rejected as bad_assignee_owner\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout)
    );

    let want = preset("fast")
        .unwrap()
        .states
        .into_iter()
        .find(|s| s.name == "clarifying_questions_drafting")
        .expect("clarifying_questions_drafting must be a fast-graph state")
        .owner;
    assert!(
        stderr.contains(&format!("want={want}")),
        "expected bad_assignee_owner want={want} in stderr, got:\n{stderr}"
    );
}

// ===========================================================================
// Loop-cap-key parity — presets.rs's `loop_cap_key()` transcription of
// `write::is_loop_edge` (and, by construction, `write::build_loop_key`'s key
// string — see the source-literal check below).
// ===========================================================================

/// Edges where the `next` oracle's `loop=` token is a REAL FINDING, not a
/// transcription bug: `write::is_amendment_enter_edge` (full
/// `deslop:spec_revision`, standard `phase_review:spec_revision`) is a
/// SEPARATE loop-capped mechanism from `write::is_loop_edge` — its key is the
/// dynamic `"plan_amendment:<phase>"` (see `write.rs`'s `legal_transitions`,
/// which attaches a loop key to an amendment-enter edge via that format,
/// independently of `is_loop_edge`), not a static `"from:to"` string, so it
/// cannot be represented as `WfEdge::loop_cap_key: Some("from:to")` in a
/// phase-agnostic preset graph at all. presets.rs's module doc is accurate as
/// written ("transcribed from `write::is_loop_edge`" — it never claimed to
/// cover this second mechanism), but `WfEdge::loop_cap_key` as a TYPE doesn't
/// currently have any way to say "this edge is loop-capped, but by a phase-
/// keyed cap, not a fixed one" — a caller reading `loop_cap_key: None` off
/// either of these two edges would wrongly conclude they're uncapped. Flagged
/// in the test-creation report; not fixed here (would touch `src/workflow`).
const AMENDMENT_ENTER_EDGES_WITH_UNMODELED_LOOP_CAP: &[(&str, &str, &str)] = &[
    ("full", "deslop", "spec_revision"),
    ("standard", "phase_review", "spec_revision"),
];

/// For every preset edge, `graph.edges[].loop_cap_key.is_some()` must equal
/// whether the legacy `next` oracle's line for that edge carries a `loop=`
/// token. `next` emits `loop=` both when `write::is_loop_edge` accepts the
/// `"from:to"` edge string (see `write.rs`'s `legal_transitions`, which gates
/// `build_loop_key` on `is_loop_edge`) AND, independently, on a plan-
/// amendment-enter edge (see `AMENDMENT_ENTER_EDGES_WITH_UNMODELED_LOOP_CAP`)
/// — so those two specific edges are carved out with an explicit assertion
/// of their (documented, real) divergence instead of the generic equality
/// check. For every other edge this equality covers BOTH directions the
/// task requires in one assertion: a preset edge that carries `Some(k)` but
/// that `is_loop_edge` doesn't recognise would show `loop_cap_key.is_some()
/// == true, has_loop == false` (fails); an `is_loop_edge`-recognised edge
/// presets.rs left uncapped would show the opposite mismatch (also fails).
#[test]
fn preset_loop_keys_match_legacy_next_oracle() {
    let tmp = tempfile::tempdir().unwrap();

    for preset_name in ["fast", "standard", "full", "research", "review"] {
        let graph = preset(preset_name).expect("known preset");
        let states = graph_states(preset_name);
        let variants = probe_variants(preset_name);

        for (from, _) in expected_by_from(preset_name) {
            let mut observed: BTreeMap<String, bool> = BTreeMap::new();
            for variant in &variants {
                let baton = write_probe_baton(tmp.path(), from, variant);
                let stdout = run_next_list(&baton);
                for (to, (_owner, has_loop)) in listed_graph_details(&stdout, from, &states) {
                    observed.insert(to, has_loop);
                }
            }
            for e in graph.edges.iter().filter(|e| e.from == from) {
                let has_loop = *observed.get(e.to).unwrap_or_else(|| {
                    panic!(
                        "{preset_name}: next oracle never listed edge {from}:{} \
                         (should already be covered by preset_edges_match_legacy_next_oracle)",
                        e.to
                    )
                });
                if AMENDMENT_ENTER_EDGES_WITH_UNMODELED_LOOP_CAP.contains(&(
                    preset_name,
                    e.from,
                    e.to,
                )) {
                    // Documented real finding (see the const's doc comment):
                    // the oracle IS loop-capped here, but through the
                    // amendment-enter mechanism, not `is_loop_edge` — pin the
                    // actual divergence rather than asserting equality.
                    assert!(
                        e.loop_cap_key.is_none(),
                        "{preset_name}: {}:{} was expected to stay untranscribed by \
                         loop_cap_key (its cap is the amendment-enter mechanism); \
                         update AMENDMENT_ENTER_EDGES_WITH_UNMODELED_LOOP_CAP if \
                         presets.rs now models it",
                        e.from,
                        e.to
                    );
                    assert!(
                        has_loop,
                        "{preset_name}: {}:{} no longer shows a loop= token from the \
                         amendment-enter mechanism — the documented finding may be stale; \
                         re-check AMENDMENT_ENTER_EDGES_WITH_UNMODELED_LOOP_CAP",
                        e.from, e.to
                    );
                    continue;
                }
                assert_eq!(
                    e.loop_cap_key.is_some(),
                    has_loop,
                    "{preset_name}: loop-cap drift for edge {from}:{} — preset loop_cap_key={:?}, \
                     next oracle loop= token present={has_loop}",
                    e.to,
                    e.loop_cap_key
                );
            }
        }
    }
}

/// Source-text differential closing the one thing the runtime oracle above
/// can't observe: the literal `"from:to"` KEY STRINGS. `write::is_loop_edge`
/// and `write::build_loop_key` are private to `write.rs` (unreachable from
/// an integration test — see this file's module doc for the same
/// black-box-CLI constraint that shapes every other test here), and
/// `build_loop_key` echoes its `edge` argument verbatim as the key (see
/// `write.rs` around line 1182: `Some((edge.to_string(), ...))`), so the key
/// `next`'s oracle would carry, if it printed it, is always exactly the
/// `"from:to"` string presets.rs already threads through as
/// `loop_cap_key_for`'s `Some(edge)`. What's left to confirm is that
/// `write::is_loop_edge`'s match arm still lists the SAME six literal
/// strings presets.rs's `loop_cap_key` does — a plain source-substring check
/// against `write.rs`, in the same spirit as
/// `lint::schema_parity::reminder_hard_path_subset`.
#[test]
fn loop_cap_keys_match_write_is_loop_edge_source_literals() {
    let write_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/write.rs"));
    let start = write_src
        .find("fn is_loop_edge(edge: &str) -> bool {")
        .expect("write.rs must still define fn is_loop_edge");
    let after_signature = &write_src[start..];
    let body_end = after_signature[1..]
        .find("\nfn ")
        .map(|i| i + 1)
        .unwrap_or(after_signature.len());
    let body = &after_signature[..body_end];

    for key in [
        "deep_review:phase_fixing",
        "cross_review:cross_fixing",
        "termination_review:phase_fixing",
        "phase_review:phase_fixing",
        "review_of_review:counter_review",
        "counter_review:review_of_review",
    ] {
        assert!(
            body.contains(&format!("\"{key}\"")),
            "write::is_loop_edge must still match {key:?} (presets.rs transcribes it as loop-capped)"
        );
    }
}
