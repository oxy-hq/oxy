use super::OrgRole;
use std::str::FromStr;

#[test]
fn as_str_returns_correct_values() {
    assert_eq!(OrgRole::Owner.as_str(), "owner");
    assert_eq!(OrgRole::Admin.as_str(), "admin");
    assert_eq!(OrgRole::Member.as_str(), "member");
}

#[test]
fn from_str_parses_valid_roles() {
    assert_eq!(OrgRole::from_str("owner").unwrap(), OrgRole::Owner);
    assert_eq!(OrgRole::from_str("admin").unwrap(), OrgRole::Admin);
    assert_eq!(OrgRole::from_str("member").unwrap(), OrgRole::Member);
}

#[test]
fn from_str_rejects_invalid_role() {
    assert!(OrgRole::from_str("superadmin").is_err());
    assert!(OrgRole::from_str("").is_err());
    assert!(OrgRole::from_str("Owner").is_err()); // case-sensitive
}

#[test]
fn roundtrip_as_str_from_str() {
    for role in [OrgRole::Owner, OrgRole::Admin, OrgRole::Member] {
        let s = role.as_str();
        let parsed = OrgRole::from_str(s).unwrap();
        assert_eq!(parsed, role);
    }
}

#[test]
fn level_returns_correct_values() {
    assert_eq!(OrgRole::Owner.level(), 2);
    assert_eq!(OrgRole::Admin.level(), 1);
    assert_eq!(OrgRole::Member.level(), 0);
}

#[test]
fn ordering_respects_level() {
    assert!(OrgRole::Owner > OrgRole::Admin);
    assert!(OrgRole::Admin > OrgRole::Member);
    assert!(OrgRole::Member < OrgRole::Admin);
    assert!(OrgRole::Admin < OrgRole::Owner);
}

#[test]
fn ordering_supports_comparison_operators() {
    assert!(OrgRole::Owner >= OrgRole::Owner);
    assert!(OrgRole::Owner >= OrgRole::Admin);
    assert!(!(OrgRole::Member >= OrgRole::Admin));
    assert!(OrgRole::Member <= OrgRole::Admin);
    assert!(OrgRole::Member <= OrgRole::Member);
    assert!(!(OrgRole::Owner <= OrgRole::Admin));
}
