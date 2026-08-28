use serde::Serialize;

use crate::{
    claim::Role,
    model::{Assignee, PublicationPolicy, RunBaton, Status},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextActions {
    pub role_state: &'static str,
    pub wake_reason: &'static str,
    pub advisory_actions: Vec<&'static str>,
    pub legal_actions: Vec<&'static str>,
    pub next_actions: Vec<&'static str>,
    pub actionable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_reason: Option<&'static str>,
}

pub fn classify(baton: &RunBaton, role: Role, participant_harness: &str) -> NextActions {
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return result("terminal", "run_terminal", Vec::new(), vec!["stop"], None);
    }

    let assigned = matches!(
        (role, &baton.assignee),
        (Role::Worker, Assignee::Worker) | (Role::Reviewer, Assignee::Reviewer)
    );
    let human_contact = baton.status == Status::HumanDecision
        && baton.human_decision.as_ref().is_some_and(|decision| {
            decision.contact_role
                == match role {
                    Role::Worker => "worker",
                    Role::Reviewer => "reviewer",
                }
        });
    let (role_state, wake_reason) = if human_contact {
        ("human_contact", "human_decision_requires_answer")
    } else if assigned {
        ("assigned", "role_assigned")
    } else {
        ("waiting", "assigned_to_peer")
    };

    let mut advisory = Vec::new();
    let mut legal = Vec::new();
    let mut blocking_reason = None;

    if human_contact {
        legal.push("answer_human");
    } else {
        match (role, &baton.status, &baton.assignee) {
            (Role::Worker, Status::Working | Status::Revising, Assignee::Worker) => {
                advisory.push("work");
                legal.push("submit_checkpoint");
            }
            (Role::Reviewer, Status::Reviewing, Assignee::Reviewer) => {
                advisory.push("review_checkpoint");
                if baton.pending_checkpoint_supersession.is_some() {
                    legal.push("accept_checkpoint_supersession");
                } else {
                    legal.push("record_review");
                }
            }
            (Role::Worker, Status::Reviewing, Assignee::Reviewer)
                if baton.pending_checkpoint_supersession.is_none() =>
            {
                legal.push("request_checkpoint_supersession");
            }
            (Role::Worker, Status::Finalizing, Assignee::Worker) => {
                legal.push("withdraw_approval");
                if publication_gate_satisfied(baton) {
                    legal.push("finalize");
                } else {
                    blocking_reason = Some("finalize awaits current explainer approval");
                }
            }
            _ => {}
        }
    }

    let harness = participant_harness.trim();
    let policy = effective_policy(baton);
    if harness.eq_ignore_ascii_case(policy.publisher_harness.trim()) {
        if publication_needs_artifact(baton) {
            legal.push("stage_explainer");
        }
        if publication_can_publish_site(baton) {
            legal.push("publish_explainer");
        }
    } else if harness.eq_ignore_ascii_case(policy.reviewer_harness.trim())
        && publication_needs_review(baton)
    {
        legal.push("review_explainer");
    }

    if advisory.is_empty() && legal.is_empty() {
        legal.push("wait");
    }
    // Always available, never a reason to wake: reporting liveness is something
    // a role may do, not work the protocol is waiting on.
    legal.push("report_progress");
    let mut actions = result(role_state, wake_reason, advisory, legal, blocking_reason);
    if baton.status != Status::HumanDecision {
        actions.legal_actions.push("request_human_decision");
    }
    actions
}

fn result(
    role_state: &'static str,
    wake_reason: &'static str,
    advisory_actions: Vec<&'static str>,
    legal_actions: Vec<&'static str>,
    blocking_reason: Option<&'static str>,
) -> NextActions {
    let next_actions = advisory_actions
        .iter()
        .chain(&legal_actions)
        .copied()
        .collect::<Vec<_>>();
    // A wake reason is an action that advances the run on the current owner's
    // behalf. Escape hatches and liveness are always available and are never
    // reasons to wake: counting them makes a foreground wait return instantly
    // and spin instead of resting.
    let actionable = next_actions.iter().any(|action| {
        !matches!(
            *action,
            "wait"
                | "stop"
                | "report_progress"
                | "request_checkpoint_supersession"
                | "withdraw_approval"
        )
    });
    NextActions {
        role_state,
        wake_reason,
        advisory_actions,
        legal_actions,
        next_actions,
        actionable,
        blocking_reason,
    }
}

fn effective_policy(baton: &RunBaton) -> PublicationPolicy {
    baton
        .publication_policy
        .clone()
        .unwrap_or_else(PublicationPolicy::fixed)
}

/// The publisher owes fresh explainer bytes whenever none are staged against the
/// current obligation, or the reviewer asked for changes.
fn publication_needs_artifact(baton: &RunBaton) -> bool {
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.artifact.is_none()
            || binding
                .review
                .as_ref()
                .is_some_and(|review| review.verdict == "changes_requested")
    })
}

/// Recording the optional human-facing Site is possible only once the bytes it
/// renders are staged and digest-bound.
fn publication_can_publish_site(baton: &RunBaton) -> bool {
    baton
        .publication_binding
        .as_ref()
        .is_some_and(|binding| binding.artifact.is_some())
}

fn publication_needs_review(baton: &RunBaton) -> bool {
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.artifact.is_some()
            && binding
                .review
                .as_ref()
                .is_none_or(|review| review.verdict != "approved")
            && !binding
                .review
                .as_ref()
                .is_some_and(|review| review.verdict == "changes_requested")
    })
}

/// The finalization gate: the reviewing harness has approved the exact staged
/// bytes bound to the current obligation.
fn publication_gate_satisfied(baton: &RunBaton) -> bool {
    let policy = effective_policy(baton);
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.artifact.as_ref().is_some_and(|artifact| {
            binding.review.as_ref().is_some_and(|review| {
                artifact.obligation == binding.obligation
                    && review.obligation == binding.obligation
                    && review.source_digest == artifact.source_digest
                    && review.verdict == "approved"
                    && review.findings.is_empty()
                    && artifact.channel == policy.channel
                    && artifact.access == policy.access
                    && artifact.publisher_harness == policy.publisher_harness
                    && review.reviewer_harness == policy.reviewer_harness
            })
        })
    })
}
