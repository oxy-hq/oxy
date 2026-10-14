//! Who a `ctx.notify` call reaches: the app names people, roles or teams; the
//! platform turns that into user ids.
//!
//! **Only the app's audience can be told** — the same rule `ctx.org.people()`
//! answers with ([`app_audience`]): the org's members plus frontline workers
//! holding a grant on this app. A target outside it is *skipped and reported*,
//! not an error, because one stale id in a fan-out to a store's roster should
//! not stop the store manager hearing about it. The app never sees an address:
//! it names somebody and the platform knows how to reach them.

use std::collections::HashSet;

use entity::{org_team_members, org_teams};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::Value;
use uuid::Uuid;

use super::input::Target;
use crate::server::api::custom_apps_functions::audience::app_audience;
use crate::server::api::operating_graph::{assignments, dto::AssignmentsQuery};

/// Most people one call may reach. A whole org's roster fits; a loop that
/// names the same team for every row is caught here rather than in an inbox.
pub const MAX_RECIPIENTS: usize = 500;

/// A target that reached nobody, and why — returned to the app as it named it.
#[derive(Debug, Clone, PartialEq)]
pub struct Skipped {
    pub target: Value,
    pub reason: &'static str,
}

/// Distinct user ids in the order their targets named them, and the targets
/// that named nobody.
pub async fn resolve(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
    targets: &[(Target, Value)],
) -> Result<(Vec<Uuid>, Vec<Skipped>), String> {
    let audience = app_audience(db, org_id, app_id).await?.ids;
    let mut seen = HashSet::new();
    let mut users = Vec::new();
    let mut skipped = Vec::new();
    for (target, raw) in targets {
        let (found, empty_reason) = match target {
            Target::User(id) => (vec![*id], "not_in_audience"),
            Target::Role { role, location } => (
                holders(db, org_id, role, *location).await?,
                "no_one_holds_it",
            ),
            Target::Team(team) => (
                team_members(db, org_id, *team).await?,
                "empty_or_unknown_team",
            ),
        };
        let reachable: Vec<Uuid> = found.into_iter().filter(|u| audience.contains(u)).collect();
        if reachable.is_empty() {
            skipped.push(Skipped {
                target: raw.clone(),
                reason: empty_reason,
            });
        }
        for u in reachable {
            if seen.insert(u) {
                users.push(u);
            }
        }
    }
    if users.len() > MAX_RECIPIENTS {
        return Err(format!(
            "TooManyRecipients: this notification reaches {} people; one call may reach \
             {MAX_RECIPIENTS}",
            users.len()
        ));
    }
    Ok((users, skipped))
}

/// Holders of `role` — matched by id, or by name ignoring case — at
/// `location` when one is given.
async fn holders(
    db: &DatabaseConnection,
    org_id: Uuid,
    role: &str,
    location: Option<Uuid>,
) -> Result<Vec<Uuid>, String> {
    let by_id = Uuid::parse_str(role).ok();
    let query = AssignmentsQuery {
        user_id: None,
        location_id: location,
    };
    Ok(assignments::rows(db, org_id, &query)
        .await
        .map_err(|e| format!("ctx.notify: {e}"))?
        .into_iter()
        .filter(|a| match by_id {
            Some(id) => a.role_id == id,
            None => a.role_name.eq_ignore_ascii_case(role),
        })
        .map(|a| a.user_id)
        .collect())
}

/// Members of `team`, when it is one of this org's teams.
async fn team_members(
    db: &DatabaseConnection,
    org_id: Uuid,
    team: Uuid,
) -> Result<Vec<Uuid>, String> {
    let in_org = org_teams::Entity::find_by_id(team)
        .filter(org_teams::Column::OrgId.eq(org_id))
        .one(db)
        .await
        .map_err(|e| format!("ctx.notify: {e}"))?
        .is_some();
    if !in_org {
        return Ok(Vec::new());
    }
    Ok(org_team_members::Entity::find()
        .filter(org_team_members::Column::TeamId.eq(team))
        .all(db)
        .await
        .map_err(|e| format!("ctx.notify: {e}"))?
        .into_iter()
        .map(|m| m.user_id)
        .collect())
}
