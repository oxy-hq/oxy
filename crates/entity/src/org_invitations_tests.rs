use super::InviteStatus;

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
