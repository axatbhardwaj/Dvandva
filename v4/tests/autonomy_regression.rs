//! Regressions for the amended scope: a run must be able to proceed on its own
//! when the human is absent, and its routine operations must not look like
//! crashes to the surrounding harness.

use dvandva_v4::{
    claim::Role,
    model::{
        Assignee, DeliverableRequirement, HandoffKind, PublicationPolicy, RunBaton, Status,
        EXPLAINER_ACCESS, EXPLAINER_CHANNEL, LEGACY_EXPLAINER_ACCESS, LEGACY_EXPLAINER_CHANNEL,
    },
    next_action,
};

fn baton() -> RunBaton {
    RunBaton::new(
        "run-a",
        "Objective",
        "claude",
        "codex",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .unwrap()
}

/// A protocol-internal problem must never leave asking a human as the only way
/// forward, because during an autonomous run there may be nobody to ask.
#[test]
fn no_reachable_state_leaves_human_decision_as_the_only_way_forward() {
    for (status, assignee) in [
        (Status::Working, Assignee::Worker),
        (Status::Revising, Assignee::Worker),
        (Status::Reviewing, Assignee::Reviewer),
        (Status::Reviewing, Assignee::Worker),
        (Status::Finalizing, Assignee::Worker),
        (Status::Working, Assignee::Reviewer),
        (Status::Revising, Assignee::Reviewer),
        (Status::Finalizing, Assignee::Reviewer),
    ] {
        for role in [Role::Worker, Role::Reviewer] {
            for harness in ["Claude", "Codex", "Other"] {
                let mut baton = baton();
                baton.status = status.clone();
                baton.assignee = assignee.clone();
                let actions = next_action::classify(&baton, role, harness);
                let autonomous = actions
                    .next_actions
                    .iter()
                    .filter(|action| **action != "request_human_decision")
                    .count();
                assert!(
                    autonomous > 0,
                    "{status:?}/{assignee:?}/{role:?}/{harness} offered no autonomous action"
                );
            }
        }
    }
}

/// A policy the reviewer cannot read is a capability problem, not a scope
/// question. It must be answerable by the kernel's own repair, so the run is
/// never parked waiting for a human to relay bytes by hand.
#[test]
fn an_unreadable_policy_is_recognized_without_asking_a_human() {
    let readable = PublicationPolicy::fixed();
    assert_eq!(readable.channel, EXPLAINER_CHANNEL);
    assert_eq!(readable.access, EXPLAINER_ACCESS);
    assert!(readable.reviewer_can_read());
    assert!(readable.is_recognized());

    let owner_only = PublicationPolicy {
        publisher_harness: "Codex".into(),
        channel: LEGACY_EXPLAINER_CHANNEL.into(),
        access: LEGACY_EXPLAINER_ACCESS.into(),
        reviewer_harness: "Claude".into(),
    };
    // Recognized, so it still loads and can be repaired — but never satisfiable.
    assert!(owner_only.is_recognized());
    assert!(!owner_only.reviewer_can_read());

    // The same Site is readable when the publisher is also the reviewer, which
    // is why the mismatch is a property of the pairing, not of Sites as such.
    let self_reviewed = PublicationPolicy {
        reviewer_harness: "Codex".into(),
        ..owner_only.clone()
    };
    assert!(self_reviewed.reviewer_can_read());

    let unknown = PublicationPolicy {
        channel: "somewhere-else".into(),
        ..owner_only
    };
    assert!(!unknown.is_recognized());
    assert!(!unknown.reviewer_can_read());
}

/// Liveness reporting must never make an idle role look like it has work, or
/// every wait would return immediately and the loop would spin.
#[test]
fn reporting_progress_never_makes_an_idle_role_actionable() {
    let mut idle = baton();
    idle.status = Status::Working;
    idle.assignee = Assignee::Worker;
    let waiting = next_action::classify(&idle, Role::Reviewer, "Other");
    assert!(waiting.legal_actions.contains(&"report_progress"));
    assert!(!waiting.advisory_actions.contains(&"report_progress"));
    assert!(
        !waiting.actionable,
        "a role with nothing but liveness and the human escape must keep waiting"
    );

    // The publisher, by contrast, genuinely owes staged bytes and is actionable.
    idle.publication_binding.as_mut().unwrap().obligation.kind = HandoffKind::RunStarted;
    let publisher = next_action::classify(&idle, Role::Reviewer, "Codex");
    assert!(publisher.legal_actions.contains(&"stage_explainer"));
    assert!(publisher.actionable);
}

/// A run created before explainer staging existed must still load, so it can
/// reach `repair_publication_policy` instead of becoming unreadable. This is the
/// exact shape the PR-914-era kernel wrote: an owner-only Site deployment with
/// no staged artifact.
#[test]
fn a_pre_staging_baton_still_loads_and_can_be_repaired() {
    use dvandva_v4::store::RunChannel;

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    std::fs::create_dir_all(run_dir.join("history")).unwrap();
    let baton = serde_json::json!({
        "schema": "dvandva.run.v2",
        "run_id": "run-a",
        "objective": {"summary": "Fix the protocol"},
        "workspace": {
            "repository_id": "github.com/axatbhardwaj/dvandva",
            "origin": "https://github.com/axatbhardwaj/Dvandva",
            "worktree": null
        },
        "task": {"reference": null, "summary": "Fix the protocol"},
        "participants": {
            "worker": {"harness": "Claude", "claim": null},
            "reviewer": {"harness": "Codex", "claim": null}
        },
        "status": "working",
        "assignee": "worker",
        "revision": 0,
        "scope_revision": 0,
        "scope_deliverables": [{"id": "kernel", "description": "Fix the kernel"}],
        "checkpoint": null,
        "review": null,
        "publication_policy": {
            "publisher_harness": "Codex",
            "channel": "codex_sites",
            "access": "owner_only",
            "reviewer_harness": "Claude"
        },
        "publication_binding": {
            "site_id": "appgprj_deadbeef",
            "obligation": {"handoff_revision": 0, "kind": "run_started", "scope_revision": 0},
            "deployment": {
                "obligation": {"handoff_revision": 0, "kind": "run_started", "scope_revision": 0},
                "source_digest": "8888888888888888888888888888888888888888888888888888888888888888",
                "site_id": "appgprj_deadbeef",
                "site_version": "1",
                "url": "https://example.chatgpt.site",
                "channel": "codex_sites",
                "access": "owner_only",
                "publisher_harness": "Codex"
            },
            "review": null
        },
        "human_decision": null,
        "predecessor_run_id": null,
        "terminal": null,
        "recovery": null
    });
    let bytes = serde_json::to_vec_pretty(&baton).unwrap();
    std::fs::write(run_dir.join("history/00000000000000000000.json"), &bytes).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();

    // It loads: an unreadable policy must be repairable, not unreachable.
    let channel = RunChannel::open(&run_dir);
    let loaded = channel.read().expect("a pre-staging baton must still load");
    assert!(loaded
        .publication_binding
        .as_ref()
        .unwrap()
        .artifact
        .is_none());
    assert!(loaded
        .publication_binding
        .as_ref()
        .unwrap()
        .deployment
        .is_some());

    // And the Site alone never gates anything, however it was recorded.
    let actions = next_action::classify(&loaded, Role::Worker, "Claude");
    assert!(actions.legal_actions.contains(&"submit_checkpoint"));

    let repaired = dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        Role::Worker,
        "claude-session",
        0,
    )
    .expect("an unreadable policy must be repairable");
    assert_eq!(
        repaired.publication_policy,
        Some(PublicationPolicy::fixed())
    );
    let binding = repaired.publication_binding.as_ref().unwrap();
    assert!(binding.artifact.is_none());
    assert!(binding.deployment.is_none());
    assert!(binding.review.is_none());
    // The stable Site ID survives, so a later rendering keeps one identity.
    assert_eq!(binding.site_id.as_deref(), Some("appgprj_deadbeef"));
}

/// A worker that has handed a checkpoint to the reviewer must rest. Escape
/// hatches stay available but are never wake reasons, or the foreground wait
/// returns immediately and the role spins for the whole review.
#[test]
fn escape_hatches_never_make_a_waiting_role_actionable() {
    let mut reviewing = baton();
    reviewing.status = Status::Reviewing;
    reviewing.assignee = Assignee::Reviewer;
    let worker = next_action::classify(&reviewing, Role::Worker, "Claude");
    assert!(
        worker
            .legal_actions
            .contains(&"request_checkpoint_supersession"),
        "the worker must still be able to report newly discovered work"
    );
    assert!(
        !worker.actionable,
        "a worker awaiting a verdict must rest, not spin"
    );

    let mut finalizing = baton();
    finalizing.status = Status::Finalizing;
    finalizing.assignee = Assignee::Worker;
    let ungated = next_action::classify(&finalizing, Role::Worker, "Claude");
    assert!(ungated.legal_actions.contains(&"withdraw_approval"));
    assert!(!ungated.legal_actions.contains(&"finalize"));
    assert!(
        !ungated.actionable,
        "withdrawal alone is an escape hatch, not work the protocol is waiting on"
    );
    assert_eq!(
        ungated.blocking_reason,
        Some("finalize awaits current explainer approval")
    );
}
