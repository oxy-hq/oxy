use super::{InviteStatus, Model};
use crate::org_members::OrgRole;
use chrono::Duration;
use sea_orm::prelude::{DateTimeWithTimeZone, Uuid};

/// An invitation created at `now`, expiring `expires_in` from then.
fn invite(now: DateTimeWithTimeZone, status: InviteStatus, expires_in: Duration) -> Model {
    Model {
        id: Uuid::nil(),
        org_id: Uuid::nil(),
        email: "invitee@example.com".to_string(),
        role: OrgRole::Member,
        invited_by: Uuid::nil(),
        token: "test-token".to_string(),
        status,
        expires_at: now + expires_in,
        created_at: now,
    }
}

fn fixed_now() -> DateTimeWithTimeZone {
    chrono::DateTime::parse_from_rfc3339("2026-07-24T12:00:00+00:00").unwrap()
}

#[test]
fn as_str_returns_correct_values() {
    assert_eq!(InviteStatus::Pending.as_str(), "pending");
    assert_eq!(InviteStatus::Accepted.as_str(), "accepted");
    assert_eq!(InviteStatus::Expired.as_str(), "expired");
}

#[test]
fn from_str_parses_valid_statuses() {
    assert_eq!(
        InviteStatus::from_str("pending").unwrap(),
        InviteStatus::Pending
    );
    assert_eq!(
        InviteStatus::from_str("accepted").unwrap(),
        InviteStatus::Accepted
    );
    assert_eq!(
        InviteStatus::from_str("expired").unwrap(),
        InviteStatus::Expired
    );
}

#[test]
fn from_str_rejects_invalid_status() {
    assert!(InviteStatus::from_str("revoked").is_err());
    assert!(InviteStatus::from_str("").is_err());
    assert!(InviteStatus::from_str("Pending").is_err()); // case-sensitive
}

#[test]
fn roundtrip_as_str_from_str() {
    for status in [
        InviteStatus::Pending,
        InviteStatus::Accepted,
        InviteStatus::Expired,
    ] {
        let s = status.as_str();
        let parsed = InviteStatus::from_str(s).unwrap();
        assert_eq!(parsed, status);
    }
}

// ---- expiry is derived from expires_at, never from status ----

#[test]
fn unexpired_pending_invite_is_live() {
    let now = fixed_now();
    let inv = invite(now, InviteStatus::Pending, Duration::days(7));
    assert!(!inv.is_expired(now));
    assert!(inv.is_live(now));
}

/// The lockout row: lapsed, but still carrying `status='pending'` because
/// nothing ever transitions it. It must read as expired everywhere.
#[test]
fn lapsed_invite_still_marked_pending_is_not_live() {
    let now = fixed_now();
    let inv = invite(now, InviteStatus::Pending, Duration::days(-1));
    assert_eq!(inv.status, InviteStatus::Pending);
    assert!(inv.is_expired(now));
    assert!(!inv.is_live(now));
}

#[test]
fn accepted_invite_is_not_live_even_when_unexpired() {
    let now = fixed_now();
    let inv = invite(now, InviteStatus::Accepted, Duration::days(7));
    assert!(!inv.is_expired(now));
    assert!(!inv.is_live(now));
}

/// `live_pending` uses `expires_at > now` and `expired_pending` uses `<=`, so
/// the instant of expiry must belong to exactly one side. A row sitting on the
/// boundary that matched neither would be invisible to the list AND immune to
/// being superseded — the original bug, reintroduced in a one-microsecond window.
#[test]
fn expiry_boundary_counts_as_expired_not_live() {
    let now = fixed_now();
    let inv = invite(now, InviteStatus::Pending, Duration::zero());
    assert_eq!(inv.expires_at, now);
    assert!(inv.is_expired(now));
    assert!(!inv.is_live(now));
}
