use serde::Serialize;

use crate::{
    claim::Role,
    model::{
        Assignee, RunBaton, Status, EXPLAINER_ACCESS, EXPLAINER_CHANNEL,
        EXPLAINER_PUBLISHER_HARNESS, EXPLAINER_REVIEWER_HARNESS,
    },
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
                if publication_gate_satisfied(baton) {
                    legal.push("submit_checkpoint");
                } else {
                    blocking_reason = Some("submit_checkpoint awaits current explainer approval");
                }
            }
            (Role::Reviewer, Status::Reviewing, Assignee::Reviewer) => {
                advisory.push("review_checkpoint");
                if baton.pending_checkpoint_supersession.is_some() {
                    legal.push("accept_checkpoint_supersession");
                } else if publication_gate_satisfied(baton) {
                    legal.push("record_review");
                } else {
                    blocking_reason = Some("record_review awaits current explainer approval");
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
    if harness.eq_ignore_ascii_case(EXPLAINER_PUBLISHER_HARNESS) {
        if publication_needs_deployment(baton) {
            legal.push("publish_explainer");
        }
    } else if harness.eq_ignore_ascii_case(EXPLAINER_REVIEWER_HARNESS)
        && publication_needs_review(baton)
    {
        legal.push("review_explainer");
    }

    if advisory.is_empty() && legal.is_empty() {
        legal.push("wait");
    }
    result(role_state, wake_reason, advisory, legal, blocking_reason)
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
    let actionable = next_actions
        .iter()
        .any(|action| !matches!(*action, "wait" | "stop"));
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

fn publication_needs_deployment(baton: &RunBaton) -> bool {
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.deployment.is_none()
            || binding
                .review
                .as_ref()
                .is_some_and(|review| review.verdict == "changes_requested")
    })
}

fn publication_needs_review(baton: &RunBaton) -> bool {
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.deployment.is_some()
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

fn publication_gate_satisfied(baton: &RunBaton) -> bool {
    baton.publication_binding.as_ref().is_some_and(|binding| {
        binding.deployment.as_ref().is_some_and(|deployment| {
            binding.review.as_ref().is_some_and(|review| {
                binding.site_id.as_ref() == Some(&deployment.site_id)
                    && deployment.obligation == binding.obligation
                    && review.obligation == binding.obligation
                    && review.source_digest == deployment.source_digest
                    && review.site_id == deployment.site_id
                    && review.site_version == deployment.site_version
                    && review.url == deployment.url
                    && review.verdict == "approved"
                    && review.findings.is_empty()
                    && deployment.channel == EXPLAINER_CHANNEL
                    && deployment.access == EXPLAINER_ACCESS
                    && deployment.publisher_harness == EXPLAINER_PUBLISHER_HARNESS
                    && review.reviewer_harness == EXPLAINER_REVIEWER_HARNESS
            })
        })
    })
}
