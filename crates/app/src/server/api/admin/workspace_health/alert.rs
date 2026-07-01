use uuid::Uuid;

use super::evaluator::HealthStatus;
use super::queries::WorkspaceLabel;
use crate::integrations::slack::client::SlackClient;
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertDecision {
    /// No notification.
    Silent,
    /// Transitioned into unhealthy — push an alert.
    Alert,
    /// Returned to healthy from a worse state — push a recovery note.
    Recovery,
}

/// Alert only on transition INTO unhealthy; one recovery note on return to
/// healthy. Degraded is dashboard-only — never pages Slack.
pub fn decide_transition(prev: Option<HealthStatus>, next: HealthStatus) -> AlertDecision {
    match (prev, next) {
        (_, HealthStatus::Unhealthy) if prev != Some(HealthStatus::Unhealthy) => {
            AlertDecision::Alert
        }
        (Some(p), HealthStatus::Healthy) if p != HealthStatus::Healthy => AlertDecision::Recovery,
        _ => AlertDecision::Silent,
    }
}

/// Render a workspace reference for Slack: prefer "*name* (org)" with the UUID
/// in monospace for traceability, falling back to the bare id when no label was
/// resolved (e.g. the workspace row was deleted between sweep and alert).
fn workspace_ref(ws: Uuid, label: Option<&WorkspaceLabel>) -> String {
    match label {
        Some(WorkspaceLabel {
            name,
            org_name: Some(org),
        }) => format!("*{name}* ({org}) `{ws}`"),
        Some(WorkspaceLabel { name, .. }) => format!("*{name}* `{ws}`"),
        None => format!("`{ws}`"),
    }
}

/// Post a health alert / recovery message to the ops Slack channel.
pub async fn push_slack(
    client: &SlackClient,
    bot_token: &str,
    channel: &str,
    ws: Uuid,
    label: Option<&WorkspaceLabel>,
    status: HealthStatus,
    reasons: &[String],
    decision: AlertDecision,
) -> Result<(), OxyError> {
    let ws_ref = workspace_ref(ws, label);
    let header = match decision {
        AlertDecision::Alert => {
            format!(":rotating_light: Workspace {ws_ref} is {}", status.as_str())
        }
        AlertDecision::Recovery => {
            format!(":white_check_mark: Workspace {ws_ref} recovered (healthy)")
        }
        AlertDecision::Silent => return Ok(()),
    };
    let body = if reasons.is_empty() {
        header
    } else {
        format!("{header}\n• {}", reasons.join("\n• "))
    };
    client
        .chat_post_message(bot_token, channel, &body, None)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::evaluator::HealthStatus::*;

    #[test]
    fn first_time_unhealthy_alerts() {
        assert_eq!(decide_transition(None, Unhealthy), AlertDecision::Alert);
    }

    #[test]
    fn healthy_to_unhealthy_alerts() {
        assert_eq!(
            decide_transition(Some(Healthy), Unhealthy),
            AlertDecision::Alert
        );
    }

    #[test]
    fn degraded_to_unhealthy_alerts() {
        assert_eq!(
            decide_transition(Some(Degraded), Unhealthy),
            AlertDecision::Alert
        );
    }

    #[test]
    fn unhealthy_to_unhealthy_is_silent() {
        assert_eq!(
            decide_transition(Some(Unhealthy), Unhealthy),
            AlertDecision::Silent
        );
    }

    #[test]
    fn unhealthy_to_healthy_recovers() {
        assert_eq!(
            decide_transition(Some(Unhealthy), Healthy),
            AlertDecision::Recovery
        );
    }

    #[test]
    fn healthy_to_degraded_is_silent() {
        // Degraded is dashboard-only; we alert Slack on unhealthy transitions.
        assert_eq!(
            decide_transition(Some(Healthy), Degraded),
            AlertDecision::Silent
        );
    }

    #[test]
    fn first_time_healthy_is_silent() {
        assert_eq!(decide_transition(None, Healthy), AlertDecision::Silent);
    }

    #[test]
    fn workspace_ref_with_org() {
        let ws = Uuid::nil();
        let label = WorkspaceLabel {
            name: "Acme Analytics".into(),
            org_name: Some("Acme Corp".into()),
        };
        assert_eq!(
            workspace_ref(ws, Some(&label)),
            format!("*Acme Analytics* (Acme Corp) `{ws}`")
        );
    }

    #[test]
    fn workspace_ref_without_org() {
        let ws = Uuid::nil();
        let label = WorkspaceLabel {
            name: "Acme Analytics".into(),
            org_name: None,
        };
        assert_eq!(
            workspace_ref(ws, Some(&label)),
            format!("*Acme Analytics* `{ws}`")
        );
    }

    #[test]
    fn workspace_ref_falls_back_to_id() {
        let ws = Uuid::nil();
        assert_eq!(workspace_ref(ws, None), format!("`{ws}`"));
    }
}
