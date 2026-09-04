//! Regressions for the amended scope: a run must be able to proceed on its own
//! when the human is absent, and its routine operations must not look like
//! crashes to the surrounding harness.

use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

use dvandva_v4::{
    claim::Role,
    model::{
        Assignee, DeliverableRequirement, ExplainerArtifact, HandoffKind, PublicationDeployment,
        PublicationPolicy, PublicationReview, RunBaton, Status, EXPLAINER_ACCESS,
        EXPLAINER_CHANNEL, LEGACY_EXPLAINER_ACCESS, LEGACY_EXPLAINER_CHANNEL,
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

    // Self-review does not make an owner-only Site reviewer-readable; the
    // two-party gate always reviews the local artifact before publication.
    let self_reviewed = PublicationPolicy {
        reviewer_harness: "Codex".into(),
        ..owner_only.clone()
    };
    assert!(!self_reviewed.reviewer_can_read());

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

    // Vadi, by contrast, genuinely owes staged bytes and is actionable,
    // regardless of whether that role is Claude or Codex.
    idle.publication_binding.as_mut().unwrap().obligation.kind = HandoffKind::RunStarted;
    let author = next_action::classify(&idle, Role::Worker, "Claude");
    assert!(author.legal_actions.contains(&"stage_explainer"));
    assert!(author.actionable);
}

#[test]
fn approved_local_bytes_make_private_sites_publication_actionable_and_required() {
    let mut run = baton();
    run.status = Status::Finalizing;
    run.assignee = Assignee::Worker;
    let binding = run.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "a".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Claude".into(),
    });

    let before_review = next_action::classify(&run, Role::Reviewer, "Codex");
    assert!(!before_review.legal_actions.contains(&"publish_explainer"));

    run.publication_binding.as_mut().unwrap().review = Some(PublicationReview {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "Codex".into(),
    });
    let publisher = next_action::classify(&run, Role::Reviewer, "Codex");
    assert!(publisher.legal_actions.contains(&"publish_explainer"));
    assert!(publisher.actionable);

    let blocked_finalizer = next_action::classify(&run, Role::Worker, "Claude");
    assert!(!blocked_finalizer.legal_actions.contains(&"finalize"));
    assert_eq!(
        blocked_finalizer.blocking_reason,
        Some("finalize awaits current explainer publication and approval")
    );

    let binding = run.publication_binding.as_mut().unwrap();
    binding.site_id = Some("site-run".into());
    binding.deployment = Some(PublicationDeployment {
        obligation,
        source_digest: digest,
        site_id: "site-run".into(),
        site_version: "version-1".into(),
        url: "https://sites.openai.test/site-run/version-1".into(),
        channel: LEGACY_EXPLAINER_CHANNEL.into(),
        access: LEGACY_EXPLAINER_ACCESS.into(),
        publisher_harness: "Codex".into(),
    });
    let finalizer = next_action::classify(&run, Role::Worker, "Claude");
    assert!(finalizer.legal_actions.contains(&"finalize"));
}

#[test]
fn approved_reverse_cast_receipts_from_v0_3_2_remain_actionable() {
    let mut run = baton();
    run.publication_policy = Some(PublicationPolicy::fixed());
    run.status = Status::Finalizing;
    run.assignee = Assignee::Worker;
    let binding = run.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "b".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Codex".into(),
    });
    binding.review = Some(PublicationReview {
        obligation,
        source_digest: digest,
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "Claude".into(),
    });

    assert!(run.local_explainer_approved(run.publication_binding.as_ref().unwrap()));
    let codex_reviewer = next_action::classify(&run, Role::Reviewer, "Codex");
    assert!(codex_reviewer.legal_actions.contains(&"publish_explainer"));
    assert!(codex_reviewer.actionable);
}

#[test]
fn reverse_cast_receipts_can_span_the_v0_3_2_upgrade_boundary() {
    let mut run = baton();
    run.publication_policy = Some(PublicationPolicy::fixed());
    let binding = run.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "d".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Codex".into(),
    });
    binding.review = Some(PublicationReview {
        obligation,
        source_digest: digest,
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "Codex".into(),
    });

    assert!(!run.local_explainer_approved(run.publication_binding.as_ref().unwrap()));
    let claude_worker = next_action::classify(&run, Role::Worker, "Claude");
    assert!(claude_worker.legal_actions.contains(&"stage_explainer"));
    let codex_reviewer = next_action::classify(&run, Role::Reviewer, "Codex");
    assert!(!codex_reviewer.legal_actions.contains(&"publish_explainer"));
    assert!(!codex_reviewer.legal_actions.contains(&"review_explainer"));
}

#[test]
fn a_pairing_without_codex_skips_the_sites_receipt() {
    let mut run = RunBaton::new(
        "run-no-codex",
        "Objective",
        "Fable",
        "Grok",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .expect("non-Codex harness pairings are constructible");
    assert_eq!(run.participants.worker.harness, "fable");
    assert_eq!(run.participants.reviewer.harness, "grok");
    run.status = Status::Finalizing;
    run.assignee = Assignee::Worker;
    assert_eq!(
        next_action::classify(&run, Role::Worker, "fable").blocking_reason,
        Some("finalize awaits current explainer approval")
    );
    let binding = run.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "c".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "fable".into(),
    });
    binding.review = Some(PublicationReview {
        obligation,
        source_digest: digest,
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "grok".into(),
    });

    let finalizer = next_action::classify(&run, Role::Worker, "fable");
    assert!(finalizer.legal_actions.contains(&"finalize"));
    assert!(!finalizer.legal_actions.contains(&"publish_explainer"));
    assert!(!next_action::classify(&run, Role::Reviewer, "grok")
        .legal_actions
        .contains(&"publish_explainer"));
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
    // Revision 0 is a clean creation root; the Site receipt is revision 1, the
    // way a 0.2 kernel actually wrote it.
    let mut root = baton.clone();
    root["publication_binding"] = serde_json::json!({
        "obligation": {"handoff_revision": 0, "kind": "run_started", "scope_revision": 0},
        "deployment": null,
        "review": null
    });
    let bytes = serde_json::to_vec_pretty(&root).unwrap();
    std::fs::write(run_dir.join("history/00000000000000000000.json"), &bytes).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
    // A 0.2 kernel wrote its Site receipt without a receipt sequence. That is
    // stored history now — a live append must advance the sequence — so it is
    // written to disk the way the old kernel left it, not appended live.
    let channel = RunChannel::open(&run_dir);
    let mut with_site = baton.clone();
    with_site["revision"] = serde_json::json!(1);
    let bytes = serde_json::to_vec_pretty(&with_site).unwrap();
    std::fs::write(run_dir.join("history/00000000000000000001.json"), &bytes).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();

    // It loads: an unreadable policy must be repairable, not unreachable.
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

    // And the Site alone never gates anything, however it was recorded: a
    // finished deliverable still has somewhere to land, while the unapproved
    // run_started explainer keeps work off the advisory list.
    let actions = next_action::classify(&loaded, Role::Worker, "Claude");
    assert!(actions.legal_actions.contains(&"submit_checkpoint"));
    assert!(actions.legal_actions.contains(&"stage_explainer"));
    assert!(!actions.advisory_actions.contains(&"work"));

    let credentials = dir.path().join("credentials");
    let repaired = dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        &credentials,
        Role::Worker,
        "claude-session",
        "claude",
        "codex",
        1,
    )
    .expect("an unreadable policy must be repairable");

    // Repair is scoped: a caller naming the wrong topology is refused.
    assert!(dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        &credentials,
        Role::Worker,
        "claude-session",
        "codex",
        "claude",
        repaired.revision,
    )
    .is_err());
    assert_eq!(
        repaired.publication_policy,
        Some(PublicationPolicy::for_participants("Claude", "Codex"))
    );
    let binding = repaired.publication_binding.as_ref().unwrap();
    assert!(binding.artifact.is_none());
    assert!(binding.deployment.is_none());
    assert!(binding.review.is_none());
    // The stable Site ID survives, so a later rendering keeps one identity.
    assert_eq!(binding.site_id.as_deref(), Some("appgprj_deadbeef"));
}

/// v0.3.2 allowed Sites publication immediately after staging. A stored
/// stage -> publish -> review chain must remain repairable even though current
/// live writes require review before publication.
#[test]
fn a_publish_before_review_v0_3_2_chain_can_be_repaired() {
    use dvandva_v4::store::RunChannel;

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    std::fs::create_dir_all(run_dir.join("history")).unwrap();
    let obligation = serde_json::json!({
        "handoff_revision": 0,
        "kind": "run_started",
        "scope_revision": 0
    });
    let digest = "8".repeat(64);
    let mut root = serde_json::json!({
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
            "worker": {"harness": "Codex", "claim": null},
            "reviewer": {"harness": "Claude", "claim": null}
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
            "site_id": null,
            "obligation": obligation,
            "receipt_seq": 0,
            "deployment": null,
            "review": null
        },
        "human_decision": null,
        "predecessor_run_id": null,
        "terminal": null,
        "recovery": null
    });
    let mut revisions = vec![root.clone()];

    root["revision"] = serde_json::json!(1);
    root["publication_binding"]["receipt_seq"] = serde_json::json!(1);
    root["publication_binding"]["artifact"] = serde_json::json!({
        "obligation": obligation,
        "source_digest": digest,
        "path": format!("explainer/{digest}.html"),
        "media_type": "text/html",
        "byte_length": 1,
        "channel": "run_artifact",
        "access": "run_private",
        "publisher_harness": "Codex"
    });
    revisions.push(root.clone());

    root["revision"] = serde_json::json!(2);
    root["publication_binding"]["receipt_seq"] = serde_json::json!(2);
    root["publication_binding"]["site_id"] = serde_json::json!("appgprj_deadbeef");
    root["publication_binding"]["deployment"] = serde_json::json!({
        "obligation": obligation,
        "source_digest": digest,
        "site_id": "appgprj_deadbeef",
        "site_version": "1",
        "url": "https://example.chatgpt.site",
        "channel": "codex_sites",
        "access": "owner_only",
        "publisher_harness": "Codex"
    });
    revisions.push(root.clone());

    root["revision"] = serde_json::json!(3);
    root["publication_binding"]["receipt_seq"] = serde_json::json!(3);
    root["publication_binding"]["review"] = serde_json::json!({
        "obligation": obligation,
        "source_digest": digest,
        "verdict": "approved",
        "findings": [],
        "reviewer_harness": "Claude"
    });
    revisions.push(root.clone());

    for (revision, baton) in revisions.iter().enumerate() {
        let bytes = serde_json::to_vec_pretty(baton).unwrap();
        std::fs::write(run_dir.join(format!("history/{revision:020}.json")), &bytes).unwrap();
    }
    std::fs::write(
        run_dir.join("baton.json"),
        serde_json::to_vec_pretty(revisions.last().unwrap()).unwrap(),
    )
    .unwrap();

    let channel = RunChannel::open(&run_dir);
    assert_eq!(channel.read().unwrap().revision, 3);
    let repaired = dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        &dir.path().join("credentials"),
        Role::Worker,
        "codex-session",
        "codex",
        "claude",
        3,
    )
    .expect("a released publish-before-review chain must remain repairable");
    assert_eq!(repaired.revision, 4);
    assert_eq!(
        repaired.publication_policy,
        Some(PublicationPolicy::for_participants("Codex", "Claude"))
    );
}

/// A worker that has handed a checkpoint to the reviewer must rest. Escape
/// hatches stay available but are never wake reasons, or the foreground wait
/// returns immediately and the role spins for the whole review.
#[test]
fn escape_hatches_never_make_a_waiting_role_actionable() {
    let mut reviewing = baton();
    reviewing.status = Status::Reviewing;
    reviewing.assignee = Assignee::Reviewer;
    let binding = reviewing.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "a".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation,
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Claude".into(),
    });
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
    let binding = finalizing.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "b".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Claude".into(),
    });
    binding.review = Some(PublicationReview {
        obligation,
        source_digest: digest,
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "Codex".into(),
    });
    let ungated = next_action::classify(&finalizing, Role::Worker, "Claude");
    assert!(ungated.legal_actions.contains(&"withdraw_approval"));
    assert!(!ungated.legal_actions.contains(&"finalize"));
    assert!(
        !ungated.actionable,
        "withdrawal alone is an escape hatch, not work the protocol is waiting on"
    );
    assert_eq!(
        ungated.blocking_reason,
        Some("finalize awaits current explainer publication and approval")
    );
}

/// Obligation-bound writes waive the revision precondition, so they need their
/// own ordering and replay rules: an exact retry must be a no-op, and a late
/// delivery must not overwrite a newer verdict for the same bytes.
#[test]
fn obligation_bound_writes_replay_cleanly_and_reject_stale_verdicts() {
    use dvandva_v4::{
        action::{Action, ReviewVerdict},
        store::RunChannel,
        transition,
    };

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let mut created = RunBaton::new(
        "run-a",
        "Objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .unwrap();
    created.workspace = Some(dvandva_v4::model::WorkspaceIdentity {
        repository_id: "github.com/axatbhardwaj/dvandva".into(),
        origin: None,
        worktree: None,
    });
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();

    let worker =
        dvandva_v4::claim::claim(&channel, Role::Worker, "codex-session", 1800, 0).unwrap();
    let reviewer =
        dvandva_v4::claim::claim(&channel, Role::Reviewer, "claude-session", 1800, 1).unwrap();

    let source = dir.path().join("explainer.html");
    std::fs::write(&source, b"<h1>bytes</h1>").unwrap();
    let obligation = channel
        .read()
        .unwrap()
        .publication_binding
        .unwrap()
        .obligation;
    let stage = || Action::StageExplainer {
        obligation: obligation.clone(),
        after_seq: None,
        source_path: source.clone(),
    };

    let staged = transition::apply(
        &channel,
        Role::Worker,
        "codex-session",
        &worker.token,
        2,
        stage(),
    )
    .unwrap();
    let digest = staged
        .publication_binding
        .as_ref()
        .unwrap()
        .artifact
        .as_ref()
        .unwrap()
        .source_digest
        .clone();

    // Replay of the identical write is a no-op, not a new revision.
    let replayed = transition::apply(
        &channel,
        Role::Worker,
        "codex-session",
        &worker.token,
        2,
        stage(),
    )
    .unwrap();
    assert_eq!(replayed.revision, staged.revision);
    assert_eq!(channel.read().unwrap().revision, staged.revision);

    let review = |verdict, findings: Vec<String>, after_seq| Action::RecordExplainerReview {
        obligation: obligation.clone(),
        after_seq,
        source_digest: digest.clone(),
        verdict,
        findings,
    };
    transition::apply(
        &channel,
        Role::Reviewer,
        "claude-session",
        &reviewer.token,
        3,
        review(
            ReviewVerdict::ChangesRequested,
            vec!["rework".into()],
            Some(1),
        ),
    )
    .unwrap();

    // A late approval for the same bytes must not overwrite the newer verdict
    // and silently unblock finalization.
    let stale = transition::apply(
        &channel,
        Role::Reviewer,
        "claude-session",
        &reviewer.token,
        3,
        review(ReviewVerdict::Approved, Vec::new(), Some(2)),
    );
    assert!(
        matches!(
            stale,
            Err(transition::TransitionError::StalePublicationBinding)
        ),
        "a late differing verdict for the same bytes must be rejected"
    );
    let head = channel.read().unwrap();
    assert_eq!(
        head.publication_binding.unwrap().review.unwrap().verdict,
        "changes_requested"
    );
}

/// An `analysis` checkpoint has no commit to materialize, so its digests are
/// only meaningful if the bytes behind them are staged and readable. A manifest
/// may not cite a digest this run never staged.
#[test]
fn an_analysis_checkpoint_must_cite_bytes_the_reviewer_can_materialize() {
    use dvandva_v4::{action::Action, model::CheckpointSubmission, store::RunChannel, transition};

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let mut created = RunBaton::new(
        "run-a",
        "Objective",
        "claude",
        "codex",
        vec![DeliverableRequirement {
            id: "review".into(),
            description: "Review package".into(),
        }],
    )
    .unwrap();
    created.workspace = Some(dvandva_v4::model::WorkspaceIdentity {
        repository_id: "github.com/axatbhardwaj/dvandva".into(),
        origin: None,
        worktree: None,
    });
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();
    let worker =
        dvandva_v4::claim::claim(&channel, Role::Worker, "claude-session", 1800, 0).unwrap();

    let submit = |identity: &str, digest: &str| Action::SubmitCheckpoint {
        checkpoint: CheckpointSubmission {
            kind: "analysis".into(),
            identity: identity.to_owned(),
            deliverables: vec![dvandva_v4::model::CheckpointDeliverable {
                id: "review".into(),
                artifacts: vec![dvandva_v4::model::ExternalRef {
                    kind: "analysis_digest".into(),
                    value: digest.to_owned(),
                }],
            }],
            verification: vec!["read the review".into()],
        },
    };

    // Citing an unstaged digest is refused: nothing would be materializable.
    let unstaged = "b".repeat(64);
    let refused = transition::apply(
        &channel,
        Role::Worker,
        "claude-session",
        &worker.token,
        1,
        submit(&"a".repeat(64), &unstaged),
    );
    assert!(matches!(
        refused,
        Err(transition::TransitionError::AnalysisNotStaged)
    ));

    // Stage the real bytes, then cite their digest.
    let source = dir.path().join("review.md");
    std::fs::write(&source, b"# Review\nthe analysis deliverable\n").unwrap();
    let staged = transition::apply(
        &channel,
        Role::Worker,
        "claude-session",
        &worker.token,
        1,
        Action::StageAnalysis {
            source_path: source.clone(),
        },
    )
    .unwrap();
    let digest = staged.staged_analysis.first().unwrap().clone();
    assert!(run_dir
        .join(dvandva_v4::model::analysis_artifact_path(&digest))
        .is_file());

    // The identity is derived from the cited digests, so it cannot be chosen.
    let derived = dvandva_v4::model::analysis_checkpoint_identity(std::slice::from_ref(&digest));
    assert!(
        transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &worker.token,
            2,
            submit(&"c".repeat(64), &digest),
        )
        .is_err(),
        "an analysis identity unrelated to its own content must be refused"
    );
    let submitted = transition::apply(
        &channel,
        Role::Worker,
        "claude-session",
        &worker.token,
        2,
        submit(&derived, &digest),
    )
    .unwrap();
    assert_eq!(submitted.status, Status::Reviewing);
    assert_eq!(submitted.checkpoint.as_ref().unwrap().kind, "analysis");

    // The bytes really are on disk at the cited digest, and they hash to it.
    let bytes = std::fs::read(run_dir.join(dvandva_v4::model::analysis_artifact_path(&digest)))
        .expect("a cited analysis digest must be materializable");
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(&bytes)),
        digest,
        "staged analysis bytes must hash to the digest the manifest cites"
    );
}

/// The PR-914 run parked because "unavailable publication capability" was a
/// documented reason to stop and ask. Typing the request is not enough on its
/// own — a label can lie — so the kernel refuses to park a run at all while it
/// still holds a deterministic recovery, whatever the request claims to be.
#[test]
fn a_run_cannot_be_parked_while_the_kernel_can_recover_itself() {
    use dvandva_v4::{
        action::{Action, HumanDecisionRequest},
        model::HumanDecisionKind,
        store::RunChannel,
        transition,
    };

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let mut created = baton();
    created.workspace = Some(dvandva_v4::model::WorkspaceIdentity {
        repository_id: "github.com/axatbhardwaj/dvandva".into(),
        origin: None,
        worktree: None,
    });
    // The incident's own state: a policy whose reviewer cannot read its channel.
    created.publication_policy = Some(PublicationPolicy {
        publisher_harness: "Codex".into(),
        channel: LEGACY_EXPLAINER_CHANNEL.into(),
        access: LEGACY_EXPLAINER_ACCESS.into(),
        reviewer_harness: "Claude".into(),
    });
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();
    let credentials = dir.path().join("credentials");
    dvandva_v4::role_session::claim(
        &run_dir,
        &credentials,
        Role::Worker,
        "claude-session",
        1800,
        0,
    )
    .unwrap();
    let worker =
        dvandva_v4::credential::load(&credentials, "claude-session", "run-a", Role::Worker)
            .unwrap();

    let escalation = |kind| {
        Action::RequestHumanDecision(HumanDecisionRequest {
            kind,
            question: "The explainer Site returns 401. Approve relaying it by hand?".into(),
            evidence: ["the reviewing harness has no session for the Site".into()].into(),
            options: ["relay the text".into(), "stop the run".into()].into(),
            proposals: Vec::new(),
        })
    };

    // The incident's escalation is refused, and relabelling it does not help:
    // the kernel is answering the condition, not the description of it.
    for kind in [
        HumanDecisionKind::Scope,
        HumanDecisionKind::Intent,
        HumanDecisionKind::Authority,
    ] {
        let refused = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &worker.token,
            1,
            escalation(kind),
        );
        assert!(
            matches!(
                refused,
                Err(transition::TransitionError::AutonomousRecoveryAvailable)
            ),
            "{kind:?} must not park a run the kernel can repair"
        );
    }
    // And the run really is still live rather than parked.
    let head = channel.read().unwrap();
    assert_eq!(head.status, Status::Working);
    assert!(head.human_decision.is_none());

    // Once the recovery has been taken, a genuine scope question is allowed
    // again: autonomy is about not stalling on protocol problems, not about
    // refusing to ask what only a human can answer.
    let repaired = dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        &credentials,
        Role::Worker,
        "claude-session",
        "claude",
        "codex",
        head.revision,
    )
    .unwrap();
    let asked = transition::apply(
        &channel,
        Role::Worker,
        "claude-session",
        &worker.token,
        repaired.revision,
        Action::RequestHumanDecision(HumanDecisionRequest {
            kind: HumanDecisionKind::Scope,
            question: "Should the migration guide be in scope?".into(),
            evidence: ["the report names it as adjacent".into()].into(),
            options: ["include it".into(), "leave it out".into()].into(),
            proposals: Vec::new(),
        }),
    )
    .expect("a real scope question must still be possible");
    assert_eq!(asked.status, Status::HumanDecision);
    assert_eq!(
        asked.human_decision.as_ref().unwrap().kind,
        HumanDecisionKind::Scope
    );
}

/// A payload written against the released API 2, which had no `kind`, must
/// still apply: the kernel advertises that API and cannot silently break it.
#[test]
fn a_released_api_2_decision_payload_still_applies() {
    use dvandva_v4::{action::Action, model::HumanDecisionKind};

    let action = serde_json::from_value::<Action>(serde_json::json!({
        "type": "request_human_decision",
        "question": "Which sections are in scope?",
        "evidence": ["the report names two areas"],
        "options": ["both", "only the kernel"],
    }))
    .expect("the released API 2 payload must still deserialize");
    match action {
        Action::RequestHumanDecision(request) => {
            assert_eq!(request.kind, HumanDecisionKind::Scope)
        }
        _ => panic!("unexpected action"),
    }

    // An unknown kind is still refused, so the vocabulary stays closed.
    assert!(serde_json::from_value::<Action>(serde_json::json!({
        "type": "request_human_decision",
        "kind": "approval",
        "question": "Approve the workaround?",
        "evidence": ["blocked"],
        "options": ["yes", "no"],
    }))
    .is_err());
}

/// `access: run_private` is a promise about bytes on disk. A digest-named path
/// that is really a symlink to a world-readable file outside the run must not
/// be stageable, readable, or gate-satisfying.
#[test]
fn run_private_artifacts_cannot_escape_the_run_directory() {
    use dvandva_v4::store;

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let explainer = run_dir.join("explainer");
    std::fs::create_dir_all(&explainer).unwrap();

    let outside = dir.path().join("outside.html");
    std::fs::write(&outside, b"<h1>not in the run</h1>").unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();

    let digest = format!("{:x}", Sha256::digest(b"<h1>not in the run</h1>"));
    let linked = explainer.join(format!("{digest}.html"));
    std::os::unix::fs::symlink(&outside, &linked).unwrap();

    // Reading refuses to follow the link, whatever it points at.
    assert!(
        store::read_private_file(&linked).is_err(),
        "a symlinked artifact must not be readable as run-private"
    );
    assert!(
        !store::is_private_regular_file(&linked),
        "a symlink is never a reusable private artifact"
    );

    // A world-readable regular file inside the run is refused too: the promise
    // is about who can read it, not only about where it lives.
    let exposed = explainer.join("exposed.html");
    std::fs::write(&exposed, b"<h1>exposed</h1>").unwrap();
    std::fs::set_permissions(&exposed, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store::read_private_file(&exposed).is_err());
    assert!(!store::is_private_regular_file(&exposed));

    // A private regular file inside the run is accepted.
    let owned = explainer.join("owned.html");
    std::fs::write(&owned, b"<h1>owned</h1>").unwrap();
    std::fs::set_permissions(&owned, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(store::read_private_file(&owned).unwrap(), b"<h1>owned</h1>");
    assert!(store::is_private_regular_file(&owned));

    // A symlinked run-state directory is refused rather than followed.
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let linked_dir = run_dir.join("analysis");
    std::os::unix::fs::symlink(&elsewhere, &linked_dir).unwrap();
    assert!(store::create_private_dir(&linked_dir).is_err());
}

/// A reclaimed role starts a new epoch. Presenting the previous session's phase
/// as the current one is exactly the inference `progress` exists to prevent.
#[test]
fn a_new_claim_epoch_does_not_inherit_the_previous_session_s_progress() {
    use dvandva_v4::{action::Action, claim, model::ProgressPhase, store::RunChannel, transition};

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let mut created = baton();
    created.workspace = Some(dvandva_v4::model::WorkspaceIdentity {
        repository_id: "github.com/axatbhardwaj/dvandva".into(),
        origin: None,
        worktree: None,
    });
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();

    // A first session reports progress, then its lease lapses.
    let first = claim::claim(&channel, Role::Worker, "session-one", 1, 0).unwrap();
    let reported = transition::apply(
        &channel,
        Role::Worker,
        "session-one",
        &first.token,
        1,
        Action::ReportProgress {
            phase: ProgressPhase::PublishingExplainer,
            detail: Some("halfway through a long build".into()),
        },
    )
    .unwrap();
    assert_eq!(
        reported
            .participants
            .worker
            .progress
            .as_ref()
            .unwrap()
            .phase,
        ProgressPhase::PublishingExplainer
    );
    while claim::verify(
        &channel.read().unwrap(),
        Role::Worker,
        "session-one",
        &first.token,
    )
    .is_ok()
    {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // The next epoch reports nothing until it reports something.
    let head = channel.read().unwrap();
    let second =
        claim::reclaim(&channel, Role::Worker, "session-two", 1800, head.revision).unwrap();
    assert_eq!(second.epoch, 2);
    let after = channel.read().unwrap();
    assert!(
        after.participants.worker.progress.is_none(),
        "a reclaim must not present the previous session's activity as current"
    );
}

/// Obligation-bound writes waive the revision precondition, so they carry a
/// targeted one instead. Unrelated peer activity must not invalidate a prepared
/// receipt, but a delayed or out-of-order receipt must not overwrite newer state.
#[test]
fn a_receipt_prepared_against_older_state_is_refused_not_applied() {
    use dvandva_v4::{
        action::{Action, ReviewVerdict},
        claim,
        model::ProgressPhase,
        store::RunChannel,
        transition,
    };

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let mut created = RunBaton::new(
        "run-a",
        "Objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .unwrap();
    created.workspace = Some(dvandva_v4::model::WorkspaceIdentity {
        repository_id: "github.com/axatbhardwaj/dvandva".into(),
        origin: None,
        worktree: None,
    });
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();
    let worker = claim::claim(&channel, Role::Worker, "codex-session", 1800, 0).unwrap();
    let reviewer = claim::claim(&channel, Role::Reviewer, "claude-session", 1800, 1).unwrap();
    let obligation = channel
        .read()
        .unwrap()
        .publication_binding
        .unwrap()
        .obligation;

    let stage = |label: &str, after_seq| {
        let source = dir.path().join(format!("{label}.html"));
        std::fs::write(&source, format!("<h1>{label}</h1>")).unwrap();
        Action::StageExplainer {
            obligation: obligation.clone(),
            after_seq,
            source_path: source,
        }
    };

    // A receipt prepared at sequence 0 lands.
    let first = transition::apply(
        &channel,
        Role::Worker,
        "codex-session",
        &worker.token,
        2,
        stage("first", Some(0)),
    )
    .unwrap();
    assert_eq!(first.publication_binding.as_ref().unwrap().receipt_seq, 1);

    // An unrelated progress edge from the peer must not invalidate a receipt
    // prepared against sequence 1: only receipts advance the sequence.
    transition::apply(
        &channel,
        Role::Reviewer,
        "claude-session",
        &reviewer.token,
        3,
        Action::ReportProgress {
            phase: ProgressPhase::Waiting,
            detail: None,
        },
    )
    .unwrap();
    let second = transition::apply(
        &channel,
        Role::Worker,
        "codex-session",
        &worker.token,
        3,
        stage("second", Some(1)),
    )
    .expect("an unrelated progress edge must not invalidate a prepared receipt");
    let current_digest = second
        .publication_binding
        .as_ref()
        .unwrap()
        .artifact
        .as_ref()
        .unwrap()
        .source_digest
        .clone();

    // A receipt prepared against the superseded state is refused rather than
    // silently replacing the newer bytes.
    let delayed = transition::apply(
        &channel,
        Role::Worker,
        "codex-session",
        &worker.token,
        4,
        stage("delayed", Some(1)),
    );
    assert!(
        matches!(
            delayed,
            Err(transition::TransitionError::StaleReceiptSequence)
        ),
        "a delayed receipt must not overwrite newer staged bytes"
    );
    assert_eq!(
        channel
            .read()
            .unwrap()
            .publication_binding
            .unwrap()
            .artifact
            .unwrap()
            .source_digest,
        current_digest
    );

    // The same rule protects verdicts: a late approval cannot replace a newer
    // changes_requested, even through a direct compare-and-swap.
    transition::apply(
        &channel,
        Role::Reviewer,
        "claude-session",
        &reviewer.token,
        5,
        Action::RecordExplainerReview {
            obligation: obligation.clone(),
            after_seq: Some(2),
            source_digest: current_digest.clone(),
            verdict: ReviewVerdict::ChangesRequested,
            findings: vec!["rework".into()],
        },
    )
    .unwrap();
    let head = channel.read().unwrap();
    let mut forged = head.clone();
    forged.revision += 1;
    let binding = forged.publication_binding.as_mut().unwrap();
    binding.receipt_seq += 1;
    binding.review.as_mut().unwrap().verdict = "approved".to_owned();
    binding.review.as_mut().unwrap().findings.clear();
    assert!(
        channel.compare_and_swap(head.revision, &forged).is_err(),
        "a verdict bound to these exact bytes must not be flipped in place"
    );
}

/// Round-three findings, each reproduced the way the reviewer reproduced it.
mod round_three {
    use super::*;
    use dvandva_v4::{
        action::{Action, ReviewVerdict},
        claim,
        model::{HumanDecision, WorkspaceIdentity},
        role_session, store,
        store::RunChannel,
        transition,
    };

    fn created_run(dir: &std::path::Path, worker: &str, reviewer: &str) -> RunChannel {
        let run_dir = dir.join("run-a");
        let mut created = RunBaton::new(
            "run-a",
            "Objective",
            worker,
            reviewer,
            vec![DeliverableRequirement {
                id: "kernel".into(),
                description: "Fix the kernel".into(),
            }],
        )
        .unwrap();
        created.workspace = Some(WorkspaceIdentity {
            repository_id: "github.com/axatbhardwaj/dvandva".into(),
            origin: None,
            worktree: None,
        });
        let channel = RunChannel::open(&run_dir);
        channel.create(&created).unwrap();
        channel
    }

    /// Rewrite the head and every history revision through `edit`, keeping the
    /// chain self-consistent, the way a run written by an older kernel looks.
    fn rewrite_run(run_dir: &std::path::Path, edit: impl Fn(&mut serde_json::Value)) {
        let mut paths = vec![run_dir.join("baton.json")];
        paths.extend(
            std::fs::read_dir(run_dir.join("history"))
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json")),
        );
        for path in paths {
            let mut baton: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            edit(&mut baton);
            std::fs::write(&path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
        }
    }

    /// [P0] A tampered earlier revision must not be laundered forward by repair,
    /// and a role with a live claim can only be repaired by that claim's session.
    #[test]
    fn repair_walks_the_whole_chain_and_proves_a_live_claim() {
        let dir = tempfile::tempdir().unwrap();
        let channel = created_run(dir.path(), "claude", "codex");
        let run_dir = dir.path().join("run-a");
        let credentials = dir.path().join("credentials");
        // A live worker claim held by the real session.
        let granted =
            role_session::claim(&run_dir, &credentials, Role::Worker, "owner", 1800, 0).unwrap();
        rewrite_run(&run_dir, |baton| {
            baton["publication_policy"]["channel"] = serde_json::json!("codex_sites");
            baton["publication_policy"]["access"] = serde_json::json!("owner_only");
        });
        let head = channel.read().unwrap();
        assert_eq!(head.revision, granted.revision);

        // Another session naming the right topology is refused while the claim
        // is live: it cannot prove it owns the role.
        let intruder = role_session::repair_publication_policy(
            &run_dir,
            &credentials,
            Role::Worker,
            "intruder",
            "claude",
            "codex",
            head.revision,
        );
        assert!(intruder.is_err(), "a live claim must gate repair");

        // Tamper revision 0 while the head still matches its own file: a
        // head-only check would miss it; the chain walk must not.
        let revision_zero = run_dir.join("history/00000000000000000000.json");
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&revision_zero).unwrap()).unwrap();
        tampered["objective"]["summary"] = serde_json::json!("something else entirely");
        std::fs::write(
            &revision_zero,
            serde_json::to_vec_pretty(&tampered).unwrap(),
        )
        .unwrap();
        let laundered = role_session::repair_publication_policy(
            &run_dir,
            &credentials,
            Role::Worker,
            "owner",
            "claude",
            "codex",
            head.revision,
        );
        assert!(
            matches!(
                laundered,
                Err(role_session::RoleSessionError::Store(
                    store::StoreError::InvalidHistory
                ))
            ),
            "repair must refuse to build on a tampered chain: {laundered:?}"
        );
        assert_eq!(channel.read().unwrap().revision, head.revision);
    }

    /// [P1 autonomy] A run this exact problem parked resumes when the problem is
    /// repaired; nobody has to answer a question the repair already answered.
    #[test]
    fn repairing_the_condition_that_parked_a_run_resumes_it_without_a_human() {
        let dir = tempfile::tempdir().unwrap();
        let channel = created_run(dir.path(), "claude", "codex");
        let run_dir = dir.path().join("run-a");
        rewrite_run(&run_dir, |baton| {
            baton["publication_policy"]["channel"] = serde_json::json!("codex_sites");
            baton["publication_policy"]["access"] = serde_json::json!("owner_only");
        });
        // Park it the way the incident did, as a validated revision.
        let mut parked = serde_json::to_value(channel.read().unwrap()).unwrap();
        parked["revision"] = serde_json::json!(1);
        parked["status"] = serde_json::json!("human_decision");
        parked["assignee"] = serde_json::json!("human");
        // The shape the released kernel left the PR-914 run in: untyped,
        // unversioned, parked while the policy was unreadable.
        parked["human_decision"] = serde_json::json!({
            "question": "The Site returns 401. Relay it by hand?",
            "requested_by": "worker",
            "evidence": ["HTTP 401 from the owner-only Site"],
            "options": ["relay", "stop"],
            "contact_role": "worker",
            "resume_status": "working",
            "resume_assignee": "worker",
            "answer": null
        });
        // Written as the released kernel wrote it — stored history, not a live
        // append, which would rightly refuse an unversioned decision.
        let mut bytes = serde_json::to_vec_pretty(&parked).unwrap();
        bytes.push(b'\n');
        std::fs::write(run_dir.join("history/00000000000000000001.json"), &bytes).unwrap();
        std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
        let repaired = role_session::repair_publication_policy(
            &run_dir,
            &dir.path().join("credentials"),
            Role::Worker,
            "claude-session",
            "claude",
            "codex",
            1,
        )
        .expect("a parked unreadable-policy run must be repairable");
        // The capability problem is gone; the human-owned gate is not. A
        // decision recorded under the released rules carries no provenance
        // proving what caused it, so repair must not answer it.
        assert_eq!(
            repaired.publication_policy.as_ref().unwrap(),
            &PublicationPolicy::for_participants("Claude", "Codex")
        );
        assert_eq!(repaired.status, Status::HumanDecision);
        assert_eq!(repaired.assignee, Assignee::Human);
        let decision: &HumanDecision = repaired.human_decision.as_ref().unwrap();
        assert!(
            decision.answer.is_none(),
            "repair must not synthesize an answer"
        );
        assert_eq!(decision.version, 1);
        // The contact role can now clear it under the decision's own rules —
        // an answer-only resume, as the released client would send.
        let credentials = dir.path().join("credentials");
        role_session::claim(
            &run_dir,
            &credentials,
            Role::Worker,
            "claude-session",
            1800,
            repaired.revision,
        )
        .unwrap();
        let token =
            dvandva_v4::credential::load(&credentials, "claude-session", "run-a", Role::Worker)
                .unwrap()
                .token;
        let resumed = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            repaired.revision + 1,
            Action::ResumeHumanDecision {
                answer: "relay".into(),
                scope_amendment: None,
            },
        )
        .expect("the human's answer clears a released-format decision");
        assert_eq!(resumed.status, Status::Working);
        // The run resumes autonomously: with the run_started explainer not yet
        // approved, the worker's owed action is staging it, not bare work.
        let actions = next_action::classify(&resumed, Role::Worker, "Claude");
        assert!(actions.next_actions.contains(&"stage_explainer"));
        assert!(actions.actionable);
    }

    /// [P1 compatibility] Checkpoints a 0.2 kernel accepted — `git` with
    /// `HEAD`, `analysis` with a free-form identity and a `document` artifact —
    /// still read, so those runs are not stranded under the same schema.
    #[test]
    fn v0_2_checkpoints_still_read() {
        use dvandva_v4::model::{
            checkpoint_manifest_digest, Checkpoint, CheckpointDeliverable, ExternalRef,
        };
        for (kind, identity, artifact_kind, artifact_value) in [
            ("git", "HEAD", "commit", "HEAD"),
            ("git", "abc123", "branch", "main"),
            ("analysis", "old-analysis-v1", "document", "review.md"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let channel = created_run(dir.path(), "claude", "codex");
            let run_dir = dir.path().join("run-a");
            let mut checkpoint = Checkpoint {
                kind: kind.into(),
                identity: identity.into(),
                deliverables: vec![CheckpointDeliverable {
                    id: "kernel".into(),
                    artifacts: vec![ExternalRef {
                        kind: artifact_kind.into(),
                        value: artifact_value.into(),
                    }],
                }],
                verification: vec!["tests passed".into()],
                scope_revision: 0,
                manifest_digest: String::new(),
            };
            checkpoint.manifest_digest = checkpoint_manifest_digest(&checkpoint);
            let binding = checkpoint.binding();
            let stored = serde_json::to_value(&checkpoint).unwrap();
            let stored_binding = serde_json::to_value(&binding).unwrap();
            rewrite_run(&run_dir, |baton| {
                baton["status"] = serde_json::json!("reviewing");
                baton["assignee"] = serde_json::json!("reviewer");
                baton["checkpoint"] = stored.clone();
                baton["checkpoint_history"] = serde_json::json!([stored_binding.clone()]);
            });
            let loaded = channel
                .read()
                .unwrap_or_else(|error| panic!("{kind}/{identity} became unreadable: {error}"));
            assert_eq!(loaded.checkpoint.as_ref().unwrap().identity, identity);
        }
    }

    /// [P1 receipt ordering] An omitted sequence is only the first receipt; an
    /// exact replay is a no-op whatever it was prepared against, even after
    /// later receipts; a stale write is refused, not applied.
    #[test]
    fn omitted_sequence_is_first_receipt_only_and_exact_replay_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let channel = created_run(dir.path(), "codex", "claude");
        let worker = claim::claim(&channel, Role::Worker, "codex-session", 1800, 0).unwrap();
        let reviewer = claim::claim(&channel, Role::Reviewer, "claude-session", 1800, 1).unwrap();
        let obligation = channel
            .read()
            .unwrap()
            .publication_binding
            .unwrap()
            .obligation;
        let stage = |label: &str, after_seq| {
            let source = dir.path().join(format!("{label}.html"));
            std::fs::write(&source, format!("<h1>{label}</h1>")).unwrap();
            Action::StageExplainer {
                obligation: obligation.clone(),
                after_seq,
                source_path: source,
            }
        };
        let apply_worker = |revision, action| {
            transition::apply(
                &channel,
                Role::Worker,
                "codex-session",
                &worker.token,
                revision,
                action,
            )
        };

        // First receipt: an omitted sequence is honoured, as the released API did.
        let first = apply_worker(2, stage("first", None)).unwrap();
        let first_digest = first
            .publication_binding
            .as_ref()
            .unwrap()
            .artifact
            .as_ref()
            .unwrap()
            .source_digest
            .clone();

        // Once any receipt exists, an omitted sequence can no longer bypass
        // ordering: a delayed "first" payload must not regress the newer bytes.
        let newer = apply_worker(3, stage("second", Some(1))).unwrap();
        assert!(matches!(
            apply_worker(4, stage("delayed", None)),
            Err(transition::TransitionError::StaleReceiptSequence)
        ));

        // A downstream receipt lands, then the *exact* documented stage of the
        // current bytes is retried with its original sequence: a no-op, not
        // stale, not invalid history.
        transition::apply(
            &channel,
            Role::Reviewer,
            "claude-session",
            &reviewer.token,
            4,
            Action::RecordExplainerReview {
                obligation: obligation.clone(),
                after_seq: Some(2),
                source_digest: newer
                    .publication_binding
                    .as_ref()
                    .unwrap()
                    .artifact
                    .as_ref()
                    .unwrap()
                    .source_digest
                    .clone(),
                verdict: ReviewVerdict::ChangesRequested,
                findings: vec!["rework".into()],
            },
        )
        .unwrap();
        let before = channel.read().unwrap();
        let replayed = apply_worker(before.revision, stage("second", Some(1)))
            .expect("an exact replay must be a no-op, not stale");
        assert_eq!(replayed.revision, before.revision);
        assert_eq!(channel.read().unwrap(), before);

        // The replayed first stage — older bytes — is still refused.
        assert!(matches!(
            apply_worker(before.revision, stage("first", Some(0))),
            Err(transition::TransitionError::StaleReceiptSequence)
        ));
        assert_ne!(
            first_digest,
            before
                .publication_binding
                .unwrap()
                .artifact
                .unwrap()
                .source_digest
        );
    }

    /// [P1 confinement] A symlinked `analysis/` directory is an escape as much as
    /// a symlinked file: the facade read and the approval gate both refuse it.
    #[test]
    fn a_symlinked_analysis_directory_is_refused_everywhere() {
        let dir = tempfile::tempdir().unwrap();
        let channel = created_run(dir.path(), "claude", "codex");
        let run_dir = dir.path().join("run-a");
        let credentials = dir.path().join("credentials");
        role_session::claim(
            &run_dir,
            &credentials,
            Role::Worker,
            "claude-session",
            1800,
            0,
        )
        .unwrap();
        let source = dir.path().join("review.md");
        std::fs::write(&source, b"# review\n").unwrap();
        let worker_credential =
            dvandva_v4::credential::load(&credentials, "claude-session", "run-a", Role::Worker)
                .unwrap();
        let staged = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &worker_credential.token,
            1,
            Action::StageAnalysis {
                source_path: source,
            },
        )
        .unwrap();
        let digest = staged.staged_analysis[0].clone();

        // Replace the analysis directory with a symlink to an outside directory
        // holding a same-named, world-readable file with the same bytes.
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join(&digest), b"# review\n").unwrap();
        std::fs::set_permissions(
            outside.join(&digest),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        std::fs::remove_dir_all(run_dir.join("analysis")).unwrap();
        std::os::unix::fs::symlink(&outside, run_dir.join("analysis")).unwrap();

        assert!(
            role_session::read_analysis(
                &run_dir,
                &credentials,
                Role::Worker,
                "claude-session",
                &digest
            )
            .is_err(),
            "the facade must not read through a symlinked run-state directory"
        );
        assert!(
            store::read_private_file_beneath(&run_dir, &format!("analysis/{digest}")).is_err(),
            "beneath reads must refuse a symlinked intermediate directory"
        );
    }
}

/// Round-four findings.
mod round_four {
    use super::*;
    use dvandva_v4::{
        action::{Action, HumanDecisionRequest},
        model::{HumanDecisionKind, WorkspaceIdentity},
        role_session, store,
        store::{RunChannel, StoreError},
        transition,
    };

    fn created_run(dir: &std::path::Path) -> (RunChannel, std::path::PathBuf) {
        let run_dir = dir.join("run-a");
        let mut created = RunBaton::new(
            "run-a",
            "Objective",
            "claude",
            "codex",
            vec![DeliverableRequirement {
                id: "kernel".into(),
                description: "Fix the kernel".into(),
            }],
        )
        .unwrap();
        created.workspace = Some(WorkspaceIdentity {
            repository_id: "github.com/axatbhardwaj/dvandva".into(),
            origin: None,
            worktree: None,
        });
        let channel = RunChannel::open(&run_dir);
        channel.create(&created).unwrap();
        (channel, run_dir)
    }

    fn worker(dir: &std::path::Path, run_dir: &std::path::Path) -> String {
        let credentials = dir.join("credentials");
        role_session::claim(
            run_dir,
            &credentials,
            Role::Worker,
            "claude-session",
            1800,
            0,
        )
        .unwrap();
        dvandva_v4::credential::load(&credentials, "claude-session", "run-a", Role::Worker)
            .unwrap()
            .token
    }

    /// [P1 autonomy] Disguised approval prose cannot be *resolved*. The kernel
    /// cannot read prose, so the invariant it enforces is structural: a decision
    /// is answered only by choosing one of its options, a scope decision only
    /// through a scope amendment, and an intent or authority answer is recorded
    /// on the objective — a pause that changes nothing cannot be resumed.
    #[test]
    fn a_pause_that_would_change_nothing_cannot_be_resolved() {
        for kind in [
            HumanDecisionKind::Scope,
            HumanDecisionKind::Intent,
            HumanDecisionKind::Authority,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (channel, run_dir) = created_run(dir.path());
            let token = worker(dir.path(), &run_dir);
            let parked = transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                1,
                Action::RequestHumanDecision(HumanDecisionRequest {
                    kind,
                    question: "Please approve the protocol workaround?".into(),
                    evidence: ["a workaround exists".into()].into(),
                    options: ["approve".into(), "reject".into()].into(),
                    proposals: Vec::new(),
                }),
            )
            .unwrap();
            assert_eq!(parked.status, Status::HumanDecision);

            // Free-form assent is not a choice.
            let prose = transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                2,
                Action::ResumeHumanDecision {
                    answer: "yes, go ahead".into(),
                    scope_amendment: None,
                },
            );
            assert!(
                matches!(prose, Err(transition::TransitionError::AnswerNotAnOption)),
                "{kind:?}: free-form assent must be refused"
            );

            let chosen = transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                2,
                Action::ResumeHumanDecision {
                    answer: "approve".into(),
                    scope_amendment: None,
                },
            );
            match kind {
                HumanDecisionKind::Scope => assert!(
                    matches!(
                        chosen,
                        Err(transition::TransitionError::DecisionWithoutChange)
                    ),
                    "a scope decision must not resume without a scope amendment"
                ),
                _ => {
                    let resumed = chosen.expect("an intent/authority choice resumes");
                    assert_eq!(resumed.status, Status::Working);
                    let recorded = resumed.objective.refs.last().unwrap();
                    assert_eq!(recorded.kind, kind.reference_kind());
                    assert_eq!(recorded.value, "approve");
                    // The same resolution cannot be replayed through history.
                    let mut forged = resumed.clone();
                    forged.revision += 1;
                    forged.objective.refs.pop();
                    assert!(channel.compare_and_swap(resumed.revision, &forged).is_err());
                }
            }
        }
    }

    /// [P1 provenance] Repair answers only a decision the kernel itself parked
    /// for this reason. A genuine scope question stays parked.
    #[test]
    fn repair_leaves_an_unrelated_decision_parked() {
        let park = |released_format: bool| {
            let dir = tempfile::tempdir().unwrap();
            let (channel, run_dir) = created_run(dir.path());
            let mut head = serde_json::to_value(channel.read().unwrap()).unwrap();
            head["publication_policy"]["channel"] = serde_json::json!("codex_sites");
            head["publication_policy"]["access"] = serde_json::json!("owner_only");
            // The policy is unreadable from the root, as a 0.2 run would be.
            for path in [
                run_dir.join("baton.json"),
                run_dir.join("history/00000000000000000000.json"),
            ] {
                std::fs::write(&path, serde_json::to_vec_pretty(&head).unwrap()).unwrap();
            }
            let mut parked = head.clone();
            parked["revision"] = serde_json::json!(1);
            parked["status"] = serde_json::json!("human_decision");
            parked["assignee"] = serde_json::json!("human");
            let mut decision = serde_json::json!({
                "kind": "scope",
                "question": "Should the migration guide be in scope?",
                "requested_by": "worker",
                "evidence": ["the report names it"],
                "options": ["include it", "leave it out"],
                "contact_role": "worker",
                "resume_status": "working",
                "resume_assignee": "worker",
                "answer": null
            });
            if released_format {
                // Stored as an older kernel wrote it: unversioned.
                parked["human_decision"] = decision;
                let mut bytes = serde_json::to_vec_pretty(&parked).unwrap();
                bytes.push(b'\n');
                std::fs::write(run_dir.join("history/00000000000000000001.json"), &bytes).unwrap();
                std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
            } else {
                decision["version"] = serde_json::json!(2);
                parked["human_decision"] = decision;
                channel
                    .compare_and_swap(0, &serde_json::from_value(parked).unwrap())
                    .unwrap();
            }
            let repaired = role_session::repair_publication_policy(
                &run_dir,
                &dir.path().join("credentials"),
                Role::Worker,
                "claude-session",
                "claude",
                "codex",
                1,
            )
            .unwrap();
            (dir, repaired)
        };

        let (_keep, unrelated) = park(false);
        assert_eq!(
            unrelated.status,
            Status::HumanDecision,
            "a role's own scope question stays parked"
        );
        assert!(unrelated.human_decision.unwrap().answer.is_none());
        assert_eq!(
            unrelated.publication_policy.unwrap(),
            PublicationPolicy::for_participants("Claude", "Codex")
        );

        let (_keep, released) = park(true);
        assert_eq!(
            released.status,
            Status::HumanDecision,
            "a released-format decision is a human-owned gate too: repair fixes the policy only"
        );
        assert!(released.human_decision.unwrap().answer.is_none());
        assert_eq!(
            released.publication_policy.unwrap(),
            PublicationPolicy::for_participants("Claude", "Codex")
        );
    }

    /// [P1 ordering] A live append must advance the receipt sequence exactly
    /// once. Stored history written before sequencing stays readable.
    #[test]
    fn a_live_receipt_must_advance_the_sequence_while_stored_history_stays_readable() {
        let dir = tempfile::tempdir().unwrap();
        let (channel, run_dir) = created_run(dir.path());
        let head = channel.read().unwrap();
        let obligation = head
            .publication_binding
            .as_ref()
            .unwrap()
            .obligation
            .clone();
        let artifact = dvandva_v4::model::ExplainerArtifact {
            obligation: obligation.clone(),
            source_digest: "a".repeat(64),
            path: dvandva_v4::model::explainer_artifact_path(&"a".repeat(64)),
            media_type: "text/html".into(),
            byte_length: 1,
            channel: "run_artifact".into(),
            access: "run_private".into(),
            publisher_harness: "Codex".into(),
        };
        // A fresh receipt at 0 -> 0 through the raw store is refused live.
        let mut forged = head.clone();
        forged.revision += 1;
        forged.publication_binding.as_mut().unwrap().artifact = Some(artifact.clone());
        assert!(
            matches!(
                channel.compare_and_swap(head.revision, &forged),
                Err(StoreError::InvalidHistory)
            ),
            "a live receipt that does not advance the sequence must be refused"
        );
        // Advancing exactly once is accepted; twice is not.
        forged.publication_binding.as_mut().unwrap().receipt_seq = 2;
        assert!(channel.compare_and_swap(head.revision, &forged).is_err());
        forged.publication_binding.as_mut().unwrap().receipt_seq = 1;
        channel.compare_and_swap(head.revision, &forged).unwrap();

        // Stored history from before sequencing: 0 -> 0 with an artifact is
        // readable when walked, so an existing run still validates.
        let stored_dir = tempfile::tempdir().unwrap();
        let (stored_channel, stored_run) = created_run(stored_dir.path());
        let root = stored_channel.read().unwrap();
        let mut legacy = root.clone();
        legacy.revision = 1;
        legacy.publication_binding.as_mut().unwrap().artifact = Some(artifact);
        let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        std::fs::write(stored_run.join("history/00000000000000000001.json"), &bytes).unwrap();
        std::fs::write(stored_run.join("baton.json"), &bytes).unwrap();
        let recovered = stored_channel
            .recover(1)
            .expect("stored pre-sequencing history must still walk");
        assert_eq!(recovered.revision, 2);
        let _ = run_dir;
    }

    /// [P1 GC] The entry moved is the pinned root's own child, and it must be the
    /// very directory that was locked and revalidated.
    #[test]
    fn archiving_refuses_a_same_basename_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let runs = dir.path().join("runs");
        std::fs::create_dir_all(&runs).unwrap();
        // The live run lives under the runs root as `run-a`.
        let (_live, _) = {
            let run_dir = runs.join("run-a");
            let mut created = RunBaton::new(
                "run-a",
                "Objective",
                "claude",
                "codex",
                vec![DeliverableRequirement {
                    id: "kernel".into(),
                    description: "Fix the kernel".into(),
                }],
            )
            .unwrap();
            created.workspace = Some(WorkspaceIdentity {
                repository_id: "github.com/axatbhardwaj/dvandva".into(),
                origin: None,
                worktree: None,
            });
            let channel = RunChannel::open(&run_dir);
            channel.create(&created).unwrap();
            (channel, run_dir)
        };
        // A stale, different directory elsewhere with the same basename.
        let elsewhere = dir.path().join("elsewhere");
        let (_stale, stale_dir) = created_run(&elsewhere);
        let stale_dir = {
            let renamed = elsewhere.join("run-a");
            std::fs::rename(&stale_dir, &renamed).unwrap();
            renamed
        };
        let far_past = std::time::SystemTime::now() - std::time::Duration::from_secs(60 * 86_400);
        let head = std::fs::File::options()
            .write(true)
            .open(stale_dir.join("baton.json"))
            .unwrap();
        head.set_modified(far_past).unwrap();

        // Archiving the stale path must not move the live `runs/run-a`.
        let moved = dvandva_v4::discovery::archive_stale_run(&runs, &stale_dir, 14).unwrap();
        assert!(
            moved.is_none(),
            "a same-basename path must not authorize moving the live entry"
        );
        assert!(runs.join("run-a").join("baton.json").is_file());
        let _ = store::open_dir_nofollow(&runs).unwrap();
    }
}

/// Round-five findings.
mod round_five {
    use super::*;
    use dvandva_v4::{
        action::{Action, HumanDecisionRequest},
        model::{HumanDecisionKind, InteractionMode, ScopeProposal, WorkspaceIdentity},
        role_session,
        store::RunChannel,
        transition,
    };

    fn created_run(
        dir: &std::path::Path,
        mode: InteractionMode,
    ) -> (RunChannel, std::path::PathBuf) {
        let run_dir = dir.join("run-a");
        let mut created = RunBaton::new(
            "run-a",
            "Objective",
            "claude",
            "codex",
            vec![DeliverableRequirement {
                id: "kernel".into(),
                description: "Fix the kernel".into(),
            }],
        )
        .unwrap();
        created.workspace = Some(WorkspaceIdentity {
            repository_id: "github.com/axatbhardwaj/dvandva".into(),
            origin: None,
            worktree: None,
        });
        created.interaction = mode;
        let channel = RunChannel::open(&run_dir);
        channel.create(&created).unwrap();
        (channel, run_dir)
    }

    fn worker_token(dir: &std::path::Path, run_dir: &std::path::Path) -> String {
        let credentials = dir.join("credentials");
        role_session::claim(
            run_dir,
            &credentials,
            Role::Worker,
            "claude-session",
            1800,
            0,
        )
        .unwrap();
        dvandva_v4::credential::load(&credentials, "claude-session", "run-a", Role::Worker)
            .unwrap()
            .token
    }

    fn proposal(objective: &str, deliverable: &str) -> ScopeProposal {
        ScopeProposal {
            objective: objective.into(),
            objective_refs: Vec::new(),
            task_reference: None,
            scope_deliverables: vec![DeliverableRequirement {
                id: deliverable.into(),
                description: format!("Deliver {deliverable}"),
            }],
        }
    }

    /// Autonomous scope choices need concrete proposals, choosing one applies
    /// it, and the same question cannot be asked twice.
    #[test]
    fn an_autonomous_scope_decision_requires_concrete_proposals() {
        let dir = tempfile::tempdir().unwrap();
        let (channel, run_dir) = created_run(dir.path(), InteractionMode::Autonomous);
        let token = worker_token(dir.path(), &run_dir);
        let ask = |kind, proposals: Vec<ScopeProposal>| {
            Action::RequestHumanDecision(HumanDecisionRequest {
                kind,
                question: "Please approve the protocol workaround?".into(),
                evidence: ["a workaround exists".into()].into(),
                options: ["approve".into(), "reject".into()].into(),
                proposals,
            })
        };
        assert!(matches!(
            transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                1,
                ask(HumanDecisionKind::Scope, Vec::new())
            ),
            Err(transition::TransitionError::NotAnAutonomousDecision)
        ));
        // Two identical proposals are not a choice.
        assert!(matches!(
            transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                1,
                ask(
                    HumanDecisionKind::Scope,
                    vec![proposal("Same", "kernel"), proposal("Same", "kernel")]
                )
            ),
            Err(transition::TransitionError::InvalidHumanDecision)
        ));
        assert_eq!(
            channel.read().unwrap().status,
            Status::Working,
            "nothing above may park the run"
        );

        // A real choice of scope is admitted, and choosing applies it.
        let parked = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            1,
            ask(
                HumanDecisionKind::Scope,
                vec![
                    proposal("Kernel only", "kernel"),
                    proposal("Kernel and guide", "guide"),
                ],
            ),
        )
        .unwrap();
        assert_eq!(parked.status, Status::HumanDecision);
        let resumed = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            parked.revision,
            Action::ResumeHumanDecision {
                answer: "reject".into(),
                scope_amendment: None,
            },
        )
        .unwrap();
        assert_eq!(resumed.status, Status::Revising);
        assert_eq!(resumed.objective.summary, "Kernel and guide");
        assert_eq!(resumed.scope_deliverables[0].id, "guide");
        assert_eq!(resumed.scope_revision, 1);

        // The decision just answered cannot be asked again.
        assert!(matches!(
            transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                resumed.revision,
                ask(
                    HumanDecisionKind::Scope,
                    vec![
                        proposal("Kernel only", "kernel"),
                        proposal("Kernel and guide", "guide")
                    ]
                ),
            ),
            Err(transition::TransitionError::RepeatedDecision)
        ));
        // Nor can a live append record a decision under the released rules,
        // which is the only shape repair would ever answer on its own.
        let mut forged = channel.read().unwrap();
        let head = forged.revision;
        forged.revision += 1;
        forged.status = Status::HumanDecision;
        forged.assignee = Assignee::Human;
        let mut decision = forged.human_decision.clone().unwrap();
        decision.answer = None;
        decision.version = 1;
        decision.question = "Another question".into();
        forged.human_decision = Some(decision);
        assert!(channel.compare_and_swap(head, &forged).is_err());
    }

    #[test]
    fn autonomous_intent_and_authority_decisions_preserve_the_human_answer() {
        for kind in [HumanDecisionKind::Intent, HumanDecisionKind::Authority] {
            let dir = tempfile::tempdir().unwrap();
            let (channel, run_dir) = created_run(dir.path(), InteractionMode::Autonomous);
            let token = worker_token(dir.path(), &run_dir);
            let request = || {
                Action::RequestHumanDecision(HumanDecisionRequest {
                    kind,
                    question: "May the requested delivery include a remote push?".into(),
                    evidence: vec!["The task does not specify remote push authority".into()],
                    options: vec![
                        "Keep the delivery local".into(),
                        "Include a remote push".into(),
                    ],
                    proposals: Vec::new(),
                })
            };
            let parked = transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                1,
                request(),
            )
            .expect("autonomy must not prevent a genuine human decision");
            assert_eq!(parked.status, Status::HumanDecision);
            assert!(
                !dvandva_v4::next_action::classify(&parked, Role::Worker, "claude")
                    .advisory_actions
                    .contains(&"work")
            );
            assert!(matches!(
                transition::apply(
                    &channel,
                    Role::Worker,
                    "claude-session",
                    &token,
                    parked.revision,
                    Action::ResumeHumanDecision {
                        answer: "yes".into(),
                        scope_amendment: None
                    },
                ),
                Err(transition::TransitionError::AnswerNotAnOption)
            ));
            let resumed = transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                parked.revision,
                Action::ResumeHumanDecision {
                    answer: "Keep the delivery local".into(),
                    scope_amendment: None,
                },
            )
            .unwrap();
            assert_eq!(resumed.scope_revision, parked.scope_revision);
            let reference = resumed.objective.refs.last().unwrap();
            assert_eq!(reference.kind, kind.reference_kind());
            assert_eq!(reference.value, "Keep the delivery local");
            assert!(matches!(
                transition::apply(
                    &channel,
                    Role::Worker,
                    "claude-session",
                    &token,
                    resumed.revision,
                    request(),
                ),
                Err(transition::TransitionError::RepeatedDecision)
            ));
            let recovered = channel.recover(resumed.revision).unwrap();
            assert_eq!(recovered.objective.refs, resumed.objective.refs);
        }
    }

    /// [P1 history integrity] A resolved intent decision must validate when the
    /// whole history is walked, not only when it was appended.
    #[test]
    fn a_resolved_intent_decision_survives_a_full_history_walk() {
        let dir = tempfile::tempdir().unwrap();
        let (channel, run_dir) = created_run(dir.path(), InteractionMode::Attended);
        let token = worker_token(dir.path(), &run_dir);
        let parked = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            1,
            Action::RequestHumanDecision(HumanDecisionRequest {
                kind: HumanDecisionKind::Intent,
                question: "Which reading?".into(),
                evidence: ["two readings".into()].into(),
                options: ["strict".into(), "lenient".into()].into(),
                proposals: Vec::new(),
            }),
        )
        .unwrap();
        let resumed = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            parked.revision,
            Action::ResumeHumanDecision {
                answer: "strict".into(),
                scope_amendment: None,
            },
        )
        .unwrap();
        assert_eq!(resumed.objective.refs.last().unwrap().value, "strict");
        let recovered = channel
            .recover(resumed.revision)
            .expect("the walked history must validate");
        assert_eq!(recovered.objective.refs.last().unwrap().value, "strict");
    }

    /// [P1 API-2 compatibility] A decision recorded by a released client — no
    /// version, no kind, padded options — still resolves under its own rules.
    #[test]
    fn a_released_format_pending_decision_still_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let (channel, run_dir) = created_run(dir.path(), InteractionMode::Attended);
        let token = worker_token(dir.path(), &run_dir);
        let head = channel.read().unwrap();
        let mut parked = serde_json::to_value(&head).unwrap();
        parked["revision"] = serde_json::json!(head.revision + 1);
        parked["status"] = serde_json::json!("human_decision");
        parked["assignee"] = serde_json::json!("human");
        parked["human_decision"] = serde_json::json!({
            "question": "Keep going?",
            "requested_by": "worker",
            "evidence": ["released client"],
            "options": [" yes ", " no "],
            "contact_role": "worker",
            "resume_status": "working",
            "resume_assignee": "worker",
            "answer": null
        });
        // Written as stored history, the way that client left it.
        let mut bytes = serde_json::to_vec_pretty(&parked).unwrap();
        bytes.push(b'\n');
        std::fs::write(
            run_dir.join(format!("history/{:020}.json", head.revision + 1)),
            &bytes,
        )
        .unwrap();
        std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
        let resumed = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            head.revision + 1,
            Action::ResumeHumanDecision {
                answer: "yes".into(),
                scope_amendment: None,
            },
        )
        .expect("a released-format decision resolves under its own rules");
        assert_eq!(resumed.status, Status::Working);
        assert_eq!(resumed.human_decision.unwrap().version, 1);
        channel
            .recover(resumed.revision)
            .expect("and its history walks");
    }

    /// [P1 claim authority] Recovering an orphaned claim requires the nonce
    /// held in the claimant's private credentials root; a public session id
    /// from another root proves nothing.
    #[test]
    fn an_orphaned_claim_is_recoverable_only_from_the_root_that_made_it() {
        let dir = tempfile::tempdir().unwrap();
        let (_channel, run_dir) = created_run(dir.path(), InteractionMode::Attended);
        let mine = dir.path().join("mine");
        let theirs = dir.path().join("theirs");
        let granted =
            role_session::claim(&run_dir, &mine, Role::Worker, "claude-session", 1800, 0).unwrap();
        // Lose the token, as a crash between install and store would.
        std::fs::remove_file(&granted.credential).unwrap();

        let impostor = role_session::start(role_session::RoleStartRequest {
            workspace: &dir.path().join("nowhere"),
            runs_dir: dir.path(),
            credentials_root: &theirs,
            role: Role::Worker,
            session_id: "claude-session",
            current_harness: "claude",
            peer_harness: "codex",
            objective: None,
            objective_refs: &[],
            task_reference: None,
            run_id: Some("run-a"),
            lease_seconds: 1800,
            wait: false,
            poll_interval: std::time::Duration::from_millis(10),
            timeout: std::time::Duration::from_millis(10),
            new_run: false,
            required_deliverables: &[],
            interaction: InteractionMode::Attended,
        });
        assert!(
            impostor.is_err(),
            "another credentials root must not recover the claim"
        );
    }

    /// [Round six] A proposal that could not be applied is refused at
    /// admission, so an autonomous run is never parked on an option nobody can
    /// choose; padded-but-valid input is canonicalized, and every admitted
    /// option resolves.
    #[test]
    fn an_unresolvable_proposal_is_refused_before_it_can_park_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let (channel, run_dir) = created_run(dir.path(), InteractionMode::Autonomous);
        let token = worker_token(dir.path(), &run_dir);
        let deliverable = |id: &str| DeliverableRequirement {
            id: id.into(),
            description: format!("Deliver {id}"),
        };
        let reference = |kind: &str, value: &str| dvandva_v4::model::ExternalRef {
            kind: kind.into(),
            value: value.into(),
        };
        let ask = |proposals: Vec<ScopeProposal>, revision: u64| {
            transition::apply(
                &channel,
                Role::Worker,
                "claude-session",
                &token,
                revision,
                Action::RequestHumanDecision(HumanDecisionRequest {
                    kind: HumanDecisionKind::Scope,
                    question: "Which scope?".into(),
                    evidence: ["two readings".into()].into(),
                    options: ["first".into(), "second".into()].into(),
                    proposals,
                }),
            )
        };
        let broken = [
            ScopeProposal {
                objective: "Duplicate deliverable ids".into(),
                objective_refs: Vec::new(),
                task_reference: None,
                scope_deliverables: vec![deliverable("a"), deliverable("a")],
            },
            ScopeProposal {
                objective: "Blank reference".into(),
                objective_refs: vec![reference(" ", "x")],
                task_reference: None,
                scope_deliverables: vec![deliverable("a")],
            },
            ScopeProposal {
                objective: "Blank task reference".into(),
                objective_refs: Vec::new(),
                task_reference: Some("   ".into()),
                scope_deliverables: vec![deliverable("a")],
            },
        ];
        for bad in broken {
            let refused = ask(vec![proposal("Fine", "kernel"), bad], 1);
            assert!(
                matches!(
                    refused,
                    Err(transition::TransitionError::InvalidHumanDecision)
                ),
                "an unresolvable proposal must be refused at admission"
            );
            assert_eq!(channel.read().unwrap().status, Status::Working);
        }

        let parked = ask(
            vec![
                ScopeProposal {
                    objective: "  Padded  ".into(),
                    objective_refs: vec![reference(" issue ", " DEF-9 ")],
                    task_reference: Some(" DEF-9 ".into()),
                    scope_deliverables: vec![DeliverableRequirement {
                        id: " k ".into(),
                        description: " Keep ".into(),
                    }],
                },
                proposal("Other", "guide"),
            ],
            1,
        )
        .unwrap();
        let recorded = &parked.human_decision.as_ref().unwrap().proposals[0];
        assert_eq!(recorded.objective, "Padded");
        assert_eq!(recorded.scope_deliverables[0].id, "k");
        assert_eq!(recorded.objective_refs[0].kind, "issue");
        let resumed = transition::apply(
            &channel,
            Role::Worker,
            "claude-session",
            &token,
            parked.revision,
            Action::ResumeHumanDecision {
                answer: "first".into(),
                scope_amendment: None,
            },
        )
        .expect("an admitted option always resolves");
        assert_eq!(resumed.objective.summary, "Padded");
        assert_eq!(
            resumed.task.as_ref().unwrap().reference.as_deref(),
            Some("DEF-9")
        );
    }
}

/// [Defect 1] Prativadi's approval is a handoff that transfers no new work
/// product, so it must not wipe the explainer receipts staged for the approved
/// checkpoint. The old kernel replaced the obligation at approval, forcing a
/// second stage/review round on identical content before `finalize` and
/// wedging real runs in `finalizing` once either role had left.
#[test]
fn approval_preserves_explainer_receipts_for_a_single_terminal_handshake() {
    use dvandva_v4::{
        action::{Action, ReviewVerdict},
        model::CheckpointSubmission,
        store::RunChannel,
        transition,
    };

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let created = RunBaton::new(
        "run-a",
        "Objective",
        "claude",
        "gpt",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .unwrap();
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();
    let worker =
        dvandva_v4::claim::claim(&channel, Role::Worker, "worker-session", 1800, 0).unwrap();
    let reviewer =
        dvandva_v4::claim::claim(&channel, Role::Reviewer, "reviewer-session", 1800, 1).unwrap();

    let commit = "a".repeat(40);
    let submitted = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        2,
        Action::SubmitCheckpoint {
            checkpoint: CheckpointSubmission {
                kind: "git".into(),
                identity: commit.clone(),
                deliverables: vec![dvandva_v4::model::CheckpointDeliverable {
                    id: "kernel".into(),
                    artifacts: vec![dvandva_v4::model::ExternalRef {
                        kind: "commit".into(),
                        value: commit.clone(),
                    }],
                }],
                verification: vec!["cargo test".into()],
            },
        },
    )
    .unwrap();
    assert_eq!(submitted.status, Status::Reviewing);
    let obligation = submitted
        .publication_binding
        .as_ref()
        .unwrap()
        .obligation
        .clone();
    assert_eq!(obligation.kind, HandoffKind::WorkerToReviewer);

    let source = dir.path().join("explainer.html");
    std::fs::write(&source, b"<h1>delivery</h1>").unwrap();
    let staged = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        3,
        Action::StageExplainer {
            obligation: obligation.clone(),
            after_seq: Some(0),
            source_path: source,
        },
    )
    .unwrap();
    let digest = staged
        .publication_binding
        .as_ref()
        .unwrap()
        .artifact
        .as_ref()
        .unwrap()
        .source_digest
        .clone();
    transition::apply(
        &channel,
        Role::Reviewer,
        "reviewer-session",
        &reviewer.token,
        4,
        Action::RecordExplainerReview {
            obligation: obligation.clone(),
            after_seq: Some(1),
            source_digest: digest,
            verdict: ReviewVerdict::Approved,
            findings: vec![],
        },
    )
    .unwrap();

    let checkpoint = submitted.checkpoint.as_ref().unwrap();
    let approved = transition::apply(
        &channel,
        Role::Reviewer,
        "reviewer-session",
        &reviewer.token,
        5,
        Action::RecordReview {
            verdict: ReviewVerdict::Approved,
            checkpoint_identity: checkpoint.identity.clone(),
            manifest_digest: checkpoint.manifest_digest.clone(),
            scope_revision: checkpoint.scope_revision,
            findings: vec![],
        },
    )
    .unwrap();
    assert_eq!(approved.status, Status::Finalizing);
    let binding = approved.publication_binding.as_ref().unwrap();
    assert_eq!(
        binding.obligation.kind,
        HandoffKind::WorkerToReviewer,
        "an approval transfers no new work product and must not open a fresh explainer obligation"
    );
    assert!(
        binding.artifact.is_some(),
        "approval must not wipe the staged explainer for the approved checkpoint"
    );
    assert_eq!(
        binding
            .review
            .as_ref()
            .map(|review| review.verdict.as_str()),
        Some("approved"),
        "approval must not wipe the reviewer's own explainer receipt"
    );

    let done = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        6,
        Action::Finalize,
    )
    .expect("one staged and approved explainer per delivery must be enough to finalize");
    assert_eq!(done.status, Status::Done);
}

/// [Defect 1 drain] Runs the old kernel already wedged — obligation replaced at
/// approval, receipts gone — must still finalize once the pair restages and
/// approves against that `reviewer_to_worker` obligation.
#[test]
fn legacy_wedged_finalizing_runs_drain_with_reviewer_to_worker_receipts() {
    use dvandva_v4::{
        action::{Action, ReviewVerdict},
        model::CheckpointSubmission,
        store::RunChannel,
        transition,
    };

    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    let created = RunBaton::new(
        "run-a",
        "Objective",
        "claude",
        "gpt",
        vec![DeliverableRequirement {
            id: "kernel".into(),
            description: "Fix the kernel".into(),
        }],
    )
    .unwrap();
    let channel = RunChannel::open(&run_dir);
    channel.create(&created).unwrap();
    let worker =
        dvandva_v4::claim::claim(&channel, Role::Worker, "worker-session", 1800, 0).unwrap();
    let reviewer =
        dvandva_v4::claim::claim(&channel, Role::Reviewer, "reviewer-session", 1800, 1).unwrap();
    let commit = "b".repeat(40);
    let submitted = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        2,
        Action::SubmitCheckpoint {
            checkpoint: CheckpointSubmission {
                kind: "git".into(),
                identity: commit.clone(),
                deliverables: vec![dvandva_v4::model::CheckpointDeliverable {
                    id: "kernel".into(),
                    artifacts: vec![dvandva_v4::model::ExternalRef {
                        kind: "commit".into(),
                        value: commit.clone(),
                    }],
                }],
                verification: vec!["cargo test".into()],
            },
        },
    )
    .unwrap();
    let checkpoint = submitted.checkpoint.as_ref().unwrap();

    // Hand-write the pre-0.3.5 approval edge: verdict recorded, status moved to
    // finalizing, and the obligation replaced with a receiptless
    // reviewer_to_worker binding — the exact shape of the wedged live runs.
    let head: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    let mut wedged = head.clone();
    let revision = head["revision"].as_u64().unwrap() + 1;
    wedged["revision"] = serde_json::json!(revision);
    wedged["status"] = serde_json::json!("finalizing");
    wedged["assignee"] = serde_json::json!("worker");
    wedged["review"] = serde_json::json!({
        "verdict": "approved",
        "checkpoint_identity": checkpoint.identity,
        "manifest_digest": checkpoint.manifest_digest,
        "scope_revision": checkpoint.scope_revision,
    });
    wedged["publication_binding"] = serde_json::json!({
        "obligation": {
            "handoff_revision": revision,
            "kind": "reviewer_to_worker",
            "scope_revision": 0,
            "checkpoint": {
                "checkpoint_identity": checkpoint.identity,
                "manifest_digest": checkpoint.manifest_digest,
                "scope_revision": checkpoint.scope_revision,
            },
        },
        "receipt_seq": 0,
        "artifact": null,
        "deployment": null,
        "review": null,
    });
    let bytes = serde_json::to_vec_pretty(&wedged).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
    std::fs::write(run_dir.join(format!("history/{revision:020}.json")), &bytes).unwrap();

    let obligation = serde_json::from_value::<dvandva_v4::model::HandoffObligation>(
        wedged["publication_binding"]["obligation"].clone(),
    )
    .unwrap();
    let source = dir.path().join("explainer.html");
    std::fs::write(&source, b"<h1>drained</h1>").unwrap();
    let staged = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        revision,
        Action::StageExplainer {
            obligation: obligation.clone(),
            after_seq: Some(0),
            source_path: source,
        },
    )
    .unwrap();
    let digest = staged
        .publication_binding
        .as_ref()
        .unwrap()
        .artifact
        .as_ref()
        .unwrap()
        .source_digest
        .clone();
    transition::apply(
        &channel,
        Role::Reviewer,
        "reviewer-session",
        &reviewer.token,
        revision + 1,
        Action::RecordExplainerReview {
            obligation,
            after_seq: Some(1),
            source_digest: digest,
            verdict: ReviewVerdict::Approved,
            findings: vec![],
        },
    )
    .unwrap();
    let done = transition::apply(
        &channel,
        Role::Worker,
        "worker-session",
        &worker.token,
        revision + 2,
        Action::Finalize,
    )
    .expect("a legacy reviewer_to_worker obligation with current receipts must drain");
    assert_eq!(done.status, Status::Done);
}

/// [Defect 2] Until the run_started explainer is locally approved — proof the
/// reviewer has actually joined — the worker must not be advised to work or to
/// checkpoint: its owed action is staging the explainer, and once staged it
/// rests instead of spinning. Approval opens the ordinary work loop.
#[test]
fn run_start_work_waits_for_the_explainer_approval_that_proves_the_pair_joined() {
    let mut run = baton();
    let unstaged = next_action::classify(&run, Role::Worker, "Claude");
    assert!(
        !unstaged.advisory_actions.contains(&"work"),
        "work must not be advisory before the pair has formed"
    );
    // A finished deliverable can still land (PR-914): submission stays legal.
    assert!(unstaged.legal_actions.contains(&"submit_checkpoint"));
    assert!(unstaged.legal_actions.contains(&"stage_explainer"));
    assert!(
        unstaged.actionable,
        "staging the run_started explainer is owed work"
    );

    let binding = run.publication_binding.as_mut().unwrap();
    let obligation = binding.obligation.clone();
    let digest = "a".repeat(64);
    binding.artifact = Some(ExplainerArtifact {
        obligation: obligation.clone(),
        source_digest: digest.clone(),
        path: format!("explainer/{digest}.html"),
        media_type: "text/html".into(),
        byte_length: 32,
        channel: EXPLAINER_CHANNEL.into(),
        access: EXPLAINER_ACCESS.into(),
        publisher_harness: "Claude".into(),
    });
    let staged = next_action::classify(&run, Role::Worker, "Claude");
    assert!(!staged.advisory_actions.contains(&"work"));
    assert!(staged.legal_actions.contains(&"submit_checkpoint"));
    assert!(
        !staged.actionable,
        "a worker waiting for the reviewer's first receipt must rest, not spin"
    );

    run.publication_binding.as_mut().unwrap().review = Some(PublicationReview {
        obligation,
        source_digest: digest,
        verdict: "approved".into(),
        findings: vec![],
        reviewer_harness: "Codex".into(),
    });
    let joined = next_action::classify(&run, Role::Worker, "Claude");
    assert!(joined.advisory_actions.contains(&"work"));
    assert!(joined.legal_actions.contains(&"submit_checkpoint"));
}

/// [Defect 2 boundary] The join gate binds only the run_started obligation:
/// mid-run revising after requested changes never waits on an explainer.
#[test]
fn mid_run_revising_never_waits_for_the_explainer() {
    let mut run = baton();
    run.status = Status::Revising;
    run.assignee = Assignee::Worker;
    run.publication_binding.as_mut().unwrap().obligation.kind = HandoffKind::ReviewerToWorker;
    let actions = next_action::classify(&run, Role::Worker, "Claude");
    assert!(actions.advisory_actions.contains(&"work"));
    assert!(actions.legal_actions.contains(&"submit_checkpoint"));
}
