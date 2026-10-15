//! The platform's notice kinds, and what each one is about.
//!
//! A closed list: every `oxy.*` kind is named here with its subject, whether
//! it can be muted, and which channels it asks for. An app cannot send one
//! (`input::is_reserved_kind`), so a kind outside this list on a signal is a
//! row from a newer binary, rendered as a plain "Oxygen notice" rather than
//! guessed at.

use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::server::api::notifications::input::RESERVED_KIND_PREFIX;

pub const BILLING_PAST_DUE: &str = "oxy.billing_past_due";
pub const WORKSPACE_READY: &str = "oxy.workspace_ready";
pub const RUN_FAILED: &str = "oxy.run_failed";
pub const ANOMALY: &str = "oxy.anomaly";

/// Every kind the platform sends.
pub const KINDS: [&str; 4] = [BILLING_PAST_DUE, WORKSPACE_READY, RUN_FAILED, ANOMALY];

/// Whether `kind` is one the platform sends.
pub fn is_platform_kind(kind: &str) -> bool {
    kind.starts_with(RESERVED_KIND_PREFIX) && KINDS.contains(&kind)
}

/// A critical kind ignores mutes. Only billing: an owner who muted it would
/// learn of a failed payment when the org is suspended.
pub fn is_critical(kind: &str) -> bool {
    kind == BILLING_PAST_DUE
}

/// The channels a platform kind is delivered on, so the only ones its settings
/// switches can change. Push always; never Slack (a monitor posts to Slack
/// through its own `slack_channel`).
///
/// Email only for a failed run: someone has to act, possibly away from
/// Oxygen. Billing already sends the owner its own past-due email
/// (`billing/notify.rs`), so asking for another would tell them twice. A
/// ready workspace and an anomaly are news to see in the product.
pub fn channels(kind: &str) -> &'static [&'static str] {
    if kind == RUN_FAILED {
        &["push", "email"]
    } else {
        &["push"]
    }
}

/// The record a notice is about, by kind and id — never its text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject {
    /// `billing`, `workspace`, `automation_run`, `airway_run` or `anomaly`.
    pub kind: &'static str,
    pub id: String,
}

/// A past-due episode of the signal's org, by the end of its grace period.
pub const SUBJECT_BILLING: &str = "billing";
pub const SUBJECT_WORKSPACE: &str = "workspace";
pub const SUBJECT_AUTOMATION_RUN: &str = "automation_run";
pub const SUBJECT_AIRWAY_RUN: &str = "airway_run";
pub const SUBJECT_ANOMALY: &str = "anomaly";

/// The subject kind as stored, mapped back to its `'static` name. `None` for
/// a name this binary does not know.
pub fn subject_kind(stored: &str) -> Option<&'static str> {
    [
        SUBJECT_BILLING,
        SUBJECT_WORKSPACE,
        SUBJECT_AUTOMATION_RUN,
        SUBJECT_AIRWAY_RUN,
        SUBJECT_ANOMALY,
    ]
    .into_iter()
    .find(|k| *k == stored)
}

/// One thing the platform tells people. Each variant carries the ids of the
/// Oxy records it is rendered from, and nothing a person wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformNotice {
    /// The org's subscription payment failed. The grace period's end names
    /// the episode: it is set once when the org enters past due, so a second
    /// episode months later is a new notice while a replayed webhook is not.
    BillingPastDue {
        org_id: Uuid,
        grace_ends_at: DateTime<Utc>,
    },
    /// A workspace finished setting up and can be opened.
    WorkspaceReady { org_id: Uuid, workspace_id: Uuid },
    /// An automation run failed for good: no retry is coming.
    AutomationRunFailed {
        org_id: Uuid,
        workspace_id: Uuid,
        run_id: String,
    },
    /// An Airway pipeline run failed for good.
    AirwayRunFailed {
        org_id: Uuid,
        workspace_id: Uuid,
        run_id: String,
    },
    /// A monitor found an anomaly event, for the people its
    /// `notify.people` names. The event, not a bucket: the Slack announcement
    /// and its ledger are per event too.
    Anomaly {
        org_id: Uuid,
        workspace_id: Uuid,
        event_id: Uuid,
    },
}

impl PlatformNotice {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::BillingPastDue { .. } => BILLING_PAST_DUE,
            Self::WorkspaceReady { .. } => WORKSPACE_READY,
            Self::AutomationRunFailed { .. } | Self::AirwayRunFailed { .. } => RUN_FAILED,
            Self::Anomaly { .. } => ANOMALY,
        }
    }

    pub fn org_id(&self) -> Uuid {
        match self {
            Self::BillingPastDue { org_id, .. }
            | Self::WorkspaceReady { org_id, .. }
            | Self::AutomationRunFailed { org_id, .. }
            | Self::AirwayRunFailed { org_id, .. }
            | Self::Anomaly { org_id, .. } => *org_id,
        }
    }

    /// The workspace the delivery run is recorded under. `None` for an
    /// org-level notice, which records it under the org's oldest workspace.
    pub fn workspace_id(&self) -> Option<Uuid> {
        match self {
            Self::BillingPastDue { .. } => None,
            Self::WorkspaceReady { workspace_id, .. }
            | Self::AutomationRunFailed { workspace_id, .. }
            | Self::AirwayRunFailed { workspace_id, .. }
            | Self::Anomaly { workspace_id, .. } => Some(*workspace_id),
        }
    }

    /// What it is about. With the kind, the dedupe key per recipient.
    pub fn subject(&self) -> Subject {
        match self {
            // The episode, not the org: the org has many past-due spells.
            Self::BillingPastDue { grace_ends_at, .. } => Subject {
                kind: SUBJECT_BILLING,
                id: grace_ends_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            },
            Self::WorkspaceReady { workspace_id, .. } => Subject {
                kind: SUBJECT_WORKSPACE,
                id: workspace_id.to_string(),
            },
            Self::AutomationRunFailed { run_id, .. } => Subject {
                kind: SUBJECT_AUTOMATION_RUN,
                id: run_id.clone(),
            },
            Self::AirwayRunFailed { run_id, .. } => Subject {
                kind: SUBJECT_AIRWAY_RUN,
                id: run_id.clone(),
            },
            Self::Anomaly { event_id, .. } => Subject {
                kind: SUBJECT_ANOMALY,
                id: event_id.to_string(),
            },
        }
    }

    pub fn critical(&self) -> bool {
        is_critical(self.kind())
    }

    /// Whether it is also emailed; see [`channels`].
    pub fn emailed(&self) -> bool {
        channels(self.kind()).contains(&"email")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_in_the_reserved_namespace_and_only_billing_is_critical() {
        for kind in KINDS {
            assert!(kind.starts_with(RESERVED_KIND_PREFIX), "{kind}");
            assert!(is_platform_kind(kind));
            assert_eq!(is_critical(kind), kind == BILLING_PAST_DUE, "{kind}");
        }
        assert!(!is_platform_kind("oxy.something_new"));
        assert!(!is_platform_kind("billing_past_due"));
    }

    #[test]
    fn only_a_failed_run_is_emailed_and_nothing_goes_to_slack() {
        for kind in KINDS {
            let ch = channels(kind);
            assert_eq!(ch[0], "push", "{kind}");
            assert!(!ch.contains(&"slack"), "{kind}");
            assert_eq!(ch.contains(&"email"), kind == RUN_FAILED, "{kind}");
        }
        let run = PlatformNotice::AirwayRunFailed {
            org_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            run_id: "r-1".into(),
        };
        assert!(run.emailed());
    }

    #[test]
    fn a_billing_notice_is_keyed_by_its_episode_not_its_org() {
        let org = Uuid::new_v4();
        let at = |s| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let a = PlatformNotice::BillingPastDue {
            org_id: org,
            grace_ends_at: at("2026-10-21T09:00:00Z"),
        };
        let b = PlatformNotice::BillingPastDue {
            org_id: org,
            grace_ends_at: at("2027-02-03T09:00:00Z"),
        };
        assert_ne!(a.subject(), b.subject());
        assert_eq!(a.subject().id, "2026-10-21T09:00:00Z");
        assert!(a.critical());
        assert!(
            !a.emailed(),
            "billing keeps its own email; the notice must not repeat it"
        );
        assert_eq!(a.workspace_id(), None);
    }
}
