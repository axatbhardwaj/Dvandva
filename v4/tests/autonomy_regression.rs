//! Regressions for the amended scope: a run must be able to proceed on its own
//! when the human is absent, and its routine operations must not look like
//! crashes to the surrounding harness.

use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;

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
        "claude",
        "codex",
        0,
    )
    .expect("an unreadable policy must be repairable");

    // Repair is scoped: a caller naming the wrong topology is refused.
    assert!(dvandva_v4::role_session::repair_publication_policy(
        &run_dir,
        Role::Worker,
        "claude-session",
        "codex",
        "claude",
        repaired.revision,
    )
    .is_err());
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

    let review = |verdict, findings: Vec<String>| Action::RecordExplainerReview {
        obligation: obligation.clone(),
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
        review(ReviewVerdict::ChangesRequested, vec!["rework".into()]),
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
        review(ReviewVerdict::Approved, Vec::new()),
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

    let submitted = transition::apply(
        &channel,
        Role::Worker,
        "claude-session",
        &worker.token,
        2,
        submit(&digest, &digest),
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
    let worker =
        dvandva_v4::claim::claim(&channel, Role::Worker, "claude-session", 1800, 0).unwrap();

    let escalation = |kind| {
        Action::RequestHumanDecision(HumanDecisionRequest {
            kind,
            question: "The explainer Site returns 401. Approve relaying it by hand?".into(),
            evidence: ["the reviewing harness has no session for the Site".into()].into(),
            options: ["relay the text".into(), "stop the run".into()].into(),
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
