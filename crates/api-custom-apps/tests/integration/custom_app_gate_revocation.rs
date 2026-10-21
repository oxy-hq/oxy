//! The data-plane gate remembers the workspace row and the ring's term, and
//! NOT the shipped verdict: a membership revoked is refused by the very next
//! data call on the same replica, with no invalidation hook in between.
//!
//! That is the property the memoisation in `custom_apps_gates` was designed
//! around — the cached term can only subtract — and this is the test that
//! holds it against a real row: the org-member writers that call no
//! invalidation hook (member removal, role change, org delete) are exactly
//! the path this exercises. The second case moves the workspace to another
//! org: the row is read fresh too, so the old org's member is refused with
//! every cache warm — on this replica and on every other, since no replica
//! holds the row.

use axum::http::{HeaderMap, StatusCode};
use entity::{org_members, organizations, workspaces};
use oxy_api_custom_apps::custom_apps_gates::check_custom_app_gates;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::custom_app_procedure_run_fixture::fixture;

#[tokio::test]
async fn a_revoked_membership_is_refused_by_the_next_data_call_on_the_same_replica() {
    let f = fixture().await;

    // Warm every cache the gate keeps — the first call loads the row and
    // computes the term, the second answers from them. Both admit the guest
    // Owner.
    for _ in 0..2 {
        check_custom_app_gates(&HeaderMap::new(), f.workspace_id)
            .await
            .unwrap_or_else(|_| panic!("the guest owner passes the gate"));
    }

    // Revoke the membership behind the gate's back: the row goes, and no
    // `invalidate_access_cache` is called — the shape of a member-removal
    // writer that knows nothing about the custom-app caches.
    org_members::Entity::delete_many()
        .filter(org_members::Column::UserId.eq(f.t.guest_id))
        .filter(org_members::Column::OrgId.eq(f.t.org_id))
        .exec(&f.t.db)
        .await
        .expect("revoke the guest's membership");

    let refused = check_custom_app_gates(&HeaderMap::new(), f.workspace_id)
        .await
        .err()
        .expect("the next call after the revocation is refused");
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "the shipped gate runs fresh: a cached term cannot keep a revoked member in"
    );
}

#[tokio::test]
async fn a_workspace_moved_to_another_org_refuses_the_old_orgs_member_at_once() {
    let f = fixture().await;
    for _ in 0..2 {
        check_custom_app_gates(&HeaderMap::new(), f.workspace_id)
            .await
            .unwrap_or_else(|_| panic!("the guest owner passes the gate"));
    }

    // Move the workspace to an org the guest is no member of, behind the
    // gate's back: the shape of the admin transfer, with no hook in between.
    let other_org = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(other_org),
        name: ActiveValue::Set("elsewhere".into()),
        slug: ActiveValue::Set(format!(
            "elsewhere-{}",
            &other_org.simple().to_string()[..8]
        )),
        ..Default::default()
    }
    .insert(&f.t.db)
    .await
    .expect("the other org");
    workspaces::Entity::update_many()
        .col_expr(
            workspaces::Column::OrgId,
            sea_orm::sea_query::Expr::value(Some(other_org)),
        )
        .filter(workspaces::Column::Id.eq(f.workspace_id))
        .exec(&f.t.db)
        .await
        .expect("move the workspace");

    let refused = check_custom_app_gates(&HeaderMap::new(), f.workspace_id)
        .await
        .err()
        .expect("the next call judges the guest against the new org");
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "the row is read fresh: a warm cache cannot keep the old org's member in"
    );
}
