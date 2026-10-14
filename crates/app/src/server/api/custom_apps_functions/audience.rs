//! Who can reach a custom app: the one audience rule `ctx.org.people()`,
//! `ctx.org.assignments()` and `ctx.notify` share, so the three cannot drift.
//!
//! Outside the `custom-app-functions` feature on purpose: the notification
//! service resolves recipients with it, and compiles in every configuration.

use uuid::Uuid;

use sea_orm::DatabaseConnection;

/// Who can reach this app, and the membership rows that said so.
pub struct Audience {
    pub ids: std::collections::HashSet<Uuid>,
    /// The org's members — every one is in `ids`. Returned so a caller that
    /// needs their roles does not read the table a second time.
    pub members: Vec<entity::org_members::Model>,
}

/// The people who can reach this app: org members, plus frontline workers
/// holding a grant on it — the one audience rule `people()` and
/// `assignments()` share, factored so it cannot drift between them.
pub async fn app_audience(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<Audience, String> {
    use entity::{app_members, org_frontline_members, org_members};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let members = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .all(db)
        .await
        .map_err(|e| format!("ctx.org: {e}"))?;
    let mut audience: std::collections::HashSet<Uuid> = members.iter().map(|m| m.user_id).collect();
    let workers: std::collections::HashSet<Uuid> = org_frontline_members::Entity::find()
        .filter(org_frontline_members::Column::OrgId.eq(org_id))
        .filter(org_frontline_members::Column::Status.eq(org_frontline_members::STATUS_ACTIVE))
        .all(db)
        .await
        .map_err(|e| format!("ctx.org: {e}"))?
        .into_iter()
        .map(|w| w.user_id)
        .collect();
    if !workers.is_empty() {
        let granted = app_members::Entity::find()
            .filter(app_members::Column::AppId.eq(app_id))
            .all(db)
            .await
            .map_err(|e| format!("ctx.org: {e}"))?;
        audience.extend(
            granted
                .into_iter()
                .map(|g| g.user_id)
                .filter(|u| workers.contains(u)),
        );
    }
    Ok(Audience {
        ids: audience,
        members,
    })
}
