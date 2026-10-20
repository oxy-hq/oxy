//! The operating graph's reads that other contexts ask: the org's assignments
//! with every name a screen shows, and its locations with their external ids.
//! The HTTP handlers stay in `oxy-app`; these take a connection and nothing else.

use entity::{
    location_external_ids as ext, locations, org_members, org_role_members, org_roles, users,
};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use super::dto::{AssignmentRow, AssignmentsQuery, LocationRow, WorkerAssignment};

pub fn to_row(l: locations::Model, external_ids: BTreeMap<String, String>) -> LocationRow {
    LocationRow {
        id: l.id,
        org_id: l.org_id,
        name: l.name,
        kind: l.kind,
        parent_id: l.parent_id,
        status: l.status,
        timezone: l.timezone,
        external_id: l.external_id,
        external_ids,
        created_at: l.created_at.to_rfc3339(),
        updated_at: l.updated_at.to_rfc3339(),
    }
}

/// Every location of the org with its external ids, name-sorted. Two bounded
/// reads: an operator has tens to hundreds of places, not millions.
pub async fn location_rows(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<Vec<LocationRow>, DbErr> {
    let rows = locations::Entity::find()
        .filter(locations::Column::OrgId.eq(org_id))
        .order_by_asc(locations::Column::Name)
        .all(db)
        .await?;
    let ids = ext::Entity::find()
        .filter(ext::Column::OrgId.eq(org_id))
        .all(db)
        .await?;
    let mut by_location: HashMap<Uuid, BTreeMap<String, String>> = HashMap::new();
    for e in ids {
        by_location
            .entry(e.location_id)
            .or_default()
            .insert(e.system, e.external_id);
    }
    Ok(rows
        .into_iter()
        .map(|l| {
            let ids = by_location.remove(&l.id).unwrap_or_default();
            to_row(l, ids)
        })
        .collect())
}

/// The org's assignments with every name a screen shows, optionally narrowed
/// to one person or one place. Five bounded reads, joined in memory.
pub async fn rows(
    db: &DatabaseConnection,
    org_id: Uuid,
    filter: &AssignmentsQuery,
) -> Result<Vec<AssignmentRow>, DbErr> {
    let mut q = org_role_members::Entity::find().filter(org_role_members::Column::OrgId.eq(org_id));
    if let Some(u) = filter.user_id {
        q = q.filter(org_role_members::Column::UserId.eq(u));
    }
    if let Some(l) = filter.location_id {
        q = q.filter(org_role_members::Column::LocationId.eq(l));
    }
    let held = q
        .order_by_asc(org_role_members::Column::CreatedAt)
        .all(db)
        .await?;
    if held.is_empty() {
        return Ok(Vec::new());
    }
    let roles: HashMap<Uuid, org_roles::Model> = org_roles::Entity::find()
        .filter(org_roles::Column::OrgId.eq(org_id))
        .all(db)
        .await?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let places: HashMap<Uuid, String> = locations::Entity::find()
        .filter(locations::Column::OrgId.eq(org_id))
        .all(db)
        .await?
        .into_iter()
        .map(|l| (l.id, l.name))
        .collect();
    let people: Vec<Uuid> = held
        .iter()
        .flat_map(|h| [Some(h.user_id), h.supervisor_id])
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let names: HashMap<Uuid, String> = users::Entity::find()
        .filter(users::Column::Id.is_in(people.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.name))
        .collect();
    let members: HashSet<Uuid> = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.is_in(people))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.user_id)
        .collect();

    let mut out: Vec<AssignmentRow> = held
        .into_iter()
        .map(|h| {
            let role = roles.get(&h.role_id);
            AssignmentRow {
                id: h.id,
                user_id: h.user_id,
                user_name: names.get(&h.user_id).cloned().unwrap_or_default(),
                user_kind: if members.contains(&h.user_id) {
                    "member"
                } else {
                    "frontline"
                },
                role_id: h.role_id,
                role_name: role.map(|r| r.name.clone()).unwrap_or_default(),
                role_scope: role.map(|r| r.scope.clone()).unwrap_or_default(),
                location_id: h.location_id,
                location_name: h.location_id.and_then(|l| places.get(&l).cloned()),
                supervisor_id: h.supervisor_id,
                supervisor_name: h.supervisor_id.and_then(|s| names.get(&s).cloned()),
                created_at: h.created_at.to_rfc3339(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (
            a.location_name.as_deref().unwrap_or(""),
            a.user_name.to_lowercase(),
        )
            .cmp(&(
                b.location_name.as_deref().unwrap_or(""),
                b.user_name.to_lowercase(),
            ))
    });
    Ok(out)
}

/// The worker list's slice: every assignment in the org, grouped by person.
pub async fn by_user(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<HashMap<Uuid, Vec<WorkerAssignment>>, DbErr> {
    let mut grouped: HashMap<Uuid, Vec<WorkerAssignment>> = HashMap::new();
    for row in rows(db, org_id, &AssignmentsQuery::default()).await? {
        grouped
            .entry(row.user_id)
            .or_default()
            .push(WorkerAssignment::from(&row));
    }
    Ok(grouped)
}
