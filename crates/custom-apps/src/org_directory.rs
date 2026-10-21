//! The org directory `ctx.org.people()`, `ctx.org.places()` and
//! `ctx.org.assignments()` read: the people an app may name, the org's places and
//! the assignments held by somebody in the app's audience.
//!
//! Free functions on a database, an org and an app — not on the function host,
//! which needs a project context, connectors and a workspace to construct — so the
//! scoping rule, the part with a decision in it, can be driven against a real
//! database without standing up an isolate (`oxy-app`'s operating-graph and
//! frontline tests do).

use oxy_app_core::audience::{Audience, app_audience};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

/// `ctx.org.places()`'s query. The org's whole registry, name-sorted, each
/// place with its hierarchy and what every integration calls it.
pub async fn org_places_directory(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<serde_json::Value, String> {
    let rows = oxy_app_core::operating_graph::readers::location_rows(db, org_id)
        .await
        .map_err(|e| format!("ctx.org.places: {e}"))?;
    Ok(serde_json::json!({ "total": rows.len(), "places": rows }))
}

/// `ctx.org.assignments()`'s query: every assignment held by somebody in the
/// app's audience, with the names a screen shows.
pub async fn org_assignments_directory(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<serde_json::Value, String> {
    use oxy_app_core::operating_graph::dto::AssignmentsQuery;
    use oxy_app_core::operating_graph::readers as assignments;
    let audience = app_audience(db, org_id, app_id).await?.ids;
    let rows: Vec<_> = assignments::rows(db, org_id, &AssignmentsQuery::default())
        .await
        .map_err(|e| format!("ctx.org.assignments: {e}"))?
        .into_iter()
        .filter(|a| audience.contains(&a.user_id))
        .map(|mut a| {
            // The supervisor is a person too. One the app may not name — an
            // area manager holding no grant on it — is withheld from the
            // row, not the row from the app: the assignment is still real.
            if a.supervisor_id.is_some_and(|s| !audience.contains(&s)) {
                a.supervisor_id = None;
                a.supervisor_name = None;
            }
            a
        })
        .collect();
    Ok(serde_json::json!({ "total": rows.len(), "assignments": rows }))
}

pub async fn org_directory(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<serde_json::Value, String> {
    use entity::{org_frontline_members, users};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    // WHO is in the directory is `app_audience`'s decision — the one rule
    // `people()` and `assignments()` share. This function only decorates:
    // a name, an org role for a member, `kind` for both.
    let Audience {
        ids: audience,
        mut members,
    } = app_audience(db, org_id, app_id).await?;
    members.sort_by_key(|m| m.user_id);

    // The members' names, one batch; the membership rows came with the
    // audience, so `org_members` is read once.
    let member_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    let names: std::collections::HashMap<Uuid, String> = users::Entity::find()
        .filter(users::Column::Id.is_in(member_ids))
        .all(db)
        .await
        .map_err(|e| format!("ctx.org.people: {e}"))?
        .into_iter()
        .map(|u| (u.id, u.name))
        .collect();
    let mut people: Vec<serde_json::Value> = members
        .into_iter()
        .filter_map(|m| {
            // A membership whose user row is missing is data drift, not a
            // person. Dropped rather than emitted with a null name, because
            // a directory entry nobody can be is worse than a shorter list.
            let name = names.get(&m.user_id)?;
            Some(serde_json::json!({
                "id": m.user_id.to_string(),
                "name": name,
                "role": m.role.as_str(),
                "kind": "member",
            }))
        })
        .collect();

    // Frontline workers: in the audience only with a grant on THIS app, which
    // is the reason two apps in one org see different frontline slices — a
    // grant is per-app — and the reason this is not a widening: every name
    // here belongs to somebody who can already use the app.
    let workers = org_frontline_members::Entity::find()
        .filter(org_frontline_members::Column::OrgId.eq(org_id))
        .find_also_related(users::Entity)
        .order_by_asc(org_frontline_members::Column::UserId)
        .all(db)
        .await
        .map_err(|e| format!("ctx.org.people: {e}"))?;
    people.extend(
        workers
            .into_iter()
            .filter(|(w, _)| audience.contains(&w.user_id))
            .filter_map(|(_, u)| {
                let u = u?;
                Some(serde_json::json!({
                    "id": u.id.to_string(),
                    "name": u.name,
                    // `role` is the org-membership vocabulary and a worker has
                    // none, so it is null rather than an invented "worker" that
                    // would sort beside owner/admin/member as if it were one.
                    // `kind` is where the distinction lives.
                    "role": serde_json::Value::Null,
                    "kind": "frontline",
                }))
            }),
    );

    Ok(serde_json::json!({ "people": people, "total": people.len() }))
}
