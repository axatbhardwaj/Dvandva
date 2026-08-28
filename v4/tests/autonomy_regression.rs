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
    let channel = RunChannel::open(&run_dir);
    let mut with_site = baton.clone();
    with_site["revision"] = serde_json::json!(1);
    channel
        .compare_and_swap(0, &serde_json::from_value(with_site).unwrap())
        .expect("a 0.2-shaped Site receipt edge must still validate");

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

    // And the Site alone never gates anything, however it was recorded.
    let actions = next_action::classify(&loaded, Role::Worker, "Claude");
    assert!(actions.legal_actions.contains(&"submit_checkpoint"));

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
        model::{HumanDecision, HumanDecisionKind, WorkspaceIdentity},
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
        parked["human_decision"] = serde_json::json!({
            "kind": "scope",
            "question": "The Site returns 401. Relay it by hand?",
            "requested_by": "worker",
            "evidence": ["HTTP 401 from the owner-only Site"],
            "options": ["relay", "stop"],
            "contact_role": "worker",
            "resume_status": "working",
            "resume_assignee": "worker",
            "answer": null
        });
        channel
            .compare_and_swap(0, &serde_json::from_value(parked).unwrap())
            .unwrap();
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
        assert_eq!(repaired.status, Status::Working);
        assert_eq!(repaired.assignee, Assignee::Worker);
        let decision: &HumanDecision = repaired.human_decision.as_ref().unwrap();
        assert!(decision
            .answer
            .as_deref()
            .unwrap()
            .contains("resolved autonomously"));
        assert_eq!(decision.kind, HumanDecisionKind::Scope);
        // Nothing is left for a human: the next actor is the worker.
        let actions = next_action::classify(&repaired, Role::Worker, "Claude");
        assert!(actions.next_actions.contains(&"work"));
        assert!(!actions.legal_actions.contains(&"answer_human"));
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
