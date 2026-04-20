use super::*;
use entity::org_members::OrgRole;
use uuid::Uuid;

// ---- is_owner_or_admin ----

#[test]
fn owner_is_owner_or_admin() {
    assert!(is_owner_or_admin(&OrgRole::Owner));
}

#[test]
fn admin_is_owner_or_admin() {
    assert!(is_owner_or_admin(&OrgRole::Admin));
}

#[test]
fn member_is_not_owner_or_admin() {
    assert!(!is_owner_or_admin(&OrgRole::Member));
}

// ---- slugify_name ----

#[test]
fn slugify_basic_name() {
    assert_eq!(slugify_name("My Organization"), "my-organization");
}

#[test]
fn slugify_preserves_lowercase() {
    assert_eq!(slugify_name("already-slug"), "already-slug");
}

#[test]
fn slugify_strips_special_chars() {
    assert_eq!(slugify_name("Org @#$ Name!"), "org-name");
}

#[test]
fn slugify_trims_whitespace() {
    assert_eq!(slugify_name("  spaced  out  "), "spaced-out");
}

#[test]
fn slugify_unicode_handling() {
    let result = slugify_name("Ünïcödé Örg");
    assert!(!result.is_empty());
}

// ---- org_response ----

#[test]
fn org_response_serializes_correctly() {
    let now = chrono::Utc::now().fixed_offset();
    let org = entity::organizations::Model {
        id: Uuid::new_v4(),
        name: "Test Org".to_string(),
        slug: "test-org".to_string(),
        created_at: now,
        updated_at: now,
    };

    let resp = org_response(&org, &OrgRole::Admin);

    assert_eq!(resp.id, org.id);
    assert_eq!(resp.name, "Test Org");
    assert_eq!(resp.slug, "test-org");
    assert_eq!(resp.role, "admin");
    assert_eq!(resp.created_at, now.to_rfc3339());
}

#[test]
fn org_response_owner_role() {
    let now = chrono::Utc::now().fixed_offset();
    let org = entity::organizations::Model {
        id: Uuid::new_v4(),
        name: "Org".to_string(),
        slug: "org".to_string(),
        created_at: now,
        updated_at: now,
    };

    let resp = org_response(&org, &OrgRole::Owner);
    assert_eq!(resp.role, "owner");
}

#[test]
fn org_response_member_role() {
    let now = chrono::Utc::now().fixed_offset();
    let org = entity::organizations::Model {
        id: Uuid::new_v4(),
        name: "Org".to_string(),
        slug: "org".to_string(),
        created_at: now,
        updated_at: now,
    };

    let resp = org_response(&org, &OrgRole::Member);
    assert_eq!(resp.role, "member");
}
