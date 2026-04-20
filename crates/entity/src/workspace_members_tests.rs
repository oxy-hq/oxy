use super::WorkspaceRole;
use std::str::FromStr;

#[test]
fn as_str_returns_correct_values() {
    assert_eq!(WorkspaceRole::Owner.as_str(), "owner");
    assert_eq!(WorkspaceRole::Admin.as_str(), "admin");
    assert_eq!(WorkspaceRole::Member.as_str(), "member");
    assert_eq!(WorkspaceRole::Viewer.as_str(), "viewer");
}

#[test]
fn from_str_parses_valid_roles() {
    assert_eq!(
        WorkspaceRole::from_str("owner").unwrap(),
        WorkspaceRole::Owner
    );
    assert_eq!(
        WorkspaceRole::from_str("admin").unwrap(),
        WorkspaceRole::Admin
    );
    assert_eq!(
        WorkspaceRole::from_str("member").unwrap(),
        WorkspaceRole::Member
    );
    assert_eq!(
        WorkspaceRole::from_str("viewer").unwrap(),
        WorkspaceRole::Viewer
    );
}

#[test]
fn from_str_rejects_invalid_role() {
    assert!(WorkspaceRole::from_str("superadmin").is_err());
    assert!(WorkspaceRole::from_str("").is_err());
    assert!(WorkspaceRole::from_str("Viewer").is_err()); // case-sensitive
}

#[test]
fn roundtrip_as_str_from_str() {
    for role in [
        WorkspaceRole::Owner,
        WorkspaceRole::Admin,
        WorkspaceRole::Member,
        WorkspaceRole::Viewer,
    ] {
        let s = role.as_str();
        let parsed = WorkspaceRole::from_str(s).unwrap();
        assert_eq!(parsed, role);
    }
}

#[test]
fn level_returns_correct_values() {
    assert_eq!(WorkspaceRole::Owner.level(), 3);
    assert_eq!(WorkspaceRole::Admin.level(), 2);
    assert_eq!(WorkspaceRole::Member.level(), 1);
    assert_eq!(WorkspaceRole::Viewer.level(), 0);
}

#[test]
fn ordering_respects_level() {
    assert!(WorkspaceRole::Owner > WorkspaceRole::Admin);
    assert!(WorkspaceRole::Admin > WorkspaceRole::Member);
    assert!(WorkspaceRole::Member > WorkspaceRole::Viewer);
    assert!(WorkspaceRole::Viewer < WorkspaceRole::Member);
    assert!(WorkspaceRole::Member < WorkspaceRole::Admin);
    assert!(WorkspaceRole::Admin < WorkspaceRole::Owner);
}

#[test]
fn ordering_supports_comparison_operators() {
    assert!(WorkspaceRole::Owner >= WorkspaceRole::Owner);
    assert!(WorkspaceRole::Owner >= WorkspaceRole::Admin);
    assert!(!(WorkspaceRole::Viewer >= WorkspaceRole::Member));
    assert!(WorkspaceRole::Member <= WorkspaceRole::Admin);
    assert!(WorkspaceRole::Member <= WorkspaceRole::Member);
    assert!(!(WorkspaceRole::Owner <= WorkspaceRole::Admin));
}
