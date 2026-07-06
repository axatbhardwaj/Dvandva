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

use common::{make_baton_v2, v2_status_owner};
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
