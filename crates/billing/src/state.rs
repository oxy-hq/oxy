//! Pure subscription-state transition logic. No DB, no Stripe — only in/out.

use chrono::{DateTime, Duration, Utc};
use entity::org_billing::BillingStatus;

pub const GRACE_PERIOD_DAYS: i64 = 7;

pub struct StateInput {
    pub current_status: BillingStatus,
    pub current_grace_ends_at: Option<DateTime<Utc>>,
    pub new_status: BillingStatus,
    pub event_time: DateTime<Utc>,
}

pub struct StateOutput {
    pub status: BillingStatus,
    pub grace_ends_at: Option<DateTime<Utc>>,
    pub send_admin_email: bool,
}

/// Apply a Stripe-driven status transition. Implements the table from the
/// design doc — set grace once on `active → past_due`, preserve it on
/// `past_due → past_due`, clear it on `past_due → active`.
pub fn apply_status_transition(i: StateInput) -> StateOutput {
    use BillingStatus::*;
    match (i.current_status, i.new_status) {
        (Active, PastDue) => StateOutput {
            status: PastDue,
            grace_ends_at: Some(i.event_time + Duration::days(GRACE_PERIOD_DAYS)),
            send_admin_email: true,
        },
        (PastDue, PastDue) => StateOutput {
            status: PastDue,
            grace_ends_at: i.current_grace_ends_at,
            send_admin_email: false,
        },
        (PastDue, Active) => StateOutput {
            status: Active,
            grace_ends_at: None,
            send_admin_email: false,
        },
        (_, new) => StateOutput {
            status: new,
            grace_ends_at: None,
            send_admin_email: false,
        },
    }
}

/// Whether the org can use the app right now. Folds `past_due` past its
/// `grace_period_ends_at` deadline into a "no" without needing a webhook,
/// matching the `effective_status()` semantics in the design doc.
pub fn grants_access(
    status: BillingStatus,
    grace_ends_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    use BillingStatus::*;
    match status {
        Active => true,
        PastDue => grace_ends_at.is_none_or(|e| now < e),
        Incomplete | Unpaid | Canceled => false,
    }
}

/// Status as the rest of the app should observe it — same as the DB row,
/// except `past_due` past its grace deadline collapses to `unpaid`.
pub fn effective_status(
    status: BillingStatus,
    grace_ends_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> BillingStatus {
    if status == BillingStatus::PastDue && grace_ends_at.is_some_and(|e| now >= e) {
        BillingStatus::Unpaid
    } else {
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BillingStatus::*;

    fn t() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn active_to_past_due_sets_grace_and_emails() {
        let event = t();
        let out = apply_status_transition(StateInput {
            current_status: Active,
            current_grace_ends_at: None,
            new_status: PastDue,
            event_time: event,
        });
        assert_eq!(out.status, PastDue);
        assert!(out.grace_ends_at.is_some());
        assert_eq!(
            out.grace_ends_at.unwrap() - event,
            Duration::days(GRACE_PERIOD_DAYS)
        );
        assert!(out.send_admin_email);
    }

    #[test]
    fn past_due_to_past_due_preserves_grace_window() {
        let existing = t() - Duration::days(3);
        let out = apply_status_transition(StateInput {
            current_status: PastDue,
            current_grace_ends_at: Some(existing),
            new_status: PastDue,
            event_time: t(),
        });
        assert_eq!(out.grace_ends_at, Some(existing));
        assert!(!out.send_admin_email);
    }

    #[test]
    fn past_due_to_active_clears_grace() {
        let out = apply_status_transition(StateInput {
            current_status: PastDue,
            current_grace_ends_at: Some(t()),
            new_status: Active,
            event_time: t(),
        });
        assert_eq!(out.status, Active);
        assert!(out.grace_ends_at.is_none());
    }

    #[test]
    fn canceled_is_terminal() {
        let out = apply_status_transition(StateInput {
            current_status: Active,
            current_grace_ends_at: None,
            new_status: Canceled,
            event_time: t(),
        });
        assert_eq!(out.status, Canceled);
        assert!(out.grace_ends_at.is_none());
    }

    #[test]
    fn grants_access_truth_table() {
        let now = t();
        assert!(grants_access(Active, None, now));
        assert!(grants_access(PastDue, Some(now + Duration::days(3)), now));
        assert!(!grants_access(PastDue, Some(now - Duration::days(1)), now));
        assert!(!grants_access(Incomplete, None, now));
        assert!(!grants_access(Unpaid, None, now));
        assert!(!grants_access(Canceled, None, now));
    }

    #[test]
    fn canceled_to_active_via_admin_reprovision_clears_grace() {
        // After `subscription.deleted` lands, admin re-provisions: a fresh
        // `subscription.created` webhook with status=active should drive the
        // local row back to Active without retaining any stale grace window.
        let now = t();
        let out = apply_status_transition(StateInput {
            current_status: Canceled,
            current_grace_ends_at: None,
            new_status: Active,
            event_time: now,
        });
        assert_eq!(out.status, Active);
        assert!(out.grace_ends_at.is_none());
        assert!(!out.send_admin_email);
    }

    #[test]
    fn effective_status_collapses_past_due_after_grace() {
        let now = t();
        assert_eq!(
            effective_status(PastDue, Some(now - Duration::days(1)), now),
            Unpaid
        );
        assert_eq!(
            effective_status(PastDue, Some(now + Duration::days(1)), now),
            PastDue
        );
        assert_eq!(effective_status(Active, None, now), Active);
    }
}
